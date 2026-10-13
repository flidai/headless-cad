//! Independent acceptance of the published spatial metadata hierarchy.
//! This exercises the native publisher, not browser metadata residency.
use cad_assembly::{BuiltAssembly, BuiltPart, Instance, RigidTransform};
use cad_kernel::Tolerance;
use cad_preview::{to_assembly_stream_bundle, RecipeContext};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const COUNT: usize = 1025;
type Bounds = [[f64; 3]; 2];

fn placement(i: usize) -> (f64, [f64; 3]) {
    (
        ((i % 11) as f64 * 17. - 71.).to_radians(),
        [
            (i % 41) as f64 * 11. - 200.,
            (i / 41) as f64 * 13. - 130.,
            (i % 7) as f64 * 6. - 18.,
        ],
    )
}

// Analytic Z rotation of native box corners; does not call the assembly's
// transform, display bounds, or page producer to obtain expected geometry.
fn expected_bounds(i: usize) -> Bounds {
    let (angle, offset) = placement(i);
    let (s, c) = angle.sin_cos();
    let mut result = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for x in [0., 2.] {
        for y in [0., 3.] {
            for z in [0., 4.] {
                let p = [
                    c * x - s * y + offset[0],
                    s * x + c * y + offset[1],
                    z + offset[2],
                ];
                for (axis, value) in p.into_iter().enumerate() {
                    result[0][axis] = result[0][axis].min(value);
                    result[1][axis] = result[1][axis].max(value);
                }
            }
        }
    }
    result
}

fn fixture() -> (BuiltAssembly, RecipeContext) {
    let part = BuiltPart {
        datums: BTreeMap::from([("seat".into(), RigidTransform::default())]),
        metadata: Default::default(),
        feature_id: "stock".into(),
        content_hash: "shared-native-box".into(),
        result: Arc::new(cad_kernel::extrude_rectangle(2., 3., 4., Tolerance::default()).unwrap()),
    };
    let instances = (0..COUNT)
        .map(|i| {
            let (angle, translation) = placement(i);
            Instance {
                // Delimiters must survive as one path component, not be split.
                id: format!("station/{i}/\"part\""),
                part: "box".into(),
                attachment: None,
                transform: RigidTransform {
                    translation,
                    rotation: [0., 0., (angle / 2.).sin(), (angle / 2.).cos()],
                },
            }
        })
        .collect();
    let assembly = BuiltAssembly::new(
        "metadata_fixture".into(),
        BTreeMap::from([("box".into(), part)]),
        instances,
        Tolerance::default(),
    )
    .unwrap();
    let context = RecipeContext {
        language: "starlark".into(),
        source: "# captured metadata fixture source\n".into(),
        source_name: "metadata_fixture.star".into(),
        modules: Default::default(),
        source_bundle_hash: Some("pinned-source-bundle".into()),
        source_provenance: None,
        output_name: "fixture".into(),
        feature_id: "metadata_fixture".into(),
        parameters: BTreeMap::new(),
        content_hash: assembly.content_hash.clone(),
        recipe_context_hash: "pinned-recipe-context".into(),
        property_assertions: Default::default(),
    };
    (assembly, context)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_page(descriptor: &Value, assets: &BTreeMap<&str, &str>) -> Value {
    let url = descriptor["url"].as_str().unwrap();
    let bytes = assets[url].as_bytes();
    assert_eq!(
        descriptor["byte_length"].as_u64().unwrap() as usize,
        bytes.len()
    );
    assert!(bytes.len() <= 1024 * 1024);
    let hash = hex_digest(bytes);
    assert_eq!(descriptor["sha256"], hash);
    assert_eq!(url, format!("metadata-{hash}.display.json"));
    let page: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(page["schema_version"], 1);
    page
}

fn union(left: Bounds, right: Bounds) -> Bounds {
    std::array::from_fn(|side| {
        std::array::from_fn(|axis| {
            if side == 0 {
                left[side][axis].min(right[side][axis])
            } else {
                left[side][axis].max(right[side][axis])
            }
        })
    })
}

struct Audit<'a> {
    assets: BTreeMap<&'a str, &'a str>,
    meshes: BTreeMap<&'a str, &'a str>,
    assembly: &'a BuiltAssembly,
    seen_pages: BTreeSet<String>,
    seen_ordinals: BTreeSet<usize>,
    leaves: usize,
}
impl Audit<'_> {
    fn visit(&mut self, descriptor: &Value) -> (usize, Bounds) {
        assert!(self
            .seen_pages
            .insert(descriptor["url"].as_str().unwrap().to_owned()));
        let page = read_page(descriptor, &self.assets);
        let mut count = 0;
        let mut expected = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
        match page["kind"].as_str().unwrap() {
            "assembly_metadata_branch" => {
                let children = page["children"].as_array().unwrap();
                assert_eq!(children.len(), 2);
                for child in children {
                    let (child_count, child_bounds) = self.visit(child);
                    count += child_count;
                    expected = union(expected, child_bounds);
                    // Every child descriptor itself must be enclosed, padding included.
                    for axis in 0..3 {
                        assert!(
                            descriptor["bounds"][0][axis].as_f64().unwrap()
                                <= child["bounds"][0][axis].as_f64().unwrap()
                        );
                        assert!(
                            descriptor["bounds"][1][axis].as_f64().unwrap()
                                >= child["bounds"][1][axis].as_f64().unwrap()
                        );
                    }
                }
            }
            "assembly_metadata_leaf" => {
                self.leaves += 1;
                let entries = page["entries"].as_array().unwrap();
                assert!(!entries.is_empty() && entries.len() <= 128);
                for entry in entries {
                    let i = entry["index"].as_u64().unwrap() as usize;
                    assert!(i < COUNT && self.seen_ordinals.insert(i));
                    let instance = &self.assembly.instances[i];
                    assert_eq!(entry["instance"], serde_json::to_value(instance).unwrap());
                    assert_eq!(
                        entry["path"],
                        serde_json::to_value(&self.assembly.instance_paths[&instance.id]).unwrap()
                    );
                    assert_eq!(entry["path"], serde_json::json!([instance.id]));
                    let definition = &entry["definition"];
                    assert_eq!(definition["mesh"], "shared-native-box");
                    assert_eq!(definition["feature_id"], "stock");
                    assert!(definition["datums"].get("seat").is_some());
                    let mesh = &definition["mesh_asset"];
                    let payload = self.meshes[mesh["url"].as_str().unwrap()];
                    assert_eq!(mesh["sha256"], hex_digest(payload.as_bytes()));
                    assert_eq!(
                        mesh["bounds"],
                        serde_json::json!([[0., 0., 0.], [2., 3., 4.]])
                    );
                    assert_eq!(definition["mesh_lods"], serde_json::json!([mesh]));
                    let placement_index: usize =
                        instance.id.split('/').nth(1).unwrap().parse().unwrap();
                    expected = union(expected, expected_bounds(placement_index));
                    count += 1;
                }
            }
            other => panic!("unexpected metadata page kind: {other}"),
        }
        assert_eq!(descriptor["occurrences"].as_u64().unwrap() as usize, count);
        let actual: Bounds = serde_json::from_value(descriptor["bounds"].clone()).unwrap();
        for axis in 0..3 {
            assert!(
                actual[0][axis] <= expected[0][axis],
                "lower bound excludes native corners"
            );
            assert!(
                actual[1][axis] >= expected[1][axis],
                "upper bound excludes native corners"
            );
            for side in 0..2 {
                assert!(
                    (actual[side][axis] - expected[side][axis]).abs() < 1e-8,
                    "bounds are not spatially selective"
                );
            }
        }
        (count, expected)
    }
}

#[test]
fn metadata_hierarchy_preserves_all_native_occurrences_and_supports_sparse_reads() {
    let (assembly, recipe) = fixture();
    let bundle = to_assembly_stream_bundle(&assembly, &recipe, false).unwrap();
    let index: Value = serde_json::from_str(&bundle.metadata_index_json).unwrap();
    assert_eq!(index["schema_version"], 1);
    assert_eq!(index["kind"], "assembly_metadata_index");
    assert_eq!(index["assembly_id"], assembly.assembly_id);
    assert_eq!(index["content_hash"], assembly.content_hash);
    assert_eq!(index["recipe_context_hash"], recipe.recipe_context_hash);
    assert_eq!(index["source_bundle_hash"], "pinned-source-bundle");
    assert_eq!(index["total_occurrences"], COUNT);
    assert!(bundle.metadata_index_json.len() < 2048);
    for forbidden in ["instances", "instance_paths", "definitions", "entries"] {
        assert!(
            index.get(forbidden).is_none(),
            "root embeds expanded {forbidden}"
        );
    }
    let assets: BTreeMap<_, _> = bundle
        .metadata_assets
        .iter()
        .map(|a| (a.name.as_str(), a.content.as_str()))
        .collect();
    assert_eq!(assets.len(), bundle.metadata_assets.len());
    let mut audit = Audit {
        assets,
        meshes: bundle
            .assets
            .iter()
            .map(|a| (a.name.as_str(), a.content.as_str()))
            .collect(),
        assembly: &assembly,
        seen_pages: BTreeSet::new(),
        seen_ordinals: BTreeSet::new(),
        leaves: 0,
    };
    let (count, _) = audit.visit(&index["root"]);
    assert_eq!(count, COUNT);
    assert_eq!(audit.seen_ordinals, (0..COUNT).collect());
    assert_eq!(audit.seen_pages.len(), bundle.metadata_assets.len());
    assert!(audit.leaves > 1);

    // Simulate a client descending one chosen branch. No sibling page is read,
    // no complete ordinal/path directory is needed, and no mesh is opened.
    let mut descriptor = index["root"].clone();
    let mut reads = 0;
    loop {
        let page = read_page(&descriptor, &audit.assets);
        reads += 1;
        if page["kind"] == "assembly_metadata_leaf" {
            assert!(page["entries"].as_array().unwrap().len() <= 128);
            break;
        }
        descriptor = page["children"][0].clone();
    }
    assert!(reads <= 6 && reads < bundle.metadata_assets.len());
    // New native pages are additive: today's browser remains schema 2 and keeps
    // captured source until a separately tested dynamic-residency client exists.
    let legacy: Value = serde_json::from_str(&bundle.scene_json).unwrap();
    assert_eq!(legacy["schema_version"], 2);
    assert_eq!(legacy["instances"].as_array().unwrap().len(), COUNT);
    assert!(bundle.html.contains("captured metadata fixture source"));
    let repeated = to_assembly_stream_bundle(&assembly, &recipe, false).unwrap();
    assert_eq!(bundle.metadata_index_json, repeated.metadata_index_json);
    let payloads = |bundle: &cad_preview::StreamBundle| {
        bundle
            .metadata_assets
            .iter()
            .map(|asset| (asset.name.clone(), asset.content.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(payloads(&bundle), payloads(&repeated));
    let mut revised_source = recipe.clone();
    revised_source.source.push_str("# source-only revision\n");
    revised_source.recipe_context_hash = "changed-recipe-context".into();
    revised_source.source_bundle_hash = Some("changed-source-bundle".into());
    let revised = to_assembly_stream_bundle(&assembly, &revised_source, false).unwrap();
    assert_ne!(bundle.metadata_index_json, revised.metadata_index_json);
    assert_eq!(payloads(&bundle), payloads(&revised));
    let revised_index: Value = serde_json::from_str(&revised.metadata_index_json).unwrap();
    assert_eq!(index["root"], revised_index["root"]);
    assert_eq!(
        revised_index["recipe_context_hash"],
        "changed-recipe-context"
    );
    assert!(revised.html.contains("source-only revision"));
}
