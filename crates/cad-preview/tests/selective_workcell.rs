//! Selective-regeneration evidence from shared frames and independently sized tools.
use std::{collections::BTreeMap, path::Path};

fn recipe(parameters: BTreeMap<String, f64>) -> cad_build::Recipe {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/workcell_line.star"),
    )
    .unwrap();
    cad_recipe::from_starlark_with_params("workcell_line.star", &source, &parameters).unwrap()
}

#[test]
fn service_station_evaluates_shared_frames_and_its_tool_only_across_edits() {
    let requested = vec!["service_station".to_owned()];
    let mut engine = cad_build::BuildEngine::default();
    let first = engine
        .build_outputs(&recipe(BTreeMap::new()), &requested)
        .unwrap();
    assert_eq!(first.stats.computed_solids, 4);
    assert_eq!(first.feature_hashes.len(), 8); // Four profiles and four solids.
    assert_eq!(first.assemblies.len(), 1);
    let station = &first.assemblies["service_station"];
    assert_eq!(station.instances.len(), 5); // The two posts share one definition.
    assert_eq!(station.parts.len(), 4);
    // Base + two posts + beam + right-triangle tool, independent dimensions.
    let expected =
        100.0 * 60.0 * 4.0 + 2.0 * 8.0 * 10.0 * 32.0 + 76.0 * 10.0 * 6.0 + 12.0 * 8.0 * 6.0 / 2.0;
    assert!((station.material_volume() - expected).abs() < 1e-7);

    let remote_edit = recipe(BTreeMap::from([
        ("edited_variant".into(), 1.0),
        ("tool_rise_extra".into(), 4.0),
    ]));
    let unchanged = engine.build_outputs(&remote_edit, &requested).unwrap();
    assert_eq!(unchanged.stats.computed_solids, 0);
    assert_eq!(unchanged.stats.cache_hits, 4);
    assert_eq!(unchanged.feature_hashes, first.feature_hashes);

    let local_edit = recipe(BTreeMap::from([("tool_rise_extra".into(), 4.0)]));
    let changed = engine.build_outputs(&local_edit, &requested).unwrap();
    assert_eq!(changed.stats.computed_solids, 1);
    assert_eq!(changed.stats.cache_hits, 3);
    assert!(
        (changed.assemblies["service_station"].material_volume() - (expected + 144.0)).abs() < 1e-7
    );

    let complete = cad_build::build(&recipe(BTreeMap::new())).unwrap();
    assert_eq!(complete.stats.computed_solids, 15);
    assert_eq!(complete.assemblies["workcell_line"].instances.len(), 60);
}

#[test]
fn thousand_station_catalog_does_not_evaluate_unrequested_unique_tools() {
    let large = recipe(BTreeMap::from([
        ("stations".into(), 1000.0),
        ("columns".into(), 25.0),
        ("variants".into(), 40.0),
    ]));
    let result = cad_build::BuildEngine::default()
        .build_outputs(&large, &["service_station".into()])
        .unwrap();
    assert_eq!(large.features.len(), 86);
    assert_eq!(result.feature_hashes.len(), 8);
    assert_eq!(result.stats.computed_solids, 4);
    assert_eq!(result.assemblies["service_station"].instances.len(), 5);
}
