use super::*;
fn ids(a: &str, b: &str) -> [String; 2] {
    [a.into(), b.into()]
}
fn constraint(id: &str, kind: ConstraintKind) -> Constraint {
    Constraint {
        id: id.into(),
        kind,
    }
}
fn fixture(angle: f64, scale: f64, offset: [f64; 2], radius: f64) -> Sketch {
    let (s, c) = angle.sin_cos();
    let position = |p: [f64; 2]| {
        [
            offset[0] + scale * (c * p[0] - s * p[1]),
            offset[1] + scale * (s * p[0] + c * p[1]),
        ]
    };
    let points = [
        ("a", [0., 0.]),
        ("b", [20., 0.]),
        ("d", [0., 20.]),
        ("c", [radius * 1.1, radius * 0.9]),
        ("r", [radius * 2.1, radius * 0.9]),
    ]
    .into_iter()
    .map(|(id, p)| SketchPoint {
        id: id.into(),
        position: position(p),
    })
    .collect();
    Sketch {
        points,
        constraints: vec![
            constraint(
                "a",
                ConstraintKind::Fixed {
                    point: "a".into(),
                    position: position([0., 0.]),
                },
            ),
            constraint(
                "b",
                ConstraintKind::Fixed {
                    point: "b".into(),
                    position: position([20., 0.]),
                },
            ),
            constraint(
                "d",
                ConstraintKind::Fixed {
                    point: "d".into(),
                    position: position([0., 20.]),
                },
            ),
            constraint(
                "radius",
                ConstraintKind::Distance {
                    a: "c".into(),
                    b: "r".into(),
                    value: radius * scale,
                },
            ),
            constraint(
                "radius_direction",
                ConstraintKind::Parallel {
                    a: ids("a", "b"),
                    b: ids("c", "r"),
                },
            ),
            constraint(
                "horizontal_tangent",
                ConstraintKind::CircleLineTangent {
                    circle: ids("c", "r"),
                    line: ids("a", "b"),
                    side: LineSide::Left,
                },
            ),
            constraint(
                "vertical_tangent",
                ConstraintKind::CircleLineTangent {
                    circle: ids("c", "r"),
                    line: ids("a", "d"),
                    side: LineSide::Right,
                },
            ),
        ],
    }
}
#[test]
fn independent_offset_line_intersection_rotation_scale_and_radius_edits() {
    for angle in [-2.3, 0., 0.7, 2.9] {
        for scale in [1e-4, 1., 100.] {
            for radius in [2., 3.5] {
                let offset = [100. * scale, -50. * scale];
                let sketch = fixture(angle, scale, offset, radius);
                let report = solve(&sketch, scale * 1e-7).unwrap();
                assert!(
                    report.is_fully_constrained(),
                    "{angle} {scale} {radius}: {report:?}"
                );
                let (s, c) = angle.sin_cos();
                let expected = [
                    offset[0] + scale * radius * (c - s),
                    offset[1] + scale * radius * (s + c),
                ];
                for (a, b) in report.points["c"].iter().zip(expected) {
                    assert!((a - b).abs() < scale * 1e-6);
                }
                for (id, end, sign) in [
                    ("horizontal_tangent", "b", 1.),
                    ("vertical_tangent", "d", -1.),
                ] {
                    let a = report.points["a"];
                    let b = report.points[end];
                    let center = report.points["c"];
                    let rim = report.points["r"];
                    let u = [b[0] - a[0], b[1] - a[1]];
                    let w = [center[0] - a[0], center[1] - a[1]];
                    let actual = ((u[0] * w[1] - u[1] * w[0]) / u[0].hypot(u[1])
                        - sign * (rim[0] - center[0]).hypot(rim[1] - center[1]))
                    .abs();
                    let row = report
                        .constraint_residuals
                        .iter()
                        .find(|r| r.id == id)
                        .unwrap();
                    assert!(row.residual_radians.is_none());
                    assert!((row.residual_mm.unwrap() - actual).abs() < scale * 1e-11);
                    assert!(actual <= report.tolerance_mm);
                }
                let mut reordered = sketch.clone();
                reordered.points.reverse();
                reordered.constraints.reverse();
                assert_eq!(report, solve(&reordered, scale * 1e-7).unwrap());
            }
        }
    }
}
#[test]
fn shared_endpoint_jacobian_and_reversed_side_are_consistent() {
    for (circle, line) in [
        (ids("c", "r"), ids("a", "b")),
        (ids("a", "r"), ids("a", "b")),
        (ids("c", "b"), ids("a", "b")),
        (ids("b", "a"), ids("a", "b")),
    ] {
        let mut sketch = fixture(0.7, 1., [0., 0.], 2.);
        sketch.constraints = vec![constraint(
            "t",
            ConstraintKind::CircleLineTangent {
                circle,
                line,
                side: LineSide::Left,
            },
        )];
        let system = System::new(&sketch, 1e-7);
        let x = system.initial.clone();
        let eval = system.evaluate(&x);
        assert!(!eval.singular);
        for i in 0..x.len() {
            let mut p = x.clone();
            let mut m = x.clone();
            p[i] += 1e-6;
            m[i] -= 1e-6;
            let derivative =
                (system.evaluate(&p).residuals[0] - system.evaluate(&m).residuals[0]) / 2e-6;
            assert!(
                (derivative - eval.jacobian[0][i]).abs() < 1e-8,
                "{i}: {derivative} vs {}",
                eval.jacobian[0][i]
            );
        }
    }
    let mut sketch = fixture(0.7, 1., [30., -20.], 2.);
    let original = solve(&sketch, 1e-7).unwrap();
    for c in &mut sketch.constraints {
        if let ConstraintKind::CircleLineTangent { line, side, .. } = &mut c.kind {
            line.swap(0, 1);
            *side = match side {
                LineSide::Left => LineSide::Right,
                LineSide::Right => LineSide::Left,
            };
        }
    }
    let reversed = solve(&sketch, 1e-7).unwrap();
    assert!(reversed.is_fully_constrained());
    for (id, p) in &original.points {
        for (a, b) in p.iter().zip(reversed.points[id]) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
#[test]
fn tangent_failures_and_supporting_line_not_segment_contract() {
    let mut s = fixture(0., 1., [0., 0.], 2.);
    s.constraints.retain(|c| c.id != "vertical_tangent");
    assert_eq!(
        solve(&s, 1e-7).unwrap().status,
        SolveStatus::Underconstrained
    );
    s = fixture(0., 1., [0., 0.], 2.);
    s.constraints.push(constraint(
        "opposite",
        ConstraintKind::CircleLineTangent {
            circle: ids("c", "r"),
            line: ids("a", "b"),
            side: LineSide::Right,
        },
    ));
    let r = solve(&s, 1e-7).unwrap();
    assert!(!r.is_fully_constrained());
    assert!(r.max_residual_mm > 1e-3);
    assert!(serde_json::to_string(&r).is_ok());
    for (point, position) in [("b", [0., 0.]), ("b", [1e-15, 0.]), ("r", [2.2, 1.8])] {
        let mut s = fixture(0., 1., [0., 0.], 2.);
        s.points
            .iter_mut()
            .find(|p| p.id == point)
            .unwrap()
            .position = position;
        let r = solve(&s, 1e-7).unwrap();
        assert_eq!(r.status, SolveStatus::NumericalFailure, "{r:?}");
        assert!(r.max_residual_mm.is_finite());
    }
    // Contact lies at x=2 on the infinite line although its segment ends at 1.
    let mut s = fixture(0., 1., [0., 0.], 2.);
    s.points.iter_mut().find(|p| p.id == "b").unwrap().position = [1., 0.];
    if let ConstraintKind::Fixed { position, .. } = &mut s.constraints[1].kind {
        *position = [1., 0.];
    }
    assert!(solve(&s, 1e-7).unwrap().is_fully_constrained());
}
#[test]
fn tangent_json_and_references_are_strict() {
    let good = r#"{"id":"t","kind":"circle_line_tangent","circle":["c","r"],"line":["a","b"],"side":"left"}"#;
    let c: Constraint = serde_json::from_str(good).unwrap();
    assert_eq!(
        serde_json::from_str::<Constraint>(&serde_json::to_string(&c).unwrap()).unwrap(),
        c
    );
    for bad in [
        good.replace("left", "either"),
        good.replace("\"side\":\"left\"", "\"side\":\"left\",\"side\":\"right\""),
        good.replace(",\"side\":\"left\"", ""),
        good.replace("\"circle_line_tangent\"", "\"parallel\""),
        good.replace("\"side\":\"left\"", "\"side\":\"left\",\"value\":2"),
        good.replace("[\"c\",\"r\"]", "[\"c\"]"),
    ] {
        assert!(serde_json::from_str::<Constraint>(&bad).is_err(), "{bad}");
    }
    for circle in [ids("c", "c"), ids("missing", "r")] {
        let mut s = fixture(0., 1., [0., 0.], 2.);
        s.constraints.push(constraint(
            "bad",
            ConstraintKind::CircleLineTangent {
                circle,
                line: ids("a", "b"),
                side: LineSide::Left,
            },
        ));
        assert!(validate(&s).is_err());
    }
}

#[test]
fn distant_contact_direction_uncertainty_fails_closed() {
    let points: Vec<_> = [
        ("a", [1e6, 0.]),
        ("b", [1e6 + 0.001, 0.]),
        ("c", [1e6 + 100., 2.]),
        ("r", [1e6 + 102., 2.]),
    ]
    .into_iter()
    .map(|(id, position)| SketchPoint {
        id: id.into(),
        position,
    })
    .collect();
    let mut constraints: Vec<_> = points
        .iter()
        .map(|p| {
            constraint(
                &p.id,
                ConstraintKind::Fixed {
                    point: p.id.clone(),
                    position: p.position,
                },
            )
        })
        .collect();
    constraints.push(constraint(
        "tangent",
        ConstraintKind::CircleLineTangent {
            circle: ids("c", "r"),
            line: ids("a", "b"),
            side: LineSide::Left,
        },
    ));
    let sketch = Sketch {
        points,
        constraints,
    };
    // Ordinary coordinate quantization fits this tolerance, but extrapolating
    // a 0.001 mm line direction to contact 100 mm away does not.
    let system = System::new(&sketch, 1e-3);
    assert!(system.world_scale * 256. * f64::EPSILON < 1e-3 / 16.);
    let evaluation = system.evaluate(&system.initial);
    assert!(evaluation.singular);
    let report = solve(&sketch, 1e-3).unwrap();
    assert_eq!(report.status, SolveStatus::NumericalFailure);
    assert!(report.max_residual_mm.is_finite());
    assert!(serde_json::to_string(&report).is_ok());
}
