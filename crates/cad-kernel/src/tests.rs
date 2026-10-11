use super::*;

fn fixture() -> OperationResult {
    extrude_rectangle(80.0, 40.0, 6.0, Tolerance::default()).unwrap()
}

#[test]
fn extrusion_has_expected_boundary_and_analytic_measurements() {
    let result = fixture();
    let model = &result.model;
    assert_eq!(
        (
            model.vertices.len(),
            model.edges.len(),
            model.loops.len(),
            model.faces.len()
        ),
        (8, 12, 6, 6)
    );
    assert_eq!((model.shells.len(), model.solids.len()), (1, 1));
    assert_eq!(
        model.bounds(),
        Bounds {
            min: [0.0; 3],
            max: [80.0, 40.0, 6.0]
        }
    );
    assert_eq!(model.volume(), 19_200.0);
    assert!(model.validate(Tolerance::default()).valid);
    assert_eq!(result.history.generated_faces["end_cap"], vec![FaceId(1)]);
    assert_eq!(result.history.generated_faces["walls"].len(), 4);
}

#[test]
fn serialization_preserves_model_and_history() {
    let original = fixture();
    let restored: OperationResult =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    assert_eq!(original, restored);
    assert!(restored.model.validate(Tolerance::default()).valid);
}

#[test]
fn input_policy_rejects_invalid_and_unrepresentable_geometry() {
    let tolerance = Tolerance::default();
    for invalid in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        tolerance.linear,
    ] {
        assert!(extrude_rectangle(invalid, 1.0, 1.0, tolerance).is_err());
        assert!(extrude_rectangle(1.0, invalid, 1.0, tolerance).is_err());
        assert!(extrude_rectangle(1.0, 1.0, invalid, tolerance).is_err());
    }
    for linear in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            extrude_rectangle(1.0, 1.0, 1.0, Tolerance { linear }),
            Err(KernelError::InvalidTolerance)
        ));
    }
    assert!(matches!(
        extrude_rectangle(1e200, 1e200, 1.0, tolerance),
        Err(KernelError::NumericRange)
    ));
    assert!(matches!(
        extrude_rectangle(1e-200, 1e-200, 1e-200, Tolerance { linear: 1e-210 }),
        Err(KernelError::NumericRange)
    ));
}

#[test]
fn subnormal_geometry_and_integration_terms_are_rejected() {
    for (width, depth, height, linear) in [
        (1e-300, 1e-20, 1.0, 1e-310),
        (2e-300, 1e-23, 1.0, 1e-310),
        // Total volume is normal, but dividing a tetrahedral determinant by six
        // would enter the subnormal range.
        (f64::MIN_POSITIVE * 4.0, 1.0, 1.0, f64::MIN_POSITIVE / 16.0),
        // A subnormal face area cannot be rescued by a very large height.
        (1e-200, 1e-110, 1e100, 1e-210),
    ] {
        assert!(matches!(
            extrude_rectangle(width, depth, height, Tolerance { linear }),
            Err(KernelError::NumericRange)
        ));
    }
    let tolerance = Tolerance {
        linear: f64::MIN_POSITIVE / 16.0,
    };
    let width = f64::MIN_POSITIVE * 8.0;
    let model = extrude_rectangle(width, 1.0, 1.0, tolerance).unwrap().model;
    assert!((model.volume() / width - 1.0).abs() < 1e-14);
    assert!(model.validate(tolerance).valid);

    // Deserialization/mutation must not bypass the numerical contract. Scale a
    // valid box and its support geometry until its face areas become subnormal.
    let mut model = fixture().model;
    for vertex in &mut model.vertices {
        vertex.position = vertex.position.map(|coordinate| coordinate * 1e-160);
    }
    for edge in &mut model.edges {
        edge.line.origin = edge.line.origin.map(|coordinate| coordinate * 1e-160);
    }
    for face in &mut model.faces {
        face.plane.origin = face.plane.origin.map(|coordinate| coordinate * 1e-160);
    }
    let report = model.validate(Tolerance { linear: 1e-170 });
    assert!(!report.valid);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "numeric_range"));
}

#[test]
fn volume_is_derived_from_geometry_and_translation_invariant() {
    let mut model = fixture().model;
    for vertex in &mut model.vertices {
        for coordinate in &mut vertex.position {
            *coordinate *= 2.0;
        }
    }
    assert_eq!(model.volume(), 8.0 * 19_200.0);
    // Supporting geometry is intentionally stale; validation must catch it.
    assert!(!model.validate(Tolerance::default()).valid);
    let mut model = fixture().model;
    for vertex in &mut model.vertices {
        for coordinate in &mut vertex.position {
            *coordinate += 1024.0;
        }
    }
    for edge in &mut model.edges {
        for coordinate in &mut edge.line.origin {
            *coordinate += 1024.0;
        }
    }
    for face in &mut model.faces {
        for coordinate in &mut face.plane.origin {
            *coordinate += 1024.0;
        }
    }
    assert_eq!(model.volume(), 19_200.0);
    assert!(model.validate(Tolerance::default()).valid);
}

#[test]
fn parameter_sweep_agrees_with_analytic_volume() {
    for width in [1e-5, 0.1, 1.0, 80.0, 1e6] {
        for depth in [1e-5, 0.5, 40.0, 1e6] {
            for height in [1e-5, 1.0, 6.0, 1e6] {
                let model = extrude_rectangle(width, depth, height, Tolerance::default())
                    .unwrap()
                    .model;
                let expected = width * depth * height;
                assert!((model.volume() - expected).abs() <= expected * 1e-14);
            }
        }
    }
}

#[test]
fn corrupted_topology_is_rejected_without_panicking() {
    let mutations: Vec<fn(&mut ModelSnapshot)> = vec![
        |m| m.edges[0].vertices[0] = VertexId(usize::MAX),
        |m| m.loops[0].edges[0].edge = EdgeId(usize::MAX),
        |m| m.faces[0].loops[0] = LoopId(usize::MAX),
        |m| m.shells[0].faces[0] = FaceId(usize::MAX),
        |m| m.solids[0].shells[0] = ShellId(usize::MAX),
        |m| m.loops[0].edges[0].reversed = !m.loops[0].edges[0].reversed,
        |m| {
            m.loops[0].edges.pop();
        },
        |m| m.faces[0].plane.normal = [0.0, 0.0, 1.0],
        |m| m.vertices[0].position[2] = 0.5,
        |m| m.vertices[0].position[0] = f64::NAN,
        |m| m.edges[0].line.direction = [0.0; 3],
        |m| m.shells[0].faces[1] = m.shells[0].faces[0],
        |m| m.loops[0].edges.clear(),
        |m| m.vertices.clear(),
        |m| m.faces.clear(),
    ];
    for mutate in mutations {
        let mut model = fixture().model;
        mutate(&mut model);
        let report = model.validate(Tolerance::default());
        assert!(!report.valid, "corrupt model unexpectedly accepted");
        assert!(!report.issues.is_empty());
    }
}

fn block(size: [f64; 3], offset: [f64; 3]) -> ModelSnapshot {
    let model = extrude_rectangle(size[0], size[1], size[2], Tolerance::default())
        .unwrap()
        .model;
    translate(&model, offset, Tolerance::default())
        .unwrap()
        .model
}

fn combine(a: &ModelSnapshot, b: &ModelSnapshot, operation: BooleanOp) -> ModelSnapshot {
    let result = boolean(a, b, operation, Tolerance::default()).unwrap();
    assert!(result.model.validate(Tolerance::default()).valid);
    result.model
}

#[test]
fn overlapping_box_booleans_have_analytic_volumes() {
    let a = block([2.0; 3], [0.0; 3]);
    let b = block([2.0; 3], [1.0, 0.0, 0.0]);
    for (operation, expected) in [
        (BooleanOp::Union, 12.0),
        (BooleanOp::Difference, 4.0),
        (BooleanOp::Intersection, 4.0),
    ] {
        let model = combine(&a, &b, operation);
        assert!((model.volume() - expected).abs() < 1e-12);
        assert_eq!(model.solids.len(), 1);
    }
}

#[test]
fn orthogonal_cut_builds_l_bracket_and_open_pocket() {
    let plate = block([3.0, 3.0, 1.0], [0.0; 3]);
    let corner = block([2.0, 2.0, 1.0], [1.0, 1.0, 0.0]);
    let bracket = combine(&plate, &corner, BooleanOp::Difference);
    assert!((bracket.volume() - 5.0).abs() < 1e-12);
    assert_eq!(bracket.bounds().max, [3.0, 3.0, 1.0]);
    let body = block([4.0, 4.0, 3.0], [0.0; 3]);
    let tool = block([2.0, 2.0, 3.0], [1.0, 1.0, 2.0]);
    let pocket = combine(&body, &tool, BooleanOp::Difference);
    assert!((pocket.volume() - 44.0).abs() < 1e-12);
    assert_eq!(pocket.shells.len(), 1);
}

#[test]
fn through_hole_has_genus_one_and_survives_subsequent_operations() {
    let body = block([4.0, 4.0, 2.0], [0.0; 3]);
    let tool = block([2.0, 2.0, 4.0], [1.0, 1.0, -1.0]);
    let frame = combine(&body, &tool, BooleanOp::Difference);
    assert!((frame.volume() - 24.0).abs() < 1e-12);
    assert_eq!(
        frame.vertices.len() as isize - frame.edges.len() as isize + frame.faces.len() as isize,
        0
    );
    let restored = combine(
        &frame,
        &combine(&body, &tool, BooleanOp::Intersection),
        BooleanOp::Union,
    );
    assert!((restored.volume() - 32.0).abs() < 1e-12);
    let identical = combine(&frame, &frame, BooleanOp::Intersection);
    assert_eq!(identical, frame);
}

#[test]
fn disjoint_components_and_empty_results_are_valid() {
    let a = block([2.0; 3], [0.0; 3]);
    let b = block([2.0; 3], [4.0, 0.0, 0.0]);
    let union = combine(&a, &b, BooleanOp::Union);
    assert_eq!(union.solids.len(), 2);
    assert_eq!(union.shells.len(), 2);
    assert!((union.volume() - 16.0).abs() < 1e-12);
    let empty = combine(&a, &b, BooleanOp::Intersection);
    assert!(empty.solids.is_empty());
    assert!(empty.vertices.is_empty());
    assert_eq!(empty.volume(), 0.0);
    assert_eq!(combine(&a, &a, BooleanOp::Difference), empty);
    assert_eq!(combine(&empty, &empty, BooleanOp::Union), empty);
    assert!((combine(&union, &empty, BooleanOp::Union).volume() - 16.0).abs() < 1e-12);
    assert!(combine(&union, &empty, BooleanOp::Intersection)
        .solids
        .is_empty());
    assert_eq!(
        translate(&empty, [1.0, 2.0, 3.0], Tolerance::default())
            .unwrap()
            .model,
        empty
    );
}

#[test]
fn cavity_and_nonmanifold_contacts_are_explicitly_unsupported() {
    let body = block([4.0; 3], [0.0; 3]);
    let cavity = block([2.0; 3], [1.0; 3]);
    assert!(
        matches!(boolean(&body,&cavity,BooleanOp::Difference,Tolerance::default()),Err(KernelError::Unsupported(message)) if message.contains("cavities"))
    );
    let a = block([2.0; 3], [0.0; 3]);
    for offset in [[2.0, 2.0, 0.0], [2.0, 2.0, 2.0]] {
        let touching = block([2.0; 3], offset);
        assert!(matches!(
            boolean(&a, &touching, BooleanOp::Union, Tolerance::default()),
            Err(KernelError::Unsupported(_))
        ));
        assert!(combine(&a, &touching, BooleanOp::Intersection)
            .solids
            .is_empty());
    }
    let face_touching = block([2.0; 3], [2.0, 0.0, 0.0]);
    assert!((combine(&a, &face_touching, BooleanOp::Union).volume() - 16.0).abs() < 1e-12);
}

#[test]
fn translation_is_immutable_repeatable_and_checks_numeric_range() {
    let original = block([2.0, 3.0, 4.0], [0.0; 3]);
    let moved = translate(&original, [5.0, -6.0, 7.0], Tolerance::default())
        .unwrap()
        .model;
    assert_eq!(
        moved.bounds(),
        Bounds {
            min: [5.0, -6.0, 7.0],
            max: [7.0, -3.0, 11.0]
        }
    );
    let restored = translate(&moved, [-5.0, 6.0, -7.0], Tolerance::default())
        .unwrap()
        .model;
    assert_eq!(restored, original);
    assert_eq!(original.bounds().min, [0.0; 3]);
    assert!(translate(&original, [f64::NAN, 0.0, 0.0], Tolerance::default()).is_err());
    assert!(translate(&original, [1e100, 0.0, 0.0], Tolerance::default()).is_err());
    assert!(matches!(
        translate(&original, [0.0; 3], Tolerance { linear: 0.0 }),
        Err(KernelError::InvalidTolerance)
    ));
}

#[test]
fn boolean_rejects_invalid_inputs_and_unresolved_near_coincidence() {
    let a = block([2.0; 3], [0.0; 3]);
    let shifted = block([2.0; 3], [1e-8, 0.0, 0.0]);
    assert!(
        matches!(boolean(&a,&shifted,BooleanOp::Union,Tolerance::default()),Err(KernelError::Unsupported(message)) if message.contains("tolerance"))
    );
    let mut invalid = a.clone();
    invalid.loops[0].edges[0].edge = EdgeId(usize::MAX);
    assert!(matches!(
        boolean(&a, &invalid, BooleanOp::Union, Tolerance::default()),
        Err(KernelError::InvalidInput(_))
    ));
}

#[test]
fn boolean_parameter_sweep_matches_independent_box_overlap_formula() {
    let a = block([2.0; 3], [0.0; 3]);
    for x in [-1.0, 0.5, 3.0] {
        for y in [-1.0, 0.5, 3.0] {
            for z in [-1.0, 0.5, 3.0] {
                let offset = [x, y, z];
                let b = block([2.0; 3], offset);
                let overlap = offset
                    .into_iter()
                    .map(|v: f64| (2.0_f64.min(v + 2.0) - 0.0_f64.max(v)).max(0.0))
                    .product::<f64>();
                for (op, expected) in [
                    (BooleanOp::Union, 16.0 - overlap),
                    (BooleanOp::Difference, 8.0 - overlap),
                    (BooleanOp::Intersection, overlap),
                ] {
                    let actual = combine(&a, &b, op).volume();
                    assert!(
                        (actual - expected).abs() < 1e-11,
                        "{offset:?} {op:?}: {actual} vs {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn coordinate_arrangement_resource_limits_fail_before_classification() {
    // Independently assemble valid disconnected inputs. A diagonal arrangement
    // creates many distinct planes even though its actual solid volume is small.
    let mut many = ModelSnapshot {
        vertices: vec![],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    let empty = many.clone();
    for i in 0..32 {
        let mut part = block([2.0; 3], [3.0 * i as f64; 3]);
        let (v, e, l, f, s) = (
            many.vertices.len(),
            many.edges.len(),
            many.loops.len(),
            many.faces.len(),
            many.shells.len(),
        );
        for edge in &mut part.edges {
            for id in &mut edge.vertices {
                id.0 += v;
            }
        }
        for boundary in &mut part.loops {
            for edge_use in &mut boundary.edges {
                edge_use.edge.0 += e;
            }
        }
        for face in &mut part.faces {
            for id in &mut face.loops {
                id.0 += l;
            }
        }
        for shell in &mut part.shells {
            for id in &mut shell.faces {
                id.0 += f;
            }
        }
        for solid in &mut part.solids {
            for id in &mut solid.shells {
                id.0 += s;
            }
        }
        many.vertices.extend(part.vertices);
        many.edges.extend(part.edges);
        many.loops.extend(part.loops);
        many.faces.extend(part.faces);
        many.shells.extend(part.shells);
        many.solids.extend(part.solids);
        if i == 27 {
            assert!(many.validate(Tolerance::default()).valid);
            assert!(
                matches!(boolean(&many,&empty,BooleanOp::Union,Tolerance::default()),Err(KernelError::ResourceLimit(message)) if message.contains("classification"))
            );
        }
    }
    assert!(many.validate(Tolerance::default()).valid);
    assert!(
        matches!(boolean(&many,&empty,BooleanOp::Union,Tolerance::default()),Err(KernelError::ResourceLimit(message)) if message.contains("grid"))
    );
}

fn move_component_unchecked(model: &mut ModelSnapshot, solid: usize, offset: [f64; 3]) {
    let mut vertices = std::collections::BTreeSet::new();
    let mut edges = std::collections::BTreeSet::new();
    for shell in &model.solids[solid].shells {
        for face_id in &model.shells[shell.0].faces {
            let face = &mut model.faces[face_id.0];
            for (axis, delta) in offset.into_iter().enumerate() {
                face.plane.origin[axis] += delta;
            }
            for loop_id in &face.loops {
                for edge_use in &model.loops[loop_id.0].edges {
                    edges.insert(edge_use.edge);
                    vertices.extend(model.edges[edge_use.edge.0].vertices);
                }
            }
        }
    }
    for id in edges {
        for (axis, delta) in offset.into_iter().enumerate() {
            model.edges[id.0].line.origin[axis] += delta;
        }
    }
    for id in vertices {
        for (axis, delta) in offset.into_iter().enumerate() {
            model.vertices[id.0].position[axis] += delta;
        }
    }
}

#[test]
fn geometric_validation_rejects_crossings_nested_solids_and_unshared_contacts() {
    let a = block([10.0; 3], [0.0; 3]);
    for (size, offset, expected_code) in [
        ([10.0; 3], [5.0, 0.0, 0.0], "geometric_intersection"),
        ([10.0; 3], [5.0; 3], "geometric_intersection"),
        ([2.0; 3], [2.0; 3], "overlapping_solids"),
        ([2.0; 3], [10.0, 2.0, 2.0], "geometric_intersection"),
        ([2.0; 3], [10.0, 10.0, 2.0], "geometric_intersection"),
        ([2.0; 3], [10.0; 3], "geometric_intersection"),
    ] {
        let b = block(size, [20.0; 3]);
        let mut combined = combine(&a, &b, BooleanOp::Union);
        move_component_unchecked(&mut combined, 1, offset.map(|coordinate| coordinate - 20.0));
        let report = combined.validate(Tolerance::default());
        assert!(!report.valid, "incorrectly accepted offset {offset:?}");
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == expected_code),
            "{report:?}"
        );
        assert!(matches!(
            boolean(&combined, &a, BooleanOp::Union, Tolerance::default()),
            Err(KernelError::InvalidInput(_))
        ));
    }
}

#[test]
fn self_intersecting_triangle_shell_is_outside_supported_domain() {
    // This closed octahedral topology has positive signed volume, but its two
    // apices lie on the same side and its triangular faces cross. Previously
    // topology-only validation accepted it. Generic triangle shells are now
    // explicitly unsupported, never reported geometrically valid.
    let points = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [2.0, 0.0, -0.5],
        [0.0, 0.0, -1.0],
    ];
    let mut model = ModelSnapshot {
        vertices: points.map(|position| Vertex { position }).to_vec(),
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    let mut edge_map = BTreeMap::new();
    for i in 0..4 {
        for ring in [[i, (i + 1) % 4, 4], [(i + 1) % 4, i, 5]] {
            let mut uses = Vec::new();
            for j in 0..3 {
                let a = ring[j];
                let b = ring[(j + 1) % 3];
                let key = [a.min(b), a.max(b)];
                let edge = *edge_map.entry(key).or_insert_with(|| {
                    let id = EdgeId(model.edges.len());
                    let delta = sub(points[key[1]], points[key[0]]);
                    let length = math::norm(delta);
                    model.edges.push(Edge {
                        vertices: key.map(VertexId),
                        line: Line {
                            origin: points[key[0]],
                            direction: delta.map(|v| v / length),
                        },
                    });
                    id
                });
                uses.push(EdgeUse {
                    edge,
                    reversed: a != key[0],
                });
            }
            let loop_id = LoopId(model.loops.len());
            model.loops.push(Loop { edges: uses });
            let normal = cross(
                sub(points[ring[1]], points[ring[0]]),
                sub(points[ring[2]], points[ring[0]]),
            );
            let length = math::norm(normal);
            model.faces.push(Face {
                plane: Plane {
                    origin: points[ring[0]],
                    normal: normal.map(|v| v / length),
                },
                loops: vec![loop_id],
            });
        }
    }
    model.shells.push(Shell {
        faces: (0..8).map(FaceId).collect(),
    });
    model.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    assert!((model.volume() - 1.0 / 3.0).abs() < 1e-14);
    let report = model.validate(Tolerance::default());
    assert!(!report.valid);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "unsupported_geometry"),
        "{report:?}"
    );
}

fn mapped_area(model: &ModelSnapshot, faces: &[FaceId]) -> f64 {
    faces
        .iter()
        .map(|id| {
            let points = model.loop_points(model.faces[id.0].loops[0]).unwrap();
            math::norm(cross(sub(points[1], points[0]), sub(points[3], points[0])))
        })
        .sum()
}

#[test]
fn translation_lineage_is_identity_and_legacy_history_is_readable() {
    let source = extrude_rectangle(10.0, 10.0, 2.0, Tolerance::default()).unwrap();
    assert!(source.history.input_faces.is_empty());
    let moved = translate(&source.model, [14.0, -3.0, 8.0], Tolerance::default()).unwrap();
    let mapping = &moved.history.input_faces["input"];
    assert_eq!(mapping.len(), 6);
    for i in 0..6 {
        assert_eq!(mapping[&FaceId(i)], vec![FaceId(i)]);
    }
    let json = serde_json::to_string(&moved).unwrap();
    assert_eq!(
        serde_json::from_str::<OperationResult>(&json).unwrap(),
        moved
    );
    let legacy: TopologyHistory =
        serde_json::from_str(r#"{"generated_faces":{"end_cap":[1]}}"#).unwrap();
    assert!(legacy.input_faces.is_empty());
    assert_eq!(legacy.generated_faces["end_cap"], vec![FaceId(1)]);
}

#[test]
fn cut_lineage_tracks_split_cap_and_reversed_tool_walls() {
    let base = block([10.0, 10.0, 2.0], [0.0; 3]);
    // Analytic result: two 4 x 10 x 2 disconnected slabs, top area 80.
    let cutter = block([2.0, 12.0, 4.0], [4.0, -1.0, -1.0]);
    let cut = boolean(&base, &cutter, BooleanOp::Difference, Tolerance::default()).unwrap();
    assert_eq!(cut.model.solids.len(), 2);
    let top = &cut.history.input_faces["left"][&FaceId(1)];
    assert_eq!(top.len(), 2);
    assert_eq!(mapped_area(&cut.model, top), 80.0);
    for &id in top {
        assert_eq!(cut.model.faces[id.0].plane.normal, [0.0, 0.0, 1.0]);
        assert_eq!(cut.model.faces[id.0].plane.origin[2], 2.0);
    }
    let tool = &cut.history.input_faces["right"];
    for (source, expected_normal, x) in [
        (FaceId(3), [-1.0, 0.0, 0.0], 6.0),
        (FaceId(5), [1.0, 0.0, 0.0], 4.0),
    ] {
        assert_eq!(tool[&source].len(), 1);
        let face = &cut.model.faces[tool[&source][0].0];
        assert_eq!(face.plane.normal, expected_normal);
        assert_eq!(face.plane.origin[0], x);
        assert_eq!(mapped_area(&cut.model, &tool[&source]), 20.0);
    }
    for i in [0, 1, 2, 4] {
        assert!(tool[&FaceId(i)].is_empty());
    }
    assert_eq!(
        cut.history,
        boolean(&base, &cutter, BooleanOp::Difference, Tolerance::default())
            .unwrap()
            .history
    );
}

#[test]
fn lineage_rebuilds_after_parameter_edits_and_records_deletion_without_fallback() {
    let base = block([10.0, 10.0, 2.0], [0.0; 3]);
    // Same operation and source face roles: editing cutter position changes
    // two top descendants into one, then editing size deletes the whole part.
    for (offset, size, top_count, top_area) in [
        ([4.0, -1.0, -1.0], [2.0, 12.0, 4.0], 2, 80.0),
        ([8.0, -1.0, -1.0], [4.0, 12.0, 4.0], 1, 80.0),
        ([-1.0, -1.0, -1.0], [12.0, 12.0, 4.0], 0, 0.0),
    ] {
        let cutter = block(size, offset);
        let cut = boolean(&base, &cutter, BooleanOp::Difference, Tolerance::default()).unwrap();
        let mapping = &cut.history.input_faces["left"];
        assert_eq!(mapping.len(), 6);
        assert_eq!(mapping[&FaceId(1)].len(), top_count);
        assert_eq!(mapped_area(&cut.model, &mapping[&FaceId(1)]), top_area);
        if top_count == 0 {
            assert!(cut
                .history
                .input_faces
                .values()
                .all(|operand| operand.values().all(Vec::is_empty)));
        }
    }
}

#[test]
fn coincident_union_and_contacting_union_have_explicit_provenance() {
    let left = block([2.0; 3], [0.0; 3]);
    let same = boolean(&left, &left, BooleanOp::Union, Tolerance::default()).unwrap();
    assert_eq!(
        same.history.input_faces["left"],
        same.history.input_faces["right"]
    );
    assert!(same.history.input_faces["left"]
        .values()
        .all(|ids| ids.len() == 1));
    let right = block([2.0; 3], [2.0, 0.0, 0.0]);
    let joined = boolean(&left, &right, BooleanOp::Union, Tolerance::default()).unwrap();
    // Internal meeting faces disappear instead of acquiring an unrelated wall.
    assert!(joined.history.input_faces["left"][&FaceId(3)].is_empty());
    assert!(joined.history.input_faces["right"][&FaceId(5)].is_empty());
    assert_eq!(
        mapped_area(
            &joined.model,
            &joined.history.input_faces["left"][&FaceId(1)]
        ),
        4.0
    );
    assert_eq!(
        mapped_area(
            &joined.model,
            &joined.history.input_faces["right"][&FaceId(1)]
        ),
        4.0
    );
}

#[test]
fn intersection_lineage_preserves_tool_orientation_and_empty_inputs() {
    let base = block([10.0, 10.0, 2.0], [0.0; 3]);
    let cutter = block([2.0, 12.0, 4.0], [4.0, -1.0, -1.0]);
    let intersection = boolean(
        &base,
        &cutter,
        BooleanOp::Intersection,
        Tolerance::default(),
    )
    .unwrap();
    assert_eq!(
        mapped_area(
            &intersection.model,
            &intersection.history.input_faces["left"][&FaceId(1)]
        ),
        20.0
    );
    for i in [3, 5] {
        let descendants = &intersection.history.input_faces["right"][&FaceId(i)];
        assert_eq!(descendants.len(), 1);
        assert_eq!(
            intersection.model.faces[descendants[0].0].plane.normal,
            cutter.faces[i].plane.normal
        );
    }
    let empty = boolean(&base, &base, BooleanOp::Difference, Tolerance::default()).unwrap();
    let both_empty = boolean(
        &empty.model,
        &empty.model,
        BooleanOp::Union,
        Tolerance::default(),
    )
    .unwrap();
    assert_eq!(both_empty.history.input_faces.len(), 2);
    assert!(both_empty
        .history
        .input_faces
        .values()
        .all(BTreeMap::is_empty));
    let restored = boolean(&base, &empty.model, BooleanOp::Union, Tolerance::default()).unwrap();
    assert!(restored.history.input_faces["right"].is_empty());
    assert!(restored.history.input_faces["left"]
        .values()
        .all(|ids| ids.len() == 1));
}
