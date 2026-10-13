use cad_build::{BuildEngine, BuildError, Recipe};
use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../examples/seated_pocket_inserts.star");
const MODULE: &str = include_str!("../../../examples/mixed_slot_fixture.star");
fn recipe(parameters: &[(&str, f64)]) -> Recipe {
    cad_recipe::from_starlark_with_modules(
        "seated_pocket_inserts.star",
        SOURCE,
        &parameters.iter().map(|(k, v)| ((*k).into(), *v)).collect(),
        &BTreeMap::from([("mixed_slot_fixture.star".into(), MODULE.into())]),
    )
    .unwrap()
}

#[test]
fn inserts_follow_actual_serial_floors_on_both_caps_and_keep_proud_height() {
    let mut engine = BuildEngine::default();
    for (side, first, second, clearance) in [
        (1., 3., 7., 0.2),
        (0., 2., 9., 0.75),
        (1., 5., 6., 0.2),
        (0., 5., 6., 0.2),
    ] {
        let source = recipe(&[
            ("pocket_side", side),
            ("first_depth", first),
            ("second_depth", second),
            ("insert_clearance", clearance),
        ]);
        let built = engine.build(&source).unwrap();
        assert_eq!(built.assemblies["seated_pocket_inserts"].instances.len(), 3);
        assert_eq!(built.nominal_seating_checks.len(), 2);
        assert_eq!(built.pocket_fit_checks.len(), 2);
        assert_eq!(built.datum_dimensions.len(), 5);
        let lineage = &built.face_assertions["first_floor_survives"].witnesses[0];
        assert_eq!(lineage.target_face.0, lineage.anchor_faces[0].0 + 4);
        for (name, length, radius, depth) in [
            ("first_insert", 20., 5. - clearance, first),
            ("second_insert", 22., 6. - clearance, second),
        ] {
            let expected =
                (2. * radius * length + std::f64::consts::PI * radius * radius) * (depth + 2.);
            assert!((built.outputs[name].result.model.volume() - expected).abs() < 1e-7);
            let seat = &built.nominal_seating_checks[&format!("{name}_floor_seat")];
            assert_eq!(seat.status, "passed");
            assert_eq!(seat.right.instance_path, ["fixture"]);
            assert_eq!(
                seat.right.part_content_hash,
                built.feature_hashes["relief_pocket"]
            );
            assert!(seat.disk_radius_lower_mm >= 1.);
            if name == "first_insert" {
                assert_eq!(seat.query.right_face, lineage.target_face);
            }
            assert!(
                (built.datum_dimensions[&format!("{name}_proud_height")].value_mm - 2.).abs()
                    < 1e-8
            );
        }
        assert_eq!(engine.build(&source).unwrap().stats.computed_solids, 0);
    }
}

#[test]
fn selected_warm_build_retains_floor_seating_and_protrusion_obligations() {
    let mut engine = BuildEngine::default();
    let source = recipe(&[]);
    let selected = ["through_stage".into()];
    engine.build_outputs(&source, &selected).unwrap();
    for parameters in [
        vec![("insert_lift", 0.1)],
        vec![("required_seat_radius", 7.)],
    ] {
        let mut candidate = recipe(&parameters);
        // Isolate the contact obligation: the authored proud-height bounds also
        // reject a lifted insert, but must not mask a missing seating check.
        for dimension in &mut candidate.datum_dimensions {
            if dimension.id.ends_with("_proud_height") {
                dimension.min = Some(0.);
                dimension.max = Some(4.);
            }
        }
        assert!(matches!(
            engine.build_outputs(&candidate, &selected),
            Err(BuildError::NominalSeatingCheckFailed { .. })
        ));
    }
    let mut bad_height = source.clone();
    let dimension = bad_height
        .datum_dimensions
        .iter_mut()
        .find(|d| d.id == "first_insert_proud_height")
        .unwrap();
    dimension.min = Some(3.);
    dimension.max = Some(4.);
    assert!(matches!(
        engine.build_outputs(&bad_height, &selected),
        Err(BuildError::DatumDimensionFailed { .. })
    ));
    let restored = engine.build_outputs(&source, &selected).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
    assert_eq!(restored.nominal_seating_checks.len(), 2);
}

#[test]
fn whole_insert_fit_rejects_sideways_collisions_that_keep_floor_seating() {
    let mut engine = BuildEngine::default();
    let selected = ["through_stage".into()];
    for side in [0., 1.] {
        for (shift, expected) in [(0., 0.2), (0.1, 0.1), (0.159, 0.041), (-0.1, 0.1)] {
            let source = recipe(&[("pocket_side", side), ("first_insert_shift", shift)]);
            let built = engine.build_outputs(&source, &selected).unwrap();
            let fit = &built.pocket_fit_checks["first_insert_pocket_fit"];
            assert_eq!(fit.status, "passed");
            assert!(fit.side_clearance_lower_mm <= expected);
            assert!(fit.side_clearance_upper_mm >= expected);
            assert!(fit.side_clearance_upper_mm - fit.side_clearance_lower_mm < 1e-6);
            assert!(fit.axial_overlap_lower_mm > 2.99);
            assert!(fit.signed_floor_gap_lower_mm <= 0.);
            assert!(fit.signed_floor_gap_upper_mm >= 0.);
            let seat = &built.nominal_seating_checks["first_insert_floor_seat"];
            assert_eq!(fit.query.pocket_floor_face, seat.query.right_face);
            assert_eq!(fit.query.insert_floor_face, seat.query.left_face);
            assert_eq!(fit.pocket.part_content_hash, seat.right.part_content_hash);
            assert_eq!(
                engine
                    .build_outputs(&source, &selected)
                    .unwrap()
                    .stats
                    .computed_solids,
                0
            );
        }
        for shift in [0.161, 0.3, -0.3] {
            let mut candidate = recipe(&[("pocket_side", side), ("first_insert_shift", shift)]);
            assert!(matches!(
                engine.build_outputs(&candidate, &selected),
                Err(BuildError::PocketFitCheckFailed { .. })
            ));
            // Positive control proves floor seating and proud-height checks cannot
            // substitute for the full-footprint fit obligation.
            candidate.pocket_fit_checks.clear();
            let control = engine.build_outputs(&candidate, &selected).unwrap();
            assert_eq!(control.nominal_seating_checks.len(), 2);
        }
    }
}

#[test]
fn pocket_fit_schema_and_parameters_are_strict() {
    let source = recipe(&[]);
    let mut value = serde_json::to_value(&source).unwrap();
    value["pocket_fit_checks"][0]["unexpected"] = true.into();
    assert!(serde_json::from_value::<Recipe>(value).is_err());
    let mut value = serde_json::to_value(&source).unwrap();
    value["schema_version"] = 1.into();
    assert!(serde_json::from_value::<Recipe>(value).is_err());
    for minimum in [0., -1., f64::NAN, f64::INFINITY] {
        let mut candidate = source.clone();
        candidate.pocket_fit_checks[0].min_side_clearance_mm = minimum;
        assert!(matches!(
            BuildEngine::default().build(&candidate),
            Err(BuildError::InvalidPocketFitCheck { .. })
        ));
    }
}

#[test]
fn pocket_fit_preserves_schema_one_roundtrip_and_rejects_null() {
    let old: Recipe = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "units": "mm", "features": [], "outputs": []
    }))
    .unwrap();
    let value = serde_json::to_value(&old).unwrap();
    assert!(value.get("pocket_fit_checks").is_none());
    let _: Recipe = serde_json::from_value(value.clone()).unwrap();
    for schema in [1, 2] {
        for empty in [serde_json::Value::Null, serde_json::json!([])] {
            let mut candidate = value.clone();
            candidate["schema_version"] = schema.into();
            candidate["pocket_fit_checks"] = empty.clone();
            assert_eq!(
                serde_json::from_value::<Recipe>(candidate).is_ok(),
                schema == 2 && empty.is_array()
            );
        }
    }
}
