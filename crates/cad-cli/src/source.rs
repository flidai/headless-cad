//! Immutable, offline recipe source snapshots and strict local module locks.
#[path = "agent_source.rs"]
pub(super) mod agent_source;
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Read;
const MAX_FILE: usize = cad_recipe::MAX_MODULE_SOURCE_BYTES;
const MAX_TOTAL: usize = cad_recipe::MAX_RECIPE_SOURCE_BYTES;
const MAX_MODULES: usize = cad_recipe::MAX_RECIPE_MODULES;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleLock {
    blake3: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lock {
    schema_version: u32,
    #[serde(deserialize_with = "unique_map")]
    modules: BTreeMap<String, ModuleLock>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub recipe_bundle_schema_version: u32,
    pub language: String,
    pub source_name: String,
    pub source: String,
    #[serde(deserialize_with = "unique_map")]
    pub modules: BTreeMap<String, String>,
    pub lock_source: Option<String>,

    #[serde(skip)]
    pub paths: Vec<PathBuf>,
}
fn unique_map<'de, D, V>(deserializer: D) -> Result<BTreeMap<String, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct Visitor<V>(std::marker::PhantomData<V>);
    impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<V> {
        type Value = BTreeMap<String, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique module labels")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = access.next_entry()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate module label"));
                }
                if result.len() > MAX_MODULES {
                    return Err(serde::de::Error::custom("module count limit exceeded"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Visitor(std::marker::PhantomData))
}
fn protected_paths(path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut paths = vec![path.canonicalize()?];
    // Protect every authored path component as well as its resolved target.
    // A directory alias can otherwise be removed while the final target survives.
    let mut current = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => current.push(".."),
            std::path::Component::CurDir => {}
            _ => current.push(component.as_os_str()),
        }
        if let (Some(parent), Some(name)) = (current.parent(), current.file_name()) {
            if let Ok(parent) = parent.canonicalize() {
                paths.push(parent.join(name));
            }
        }
    }
    Ok(paths)
}
fn bounded(path: &Path, limit: usize) -> Result<String, Box<dyn Error>> {
    let mut bytes = Vec::new();
    if !fs::metadata(path)?.is_file() {
        return Err("recipe sources must be regular files".into());
    }
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err("recipe sources must be regular files".into());
    }
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("recipe source size limit exceeded".into());
    }
    Ok(String::from_utf8(bytes)?)
}
fn label(label: &str) -> Result<(), Box<dyn Error>> {
    cad_recipe::validate_module_label(label)?;
    Ok(())
}

impl Snapshot {
    pub fn read(path: &Path) -> Result<Self, Box<dyn Error>> {
        // JSON replay may escape each source byte; bound the serialized envelope
        // separately, then enforce the decoded root/module limits below.
        let text = bounded(
            path,
            if path.extension().is_some_and(|v| v == "json") {
                MAX_TOTAL * 8
            } else {
                MAX_FILE
            },
        )?;
        if path.extension().is_some_and(|v| v == "json")
            && serde_json::from_str::<Value>(&text)?
                .get("recipe_bundle_schema_version")
                .is_some()
        {
            let mut snapshot: Self = serde_json::from_str(&text)?;
            snapshot.paths = protected_paths(path)?;
            snapshot.validate()?;
            return Ok(snapshot);
        }
        let language = match path.extension().and_then(|v| v.to_str()) {
            Some("star") => "starlark",
            Some("json") => "json",
            _ => return Err("recipe extension must be .star or .json".into()),
        };
        let mut snapshot = Self {
            recipe_bundle_schema_version: 1,
            language: language.into(),
            source_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            source: text,
            modules: BTreeMap::new(),
            lock_source: None,
            paths: protected_paths(path)?,
        };
        let lock_path = PathBuf::from(format!("{}.lock.json", path.display()));
        if language == "starlark" && lock_path.try_exists()? {
            let root = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .canonicalize()?;
            let lock_real = lock_path.canonicalize()?;
            if !lock_real.starts_with(&root) {
                return Err("module lock symlink escapes recipe directory".into());
            }
            let lock_source = bounded(&lock_path, MAX_FILE)?;
            let lock: Lock = serde_json::from_str(&lock_source)?;
            if lock.schema_version != 1 || lock.modules.len() > MAX_MODULES {
                return Err("unsupported module lock schema or module count limit".into());
            }
            snapshot.paths.extend(protected_paths(&lock_path)?);
            let mut total = snapshot.source.len();
            for name in lock.modules.keys() {
                label(name)?;
                let module_path = root.join(name).canonicalize()?;
                if !module_path.starts_with(&root) {
                    return Err(format!("module '{name}' symlink escapes recipe directory").into());
                }
                let source = bounded(&module_path, MAX_FILE)?;
                total = total.saturating_add(source.len());
                if total > MAX_TOTAL {
                    return Err("recipe source bundle size limit exceeded".into());
                }
                snapshot.paths.extend(protected_paths(&root.join(name))?);
                snapshot.modules.insert(name.clone(), source);
            }
            snapshot.lock_source = Some(lock_source);
        }
        snapshot.validate()?;
        Ok(snapshot)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.recipe_bundle_schema_version != 1
            || !matches!(self.language.as_str(), "starlark" | "json")
            || self.source.len() > MAX_TOTAL
            || self.modules.len() > MAX_MODULES
            || self.source_name.is_empty()
            || self.source_name.contains(['/', '\\'])
        {
            return Err("invalid recipe source bundle".into());
        }
        if self.language == "starlark" && self.source.len() > MAX_FILE {
            return Err("root Starlark source size limit exceeded".into());
        }
        if self.language != "starlark" && (!self.modules.is_empty() || self.lock_source.is_some()) {
            return Err("JSON IR cannot load modules".into());
        }
        if self
            .lock_source
            .as_ref()
            .is_some_and(|s| s.len() > MAX_FILE)
        {
            return Err("module lock size limit exceeded".into());
        }
        let lock = self
            .lock_source
            .as_ref()
            .map(|s| serde_json::from_str::<Lock>(s))
            .transpose()?;
        if let Some(lock) = &lock {
            if lock.schema_version != 1 || lock.modules.len() != self.modules.len() {
                return Err("module lock inventory mismatch".into());
            }
        } else if !self.modules.is_empty() {
            return Err("module sources require a lock".into());
        }
        let mut total = self.source.len();
        for (name, source) in &self.modules {
            label(name)?;
            total = total.saturating_add(source.len());
            if source.len() > MAX_FILE || total > MAX_TOTAL {
                return Err("recipe module source limit exceeded".into());
            }
            let expected = lock
                .as_ref()
                .and_then(|l| l.modules.get(name))
                .ok_or("module missing from lock")?;
            if expected.blake3 != blake3::hash(source.as_bytes()).to_hex().as_str() {
                return Err(format!("locked module digest mismatch: {name}").into());
            }
        }
        Ok(())
    }
    pub fn recipe(&self, parameters: &BTreeMap<String, f64>) -> Result<Recipe, Box<dyn Error>> {
        Ok(self.evaluate(parameters)?.0)
    }
    pub fn evaluate(
        &self,
        parameters: &BTreeMap<String, f64>,
    ) -> Result<(Recipe, Option<cad_recipe::SourceProvenance>), Box<dyn Error>> {
        if self.language == "starlark" {
            let evaluated = cad_recipe::from_starlark_with_modules_and_provenance(
                &self.source_name,
                &self.source,
                parameters,
                &self.modules,
            )?;
            Ok((evaluated.recipe, Some(evaluated.provenance)))
        } else if parameters.is_empty() {
            Ok((serde_json::from_str(&self.source)?, None))
        } else {
            Err("parameter overrides require a .star recipe; compiled JSON already contains resolved values".into())
        }
    }
    pub fn bundle_hash(&self) -> Option<String> {
        self.lock_source.as_ref().map(|_| {
            blake3::hash(
                &serde_json::to_vec(&(&self.language, &self.source, &self.modules))
                    .expect("source strings serialize"),
            )
            .to_hex()
            .to_string()
        })
    }
    pub fn bind(&self, value: &mut Value) {
        if let Some(hash) = self.bundle_hash() {
            value["source_bundle_hash"] = json!(hash);
            if value.get("snapshot").is_some() {
                value["snapshot"]["source_bundle_hash"] = json!(hash);
                if let Some(query) = value["snapshot"]["query_context_hash"].as_str() {
                    value["snapshot"]["query_context_hash"] =
                        json!(blake3::hash(&serde_json::to_vec(&(query, &hash)).unwrap())
                            .to_hex()
                            .to_string());
                }
            }
        }
    }
}

fn existing_lock(path: &Path) -> Result<Option<(String, Lock)>, Box<dyn Error>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err("lock destination must be a regular file, never a symlink".into());
    }
    let text = bounded(path, MAX_FILE)?;
    let lock: Lock = serde_json::from_str(&text)?;
    if lock.schema_version != 1 {
        return Err("unsupported existing module lock schema".into());
    }
    for (name, entry) in &lock.modules {
        label(name)?;
        if entry.blake3.len() != 64
            || !entry
                .blake3
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("existing lock contains invalid BLAKE3 digest".into());
        }
    }
    Ok(Some((text, lock)))
}

/// Explicitly pin syntax-discovered local imports. No Starlark evaluation occurs.
pub(super) fn lock_recipe(path: &Path) -> Result<Value, Box<dyn Error>> {
    use std::io::Write;
    if !path.extension().is_some_and(|v| v == "star") {
        return Err("lock requires a .star recipe".into());
    }
    let root = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let lock_path = root.join(format!(
        "{}.lock.json",
        path.file_name()
            .ok_or("recipe filename missing")?
            .to_string_lossy()
    ));
    let prior = existing_lock(&lock_path)?;
    let root_text = bounded(path, MAX_FILE)?;
    let root_real = path.canonicalize()?;
    let mut captured = vec![(path.to_path_buf(), root_real, root_text.clone())];
    let mut modules = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    let mut total = root_text.len();
    struct Discovery<'a> {
        root: &'a Path,
        visiting: &'a mut BTreeSet<String>,
        modules: &'a mut BTreeMap<String, ModuleLock>,
        captured: &'a mut Vec<(PathBuf, PathBuf, String)>,
        total: &'a mut usize,
    }
    impl Discovery<'_> {
        fn visit(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
            label(name)?;
            if self.modules.contains_key(name) {
                return Ok(());
            }
            if !self.visiting.insert(name.to_owned()) {
                return Err(format!("module import cycle at '{name}'").into());
            }
            if self.modules.len().saturating_add(self.visiting.len()) > MAX_MODULES {
                return Err("module count limit exceeded".into());
            }
            let logical = self.root.join(name);
            let real = logical.canonicalize()?;
            if !real.starts_with(self.root) {
                return Err(format!("module '{name}' symlink escapes recipe directory").into());
            }
            let text = bounded(&real, MAX_FILE)?;
            *self.total = self.total.saturating_add(text.len());
            if *self.total > MAX_TOTAL {
                return Err("recipe source bundle size limit exceeded".into());
            }
            let loads = cad_recipe::starlark_loads(name, &text)?;
            self.captured.push((logical, real, text.clone()));
            for dependency in loads {
                self.visit(&dependency)?;
            }
            self.visiting.remove(name);
            self.modules.insert(
                name.to_owned(),
                ModuleLock {
                    blake3: blake3::hash(text.as_bytes()).to_hex().to_string(),
                },
            );
            Ok(())
        }
    }
    let loads = cad_recipe::starlark_loads(&path.to_string_lossy(), &root_text)?;
    let mut discovery = Discovery {
        root: &root,
        visiting: &mut visiting,
        modules: &mut modules,
        captured: &mut captured,
        total: &mut total,
    };
    for name in loads {
        discovery.visit(&name)?;
    }
    let empty = BTreeMap::new();
    let previous = prior
        .as_ref()
        .map(|(_, lock)| &lock.modules)
        .unwrap_or(&empty);
    let added: Vec<_> = modules
        .keys()
        .filter(|name| !previous.contains_key(*name))
        .cloned()
        .collect();
    let changed: Vec<_> = modules
        .iter()
        .filter(|(name, entry)| previous.get(*name).is_some_and(|old| old != *entry))
        .map(|(name, _)| name.clone())
        .collect();
    let removed: Vec<_> = previous
        .keys()
        .filter(|name| !modules.contains_key(*name))
        .cloned()
        .collect();
    let text = serde_json::to_string_pretty(&Lock {
        schema_version: 1,
        modules: modules.clone(),
    })? + "\n";
    // Source paths include canonical targets and authored aliases. A lock cannot
    // replace an input file through any of those names.
    for (logical, real, _) in &captured {
        if real == &lock_path || protected_paths(logical)?.contains(&lock_path) {
            return Err("lock destination overlaps a recipe source".into());
        }
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = root.join(format!(
        ".cad-lock-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let publish = (|| -> Result<(), Box<dyn Error>> {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        // Best-effort concurrent-edit guard immediately before atomic rename.
        for (logical, real, bytes) in &captured {
            if logical.canonicalize()? != *real || bounded(logical, MAX_FILE)? != *bytes {
                return Err("recipe sources changed while locking; retry".into());
            }
        }
        if existing_lock(&lock_path)? != prior {
            return Err("module lock changed concurrently; retry".into());
        }
        fs::rename(&temporary, &lock_path)?;
        Ok(())
    })();
    if publish.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    publish?;
    Ok(
        json!({"schema_version":1,"command":"lock","lock_path":lock_path,"module_count":modules.len(),"source_bytes":total,"added":added,"changed":changed,"removed":removed,"updated":prior.as_ref().is_none_or(|(old,_)|old != &text),"evaluated_recipe":false}),
    )
}
