//! Opt-in local durable state. Every on-disk name is fixed or a validated
//! digest/job ID; directory handles are pinned and each descendant is opened
//! with NOFOLLOW. The state root is exclusively locked for its whole lifetime.
use crate::{agent_jobs, source::Snapshot};
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    os::fd::AsRawFd,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, String>;
pub(crate) const SCHEMA: u32 = 2;
const MAX_RECORD: usize = 64 * 1024 * 1024 + 4096;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    schema_version: u32,
    workspace: String,
    workspace_dev: u64,
    workspace_ino: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedSnapshot {
    schema_version: u32,
    snapshot_id: String,
    parent_snapshot_id: Option<String>,
    snapshot: Snapshot,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Intent {
    pub schema_version: u32,
    pub request_id: String,
    pub fingerprint: String,
    pub job_id: String,
    pub snapshot_id: String,
    pub mode: String,
    pub executable_digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    schema_version: u32,
    pub job_id: String,
    request_fingerprint: String,
    snapshot_id: String,
    mode: String,
    pub report: Value,
    pub artifacts: BTreeMap<String, (u64, String)>,
    tree_bytes: u64,
    tree_digest: String,
}
pub(crate) struct AgentStore {
    _root: Dir,
    snapshots: Dir,
    intents: Dir,
    receipts: Dir,
    jobs: Dir,
    executable_digest: String,
}

#[cfg(target_os = "linux")]
fn dir_at(parent: &Dir, name: &str) -> Result<Dir> {
    use rustix::fs::{openat, Mode, OFlags};
    let file = File::from(
        openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    if !file.metadata().map_err(|e| e.to_string())?.is_dir() {
        return Err("durable state component is not a directory".into());
    }
    Ok(Dir::from_std_file(file))
}
#[cfg(target_os = "linux")]
fn file_at(parent: &Dir, name: &str) -> Result<File> {
    use rustix::fs::{openat, Mode, OFlags};
    let file = File::from(
        openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("durable state entry is not a regular file".into());
    }
    Ok(file)
}
#[cfg(target_os = "linux")]
fn make_dir(parent: &Dir, name: &str) -> Result<Dir> {
    use rustix::fs::{mkdirat, Mode};
    match mkdirat(parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
        Ok(()) => rustix::fs::fsync(parent).map_err(|e| e.to_string())?,
        Err(e) if e == rustix::io::Errno::EXIST => {}
        Err(e) => return Err(e.to_string()),
    }
    dir_at(parent, name)
}
#[cfg(target_os = "linux")]
fn open_root(path: &Path) -> Result<Dir> {
    use rustix::fs::{open, Mode, OFlags};
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let root = File::from(
        open(
            "/",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    let mut current = Dir::from_std_file(root);
    let parts = absolute
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(Ok(name.to_owned())),
            Component::RootDir => None,
            _ => Some(Err(
                "state directory path must not contain '.' or '..'".to_owned()
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    if parts.is_empty() {
        return Err("state directory cannot be filesystem root".into());
    }
    for (i, part) in parts.iter().enumerate() {
        let name = part.to_str().ok_or("state path must be UTF-8")?;
        current = if i + 1 == parts.len() {
            make_dir(&current, name)?
        } else {
            dir_at(&current, name)?
        };
    }
    Ok(current)
}
#[cfg(not(target_os = "linux"))]
fn open_root(_: &Path) -> Result<Dir> {
    Err("durable agent state requires Linux".into())
}
fn read_bounded(dir: &Dir, name: &str, limit: usize) -> Result<Vec<u8>> {
    let file = file_at(dir, name)?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit as u64 {
        return Err("durable state record exceeds limit".into());
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("durable state record exceeds limit".into());
    }
    Ok(bytes)
}
fn atomic_record(dir: &Dir, name: &str, bytes: &[u8]) -> Result<()> {
    use rustix::fs::{openat, renameat_with, Mode, OFlags, RenameFlags};
    if bytes.len() > MAX_RECORD {
        return Err("durable state record exceeds limit".into());
    }
    let temp = format!(
        ".tmp-{:08x}-{:016x}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = File::from(
        openat(
            dir,
            &temp,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|e| e.to_string())?,
    );
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    renameat_with(dir, &temp, dir, name, RenameFlags::NOREPLACE).map_err(|e| e.to_string())?;
    rustix::fs::fsync(dir).map_err(|e| e.to_string())?;
    Ok(())
}
fn names(dir: &Dir, allow_stale_records: bool) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut count = 0usize;
    let mut temps = 0usize;
    let mut temp_bytes = 0u64;
    for entry in dir.read_dir(".").map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        count += 1;
        if count > 10000 {
            return Err("durable state entry count exceeded".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "durable state name is not UTF-8")?;
        if allow_stale_records && name.starts_with(".tmp-") {
            temps += 1;
            if temps > 32 {
                return Err("too many interrupted state records".into());
            }
            let file = file_at(dir, &name)?;
            let size = file.metadata().map_err(|e| e.to_string())?.len();
            temp_bytes = temp_bytes
                .checked_add(size)
                .ok_or("interrupted state size overflow")?;
            if size > MAX_RECORD as u64 || temp_bytes > 64 * 1024 * 1024 {
                return Err("interrupted state record quota exceeded".into());
            }
            continue;
        }
        result.push(name);
    }
    result.sort();
    Ok(result)
}
fn is_hex(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn valid_job_id(id: &str) -> bool {
    id.len() == 20
        && id.starts_with("job_")
        && id[4..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn valid_request_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}
pub(crate) fn request_hash(id: &str) -> Result<String> {
    if !valid_request_id(id) {
        return Err("request_id must be 1..128 bytes without control characters".into());
    }
    Ok(blake3::hash(id.as_bytes()).to_hex().to_string())
}
fn snapshot_size(s: &Snapshot) -> usize {
    s.source.len()
        + s.modules.values().map(String::len).sum::<usize>()
        + s.lock_source.as_ref().map_or(0, String::len)
}

impl AgentStore {
    pub(crate) fn open(state_dir: &Path, workspace: &Path, pinned_workspace: &Dir) -> Result<Self> {
        #[cfg(not(target_os = "linux"))]
        return Err("durable agent state requires Linux".into());
        let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
        use cap_std::fs::MetadataExt as _;
        use std::os::unix::fs::MetadataExt as _;
        let pinned_meta = pinned_workspace.metadata(".").map_err(|e| e.to_string())?;
        let canonical_meta = std::fs::metadata(&workspace).map_err(|e| e.to_string())?;
        if pinned_meta.dev() != canonical_meta.dev() || pinned_meta.ino() != canonical_meta.ino() {
            return Err("workspace identity changed during durable admission".into());
        }
        let state_absolute = if state_dir.is_absolute() {
            state_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(state_dir)
        };
        if state_absolute.starts_with(&workspace) || workspace.starts_with(&state_absolute) {
            return Err("state directory and workspace must be separate roots".into());
        }
        let root = open_root(state_dir)?;
        use rustix::fs::{flock, FlockOperation};
        // Lock the pinned directory inode itself. A replaceable lock filename
        // could let two processes lock different inodes under the same path.
        flock(&root, FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| "state directory is already locked".to_owned())?;
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt as _;
            use std::os::unix::fs::MetadataExt;
            let selected = std::fs::symlink_metadata(&state_absolute).map_err(|e| e.to_string())?;
            let pinned = root.metadata(".").map_err(|e| e.to_string())?;
            if !selected.is_dir()
                || selected.dev() != pinned.dev()
                || selected.ino() != pinned.ino()
            {
                return Err("state directory changed during admission".into());
            }
        }
        let workspace_string = workspace
            .to_str()
            .ok_or("workspace path must be UTF-8")?
            .to_owned();
        match read_bounded(&root, "meta.json", 4096) {
            Ok(bytes) => {
                let meta: Meta = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if meta.schema_version != SCHEMA
                    || meta.workspace != workspace_string
                    || meta.workspace_dev != pinned_meta.dev()
                    || meta.workspace_ino != pinned_meta.ino()
                {
                    return Err("durable state schema or workspace identity mismatch".into());
                }
            }
            Err(_) => {
                if !names(&root, true)?.is_empty() {
                    return Err("existing durable state lacks valid metadata".into());
                }
                atomic_record(
                    &root,
                    "meta.json",
                    &serde_json::to_vec(&Meta {
                        schema_version: SCHEMA,
                        workspace: workspace_string,
                        workspace_dev: pinned_meta.dev(),
                        workspace_ino: pinned_meta.ino(),
                    })
                    .map_err(|e| e.to_string())?,
                )?;
            }
        }
        let snapshots = make_dir(&root, "snapshots")?;
        let intents = make_dir(&root, "intents")?;
        let receipts = make_dir(&root, "receipts")?;
        let jobs = make_dir(&root, "jobs")?;
        let executable_digest =
            blake3::hash(&std::fs::read("/proc/self/exe").map_err(|e| e.to_string())?)
                .to_hex()
                .to_string();
        Ok(Self {
            _root: root,
            snapshots,
            intents,
            receipts,
            jobs,
            executable_digest,
        })
    }
    pub(crate) fn executable_digest(&self) -> &str {
        &self.executable_digest
    }
    pub(crate) fn snapshots(&self) -> Result<BTreeMap<String, (Snapshot, Option<String>)>> {
        let mut out = BTreeMap::new();
        let mut bytes_total = 0usize;
        for name in names(&self.snapshots, true)? {
            let id = name
                .strip_suffix(".json")
                .ok_or("invalid snapshot filename")?;
            if !is_hex(id) {
                return Err("invalid snapshot filename".into());
            }
            let bytes = read_bounded(&self.snapshots, &name, MAX_RECORD)?;
            let saved: SavedSnapshot = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let canonical = serde_json::to_vec(&saved.snapshot).map_err(|e| e.to_string())?;
            if saved.schema_version != SCHEMA
                || saved.snapshot_id != id
                || saved.snapshot.recipe_bundle_schema_version != 1
                || blake3::hash(&canonical).to_hex().as_str() != id
            {
                return Err("snapshot identity or schema mismatch".into());
            }
            bytes_total = bytes_total.saturating_add(snapshot_size(&saved.snapshot));
            if out
                .insert(id.to_owned(), (saved.snapshot, saved.parent_snapshot_id))
                .is_some()
            {
                return Err("duplicate snapshot".into());
            }
            if out.len() > 32 || bytes_total > 64 * 1024 * 1024 {
                return Err("durable snapshot quota exceeded".into());
            }
        }
        for (_, parent) in out.values() {
            if parent.as_ref().is_some_and(|id| !out.contains_key(id)) {
                return Err("snapshot parent missing".into());
            }
        }
        Ok(out)
    }
    pub(crate) fn persist_snapshot(
        &self,
        id: &str,
        snapshot: &Snapshot,
        parent: Option<&str>,
    ) -> Result<Option<String>> {
        if !is_hex(id) {
            return Err("invalid snapshot identifier".into());
        }
        let canonical = serde_json::to_vec(snapshot).map_err(|e| e.to_string())?;
        if blake3::hash(&canonical).to_hex().as_str() != id
            || snapshot.recipe_bundle_schema_version != 1
        {
            return Err("snapshot identity mismatch".into());
        }
        let existing = self.snapshots()?;
        if let Some((old, first_parent)) = existing.get(id) {
            if serde_json::to_vec(old).map_err(|e| e.to_string())? != canonical {
                return Err("snapshot collision".into());
            }
            return Ok(first_parent.clone());
        }
        if existing.len() >= 32
            || existing
                .values()
                .map(|(s, _)| snapshot_size(s))
                .sum::<usize>()
                .saturating_add(snapshot_size(snapshot))
                > 64 * 1024 * 1024
        {
            return Err("durable snapshot quota exceeded".into());
        }
        if parent.is_some_and(|p| !existing.contains_key(p)) {
            return Err("unknown parent snapshot".into());
        }
        let saved = SavedSnapshot {
            schema_version: SCHEMA,
            snapshot_id: id.to_owned(),
            parent_snapshot_id: parent.map(str::to_owned),
            snapshot: snapshot.clone(),
        };
        atomic_record(
            &self.snapshots,
            &format!("{id}.json"),
            &serde_json::to_vec(&saved).map_err(|e| e.to_string())?,
        )?;
        Ok(parent.map(str::to_owned))
    }
    pub(crate) fn intents(&self) -> Result<BTreeMap<String, Intent>> {
        let mut out = BTreeMap::new();
        let mut job_ids = std::collections::BTreeSet::new();
        for name in names(&self.intents, true)? {
            let hash = name
                .strip_suffix(".json")
                .ok_or("invalid intent filename")?;
            if !is_hex(hash) {
                return Err("invalid intent filename".into());
            }
            let intent: Intent = serde_json::from_slice(&read_bounded(&self.intents, &name, 4096)?)
                .map_err(|e| e.to_string())?;
            if intent.schema_version != SCHEMA
                || request_hash(&intent.request_id)? != hash
                || !is_hex(&intent.fingerprint)
                || !valid_job_id(&intent.job_id)
                || !is_hex(&intent.snapshot_id)
                || !matches!(intent.mode.as_str(), "compile" | "inspect" | "build")
                || !is_hex(&intent.executable_digest)
                || !job_ids.insert(intent.job_id.clone())
            {
                return Err("durable intent identity mismatch".into());
            }
            out.insert(intent.request_id.clone(), intent);
            if out.len() > agent_jobs::MAX_JOBS {
                return Err("durable job count exceeded".into());
            }
        }
        Ok(out)
    }
    pub(crate) fn persist_intent(&self, intent: &Intent) -> Result<()> {
        if intent.schema_version != SCHEMA
            || !is_hex(&intent.fingerprint)
            || !valid_job_id(&intent.job_id)
            || !is_hex(&intent.snapshot_id)
            || intent.executable_digest != self.executable_digest
        {
            return Err("invalid durable intent".into());
        }
        let hash = request_hash(&intent.request_id)?;
        atomic_record(
            &self.intents,
            &format!("{hash}.json"),
            &serde_json::to_vec(intent).map_err(|e| e.to_string())?,
        )
    }
    pub(crate) fn receipts(
        &self,
        intents: &BTreeMap<String, Intent>,
    ) -> Result<BTreeMap<String, Receipt>> {
        let mut out = BTreeMap::new();
        for name in names(&self.receipts, true)? {
            let hash = name
                .strip_suffix(".json")
                .ok_or("invalid receipt filename")?;
            if !is_hex(hash) {
                return Err("invalid receipt filename".into());
            }
            let receipt: Receipt =
                serde_json::from_slice(&read_bounded(&self.receipts, &name, MAX_RECORD)?)
                    .map_err(|e| e.to_string())?;
            let intent = intents
                .values()
                .find(|i| request_hash(&i.request_id).ok().as_deref() == Some(hash))
                .ok_or("receipt without intent")?;
            if receipt.schema_version != SCHEMA
                || receipt.job_id != intent.job_id
                || receipt.request_fingerprint != intent.fingerprint
                || receipt.snapshot_id != intent.snapshot_id
                || receipt.mode != intent.mode
                || receipt.report["job_id"] != intent.job_id
                || receipt.report["snapshot_id"] != intent.snapshot_id
                || receipt.report["mode"] != intent.mode
                || receipt.report["publication"] != "immutable_candidate_only"
                || !receipt.report["worker"]["success"].is_boolean()
                || !is_hex(&receipt.tree_digest)
            {
                return Err("durable receipt mismatch".into());
            }
            if receipt.artifacts.len() > 10000 {
                return Err("durable artifact count exceeded".into());
            }
            let expected_inventory:Vec<_>=receipt.artifacts.iter().map(|(name,(size,hash))|serde_json::json!({"name":name,"bytes":size,"blake3":hash})).collect();
            if receipt.report["artifacts"] != serde_json::json!(expected_inventory) {
                return Err("durable artifact inventory mismatch".into());
            }
            if intent.mode == "build" && receipt.report["worker"]["success"] == true {
                let manifest = self.read_job_file(
                    &receipt.job_id,
                    &["generation", "manifest.json"],
                    agent_jobs::MAX_JOB_BYTES as usize,
                )?;
                if receipt.report["manifest_hash"] != blake3::hash(&manifest).to_hex().as_str() {
                    return Err("durable manifest digest mismatch".into());
                }
                let parsed: Value = serde_json::from_slice(&manifest).map_err(|e| e.to_string())?;
                let names = parsed["artifact_files"]
                    .as_array()
                    .ok_or("durable manifest inventory missing")?;
                if names.len() != receipt.artifacts.len() {
                    return Err("durable manifest inventory mismatch".into());
                }
                for (name, (length, digest)) in &receipt.artifacts {
                    if name.is_empty()
                        || name.len() > 256
                        || name.contains(['/', '\\'])
                        || name == "."
                        || name == ".."
                        || !is_hex(digest)
                        || *length > agent_jobs::MAX_JOB_BYTES
                        || !names.iter().any(|n| n == name)
                    {
                        return Err("invalid durable artifact inventory".into());
                    }
                    let content = self.read_job_file(
                        &receipt.job_id,
                        &["generation", name],
                        agent_jobs::MAX_JOB_BYTES as usize,
                    )?;
                    if content.len() as u64 != *length
                        || blake3::hash(&content).to_hex().as_str() != digest
                    {
                        return Err("durable artifact digest mismatch".into());
                    }
                }
            } else if !receipt.artifacts.is_empty() || !receipt.report["manifest_hash"].is_null() {
                return Err("unexpected durable manifest or artifacts".into());
            }
            let (bytes, digest) = self.seal_job(&receipt.job_id)?;
            if bytes != receipt.tree_bytes || digest != receipt.tree_digest {
                return Err("durable job tree tampered".into());
            }
            out.insert(intent.request_id.clone(), receipt);
        }
        Ok(out)
    }
    pub(crate) fn create_job_dir(&self, id: &str) -> Result<Dir> {
        if !valid_job_id(id) {
            return Err("invalid job identifier".into());
        }
        use rustix::fs::{mkdirat, Mode};
        mkdirat(&self.jobs, id, Mode::RUSR | Mode::WUSR | Mode::XUSR).map_err(|e| e.to_string())?;
        rustix::fs::fsync(&self.jobs).map_err(|e| e.to_string())?;
        dir_at(&self.jobs, id)
    }
    pub(crate) fn copy_job_from_temp(&self, id: &str, temp: &Path) -> Result<()> {
        use rustix::fs::{mkdirat, openat, Mode, OFlags};
        let source = open_root_existing(temp)?;
        let target = self.create_job_dir(id)?;
        fn copy(
            source: &Dir,
            target: &Dir,
            depth: usize,
            count: &mut usize,
            total: &mut u64,
        ) -> Result<()> {
            if depth > 16 {
                return Err("candidate tree too deep".into());
            }
            for name in names(source, false)? {
                *count += 1;
                if *count > 10000 {
                    return Err("candidate file count exceeded".into());
                }
                if let Ok(child) = dir_at(source, &name) {
                    mkdirat(target, &name, Mode::RUSR | Mode::WUSR | Mode::XUSR)
                        .map_err(|e| e.to_string())?;
                    let dst = dir_at(target, &name)?;
                    copy(&child, &dst, depth + 1, count, total)?;
                    rustix::fs::fsync(&dst).map_err(|e| e.to_string())?;
                } else {
                    let file = file_at(source, &name)?;
                    let size = file.metadata().map_err(|e| e.to_string())?.len();
                    *total = total.checked_add(size).ok_or("candidate byte overflow")?;
                    if *total > agent_jobs::MAX_JOB_BYTES {
                        return Err("candidate exceeds per-job byte limit".into());
                    }
                    let mut out = File::from(
                        openat(
                            target,
                            &name,
                            OFlags::WRONLY
                                | OFlags::CREATE
                                | OFlags::EXCL
                                | OFlags::NOFOLLOW
                                | OFlags::CLOEXEC,
                            Mode::RUSR | Mode::WUSR,
                        )
                        .map_err(|e| e.to_string())?,
                    );
                    let copied = std::io::copy(&mut file.take(size.saturating_add(1)), &mut out)
                        .map_err(|e| e.to_string())?;
                    if copied != size {
                        return Err("candidate file changed while copying".into());
                    }
                    out.sync_all().map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
        let mut count = 0;
        let mut total = 0;
        copy(&source, &target, 0, &mut count, &mut total)?;
        rustix::fs::fsync(&target).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(crate) fn open_job_dir(&self, id: &str) -> Result<Dir> {
        if !valid_job_id(id) {
            return Err("invalid job identifier".into());
        }
        dir_at(&self.jobs, id)
    }
    pub(crate) fn pinned_path(dir: &Dir) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()))
    }
    pub(crate) fn read_job_file(
        &self,
        id: &str,
        components: &[&str],
        limit: usize,
    ) -> Result<Vec<u8>> {
        let mut dir = self.open_job_dir(id)?;
        for component in &components[..components.len().saturating_sub(1)] {
            dir = dir_at(&dir, component)?;
        }
        let name = components.last().ok_or("missing job file")?;
        if name.is_empty() || name.contains(['/', '\\']) || *name == "." || *name == ".." {
            return Err("invalid job file name".into());
        }
        read_bounded(&dir, name, limit)
    }
    pub(crate) fn seal_job(&self, id: &str) -> Result<(u64, String)> {
        let dir = self.open_job_dir(id)?;
        let mut hash = blake3::Hasher::new();
        let mut bytes = 0u64;
        let mut count = 0usize;
        fn walk(
            dir: &Dir,
            prefix: &str,
            hash: &mut blake3::Hasher,
            bytes: &mut u64,
            count: &mut usize,
            depth: usize,
        ) -> Result<()> {
            if depth > 16 {
                return Err("durable job tree too deep".into());
            }
            for name in names(dir, false)? {
                *count += 1;
                if *count > 10000 {
                    return Err("durable job file count exceeded".into());
                }
                let label = format!("{prefix}{name}");
                if let Ok(child) = dir_at(dir, &name) {
                    hash.update(&((label.len() + 1) as u64).to_le_bytes());
                    hash.update(label.as_bytes());
                    hash.update(b"/");
                    walk(&child, &format!("{label}/"), hash, bytes, count, depth + 1)?;
                    continue;
                }
                let mut file = file_at(dir, &name)?;
                let size = file.metadata().map_err(|e| e.to_string())?.len();
                *bytes = bytes.checked_add(size).ok_or("durable job byte overflow")?;
                if *bytes > agent_jobs::MAX_JOB_BYTES {
                    return Err("durable job byte limit exceeded".into());
                }
                hash.update(&(label.len() as u64).to_le_bytes());
                hash.update(label.as_bytes());
                hash.update(&size.to_le_bytes());
                let mut file_hash = blake3::Hasher::new();
                let mut buf = [0u8; 8192];
                let mut remaining = size;
                while remaining > 0 {
                    let limit = buf.len().min(remaining as usize);
                    let n = file.read(&mut buf[..limit]).map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Err("durable job file changed while hashing".into());
                    }
                    file_hash.update(&buf[..n]);
                    remaining -= n as u64;
                }
                if file.read(&mut buf[..1]).map_err(|e| e.to_string())? != 0 {
                    return Err("durable job file changed while hashing".into());
                }
                hash.update(file_hash.finalize().as_bytes());
            }
            Ok(())
        }
        walk(&dir, "", &mut hash, &mut bytes, &mut count, 0)?;
        Ok((bytes, hash.finalize().to_hex().to_string()))
    }
    pub(crate) fn retained_bytes(&self, intents: &BTreeMap<String, Intent>) -> Result<u64> {
        let allowed = intents
            .values()
            .map(|i| i.job_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let mut total = 0u64;
        for id in names(&self.jobs, false)? {
            if !valid_job_id(&id) || !allowed.contains(id.as_str()) {
                return Err("job directory without intent".into());
            }
            total = total
                .checked_add(self.seal_job(&id)?.0)
                .ok_or("durable retained byte overflow")?;
            if total > agent_jobs::MAX_RETAINED_BYTES {
                return Err("durable retained byte limit exceeded".into());
            }
        }
        Ok(total)
    }
    pub(crate) fn persist_receipt(
        &self,
        request_id: &str,
        job_id: &str,
        report: &Value,
        artifacts: &BTreeMap<String, (u64, String)>,
    ) -> Result<()> {
        let hash = request_hash(request_id)?;
        let (tree_bytes, tree_digest) = self.seal_job(job_id)?;
        let intent = self
            .intents()?
            .remove(request_id)
            .ok_or("receipt without intent")?;
        if intent.job_id != job_id
            || report["snapshot_id"] != intent.snapshot_id
            || report["mode"] != intent.mode
        {
            return Err("receipt does not bind intent".into());
        }
        let receipt = Receipt {
            schema_version: SCHEMA,
            job_id: job_id.to_owned(),
            request_fingerprint: intent.fingerprint,
            snapshot_id: intent.snapshot_id,
            mode: intent.mode,
            report: report.clone(),
            artifacts: artifacts.clone(),
            tree_bytes,
            tree_digest,
        };
        atomic_record(
            &self.receipts,
            &format!("{hash}.json"),
            &serde_json::to_vec(&receipt).map_err(|e| e.to_string())?,
        )
    }
    pub(crate) fn verify_job_seal(&self, id: &str) -> Result<()> {
        let intents = self.intents()?;
        let receipt = self
            .receipts(&intents)?
            .into_values()
            .find(|r| r.job_id == id)
            .ok_or("job has no completed receipt")?;
        let (bytes, digest) = self.seal_job(id)?;
        if bytes != receipt.tree_bytes || digest != receipt.tree_digest {
            return Err("durable job tree tampered".into());
        }
        Ok(())
    }
    pub(crate) fn copy_job_to_temp(&self, id: &str, target: &Path) -> Result<()> {
        let source = self.open_job_dir(id)?;
        let target = open_root_existing(target)?;
        fn copy(
            src: &Dir,
            dst: &Dir,
            depth: usize,
            count: &mut usize,
            total: &mut u64,
        ) -> Result<()> {
            use rustix::fs::{mkdirat, openat, Mode, OFlags};
            if depth > 16 {
                return Err("durable job tree too deep".into());
            }
            for name in names(src, false)? {
                *count += 1;
                if *count > 10000 {
                    return Err("durable job file count exceeded".into());
                }
                if let Ok(child) = dir_at(src, &name) {
                    mkdirat(dst, &name, Mode::RUSR | Mode::WUSR | Mode::XUSR)
                        .map_err(|e| e.to_string())?;
                    copy(&child, &dir_at(dst, &name)?, depth + 1, count, total)?;
                } else {
                    let input = file_at(src, &name)?;
                    let size = input.metadata().map_err(|e| e.to_string())?.len();
                    *total = total.checked_add(size).ok_or("durable job byte overflow")?;
                    if *total > agent_jobs::MAX_JOB_BYTES {
                        return Err("durable job byte limit exceeded".into());
                    }
                    let mut output = File::from(
                        openat(
                            dst,
                            &name,
                            OFlags::WRONLY
                                | OFlags::CREATE
                                | OFlags::EXCL
                                | OFlags::NOFOLLOW
                                | OFlags::CLOEXEC,
                            Mode::RUSR | Mode::WUSR,
                        )
                        .map_err(|e| e.to_string())?,
                    );
                    let copied =
                        std::io::copy(&mut input.take(size.saturating_add(1)), &mut output)
                            .map_err(|e| e.to_string())?;
                    if copied != size {
                        return Err("durable job changed while copying".into());
                    }
                }
            }
            Ok(())
        }
        let mut count = 0;
        let mut total = 0;
        copy(&source, &target, 0, &mut count, &mut total)
    }
}

#[cfg(target_os = "linux")]
fn open_root_existing(path: &Path) -> Result<Dir> {
    use rustix::fs::{open, Mode, OFlags};
    let file = File::from(
        open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(Dir::from_std_file(file))
}
