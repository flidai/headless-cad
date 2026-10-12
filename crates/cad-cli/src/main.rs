mod source;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cad_build::{BuildEngine, BuildResult, Recipe};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

mod capabilities;
mod exchange;
mod integrity;
mod section;
mod server;
mod watch;

#[derive(Debug)]
struct InvalidBrep(cad_kernel::ValidationReport);

impl std::fmt::Display for InvalidBrep {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "B-rep validation failed with {} issue(s)",
            self.0.issues.len()
        )
    }
}

impl Error for InvalidBrep {}

#[derive(Debug)]
struct UnresolvedSelection(Value);

impl std::fmt::Display for UnresolvedSelection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            self.0["diagnostic"]
                .as_str()
                .unwrap_or("semantic face reference did not resolve"),
        )
    }
}

impl Error for UnresolvedSelection {}

#[derive(Parser)]
#[command(
    name = "cad",
    version,
    about = "Build reproducible solid models with a Rust kernel"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Override a declared Starlark parameter, repeatable.
    #[arg(long = "param", global = true, value_name = "NAME=VALUE")]
    params: Vec<String>,
    /// Persistent geometry cache directory.
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Verify published artifact bytes without evaluating a recipe.
    Verify {
        directory: PathBuf,
        /// Trusted BLAKE3 hash of the exact manifest bytes.
        #[arg(long)]
        manifest_hash: Option<String>,
    },
    /// Describe supported recipe operations, limits and headless workflows as JSON.
    Capabilities {
        /// Return one operation and its inline type/domain dependencies.
        #[arg(long, conflicts_with = "object_type")]
        operation: Option<String>,
        /// Return one object type and its transitive inline object dependencies.
        #[arg(long = "type", conflicts_with = "operation")]
        object_type: Option<String>,
    },
    /// Integrate volume, centroid and central inertia from exact output geometry.
    Properties {
        recipe: PathBuf,
        #[arg(long)]
        output: String,
        /// Optional uniform body density; assemblies use each part's material.
        #[arg(long)]
        density_kg_per_m3: Option<f64>,
    },
    /// Query an authoritative analytic planar section without publishing artifacts.
    Section {
        recipe: PathBuf,
        #[arg(long)]
        output: String,
        #[arg(long, allow_hyphen_values = true)]
        origin: String,
        #[arg(long, default_value = "0,0,1", allow_hyphen_values = true)]
        normal: String,
        #[arg(long, default_value = "1,0,0", allow_hyphen_values = true)]
        x_direction: String,
        #[arg(long, default_value = "json", value_parser = ["json", "svg"])]
        format: String,
    },
    /// Report component quantities, materials, and mass for an assembly output.
    Bom {
        recipe: PathBuf,
        #[arg(long)]
        assembly: String,
    },
    /// Resolve a semantic face role through the feature graph without publishing artifacts.
    Select {
        recipe: PathBuf,
        #[arg(long = "from")]
        anchor: String,
        #[arg(long)]
        role: String,
        #[arg(long = "in")]
        target: String,
        /// Require exactly this positive number of faces (default: 1).
        #[arg(long, conflicts_with = "all")]
        count: Option<std::num::NonZeroUsize>,
        /// Accept every surviving face, including splits.
        #[arg(long, conflicts_with = "count")]
        all: bool,
    },
    /// Rebuild changed recipes while preserving the last successful artifacts.
    Watch {
        recipe: PathBuf,
        #[arg(long, default_value = "out")]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 500, value_parser = clap::value_parser!(u64).range(1..))]
        interval_ms: u64,
        /// Publish compact previews without engineering export artifacts.
        #[arg(long)]
        display_only: bool,
        /// Select outputs (repeatable); replaces the complete published generation.
        #[arg(long = "output", value_name = "NAME")]
        outputs: Vec<String>,
    },
    /// Serve generated previews and exports through a read-only local viewer.
    Serve {
        #[arg(default_value = "out")]
        directory: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Create a starter recipe without overwriting existing files.
    Init { directory: PathBuf },
    /// Validate a B-rep snapshot or assembly manifest and its referenced definitions.
    Validate { model: PathBuf },
    /// Compare output hashes, volumes, and bounds for two recipes.
    Diff { before: PathBuf, after: PathBuf },
    /// Explicitly pin transitive local Starlark imports without evaluating the recipe.
    Lock { recipe: PathBuf },
    /// Compile a .star or .json recipe into the versioned JSON feature graph.
    Compile { recipe: PathBuf },
    /// Evaluate a recipe and report geometry, provenance, and validation as JSON.
    Inspect {
        recipe: PathBuf,
        /// Evaluate only selected outputs and all global semantic requirements.
        #[arg(long = "output", value_name = "NAME")]
        outputs: Vec<String>,
    },
    /// Build and validate every output; exits nonzero on any invalid result.
    Check { recipe: PathBuf },
    /// Write each output as B-rep JSON, STEP, and STL, plus a build manifest.
    Build {
        recipe: PathBuf,
        #[arg(long, default_value = "out")]
        out_dir: PathBuf,
        /// Publish compact previews without B-rep, STEP, STL, or SVG exports.
        #[arg(long)]
        display_only: bool,
        /// Select outputs (repeatable); replaces the complete published generation.
        #[arg(long = "output", value_name = "NAME")]
        outputs: Vec<String>,
    },
}

fn report(recipe: &Recipe, build: &BuildResult) -> Result<Value, Box<dyn Error>> {
    let mut outputs: serde_json::Map<String, Value> = build
        .outputs
        .iter()
        .map(|(name, output)| {
            let model = &output.result.model;
            (
                name.clone(),
                json!({
                    "feature_id": output.feature_id,
                    "content_hash": output.content_hash,
                    "provenance": output.provenance,
                    "bounds_mm": if model.solids.is_empty() { Value::Null } else { json!(model.bounds()) },
                    "volume_mm3": model.volume(),
                    "topology": {
                        "vertices": model.vertices.len(), "edges": model.edges.len(),
                        "faces": model.faces.len(), "shells": model.shells.len(),
                        "solids": model.solids.len(),
                    },
                    "validation": model.validate(recipe.tolerance.unwrap_or_default()),
                    "history": output.result.history,
                }),
            )
        })
        .collect();
    for (name, assembly) in &build.assemblies {
        let bom = assembly.bill_of_materials(recipe.tolerance.unwrap_or_default())?;
        outputs.insert(name.clone(), json!({
            "kind": "assembly",
            "feature_id": assembly.assembly_id,
            "content_hash": assembly.content_hash,
            "bounds_mm": assembly.bounds(),
            "bounds_kind": "conservative_axis_aligned",
            "volume_mm3": assembly.material_volume(),
            "material_volume_mm3": assembly.material_volume(),
            "volume_kind": "summed_instance_material_not_union",
            "definitions": assembly.parts.len(),
            "assembly_definitions": assembly.hierarchy.len(),
            "instances": assembly.instance_count(),
            "bom": bom,
            "joints": assembly.joint_reports(recipe.tolerance.unwrap_or_default())?,
            "topology": { "solids": assembly.solid_count() },
            "validation": { "valid": true, "scope": "individual definitions and rigid placements; interference not checked" },
        }));
    }
    Ok(json!({
        "kernel_version": cad_kernel::KERNEL_VERSION,
        "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
        "executor_build_id": cad_build::executor_build_id(),
        "solver_build_id": cad_build::solver_build_id(),
        "schema_version": recipe.schema_version,
        "units": recipe.units,
        "tolerance": recipe.tolerance.unwrap_or_default(),
        "stats": build.stats,
        "scope": build.scope,
        "feature_hashes": build.feature_hashes,
        "face_assertions": build.face_assertions,
        "property_assertions": build.property_assertions,
        "section_views": build.section_views,
        "exchange_requirements": exchange::pending(&recipe.exchange_requirements),
        "exchange_policy_status": if recipe.exchange_requirements.is_empty() { "not_requested" } else { "pending_publication" },
        "sketch_reports": build.sketch_reports,
        "mechanism_reports": build.mechanism_reports,
        "mechanism_bindings": build.mechanism_bindings,
        "mechanism_context_hash": build.mechanism_context_hash,
        "face_datums": build.face_datums,
        "outputs": outputs,
    }))
}

fn bom_document(bom: &cad_assembly::BomReport) -> Result<Value, Box<dyn Error>> {
    let mut document = serde_json::to_value(bom)?;
    document["schema_version"] = json!(1);
    document["volume_units"] = json!("mm3");
    document["mass_units"] = json!("kg");
    document["scope"] =
        json!("leaf part occurrences; overlaps counted; no interference or material certification");
    Ok(document)
}

// Recipe output names are logical IDs. Only portable filename components may
// become paths; this check occurs before creating any artifact files.
fn filename(name: &str) -> Result<(), Box<dyn Error>> {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        || name.eq_ignore_ascii_case("manifest")
        || name.ends_with('.')
        || reserved
    {
        return Err(format!("output name '{name}' must be a portable filename (letters, digits, '-', '_', '.'); 'manifest' is reserved").into());
    }
    Ok(())
}

fn parameters(values: &[String]) -> Result<BTreeMap<String, f64>, Box<dyn Error>> {
    let mut result = BTreeMap::new();
    for entry in values {
        let (name, value) = entry
            .split_once('=')
            .ok_or("parameter must be NAME=VALUE")?;
        let number: f64 = value.parse()?;
        if name.is_empty() || !number.is_finite() {
            return Err("parameter name must be nonempty and value must be finite".into());
        }
        if result.insert(name.to_owned(), number).is_some() {
            return Err(format!("duplicate parameter override '{name}'").into());
        }
    }
    Ok(result)
}

fn engine(cache: &Option<PathBuf>) -> BuildEngine {
    match cache {
        Some(path) => BuildEngine::with_cache_dir(path.clone()),
        None => BuildEngine::default(),
    }
}

const STARTER: &str = "# Dimensions are millimetres. Override with --param width=100.\nwidth = param(name = 'width', default = 80)\ndepth = param(name = 'depth', default = 40)\nheight = param(name = 'height', default = 6)\nprofile = rectangle(id = 'outline', width = width, depth = depth)\npart = extrude(id = 'blank', profile = profile, height = height)\noutput(name = 'plate', feature = part)\n";

fn initialize(directory: &Path) -> Result<Value, Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let path = directory.join("main.star");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(STARTER.as_bytes())?;
    file.sync_all()?;
    Ok(json!({ "created": path }))
}

// Publish a complete generation in a single directory rename/exchange. Existing
// nonempty directories must carry our manifest, and may contain only its files.
// A failed build leaves the previous generation available and unchanged.
fn publish(out_dir: &Path, artifacts: &[(String, String)]) -> Result<(), Box<dyn Error>> {
    let absolute = std::path::absolute(out_dir)?;
    let parent = absolute
        .parent()
        .ok_or("output must have a parent directory")?;
    if absolute.file_name().is_none() {
        return Err("output cannot be a filesystem root".into());
    }
    fs::create_dir_all(parent)?;
    let existing = match fs::symlink_metadata(&absolute) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("output path must be a real directory, not a symlink or file".into());
            }
            let entries = fs::read_dir(&absolute)?.collect::<Result<Vec<_>, _>>()?;
            if !entries.is_empty() {
                let manifest_path = absolute.join("manifest.json");
                let old: Value =
                    serde_json::from_str(&fs::read_to_string(manifest_path).map_err(|_| {
                        "refusing to replace a nonempty directory without a CAD build manifest"
                    })?)?;
                let files = old["artifact_files"].as_array().ok_or("existing directory has no owned-artifact manifest; use a dedicated empty output directory")?;
                let owned: BTreeSet<&str> = files.iter().filter_map(Value::as_str).collect();
                for entry in entries {
                    if !entry.file_type()?.is_file()
                        || !owned.contains(entry.file_name().to_string_lossy().as_ref())
                    {
                        return Err(format!(
                            "output directory contains unmanaged file '{}'",
                            entry.file_name().to_string_lossy()
                        )
                        .into());
                    }
                }
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let staging = tempfile::Builder::new()
        .prefix(".cad-stage-")
        .tempdir_in(parent)?;
    for (name, content) in artifacts {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging.path().join(name))?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    if existing {
        #[cfg(target_os = "linux")]
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            staging.path(),
            rustix::fs::CWD,
            &absolute,
            rustix::fs::RenameFlags::EXCHANGE,
        )?;
        #[cfg(not(target_os = "linux"))]
        {
            // Portable fallback: complete generations only, but readers can see
            // a short missing-directory interval. Restore the old one on error.
            let backup = tempfile::Builder::new()
                .prefix(".cad-previous-")
                .tempdir_in(parent)?;
            fs::remove_dir(backup.path())?;
            fs::rename(&absolute, backup.path())?;
            if let Err(error) = fs::rename(staging.path(), &absolute) {
                if let Err(rollback) = fs::rename(backup.path(), &absolute) {
                    let preserved = backup.keep();
                    return Err(format!("publication failed: {error}; rollback failed: {rollback}; previous generation preserved at {}", preserved.display()).into());
                }
                return Err(error.into());
            }
        }
    } else {
        fs::rename(staging.path(), &absolute)?;
    }
    // After exchange the temporary directory contains the previous generation.
    // Its drop removes old artifacts, including outputs removed from the recipe.
    Ok(())
}

fn difference(before: &Value, after: &Value) -> Value {
    let before_outputs = before["outputs"].as_object().expect("build outputs");
    let after_outputs = after["outputs"].as_object().expect("build outputs");
    let names: BTreeSet<_> = before_outputs.keys().chain(after_outputs.keys()).collect();
    let changes: serde_json::Map<String, Value> = names.into_iter().map(|name| {
        let a = before_outputs.get(name);
        let b = after_outputs.get(name);
        let status = match (a,b) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(a), Some(b)) if a["content_hash"] == b["content_hash"] => "unchanged",
            _ => "changed",
        };
        let details = |v: Option<&Value>| v.map(|v| json!({"content_hash":v["content_hash"],"volume_mm3":v["volume_mm3"],"bounds_mm":v["bounds_mm"],"topology":v["topology"],"bom":v["bom"]}));
        let delta = match (a,b) {
            (Some(a),Some(b)) => Some(b["volume_mm3"].as_f64().unwrap() - a["volume_mm3"].as_f64().unwrap()),
            _ => None,
        };
        (name.clone(), json!({"status":status,"before":details(a),"after":details(b),"volume_delta_mm3":delta}))
    }).collect();
    let before_assertions = before["face_assertions"]
        .as_object()
        .expect("build assertions");
    let after_assertions = after["face_assertions"]
        .as_object()
        .expect("build assertions");
    let assertion_names: BTreeSet<_> = before_assertions
        .keys()
        .chain(after_assertions.keys())
        .collect();
    let assertions: serde_json::Map<String, Value> = assertion_names
        .into_iter()
        .map(|name| {
            let a = before_assertions.get(name);
            let b = after_assertions.get(name);
            let status = match (a, b) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(a), Some(b)) if a == b => "unchanged",
                _ => "changed",
            };
            (name.clone(), json!({"status":status,"before":a,"after":b}))
        })
        .collect();
    let before_properties = before["property_assertions"]
        .as_object()
        .expect("build property assertions");
    let after_properties = after["property_assertions"]
        .as_object()
        .expect("build property assertions");
    let properties: serde_json::Map<String, Value> = before_properties
        .keys()
        .chain(after_properties.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| {
            let a = before_properties.get(name);
            let b = after_properties.get(name);
            let status = match (a, b) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(a), Some(b)) if a == b => "unchanged",
                _ => "changed",
            };
            (name.clone(), json!({"status":status,"before":a,"after":b}))
        })
        .collect();
    let before_exchange = before["exchange_requirements"]
        .as_object()
        .expect("exchange requirements");
    let after_exchange = after["exchange_requirements"]
        .as_object()
        .expect("exchange requirements");
    let exchange_changes: serde_json::Map<String, Value> = before_exchange
        .keys()
        .chain(after_exchange.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|id| {
            let a = before_exchange.get(id);
            let b = after_exchange.get(id);
            let status = match (a, b) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(a), Some(b)) if a == b => "unchanged",
                _ => "changed",
            };
            (id.clone(), json!({"status":status,"before":a,"after":b}))
        })
        .collect();
    let before_views = before["section_views"].as_object().expect("section views");
    let after_views = after["section_views"].as_object().expect("section views");
    let view_changes: serde_json::Map<String, Value> = before_views
        .keys()
        .chain(after_views.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|id| {
            let a = before_views.get(id);
            let b = after_views.get(id);
            let status = match (a, b) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(a), Some(b)) if a == b => "unchanged",
                _ => "changed",
            };
            (id.clone(), json!({"status":status,"before":a,"after":b}))
        })
        .collect();
    let before_datums = before["face_datums"]
        .as_object()
        .expect("build face datums");
    let after_datums = after["face_datums"].as_object().expect("build face datums");
    let parts: BTreeSet<_> = before_datums.keys().chain(after_datums.keys()).collect();
    let datums: serde_json::Map<String, Value> = parts
        .into_iter()
        .map(|part| {
            let a = before_datums.get(part).and_then(Value::as_object);
            let b = after_datums.get(part).and_then(Value::as_object);
            let names: BTreeSet<_> = a
                .into_iter()
                .flat_map(|v| v.keys())
                .chain(b.into_iter().flat_map(|v| v.keys()))
                .collect();
            let changes: serde_json::Map<String, Value> = names
                .into_iter()
                .map(|name| {
                    let old = a.and_then(|v| v.get(name));
                    let new = b.and_then(|v| v.get(name));
                    let status = match (old, new) {
                        (None, Some(_)) => "added",
                        (Some(_), None) => "removed",
                        (Some(a), Some(b)) if a == b => "unchanged",
                        _ => "changed",
                    };
                    (
                        name.clone(),
                        json!({"status":status,"before":old,"after":new}),
                    )
                })
                .collect();
            (part.clone(), json!(changes))
        })
        .collect();
    let before_sketches = before["sketch_reports"]
        .as_object()
        .expect("sketch reports");
    let after_sketches = after["sketch_reports"].as_object().expect("sketch reports");
    let sketches: serde_json::Map<String, Value> = before_sketches.keys()
        .chain(after_sketches.keys()).collect::<BTreeSet<_>>().into_iter().map(|id| {
            let a = before_sketches.get(id);
            let b = after_sketches.get(id);
            let old_hash = &before["feature_hashes"][id];
            let new_hash = &after["feature_hashes"][id];
            let status = match (a,b) {
                (None,Some(_)) => "added",
                (Some(_),None) => "removed",
                (Some(a),Some(b)) if a == b && old_hash == new_hash => "unchanged",
                _ => "changed",
            };
            (id.clone(),json!({"status":status,"before":a,"after":b,"before_hash":old_hash,"after_hash":new_hash}))
        }).collect();
    let mechanisms = json!({
        "status": if before["mechanism_context_hash"] == after["mechanism_context_hash"] { "unchanged" } else { "changed" },
        "before": before["mechanism_reports"], "after": after["mechanism_reports"],
        "before_bindings": before["mechanism_bindings"], "after_bindings": after["mechanism_bindings"],
        "before_context_hash": before["mechanism_context_hash"], "after_context_hash": after["mechanism_context_hash"],
    });
    json!({"outputs":changes,"mechanisms":mechanisms,"section_views":view_changes,"exchange_requirements":exchange_changes,"exchange_policy_status":{"before":before["exchange_policy_status"],"after":after["exchange_policy_status"]},"face_assertions":assertions,"property_assertions":properties,"sketches":sketches,"face_datums":datums,"scope":{"before":before["scope"],"after":after["scope"]},"comparison":"build hashes, measured properties, sketch solutions, assertion witnesses, native section views, pending exchange requirements and associative datum resolutions; not a proof of geometric equivalence"})
}

fn validate_assembly_manifest(path: &Path, raw: &Value) -> Result<Value, Box<dyn Error>> {
    if raw["schema_version"] != 1 || raw["units"] != "mm" || raw["kind"] != "assembly" {
        return Err("unsupported assembly artifact schema, kind or units".into());
    }
    if raw["bounds_kind"] != "conservative_axis_aligned"
        || raw["volume_kind"] != "summed_instance_material_not_union"
    {
        return Err("assembly measurement kind metadata is inconsistent".into());
    }
    if raw["assembly_build_id"].as_str() != Some(cad_assembly::assembly_build_id()) {
        return Err("assembly artifact requires its producing implementation; rebuild the recipe with this executable before validating".into());
    }
    let text = |value: &Value| -> Result<String, Box<dyn Error>> {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "missing assembly artifact string".into())
    };
    // Display is a derived proxy, outside exact validation. Validate its reference
    // syntax without requiring the optional display file or trusting its content.
    filename(&text(&raw["display_asset"])?)?;
    let definitions = raw["definitions"]
        .as_object()
        .ok_or("missing assembly definitions")?;
    let instances = raw["instances"]
        .as_array()
        .ok_or("missing assembly instances")?;
    if definitions.len() > cad_build::MAX_PARTS || instances.len() > cad_build::MAX_INSTANCES {
        return Err("assembly artifact exceeds definition or instance count limit".into());
    }
    // Typed placement parsing and count checks precede any referenced asset I/O.
    let instances = serde_json::from_value(raw["instances"].clone())?;
    let hierarchy: BTreeMap<String, cad_assembly::AssemblyDefinition> =
        serde_json::from_value(raw["hierarchy"].clone())?;
    let instance_paths: BTreeMap<String, Vec<String>> =
        serde_json::from_value(raw["instance_paths"].clone())?;
    if hierarchy.len() > cad_build::MAX_ASSEMBLIES
        || instance_paths.len() > cad_build::MAX_INSTANCES
        || hierarchy.values().fold(0usize, |total, definition| {
            total
                .saturating_add(definition.instances.len())
                .saturating_add(definition.assemblies.len())
        }) > cad_build::MAX_INSTANCES
    {
        return Err("assembly hierarchy exceeds definition or occurrence count limit".into());
    }
    let tolerance = serde_json::from_value(raw["tolerance"].clone())?;
    let root = path.parent().unwrap_or(Path::new(".")).canonicalize()?;
    let mut assets: BTreeMap<PathBuf, std::sync::Arc<cad_kernel::OperationResult>> =
        BTreeMap::new();
    let mut parts = BTreeMap::new();
    for (name, definition) in definitions {
        let asset = text(&definition["geometry_asset"])?;
        filename(&asset)?;
        let asset_path = root.join(&asset).canonicalize()?;
        if !asset_path.starts_with(&root) {
            return Err("assembly geometry asset escapes artifact directory".into());
        }
        let result = match assets.get(&asset_path) {
            Some(result) => std::sync::Arc::clone(result),
            None => {
                let result = std::sync::Arc::new(serde_json::from_slice(&fs::read(&asset_path)?)?);
                assets.insert(asset_path, std::sync::Arc::clone(&result));
                result
            }
        };
        parts.insert(
            name.clone(),
            cad_assembly::BuiltPart {
                datums: serde_json::from_value(
                    definition
                        .get("datums")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )?,
                feature_id: text(&definition["feature_id"])?,
                content_hash: text(&definition["content_hash"])?,
                result,
                metadata: serde_json::from_value(
                    definition
                        .get("metadata")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )?,
            },
        );
    }
    let assembly = cad_assembly::BuiltAssembly {
        assembly_id: text(&raw["assembly_id"])?,
        content_hash: text(&raw["content_hash"])?,
        parts,
        instances,
        hierarchy,
        instance_paths,
    };
    assembly.validate(tolerance)?;
    fn measured(value: &Value, expected: f64, field: &str) -> Result<(), Box<dyn Error>> {
        if value.as_f64() != Some(expected) {
            return Err(
                format!("assembly metadata '{field}' does not match measured geometry").into(),
            );
        }
        Ok(())
    }
    fn bounds(
        value: &Value,
        expected: Option<cad_kernel::Bounds>,
        field: &str,
    ) -> Result<(), Box<dyn Error>> {
        let actual: Option<cad_kernel::Bounds> = serde_json::from_value(value.clone())?;
        if actual != expected {
            return Err(
                format!("assembly metadata '{field}' does not match measured geometry").into(),
            );
        }
        Ok(())
    }
    measured(
        &raw["material_volume_mm3"],
        assembly.material_volume(),
        "material_volume_mm3",
    )?;
    bounds(&raw["bounds_mm"], assembly.bounds(), "bounds_mm")?;
    for (id, part) in &assembly.parts {
        let definition = &definitions[id];
        measured(
            &definition["material_volume_mm3"],
            part.result.model.volume(),
            &format!("definitions.{id}.material_volume_mm3"),
        )?;
        let expected = (!part.result.model.solids.is_empty()).then(|| part.result.model.bounds());
        bounds(
            &definition["local_bounds_mm"],
            expected,
            &format!("definitions.{id}.local_bounds_mm"),
        )?;
    }
    Ok(json!({
        "validation": {"valid": true, "scope": "individual definitions, rigid placements and measured metadata; interference and display proxy content not checked"},
        "instances": assembly.instance_count(), "definitions": assembly.parts.len(),
        "assembly_definitions": assembly.hierarchy.len(),
        "material_volume_mm3": assembly.material_volume(), "bounds_mm": assembly.bounds(),
        "bounds_kind": "conservative_axis_aligned", "volume_kind": "summed_instance_material_not_union",
    }))
}

// Convert an integrated mm³/mm⁵ quantity to kg/kg·mm². Reorder factors
// to avoid avoidable intermediate overflow; reject nonzero values rounded away.
fn density_scaled(value: f64, density: f64) -> Result<f64, Box<dyn Error>> {
    if value == 0.0 {
        return Ok(0.0);
    }
    for result in [
        (value * density) * 1e-9,
        (value * 1e-9) * density,
        value * (density * 1e-9),
    ] {
        if result.is_normal() {
            return Ok(result);
        }
    }
    Err("mass properties exceed normal floating-point range".into())
}

fn properties_report(
    recipe: &Recipe,
    build: &BuildResult,
    output: &str,
    density: Option<f64>,
    source: &[u8],
    parameters: &BTreeMap<String, f64>,
) -> Result<Value, Box<dyn Error>> {
    let tolerance = recipe.tolerance.unwrap_or_default();
    let (mut report, kind, feature_id, content_hash) = if let Some(body) = build.outputs.get(output)
    {
        let properties = body.result.model.mass_properties(tolerance)?;
        let mass = density
            .map(|density| density_scaled(properties.volume, density))
            .transpose()?;
        let mass_inertia = match (density, properties.central_inertia) {
            (Some(density), Some(tensor)) => {
                let mut result = [[0.; 3]; 3];
                for i in 0..3 {
                    for j in 0..3 {
                        result[i][j] = density_scaled(tensor[i][j], density)?;
                    }
                }
                Some(result)
            }
            _ => None,
        };
        (
            json!({
                "volume": properties.volume,
                "volume_centroid": properties.centroid,
                "central_volume_inertia": properties.central_inertia,
                "density_kg_per_m3": density,
                "mass_kg": mass,
                "center_of_mass": if density.is_some() { properties.centroid } else { None },
                "central_mass_inertia": mass_inertia,
                "mass_status": if density.is_some() { "complete" } else { "density_unspecified" },
                "volume_kind": "integrated_solid_material",
            }),
            "body",
            body.feature_id.as_str(),
            body.content_hash.as_str(),
        )
    } else if let Some(assembly) = build.assemblies.get(output) {
        if density.is_some() {
            return Err(
                "assembly density override is unsupported; set material density on each part"
                    .into(),
            );
        }
        let properties = assembly.mass_properties(tolerance)?;
        let mut report = serde_json::to_value(&properties)?;
        report["mass_status"] = json!(if properties.missing_density_occurrences == 0 {
            "complete"
        } else {
            "missing_part_density"
        });
        report["volume_kind"] = json!("summed_occurrence_material_not_union");
        (
            report,
            "assembly",
            assembly.assembly_id.as_str(),
            assembly.content_hash.as_str(),
        )
    } else {
        return Err(format!("unknown recipe output '{output}'").into());
    };
    report["schema_version"] = json!(1);
    report["kind"] = json!(kind);
    report["output_name"] = json!(output);
    report["coordinate_frame"] = json!(if kind == "assembly" {
        "root_assembly_frame"
    } else {
        "output_model_frame"
    });
    report["units"] = json!({"volume":"mm3", "volume_centroid":"mm", "central_volume_inertia":"mm5", "mass_kg":"kg", "center_of_mass":"mm", "central_mass_inertia":"kg*mm2", "density_kg_per_m3":"kg/m3"});
    report["inertia_convention"] = json!(
        "central symmetric tensor: integral ((r dot r) identity - r outer r); off-diagonals are negative product moments; r measured from the corresponding centroid"
    );
    report["snapshot"] = json!({
        "recipe_source_hash": blake3::hash(source).to_hex().to_string(),
        "parameter_overrides": parameters, "tolerance": tolerance,
        "feature_id": feature_id, "content_hash": content_hash,
        "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
        "executor_build_id": cad_build::executor_build_id(),
        "assembly_build_id": if kind == "assembly" { Some(cad_assembly::assembly_build_id()) } else { None },
    });
    report["stats"] = json!(build.stats);
    report["scope"] = json!(build.scope);
    Ok(report)
}

fn run(cli: Cli) -> Result<Value, Box<dyn Error>> {
    run_with_source(cli, None)
}

fn run_with_source(cli: Cli, source: Option<&source::Snapshot>) -> Result<Value, Box<dyn Error>> {
    if let Command::Verify {
        directory,
        manifest_hash,
    } = &cli.command
    {
        if !cli.params.is_empty() || cli.cache_dir.is_some() {
            return Err("verify does not accept --param or --cache-dir".into());
        }
        return Ok(integrity::verify(directory, manifest_hash.as_deref())?);
    }
    if let Command::Lock { recipe } = &cli.command {
        if !cli.params.is_empty() || cli.cache_dir.is_some() {
            return Err(
                "lock does not accept --param or --cache-dir; it parses imports without evaluation"
                    .into(),
            );
        }
        return source::lock_recipe(recipe);
    }
    let parameters = parameters(&cli.params)?;
    if !parameters.is_empty()
        && matches!(
            cli.command,
            Command::Init { .. }
                | Command::Validate { .. }
                | Command::Serve { .. }
                | Command::Capabilities { .. }
        )
    {
        return Err("parameter overrides apply only to recipe commands".into());
    }
    match &cli.command {
        Command::Capabilities {
            operation,
            object_type,
        } => {
            if cli.cache_dir.is_some() {
                return Err("capabilities does not use a geometry cache; omit --cache-dir".into());
            }
            let mut document = capabilities::report(operation.as_deref(), object_type.as_deref())?;
            document["cli"] = json!({
                "version": env!("CARGO_PKG_VERSION"),
                "authoring": {
                    "languages": ["starlark", "json_recipe_ir"],
                    "argument_catalog_representation": "json_recipe_ir",
                    "workflow": "Author a recipe, compile Starlark to the typed JSON IR, check geometry and obligations, review inspect/diff results, then build artifacts.",
                    "source_authority": "Git-versioned recipes and explicit parameter overrides",
                    "modules": {"scope":"local locked Starlark only; no network","pin_command":"cad lock recipe.star (explicit create/update, never implicit during build)","lockfile":"<recipe-filename>.lock.json","lock_schema":{"schema_version":1,"modules":{"lib/example.star":{"blake3":"64 lowercase hexadecimal characters"}}},"labels":"canonical project-root-relative .star paths; symlink escape rejected","maximum_modules":32,"maximum_starlark_root_bytes":1048576,"maximum_module_bytes":1048576,"maximum_replay_json_bytes":67108864,"maximum_bundle_source_bytes":8388608,"captured_artifact":"recipe.bundle.json","replay":"cad build recipe.bundle.json (supply original --param overrides)","parameter_overrides":"not stored in source bundle; replay using original explicit --param overrides from manifest recipe_source.parameter_overrides","identity":"source_bundle_hash binds root language/source and labeled module bytes; root recipe_source_hash retains root-only meaning"},
                    "viewer": "read_only_inspection",
                    "feasibility": "The catalog describes supported domains; only evaluation and validation can certify a particular input."
                },
                "artifact_verification": {
                    "command": "verify",
                    "evaluates_geometry": false,
                    "publishes_artifacts": false,
                    "manifest_schema_version": integrity::MANIFEST_SCHEMA_VERSION,
                    "algorithm": "blake3",
                    "optional_trusted_pin": "--manifest-hash",
                    "maximum_manifest_bytes": integrity::MAX_MANIFEST_BYTES,
                    "maximum_payload_files": integrity::MAX_PAYLOAD_FILES,
                    "directory_membership": "exact; no extra files or directories",
                    "trust": "Hashes verify integrity, not authenticity; pin the manifest hash through a trusted channel."
                },
                "recipe_commands": [
                    {"name":"lock", "publishes_artifacts":false,"writes_source_lock":true,"evaluates_geometry":false,"evaluates_recipe":false,"purpose":"Explicitly discover and pin transitive local imports to adjacent schema1 lock; no implicit build repins; invalid inputs preserve old lock.","parameters_and_cache":"not accepted","concurrent_edits":"best-effort captured-source and prior-lock recheck before atomic rename"},
                    {"name":"compile", "publishes_artifacts":false, "evaluates_geometry":false, "purpose":"Compile authoring syntax to recipe IR; this alone does not validate geometry."},
                    {"name":"check", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Evaluate all outputs and native geometry/engineering validation; exchange obligations remain pending until full publication."},
                    {"name":"inspect", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Report output geometry, provenance, scope and checks; optional --output selects the demanded closure."},
                    {"name":"select", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Resolve a source face role through feature history; deletion and ambiguity are explicit."},
                    {"name":"section", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Query analytic line/arc plane sections as source-bound JSON or SVG; assembly areas sum occurrences, not union. Topology-event and oblique curved intersections reject."},
                    {"name":"properties", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Measure a named output independently of display LOD; assemblies sum occurrences."},
                    {"name":"bom", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Report assembly component quantities and material properties."},
                    {"name":"diff", "publishes_artifacts":false, "evaluates_geometry":true, "purpose":"Compare two recipe revisions and their resolved results."},
                    {"name":"build", "publishes_artifacts":true, "evaluates_geometry":true, "purpose":"Atomically publish a validated generation with captured source and open exports."},
                    {"name":"watch", "publishes_artifacts":true, "evaluates_geometry":true, "purpose":"Long-lived recipe watcher; rebuild complete generations after source edits and preserve the last successful generation on failure."}
                ],
                "execution": {
                    "capabilities_has_side_effects": false,
                    "optional_geometry_cache": "Recipe evaluation commands can write --cache-dir; publishes_artifacts refers only to generation publication.",
                    "failure_channel": "runtime_errors_stderr_json",
                    "usage_errors": "CLI argument parsing errors use human-readable stderr",
                    "success_channel": "stdout_json",
                    "success_format_overrides": {"section --format svg":"stdout_svg_with_query_metadata"},
                    "section_build_id": section::implementation_id(),
                    "failure_exit_status": "nonzero",
                    "global_obligations_apply_to_selected_outputs": true
                }
            });
            return Ok(document);
        }
        Command::Bom { recipe, assembly } => {
            let snapshot = source::Snapshot::read(recipe)?;
            let source = snapshot.source.as_bytes();
            let recipe = snapshot.recipe(&parameters)?;
            if !recipe
                .assembly_outputs
                .iter()
                .any(|output| &output.name == assembly)
            {
                return Err(format!("unknown assembly output '{assembly}'").into());
            }
            let built =
                engine(&cli.cache_dir).build_outputs(&recipe, std::slice::from_ref(assembly))?;
            let model = built
                .assemblies
                .get(assembly)
                .ok_or_else(|| format!("unknown assembly output '{assembly}'"))?;
            let mut result =
                bom_document(&model.bill_of_materials(recipe.tolerance.unwrap_or_default())?)?;
            result["recipe_source_hash"] = json!(blake3::hash(source).to_hex().to_string());
            result["parameter_overrides"] = json!(parameters);
            result["stats"] = json!(built.stats);
            result["build_scope"] = json!(built.scope);
            result["property_assertions"] = json!(built.property_assertions);
            result["sketch_reports"] = json!(built.sketch_reports);
            result["mechanism_reports"] = json!(built.mechanism_reports);
            result["mechanism_bindings"] = json!(built.mechanism_bindings);
            result["mechanism_context_hash"] = json!(built.mechanism_context_hash);
            result["section_views"] = json!(built.section_views);
            result["exchange_requirements"] = exchange::pending(&recipe.exchange_requirements);
            result["exchange_policy_status"] = json!(if recipe.exchange_requirements.is_empty() {
                "not_requested"
            } else {
                "pending_publication"
            });
            snapshot.bind(&mut result);
            return Ok(result);
        }
        Command::Select {
            recipe,
            anchor,
            role,
            target,
            count,
            all,
        } => {
            let snapshot = source::Snapshot::read(recipe)?;
            let source_bytes = snapshot.source.as_bytes();
            let recipe = snapshot.recipe(&parameters)?;
            let build = engine(&cli.cache_dir).build(&recipe)?;
            let selector = cad_build::FaceSelector {
                anchor_feature: anchor.clone(),
                role: role.clone(),
                target_feature: target.clone(),
                cardinality: if *all {
                    cad_build::FaceCardinality::All
                } else {
                    cad_build::FaceCardinality::Exactly {
                        count: count.map(std::num::NonZeroUsize::get).unwrap_or(1),
                    }
                },
            };
            let resolution = build.resolve_faces(&selector);
            let resolved = matches!(resolution.status, cad_build::FaceResolutionStatus::Resolved);
            let mut report = serde_json::to_value(&resolution)?;
            report["stats"] = json!(build.stats);
            report["property_assertions"] = json!(build.property_assertions);
            report["sketch_reports"] = json!(build.sketch_reports);
            report["mechanism_reports"] = json!(build.mechanism_reports);
            report["mechanism_bindings"] = json!(build.mechanism_bindings);
            report["mechanism_context_hash"] = json!(build.mechanism_context_hash);
            report["section_views"] = json!(build.section_views);
            report["exchange_requirements"] = exchange::pending(&recipe.exchange_requirements);
            report["exchange_policy_status"] = json!(if recipe.exchange_requirements.is_empty() {
                "not_requested"
            } else {
                "pending_publication"
            });
            report["snapshot"] = json!({
                "recipe_source_hash": blake3::hash(source_bytes).to_hex().to_string(),
                "parameter_overrides": parameters,
                "tolerance": recipe.tolerance.unwrap_or_default(),
                "kernel_version": cad_kernel::KERNEL_VERSION,
                "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
                "anchor_content_hash": build.feature_hashes.get(anchor),
                "target_content_hash": build.feature_hashes.get(target),
            });
            snapshot.bind(&mut report);
            if !resolved {
                return Err(Box::new(UnresolvedSelection(report)));
            }
            return Ok(report);
        }
        Command::Watch {
            recipe,
            out_dir,
            interval_ms,
            display_only,
            outputs,
        } => {
            return watch::run(
                recipe,
                std::time::Duration::from_millis(*interval_ms),
                |bytes| {
                    run_with_source(
                        Cli {
                            command: Command::Build {
                                recipe: recipe.clone(),
                                out_dir: out_dir.clone(),
                                display_only: *display_only,
                                outputs: outputs.clone(),
                            },
                            params: cli.params.clone(),
                            cache_dir: cli.cache_dir.clone(),
                        },
                        Some(bytes),
                    )
                },
            );
        }
        Command::Serve {
            directory,
            host,
            port,
        } => {
            server::serve(directory, host, *port)?;
            return Ok(json!({"status":"stopped"}));
        }
        Command::Init { directory } => return initialize(directory),
        Command::Validate { model } => {
            let raw: Value = serde_json::from_str(&fs::read_to_string(model)?)?;
            if raw.get("kind").and_then(Value::as_str) == Some("assembly") {
                return validate_assembly_manifest(model, &raw);
            }
            let snapshot: cad_kernel::ModelSnapshot =
                serde_json::from_value(raw.get("model").cloned().unwrap_or(raw))?;
            let validation = snapshot.validate(cad_kernel::Tolerance::default());
            if !validation.valid {
                return Err(Box::new(InvalidBrep(validation)));
            }
            return Ok(
                json!({"validation":validation,"volume_mm3":snapshot.volume(),"bounds_mm":if snapshot.solids.is_empty() { Value::Null } else { json!(snapshot.bounds()) }}),
            );
        }
        Command::Diff { before, after } => {
            let before_source = source::Snapshot::read(before)?;
            let after_source = source::Snapshot::read(after)?;
            let a = before_source.recipe(&parameters)?;
            let b = after_source.recipe(&parameters)?;
            let mut engine = engine(&cli.cache_dir);
            let mut result = difference(
                &report(&a, &engine.build(&a)?)?,
                &report(&b, &engine.build(&b)?)?,
            );
            if before_source.bundle_hash().is_some() || after_source.bundle_hash().is_some() {
                result["source_bundles"] = json!({"before":before_source.bundle_hash(),"after":after_source.bundle_hash()});
            }
            return Ok(result);
        }
        _ => {}
    }
    let path = match &cli.command {
        Command::Compile { recipe }
        | Command::Inspect { recipe, .. }
        | Command::Check { recipe }
        | Command::Build { recipe, .. }
        | Command::Properties { recipe, .. }
        | Command::Section { recipe, .. } => recipe,
        _ => unreachable!(),
    };
    // One immutable byte snapshot feeds graph evaluation, provenance, and the
    // viewer. In watch mode these are the bytes that triggered this rebuild.
    let snapshot = match source {
        Some(snapshot) => snapshot.clone(),
        None => source::Snapshot::read(path)?,
    };
    let source_bytes = snapshot.source.as_bytes();
    let source_text = snapshot.source.as_str();
    let recipe = snapshot.recipe(&parameters)?;
    if matches!(cli.command, Command::Compile { .. }) {
        return Ok(serde_json::to_value(recipe)?);
    }
    if let Command::Properties {
        density_kg_per_m3: Some(density),
        ..
    } = &cli.command
    {
        if !density.is_finite() || *density <= 0.0 {
            return Err("density must be finite and positive in kg/m³".into());
        }
    }
    let selected = match &cli.command {
        Command::Build { outputs, .. } | Command::Inspect { outputs, .. }
            if !outputs.is_empty() =>
        {
            Some(outputs.as_slice())
        }
        Command::Properties { output, .. } | Command::Section { output, .. } => {
            Some(std::slice::from_ref(output))
        }
        _ => None,
    };
    let build = match selected {
        Some(outputs) => engine(&cli.cache_dir).build_outputs(&recipe, outputs)?,
        None => engine(&cli.cache_dir).build(&recipe)?,
    };
    if let Command::Properties {
        output,
        density_kg_per_m3,
        ..
    } = &cli.command
    {
        let mut result = properties_report(
            &recipe,
            &build,
            output,
            *density_kg_per_m3,
            source_bytes,
            &parameters,
        )?;
        snapshot.bind(&mut result);
        result["property_assertions"] = json!(build.property_assertions);
        result["sketch_reports"] = json!(build.sketch_reports);
        result["mechanism_reports"] = json!(build.mechanism_reports);
        result["mechanism_bindings"] = json!(build.mechanism_bindings);
        result["mechanism_context_hash"] = json!(build.mechanism_context_hash);
        result["section_views"] = json!(build.section_views);
        result["exchange_requirements"] = exchange::pending(&recipe.exchange_requirements);
        result["exchange_policy_status"] = json!(if recipe.exchange_requirements.is_empty() {
            "not_requested"
        } else {
            "pending_publication"
        });
        return Ok(result);
    }
    if let Command::Section {
        output,
        origin,
        normal,
        x_direction,
        format,
        ..
    } = &cli.command
    {
        let plane = cad_kernel::SectionPlane {
            origin: section::vector(origin)?,
            normal: section::vector(normal)?,
            x_direction: section::vector(x_direction)?,
        };
        let mut result =
            section::report(&recipe, &build, output, plane, source_bytes, &parameters)?;
        snapshot.bind(&mut result);
        if format == "svg" {
            result["svg"] = json!(section::svg(&result)?);
        }
        return Ok(result);
    }
    let mut summary = report(&recipe, &build)?;
    snapshot.bind(&mut summary);
    if let Command::Build {
        out_dir,
        display_only,
        ..
    } = &cli.command
    {
        summary["generation_mode"] = json!(if *display_only {
            "display_only"
        } else {
            "full"
        });
        // Do not replace directories containing the source recipe or cache.
        if out_dir.exists() {
            let destination = out_dir.canonicalize()?;
            if snapshot.paths.iter().any(|p| p.starts_with(&destination))
                || cli
                    .cache_dir
                    .as_ref()
                    .and_then(|p| p.canonicalize().ok())
                    .is_some_and(|p| p.starts_with(&destination))
            {
                return Err("output directory cannot contain the source recipe or cache".into());
            }
        }
        let mut artifacts = vec![(
            "recipe.json".to_owned(),
            serde_json::to_string_pretty(&recipe)?,
        )];
        let language = snapshot.language.as_str();
        let source_artifact = if language == "starlark" {
            "recipe.star"
        } else {
            "recipe.source.json"
        };
        let source_name = snapshot.source_name.clone();
        if snapshot.lock_source.is_some() {
            artifacts.push((
                "recipe.bundle.json".into(),
                serde_json::to_string_pretty(&snapshot)?,
            ));
            summary["recipe_bundle"] = json!({"artifact":"recipe.bundle.json","replay":"cad build recipe.bundle.json (supply original --param overrides)","source_bundle_hash":snapshot.bundle_hash()});
        }
        artifacts.push((source_artifact.to_owned(), source_text.to_owned()));
        summary["recipe_source_hash"] = json!(blake3::hash(source_bytes).to_hex().to_string());
        summary["recipe_source"] = json!({
            "name": source_name,
            "language": language,
            "artifact": source_artifact,
            "parameter_overrides": parameters,
        });
        summary["recipe_context_hash"] = json!(blake3::hash(&serde_json::to_vec(&json!({
            "source_hash": summary["recipe_source_hash"],
            "source_bundle_hash": snapshot.bundle_hash(),
            "source": summary["recipe_source"],
            "generation_mode": summary["generation_mode"],
            "scope": summary["scope"],
            "property_assertions": summary["property_assertions"],
            "exchange_requirements": recipe.exchange_requirements,
            "section_views": recipe.section_views,
            "section_build_id": section::implementation_id(),
            "sketch_reports": summary["sketch_reports"],
            "mechanism_context_hash": summary["mechanism_context_hash"],
            "solver_build_id": summary["solver_build_id"],
            "kernel_build_id": summary["kernel_build_id"],
            "executor_build_id": summary["executor_build_id"],
        }))?)
        .to_hex()
        .to_string());
        let property_assertions: BTreeMap<String, cad_preview::EngineeringCheck> =
            serde_json::from_value(summary["property_assertions"].clone())?;
        let mut mesh_cache = cad_mesh::GenerationMeshCache::default();
        let mut names = BTreeSet::new();
        for (name, output) in &build.outputs {
            filename(name)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(format!(
                    "output name '{name}' collides on a case-insensitive filesystem"
                )
                .into());
            }
            let tolerance = recipe.tolerance.unwrap_or_default();
            let model = &output.result.model;
            let artifact_start = artifacts.len();
            if !display_only {
                artifacts.push((
                    format!("{name}.brep.json"),
                    serde_json::to_string_pretty(&*output.result)?,
                ));
                artifacts.push((
                    format!("{name}.step"),
                    cad_export::to_step(model, tolerance)?,
                ));
                artifacts.push((
                    format!("{name}.stl"),
                    cad_export::to_stl_with_cache(model, tolerance, &mut mesh_cache)?,
                ));
                artifacts.push((
                    format!("{name}.exchange.json"),
                    exchange::report(
                        name,
                        &output.content_hash,
                        &summary,
                        &artifacts,
                        cad_export::ExchangeSource::Body(model),
                        tolerance,
                    )?,
                ));
                artifacts.push((
                    format!("{name}.preview.svg"),
                    cad_preview::to_svg_with_cache(
                        model,
                        recipe.tolerance.unwrap_or_default(),
                        &mut mesh_cache,
                    )?,
                ));
            }
            let context = cad_preview::RecipeContext {
                modules: snapshot.modules.clone(),
                source_bundle_hash: snapshot.bundle_hash(),
                language: language.to_owned(),
                source: source_text.to_owned(),
                source_name: source_name.clone(),
                output_name: name.clone(),
                feature_id: output.feature_id.clone(),
                parameters: parameters.clone(),
                property_assertions: property_assertions.clone(),
                content_hash: output.content_hash.clone(),
                recipe_context_hash: summary["recipe_context_hash"]
                    .as_str()
                    .expect("computed recipe context hash")
                    .to_owned(),
            };
            artifacts.push((
                format!("{name}.preview.html"),
                cad_preview::to_html_with_recipe_and_cache(
                    model,
                    &context,
                    recipe.tolerance.unwrap_or_default(),
                    &mut mesh_cache,
                )?,
            ));
            summary["outputs"][name]["available_files"] = json!(artifacts[artifact_start..]
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>());
        }
        let mut definition_assets = BTreeSet::new();
        let mut shared_display_assets = BTreeMap::<String, usize>::new();
        for (name, assembly) in &build.assemblies {
            filename(name)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(format!(
                    "output name '{name}' collides on a case-insensitive filesystem"
                )
                .into());
            }
            let artifact_start = artifacts.len();
            let mut dependencies = BTreeSet::new();
            if !display_only {
                let mut definitions = serde_json::Map::new();
                for (part_id, part) in &assembly.parts {
                    let asset = format!("definition-{}.brep.json", part.content_hash);
                    filename(&asset)?;
                    dependencies.insert(asset.clone());
                    if definition_assets.insert(asset.clone()) {
                        artifacts
                            .push((asset.clone(), serde_json::to_string_pretty(&*part.result)?));
                    }
                    definitions.insert(part_id.clone(), json!({
                    "feature_id": part.feature_id,
                    "content_hash": part.content_hash,
                    "metadata": part.metadata,
                    "datums": part.datums,
                    "geometry_asset": asset,
                    "local_bounds_mm": if part.result.model.solids.is_empty() { Value::Null } else { json!(part.result.model.bounds()) },
                    "material_volume_mm3": part.result.model.volume(),
                }));
                }
                let manifest = json!({
                    "schema_version": 1, "kind": "assembly", "units": "mm",
                    "assembly_build_id": cad_assembly::assembly_build_id(),
                    "assembly_id": assembly.assembly_id, "content_hash": assembly.content_hash,
                    "tolerance": recipe.tolerance.unwrap_or_default(),
                    "definitions": definitions, "instances": assembly.instances,
                    "hierarchy": assembly.hierarchy, "instance_paths": assembly.instance_paths,
                    "bounds_mm": assembly.bounds(), "bounds_kind": "conservative_axis_aligned",
                    "material_volume_mm3": assembly.material_volume(),
                    "volume_kind": "summed_instance_material_not_union",
                    "display_asset": format!("{name}.display.json"),
                });
                artifacts.push((
                    format!("{name}.assembly.json"),
                    serde_json::to_string_pretty(&manifest)?,
                ));
                let bom = assembly.bill_of_materials(recipe.tolerance.unwrap_or_default())?;
                artifacts.push((
                    format!("{name}.bom.json"),
                    serde_json::to_string_pretty(&bom_document(&bom)?)?,
                ));
                artifacts.push((format!("{name}.bom.csv"), cad_export::bom_to_csv(&bom)?));
            }
            artifacts.push((
                format!("{name}.display.json"),
                if *display_only {
                    cad_preview::to_assembly_gpu_scene_json_with_cache(
                        assembly,
                        recipe.tolerance.unwrap_or_default(),
                        &mut mesh_cache,
                    )?
                } else {
                    cad_preview::to_assembly_scene_json_with_cache(
                        assembly,
                        recipe.tolerance.unwrap_or_default(),
                        &mut mesh_cache,
                    )?
                },
            ));
            if !display_only {
                artifacts.push((
                    format!("{name}.step"),
                    cad_export::assembly_to_step(assembly, recipe.tolerance.unwrap_or_default())?,
                ));
                artifacts.push((
                    format!("{name}.stl"),
                    cad_export::assembly_to_stl_with_cache(
                        assembly,
                        recipe.tolerance.unwrap_or_default(),
                        &mut mesh_cache,
                    )?,
                ));
                artifacts.push((
                    format!("{name}.exchange.json"),
                    exchange::report(
                        name,
                        &assembly.content_hash,
                        &summary,
                        &artifacts,
                        cad_export::ExchangeSource::Assembly(assembly),
                        recipe.tolerance.unwrap_or_default(),
                    )?,
                ));
                artifacts.push((
                    format!("{name}.preview.svg"),
                    cad_preview::to_assembly_svg_with_cache(
                        assembly,
                        recipe.tolerance.unwrap_or_default(),
                        &mut mesh_cache,
                    )?,
                ));
            }
            let context = cad_preview::RecipeContext {
                modules: snapshot.modules.clone(),
                source_bundle_hash: snapshot.bundle_hash(),
                language: language.to_owned(),
                source: source_text.to_owned(),
                source_name: source_name.clone(),
                output_name: name.clone(),
                feature_id: assembly.assembly_id.clone(),
                parameters: parameters.clone(),
                property_assertions: property_assertions.clone(),
                content_hash: assembly.content_hash.clone(),
                recipe_context_hash: summary["recipe_context_hash"].as_str().unwrap().to_owned(),
            };
            if !display_only {
                artifacts.push((
                    format!("{name}.preview.html"),
                    cad_preview::to_assembly_html_with_cache(
                        assembly,
                        &context,
                        recipe.tolerance.unwrap_or_default(),
                        &mut mesh_cache,
                    )?,
                ));
            }
            artifacts.push((
                format!("{name}.gpu.html"),
                cad_preview::to_assembly_gpu_html_with_fallback_and_cache(
                    assembly,
                    &context,
                    !display_only,
                    recipe.tolerance.unwrap_or_default(),
                    &mut mesh_cache,
                )?,
            ));
            let bundle = cad_preview::to_assembly_stream_bundle_with_cache(
                assembly,
                &context,
                !display_only,
                recipe.tolerance.unwrap_or_default(),
                &mut mesh_cache,
            )?;
            artifacts.push((format!("{name}.stream.html"), bundle.html));
            artifacts.push((format!("{name}.stream.json"), bundle.scene_json));
            for asset in bundle.assets {
                filename(&asset.name)?;
                dependencies.insert(asset.name.clone());
                if let Some(&index) = shared_display_assets.get(&asset.name) {
                    if artifacts[index].1 != asset.content {
                        return Err("shared display mesh digest collision".into());
                    }
                } else {
                    shared_display_assets.insert(asset.name.clone(), artifacts.len());
                    artifacts.push((asset.name, asset.content));
                }
            }
            dependencies.extend(
                artifacts[artifact_start..]
                    .iter()
                    .map(|(name, _)| name.clone()),
            );
            summary["outputs"][name]["available_files"] = json!(dependencies);
        }
        summary["exchange_requirements"] =
            exchange::enforce(&recipe.exchange_requirements, &artifacts, *display_only)?;
        summary["exchange_policy_status"] = json!(if recipe.exchange_requirements.is_empty() {
            "not_requested"
        } else {
            "passed"
        });
        // Source-authored views are global native obligations, including when
        // their target is omitted from selected/display-only geometry exports.
        for (id, view) in &build.section_views {
            filename(&format!("section-{id}"))?;
            let mut view_report =
                section::bind_view_report(&recipe, &build, view, source_bytes, &parameters)?;
            snapshot.bind(&mut view_report);
            // Execution counters belong in the build manifest. Source-owned view
            // bytes must remain identical when the same build is served from cache.
            view_report
                .as_object_mut()
                .expect("section report")
                .remove("stats");
            // These are staged publication reports, not standalone native queries:
            // carry the same satisfied exchange receipts as their generation.
            view_report["exchange_requirements"] = summary["exchange_requirements"].clone();
            view_report["exchange_policy_status"] = summary["exchange_policy_status"].clone();
            view_report["publication"] = json!({
                "recipe_context_hash":summary["recipe_context_hash"],
                "recipe_source":summary["recipe_source"],
                "generation_mode":summary["generation_mode"],
                "fidelity":"native analytic inspection section; no manufacturing dimensions, tolerances or GD&T",
            });
            let json_text = serde_json::to_string_pretty(&view_report)?;
            let svg_text = section::svg(&view_report)?;
            let json_file = format!("section-{id}.json");
            let svg_file = format!("section-{id}.svg");
            let record = |file: &str, text: &str| json!({"file":file,"algorithm":"blake3","digest":blake3::hash(text.as_bytes()).to_hex().to_string(),"bytes":text.len()});
            summary["section_views"][id]["artifacts"] =
                json!({"json":record(&json_file,&json_text),"svg":record(&svg_file,&svg_text)});
            summary["section_views"][id]["snapshot"] = view_report["snapshot"].clone();
            summary["section_views"][id]["fidelity"] =
                view_report["publication"]["fidelity"].clone();
            artifacts.push((json_file, json_text));
            artifacts.push((svg_file, svg_text));
        }
        summary["mesh_cache"] = serde_json::to_value(mesh_cache.stats())?;
        let mut artifact_names = BTreeSet::new();
        for (name, _) in &artifacts {
            if !artifact_names.insert(name.to_ascii_lowercase()) {
                return Err(format!("generated artifact filename collision: '{name}'").into());
            }
        }
        let mut files: Vec<_> = artifacts.iter().map(|(name, _)| name.clone()).collect();
        files.push("manifest.json".into());
        summary["artifact_files"] = json!(files);
        summary["manifest_schema_version"] = json!(integrity::MANIFEST_SCHEMA_VERSION);
        summary["artifact_integrity"] = integrity::inventory(&artifacts)?;
        let manifest = serde_json::to_string_pretty(&summary)?;
        integrity::check_manifest_size(manifest.len() as u64)?;
        artifacts.push(("manifest.json".into(), manifest));
        publish(out_dir, &artifacts)?;
        summary["artifacts"] = json!(artifacts
            .iter()
            .map(|(name, _)| out_dir.join(name))
            .collect::<Vec<_>>());
    }
    Ok(summary)
}

fn main() {
    let cli = Cli::parse();
    let svg = matches!(&cli.command,Command::Section {format,..} if format=="svg");
    let section_input = if let Command::Section {
        output,
        origin,
        normal,
        x_direction,
        ..
    } = &cli.command
    {
        let vector = |s: &str| section::vector(s).map_or_else(|_| json!(s), |v| json!(v));
        Some((
            output.clone(),
            json!({"origin":vector(origin),"normal":vector(normal),"x_direction":vector(x_direction)}),
        ))
    } else {
        None
    };
    match run(cli) {
        Ok(value) if svg => print!("{}", value["svg"].as_str().expect("successful SVG section")),
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON value serializes")
        ),
        Err(error) => {
            let mut causes = Vec::new();
            let mut source = error.source();
            while let Some(cause) = source {
                causes.push(cause.to_string());
                source = cause.source();
            }
            let mut report =
                json!({ "error": error.to_string(), "code": "invalid_request", "causes": causes });
            if let Some(requirement) = error.downcast_ref::<exchange::RequirementFailure>() {
                report["code"] = json!("exchange_requirement_failed");
                report["requirement"] = json!(requirement.requirement);
                report["reason"] = json!(requirement.reason);
                report["observed_status"] = json!(requirement.observed_status);
                report["source_count"] = json!(requirement.source_count);
            } else if let Some(integrity) = error.downcast_ref::<integrity::IntegrityError>() {
                report["code"] = json!("artifact_integrity_failed");
                report["diagnostic"] = json!(integrity.to_string());
            } else if let Some(selection) = error.downcast_ref::<UnresolvedSelection>() {
                report["code"] = json!(format!(
                    "selection_{}",
                    selection.0["status"].as_str().unwrap_or("unresolved")
                ));
                report["selection"] = selection.0.clone();
            } else if let Some(validation) = error.downcast_ref::<InvalidBrep>() {
                report["code"] = json!("invalid_brep");
                report["validation"] = json!(validation.0);
            } else if let Some(build) = error.downcast_ref::<cad_build::BuildError>() {
                report["code"] = json!("invalid_graph");
                match build {
                    cad_build::BuildError::PolygonProfile { feature, message } => {
                        report["code"] = json!("invalid_polygon_profile");
                        report["feature"] = json!(feature);
                        report["diagnostic"] = json!(message);
                    }
                    cad_build::BuildError::Sketch { feature, .. } => {
                        report["code"] = json!("invalid_sketch");
                        report["feature"] = json!(feature);
                    }
                    cad_build::BuildError::MechanismFailed {
                        id,
                        message,
                        report: solver_report,
                    } => {
                        report["code"] = json!("mechanism_solve_failed");
                        report["mechanism"] = json!(id);
                        report["diagnostic"] = json!(message);
                        report["solver_report"] = json!(solver_report);
                    }
                    cad_build::BuildError::SketchSolveFailed {
                        feature,
                        report: solve,
                        content_hash,
                    } => {
                        report["code"] = json!("sketch_solve_failed");
                        report["feature"] = json!(feature);
                        report["sketch"] = json!(solve);
                        report["solver_build_id"] = json!(cad_build::solver_build_id());
                        report["target_content_hash"] = json!(content_hash);
                    }
                    cad_build::BuildError::SectionQuery { message } => {
                        report["code"] = json!("section_failed");
                        report["diagnostic"] = json!(message);
                    }
                    cad_build::BuildError::InvalidSectionView { id, message } => {
                        report["code"] = json!("invalid_section_view");
                        report["section_view_id"] = json!(id);
                        report["diagnostic"] = json!(message);
                    }
                    cad_build::BuildError::SectionViewFailed {
                        id,
                        output,
                        message,
                        actual_area,
                    } => {
                        report["code"] = json!("section_view_failed");
                        report["section_view_id"] = json!(id);
                        report["output_name"] = json!(output);
                        report["diagnostic"] = json!(message);
                        report["actual_area_mm2"] = json!(actual_area);
                    }
                    cad_build::BuildError::SectionDimensionFailed {
                        view,
                        id,
                        output,
                        message,
                        actual_value,
                    } => {
                        report["code"] = json!("section_dimension_failed");
                        report["section_view_id"] = json!(view);
                        report["dimension_id"] = json!(id);
                        report["output_name"] = json!(output);
                        report["diagnostic"] = json!(message);
                        report["actual_value_mm"] = json!(actual_value);
                    }
                    cad_build::BuildError::InvalidPropertyAssertion { id, .. } => {
                        report["code"] = json!("invalid_property_assertion");
                        report["assertion_id"] = json!(id);
                    }
                    cad_build::BuildError::PropertyAssertionFailed {
                        assertion_id,
                        report: assertion,
                    } => {
                        report["code"] = json!("property_assertion_failed");
                        report["assertion_id"] = json!(assertion_id);
                        report["assertion"] = json!(assertion);
                        report["snapshot"] = json!({
                            "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
                            "executor_build_id": cad_build::executor_build_id(),
                            "target_content_hash": assertion.target_content_hash,
                        });
                    }
                    cad_build::BuildError::FaceAssertionFailed {
                        assertion_id,
                        resolution,
                        anchor_content_hash,
                        target_content_hash,
                    } => {
                        report["code"] = json!("face_assertion_failed");
                        report["assertion_id"] = json!(assertion_id);
                        report["selection"] = json!(resolution);
                        report["snapshot"] = json!({
                            "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
                            "executor_build_id": cad_build::executor_build_id(),
                            "anchor_content_hash": anchor_content_hash,
                            "target_content_hash": target_content_hash,
                        });
                    }
                    cad_build::BuildError::FaceDatumFailed {
                        part,
                        name,
                        resolution,
                        anchor_content_hash,
                        target_content_hash,
                    } => {
                        report["code"] = json!("face_datum_failed");
                        report["part"] = json!(part);
                        report["datum"] = json!(name);
                        report["selection"] = json!(resolution);
                        report["snapshot"] = json!({
                            "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
                            "executor_build_id": cad_build::executor_build_id(),
                            "anchor_content_hash": anchor_content_hash,
                            "target_content_hash": target_content_hash,
                        });
                    }
                    cad_build::BuildError::FaceDatumFrame { part, name, .. } => {
                        report["code"] = json!("face_datum_frame_failed");
                        report["part"] = json!(part);
                        report["datum"] = json!(name);
                    }
                    cad_build::BuildError::InvalidFaceDatum { part, name, .. } => {
                        report["code"] = json!("invalid_face_datum");
                        report["part"] = json!(part);
                        report["datum"] = json!(name);
                    }
                    cad_build::BuildError::UnknownSelectedOutput(output) => {
                        report["code"] = json!("unknown_selected_output");
                        report["output"] = json!(output);
                    }
                    cad_build::BuildError::EmptyOutputSelection => {
                        report["code"] = json!("empty_output_selection");
                    }
                    cad_build::BuildError::Kernel { feature, message } => {
                        report["code"] = json!("kernel_operation_failed");
                        report["feature"] = json!(feature);
                        report["diagnostic"] = json!(message);
                        report["snapshot"] = json!({
                            "kernel_build_id": cad_kernel::KERNEL_BUILD_ID,
                            "executor_build_id": cad_build::executor_build_id(),
                        });
                    }
                    cad_build::BuildError::Dimension { feature, parameter } => {
                        report["feature"] = json!(feature);
                        report["parameter"] = json!(parameter);
                    }
                    _ => {}
                }
            } else if error.is::<cad_export::ExportError>() {
                report["code"] = json!("export_failed");
            } else if error.is::<cad_recipe::RecipeError>() {
                report["code"] = json!("recipe_failed");
            } else if error.is::<std::io::Error>() {
                report["code"] = json!("io_failed");
            } else if error.is::<serde_json::Error>() {
                report["code"] = json!("invalid_json");
            }
            if let Some((output, plane)) = section_input {
                report["output_name"] = json!(output);
                report["plane"] = plane;
                if report["code"] == "invalid_request" {
                    report["code"] = json!("section_failed");
                }
                report["diagnostic"] = json!(error.to_string());
            }
            eprintln!("{report}");
            std::process::exit(1);
        }
    }
}
