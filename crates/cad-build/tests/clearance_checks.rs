use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::{json, Value};
fn value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":2,"depth":3},{"op":"extrude","id":"body","profile":"p","height":4}],"outputs":[{"name":"body","feature":"body"}],"parts":[{"id":"block","feature":"body"}],
 "assemblies":[{"id":"cell","instances":[{"id":"leaf/a","part":"block"}]},{"id":"root","assemblies":[{"id":"left","assembly":"cell"},{"id":"right","assembly":"cell","transform":{"translation":[5,7,0]}}]}],"assembly_outputs":[{"name":"pair","assembly":"root"}],
 "clearance_checks":[{"id":"gap","output":"pair","left_instance_path":["left","leaf/a"],"right_instance_path":["right","leaf/a"],"min_clearance":4.9999}]})
}
fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn diagonal_euclidean_clearance_is_global_and_retains_exact_occurrence_witnesses() {
    let mut r = parse(value());
    let mut engine = BuildEngine::default();
    let first = engine.build_outputs(&r, &["body".into()]).unwrap();
    assert!(first.assemblies.is_empty());
    assert!(first.scope.validation_roots.contains(&"body".into()));
    let gap = &first.clearance_checks["gap"];
    assert_eq!(gap.status, "passed");
    assert!(gap.lower_bound_mm < 5. && gap.upper_bound_mm > 5.);
    assert!(gap.upper_bound_mm - gap.lower_bound_mm < 1e-7);
    assert_eq!(gap.left.instance_path, ["left", "leaf/a"]);
    assert_eq!(gap.right.instance_path, ["right", "leaf/a"]);
    assert_eq!(gap.left.feature_id, "body");
    assert_eq!(gap.left.part_content_hash, first.feature_hashes["body"]);
    assert!(gap.placement_uncertainty_mm > 0.);
    let a = gap.query.left_point;
    let b = gap.query.right_point;
    let distance = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    assert!((distance - 5.).abs() < 1e-8);
    r.clearance_checks[0].min_clearance = 5.001;
    assert!(
        matches!(engine.build_outputs(&r,&["body".into()]),Err(BuildError::ClearanceCheckFailed{lower_bound_mm:Some(lo),upper_bound_mm:Some(hi),..}) if lo<5.&&hi>5.)
    );
    r.clearance_checks[0].min_clearance = 4.5;
    let second = engine.build_outputs(&r, &["body".into()]).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(first.feature_hashes, second.feature_hashes);
}
#[test]
fn global_rigid_pose_and_swapping_paths_preserve_native_distance() {
    let mut v = value();
    let q = [
        0.,
        0.,
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ];
    v["assemblies"][1]["assemblies"][0]["transform"] =
        json!({"translation":[10,-20,30],"rotation":q});
    v["assemblies"][1]["assemblies"][1]["transform"] =
        json!({"translation":[3,-15,30],"rotation":q});
    let mut r = parse(v);
    let a = cad_build::build(&r).unwrap();
    let gap = &a.clearance_checks["gap"];
    assert!(gap.lower_bound_mm < 5. && gap.upper_bound_mm > 5.);
    let c = &mut r.clearance_checks[0];
    std::mem::swap(&mut c.left_instance_path, &mut c.right_instance_path);
    let b = cad_build::build(&r).unwrap();
    assert!((gap.lower_bound_mm - b.clearance_checks["gap"].lower_bound_mm).abs() < 1e-8);
}
#[test]
fn explicit_curved_nonconvex_contact_and_unresolved_domains_fail() {
    for offset in [[2., 0., 0.], [1., 0., 0.], [2. + 1e-9, 0., 0.]] {
        let mut v = value();
        v["assemblies"][1]["assemblies"][1]["transform"]["translation"] = json!(offset);
        assert!(matches!(
            cad_build::build(&parse(v)),
            Err(BuildError::ClearanceCheckFailed { .. })
        ));
    }
    let mut v = value();
    v["features"][0] = json!({"op":"circle","id":"p","radius":1,"inner_radius":0.5});
    // Hollow circular bodies now support parallel lateral offsets, but a true
    // relative tilt still lies outside the admitted annular family.
    v["assemblies"][1]["assemblies"][1]["transform"]["rotation"] =
        json!([(0.1_f64).sin(), 0., 0., (0.1_f64).cos()]);
    assert!(matches!(
        cad_build::build(&parse(v)),
        Err(BuildError::ClearanceCheckFailed { .. })
    ));
    let mut v = value();
    v["features"][0] =
        json!({"op":"polygon","id":"p","points":[[0,0],[2,0],[2,1],[1,1],[1,3],[0,3]]});
    assert!(matches!(
        cad_build::build(&parse(v)),
        Err(BuildError::ClearanceCheckFailed { .. })
    ));
    let mut v = value();
    v["assemblies"][1]["assemblies"][0]["transform"] = json!({"translation":[1e8,0,0]});
    v["assemblies"][1]["assemblies"][1]["transform"]["translation"] = json!([1e8 + 5., 7., 0.]);
    assert!(cad_build::build(&parse(v)).is_err());
}
#[test]
fn literal_leaf_paths_and_declaration_domain_are_strict() {
    for path in [
        json!(["left/leaf/a"]),
        json!(["left"]),
        json!(["left", "leaf", "a"]),
    ] {
        let mut v = value();
        v["clearance_checks"][0]["left_instance_path"] = path;
        assert!(matches!(
            cad_build::build(&parse(v)),
            Err(BuildError::ClearanceCheckFailed { .. })
        ));
    }
    for (field, invalid) in [
        ("output", json!("body")),
        ("min_clearance", json!(0)),
        ("left_instance_path", json!([])),
        ("left_instance_path", json!(["right", "leaf/a"])),
    ] {
        let mut v = value();
        v["clearance_checks"][0][field] = invalid;
        v["features"][0]["width"] = json!(-1);
        assert!(matches!(
            cad_build::build(&parse(v)),
            Err(BuildError::InvalidClearanceCheck { .. })
        ));
    }
    for invalid in [
        json!(null),
        json!([["gap", "pair", ["a"], ["b"], 1]]),
        json!([{"id":"x","output":"pair","left_instance_path":["a"],"right_instance_path":["b"],"min_clearance":1,"extra":true}]),
    ] {
        let mut v = value();
        v["clearance_checks"] = invalid;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut v =
        json!({"schema_version":1,"units":"mm","features":[],"outputs":[],"clearance_checks":[]});
    assert!(serde_json::from_value::<Recipe>(v.clone()).is_err());
    v["schema_version"] = json!(2);
    assert!(serde_json::from_value::<Recipe>(v).is_ok());
    let mut r = parse(value());
    let check = r.clearance_checks[0].clone();
    r.clearance_checks = (0..65)
        .map(|i| {
            let mut c = check.clone();
            c.id = format!("check{i}");
            c
        })
        .collect();
    assert!(matches!(
        cad_build::build(&r),
        Err(BuildError::ResourceLimit {
            resource: "clearance checks",
            ..
        })
    ));
}

#[test]
fn oversized_source_operand_is_rejected_before_placement() {
    let mut v = value();
    let points: Vec<_> = (0..65)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 65.;
            [10. * a.cos(), 10. * a.sin()]
        })
        .collect();
    v["features"][0] = json!({"op":"polygon","id":"p","points":points});
    // Placement would independently exceed the precision budget. Admission must
    // reject the oversized source before inspecting/materializing that placement.
    v["assemblies"][1]["assemblies"][0]["transform"] = json!({"translation":[1e8,0,0]});
    assert!(
        matches!(cad_build::build(&parse(v)), Err(BuildError::ClearanceCheckFailed {message,..}) if message.starts_with("pre-placement clearance admission:") && message.contains("limit"))
    );
}

#[test]
fn solid_cylinders_and_mixed_pairs_are_global_native_clearance_obligations() {
    for (mixed, tilted) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut v = value();
        v["features"][0] = json!({"op":"circle","id":"p","radius":1});
        v["assemblies"][1]["assemblies"][1]["transform"]["translation"] = json!([5, 0, 0]);
        if mixed {
            v["features"].as_array_mut().unwrap().extend([
                json!({"op":"rectangle","id":"q","width":2,"depth":3}),
                json!({"op":"extrude","id":"box","profile":"q","height":4}),
            ]);
            v["parts"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"box_part","feature":"box"}));
            v["assemblies"].as_array_mut().unwrap().insert(
                1,
                json!({"id":"box_cell","instances":[{"id":"leaf/a","part":"box_part"}]}),
            );
            v["assemblies"][2]["assemblies"][1]["assembly"] = json!("box_cell");
        }
        if tilted {
            let root = if mixed { 2 } else { 1 };
            let q = [
                0.,
                std::f64::consts::FRAC_1_SQRT_2,
                0.,
                std::f64::consts::FRAC_1_SQRT_2,
            ];
            v["assemblies"][root]["assemblies"][0]["transform"] =
                json!({"translation":[10,-20,30],"rotation":q});
            v["assemblies"][root]["assemblies"][1]["transform"] =
                json!({"translation":[10,-20,25],"rotation":q});
        }
        let expected = if mixed { 4. } else { 3. };
        v["clearance_checks"][0]["min_clearance"] = json!(expected - 1e-5);
        let mut r = parse(v);
        let mut engine = BuildEngine::default();
        let b = engine.build_outputs(&r, &["body".into()]).unwrap();
        let c = &b.clearance_checks["gap"];
        assert!(c.lower_bound_mm <= expected && c.upper_bound_mm >= expected);
        if !mixed && !tilted {
            // Full cylinder bounds span 2+2+4 mm; seam vertices alone
            // would understate the placement arithmetic operands.
            assert!(c.placement_uncertainty_mm >= 21. * 1024. * f64::EPSILON);
        }
        assert!(c.upper_bound_mm - c.lower_bound_mm < 1e-7);
        assert!(b.assemblies.is_empty());
        r.clearance_checks[0].min_clearance = expected + 1e-5;
        assert!(matches!(
            engine.build_outputs(&r, &["body".into()]),
            Err(BuildError::ClearanceCheckFailed {
                lower_bound_mm: Some(_),
                ..
            })
        ));
        r.clearance_checks[0].min_clearance = expected - 1e-5;
        assert_eq!(
            engine
                .build_outputs(&r, &["body".into()])
                .unwrap()
                .stats
                .computed_solids,
            0
        );
    }
}

#[test]
fn convex_revolved_cones_frusta_and_mixed_pairs_preserve_global_checks() {
    for profile in [
        json!([[0, 0], [2, 0], [0, 4]]),
        json!([[0, 0], [2, 0], [1, 4], [0, 4]]),
        json!([[0, 0], [2, 0], [2, 1], [1, 4], [0, 4]]),
    ] {
        for mixed in [false, true] {
            let mut v = value();
            v["features"][0] = json!({"op":"polygon","id":"p","points":profile});
            v["features"][1] = json!({"op":"revolve","id":"body","profile":"p"});
            if mixed {
                v["features"].as_array_mut().unwrap().extend([
                    json!({"op":"rectangle","id":"q","width":2,"depth":3}),
                    json!({"op":"extrude","id":"box","profile":"q","height":4}),
                ]);
                v["parts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"box_part","feature":"box"}));
                v["assemblies"].as_array_mut().unwrap().insert(
                    1,
                    json!({"id":"box_cell","instances":[{"id":"leaf/a","part":"box_part"}]}),
                );
                v["assemblies"][2]["assemblies"][1]["assembly"] = json!("box_cell");
            }
            let root = if mixed { 2 } else { 1 };
            // Rotate the whole pair: its native Z separation becomes world X.
            let q = [
                0.,
                std::f64::consts::FRAC_1_SQRT_2,
                0.,
                std::f64::consts::FRAC_1_SQRT_2,
            ];
            v["assemblies"][root]["assemblies"][0]["transform"] =
                json!({"translation":[10,-20,30],"rotation":q});
            v["assemblies"][root]["assemblies"][1]["transform"] =
                json!({"translation":[17,-20,30],"rotation":q});
            v["clearance_checks"][0]["min_clearance"] = json!(2.99999);
            let mut r = parse(v);
            let mut engine = BuildEngine::default();
            let b = engine.build_outputs(&r, &["body".into()]).unwrap();
            let c = &b.clearance_checks["gap"];
            assert!(c.lower_bound_mm <= 3. && c.upper_bound_mm >= 3.);
            assert!(c.upper_bound_mm - c.lower_bound_mm < 1e-7);
            assert!(b.assemblies.is_empty());
            r.clearance_checks[0].min_clearance = 3.00001;
            assert!(matches!(
                engine.build_outputs(&r, &["body".into()]),
                Err(BuildError::ClearanceCheckFailed {
                    lower_bound_mm: Some(_),
                    ..
                })
            ));
            r.clearance_checks[0].min_clearance = 2.99999;
            assert_eq!(
                engine
                    .build_outputs(&r, &["body".into()])
                    .unwrap()
                    .stats
                    .computed_solids,
                0
            );
        }
    }
}

#[test]
fn noncoaxial_hollow_and_nonconvex_revolutions_fail_explicitly() {
    for profile in [
        json!([[1, 0], [2, 0], [3, 4], [2, 4]]),
        json!([[0, 0], [2, 0], [1, 2], [2, 4], [0, 4]]),
    ] {
        let mut v = value();
        v["features"][0] = json!({"op":"polygon","id":"p","points":profile});
        v["features"][1] = json!({"op":"revolve","id":"body","profile":"p"});
        assert!(
            matches!(cad_build::build(&parse(v)),Err(BuildError::ClearanceCheckFailed {message,..}) if !message.is_empty())
        );
    }
}

fn coaxial_pair() -> Value {
    let mut v = value();
    v["features"][0] = json!({"op":"circle","id":"p","radius":1});
    v["features"].as_array_mut().unwrap().extend([
        json!({"op":"circle","id":"ring_profile","radius":3,"inner_radius":2}),
        json!({"op":"extrude","id":"ring","profile":"ring_profile","height":4}),
    ]);
    v["parts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ring_part","feature":"ring"}));
    v["assemblies"].as_array_mut().unwrap().insert(
        1,
        json!({"id":"ring_cell","instances":[{"id":"leaf/a","part":"ring_part"}]}),
    );
    v["assemblies"][2]["assemblies"][1]["assembly"] = json!("ring_cell");
    v["assemblies"][2]["assemblies"][1]["transform"] = json!({"translation":[0,0,0]});
    v["clearance_checks"][0]["min_clearance"] = json!(0.99999);
    v
}

#[test]
fn coaxial_shaft_inside_ring_is_native_and_globally_enforced() {
    for (opposed, posed) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut v = coaxial_pair();
        if opposed {
            v["assemblies"][2]["assemblies"][1]["transform"] =
                json!({"translation":[0,0,4],"rotation":[1,0,0,0]});
        }
        if posed {
            let q = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
            v["assemblies"].as_array_mut().unwrap().push(json!({"id":"posed","assemblies":[{"id":"pose","assembly":"root","transform":{"translation":[10,-20,30],"rotation":q}}]}));
            v["assembly_outputs"][0]["assembly"] = json!("posed");
            v["clearance_checks"][0]["left_instance_path"] = json!(["pose", "left", "leaf/a"]);
            v["clearance_checks"][0]["right_instance_path"] = json!(["pose", "right", "leaf/a"]);
        }
        let mut r = parse(v);
        let mut engine = BuildEngine::default();
        let b = engine.build_outputs(&r, &["body".into()]).unwrap();
        let c = &b.clearance_checks["gap"];
        assert_eq!(c.query.method, cad_kernel::ClearanceMethod::CoaxialMeridian);
        assert!(c.lower_bound_mm <= 1. && c.upper_bound_mm >= 1.);
        assert!(c.upper_bound_mm - c.lower_bound_mm < 1e-7);
        assert!(b.assemblies.is_empty());
        r.clearance_checks[0].min_clearance = 1.00001;
        assert!(matches!(
            engine.build_outputs(&r, &["body".into()]),
            Err(BuildError::ClearanceCheckFailed {
                lower_bound_mm: Some(_),
                ..
            })
        ));
        r.clearance_checks[0].min_clearance = 0.99999;
        assert_eq!(
            engine
                .build_outputs(&r, &["body".into()])
                .unwrap()
                .stats
                .computed_solids,
            0
        );
    }
}

#[test]
fn coaxial_query_rejects_overlap_contact_and_actual_axis_tilt() {
    for radius in [2., 2.5, 4.] {
        let mut v = coaxial_pair();
        v["features"][0]["radius"] = json!(radius);
        assert!(matches!(
            cad_build::build(&parse(v)),
            Err(BuildError::ClearanceCheckFailed { .. })
        ));
    }
    for transform in [
        json!({"rotation":[(0.001_f64).sin(),0,0,(0.001_f64).cos()]}),
        json!({"translation":[0.25,0,0],"rotation":[(0.001_f64).sin(),0,0,(0.001_f64).cos()]}),
    ] {
        let mut v = coaxial_pair();
        v["assemblies"][2]["assemblies"][1]["transform"] = transform;
        assert!(matches!(
            cad_build::build(&parse(v)),
            Err(BuildError::ClearanceCheckFailed { .. })
        ));
    }
}

#[test]
fn coaxial_nonconvex_revolution_uses_filled_meridian_without_hull_substitution() {
    let mut v = value();
    v["features"][0] = json!({"op":"polygon","id":"p","points":[[0,0],[2,0],[1,2],[2,4],[0,4]]});
    v["features"][1] = json!({"op":"revolve","id":"body","profile":"p"});
    v["assemblies"][1]["assemblies"][1]["transform"] = json!({"translation":[0,0,7]});
    v["clearance_checks"][0]["min_clearance"] = json!(2.99999);
    let b = cad_build::build(&parse(v)).unwrap();
    let q = &b.clearance_checks["gap"].query;
    assert_eq!(q.method, cad_kernel::ClearanceMethod::CoaxialMeridian);
    assert!(q.lower_bound_mm <= 3. && q.upper_bound_mm >= 3.);
}

#[test]
fn parallel_annular_eccentric_and_external_pairs_are_global_native_queries() {
    for (two_rings, offset, expected) in [
        (false, [0.25, 0., 0.], 0.75),
        (true, [8., 0., 0.], 2.),
        (true, [4., 0., 7.], 3.),
        (true, [8., 0., 7.], 13_f64.sqrt()),
    ] {
        for opposed in [false, true] {
            let mut v = coaxial_pair();
            if two_rings {
                v["features"][0] = json!({"op":"circle","id":"p","radius":3,"inner_radius":2});
            }
            let translation = [
                offset[0],
                offset[1],
                offset[2] + if opposed { 4. } else { 0. },
            ];
            v["assemblies"][2]["assemblies"][1]["transform"] =
                json!({"translation":translation,"rotation":if opposed {[1,0,0,0]}else{[0,0,0,1]}});
            v["clearance_checks"][0]["min_clearance"] = json!(expected - 1e-5);
            let mut r = parse(v);
            let mut engine = BuildEngine::default();
            let b = engine.build_outputs(&r, &["body".into()]).unwrap();
            let c = &b.clearance_checks["gap"];
            assert_eq!(c.query.method, cad_kernel::ClearanceMethod::ParallelAnnular);
            assert!(c.lower_bound_mm <= expected && c.upper_bound_mm >= expected);
            assert!(c.upper_bound_mm - c.lower_bound_mm < 1e-7);
            assert!(b.assemblies.is_empty());
            r.clearance_checks[0].min_clearance = expected + 1e-5;
            assert!(matches!(
                engine.build_outputs(&r, &["body".into()]),
                Err(BuildError::ClearanceCheckFailed {
                    lower_bound_mm: Some(_),
                    ..
                })
            ));
            r.clearance_checks[0].min_clearance = expected - 1e-5;
            assert_eq!(
                engine
                    .build_outputs(&r, &["body".into()])
                    .unwrap()
                    .stats
                    .computed_solids,
                0
            );
        }
    }
}
