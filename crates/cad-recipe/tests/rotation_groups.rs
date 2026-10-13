const SOURCE: &str = include_str!("../../../examples/drive_module_rotation.star");

#[test]
fn complete_drive_group_preserves_source_obligations_and_counts_actual_queries() {
    let recipe = cad_recipe::from_starlark("drive_module_rotation.star", SOURCE).unwrap();
    let json = serde_json::to_string(&recipe).unwrap();
    let replay: cad_build::Recipe = serde_json::from_str(&json).unwrap();
    let mut engine = cad_build::BuildEngine::default();
    let built = engine.build(&replay).unwrap();
    let group = &built.rigid_rotation_groups["rotor"];
    assert_eq!(group.members.len(), 9);
    assert_eq!(group.coverage, "named_pairs_only");
    assert_eq!(group.clearance_check_ids.len(), 13);
    assert_eq!(built.assemblies["drive_module_release"].instances.len(), 30);
    assert_eq!(built.assemblies["drive_module_release"].parts.len(), 14);
    assert_eq!(built.datum_dimensions.len(), 20);
    assert_eq!(
        built.drawing_sheets["end_cover_machining"]
            .datum_dimension_ids
            .len(),
        10
    );
    let r = 12. / 2.;
    let pi = std::f64::consts::PI;
    let housing_volume =
        (2048. - pi * (r + 4.025f64).powi(2)) * 16. - 2. * pi * 1.75f64.powi(2) * 5.;
    for (side, output, floor_z) in [("left", "housing", 5.), ("right", "right_housing", 11.)] {
        let feature = format!("housing_{side}_pilots");
        assert!(built.sketch_reports[&feature].is_fully_constrained());
        let body = &built.outputs[output].result.model;
        assert!((body.volume() - housing_volume).abs() < 1e-6);
        let section = &built.section_views[&format!("{side}_cover_pilot_section")];
        assert_eq!(section.circle_dimensions.len(), 2);
        for tag in ["minus", "plus"] {
            let floor = &built.face_assertions[&format!("{side}_cover_{tag}_floor")];
            assert_eq!(floor.witnesses.len(), 1);
            let plane = body.faces[floor.witnesses[0].target_face.0]
                .try_plane()
                .unwrap();
            assert!((plane.origin[2] - floor_z).abs() < 1e-9);
            let diameter = section
                .circle_dimensions
                .iter()
                .find(|dimension| {
                    dimension.declaration.id == format!("{side}_cover_{tag}_diameter")
                })
                .unwrap();
            assert!((diameter.value_mm - 3.5).abs() < 1e-9);
            assert!(
                (built.datum_dimensions[&format!("{side}_cover_{tag}_depth")].value_mm - 5.).abs()
                    < 1e-9
            );
            assert!(
                (built.datum_dimensions[&format!("{side}_cover_{tag}_remaining_floor")].value_mm
                    - 11.)
                    .abs()
                    < 1e-9
            );
        }
        assert!(
            (built.datum_dimensions[&format!("{side}_cover_thickness")].value_mm - 2.).abs() < 1e-9
        );
    }
    let screw_volume = pi * (1.65f64.powi(2) * 8.5 + 3f64.powi(2) * 2.);
    assert!(
        (built.outputs["cover_screw_envelope"].result.model.volume() - screw_volume).abs() < 1e-6
    );
    let cover_area = 2048. - pi * (r + 4.5f64).powi(2) - 2. * pi * 2.05f64.powi(2);
    assert!((built.outputs["end_cover"].result.model.volume() - 2. * cover_area).abs() < 1e-6);
    assert_eq!(
        built.section_views["end_cover_section"]
            .circle_dimensions
            .len(),
        3
    );
    assert!(built.tolerance_stacks["shaft_axial_endplay"].measurement_interval_certified);
    assert!(built.tolerance_stacks["hub_axial_endplay"].measurement_interval_certified);
    let mut queries = 0;
    let mut common = 0;
    let mut bore_orbits = 0;
    for (id, check) in &built.clearance_checks {
        assert_eq!(check.rotation_group.as_deref(), Some("rotor"));
        assert!(check.lower_bound_mm >= check.declaration.min_clearance);
        if check.motion_proof.as_deref() == Some("common_rigid_motion") {
            common += 1;
            queries += 1;
            assert_eq!(check.moving_endpoint.as_deref(), Some("both"));
            assert!(check.rotation_sweep.is_none());
        } else if ["left_cover_shaft_gap", "right_cover_shaft_gap"].contains(&id.as_str()) {
            bore_orbits += 1;
            assert_eq!(check.motion_proof.as_deref(), Some("continuous_rotation"));
            assert_eq!(check.moving_endpoint.as_deref(), Some("right"));
            assert_eq!(
                serde_json::to_value(check.query.method).unwrap(),
                "bore_trim_witness"
            );
            let sweep = check.rotation_sweep.as_ref().unwrap();
            assert_eq!(
                serde_json::to_value(sweep.proof_kind).unwrap(),
                "bore_witness_orbit"
            );
            assert_eq!(sweep.query_count, 3);
            let certificate = sweep.bore_orbit.as_ref().unwrap();
            assert_eq!((certificate.start, certificate.end), (0., 1.));
            let expected = if id == "left_cover_shaft_gap" {
                2.5
            } else {
                4.5
            };
            assert!((check.lower_bound_mm - expected).abs() < 1e-4);
            queries += sweep.query_count;
        } else {
            let sweep = check.rotation_sweep.as_ref().unwrap();
            assert_eq!(
                serde_json::to_value(sweep.proof_kind).unwrap(),
                "rotational_symmetry"
            );
            assert!(sweep.intervals.is_empty());
            assert_eq!(sweep.query_count, 3);
            let certificate = sweep.symmetry.as_ref().unwrap();
            assert_eq!((certificate.start, certificate.end), (0., 1.));
            queries += sweep.query_count;
        }
    }
    assert_eq!((common, bore_orbits, queries), (6, 2, 27));
    let mut reversed = replay.clone();
    reversed.rigid_rotation_groups[0].angle_radians *= -1.;
    let cached = engine.build(&reversed).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.feature_hashes, built.feature_hashes);
    reversed.clearance_checks[0].min_clearance = 1.;
    assert!(engine.build_outputs(&reversed, &["shaft".into()]).is_err());
}

#[test]
fn strict_source_group_schema_rejects_invalid_motion_and_missing_authority() {
    for (from, to) in [
        ("default = 6.283185307179586", "default = 0.0"),
        (
            "reference_instance_path = [\"shaft\"]",
            "reference_instance_path = [\"base\"]",
        ),
        ("rotation_group = \"rotor\"", "rotation_group = \"unknown\""),
    ] {
        let source = SOURCE.replace(from, to);
        assert_ne!(source, SOURCE);
        let result = cad_recipe::from_starlark("invalid.star", &source);
        assert!(result.is_err() || cad_build::build(&result.unwrap()).is_err());
    }
}

#[test]
fn authored_left_moving_endpoint_keeps_initial_witness_orientation() {
    let mut recipe = cad_recipe::from_starlark("drive_module_rotation.star", SOURCE).unwrap();
    let c = recipe
        .clearance_checks
        .iter_mut()
        .find(|c| c.id == "left_support_running_gap")
        .unwrap();
    std::mem::swap(&mut c.left_instance_path, &mut c.right_instance_path);
    let built = cad_build::build(&recipe).unwrap();
    let report = &built.clearance_checks["left_support_running_gap"];
    assert_eq!(report.left.instance_path, ["shaft"]);
    assert_eq!(report.moving_endpoint.as_deref(), Some("left"));
    assert!(report.right_rotation_world.is_none());
    let initial = &report
        .rotation_sweep
        .as_ref()
        .unwrap()
        .samples
        .iter()
        .find(|s| s.time == 0.)
        .unwrap()
        .query;
    assert_eq!(report.query.left_point, initial.right_point);
    assert_eq!(report.query.right_point, initial.left_point);
    assert_eq!(report.query.axis, initial.axis.map(|v| -v));
}
