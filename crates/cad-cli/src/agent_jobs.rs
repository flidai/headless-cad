//! Local immutable candidate jobs. This is a bounded worker adapter, not an OS sandbox.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub const MAX_JOBS: usize = 16;
pub const MAX_JOB_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_STDOUT_BYTES: usize = 768 * 1024;
pub const MAX_STDERR_BYTES: usize = 128 * 1024;
pub const MAX_READ_BYTES: usize = 64 * 1024;
pub const MAX_JOB_REPORT_BYTES: usize = 900 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);
type Result<T> = std::result::Result<T, String>;
struct Job {
    directory: tempfile::TempDir,
    report: Value,
    artifacts: BTreeMap<String, (u64, String)>,
}
pub struct AgentJobs {
    executable: PathBuf,
    jobs: BTreeMap<String, Job>,
    sequence: u64,
}
impl AgentJobs {
    pub fn new() -> Result<Self> {
        Ok(Self {
            executable: std::env::current_exe().map_err(|e| e.to_string())?,
            jobs: BTreeMap::new(),
            sequence: 0,
        })
    }
    pub fn capabilities(
        &self,
        operation: Option<&str>,
        object_type: Option<&str>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Value> {
        if operation.is_some() && object_type.is_some() {
            return Err("choose operation or object type".into());
        }
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut args = vec!["capabilities".to_owned()];
        for (flag, value) in [("--operation", operation), ("--type", object_type)] {
            if let Some(v) = value {
                if v.len() > 128 {
                    return Err("capability identifier too long".into());
                }
                args.extend([flag.into(), v.into()]);
            }
        }
        run(&self.executable, directory.path(), &args, TIMEOUT, &cancel)
    }
    /// Bundle must come from the confined source store; never accepts a filesystem path.
    pub fn evaluate(
        &mut self,
        snapshot_id: &str,
        bundle_json: &str,
        mode: &str,
        outputs: &[String],
        params: &BTreeMap<String, Value>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Value> {
        if snapshot_id.is_empty() || snapshot_id.len() > 128 {
            return Err("invalid snapshot identifier".into());
        }
        if self.jobs.len() >= MAX_JOBS {
            return Err("retained job limit reached; restart session to release candidates".into());
        }
        if !matches!(mode, "compile" | "inspect" | "build") {
            return Err("unknown evaluation mode".into());
        }
        if outputs.len() > 64
            || outputs
                .iter()
                .any(|s| s.is_empty() || s.len() > 128 || s.starts_with('-'))
            || (mode == "compile" && !outputs.is_empty())
        {
            return Err("invalid output selection".into());
        }
        if params.len() > 128
            || params.iter().any(|(k, v)| {
                k.is_empty()
                    || k.len() > 128
                    || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    || !v.is_number()
            })
        {
            return Err("parameters must be bounded named numeric values".into());
        }
        if bundle_json.len() > cad_recipe::MAX_RECIPE_SOURCE_BYTES * 8 {
            return Err("source bundle too large".into());
        }
        let bundle: Value = serde_json::from_str(bundle_json).map_err(|e| e.to_string())?;
        if bundle["recipe_bundle_schema_version"] != 1 {
            return Err("captured recipe bundle required".into());
        }
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.path().join("recipe.bundle.json"))
            .map_err(|e| e.to_string())?;
        file.write_all(bundle_json.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut args = vec![mode.to_owned(), "recipe.bundle.json".into()];
        if mode == "build" {
            args.extend(["--out-dir".into(), "generation".into()]);
        }
        for output in outputs {
            args.extend(["--output".into(), output.clone()]);
        }
        for (name, value) in params {
            args.extend(["--param".into(), format!("{name}={value}")]);
        }
        let started = Instant::now();
        let worker = match run(&self.executable, directory.path(), &args, TIMEOUT, &cancel) {
            Ok(v) => v,
            Err(message) => {
                json!({"success":false,"diagnostic":{"code":"worker_limit_or_failure","error":message}})
            }
        };
        let mut artifacts = BTreeMap::new();
        if mode == "build" && worker["success"] == true {
            let verified = run(
                &self.executable,
                directory.path(),
                &["verify".into(), "generation".into()],
                TIMEOUT.saturating_sub(started.elapsed()),
                &cancel,
            )?;
            if verified["success"] != true {
                return Err("candidate verification failed".into());
            }
            let manifest: Value = serde_json::from_slice(
                &fs::read(directory.path().join("generation/manifest.json"))
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            for name in manifest["artifact_files"]
                .as_array()
                .ok_or("missing artifact inventory")?
            {
                let name = name.as_str().ok_or("invalid artifact inventory")?;
                safe_name(name)?;
                let path = directory.path().join("generation").join(name);
                let bytes = fs::read(&path).map_err(|e| e.to_string())?;
                artifacts.insert(
                    name.into(),
                    (
                        bytes.len() as u64,
                        blake3::hash(&bytes).to_hex().to_string(),
                    ),
                );
            }
        }
        let used = self.jobs.values().try_fold(0u64, |n, j| {
            tree_bytes(j.directory.path()).map(|v| n.saturating_add(v))
        })?;
        let final_bytes = tree_bytes(directory.path())?;
        validate_retention(final_bytes, used, None)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("candidate cancelled before retention".into());
        }
        let next_sequence = self
            .sequence
            .checked_add(1)
            .ok_or("job identifier exhausted")?;
        let id = format!("job_{next_sequence:016x}");
        let manifest_hash = if mode == "build" && worker["success"] == true {
            Some(
                blake3::hash(
                    &fs::read(directory.path().join("generation/manifest.json"))
                        .map_err(|e| e.to_string())?,
                )
                .to_hex()
                .to_string(),
            )
        } else {
            None
        };
        let inventory: Vec<_> = artifacts
            .iter()
            .map(|(name, (bytes, hash))| json!({"name":name,"bytes":bytes,"blake3":hash}))
            .collect();
        let report = json!({"job_id":id,"snapshot_id":snapshot_id,"mode":mode,"worker":worker,"artifacts":inventory,"publication":"immutable_candidate_only","manifest_hash":manifest_hash});
        validate_retention(final_bytes, used, Some(&report))?;
        self.sequence = next_sequence;
        self.jobs.insert(
            id,
            Job {
                directory,
                report: report.clone(),
                artifacts,
            },
        );
        Ok(report)
    }
    pub fn verify(
        &self,
        id: &str,
        expected_manifest_digest: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<Value> {
        if expected_manifest_digest.len() != 64
            || !expected_manifest_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err("expected manifest digest must be BLAKE3 hex".into());
        }
        let job = self.jobs.get(id).ok_or("unknown session job")?;
        if job.artifacts.is_empty() {
            return Err("job has no verified generation".into());
        }
        run(
            &self.executable,
            job.directory.path(),
            &[
                "verify".into(),
                "generation".into(),
                "--manifest-hash".into(),
                expected_manifest_digest.into(),
            ],
            TIMEOUT,
            &cancel,
        )
    }
    /// Allows discovery after a response-delivery/cancellation race.
    pub fn list_jobs(&self) -> Value {
        let jobs:Vec<_>=self.jobs.iter().map(|(id,job)| json!({"job_id":id,"snapshot_id":job.report["snapshot_id"],"mode":job.report["mode"],"success":job.report["worker"]["success"],"manifest_hash":job.report["manifest_hash"]})).collect();
        json!({"jobs":jobs,"retention":"session_only"})
    }
    pub fn job(&self, id: &str) -> Result<Value> {
        self.jobs
            .get(id)
            .map(|j| j.report.clone())
            .ok_or_else(|| "unknown session job".into())
    }
    pub fn read_artifact(
        &self,
        id: &str,
        name: &str,
        offset: usize,
        max_bytes: usize,
    ) -> Result<Value> {
        if max_bytes == 0 || max_bytes > MAX_READ_BYTES {
            return Err("invalid artifact read budget".into());
        }
        safe_name(name)?;
        let job = self.jobs.get(id).ok_or("unknown session job")?;
        let (length, hash) = job
            .artifacts
            .get(name)
            .ok_or("artifact is not in verified candidate inventory")?;
        // Verification binds every returned chunk to the immutable candidate inventory.
        let bytes = fs::read(job.directory.path().join("generation").join(name))
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 != *length || blake3::hash(&bytes).to_hex().as_str() != hash {
            return Err("artifact integrity mismatch".into());
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| "artifact is not UTF-8 text")?;
        if offset > text.len() || !text.is_char_boundary(offset) {
            return Err("offset must be a UTF-8 boundary".into());
        }
        let mut end = offset.saturating_add(max_bytes).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset && offset < text.len() {
            return Err("read budget is too small for the next UTF-8 character".into());
        }
        Ok(
            json!({"job_id":id,"name":name,"offset":offset,"next_offset":end,"total_bytes":length,"blake3":hash,"text":&text[offset..end],"eof":end==text.len()}),
        )
    }
}
// Applied regardless of worker status and before consuming any job ID or slot.
fn validate_retention(
    candidate_bytes: u64,
    retained_bytes: u64,
    report: Option<&Value>,
) -> Result<()> {
    if candidate_bytes > MAX_JOB_BYTES {
        return Err("candidate exceeds per-job byte limit; discarded".into());
    }
    if retained_bytes.saturating_add(candidate_bytes) > MAX_RETAINED_BYTES {
        return Err("retained candidate byte limit reached".into());
    }
    if let Some(report) = report {
        if serde_json::to_vec(report).map_err(|e| e.to_string())?.len() > MAX_JOB_REPORT_BYTES {
            return Err("candidate report exceeds readable response limit; discarded".into());
        }
    }
    Ok(())
}
fn safe_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 256
        || name.contains(['/', '\\'])
        || name == "."
        || name == ".."
    {
        Err("invalid artifact name".into())
    } else {
        Ok(())
    }
}
fn tree_bytes(root: &Path) -> Result<u64> {
    tree_bytes_impl(root, false)
}
// Publication renames staging directories while this advisory live poll runs.
// Only disappearing descendants may be skipped; quiescent retention is strict.
fn live_tree_bytes(root: &Path) -> Result<u64> {
    tree_bytes_impl(root, true)
}
fn tree_bytes_impl(root: &Path, live: bool) -> Result<u64> {
    let root_meta = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err("worker root is not a real directory".into());
    }
    let mut total = 0u64;
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(path) = pending.pop() {
        let entries = match fs::read_dir(&path) {
            Ok(v) => v,
            Err(e) if live && path != root && e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        for entry in entries {
            let entry = match entry {
                Ok(v) => v,
                Err(e) if live && e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
            };
            count += 1;
            if count > 10000 {
                return Err("worker file count exceeded".into());
            }
            let meta = match fs::symlink_metadata(entry.path()) {
                Ok(v) => v,
                Err(e) if live && e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
            };
            if meta.file_type().is_symlink() {
                return Err("worker created symlink".into());
            }
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            } else {
                return Err("worker created special file".into());
            }
        }
    }
    Ok(total)
}

fn drain(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = pipe.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            let keep = n.min(limit.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&chunk[..keep]);
            if keep < n {
                overflow.store(true, Ordering::Relaxed);
            }
        }
        Ok(bytes)
    })
}
fn run(
    executable: &Path,
    directory: &Path,
    args: &[String],
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
) -> Result<Value> {
    run_process(executable, directory, args, timeout, cancel, true)
}
fn run_process(
    executable: &Path,
    directory: &Path,
    args: &[String],
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
    native_worker: bool,
) -> Result<Value> {
    let mut command = Command::new(executable);
    if native_worker {
        if !args.first().is_some_and(|a| {
            matches!(
                a.as_str(),
                "compile" | "inspect" | "build" | "capabilities" | "verify"
            )
        }) {
            return Err("invalid fixed worker operation".into());
        }
        match fs::remove_file(directory.join(crate::worker_limits::ADMISSION_FILE)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        command.arg("_agent-worker");
    }
    let mut child = command
        .args(args)
        .current_dir(directory)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = drain(
        child.stdout.take().unwrap(),
        MAX_STDOUT_BYTES,
        overflow.clone(),
    );
    let stderr = drain(
        child.stderr.take().unwrap(),
        MAX_STDERR_BYTES,
        overflow.clone(),
    );
    let start = Instant::now();
    let mut failure = None;
    let status = loop {
        let bytes = live_tree_bytes(directory);
        if cancel.load(Ordering::Relaxed)
            || start.elapsed() > timeout
            || overflow.load(Ordering::Relaxed)
            || bytes.as_ref().map_or(true, |n| *n > MAX_JOB_BYTES)
        {
            failure = Some(
                if cancel.load(Ordering::Relaxed) {
                    "worker_cancelled"
                } else if start.elapsed() > timeout {
                    "worker_timeout"
                } else if overflow.load(Ordering::Relaxed) {
                    "worker_output_limit"
                } else if bytes.is_err() {
                    "worker_tree_invalid"
                } else {
                    "worker_disk_limit"
                }
                .to_owned(),
            );
            let _ = child.kill();
            break child.wait().map_err(|e| e.to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    let out = stdout
        .join()
        .map_err(|_| "stdout reader failed")?
        .map_err(|e| e.to_string())?;
    let err = stderr
        .join()
        .map_err(|_| "stderr reader failed")?
        .map_err(|e| e.to_string())?;
    let admission = if native_worker {
        let path = directory.join(crate::worker_limits::ADMISSION_FILE);
        match fs::File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(16385)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 16384 {
                    return Err("worker admission record too large".into());
                }
                let record = serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| "invalid worker admission record")?;
                crate::worker_limits::validate_record(&record)?;
                Some(record)
            }
            Err(_) => None,
        }
    } else {
        None
    };
    if overflow.load(Ordering::Relaxed) {
        failure = Some("worker_output_limit".into());
    }
    if cancel.load(Ordering::Relaxed) {
        failure = Some("worker_cancelled".into());
    }
    if let Some(code) = failure {
        if native_worker {
            return Ok(
                json!({"success":false,"admission":admission,"result":{"code":code,"error":code}}),
            );
        }
        return Err(code);
    }
    let status = status?;
    if tree_bytes(directory)? > MAX_JOB_BYTES {
        if native_worker {
            return Ok(
                json!({"success":false,"admission":admission,"result":{"code":"worker_disk_limit","error":"worker exceeded disk byte limit"}}),
            );
        }
        return Err("worker disk limit exceeded".into());
    }
    if native_worker && status.success() && admission.is_none() {
        return Err("worker admission record missing".into());
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let code = match signal {
                s if s == rustix::process::Signal::XCPU.as_raw() => "worker_cpu_limit",
                s if s == rustix::process::Signal::XFSZ.as_raw() => "worker_file_limit",
                _ => "worker_terminated",
            };
            return Ok(
                json!({"success":false,"admission":admission,"result":{"code":code,"signal":signal,"error":"worker terminated; other signals do not identify a specific resource failure","stderr_excerpt":String::from_utf8_lossy(&err[..err.len().min(16*1024)]),"stderr_truncated":err.len()>16*1024}}),
            );
        }
    }
    let payload = if status.success() { &out } else { &err };
    let result: Value =
        serde_json::from_slice(payload).map_err(|_| "worker returned invalid JSON")?;
    Ok(json!({"success":status.success(),"result":result,"admission":admission}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn private_workers_enforce_timeout_cancellation_and_output_limits() {
        let dir = tempfile::tempdir().unwrap();
        // A fixed test executable exercises only the internal runner, never a public request.
        let executable = |name: &str| {
            std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|p| p.join(name))
                .find(|p| p.is_file())
                .unwrap()
        };
        let sleep = executable("sleep");
        let yes = executable("yes");
        let cancelled = Arc::new(AtomicBool::new(true));
        assert!(run_process(
            &sleep,
            dir.path(),
            &["5".into()],
            Duration::from_secs(1),
            &cancelled,
            false
        )
        .is_err());
        let started = Instant::now();
        assert!(run_process(
            &sleep,
            dir.path(),
            &["5".into()],
            Duration::from_millis(10),
            &Arc::new(AtomicBool::new(false)),
            false
        )
        .is_err());
        assert!(started.elapsed() >= Duration::from_millis(10));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(run_process(
            &yes,
            dir.path(),
            &[],
            Duration::from_secs(2),
            &Arc::new(AtomicBool::new(false)),
            false
        )
        .is_err());
    }
    #[test]
    fn artifact_reads_are_inventory_bound_and_digest_checked() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("generation")).unwrap();
        let bytes = "aéz".as_bytes();
        fs::write(dir.path().join("generation/test.json"), bytes).unwrap();
        let mut jobs = AgentJobs::new().unwrap();
        jobs.jobs.insert(
            "test".into(),
            Job {
                directory: dir,
                report: json!({"snapshot_id":"source","mode":"build","worker":{"success":true,"result":{"private_large_report":"not in listing"}},"manifest_hash":"digest"}),
                artifacts: BTreeMap::from([(
                    "test.json".into(),
                    (bytes.len() as u64, blake3::hash(bytes).to_hex().to_string()),
                )]),
            },
        );
        assert_eq!(
            jobs.list_jobs()["jobs"][0],
            json!({"job_id":"test","snapshot_id":"source","mode":"build","success":true,"manifest_hash":"digest"})
        );
        assert_eq!(
            jobs.read_artifact("test", "test.json", 0, 2).unwrap()["text"],
            "a"
        );
        assert_eq!(
            jobs.read_artifact("test", "test.json", 1, 2).unwrap()["text"],
            "é"
        );
        assert!(jobs.read_artifact("test", "test.json", 2, 2).is_err());
        assert!(jobs.read_artifact("test", "../test.json", 0, 2).is_err());
        assert!(jobs.read_artifact("test", "missing.json", 0, 2).is_err());
        fs::write(
            jobs.jobs["test"]
                .directory
                .path()
                .join("generation/test.json"),
            b"tampered",
        )
        .unwrap();
        assert!(jobs.read_artifact("test", "test.json", 0, 2).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn worker_inventory_rejects_links_and_observes_large_sparse_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = fs::File::create(dir.path().join("large")).unwrap();
        file.set_len(MAX_JOB_BYTES + 1).unwrap();
        assert!(tree_bytes(dir.path()).unwrap() > MAX_JOB_BYTES);
        fs::remove_file(dir.path().join("large")).unwrap();
        std::os::unix::fs::symlink("/", dir.path().join("escape")).unwrap();
        assert!(tree_bytes(dir.path()).is_err());
    }
    #[test]
    fn live_tree_monitor_tolerates_atomic_generation_publication() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().to_path_buf();
        let finished = Arc::new(AtomicBool::new(false));
        let done = finished.clone();
        let publisher = thread::spawn(move || {
            for i in 0..200 {
                let payload = format!("generation {i}");
                let manifest = json!({"artifact_files":["manifest.json","part.json"]});
                crate::publish(
                    &directory.join("generation"),
                    &[
                        ("manifest.json".into(), manifest.to_string()),
                        ("part.json".into(), payload),
                    ],
                )
                .unwrap();
            }
            done.store(true, Ordering::Release);
        });
        let mut polls = 0;
        while !finished.load(Ordering::Acquire) {
            live_tree_bytes(root.path()).unwrap();
            polls += 1;
        }
        publisher.join().unwrap();
        assert!(polls > 0);
        assert!(tree_bytes(root.path()).unwrap() > 0);
        assert_eq!(
            fs::read_to_string(root.path().join("generation/part.json")).unwrap(),
            "generation 199"
        );
        assert!(live_tree_bytes(&root.path().join("missing_root")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/", root.path().join("escape")).unwrap();
            assert!(live_tree_bytes(root.path()).is_err());
            assert!(tree_bytes(root.path()).is_err());
        }
    }
    #[test]
    fn failed_candidate_retention_obeys_disk_and_report_caps() {
        let failed = json!({"worker":{"success":false}});
        assert!(validate_retention(MAX_JOB_BYTES + 1, 0, Some(&failed)).is_err());
        assert!(validate_retention(1, MAX_RETAINED_BYTES, Some(&failed)).is_err());
        let excessive =
            json!({"worker":{"success":false,"error":"x".repeat(MAX_JOB_REPORT_BYTES)}});
        assert!(validate_retention(1, 0, Some(&excessive)).is_err());
        assert!(validate_retention(MAX_JOB_BYTES, 0, Some(&failed)).is_ok());
    }
    #[test]
    fn cancelled_evaluations_never_consume_retained_slots() {
        let mut jobs = AgentJobs::new().unwrap();
        let bundle = r#"{"recipe_bundle_schema_version":1}"#;
        for _ in 0..3 {
            assert!(jobs
                .evaluate(
                    "snapshot",
                    bundle,
                    "compile",
                    &[],
                    &BTreeMap::new(),
                    Arc::new(AtomicBool::new(true))
                )
                .is_err());
            assert!(jobs.jobs.is_empty());
            assert_eq!(jobs.sequence, 0);
        }
        assert_eq!(
            jobs.capabilities(None, None, Arc::new(AtomicBool::new(true)))
                .unwrap()["result"]["code"],
            "worker_cancelled"
        );
    }
    #[test]
    fn invalid_requests_never_start_workers() {
        let mut jobs = AgentJobs::new().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        assert!(jobs
            .evaluate("s", "{}", "shell", &[], &BTreeMap::new(), cancel.clone())
            .is_err());
        assert!(jobs
            .evaluate(
                "s",
                "{}",
                "build",
                &["--help".into()],
                &BTreeMap::new(),
                cancel.clone()
            )
            .is_err());
        assert!(jobs
            .evaluate("s", "{}", "build", &[], &BTreeMap::new(), cancel)
            .is_err());
        assert!(jobs.jobs.is_empty());
        assert_eq!(jobs.list_jobs()["jobs"], json!([]));
    }
}
