const SOURCE: &str = include_str!("../../../examples/drive_module_retained.star");
#[test]
fn retained_drive_source_preserves_native_stops_and_replay() {
    let recipe = cad_recipe::from_starlark("drive_module_retained.star", SOURCE).unwrap();
    let encoded = serde_json::to_string(&recipe).unwrap();
    let replay: cad_build::Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&replay).unwrap());
    let built = cad_build::build(&replay).unwrap();
    assert_eq!(built.assemblies["drive_module_release"].instances.len(), 24);
}

fn recipe(params: &[(&str, f64)]) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "drive_module_retained.star",
        SOURCE,
        &params.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    )
    .unwrap()
}
#[test]
fn retained_drive_revisions_keep_independent_stop_geometry_and_local_cache_changes() {
    let mut engine = cad_build::BuildEngine::default();
    let first = engine.build(&recipe(&[])).unwrap();
    let assembly = &first.assemblies["drive_module_release"];
    assert_eq!(assembly.parts.len(), 11);
    assert_eq!(first.datum_dimensions.len(), 10);
    assert_eq!(first.clearance_checks.len(), 11);
    for (id, value) in [
        ("shaft_stop_span", 82.9),
        ("bush_outer_span", 80.),
        ("hub_stop_span", 19.6),
        ("hub_width", 14.),
    ] {
        let d = &first.datum_dimensions[id];
        assert!((d.value_mm - value).abs() < 1e-9);
        assert!(d.lower_bound_mm <= value && d.upper_bound_mm >= value);
    }
    for (id, low, high) in [
        ("shaft_axial_endplay", 0.13, 0.27),
        ("hub_axial_endplay", 0.115, 0.285),
    ] {
        let s = &first.tolerance_stacks[id];
        assert!(s.measured_lower_bound_mm < 0.2 && s.measured_upper_bound_mm > 0.2);
        assert!((s.lower_limit_mm - low).abs() < 1e-10);
        assert!((s.upper_limit_mm - high).abs() < 1e-10);
    }
    let pitch = engine.build(&recipe(&[("mounting_pitch", 75.)])).unwrap();
    assert_eq!(pitch.stats.computed_solids, 4);
    for id in [
        "sleeve_body",
        "disc_body",
        "washer_body",
        "ring_body",
        "key_body",
        "housing_body",
    ] {
        assert_eq!(pitch.feature_hashes[id], first.feature_hashes[id], "{id}");
    }
    for id in ["base_body", "shaft_stock", "seat_tool", "shaft_body"] {
        assert_ne!(pitch.feature_hashes[id], first.feature_hashes[id], "{id}");
    }
    let diameter = engine.build(&recipe(&[("shaft_diameter", 14.)])).unwrap();
    let changed: Vec<_> = assembly
        .parts
        .iter()
        .filter_map(|(id, p)| {
            (diameter.assemblies["drive_module_release"].parts[id].content_hash != p.content_hash)
                .then_some(id.as_str())
        })
        .collect();
    assert_eq!(
        changed,
        [
            "drive_disc",
            "housing",
            "retaining_ring",
            "shaft",
            "sleeve",
            "washer"
        ]
    );
    let restored = engine.build(&recipe(&[])).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
    assert_eq!(restored.feature_hashes, first.feature_hashes);
}
#[test]
fn retained_drive_global_endplay_and_clearance_gates_survive_selected_outputs_and_cache() {
    let mut engine = cad_build::BuildEngine::default();
    let source = recipe(&[]);
    let first = engine.build_outputs(&source, &["washer".into()]).unwrap();
    assert_eq!(first.datum_dimensions.len(), 10);
    assert_eq!(first.clearance_checks.len(), 11);
    assert!(matches!(
        engine.build_outputs(&recipe(&[("minimum_endplay", 0.3)]), &["washer".into()]),
        Err(cad_build::BuildError::ToleranceStackFailed { .. })
    ));
    assert!(matches!(
        engine.build_outputs(&recipe(&[("required_clearance", 0.3)]), &["washer".into()]),
        Err(cad_build::BuildError::ClearanceCheckFailed { .. })
    ));
    let restored = engine.build_outputs(&source, &["washer".into()]).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
    assert_eq!(restored.feature_hashes, first.feature_hashes);
}

#[test]
fn retained_drive_parameter_corners_keep_two_independent_native_endplay_stacks() {
    let mut engine = cad_build::BuildEngine::default();
    for diameter in [10., 12., 16.] {
        for pitch in [60., 75., 90.] {
            let built = engine
                .build(&recipe(&[
                    ("shaft_diameter", diameter),
                    ("mounting_pitch", pitch),
                ]))
                .unwrap();
            for id in ["shaft_axial_endplay", "hub_axial_endplay"] {
                let s = &built.tolerance_stacks[id];
                assert!(s.measured_lower_bound_mm < 0.2 && s.measured_upper_bound_mm > 0.2);
            }
            for (id, expected) in [
                ("left_support_running_gap", 0.1),
                ("right_support_running_gap", 0.15),
                ("disc_shaft_gap", 0.15),
                ("shaft_washer_bush_gap", 0.1),
            ] {
                let q = &built.clearance_checks[id];
                assert!(
                    q.lower_bound_mm < expected && q.upper_bound_mm > expected,
                    "D{diameter} p{pitch}: {id}"
                );
            }
        }
    }
}
