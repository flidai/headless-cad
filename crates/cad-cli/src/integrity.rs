//! Release-byte integrity, independent of recipe and geometry evaluation.
//! A digest is not a signature. Trust requires an independently supplied manifest pin.
//! Verification assumes immutable generations, as produced by atomic publication;
//! it does not create a filesystem snapshot against arbitrary in-place writers.
//! Static symlinks are rejected everywhere. Linux additionally rejects racing
//! symlink/FIFO substitutions; the portable fallback confines opens to the pinned
//! directory but requires callers not to mutate its entries during verification.
use cap_std::fs::Dir;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Value};
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub const MANIFEST_SCHEMA_VERSION: u64 = 1;
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PAYLOAD_FILES: usize = 100_000;

#[derive(Debug)]
pub struct IntegrityError(String);
impl fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for IntegrityError {}
impl From<io::Error> for IntegrityError {
    fn from(error: io::Error) -> Self {
        Self(format!("artifact I/O failed: {error}"))
    }
}
fn fail(message: impl Into<String>) -> IntegrityError {
    IntegrityError(message.into())
}

pub fn check_manifest_size(bytes: u64) -> Result<(), IntegrityError> {
    if bytes > MAX_MANIFEST_BYTES {
        return Err(fail("manifest exceeds the 16 MiB byte limit"));
    }
    Ok(())
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
fn digest_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[cfg(test)]
pub fn inventory(artifacts: &[(String, String)]) -> Result<Value, IntegrityError> {
    if artifacts.len() > MAX_PAYLOAD_FILES {
        return Err(fail("payload file count exceeds 100000"));
    }
    let mut files = BTreeMap::new();
    let mut names = BTreeSet::new();
    for (name, content) in artifacts {
        if !safe_name(name)
            || name.eq_ignore_ascii_case("manifest.json")
            || !names.insert(name.to_ascii_lowercase())
        {
            return Err(fail(format!(
                "invalid or duplicate payload filename: {name}"
            )));
        }
        files.insert(name, json!({"bytes":content.len(),"digest":blake3::hash(content.as_bytes()).to_hex().to_string()}));
    }
    Ok(json!({"algorithm":"blake3","files":files}))
}

// Pin the directory inode across atomic publication. On Linux, NOFOLLOW and
// NONBLOCK also prevent a racing symlink/FIFO replacement from being followed
// or blocking before the opened file's type is checked.
fn open_directory(path: &Path) -> Result<Dir, IntegrityError> {
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{open, Mode, OFlags};
        let fd = open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| fail(format!("cannot open artifact directory: {e}")))?;
        Ok(Dir::from_std_file(File::from(fd)))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(fail("artifact directory must be a real directory"));
        }
        Ok(Dir::open_ambient_dir(path, cap_std::ambient_authority())?)
    }
}
fn open_regular(directory: &Dir, name: &str) -> Result<File, IntegrityError> {
    let metadata = directory
        .symlink_metadata(name)
        .map_err(|e| fail(format!("cannot inspect '{name}': {e}")))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(fail(format!("artifact '{name}' is not a regular file")));
    }
    #[cfg(target_os = "linux")]
    let file = {
        use rustix::fs::{openat, Mode, OFlags};
        let fd = openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| fail(format!("cannot open '{name}': {e}")))?;
        File::from(fd)
    };
    #[cfg(not(target_os = "linux"))]
    let file = directory.open(name)?.into_std();
    if !file.metadata()?.is_file() {
        return Err(fail(format!("artifact '{name}' is not a regular file")));
    }
    Ok(file)
}

pub fn verify(path: &Path, pin: Option<&str>) -> Result<Value, IntegrityError> {
    if pin.is_some_and(|hash| !digest_valid(hash)) {
        return Err(fail(
            "manifest hash must be 64 lowercase hexadecimal characters",
        ));
    }
    let directory = open_directory(path)?;
    let manifest_file = open_regular(&directory, "manifest.json")?;
    check_manifest_size(manifest_file.metadata()?.len())?;
    let mut bytes = Vec::new();
    manifest_file
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    check_manifest_size(bytes.len() as u64)?;
    let manifest_hash = blake3::hash(&bytes).to_hex().to_string();
    if pin.is_some_and(|expected| expected != manifest_hash) {
        return Err(fail("manifest hash does not match trusted pin"));
    }
    let raw: UniqueValue =
        serde_json::from_slice(&bytes).map_err(|e| fail(format!("invalid manifest JSON: {e}")))?;
    let manifest = raw.0;
    if manifest["manifest_schema_version"].as_u64() != Some(MANIFEST_SCHEMA_VERSION) {
        return Err(fail("unsupported or missing manifest_schema_version; rebuild legacy artifacts to add integrity"));
    }
    let integrity = manifest["artifact_integrity"]
        .as_object()
        .ok_or_else(|| fail("missing artifact_integrity object"))?;
    if integrity.get("algorithm").and_then(Value::as_str) != Some("blake3") {
        return Err(fail("unsupported artifact integrity algorithm"));
    }
    let files = integrity
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| fail("missing integrity files object"))?;
    if files.len() > MAX_PAYLOAD_FILES {
        return Err(fail("payload file count exceeds 100000"));
    }
    let declared = manifest["artifact_files"]
        .as_array()
        .ok_or_else(|| fail("missing artifact_files array"))?;
    if declared.len() > MAX_PAYLOAD_FILES + 1 {
        return Err(fail("artifact_files count exceeds limit"));
    }
    let mut names = BTreeSet::new();
    let mut folded = BTreeSet::new();
    for value in declared {
        let name = value
            .as_str()
            .ok_or_else(|| fail("artifact_files entries must be strings"))?;
        if !safe_name(name) {
            return Err(fail(format!("unsafe artifact filename: {name}")));
        }
        if !names.insert(name) || !folded.insert(name.to_ascii_lowercase()) {
            return Err(fail(format!("duplicate artifact filename: {name}")));
        }
    }
    if !names.remove("manifest.json") {
        return Err(fail(
            "artifact_files must include manifest.json exactly once",
        ));
    }
    if names.len() != files.len() || files.keys().any(|name| !names.contains(name.as_str())) {
        return Err(fail("artifact_files and integrity files membership differ"));
    }
    // Enumerate through the same handle, rejecting any extra file/directory and
    // all nonregular entries. Bound iteration even for an adversarial directory.
    let mut found = BTreeSet::new();
    for entry in directory.entries()? {
        let entry = entry?;
        if found.len() > MAX_PAYLOAD_FILES {
            return Err(fail("directory entry count exceeds limit"));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| fail("directory contains a non-UTF-8 filename"))?;
        if name != "manifest.json" && !names.contains(name.as_str()) {
            return Err(fail(format!("untracked directory entry: {name}")));
        }
        if !entry.file_type()?.is_file() {
            return Err(fail(format!("artifact '{name}' is not a regular file")));
        }
        found.insert(name);
    }
    if found.len() != names.len() + 1
        || !found.contains("manifest.json")
        || names.iter().any(|name| !found.contains(*name))
    {
        return Err(fail("artifact directory membership differs from manifest"));
    }
    let mut verified_bytes = 0u64;
    for (name, expected) in files {
        let size = expected["bytes"]
            .as_u64()
            .ok_or_else(|| fail(format!("invalid byte count for '{name}'")))?;
        let digest = expected["digest"]
            .as_str()
            .filter(|digest| digest_valid(digest))
            .ok_or_else(|| fail(format!("invalid digest for '{name}'")))?;
        let mut file = open_regular(&directory, name)?;
        if file.metadata()?.len() != size {
            return Err(fail(format!("byte count mismatch for '{name}'")));
        }
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut actual = 0u64;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            actual = actual
                .checked_add(count as u64)
                .ok_or_else(|| fail("artifact byte count overflow"))?;
            if actual > size {
                return Err(fail(format!("byte count mismatch for '{name}'")));
            }
            hasher.update(&buffer[..count]);
        }
        if actual != size || hasher.finalize().to_hex().as_str() != digest {
            return Err(fail(format!("digest or byte count mismatch for '{name}'")));
        }
        verified_bytes = verified_bytes
            .checked_add(actual)
            .ok_or_else(|| fail("total verified bytes overflow"))?;
    }
    Ok(
        json!({"schema_version":1,"valid":true,"manifest_hash":manifest_hash,"verified_files":files.len(),"verified_bytes":verified_bytes,"algorithm":"blake3"}),
    )
}

// serde_json::Value normally accepts duplicate object keys with last-one-wins
// semantics. Reject them at every depth so a manifest has one interpretation.
struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| UniqueValue(Value::Number(v)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_keys_at_any_depth() {
        for input in [r#"{"a":1,"a":2}"#, r#"{"files":{"part":{},"part":{}}}"#] {
            assert!(serde_json::from_str::<UniqueValue>(input).is_err());
        }
        assert!(serde_json::from_str::<UniqueValue>(r#"{"a":[true,null,1,-2,0.2,"x"]}"#).is_ok());
    }
    #[test]
    fn bounded_portable_inventory() {
        for name in ["../x", "a/b", "a\\b", ".", "..", "", "x.", "manifest.json"] {
            assert!(inventory(&[(name.into(), "test".into())]).is_err());
        }
        assert!(inventory(&[("a".into(), "test".into()), ("A".into(), "test".into())]).is_err());
        assert!(check_manifest_size(MAX_MANIFEST_BYTES).is_ok());
        assert!(check_manifest_size(MAX_MANIFEST_BYTES + 1).is_err());
    }
}
