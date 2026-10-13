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
