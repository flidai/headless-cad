use super::*;
fn four_bar(angle: f64, upper: bool) -> PlanarMechanism {
    PlanarMechanism {
        pins: vec![
            MechanismPin {
                id: "A".into(),
                initial: [0., 0.],
                ground: Some([0., 0.]),
            },
            MechanismPin {
                id: "D".into(),
                initial: [80., 0.],
                ground: Some([80., 0.]),
            },
            MechanismPin {
                id: "B".into(),
                initial: [26., 15.],
                ground: None,
            },
            MechanismPin {
                id: "C".into(),
                initial: if upper { [80., 50.] } else { [50., -40.] },
                ground: None,
            },
        ],
        links: vec![
            MechanismLink {
                id: "crank".into(),
                from: "A".into(),
                to: "B".into(),
                length: 30.,
            },
            MechanismLink {
                id: "coupler".into(),
                from: "B".into(),
                to: "C".into(),
                length: 70.,
            },
            MechanismLink {
                id: "rocker".into(),
                from: "D".into(),
                to: "C".into(),
                length: 50.,
            },
        ],
        drives: vec![MechanismAngleDrive {
            id: "motor".into(),
            link: "crank".into(),
            angle_radians: angle,
        }],
        branches: vec![MechanismBranch {
            id: "assembly_mode".into(),
            pins: ["B".into(), "D".into(), "C".into()],
            side: if upper {
                MechanismBranchSide::Left
            } else {
                MechanismBranchSide::Right
            },
        }],
    }
}
#[test]
fn arbitrary_four_bar_closure_matches_independent_circle_intersections_and_frames() {
    let t = Tolerance::default();
    for degrees in [30_f64, 45., 60., 80.] {
        for upper in [true, false] {
            let angle = degrees.to_radians();
            let input = four_bar(angle, upper);
            let report = solve_planar_mechanism(&input, t).unwrap();
            assert_eq!(report.solver.degrees_of_freedom, 0);
            assert!(report.solver.is_fully_constrained());
            let b = [30. * angle.cos(), 30. * angle.sin()];
            let delta = [80. - b[0], -b[1]];
            let distance = delta[0].hypot(delta[1]);
            let along = (70_f64.powi(2) - 50_f64.powi(2) + distance * distance) / (2. * distance);
            let height = (70_f64.powi(2) - along * along).sqrt();
            let sign = if upper { 1. } else { -1. };
            let c = [
                b[0] + along * delta[0] / distance - sign * height * delta[1] / distance,
                b[1] + along * delta[1] / distance + sign * height * delta[0] / distance,
            ];
            for id in ["B", "C"] {
                for (actual, expected) in
                    report.pins[id]
                        .into_iter()
                        .zip(if id == "B" { b } else { c })
                {
                    assert!(
                        (actual - expected).abs() < 1e-6,
                        "{degrees} {id}: {actual} != {expected}"
                    );
                }
            }
            for link in &input.links {
                let frame = &report.link_frames[&link.id];
                let expected = report.pins[&link.to];
                let endpoint = frame.point([link.length, 0., 0.]);
                assert!((endpoint[0] - expected[0]).hypot(endpoint[1] - expected[1]) < t.linear);
            }
            assert!(
                report.branches[0].signed_double_area_mm2.abs()
                    > report.branches[0].uncertainty_margin_mm2
            );
            let mut reordered = input.clone();
            reordered.pins.reverse();
            reordered.links.reverse();
            assert_eq!(report, solve_planar_mechanism(&reordered, t).unwrap());
        }
    }
}
#[test]
fn underconstraint_inconsistency_wrong_branch_and_tangent_branch_never_produce_placements() {
    let t = Tolerance::default();
    let mut input = four_bar(0.8, true);
    input.drives.clear();
    let err = solve_planar_mechanism(&input, t).unwrap_err();
    assert_eq!(
        err.solver_report().unwrap().status,
        cad_sketch::SolveStatus::Underconstrained
    );
    let mut input = four_bar(0.8, true);
    input.links[1].length = 1.;
    let err = solve_planar_mechanism(&input, t).unwrap_err();
    assert!(err.solver_report().is_some());
    let mut input = four_bar(0.8, true);
    input.branches[0].side = MechanismBranchSide::Right;
    let err = solve_planar_mechanism(&input, t).unwrap_err();
    assert!(err.to_string().contains("wrong-sided"));
    assert!(err.solver_report().unwrap().is_fully_constrained());
    let input = PlanarMechanism {
        pins: vec![
            MechanismPin {
                id: "a".into(),
                initial: [0., 0.],
                ground: Some([0., 0.]),
            },
            MechanismPin {
                id: "b".into(),
                initial: [10., 0.],
                ground: Some([10., 0.]),
            },
            MechanismPin {
                id: "c".into(),
                initial: [5., 1e-8],
                ground: Some([5., 1e-8]),
            },
        ],
        links: vec![MechanismLink {
            id: "bar".into(),
            from: "a".into(),
            to: "b".into(),
            length: 10.,
        }],
        drives: vec![],
        branches: vec![MechanismBranch {
            id: "nearline".into(),
            pins: ["a".into(), "b".into(), "c".into()],
            side: MechanismBranchSide::Left,
        }],
    };
    assert!(solve_planar_mechanism(&input, t)
        .unwrap_err()
        .to_string()
        .contains("unresolved"));
}
#[test]
fn arbitrary_triangulated_graph_and_strict_schema_limits() {
    let t = Tolerance::default();
    let input = PlanarMechanism {
        pins: vec![
            MechanismPin {
                id: "a".into(),
                initial: [0., 0.],
                ground: Some([0., 0.]),
            },
            MechanismPin {
                id: "b".into(),
                initial: [4., 0.],
                ground: Some([4., 0.]),
            },
            MechanismPin {
                id: "c".into(),
                initial: [0.3, 2.7],
                ground: None,
            },
        ],
        links: vec![
            MechanismLink {
                id: "left".into(),
                from: "a".into(),
                to: "c".into(),
                length: 3.,
            },
            MechanismLink {
                id: "right".into(),
                from: "b".into(),
                to: "c".into(),
                length: 5.,
            },
        ],
        drives: vec![],
        branches: vec![MechanismBranch {
            id: "up".into(),
            pins: ["a".into(), "b".into(), "c".into()],
            side: MechanismBranchSide::Left,
        }],
    };
    let report = solve_planar_mechanism(&input, t).unwrap();
    assert!(report.pins["c"][0].abs() < 1e-6);
    assert!((report.pins["c"][1] - 3.).abs() < 1e-6);
    let mut invalid = input.clone();
    invalid.links[0].length = 0.;
    assert!(validate_planar_mechanism(&invalid).is_err());
    let mut invalid = input.clone();
    invalid.pins[0].initial = [f64::NAN, 0.];
    assert!(validate_planar_mechanism(&invalid).is_err());
    let mut invalid = input.clone();
    invalid.links[0].to = "missing".into();
    assert!(validate_planar_mechanism(&invalid).is_err());
    let mut value = serde_json::to_value(&input).unwrap();
    value["dynamics"] = serde_json::json!(true);
    assert!(serde_json::from_value::<PlanarMechanism>(value).is_err());
    let mut invalid = input;
    invalid
        .pins
        .resize(MAX_MECHANISM_PINS + 1, invalid.pins[0].clone());
    assert!(validate_planar_mechanism(&invalid).is_err());
}
