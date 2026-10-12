//! Product acceptance fixtures deliberately assert engineering outcomes instead
//! of exact face counts: Boolean boundary subdivision may evolve legitimately.
use std::path::Path;

#[test]
fn tooling_plate_top_reference_preserves_ancestry_and_independent_area() {
    use cad_build::{FaceCardinality, FaceResolutionStatus, FaceSelector};
    use std::collections::BTreeMap;

    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/reference_fixture.star"),
    )
    .unwrap();
    for (width, depth, thickness, groove_width, groove_depth) in [
        (80.0, 40.0, 8.0, 8.0, 4.0),
        (100.0, 60.0, 12.0, 10.0, 6.0),
        (64.0, 20.0, 3.0, 2.0, 1.0),
        (120.0, 48.0, 16.0, 16.0, 15.0),
    ] {
        let parameters = BTreeMap::from([
            ("width".into(), width),
            ("depth".into(), depth),
            ("thickness".into(), thickness),
            ("groove_width".into(), groove_width),
            ("groove_depth".into(), groove_depth),
        ]);
        let recipe =
            cad_recipe::from_starlark_with_params("reference_fixture.star", &source, &parameters)
                .unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let selector = FaceSelector {
            anchor_feature: "blank".into(),
            role: "end_cap".into(),
            target_feature: "finished".into(),
            cardinality: FaceCardinality::All,
        };
        let resolution = built.resolve_faces(&selector);
        assert_eq!(resolution.status, FaceResolutionStatus::Resolved);
        let model = &built.outputs["reference_fixture"].result.model;
        let mut selected_area = 0.0;
        for witness in &resolution.witnesses {
            let face = &model.faces[witness.target_face.0];
            assert_eq!(face.try_plane().unwrap().normal, [0.0, 0.0, 1.0]);
            assert_eq!(
                face.try_plane().unwrap().origin[2],
                thickness,
                "selected a stop top or groove floor"
            );
            let mut min = [f64::INFINITY; 2];
            let mut max = [f64::NEG_INFINITY; 2];
            for loop_id in &face.loops {
                for edge_use in &model.loops[loop_id.0].edges {
                    for vertex_id in model.edges[edge_use.edge.0].vertices {
                        let point = model.vertices[vertex_id.0].position;
                        for axis in 0..2 {
                            min[axis] = min[axis].min(point[axis]);
                            max[axis] = max[axis].max(point[axis]);
                        }
                    }
                }
            }
            selected_area += (max[0] - min[0]) * (max[1] - min[1]);
        }
        let expected_area = width * depth - 3.0 * groove_width * (depth - 8.0) - 64.0;
        assert!((selected_area - expected_area).abs() < 1e-7);
        let expected_volume =
            width * depth * thickness - 3.0 * groove_width * (depth - 8.0) * groove_depth + 768.0;
        assert!((model.volume() - expected_volume).abs() < 1e-7);
        let single = built.resolve_faces(&FaceSelector {
            cardinality: FaceCardinality::Exactly { count: 1 },
            ..selector
        });
        assert_eq!(single.status, FaceResolutionStatus::Ambiguous);
        assert_eq!(single.witnesses.len(), resolution.witnesses.len());
    }
}

#[test]
fn product_recipes_match_independent_analytical_expectations() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(examples.join("expected.json")).unwrap())
            .unwrap();
    for (filename, outputs) in expected.as_object().unwrap() {
        let source = std::fs::read_to_string(examples.join(filename)).unwrap();
        let recipe = cad_recipe::from_starlark(filename, &source).unwrap();
        let result = cad_build::build(&recipe).unwrap_or_else(|e| panic!("{filename}: {e}"));
        assert_eq!(
            result.outputs.len() + result.assemblies.len(),
            outputs.as_object().unwrap().len(),
            "{filename}: unexpected output set"
        );
        for (name, expected) in outputs.as_object().unwrap() {
            let (volume, solid_count, bounds) = if let Some(assembly) = result.assemblies.get(name)
            {
                assembly
                    .validate(recipe.tolerance.unwrap_or_default())
                    .unwrap();
                assert!(cad_preview::to_assembly_svg(assembly)
                    .unwrap()
                    .contains("<polygon"));
                let display: serde_json::Value =
                    serde_json::from_str(&cad_preview::to_assembly_scene_json(assembly).unwrap())
                        .unwrap();
                assert_eq!(display["kind"], "assembly");
                (
                    assembly.material_volume(),
                    assembly.solid_count(),
                    assembly.bounds().unwrap(),
                )
            } else {
                let model = &result.outputs[name].result.model;
                assert!(
                    model.validate(recipe.tolerance.unwrap_or_default()).valid,
                    "{filename}:{name}"
                );
                assert!(cad_preview::to_svg(model).unwrap().contains("<polygon"));
                assert!(cad_preview::to_html(model)
                    .unwrap()
                    .contains("Model overview"));
                (model.volume(), model.solids.len(), model.bounds())
            };
            let expected_volume = expected["volume_mm3"].as_f64().unwrap();
            // Numerical sketch solutions carry a dimensional residual budget.
            // Their fixtures state separately propagated volume/position bounds;
            // existing directly constructed fixtures retain the stricter default.
            let volume_tolerance = expected["volume_tolerance_mm3"].as_f64().unwrap_or(1e-7);
            let linear_tolerance = expected["linear_tolerance_mm"].as_f64().unwrap_or(1e-7);
            assert!(
                (volume - expected_volume).abs() < volume_tolerance,
                "{filename}:{name}: actual {} expected {expected_volume}",
                volume
            );
            assert_eq!(solid_count as u64, expected["solids"].as_u64().unwrap());
            for axis in 0..3 {
                assert!(
                    (bounds.min[axis] - expected["bounds_mm"][0][axis].as_f64().unwrap()).abs()
                        < linear_tolerance
                );
                assert!(
                    (bounds.max[axis] - expected["bounds_mm"][1][axis].as_f64().unwrap()).abs()
                        < linear_tolerance
                );
            }
        }
    }
}

#[test]
fn ambitious_assemblies_remain_parametric_and_match_material_accounting() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    // The slots never overlap the locating fence or the separated jaws.
    let fixture_width = 140.0;
    let fixture_source = std::fs::read_to_string(examples.join("fixture.star")).unwrap();
    let fixture = cad_recipe::from_starlark_with_params(
        "fixture.star",
        &fixture_source,
        &std::collections::BTreeMap::from([
            ("width".into(), fixture_width),
            ("jaw_clearance".into(), 5.0),
        ]),
    )
    .unwrap();
    let fixture = cad_build::build(&fixture).unwrap();
    let model = &fixture.assemblies["fixture"];
    let plate = fixture_width * 80.0 * 8.0;
    let added_fence = fixture_width * 8.0 * (24.0 - 8.0);
    let removed_slots = 4.0 * 8.0 * 36.0 * 8.0;
    let separate_jaws = 2.0 * 18.0 * 12.0 * 16.0;
    assert!(
        (model.material_volume() - (plate + added_fence - removed_slots + separate_jaws)).abs()
            < 1e-7
    );
    assert_eq!(model.solid_count(), 3);
    assert_eq!(model.bounds().unwrap().max, [fixture_width, 80.0, 29.0]);

    // Open cavity removal, eight nonoverlapping through-vents, four posts whose
    // bases meet the floor, and an independent lid have additive volumes.
    let enclosure_width = 120.0;
    let enclosure_source = std::fs::read_to_string(examples.join("enclosure.star")).unwrap();
    let enclosure = cad_recipe::from_starlark_with_params(
        "enclosure.star",
        &enclosure_source,
        &std::collections::BTreeMap::from([
            ("width".into(), enclosure_width),
            ("lid_gap".into(), 12.0),
        ]),
    )
    .unwrap();
    let enclosure = cad_build::build(&enclosure).unwrap();
    let body = enclosure_width * 68.0 * 36.0
        - (enclosure_width - 8.0) * 60.0 * 32.0
        - 8.0 * 8.0 * 4.0 * 8.0
        + 4.0 * 6.0 * 6.0 * 12.0;
    let lid = enclosure_width * 68.0 * 3.0;
    let model = &enclosure.assemblies["enclosure"];
    assert!((enclosure.outputs["enclosure_body"].result.model.volume() - body).abs() < 1e-7);
    assert!((model.material_volume() - (body + lid)).abs() < 1e-7);
    assert_eq!(model.solid_count(), 2);
    assert_eq!(model.bounds().unwrap().max, [enclosure_width, 68.0, 51.0]);
}

#[test]
fn tilted_jig_parameter_edits_preserve_exact_tool_coordinates_and_cap_intent() {
    use cad_build::{FaceCardinality, FaceResolutionStatus, FaceSelector};
    use std::collections::BTreeMap;
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/tilted_jig.star"),
    )
    .unwrap();
    let cosine = 3.0_f64.sqrt() / 2.0;
    for (length, rise, span) in [(30.0, 12.0, 8.0), (10.0, 3.0, 3.0), (20.0, 20.0, 6.0)] {
        let params = BTreeMap::from([
            ("tool_length".into(), length),
            ("tool_rise".into(), rise),
            ("tool_span".into(), span),
        ]);
        let recipe =
            cad_recipe::from_starlark_with_params("tilted_jig.star", &source, &params).unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let model = &built.outputs["left_tool"].result.model;
        // Independent scalar coordinates: stand the triangular section upright,
        // then turn its X/thickness basis by 30 degrees in the base plane.
        let mut expected_points = Vec::new();
        for thickness in [0.0, span] {
            for (along, up) in [(0.0, 0.0), (length, 0.0), (0.0, rise)] {
                expected_points.push([
                    10.0 + cosine * along + 0.5 * thickness,
                    20.0 + 0.5 * along - cosine * thickness,
                    4.0 + up,
                ]);
            }
        }
        assert_eq!(model.vertices.len(), expected_points.len());
        for expected in expected_points {
            assert_eq!(
                model
                    .vertices
                    .iter()
                    .filter(|vertex| {
                        (0..3).all(|axis| (vertex.position[axis] - expected[axis]).abs() < 1e-7)
                    })
                    .count(),
                1,
                "missing or duplicated analytic point {expected:?}"
            );
        }
        assert!((model.volume() - length * rise * span / 2.0).abs() < 1e-7);
        assert!(
            (built.assemblies["tilted_jig"].material_volume() - (15680.0 + length * rise * span))
                .abs()
                < 1e-7
        );
        let selection = built.resolve_faces(&FaceSelector {
            anchor_feature: "tool_blank".into(),
            role: "end_cap".into(),
            target_feature: "left_tool".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        });
        assert_eq!(selection.status, FaceResolutionStatus::Resolved);
        let face = &model.faces[selection.witnesses[0].target_face.0];
        let expected_normal = [0.5, -cosine, 0.0];
        for (actual, expected) in face
            .try_plane()
            .unwrap()
            .normal
            .into_iter()
            .zip(expected_normal)
        {
            assert!((actual - expected).abs() < 1e-12);
        }
    }
}

#[test]
fn clipped_tools_edits_match_independent_cross_sections_and_both_operand_references() {
    use cad_build::{FaceCardinality, FaceResolutionStatus, FaceSelector};
    use std::collections::BTreeMap;
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/clipped_tools.star"),
    )
    .unwrap();
    for (crop_height, shoulder_height) in [(6.0, 15.0), (2.0, 11.0), (4.0, 18.0)] {
        let parameters = BTreeMap::from([
            ("crop_height".into(), crop_height),
            ("shoulder_height".into(), shoulder_height),
        ]);
        let recipe =
            cad_recipe::from_starlark_with_params("clipped_tools.star", &source, &parameters)
                .unwrap();
        let built = cad_build::build(&recipe).unwrap();
        // Octagonal section: 12*14 rectangle minus four right triangles with legs of length 3.
        let octagonal_volume = 150.0 * crop_height;
        // Shoulder section: 20*h rectangle minus two right triangles with legs of length (h-10).
        let shoulder_volume = 10.0 * (20.0 * shoulder_height - (shoulder_height - 10.0).powi(2));
        assert!((built.outputs["octagonal"].result.model.volume() - octagonal_volume).abs() < 1e-7);
        let model = &built.outputs["shouldered"].result.model;
        assert!((model.volume() - shoulder_volume).abs() < 1e-7);
        assert!(
            (built.assemblies["clipped_tools"].material_volume()
                - (5250.0 + octagonal_volume + shoulder_volume))
                .abs()
                < 1e-7
        );
        let section = [
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [30.0 - shoulder_height, shoulder_height],
            [shoulder_height - 10.0, shoulder_height],
            [0.0, 10.0],
        ];
        assert_eq!(model.vertices.len(), 12);
        for y in [0.0, 10.0] {
            for [x, z] in section {
                assert_eq!(
                    model
                        .vertices
                        .iter()
                        .filter(|v| {
                            (v.position[0] - x).abs() < 1e-7
                                && (v.position[1] - y).abs() < 1e-7
                                && (v.position[2] - z).abs() < 1e-7
                        })
                        .count(),
                    1,
                    "missing analytic section vertex [{x},{y},{z}]"
                );
            }
        }
        for (anchor, normal) in [
            (
                "shoulder_operand",
                [
                    std::f64::consts::FRAC_1_SQRT_2,
                    0.0,
                    std::f64::consts::FRAC_1_SQRT_2,
                ],
            ),
            ("shoulder_crop", [0.0, 0.0, 1.0]),
        ] {
            let selection = built.resolve_faces(&FaceSelector {
                anchor_feature: anchor.into(),
                role: "end_cap".into(),
                target_feature: "shouldered".into(),
                cardinality: FaceCardinality::Exactly { count: 1 },
            });
            assert_eq!(selection.status, FaceResolutionStatus::Resolved);
            let face = &model.faces[selection.witnesses[0].target_face.0];
            for (actual, expected) in face.try_plane().unwrap().normal.into_iter().zip(normal) {
                assert!((actual - expected).abs() < 1e-12);
            }
        }
    }
}
