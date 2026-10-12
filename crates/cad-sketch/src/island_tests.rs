use super::*;
fn rod(prefix: &str, offset: f64, length: f64) -> Sketch {
    let a = format!("{prefix}_a");
    let b = format!("{prefix}_b");
    Sketch {
        points: vec![
            SketchPoint {
                id: a.clone(),
                position: [offset, 0.],
            },
            SketchPoint {
                id: b.clone(),
                position: [offset + length * 0.9, length * 0.1],
            },
        ],
        constraints: vec![
            Constraint {
                id: format!("{prefix}_anchor"),
                kind: ConstraintKind::Fixed {
                    point: a.clone(),
                    position: [offset, 0.],
                },
            },
            Constraint {
                id: format!("{prefix}_level"),
                kind: ConstraintKind::Horizontal {
                    a: a.clone(),
                    b: b.clone(),
                },
            },
            Constraint {
                id: format!("{prefix}_length"),
                kind: ConstraintKind::Distance {
                    a,
                    b,
                    value: length,
                },
            },
        ],
    }
}
fn merge(mut a: Sketch, b: Sketch) -> Sketch {
    a.points.extend(b.points);
    a.constraints.extend(b.constraints);
    a
}
#[test]
fn many_disconnected_patterns_have_deterministic_complete_reports() {
    let mut s = Sketch {
        points: vec![],
        constraints: vec![],
    };
    for i in 0..40 {
        s = merge(s, rod(&format!("part{i:02}"), i as f64 * 30., 5.));
    }
    let report = solve(&s, 1e-7).unwrap();
    assert!(report.is_fully_constrained());
    assert_eq!(report.schema_version, 3);
    assert_eq!(report.islands.len(), 40);
    assert_eq!(report.rank, 160);
    assert_eq!(report.degrees_of_freedom, 0);
    assert_eq!(report.redundant_equations, 0);
    assert_eq!(
        report.iterations,
        report.islands.iter().map(|i| i.iterations).sum::<usize>()
    );
    assert_eq!(
        report.condition_bound,
        report
            .islands
            .iter()
            .map(|i| i.condition_bound.unwrap())
            .reduce(f64::max)
    );
    for i in 0..40 {
        let id = format!("part{i:02}_b");
        assert!((report.points[&id][0] - (i as f64 * 30. + 5.)).abs() < 1e-7);
    }
    assert!(report.islands.windows(2).all(|p| p[0].id < p[1].id));
    assert!(report
        .constraint_residuals
        .windows(2)
        .all(|p| p[0].id < p[1].id));
    s.points.reverse();
    s.constraints.reverse();
    assert_eq!(report, solve(&s, 1e-7).unwrap());
    let serialized = serde_json::to_string(&report).unwrap();
    let decoded: SolveReport = serde_json::from_str(&serialized).unwrap();
    assert_eq!(decoded.schema_version, 3);
    assert_eq!(decoded.islands.len(), 40);
    assert_eq!(decoded.rank, report.rank);
    for (id, p) in &report.points {
        for (a, b) in p.iter().zip(decoded.points[id]) {
            assert!((a - b).abs() < 1e-12);
        }
    }
}
#[test]
fn independent_scales_use_local_origins_and_preserve_refined_results() {
    let small = rod("tiny", 0., 1e-5);
    let large = rod("large", 1e6, 1e4);
    let a = solve_refined(&small, 1e-5).unwrap();
    let b = solve_refined(&large, 1e-5).unwrap();
    assert!(a.is_fully_constrained() && b.is_fully_constrained());
    let joined = solve_refined(&merge(small, large), 1e-5).unwrap();
    assert!(joined.is_fully_constrained());
    for (id, p) in a.points.iter().chain(&b.points) {
        assert_eq!(joined.points[id], *p);
    }
    assert_eq!(joined.islands[0], b.islands[0]);
    assert_eq!(joined.islands[1], a.islands[0]);
}
#[test]
fn failures_keep_other_islands_and_status_severity() {
    let valid = rod("good", 0., 5.);
    let mut bad = rod("singular", 0., 5.);
    bad.points[1].position = [0., 0.];
    let mut conflict = rod("conflict", 0., 5.);
    conflict.constraints.push(Constraint {
        id: "contradiction".into(),
        kind: ConstraintKind::Distance {
            a: "conflict_a".into(),
            b: "conflict_b".into(),
            value: 8.,
        },
    });
    let free = Sketch {
        points: vec![SketchPoint {
            id: "free".into(),
            position: [7., 3.],
        }],
        constraints: vec![],
    };
    let partial = solve(&merge(merge(valid.clone(), conflict), free), 1e-7).unwrap();
    assert_eq!(partial.status, SolveStatus::NotConverged);
    assert_eq!(partial.islands.len(), 3);
    assert!(partial.condition_bound.is_none());
    let s = merge(
        merge(valid, bad),
        Sketch {
            points: vec![SketchPoint {
                id: "free".into(),
                position: [7., 3.],
            }],
            constraints: vec![],
        },
    );
    let report = solve(&s, 1e-7).unwrap();
    assert_eq!(report.status, SolveStatus::NumericalFailure);
    assert_eq!(report.islands.len(), 3);
    assert_eq!(report.points.len(), 5);
    assert_eq!(
        report
            .islands
            .iter()
            .find(|i| i.id == "good_a")
            .unwrap()
            .status,
        SolveStatus::FullyConstrained
    );
    assert_eq!(
        report
            .islands
            .iter()
            .find(|i| i.id == "free")
            .unwrap()
            .degrees_of_freedom,
        2
    );
    assert!(report
        .islands
        .iter()
        .all(|i| i.iterations <= MAX_ITERATIONS));
    assert!(serde_json::to_string(&report).is_ok());
}
#[test]
fn all_structural_and_island_budgets_preflight_before_numerical_work() {
    let mut oversized = Sketch {
        points: (0..33)
            .map(|i| SketchPoint {
                id: format!("p{i:02}"),
                position: [0., 0.],
            })
            .collect(),
        constraints: vec![],
    };
    for i in 1..33 {
        oversized.constraints.push(Constraint {
            id: format!("c{i:02}"),
            kind: ConstraintKind::Distance {
                a: "p00".into(),
                b: format!("p{i:02}"),
                value: 1.,
            },
        });
    }
    // Initial directions are singular, but structural island rejection comes first.
    assert!(
        matches!(solve(&oversized,1e-7),Err(SketchError::IslandResourceLimit{island,points:33,constraints:32})if island=="p00")
    );
    let mut repeated = Sketch {
        points: vec![SketchPoint {
            id: "p".into(),
            position: [0., 0.],
        }],
        constraints: vec![],
    };
    for i in 0..129 {
        repeated.constraints.push(Constraint {
            id: format!("f{i}"),
            kind: ConstraintKind::Fixed {
                point: "p".into(),
                position: [0., 0.],
            },
        });
    }
    assert!(matches!(
        validate(&repeated),
        Err(SketchError::IslandResourceLimit {
            points: 1,
            constraints: 129,
            ..
        })
    ));
    repeated.constraints.pop();
    assert!(solve(&repeated, 1e-7).unwrap().is_fully_constrained());
    let empty = Sketch {
        points: vec![],
        constraints: vec![],
    };
    assert!(matches!(validate(&empty), Err(SketchError::ResourceLimit)));
    let too_many = Sketch {
        points: (0..257)
            .map(|i| SketchPoint {
                id: i.to_string(),
                position: [0., 0.],
            })
            .collect(),
        constraints: vec![],
    };
    assert!(matches!(
        validate(&too_many),
        Err(SketchError::ResourceLimit)
    ));
    oversized.constraints.push(Constraint {
        id: "missing".into(),
        kind: ConstraintKind::Fixed {
            point: "absent".into(),
            position: [0., 0.],
        },
    });
    assert!(matches!(
        validate(&oversized),
        Err(SketchError::MissingPoint { .. })
    ));
}
#[test]
fn every_multi_point_constraint_connects_all_its_references() {
    for kind in [
        ConstraintKind::Coincident {
            a: "a".into(),
            b: "b".into(),
        },
        ConstraintKind::Vertical {
            a: "a".into(),
            b: "b".into(),
        },
        ConstraintKind::EqualLength {
            a: ["a".into(), "b".into()],
            b: ["c".into(), "d".into()],
        },
        ConstraintKind::Parallel {
            a: ["a".into(), "b".into()],
            b: ["c".into(), "d".into()],
        },
        ConstraintKind::Perpendicular {
            a: ["a".into(), "b".into()],
            b: ["c".into(), "d".into()],
        },
        ConstraintKind::Angle {
            a: ["a".into(), "b".into()],
            b: ["c".into(), "d".into()],
            degrees: 30.,
        },
        ConstraintKind::CircleLineTangent {
            circle: ["a".into(), "b".into()],
            line: ["c".into(), "d".into()],
            side: LineSide::Left,
        },
    ] {
        let expected = if matches!(
            kind,
            ConstraintKind::Coincident { .. } | ConstraintKind::Vertical { .. }
        ) {
            3
        } else {
            1
        };
        let sketch = Sketch {
            points: ["a", "b", "c", "d"]
                .into_iter()
                .enumerate()
                .map(|(i, id)| SketchPoint {
                    id: id.into(),
                    position: [i as f64, 0.],
                })
                .collect(),
            constraints: vec![Constraint {
                id: "link".into(),
                kind,
            }],
        };
        validate(&sketch).unwrap();
        let partition = islands::partition(&sketch).unwrap();
        assert_eq!(partition.len(), expected);
    }
}
