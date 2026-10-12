use super::*;
fn constraint(id: &str, kind: ConstraintKind) -> Constraint {
    Constraint {
        id: id.into(),
        kind,
    }
}
fn point(id: &str, position: [f64; 2]) -> SketchPoint {
    SketchPoint {
        id: id.into(),
        position,
    }
}
fn pair(a: &str, b: &str) -> [String; 2] {
    [a.into(), b.into()]
}
fn triangle(degrees: f64, scale: f64) -> Sketch {
    let angle = degrees.to_radians();
    Sketch {
        points: vec![
            point("a", [0., 0.]),
            point("b", [scale, 0.]),
            point("c", [scale * angle.cos() * 0.9, scale * angle.sin() * 1.1]),
        ],
        constraints: vec![
            constraint(
                "anchor",
                ConstraintKind::Fixed {
                    point: "a".into(),
                    position: [0., 0.],
                },
            ),
            constraint(
                "base",
                ConstraintKind::Fixed {
                    point: "b".into(),
                    position: [scale, 0.],
                },
            ),
            constraint(
                "length",
                ConstraintKind::EqualLength {
                    a: pair("a", "b"),
                    b: pair("a", "c"),
                },
            ),
            constraint(
                "angle",
                ConstraintKind::Angle {
                    a: pair("a", "b"),
                    b: pair("a", "c"),
                    degrees,
                },
            ),
        ],
    }
}
#[test]
fn directed_angles_independent_coordinates_scales_and_permutations() {
    for degrees in [-179.99, -120., -30., 0.01, 30., 90., 179.99, 180.] {
        for scale in [1e-6, 1., 1e4] {
            let s = triangle(degrees, scale);
            let r = solve(&s, scale * 1e-9).unwrap();
            assert!(r.is_fully_constrained(), "{degrees} {scale}: {r:?}");
            let expected = [
                scale * degrees.to_radians().cos(),
                scale * degrees.to_radians().sin(),
            ];
            for (actual, want) in r.points["c"].iter().zip(expected) {
                assert!((actual - want).abs() < scale * 1e-8);
            }
            assert!(r.max_angular_residual_radians <= ANGULAR_TOLERANCE_RADIANS);
            assert_eq!(r.schema_version, 2);
            let mut shuffled = s.clone();
            shuffled.points.reverse();
            shuffled.constraints.reverse();
            assert_eq!(r, solve(&shuffled, scale * 1e-9).unwrap());
            let json = serde_json::to_value(&r).unwrap();
            for residual in json["constraint_residuals"].as_array().unwrap() {
                assert_ne!(
                    residual.get("residual_mm").is_some(),
                    residual.get("residual_radians").is_some()
                );
            }
            assert_eq!(r, serde_json::from_value(json).unwrap());
        }
    }
}
#[test]
fn shared_endpoint_analytic_jacobians_match_finite_differences() {
    for kind in [
        ConstraintKind::EqualLength {
            a: pair("a", "b"),
            b: pair("a", "c"),
        },
        ConstraintKind::Parallel {
            a: pair("a", "b"),
            b: pair("a", "c"),
        },
        ConstraintKind::Perpendicular {
            a: pair("a", "b"),
            b: pair("a", "c"),
        },
        ConstraintKind::Angle {
            a: pair("a", "b"),
            b: pair("a", "c"),
            degrees: -25.,
        },
    ] {
        let s = Sketch {
            points: vec![
                point("a", [1., 2.]),
                point("b", [5., 3.]),
                point("c", [3., 6.]),
            ],
            constraints: vec![constraint("test", kind)],
        };
        let system = System::new(&s, 1e-7);
        let x = system.initial.clone();
        let eval = system.evaluate(&x);
        assert!(!eval.singular);
        for i in 0..x.len() {
            let mut plus = x.clone();
            let mut minus = x.clone();
            plus[i] += 1e-6;
            minus[i] -= 1e-6;
            let derivative =
                (system.evaluate(&plus).residuals[0] - system.evaluate(&minus).residuals[0]) / 2e-6;
            assert!(
                (derivative - eval.jacobian[0][i]).abs() < 1e-8,
                "{derivative} {:?}",
                eval.jacobian
            );
        }
    }
}
#[test]
fn parallelogram_and_perpendicular_rectangle() {
    for skew in [-4., 3.] {
        let s = Sketch {
            points: vec![
                point("a", [0., 0.]),
                point("b", [10., 0.]),
                point("c", [skew, 8.]),
                point("d", [9. + skew, 9.]),
            ],
            constraints: vec![
                constraint(
                    "a",
                    ConstraintKind::Fixed {
                        point: "a".into(),
                        position: [0., 0.],
                    },
                ),
                constraint(
                    "b",
                    ConstraintKind::Fixed {
                        point: "b".into(),
                        position: [10., 0.],
                    },
                ),
                constraint(
                    "c",
                    ConstraintKind::Fixed {
                        point: "c".into(),
                        position: [skew, 8.],
                    },
                ),
                constraint(
                    "parallel1",
                    ConstraintKind::Parallel {
                        a: pair("a", "b"),
                        b: pair("d", "c"),
                    },
                ),
                constraint(
                    "parallel2",
                    ConstraintKind::Parallel {
                        a: pair("a", "c"),
                        b: pair("b", "d"),
                    },
                ),
            ],
        };
        let r = solve(&s, 1e-7).unwrap();
        assert!(r.is_fully_constrained(), "{r:?}");
        assert!((r.points["d"][0] - (10. + skew)).abs() < 1e-6);
        assert!((r.points["d"][1] - 8.).abs() < 1e-6);
    }
    let mut s = triangle(90., 10.);
    s.constraints.last_mut().unwrap().kind = ConstraintKind::Perpendicular {
        a: pair("a", "b"),
        b: pair("c", "a"),
    };
    let r = solve(&s, 1e-7).unwrap();
    assert!(r.is_fully_constrained(), "{r:?}");
    assert!((r.points["c"][1] - 10.).abs() < 1e-6);
}
#[test]
fn angular_failures_remain_finite_and_do_not_use_linear_tolerance() {
    let mut s = triangle(30., 10.);
    s.constraints.push(constraint(
        "conflict",
        ConstraintKind::Angle {
            a: pair("a", "b"),
            b: pair("a", "c"),
            degrees: 40.,
        },
    ));
    let r = solve(&s, 1.).unwrap();
    assert!(!r.is_fully_constrained());
    assert!(r.max_angular_residual_radians > 1e-3);
    assert!(serde_json::to_string(&r)
        .unwrap()
        .contains("residual_radians"));
    let mut cut = triangle(180., 10.);
    cut.points[2].position = [10., 0.];
    let r = solve(&cut, 1e-7).unwrap();
    assert_eq!(r.status, SolveStatus::NumericalFailure);
    let mut short = triangle(30., 10.);
    short.points[2].position = [1e-15, 1e-15];
    let r = solve(&short, 1e-7).unwrap();
    assert_eq!(r.status, SolveStatus::NumericalFailure);
    assert!(r.max_angular_residual_radians.is_finite());
    let mut precision = triangle(30., 1.);
    for p in &mut precision.points {
        p.position[0] += 1e6;
    }
    for c in &mut precision.constraints {
        if let ConstraintKind::Fixed { position, .. } = &mut c.kind {
            position[0] += 1e6;
        }
    }
    let r = solve(&precision, 1.).unwrap();
    assert_eq!(r.status, SolveStatus::NumericalFailure);
}
#[test]
fn angular_json_strict_and_fractional() {
    let good = r#"{"id":"x","degrees":-32.125,"b":["a","c"],"kind":"angle","a":["a","b"]}"#;
    let c: Constraint = serde_json::from_str(good).unwrap();
    assert!(matches!(c.kind,ConstraintKind::Angle{degrees,..} if degrees == -32.125));
    for bad in [
        good.replace("-32.125", "null"),
        good.replace("\"degrees\":-32.125", "\"degrees\":1,\"degrees\":2"),
        good.replace("\"angle\"", "\"parallel\""),
        good.replace("[\"a\",\"b\"]", "\"a\""),
        good.replace("\"degrees\":-32.125", "\"extra\":2"),
        good.replace("[\"a\",\"c\"]", "[\"a\",\"c\",\"d\"]"),
    ] {
        assert!(serde_json::from_str::<Constraint>(&bad).is_err(), "{bad}");
    }
    for degrees in [-180., 181., f64::NAN] {
        assert!(validate(&triangle(degrees, 10.)).is_err());
    }
}

#[test]
fn near_parallel_cut_policies_and_shared_endpoint_cancellation() {
    for degrees in [-1e-8, 0., 1e-8, -180. + 1e-10] {
        let s = triangle(degrees, 20.);
        let r = solve(&s, 1e-7).unwrap();
        assert!(r.is_fully_constrained(), "{degrees}: {r:?}");
    }
    let mut cut = triangle(90., 20.);
    cut.points[2].position = [0., 20.];
    cut.constraints.last_mut().unwrap().kind = ConstraintKind::Parallel {
        a: pair("a", "b"),
        b: pair("a", "c"),
    };
    assert_eq!(
        solve(&cut, 1e-7).unwrap().status,
        SolveStatus::NumericalFailure
    );
    for kind in [
        ConstraintKind::EqualLength {
            a: pair("a", "b"),
            b: pair("b", "a"),
        },
        ConstraintKind::Parallel {
            a: pair("a", "b"),
            b: pair("b", "a"),
        },
        ConstraintKind::Angle {
            a: pair("a", "b"),
            b: pair("a", "b"),
            degrees: 0.,
        },
    ] {
        let s = Sketch {
            points: vec![point("a", [1., 2.]), point("b", [4., 6.])],
            constraints: vec![constraint("identity", kind)],
        };
        let system = System::new(&s, 1e-7);
        let e = system.evaluate(&system.initial);
        assert!(!e.singular);
        assert_eq!(e.residuals, vec![0.]);
        assert!(e.jacobian.iter().flatten().all(|v| *v == 0.));
        assert_eq!(solve(&s, 1e-7).unwrap().rank, 0);
    }
}
