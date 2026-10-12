//! Independent placement matrices and section formulas for datum-authored views.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/datum_section_fixture.star");
fn recipe(height: f64, depth: f64, spacing: f64, pose: usize, shift: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "datum_section_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("height".into(), height),
            ("depth".into(), depth),
            ("spacing".into(), spacing),
            ("pose".into(), pose as f64),
            ("shift".into(), shift),
        ]),
    )
    .unwrap()
}
fn rotation(pose: usize, p: [f64; 3]) -> [f64; 3] {
    // Closed-form matrices; intentionally independent of production quaternion helpers.
    match pose {
        0 => p,
        1 => [p[0], -p[2], p[1]],
        2 => {
            let q = std::f64::consts::FRAC_1_SQRT_2;
            [
                (p[0] + p[1]) / 2. + q * p[2],
                (p[0] + p[1]) / 2. - q * p[2],
                q * (p[1] - p[0]),
            ]
        }
        _ => unreachable!(),
    }
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-8, "{actual} != {expected}");
}
fn vector(actual: [f64; 3], expected: [f64; 3]) {
    for i in 0..3 {
        close(actual[i], expected[i]);
    }
}
#[test]
fn twenty_seven_configurations_preserve_associative_section_geometry_and_world_frames() {
    let mut engine = cad_build::BuildEngine::default();
    for height in [12., 15., 18.] {
        for (index, depth) in [2., 3., 4.].into_iter().enumerate() {
            let spacing = [80., 90., 100.][index];
            for pose in 0..3 {
                let shift = 25.;
                let recipe = recipe(height, depth, spacing, pose, shift);
                let build = engine.build(&recipe).unwrap();
                assert_eq!(build.section_views.len(), 2);
                let expected_area = 2.
                    * (2400. - std::f64::consts::PI * (3. + depth / 2.).powi(2)
                        + std::f64::consts::PI * 2.8_f64.powi(2));
                for (id, station, offset, centroid_x) in [
                    ("left_seat", "left", 0., spacing / 2.),
                    ("right_seat", "right", spacing, -spacing / 2.),
                ] {
                    let report = &build.section_views[id];
                    assert_eq!(report.status, "passed");
                    close(report.query.section.area, expected_area);
                    let centroid = report.query.section.centroid.unwrap();
                    close(centroid[0], centroid_x);
                    close(centroid[1], 0.);
                    assert!(report.query.section.loops.is_none());
                    assert_eq!(report.query.occurrences.len(), 6);
                    let witness = report.anchor_witness.as_ref().unwrap();
                    assert_eq!(witness.instance_path, vec!["carrier", station, "housing"]);
                    assert_eq!(witness.part_id, "housing");
                    assert_eq!(witness.datum, "top");
                    assert_eq!(report.declaration.origin, [0., 0., -depth / 2.]);
                    let native = &build.assemblies["datum_section_fixture"];
                    assert_eq!(
                        native.instance_paths[&witness.instance_id],
                        witness.instance_path
                    );
                    assert_eq!(
                        witness.part_content_hash,
                        build.outputs["housing"].content_hash
                    );
                    let semantic = witness.semantic_datum.as_ref().unwrap();
                    assert_eq!(
                        semantic.selection.status,
                        cad_build::FaceResolutionStatus::Resolved
                    );
                    assert_eq!(semantic.definition.part, "housing");
                    assert_eq!(semantic.definition.anchor_feature, "stock");
                    assert_eq!(semantic.definition.role, "end_cap");
                    assert_eq!(semantic.target_content_hash, witness.part_content_hash);
                    assert_eq!(semantic.frame, witness.datum_frame);
                    vector(witness.datum_frame.translation, [30., 20., height]);
                    let translation = [shift, -12., 7.];
                    let expected = rotation(pose, [30. + offset, 20., height - depth / 2.]);
                    vector(
                        witness.resolved_plane.origin,
                        std::array::from_fn(|i| expected[i] + translation[i]),
                    );
                    vector(witness.resolved_plane.normal, rotation(pose, [0., 0., 1.]));
                    vector(
                        witness.resolved_plane.x_direction,
                        rotation(pose, [1., 0., 0.]),
                    );
                    assert_eq!(report.query.section.plane, witness.resolved_plane);
                    let full_world = rotation(pose, [30. + offset, 20., height]);
                    vector(
                        witness.world_frame.translation,
                        std::array::from_fn(|i| full_world[i] + translation[i]),
                    );
                    let occurrence = rotation(pose, [offset, 0., 0.]);
                    vector(
                        witness.occurrence_frame.translation,
                        std::array::from_fn(|i| occurrence[i] + translation[i]),
                    );
                    let sections = &report.query.occurrences;
                    assert_eq!(sections.iter().filter(|s| s.section.area > 0.).count(), 4);
                    assert!(sections
                        .iter()
                        .filter(|s| s.part_id == "foot")
                        .all(|s| s.section.loops.is_empty()));
                }
            }
        }
    }
}
#[test]
fn placement_only_shift_reuses_geometry_and_moves_both_anchor_witnesses() {
    let mut engine = cad_build::BuildEngine::default();
    let first = engine.build(&recipe(15., 3., 90., 2, 25.)).unwrap();
    let moved = engine.build(&recipe(15., 3., 90., 2, 32.)).unwrap();
    assert_eq!(moved.stats.computed_solids, 0);
    assert_eq!(first.feature_hashes, moved.feature_hashes);
    assert_ne!(
        first.assemblies["datum_section_fixture"].content_hash,
        moved.assemblies["datum_section_fixture"].content_hash
    );
    for id in ["left_seat", "right_seat"] {
        let a = &first.section_views[id];
        let b = &moved.section_views[id];
        close(a.query.section.area, b.query.section.area);
        let old = a.anchor_witness.as_ref().unwrap();
        let new = b.anchor_witness.as_ref().unwrap();
        assert_eq!(old.part_content_hash, new.part_content_hash);
        vector(
            new.resolved_plane.origin,
            [
                old.resolved_plane.origin[0] + 7.,
                old.resolved_plane.origin[1],
                old.resolved_plane.origin[2],
            ],
        );
        assert_eq!(a.declaration, b.declaration);
    }
}
