//! Safe Linux process admission before any recipe loading; not a filesystem or RSS sandbox.
use serde_json::{json, Value};
use std::io::Write;
pub const ADMISSION_FILE: &str = "worker-admission.json";
pub const PARENT_PID_ENV: &str = "CAD_AGENT_EXPECTED_PARENT_PID";
pub const ADDRESS_SPACE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const CPU_SOFT_SECONDS: u64 = 45;
pub const CPU_HARD_SECONDS: u64 = 46;
pub const OPEN_FILES: u64 = 128;
pub fn policy() -> Value {
    json!({"platform":"linux","enforcement":"process_rlimits_before_recipe_evaluation","parent_lifetime":"parent_death_sigkill_registered_before_admission","address_space_bytes":ADDRESS_SPACE_BYTES,"cpu_soft_seconds":CPU_SOFT_SECONDS,"cpu_hard_seconds":CPU_HARD_SECONDS,"file_bytes":FILE_BYTES,"core_bytes":0,"open_files":OPEN_FILES,"respects_stricter_inherited_limits":true,"hard_rss_limit":false,"aggregate_disk_quota":false,"filesystem_sandbox":false})
}
#[cfg(target_os = "linux")]
pub fn enforce_parent_lifetime() -> Result<(), String> {
    use rustix::process::{getppid, set_parent_process_death_signal, Signal};
    let expected: u32 = std::env::var(PARENT_PID_ENV)
        .map_err(|_| "missing worker parent identity")?
        .parse()
        .map_err(|_| "invalid worker parent identity")?;
    if expected == 0 {
        return Err("invalid worker parent identity".into());
    }
    set_parent_process_death_signal(Some(Signal::KILL))
        .map_err(|e| format!("worker parent lifetime registration failed: {e}"))?;
    if getppid().map(|pid| pid.as_raw_nonzero().get() as u32) != Some(expected) {
        return Err("worker parent changed before lifetime registration".into());
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub fn enforce_parent_lifetime() -> Result<(), String> {
    Err("worker parent lifetime enforcement requires Linux".into())
}
pub fn validate_record(record: &Value) -> Result<(), String> {
    if record["schema_version"] != 1
        || record["platform"] != "linux"
        || record["enforcement"] != "process_rlimits_before_recipe_evaluation"
        || record["parent_lifetime"] != "parent_death_sigkill_registered_before_admission"
    {
        return Err("invalid worker admission identity".into());
    }
    for (name, soft, hard) in [
        ("core_bytes", 0, 0),
        (
            "address_space_bytes",
            ADDRESS_SPACE_BYTES,
            ADDRESS_SPACE_BYTES,
        ),
        ("cpu_seconds", CPU_SOFT_SECONDS, CPU_HARD_SECONDS),
        ("file_bytes", FILE_BYTES, FILE_BYTES),
        ("open_files", OPEN_FILES, OPEN_FILES),
    ] {
        let entry = &record["effective_limits"][name];
        let s = entry["soft"]
            .as_u64()
            .ok_or("missing admitted soft limit")?;
        let h = entry["hard"]
            .as_u64()
            .ok_or("missing admitted hard limit")?;
        if s > h || s > soft || h > hard {
            return Err("worker admitted limits exceed policy".into());
        }
    }
    Ok(())
}
#[cfg(target_os = "linux")]
pub fn apply() -> Result<Value, String> {
    use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
    let mut effective = serde_json::Map::new();
    for (name, resource, soft, hard) in [
        ("core_bytes", Resource::Core, 0, 0),
        (
            "address_space_bytes",
            Resource::As,
            ADDRESS_SPACE_BYTES,
            ADDRESS_SPACE_BYTES,
        ),
        (
            "cpu_seconds",
            Resource::Cpu,
            CPU_SOFT_SECONDS,
            CPU_HARD_SECONDS,
        ),
        ("file_bytes", Resource::Fsize, FILE_BYTES, FILE_BYTES),
        ("open_files", Resource::Nofile, OPEN_FILES, OPEN_FILES),
    ] {
        let previous = getrlimit(resource);
        let maximum = previous.maximum.map_or(hard, |v| v.min(hard));
        let current = previous.current.map_or(soft, |v| v.min(soft)).min(maximum);
        setrlimit(
            resource,
            Rlimit {
                current: Some(current),
                maximum: Some(maximum),
            },
        )
        .map_err(|e| format!("worker admission failed for {name}: {e}"))?;
        let observed = getrlimit(resource);
        if observed.current != Some(current) || observed.maximum != Some(maximum) {
            return Err(format!("worker admission verification failed for {name}"));
        }
        effective.insert(name.into(), json!({"soft":current,"hard":maximum}));
    }
    Ok(
        json!({"schema_version":1,"platform":"linux","enforcement":"process_rlimits_before_recipe_evaluation","parent_lifetime":"parent_death_sigkill_registered_before_admission","effective_limits":effective,"hard_rss_limit":false,"aggregate_disk_quota":false,"filesystem_sandbox":false}),
    )
}
#[cfg(not(target_os = "linux"))]
pub fn apply() -> Result<Value, String> {
    Err("hard worker resource admission requires Linux".into())
}
pub fn apply_and_record() -> Result<Value, String> {
    let record = apply()?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(ADMISSION_FILE)
        .map_err(|e| format!("worker admission record: {e}"))?;
    file.write_all(&serde_json::to_vec(&record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(record)
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn admission_probe() {
        if std::env::var_os("CAD_LIMIT_PROBE").is_none() {
            return;
        }
        use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
        // A stricter inherited limit must not be widened.
        setrlimit(
            Resource::Nofile,
            Rlimit {
                current: Some(48),
                maximum: Some(64),
            },
        )
        .unwrap();
        let probe = std::env::var("CAD_LIMIT_PROBE").unwrap();
        if probe == "cpu" {
            setrlimit(
                Resource::Cpu,
                Rlimit {
                    current: Some(1),
                    maximum: Some(2),
                },
            )
            .unwrap();
        }
        let r = apply_and_record().unwrap();
        if probe == "cpu" {
            loop {
                std::hint::black_box(1u64.wrapping_add(1));
            }
        }
        if probe == "file" {
            std::fs::File::create("too-large")
                .unwrap()
                .set_len(FILE_BYTES + 1)
                .unwrap();
            panic!("file size limit was not enforced");
        }
        validate_record(&r).unwrap();
        let mut invalid = r.clone();
        invalid["effective_limits"]["address_space_bytes"]["hard"] = json!(ADDRESS_SPACE_BYTES + 1);
        assert!(validate_record(&invalid).is_err());
        assert_eq!(r["effective_limits"]["open_files"]["soft"], 48);
        assert_eq!(getrlimit(Resource::As).maximum, Some(ADDRESS_SPACE_BYTES));
        assert_eq!(getrlimit(Resource::Core).maximum, Some(0));
        let mut reservation = Vec::<u8>::new();
        assert!(reservation
            .try_reserve_exact(ADDRESS_SPACE_BYTES as usize)
            .is_err());
        let file = std::fs::File::create("too-large").unwrap();
        // Ignore no signal: using an already oversized offset is tested separately
        // by the parent process; this probe verifies the admitted ceiling itself.
        assert_eq!(getrlimit(Resource::Fsize).maximum, Some(FILE_BYTES));
        drop(file);
        let mut files = Vec::new();
        loop {
            match std::fs::File::open("/dev/null") {
                Ok(f) => files.push(f),
                Err(e) => {
                    assert_eq!(e.raw_os_error(), Some(24));
                    break;
                }
            }
        }
    }
    #[test]
    fn cpu_and_file_limits_terminate_only_isolated_probes() {
        use std::os::unix::process::ExitStatusExt;
        for (probe, signal) in [
            ("cpu", rustix::process::Signal::XCPU.as_raw()),
            ("file", rustix::process::Signal::XFSZ.as_raw()),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "worker_limits::tests::admission_probe",
                    "--nocapture",
                ])
                .env("CAD_LIMIT_PROBE", probe)
                .current_dir(dir.path())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert_eq!(status.signal(), Some(signal), "{probe}");
            assert!(!dir.path().join("core").exists());
        }
    }
    #[test]
    fn resource_admission_isolated_from_test_runner() {
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "worker_limits::tests::admission_probe",
                "--nocapture",
            ])
            .env("CAD_LIMIT_PROBE", "1")
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let record: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(ADMISSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(record["effective_limits"]["open_files"]["hard"], 64);
    }
}
