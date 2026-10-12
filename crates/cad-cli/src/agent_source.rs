//! Capability-confined immutable source capture for the local agent interface.
//!
//! Register as a CHILD of `source`, so lock parsing and snapshot validation stay
//! identical to ordinary CLI builds. Never reopen a canonicalized ambient path.
use super::{Lock, Snapshot, MAX_FILE, MAX_MODULES, MAX_TOTAL};
use cap_std::fs::Dir;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{self, Read};

pub(crate) const MAX_BUNDLE_BYTES: usize = MAX_TOTAL * 8;
const MAX_PATH_BYTES: usize = 1024;
const MAX_PATH_COMPONENTS: usize = 32;

fn components(path: &str) -> Result<Vec<&str>, Box<dyn Error>> {
    let parts: Vec<_> = path.split('/').collect();
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || parts.len() > MAX_PATH_COMPONENTS
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == ".." || p.trim() != *p)
    {
        return Err("source requires a bounded canonical workspace-relative path".into());
    }
    Ok(parts)
}

/// Open ONE validated component relative to an already-held directory. NOFOLLOW
/// is applied to every component, including directories; handles remain pinned
/// if another process renames a parent. NONBLOCK prevents a replacement FIFO
/// from hanging before its opened-handle metadata can be checked. Devices and
/// sockets are rejected before any read. This first agent interface is Linux-only;
/// other platforms fail closed rather than claiming equivalent race protection.
#[cfg(target_os = "linux")]
fn component(dir: &Dir, name: &str, directory: bool) -> io::Result<std::fs::File> {
    use rustix::fs::{openat, Mode, OFlags};
    let mut flags =
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::NOCTTY;
    if directory {
        flags |= OFlags::DIRECTORY;
    }
    let file = std::fs::File::from(openat(dir, name, flags, Mode::empty())?);
    let metadata = file.metadata()?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source must be a regular file beneath real directories",
        ));
    }
    Ok(file)
}
#[cfg(not(target_os = "linux"))]
fn component(_: &Dir, _: &str, _: bool) -> io::Result<std::fs::File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "confined agent capture currently requires Linux",
    ))
}
fn parent(dir: &Dir, path: &str) -> Result<(Dir, String), Box<dyn Error>> {
    let parts = components(path)?;
    let mut current = dir.try_clone()?;
    for name in &parts[..parts.len() - 1] {
        current = Dir::from_std_file(component(&current, name, true)?);
    }
    Ok((current, parts.last().unwrap().to_string()))
}
fn read(dir: &Dir, name: &str, limit: usize) -> io::Result<String> {
    let file = component(dir, name, false)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source size limit exceeded",
        ));
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source size limit exceeded",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source must be UTF-8"))
}
fn checked(mut snapshot: Snapshot) -> Result<Snapshot, Box<dyn Error>> {
    snapshot.validate()?;
    if components(&snapshot.source_name)?.len() != 1 {
        return Err("bundle source_name must be one portable filename".into());
    }
    // A captured agent source has no ambient filesystem authority. Publication
    // goes to separate server-owned jobs, never over this source directory.
    snapshot.paths.clear();
    Ok(snapshot)
}

/// Capture a root, its adjacent lock and all pinned modules without following
/// symlinks. Module labels are relative to the pinned recipe-parent capability.
/// Concurrent regular-file edits can produce a different captured root revision;
/// the exact captured bytes are authoritative and every module must match its
/// captured lock. No cross-file atomic snapshot of an actively edited tree is
/// claimed. Compilation/evaluation happens later, entirely from this snapshot.
pub(crate) fn capture(workspace: &Dir, relative: &str) -> Result<Snapshot, Box<dyn Error>> {
    let (root, name) = parent(workspace, relative)?;
    let language = if name.ends_with(".star") {
        "starlark"
    } else if name.ends_with(".json") {
        "json"
    } else {
        return Err("recipe extension must be .star or .json".into());
    };
    let text = read(
        &root,
        &name,
        if language == "json" {
            MAX_BUNDLE_BYTES
        } else {
            MAX_FILE
        },
    )?;
    if language == "json"
        && serde_json::from_str::<serde_json::Value>(&text)?
            .get("recipe_bundle_schema_version")
            .is_some()
    {
        return decode_bundle(text.as_bytes());
    }
    let mut snapshot = Snapshot {
        recipe_bundle_schema_version: 1,
        language: language.into(),
        source_name: name.clone(),
        source: text,
        modules: BTreeMap::new(),
        lock_source: None,
        paths: vec![],
    };
    if language == "starlark" {
        let lock = match read(&root, &format!("{name}.lock.json"), MAX_FILE) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if let Some(text) = lock {
            let lock: Lock = serde_json::from_str(&text)?;
            if lock.schema_version != 1 || lock.modules.len() > MAX_MODULES {
                return Err("unsupported module lock schema or module count limit".into());
            }
            let mut total = snapshot.source.len();
            for label in lock.modules.keys() {
                cad_recipe::validate_module_label(label)?;
                let (directory, file) = parent(&root, label)?;
                let source = read(&directory, &file, MAX_FILE)?;
                total = total.saturating_add(source.len());
                if total > MAX_TOTAL {
                    return Err("recipe source bundle size limit exceeded".into());
                }
                snapshot.modules.insert(label.clone(), source);
            }
            snapshot.lock_source = Some(text);
        }
    }
    checked(snapshot)
}

pub(crate) fn decode_bundle(bytes: &[u8]) -> Result<Snapshot, Box<dyn Error>> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err("serialized source bundle size limit exceeded".into());
    }
    checked(serde_json::from_slice(bytes)?)
}

/// Revise only root source, retaining the captured module lock, module bytes,
/// language and logical filename. The owning source store records parent ID and
/// derives the candidate ID from the returned immutable serialized snapshot.
/// This does not write to the workspace or refresh imported module pins.
pub(crate) fn revise(snapshot: &Snapshot, source: String) -> Result<Snapshot, Box<dyn Error>> {
    let mut candidate = snapshot.clone();
    candidate.source = source;
    checked(candidate)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use cap_std::ambient_authority;
    use std::os::unix::fs::symlink;
    fn workspace() -> (tempfile::TempDir, Dir) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = Dir::open_ambient_dir(tmp.path(), ambient_authority()).unwrap();
        (tmp, dir)
    }
    fn fixture(root: &std::path::Path) {
        std::fs::create_dir(root.join("lib")).unwrap();
        std::fs::write(root.join("main.star"),"load(\"lib/dims.star\", \"width\")\np=rectangle(id=\"p\",width=width,depth=4)\nb=extrude(id=\"b\",profile=p,height=2)\noutput(name=\"part\",feature=b)\n").unwrap();
        std::fs::write(root.join("lib/dims.star"), "width=8\n").unwrap();
        let hash = blake3::hash(b"width=8\n").to_hex().to_string();
        std::fs::write(root.join("main.star.lock.json"),serde_json::to_vec(&serde_json::json!({"schema_version":1,"modules":{"lib/dims.star":{"blake3":hash}}})).unwrap()).unwrap();
    }
    #[test]
    fn immutable_capture_reuses_lock_and_recipe_semantics() {
        let (tmp, dir) = workspace();
        fixture(tmp.path());
        let captured = capture(&dir, "main.star").unwrap();
        let ambient = Snapshot::read(&tmp.path().join("main.star")).unwrap();
        assert_eq!(captured.bundle_hash(), ambient.bundle_hash());
        assert_eq!(
            serde_json::to_value(captured.recipe(&BTreeMap::new()).unwrap()).unwrap(),
            serde_json::to_value(ambient.recipe(&BTreeMap::new()).unwrap()).unwrap()
        );
        std::fs::write(tmp.path().join("lib/dims.star"), "width=99\n").unwrap();
        assert!(capture(&dir, "main.star").is_err());
        assert!(captured.recipe(&BTreeMap::new()).is_ok());
        assert!(captured.paths.is_empty());
        let bundle = serde_json::to_vec(&captured).unwrap();
        assert_eq!(decode_bundle(&bundle).unwrap().source, captured.source);
        let revision = revise(&captured, "# candidate\n".into()).unwrap();
        assert_eq!(revision.modules, captured.modules);
        assert_eq!(revision.lock_source, captured.lock_source);
        assert_eq!(revision.source_name, captured.source_name);
        assert_ne!(revision.source, captured.source);
        assert!(revise(&captured, "x".repeat(MAX_FILE + 1)).is_err());
    }
    #[test]
    fn json_and_replay_are_captured_without_filesystem_dependencies() {
        let (tmp, dir) = workspace();
        fixture(tmp.path());
        let recipe = capture(&dir, "main.star")
            .unwrap()
            .recipe(&BTreeMap::new())
            .unwrap();
        let json = serde_json::to_string(&recipe).unwrap();
        std::fs::write(tmp.path().join("plain.json"), &json).unwrap();
        let captured = capture(&dir, "plain.json").unwrap();
        assert_eq!(captured.language, "json");
        assert!(captured.recipe(&BTreeMap::new()).is_ok());
        let revised = revise(&captured, json.replace("[]", "[ ]")).unwrap();
        assert_eq!(revised.language, "json");
        std::fs::write(
            tmp.path().join("replay.json"),
            serde_json::to_vec(&captured).unwrap(),
        )
        .unwrap();
        std::fs::remove_file(tmp.path().join("plain.json")).unwrap();
        assert_eq!(capture(&dir, "replay.json").unwrap(), captured);
        let mut raw = serde_json::to_value(&captured).unwrap();
        raw["paths"] = serde_json::json!(["/outside"]);
        assert!(decode_bundle(&serde_json::to_vec(&raw).unwrap()).is_err());
    }
    #[test]
    fn traversal_root_lock_and_module_symlinks_are_rejected() {
        let (tmp, dir) = workspace();
        fixture(tmp.path());
        for name in [
            "",
            "../main.star",
            "/main.star",
            "./main.star",
            "a//b.star",
            "a/../main.star",
            "a\\b.star",
            "C:main.star",
            " main.star",
            "main.star\n",
        ] {
            assert!(capture(&dir, name).is_err(), "{name:?}");
        }
        symlink("main.star", tmp.path().join("alias.star")).unwrap();
        assert!(capture(&dir, "alias.star").is_err());
        symlink("lib", tmp.path().join("alias")).unwrap();
        assert!(capture(&dir, "alias/dims.star").is_err());
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.star"), "secret").unwrap();
        std::fs::remove_file(tmp.path().join("lib/dims.star")).unwrap();
        symlink(
            outside.path().join("secret.star"),
            tmp.path().join("lib/dims.star"),
        )
        .unwrap();
        assert!(capture(&dir, "main.star").is_err());
        std::fs::remove_file(tmp.path().join("main.star.lock.json")).unwrap();
        symlink(
            outside.path().join("missing.json"),
            tmp.path().join("main.star.lock.json"),
        )
        .unwrap();
        assert!(capture(&dir, "main.star").is_err());
    }
    #[test]
    fn fifo_directory_bad_utf8_and_oversize_sources_fail_without_reading() {
        let (tmp, dir) = workspace();
        rustix::fs::mkfifoat(
            &dir,
            "pipe.star",
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        let started = std::time::Instant::now();
        assert!(capture(&dir, "pipe.star").is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        std::fs::write(tmp.path().join("main.star"), "# source").unwrap();
        rustix::fs::mkfifoat(
            &dir,
            "main.star.lock.json",
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(capture(&dir, "main.star").is_err());
        std::fs::create_dir(tmp.path().join("directory.star")).unwrap();
        assert!(capture(&dir, "directory.star").is_err());
        std::fs::write(tmp.path().join("bad.star"), [255]).unwrap();
        assert!(capture(&dir, "bad.star").is_err());
        let file = std::fs::File::create(tmp.path().join("large.star")).unwrap();
        file.set_len((MAX_FILE + 1) as u64).unwrap();
        assert!(capture(&dir, "large.star").is_err());
    }
    #[test]
    fn pinned_parent_survives_rename_without_following_replacement() {
        let (tmp, dir) = workspace();
        std::fs::create_dir(tmp.path().join("project")).unwrap();
        std::fs::write(tmp.path().join("project/main.star"), "captured").unwrap();
        let (parent, name) = parent(&dir, "project/main.star").unwrap();
        std::fs::rename(tmp.path().join("project"), tmp.path().join("old")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("main.star"), "secret").unwrap();
        symlink(outside.path(), tmp.path().join("project")).unwrap();
        assert_eq!(read(&parent, &name, MAX_FILE).unwrap(), "captured");
        assert!(capture(&dir, "project/main.star").is_err());
    }
    #[test]
    fn bundle_and_lock_unknown_duplicate_or_untrusted_fields_fail() {
        let (tmp, dir) = workspace();
        fixture(tmp.path());
        let captured = capture(&dir, "main.star").unwrap();
        let mut value = serde_json::to_value(&captured).unwrap();
        value["modules"]["lib/dims.star"] = serde_json::json!("width=9\n");
        assert!(decode_bundle(&serde_json::to_vec(&value).unwrap()).is_err());
        value = serde_json::to_value(&captured).unwrap();
        value["source_name"] = serde_json::json!("../escape.star");
        assert!(decode_bundle(&serde_json::to_vec(&value).unwrap()).is_err());
        value = serde_json::to_value(&captured).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(decode_bundle(&serde_json::to_vec(&value).unwrap()).is_err());
        let text = String::from_utf8(serde_json::to_vec(&captured).unwrap()).unwrap();
        let duplicate = text.replacen('{', "{\"language\":\"starlark\",", 1);
        assert!(decode_bundle(duplicate.as_bytes()).is_err());
        std::fs::write(tmp.path().join("main.star.lock.json"),r#"{"schema_version":1,"modules":{"lib/dims.star":{"blake3":"a"},"lib/dims.star":{"blake3":"b"}}}"#).unwrap();
        assert!(capture(&dir, "main.star").is_err());
    }
}
