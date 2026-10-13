//! Immutable spatial occurrence metadata pages. Bounds describe display geometry.
//! The encoded page cap does not bound builder RAM or browser decoded memory.
use crate::{DisplayMeshAsset, PreviewError};
use cad_assembly::Instance;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_METADATA_LEAF_OCCURRENCES: usize = 128;
pub const MAX_METADATA_PAGE_BYTES: usize = 1024 * 1024;
type Bounds = [[f64; 3]; 2];
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataPageDescriptor {
    pub url: String,
    pub sha256: String,
    pub byte_length: usize,
    pub bounds: Bounds,
    pub occurrences: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssemblyMetadataIndex {
    pub schema_version: u32,
    pub kind: String,
    pub assembly_id: String,
    pub content_hash: String,
    pub total_occurrences: usize,
    pub recipe_context_hash: String,
    pub source_bundle_hash: Option<String>,
    pub root: Option<MetadataPageDescriptor>,
}
#[derive(Clone, Serialize)]
struct Entry {
    index: usize,
    empty: bool,
    instance: Instance,
    path: Vec<String>,
    definition: Value,
}
struct SpatialEntry {
    entry: Entry,
    bounds: Bounds,
}
fn invalid(message: &str) -> PreviewError {
    PreviewError::Assembly(message.into())
}
fn transformed_bounds(local: Bounds, instance: &Instance) -> Result<Bounds, PreviewError> {
    if local.iter().flatten().any(|x| !x.is_finite()) || (0..3).any(|i| local[0][i] > local[1][i]) {
        return Err(invalid("invalid metadata local display bounds"));
    }
    let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    // Use absolute source and translation magnitudes, not the possibly cancelled
    // output, to pad rigid-transform floating arithmetic outwards.
    let scale = local.iter().flatten().map(|x| x.abs()).fold(1., f64::max)
        + instance
            .transform
            .translation
            .iter()
            .map(|x| x.abs())
            .fold(0., f64::max);
    let padding = scale * (128. * f64::EPSILON);
    for bits in 0..8 {
        let p = instance
            .transform
            .point(std::array::from_fn(|i| local[(bits >> i) & 1][i]));
        for i in 0..3 {
            bounds[0][i] = bounds[0][i].min(p[i] - padding);
            bounds[1][i] = bounds[1][i].max(p[i] + padding);
        }
        if p.iter().any(|x| !x.is_finite()) {
            return Err(invalid("nonfinite transformed metadata bounds"));
        }
    }
    if bounds.iter().flatten().any(|x| !x.is_finite()) {
        return Err(invalid("nonfinite padded metadata bounds"));
    }
    Ok(bounds)
}
fn union(entries: &[SpatialEntry]) -> Bounds {
    let mut bounds = entries[0].bounds;
    for entry in &entries[1..] {
        for (value, other) in bounds[0].iter_mut().zip(entry.bounds[0]) {
            *value = value.min(other);
        }
        for (value, other) in bounds[1].iter_mut().zip(entry.bounds[1]) {
            *value = value.max(other);
        }
    }
    bounds
}
fn page(
    entries: &mut [SpatialEntry],
    assets: &mut BTreeMap<String, String>,
) -> Result<MetadataPageDescriptor, PreviewError> {
    let bounds = union(entries);
    let leaf = if entries.len() <= MAX_METADATA_LEAF_OCCURRENCES {
        Some(serde_json::to_string(
            &serde_json::json!({"schema_version":1,"kind":"assembly_metadata_leaf","entries":entries.iter().map(|e| &e.entry).collect::<Vec<_>>()}),
        )?)
    } else {
        None
    };
    let content = if leaf
        .as_ref()
        .is_some_and(|s| s.len() <= MAX_METADATA_PAGE_BYTES)
    {
        leaf.unwrap()
    } else {
        if entries.len() == 1 {
            return Err(invalid("encoded metadata page exceeds 1 MiB"));
        }
        let axis = (0..3)
            .max_by(|&a, &b| {
                // Half spans avoid overflow for finite bounds with opposite signs.
                let span = |i: usize| bounds[1][i] * 0.5 - bounds[0][i] * 0.5;
                span(a).total_cmp(&span(b)).then_with(|| b.cmp(&a))
            })
            .unwrap();
        entries.sort_by(|a, b| {
            let center = |e: &SpatialEntry| e.bounds[0][axis] * 0.5 + e.bounds[1][axis] * 0.5;
            center(a)
                .total_cmp(&center(b))
                .then(a.entry.index.cmp(&b.entry.index))
        });
        let middle = entries.len() / 2;
        let (left, right) = entries.split_at_mut(middle);
        let children = [page(left, assets)?, page(right, assets)?];
        serde_json::to_string(
            &serde_json::json!({"schema_version":1,"kind":"assembly_metadata_branch","children":children}),
        )?
    };
    if content.len() > MAX_METADATA_PAGE_BYTES {
        return Err(invalid("encoded metadata page exceeds 1 MiB"));
    }
    let sha256: String = Sha256::digest(content.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let url = format!("metadata-{sha256}.display.json");
    let descriptor = MetadataPageDescriptor {
        url: url.clone(),
        sha256,
        byte_length: content.len(),
        bounds,
        occurrences: entries.len(),
    };
    if let Some(previous) = assets.insert(url, content.clone()) {
        if previous != content {
            return Err(invalid("metadata SHA-256 collision"));
        }
    }
    Ok(descriptor)
}
pub(crate) fn build(
    assembly_id: &str,
    recipe: &crate::RecipeContext,
    instances: &[Instance],
    paths: &BTreeMap<String, Vec<String>>,
    definitions: &BTreeMap<String, Value>,
    mesh_bounds: &BTreeMap<String, Option<Bounds>>,
) -> Result<(String, Vec<DisplayMeshAsset>), PreviewError> {
    let mut entries = Vec::with_capacity(instances.len());
    for (index, instance) in instances.iter().enumerate() {
        let definition = definitions
            .get(&instance.part)
            .ok_or_else(|| invalid("missing metadata definition"))?;
        let mesh = definition
            .get("mesh")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("missing metadata mesh identity"))?;
        let bounds = mesh_bounds
            .get(mesh)
            .copied()
            .ok_or_else(|| invalid("missing occurrence display bounds"))?;
        entries.push(SpatialEntry {
            bounds: transformed_bounds(bounds.unwrap_or([[0.; 3]; 2]), instance)?,
            entry: Entry {
                index,
                empty: bounds.is_none(),
                instance: instance.clone(),
                path: paths
                    .get(&instance.id)
                    .ok_or_else(|| invalid("missing exact occurrence path"))?
                    .clone(),
                definition: definition.clone(),
            },
        });
    }
    let mut assets = BTreeMap::new();
    let root = if entries.is_empty() {
        None
    } else {
        Some(page(&mut entries, &mut assets)?)
    };
    let index = AssemblyMetadataIndex {
        schema_version: 1,
        kind: "assembly_metadata_index".into(),
        assembly_id: assembly_id.into(),
        content_hash: recipe.content_hash.clone(),
        recipe_context_hash: recipe.recipe_context_hash.clone(),
        source_bundle_hash: recipe.source_bundle_hash.clone(),
        total_occurrences: entries.len(),
        root,
    };
    Ok((
        serde_json::to_string_pretty(&index)?,
        assets
            .into_iter()
            .map(|(name, content)| DisplayMeshAsset { name, content })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_index_and_empty_occurrence_preserve_identity() {
        let recipe = crate::RecipeContext {
            language: "starlark".into(),
            source: "# empty".into(),
            source_name: "empty.star".into(),
            modules: BTreeMap::new(),
            source_bundle_hash: Some("bundle".into()),
            source_provenance: None,
            output_name: "empty".into(),
            feature_id: "root".into(),
            parameters: BTreeMap::new(),
            content_hash: "assembly-hash".into(),
            recipe_context_hash: "context".into(),
            property_assertions: BTreeMap::new(),
            native_engineering_checks: Default::default(),
        };
        let (index, assets) = build(
            "root",
            &recipe,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let index: AssemblyMetadataIndex = serde_json::from_str(&index).unwrap();
        assert_eq!(index.total_occurrences, 0);
        assert!(index.root.is_none());
        assert!(assets.is_empty());
        assert_eq!(index.content_hash, recipe.content_hash);
        assert_eq!(index.recipe_context_hash, recipe.recipe_context_hash);
        assert_eq!(index.source_bundle_hash, recipe.source_bundle_hash);

        let instance = Instance {
            attachment: None,
            id: "nested-empty".into(),
            part: "empty-part".into(),
            transform: cad_assembly::RigidTransform {
                translation: [4., -7., 12.],
                ..Default::default()
            },
        };
        let path = vec!["nested".to_owned(), "empty".to_owned()];
        let definition = serde_json::json!({"mesh":"empty-mesh", "feature_id":"empty-feature", "metadata":{}, "datums":{}});
        let (index, assets) = build(
            "root",
            &recipe,
            std::slice::from_ref(&instance),
            &BTreeMap::from([(instance.id.clone(), path.clone())]),
            &BTreeMap::from([(instance.part.clone(), definition.clone())]),
            &BTreeMap::from([("empty-mesh".into(), None)]),
        )
        .unwrap();
        let index: AssemblyMetadataIndex = serde_json::from_str(&index).unwrap();
        assert_eq!(index.total_occurrences, 1);
        let root = index.root.unwrap();
        assert_eq!(root.occurrences, 1);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, root.url);
        let page: Value = serde_json::from_str(&assets[0].content).unwrap();
        let entry = &page["entries"][0];
        assert_eq!(entry["empty"], true);
        assert_eq!(entry["index"], 0);
        assert_eq!(entry["instance"], serde_json::to_value(&instance).unwrap());
        assert_eq!(entry["path"], serde_json::to_value(path).unwrap());
        assert_eq!(entry["definition"], definition);
        for (axis, coordinate) in instance.transform.translation.iter().enumerate() {
            assert!(root.bounds[0][axis] < *coordinate);
            assert!(root.bounds[1][axis] > *coordinate);
        }
    }
    #[test]
    fn rejects_nonfinite_transforms_and_bounds() {
        let mut instance = Instance {
            attachment: None,
            id: "a".into(),
            part: "p".into(),
            transform: Default::default(),
        };
        assert!(transformed_bounds([[0.; 3], [f64::INFINITY; 3]], &instance).is_err());
        instance.transform.translation = [f64::MAX; 3];
        assert!(transformed_bounds([[0.; 3], [1.; 3]], &instance).is_err());
        instance.transform.translation = [f64::NAN; 3];
        assert!(transformed_bounds([[0.; 3], [1.; 3]], &instance).is_err());
    }
    #[test]
    fn encoded_page_limit_is_enforced() {
        let mut entries = vec![SpatialEntry {
            bounds: [[0.; 3], [1.; 3]],
            entry: Entry {
                index: 0,
                empty: false,
                instance: Instance {
                    attachment: None,
                    id: "a".into(),
                    part: "p".into(),
                    transform: Default::default(),
                },
                path: vec!["a".into()],
                definition: serde_json::json!({"description":"x".repeat(MAX_METADATA_PAGE_BYTES)}),
            },
        }];
        assert!(page(&mut entries, &mut BTreeMap::new())
            .unwrap_err()
            .to_string()
            .contains("1 MiB"));
        entries[0].entry.definition =
            serde_json::json!({"description":"x".repeat(MAX_METADATA_PAGE_BYTES / 2)});
        let mut second = entries[0].entry.clone();
        second.index = 1;
        entries.push(SpatialEntry {
            bounds: [[0.; 3], [1.; 3]],
            entry: second,
        });
        let mut assets = BTreeMap::new();
        let root = page(&mut entries, &mut assets).unwrap();
        assert_eq!(assets.len(), 3);
        assert!(assets
            .values()
            .all(|page| page.len() <= MAX_METADATA_PAGE_BYTES));
        let branch: Value = serde_json::from_str(&assets[&root.url]).unwrap();
        assert_eq!(branch["children"].as_array().unwrap().len(), 2);
    }
}
