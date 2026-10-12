//! Private, fsynced artifact spool with one final verified publication.
use cap_std::fs::Dir;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRecord {
    pub name: String,
    pub bytes: u64,
    pub digest: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Destination {
    Absent,
    Existing { device: u64, inode: u64 },
}

pub struct ArtifactSink {
    absolute: PathBuf,
    parent: PathBuf,
    destination: Destination,
    staging: tempfile::TempDir,
    stage: Dir,
    records: Vec<ArtifactRecord>,
    indices: BTreeMap<String, usize>,
    folded: BTreeSet<String>,
    manifest_written: bool,
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.ends_with('.')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

fn destination_state(path: &Path) -> Result<Destination> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Destination::Absent)
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("output path must be a real directory, not a symlink or file".into());
    }
    let mut entries = fs::read_dir(path)?;
    if let Some(first) = entries.next() {
        let manifest = open_existing_regular(path, "manifest.json")
            .map_err(|_| "refusing to replace a nonempty directory without a CAD build manifest")?;
        let mut bytes = Vec::new();
        manifest
            .take(crate::integrity::MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)?;
        crate::integrity::check_manifest_size(bytes.len() as u64)?;
        let old: Value = serde_json::from_slice(&bytes)?;
        let files = old["artifact_files"].as_array().ok_or("existing directory has no owned-artifact manifest; use a dedicated empty output directory")?;
        let owned: BTreeSet<&str> = files.iter().filter_map(Value::as_str).collect();
        for (index, entry) in std::iter::once(first).chain(entries).enumerate() {
            if index > crate::integrity::MAX_PAYLOAD_FILES {
                return Err("output directory entry count exceeds limit".into());
            }
            let entry = entry?;
            let name = entry.file_name();
            if !entry.file_type()?.is_file() || !owned.contains(name.to_string_lossy().as_ref()) {
                return Err(format!(
                    "output directory contains unmanaged file '{}'",
                    name.to_string_lossy()
                )
                .into());
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Destination::Existing {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        Ok(Destination::Existing {
            device: 0,
            inode: 0,
        })
    }
}

fn open_existing_regular(dir: &Path, name: &str) -> Result<File> {
    let pinned = Dir::open_ambient_dir(dir, cap_std::ambient_authority())?;
    let metadata = pinned.symlink_metadata(name)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("manifest is not a regular file".into());
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{openat, Mode, OFlags};
        let fd = openat(
            &pinned,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let file = File::from(fd);
        if !file.metadata()?.is_file() {
            return Err("manifest is not a regular file".into());
        }
        Ok(file)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(pinned.open(name)?.into_std())
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

impl ArtifactSink {
    pub fn new(out_dir: &Path) -> Result<Self> {
        let absolute = std::path::absolute(out_dir)?;
        let parent = absolute
            .parent()
            .ok_or("output must have a parent directory")?
            .to_path_buf();
        if absolute.file_name().is_none() {
            return Err("output cannot be a filesystem root".into());
        }
        fs::create_dir_all(&parent)?;
        let destination = destination_state(&absolute)?;
        let staging = tempfile::Builder::new()
            .prefix(".cad-stage-")
            .tempdir_in(&parent)?;
        let stage = Dir::open_ambient_dir(staging.path(), cap_std::ambient_authority())?;
        Ok(Self {
            absolute,
            parent,
            destination,
            staging,
            stage,
            records: Vec::new(),
            indices: BTreeMap::new(),
            folded: BTreeSet::new(),
            manifest_written: false,
        })
    }

    pub fn push(&mut self, (name, content): (String, String)) -> Result<()> {
        if !safe_name(&name) || self.folded.contains(&name.to_ascii_lowercase()) {
            return Err(format!("invalid or duplicate payload filename: {name}").into());
        }
        if self.manifest_written {
            return Err("manifest.json must be the final artifact".into());
        }
        let is_manifest = name == "manifest.json";
        if name.eq_ignore_ascii_case("manifest.json") && !is_manifest {
            return Err("manifest filename must be exactly manifest.json".into());
        }
        if !is_manifest && self.records.len() >= crate::integrity::MAX_PAYLOAD_FILES {
            return Err("payload file count exceeds 100000".into());
        }
        if is_manifest {
            crate::integrity::check_manifest_size(content.len() as u64)?;
        }
        let mut file = self.create_new(&name)?;
        if let Err(error) = file
            .write_all(content.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            let _ = self.stage.remove_file(&name);
            return Err(error.into());
        }
        let record = ArtifactRecord {
            name: name.clone(),
            bytes: content.len() as u64,
            digest: blake3::hash(content.as_bytes()).to_hex().to_string(),
        };
        self.indices.insert(name.clone(), self.records.len());
        self.folded.insert(name.to_ascii_lowercase());
        self.records.push(record);
        if is_manifest {
            self.manifest_written = true;
        }
        Ok(())
    }

    fn create_new(&self, name: &str) -> Result<File> {
        #[cfg(target_os = "linux")]
        {
            use rustix::fs::{openat, Mode, OFlags};
            Ok(File::from(openat(
                &self.stage,
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )?))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.staging.path().join(name))?)
        }
    }

    fn open_record(&self, name: &str) -> Result<File> {
        if !self.indices.contains_key(name) {
            return Err("unknown staged artifact".into());
        }
        let metadata = self.stage.symlink_metadata(name)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("staged artifact is not a regular file".into());
        }
        #[cfg(target_os = "linux")]
        {
            use rustix::fs::{openat, Mode, OFlags};
            let file = File::from(openat(
                &self.stage,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            if !file.metadata()?.is_file() {
                return Err("staged artifact is not a regular file".into());
            }
            Ok(file)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(self.stage.open(name)?.into_std())
        }
    }

    pub fn records(&self) -> &[ArtifactRecord] {
        &self.records
    }
    pub fn get(&self, name: &str) -> Option<&ArtifactRecord> {
        self.indices.get(name).map(|&index| &self.records[index])
    }
    pub fn contains(&self, name: &str) -> bool {
        self.indices.contains_key(name)
    }

    pub fn read_text(&self, name: &str, limit: u64) -> Result<String> {
        let record = self.get(name).ok_or("unknown staged artifact")?;
        if record.bytes > limit {
            return Err("staged artifact exceeds read limit".into());
        }
        let file = self.open_record(name)?;
        if file.metadata()?.len() != record.bytes {
            return Err("staged artifact length changed".into());
        }
        let mut bytes = Vec::new();
        file.take(record.bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != record.bytes
            || bytes.len() as u64 > limit
            || blake3::hash(&bytes).to_hex().as_str() != record.digest
        {
            return Err("staged artifact changed".into());
        }
        Ok(String::from_utf8(bytes)?)
    }

    pub fn same_bytes(&self, name: &str, content: &str) -> Result<bool> {
        let record = self.get(name).ok_or("unknown staged artifact")?;
        let mut file = self.open_record(name)?;
        if file.metadata()?.len() != record.bytes {
            return Err("staged artifact length changed".into());
        }
        let mut compared = 0usize;
        let mut equal = record.bytes == content.len() as u64;
        let mut hasher = blake3::Hasher::new();
        let expected = content.as_bytes();
        let mut buffer = [0u8; 64 * 1024];
        while (compared as u64) < record.bytes {
            let length = (record.bytes as usize - compared).min(buffer.len());
            file.read_exact(&mut buffer[..length])?;
            hasher.update(&buffer[..length]);
            equal &= expected
                .get(compared..compared + length)
                .is_some_and(|slice| buffer[..length] == *slice);
            compared += length;
        }
        if file.read(&mut buffer[..1])? != 0 || file.metadata()?.len() != record.bytes {
            return Err("staged artifact length changed".into());
        }
        if hasher.finalize().to_hex().as_str() != record.digest {
            return Err("staged artifact digest changed".into());
        }
        Ok(equal)
    }

    pub fn inventory(&self) -> Result<Value> {
        let mut files = BTreeMap::new();
        for record in &self.records {
            if record.name != "manifest.json" {
                files.insert(
                    &record.name,
                    json!({"bytes":record.bytes,"digest":record.digest}),
                );
            }
        }
        Ok(json!({"algorithm":"blake3","files":files}))
    }

    pub fn commit(self) -> Result<()> {
        if !self.manifest_written {
            return Err("manifest.json is required before publication".into());
        }
        let manifest_digest = self
            .get("manifest.json")
            .ok_or("missing staged manifest")?
            .digest
            .clone();
        let verified = crate::integrity::verify(self.staging.path(), Some(&manifest_digest))?;
        if verified["valid"] != true {
            return Err("staged generation integrity verification failed".into());
        }
        sync_directory(self.staging.path())?;
        if destination_state(&self.absolute)? != self.destination {
            return Err("output directory changed before publication".into());
        }
        match self.destination {
            Destination::Existing { .. } => {
                #[cfg(target_os = "linux")]
                rustix::fs::renameat_with(
                    rustix::fs::CWD,
                    self.staging.path(),
                    rustix::fs::CWD,
                    &self.absolute,
                    rustix::fs::RenameFlags::EXCHANGE,
                )?;
                #[cfg(not(target_os = "linux"))]
                {
                    let backup = tempfile::Builder::new()
                        .prefix(".cad-previous-")
                        .tempdir_in(&self.parent)?;
                    fs::remove_dir(backup.path())?;
                    fs::rename(&self.absolute, backup.path())?;
                    if let Err(error) = fs::rename(self.staging.path(), &self.absolute) {
                        if let Err(rollback) = fs::rename(backup.path(), &self.absolute) {
                            let preserved = backup.keep();
                            return Err(format!("publication failed: {error}; rollback failed: {rollback}; previous generation preserved at {}",preserved.display()).into());
                        }
                        return Err(error.into());
                    }
                }
            }
            Destination::Absent => fs::rename(self.staging.path(), &self.absolute)?,
        }
        sync_directory(&self.parent).map_err(|error| {
            format!("generation published, but parent-directory durability sync failed: {error}")
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(sink: &ArtifactSink) -> String {
        let files = sink
            .records()
            .iter()
            .map(|record| record.name.as_str())
            .chain(std::iter::once("manifest.json"))
            .collect::<Vec<_>>();
        json!({
            "manifest_schema_version": crate::integrity::MANIFEST_SCHEMA_VERSION,
            "artifact_files": files,
            "artifact_integrity": sink.inventory().unwrap(),
        })
        .to_string()
    }

    #[test]
    fn publishes_verified_generation_and_replaces_only_owned_directory() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("generation");
        let mut first = ArtifactSink::new(&output).unwrap();
        first.push(("data.txt".into(), "αβγ".into())).unwrap();
        assert!(first.same_bytes("data.txt", "αβγ").unwrap());
        assert!(!first.same_bytes("data.txt", "δεζ").unwrap());
        assert_eq!(first.read_text("data.txt", 6).unwrap(), "αβγ");
        assert!(first.read_text("data.txt", 5).is_err());
        first
            .push(("manifest.json".into(), manifest(&first)))
            .unwrap();
        first.commit().unwrap();
        assert_eq!(fs::read_to_string(output.join("data.txt")).unwrap(), "αβγ");

        let mut second = ArtifactSink::new(&output).unwrap();
        second.push(("data.txt".into(), "new".into())).unwrap();
        second
            .push(("manifest.json".into(), manifest(&second)))
            .unwrap();
        second.commit().unwrap();
        assert_eq!(fs::read_to_string(output.join("data.txt")).unwrap(), "new");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_commit_preserves_previous_generation_and_discards_stage() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("generation");
        let mut first = ArtifactSink::new(&output).unwrap();
        first.push(("data.txt".into(), "old".into())).unwrap();
        first
            .push(("manifest.json".into(), manifest(&first)))
            .unwrap();
        first.commit().unwrap();

        let mut second = ArtifactSink::new(&output).unwrap();
        let stage = second.staging.path().to_path_buf();
        second.push(("data.txt".into(), "new".into())).unwrap();
        second
            .push(("manifest.json".into(), manifest(&second)))
            .unwrap();
        fs::write(stage.join("untracked.txt"), "extra").unwrap();
        assert!(second.commit().is_err());
        assert!(!stage.exists());
        assert_eq!(fs::read_to_string(output.join("data.txt")).unwrap(), "old");

        let mut third = ArtifactSink::new(&output).unwrap();
        third.push(("data.txt".into(), "new".into())).unwrap();
        third
            .push(("manifest.json".into(), manifest(&third)))
            .unwrap();
        fs::write(output.join("unmanaged.txt"), "mine").unwrap();
        assert!(third.commit().is_err());
        assert_eq!(fs::read_to_string(output.join("data.txt")).unwrap(), "old");
    }

    #[test]
    fn duplicate_names_and_manifest_order_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut sink = ArtifactSink::new(&root.path().join("generation")).unwrap();
        sink.push(("Case.txt".into(), "first".into())).unwrap();
        assert!(sink.push(("case.txt".into(), "second".into())).is_err());
        assert!(sink.push(("../escape".into(), "bad".into())).is_err());
        sink.push(("manifest.json".into(), manifest(&sink)))
            .unwrap();
        assert!(sink.push(("later.txt".into(), "bad".into())).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_substitution_is_not_read_or_published() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside.txt");
        fs::write(&outside, "secret").unwrap();
        let output = root.path().join("generation");
        let mut sink = ArtifactSink::new(&output).unwrap();
        sink.push(("data.txt".into(), "expected".into())).unwrap();
        sink.push(("manifest.json".into(), manifest(&sink)))
            .unwrap();
        fs::remove_file(sink.staging.path().join("data.txt")).unwrap();
        symlink(&outside, sink.staging.path().join("data.txt")).unwrap();
        assert!(sink.read_text("data.txt", 64).is_err());
        assert!(sink.same_bytes("data.txt", "expected").is_err());
        assert!(sink.commit().is_err());
        assert!(!output.exists());
        assert_eq!(fs::read_to_string(outside).unwrap(), "secret");
    }
}
