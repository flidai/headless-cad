//! Exact coordinate-cell operations for axis-aligned, planar rectangular faces.
//! Boundaries use the input coordinates without snapping or voxel approximation.
//! Resource limits and unsupported near-coincident planes are explicit errors.
use crate::math::{finite, norm, sub};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

const MAX_CELLS: usize = 200_000;
const MAX_CLASSIFICATION_WORK: usize = 20_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanOp {
    Union,
    Difference,
    Intersection,
}

fn validated(model: &ModelSnapshot, tolerance: Tolerance) -> Result<(), KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    let report = model.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    Ok(())
}

fn empty() -> ModelSnapshot {
    ModelSnapshot {
        vertices: vec![],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    }
}

fn result(model: ModelSnapshot, role: &str) -> OperationResult {
    let generated_faces = BTreeMap::from([(
        role.to_owned(),
        (0..model.faces.len()).map(FaceId).collect(),
    )]);
    OperationResult {
        model,
        history: TopologyHistory {
            generated_faces,
            input_faces: BTreeMap::new(),
        },
    }
}

/// Translate immutable geometry by an offset in millimetres, preserving topology.
/// Reject coordinate overflow or translations whose rounding changes edge lengths
/// by more than the modeling tolerance.
pub fn translate(
    model: &ModelSnapshot,
    offset: [f64; 3],
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    if model.has_curved_geometry() {
        let mut result = crate::transform(model, offset, [0., 0., 0., 1.], tolerance)?;
        let faces = result
            .history
            .generated_faces
            .remove("transformed")
            .unwrap();
        result
            .history
            .generated_faces
            .insert("translated".into(), faces);
        return Ok(result);
    }
    validated(model, tolerance)?;
    if !finite(offset) {
        return Err(KernelError::NumericRange);
    }
    let shift = |point: [f64; 3]| -> Result<[f64; 3], KernelError> {
        let moved = std::array::from_fn(|axis| point[axis] + offset[axis]);
        if finite(moved) {
            Ok(moved)
        } else {
            Err(KernelError::NumericRange)
        }
    };
    let mut moved = model.clone();
    for vertex in &mut moved.vertices {
        vertex.position = shift(vertex.position)?;
    }
    for edge in &mut moved.edges {
        edge.try_line_mut().unwrap().origin = shift(edge.try_line().unwrap().origin)?;
    }
    for face in &mut moved.faces {
        face.try_plane_mut().unwrap().origin = shift(face.try_plane().unwrap().origin)?;
    }
    for edge in &model.edges {
        let [a, b] = edge.vertices;
        let before = norm(sub(
            model.vertices[a.0].position,
            model.vertices[b.0].position,
        ));
        let after = norm(sub(
            moved.vertices[a.0].position,
            moved.vertices[b.0].position,
        ));
        if (before - after).abs() > tolerance.linear {
            return Err(KernelError::NumericRange);
        }
    }
    let report = moved.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let mut result = result(moved, "translated");
    result.history.input_faces.insert(
        "input".into(),
        (0..model.faces.len())
            .map(|i| (FaceId(i), vec![FaceId(i)]))
            .collect(),
    );
    Ok(result)
}

struct RectFace {
    axis: usize,
    sign: i32,
    bounds: Bounds,
}

/// Attribute arrangement patches only to exact supporting input rectangles.
/// Arrangement coordinates include every input rectangle boundary, so a patch
/// is wholly inside a source rectangle or has no positive-area overlap with it.
/// Plane identity is exact; tolerance is never a nearest-face search radius.
fn boolean_result(
    model: ModelSnapshot,
    left: &[RectFace],
    right: &[RectFace],
    operation: BooleanOp,
) -> Result<OperationResult, KernelError> {
    let output = rectangular_faces(&model)?;
    let mut result = result(model, "boundary");
    let mut covered = vec![false; output.len()];
    let mut work = 0usize;
    for (operand, sources, orientation) in [
        ("left", left, 1),
        (
            "right",
            right,
            if operation == BooleanOp::Difference {
                -1
            } else {
                1
            },
        ),
    ] {
        let mut mapping: BTreeMap<_, Vec<FaceId>> =
            (0..sources.len()).map(|i| (FaceId(i), vec![])).collect();
        let mut planes: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let key = |face: &RectFace, orientation: i32| {
            let coordinate = face.bounds.min[face.axis];
            // IEEE -0 and +0 identify the same exact geometric plane.
            let bits = if coordinate == 0.0 {
                0
            } else {
                coordinate.to_bits()
            };
            (face.axis, bits, face.sign * orientation)
        };
        for (i, face) in sources.iter().enumerate() {
            planes.entry(key(face, orientation)).or_default().push(i);
        }
        for (i, face) in output.iter().enumerate() {
            let Some(candidates) = planes.get(&key(face, 1)) else {
                continue;
            };
            work = work.checked_add(candidates.len()).ok_or_else(|| {
                KernelError::ResourceLimit("face provenance work overflow".into())
            })?;
            if work > MAX_CLASSIFICATION_WORK {
                return Err(KernelError::ResourceLimit(
                    "face provenance exceeds operation budget".into(),
                ));
            }
            for &source_id in candidates {
                let source = &sources[source_id];
                if (0..3).filter(|&axis| axis != face.axis).all(|axis| {
                    source.bounds.min[axis] <= face.bounds.min[axis]
                        && face.bounds.max[axis] <= source.bounds.max[axis]
                }) {
                    mapping.get_mut(&FaceId(source_id)).unwrap().push(FaceId(i));
                    covered[i] = true;
                }
            }
        }
        result.history.input_faces.insert(operand.into(), mapping);
    }
    if covered.contains(&false) {
        return Err(KernelError::Unsupported(
            "Boolean boundary has no exact input face provenance".into(),
        ));
    }
    Ok(result)
}

fn rectangular_faces(model: &ModelSnapshot) -> Result<Vec<RectFace>, KernelError> {
    model
        .faces
        .iter()
        .map(|face| {
            if face.loops.len() != 1 {
                return Err(KernelError::Unsupported(
                    "orthogonal Booleans do not accept multi-loop faces".into(),
                ));
            }
            let axis = (0..3)
                .find(|&axis| {
                    face.try_plane().unwrap().normal[axis].abs() == 1.0
                        && (0..3)
                            .filter(|&i| i != axis)
                            .all(|i| face.try_plane().unwrap().normal[i] == 0.0)
                })
                .ok_or_else(|| {
                    KernelError::Unsupported(
                        "Boolean faces must have exact axis-aligned normals".into(),
                    )
                })?;
            let points = model
                .loop_points(face.loops[0])
                .ok_or(KernelError::NumericRange)?;
            if points.len() != 4 {
                return Err(KernelError::Unsupported(
                    "Boolean faces must be rectangular quadrilaterals".into(),
                ));
            }
            let mut bounds = Bounds {
                min: [f64::INFINITY; 3],
                max: [f64::NEG_INFINITY; 3],
            };
            for point in &points {
                for (i, &coordinate) in point.iter().enumerate() {
                    bounds.min[i] = bounds.min[i].min(coordinate);
                    bounds.max[i] = bounds.max[i].max(coordinate);
                }
            }
            if bounds.min[axis] != bounds.max[axis] {
                return Err(KernelError::Unsupported(
                    "Boolean faces must lie exactly on an axis-aligned plane".into(),
                ));
            }
            for i in 0..4 {
                let edge = sub(points[(i + 1) % 4], points[i]);
                if edge.into_iter().filter(|v| *v != 0.0).count() != 1 {
                    return Err(KernelError::Unsupported(
                        "Boolean edges must be exactly axis-aligned".into(),
                    ));
                }
                for transverse in (0..3).filter(|&i| i != axis) {
                    if points[i][transverse] != bounds.min[transverse]
                        && points[i][transverse] != bounds.max[transverse]
                    {
                        return Err(KernelError::Unsupported(
                            "Boolean faces must be rectangles".into(),
                        ));
                    }
                }
            }
            Ok(RectFace {
                axis,
                sign: if face.try_plane().unwrap().normal[axis] > 0.0 {
                    1
                } else {
                    -1
                },
                bounds,
            })
        })
        .collect()
}

// Fast-path admission checks exact support identity independently of the
// tolerance-based generic input validator. The bounded rectangular faces then
// certify the complete material AABB; seam vertices or display bounds are never
// substituted for a curved support (this path is planar/orthogonal only).
fn exact_rectangular_supports(
    model: &ModelSnapshot,
    faces: &[RectFace],
) -> Result<(), KernelError> {
    for (face, rect) in model.faces.iter().zip(faces) {
        if face.try_plane().unwrap().origin[rect.axis] != rect.bounds.min[rect.axis] {
            return Err(KernelError::Unsupported(
                "disjoint orthogonal operands require exact native plane supports".into(),
            ));
        }
    }
    for edge in &model.edges {
        let a = model.vertices[edge.vertices[0].0].position;
        let b = model.vertices[edge.vertices[1].0].position;
        let changes: Vec<_> = (0..3).filter(|&i| a[i] != b[i]).collect();
        let line = edge.try_line().ok_or_else(|| {
            KernelError::Unsupported("disjoint orthogonal operands require native lines".into())
        })?;
        if changes.len() != 1
            || (0..3).any(|i| {
                if changes.contains(&i) {
                    line.direction[i].abs() != 1.
                } else {
                    line.direction[i] != 0. || line.origin[i] != a[i]
                }
            })
        {
            return Err(KernelError::Unsupported(
                "disjoint orthogonal operands require exact native line supports".into(),
            ));
        }
    }
    Ok(())
}
fn disjoint_bounds(a: &[RectFace], b: &[RectFace], tolerance: Tolerance) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let bounds = |faces: &[RectFace]| Bounds {
        min: std::array::from_fn(|i| {
            faces
                .iter()
                .map(|f| f.bounds.min[i])
                .fold(f64::INFINITY, f64::min)
        }),
        max: std::array::from_fn(|i| {
            faces
                .iter()
                .map(|f| f.bounds.max[i])
                .fold(f64::NEG_INFINITY, f64::max)
        }),
    };
    let (a, b) = (bounds(a), bounds(b));
    (0..3).any(|i| {
        [b.min[i] - a.max[i], a.min[i] - b.max[i]]
            .into_iter()
            .any(|gap| gap.is_normal() && gap.next_down() > tolerance.linear)
    })
}
fn disjoint_result(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    operation: BooleanOp,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    let model = match operation {
        BooleanOp::Intersection => empty(),
        BooleanOp::Difference => a.clone(),
        BooleanOp::Union => {
            let counts = [
                a.vertices.len(),
                a.edges.len(),
                a.loops.len(),
                a.faces.len(),
                a.shells.len(),
                a.solids.len(),
            ];
            let other = [
                b.vertices.len(),
                b.edges.len(),
                b.loops.len(),
                b.faces.len(),
                b.shells.len(),
                b.solids.len(),
            ];
            let mut totals = [0usize; 6];
            for i in 0..6 {
                totals[i] = counts[i].checked_add(other[i]).ok_or_else(|| {
                    KernelError::ResourceLimit("disjoint topology count overflow".into())
                })?;
            }
            // Keep the final validator's actual quadratic budget, before copying
            // any native arrays. This shortcut removes cell allocation, not the
            // native geometric validity checks or their resource contract.
            if totals[3]
                .checked_mul(totals[3].saturating_sub(1))
                .and_then(|n| n.checked_div(2))
                .is_none_or(|n| n > crate::validation::geometry::MAX_FACE_PAIRS)
            {
                return Err(KernelError::ResourceLimit(
                    "disjoint union exceeds native face-pair validation budget".into(),
                ));
            }
            let mut model = a.clone();
            model.vertices.extend(b.vertices.iter().cloned());
            model.edges.extend(b.edges.iter().map(|e| Edge {
                vertices: e.vertices.map(|v| VertexId(v.0 + counts[0])),
                curve: e.curve.clone(),
            }));
            model.loops.extend(b.loops.iter().map(|l| {
                Loop {
                    edges: l
                        .edges
                        .iter()
                        .map(|u| EdgeUse {
                            edge: EdgeId(u.edge.0 + counts[1]),
                            reversed: u.reversed,
                        })
                        .collect(),
                }
            }));
            model.faces.extend(b.faces.iter().map(|f| Face {
                surface: f.surface.clone(),
                loops: f.loops.iter().map(|id| LoopId(id.0 + counts[2])).collect(),
            }));
            model.shells.extend(b.shells.iter().map(|s| Shell {
                faces: s.faces.iter().map(|id| FaceId(id.0 + counts[3])).collect(),
            }));
            model.solids.extend(b.solids.iter().map(|s| {
                Solid {
                    shells: s
                        .shells
                        .iter()
                        .map(|id| ShellId(id.0 + counts[4]))
                        .collect(),
                }
            }));
            model
        }
    };
    let validation = model.validate(tolerance);
    if !validation.valid {
        return Err(KernelError::InvalidResult(validation));
    }
    let mut result = result(model, "boundary");
    result.history.input_faces.insert(
        "left".into(),
        (0..a.faces.len())
            .map(|i| {
                (
                    FaceId(i),
                    if operation == BooleanOp::Intersection {
                        vec![]
                    } else {
                        vec![FaceId(i)]
                    },
                )
            })
            .collect(),
    );
    result.history.input_faces.insert(
        "right".into(),
        (0..b.faces.len())
            .map(|i| {
                (
                    FaceId(i),
                    if operation == BooleanOp::Union {
                        vec![FaceId(a.faces.len() + i)]
                    } else {
                        vec![]
                    },
                )
            })
            .collect(),
    );
    Ok(result)
}

// The sample lies strictly between every input coordinate plane. A ray along X
// therefore never hits a face edge. Outward oriented crossings encode occupancy.
fn inside(faces: &[RectFace], sample: [f64; 3]) -> Result<bool, KernelError> {
    let winding: i32 = faces
        .iter()
        .filter(|face| {
            face.axis == 0
                && face.bounds.min[0] > sample[0]
                && face.bounds.min[1] < sample[1]
                && sample[1] < face.bounds.max[1]
                && face.bounds.min[2] < sample[2]
                && sample[2] < face.bounds.max[2]
        })
        .map(|face| face.sign)
        .sum();
    match winding {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(KernelError::Unsupported(
            "overlapping or inconsistently oriented input solids".into(),
        )),
    }
}

fn index(cell: [usize; 3], shape: [usize; 3]) -> usize {
    (cell[0] * shape[1] + cell[1]) * shape[2] + cell[2]
}
fn cell(index: usize, shape: [usize; 3]) -> [usize; 3] {
    [
        index / (shape[1] * shape[2]),
        (index / shape[2]) % shape[1],
        index % shape[2],
    ]
}
const DIRECTIONS: [(usize, isize); 6] = [(2, -1), (2, 1), (1, -1), (0, 1), (1, 1), (0, -1)];
fn neighbor(mut cell: [usize; 3], direction: usize, shape: [usize; 3]) -> Option<usize> {
    let (axis, sign) = DIRECTIONS[direction];
    cell[axis] = cell[axis].checked_add_signed(sign)?;
    if cell[axis] >= shape[axis] {
        None
    } else {
        Some(index(cell, shape))
    }
}

/// Boolean operation on orthogonal solids. Uses an exact arrangement of input
/// planes, not a fixed-resolution voxel approximation. Through-holes and
/// disconnected solids are supported. Cavities and nonmanifold contacts return
/// explicit Unsupported errors. Distinct plane gaps <= tolerance are rejected.
/// Strictly separated operand AABBs with exact orthogonal native supports bypass
/// the Cartesian arrangement: union concatenates native topology, difference
/// preserves the left operand, and intersection is empty. Coordinate-gap checks
/// and bounded final validation still apply, including quadratic face-pair work.
/// This shortcut does not admit disconnected curved solids.
/// Intersection additionally supports two single strictly convex planar solids
/// (48 input planes maximum) through analytic supporting-plane reconstruction.
/// Unresolved near-coincidence, ill-conditioned plane triples, and tangent or
/// lower-dimensional intersections in that general domain are explicit errors.
/// Two solid circular cylinders additionally support trimmed analytic circle
/// arrangements: parallel axes, common axial interval for union, and a through
/// tool for difference. Intersection may trim the axial interval. Difference also
/// supports contained cylindrical through/blind cuts in round or annular stock,
/// preserving dimensions while splitting the outer periodic wall into two exact
/// semicylinder patches. Subsequent separated bores and coaxial counterbores use
/// the native prism drilling domain. Annular unions/intersections, disconnected
/// curved unions, skew axes and unresolved contacts remain unsupported.
/// Oblique support identity is arithmetic-certified, not a real-number equality
/// proof; modeling tolerance never merges cap planes or cylinder supports.
pub fn boolean(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    operation: BooleanOp,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    validated(a, tolerance)?;
    validated(b, tolerance)?;
    if operation == BooleanOp::Difference
        && b.has_curved_geometry()
        && crate::analytic::recognize(a, Some(tolerance)).is_err()
    {
        return crate::through_bore::cut(a, b, tolerance)
            .or_else(|_| crate::bored_prism::cut(a, b, tolerance))
            .or_else(|_| crate::stepped_bore::cut(a, b, tolerance));
    }
    if a.has_curved_geometry() || b.has_curved_geometry() {
        let result = crate::circular_boolean::boolean(a, b, operation, tolerance);
        return if operation == BooleanOp::Difference {
            result
                .or_else(|_| crate::round_stock::cut(a, b, tolerance))
                .or_else(|_| crate::keyseat::cut(a, b, tolerance))
                .or_else(|_| crate::stepped_keyseat::cut(a, b, tolerance))
                .or_else(|_| crate::hub_keyway::cut(a, b, tolerance))
        } else {
            result
        };
    }
    let rectangles = (rectangular_faces(a), rectangular_faces(b));
    if matches!(operation, BooleanOp::Intersection)
        && (rectangles.0.is_err() || rectangles.1.is_err())
    {
        return crate::convex_intersection::intersect(a, b, tolerance);
    }
    let a_faces = rectangles.0?;
    let b_faces = rectangles.1?;
    if a.vertices.is_empty() && b.vertices.is_empty() {
        return boolean_result(empty(), &a_faces, &b_faces, operation);
    }
    let mut axes: [Vec<f64>; 3] = std::array::from_fn(|axis| {
        a.vertices
            .iter()
            .chain(&b.vertices)
            .map(|v| v.position[axis])
            .collect()
    });
    for coordinates in &mut axes {
        coordinates.sort_by(f64::total_cmp);
        coordinates.dedup_by(|a, b| *a == *b);
        for pair in coordinates.windows(2) {
            let gap = pair[1] - pair[0];
            if !gap.is_normal() {
                return Err(KernelError::NumericRange);
            }
            if gap <= tolerance.linear {
                return Err(KernelError::Unsupported("distinct coordinate planes are within modeling tolerance; explicit snapping is required".into()));
            }
        }
    }
    if disjoint_bounds(&a_faces, &b_faces, tolerance) {
        exact_rectangular_supports(a, &a_faces)?;
        exact_rectangular_supports(b, &b_faces)?;
        return disjoint_result(a, b, operation, tolerance);
    }
    let shape = axes.each_ref().map(|axis| axis.len() - 1);
    let count = shape
        .into_iter()
        .try_fold(1usize, |total, n| total.checked_mul(n))
        .ok_or_else(|| KernelError::ResourceLimit("coordinate grid overflow".into()))?;
    if count > MAX_CELLS {
        return Err(KernelError::ResourceLimit(format!(
            "coordinate grid has {count} cells; maximum is {MAX_CELLS}"
        )));
    }
    let work = count
        .checked_mul(a_faces.len() + b_faces.len())
        .ok_or_else(|| KernelError::ResourceLimit("classification work overflow".into()))?;
    if work > MAX_CLASSIFICATION_WORK {
        return Err(KernelError::ResourceLimit(
            "coordinate classification exceeds operation budget".into(),
        ));
    }
    let mut occupied = vec![false; count];
    for (i, occupied) in occupied.iter_mut().enumerate() {
        let cell = cell(i, shape);
        let sample = std::array::from_fn(|axis| {
            let low = axes[axis][cell[axis]];
            low + (axes[axis][cell[axis] + 1] - low) / 2.0
        });
        if (0..3).any(|axis| {
            sample[axis] <= axes[axis][cell[axis]] || sample[axis] >= axes[axis][cell[axis] + 1]
        }) {
            return Err(KernelError::NumericRange);
        }
        let in_a = inside(&a_faces, sample)?;
        let in_b = inside(&b_faces, sample)?;
        *occupied = match operation {
            BooleanOp::Union => in_a || in_b,
            BooleanOp::Difference => in_a && !in_b,
            BooleanOp::Intersection => in_a && in_b,
        };
    }
    let mut components = vec![usize::MAX; count];
    let mut component_count = 0;
    for start in 0..count {
        if !occupied[start] || components[start] != usize::MAX {
            continue;
        }
        let mut pending = vec![start];
        components[start] = component_count;
        while let Some(i) = pending.pop() {
            for direction in 0..6 {
                if let Some(next) = neighbor(cell(i, shape), direction, shape) {
                    if occupied[next] && components[next] == usize::MAX {
                        components[next] = component_count;
                        pending.push(next);
                    }
                }
            }
        }
        component_count += 1;
    }
    let mut model = empty();
    let mut vertices = BTreeMap::new();
    let mut edges = BTreeMap::new();
    let mut component_faces = vec![Vec::new(); component_count];
    let rings = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ];
    for i in 0..count {
        if !occupied[i] {
            continue;
        }
        let [x, y, z] = cell(i, shape);
        let corners = [
            [x, y, z],
            [x + 1, y, z],
            [x + 1, y + 1, z],
            [x, y + 1, z],
            [x, y, z + 1],
            [x + 1, y, z + 1],
            [x + 1, y + 1, z + 1],
            [x, y + 1, z + 1],
        ];
        for direction in 0..6 {
            if neighbor([x, y, z], direction, shape).is_some_and(|next| occupied[next]) {
                continue;
            }
            let mut ring = Vec::new();
            for corner in rings[direction] {
                let coordinate = corners[corner];
                let id = *vertices.entry(coordinate).or_insert_with(|| {
                    let id = VertexId(model.vertices.len());
                    model.vertices.push(Vertex {
                        position: std::array::from_fn(|axis| axes[axis][coordinate[axis]]),
                    });
                    id
                });
                ring.push(id);
            }
            let mut uses = Vec::new();
            for j in 0..4 {
                let a = ring[j];
                let b = ring[(j + 1) % 4];
                let key = [a.min(b), a.max(b)];
                let id = *edges.entry(key).or_insert_with(|| {
                    let id = EdgeId(model.edges.len());
                    let origin = model.vertices[key[0].0].position;
                    let delta = sub(model.vertices[key[1].0].position, origin);
                    let length = norm(delta);
                    model.edges.push(Edge {
                        vertices: key,
                        curve: Curve::Line(Line {
                            origin,
                            direction: delta.map(|v| v / length),
                        }),
                    });
                    id
                });
                uses.push(EdgeUse {
                    edge: id,
                    reversed: a != key[0],
                });
            }
            let loop_id = LoopId(model.loops.len());
            model.loops.push(Loop { edges: uses });
            let (axis, sign) = DIRECTIONS[direction];
            let mut normal = [0.0; 3];
            normal[axis] = sign as f64;
            let face_id = FaceId(model.faces.len());
            model.faces.push(Face {
                surface: Surface::Plane(Plane {
                    origin: model.vertices[ring[0].0].position,
                    normal,
                }),
                loops: vec![loop_id],
            });
            component_faces[components[i]].push(face_id);
        }
    }
    for faces in component_faces {
        let shell_id = ShellId(model.shells.len());
        model.shells.push(Shell { faces });
        model.solids.push(Solid {
            shells: vec![shell_id],
        });
    }
    let report = model.validate(tolerance);
    if !report.valid {
        let codes: BTreeSet<_> = report
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect();
        if codes.contains("disconnected_shell") {
            return Err(KernelError::Unsupported(
                "enclosed cavities require multiple boundary shells and are not implemented".into(),
            ));
        }
        if codes.contains("edge_incidence")
            || codes.contains("nonmanifold_vertex")
            || codes.contains("shell_incidence")
        {
            return Err(KernelError::Unsupported(
                "Boolean result has nonmanifold edge or vertex contacts".into(),
            ));
        }
        return Err(KernelError::InvalidResult(report));
    }
    boolean_result(model, &a_faces, &b_faces, operation)
}

#[cfg(test)]
mod disjoint_tests {
    use super::*;
    fn diagonal() -> ModelSnapshot {
        let t = Tolerance::default();
        let unit = extrude_rectangle(1., 1., 1., t).unwrap().model;
        let mut model = unit.clone();
        for i in 1..36 {
            let moved = translate(&unit, [2. * i as f64; 3], t).unwrap().model;
            let old = model.clone();
            let result = boolean(&model, &moved, BooleanOp::Union, t).unwrap();
            for j in 0..old.faces.len() {
                assert_eq!(
                    result.history.input_faces["left"][&FaceId(j)],
                    vec![FaceId(j)]
                );
            }
            for j in 0..moved.faces.len() {
                assert_eq!(
                    result.history.input_faces["right"][&FaceId(j)],
                    vec![FaceId(old.faces.len() + j)]
                );
            }
            assert_eq!(&result.model.faces[..old.faces.len()], old.faces.as_slice());
            assert_eq!(&result.model.edges[..old.edges.len()], old.edges.as_slice());
            model = result.model;
        }
        model
    }
    #[test]
    fn diagonal_components_preserve_exact_topology_full_moments_and_closed_mesh() {
        let t = Tolerance::default();
        let model = diagonal();
        assert_eq!(
            (model.solids.len(), model.shells.len(), model.faces.len()),
            (36, 36, 216)
        );
        assert_eq!(
            model.bounds(),
            Bounds {
                min: [0.; 3],
                max: [71.; 3]
            }
        );
        let mass = model.mass_properties(t).unwrap();
        assert!((mass.volume - 36.).abs() < 1e-10);
        for x in mass.centroid.unwrap() {
            assert!((x - 35.5).abs() < 1e-10);
        }
        for (i, row) in mass.central_inertia.unwrap().iter().enumerate() {
            for (j, value) in row.iter().enumerate() {
                assert!((value - if i == j { 31086. } else { -15540. }).abs() < 1e-7);
            }
        }
        let mut incidence = BTreeMap::<(usize, usize), (usize, i32)>::new();
        let mut volume = 0.;
        let mut count = 0;
        for i in 0..model.faces.len() {
            for triangle in crate::triangulate_face(&model, FaceId(i), t).unwrap() {
                count += 1;
                let p = triangle.map(|v| model.vertices[v.0].position);
                volume += crate::math::dot(p[0], crate::math::cross(p[1], p[2])) / 6.;
                for j in 0..3 {
                    let a = triangle[j].0;
                    let b = triangle[(j + 1) % 3].0;
                    let entry = incidence.entry((a.min(b), a.max(b))).or_default();
                    entry.0 += 1;
                    entry.1 += if a < b { 1 } else { -1 };
                }
            }
        }
        assert_eq!(count, 432);
        assert!((volume - 36.).abs() < 1e-9);
        assert!(incidence.values().all(|v| *v == (2, 0)));
        // Exact cyclic rigid placement retains the supported orthogonal family.
        let moved = crate::transform(&model, [20., -8., 3.], [0.5; 4], t)
            .unwrap()
            .model;
        let properties = moved.mass_properties(t).unwrap();
        assert!((properties.volume - 36.).abs() < 1e-10);
        assert_eq!(properties.centroid.unwrap(), [55.5, 27.5, 38.5]);
    }
    #[test]
    fn intersecting_compounds_still_refuse_an_oversized_native_grid() {
        let t = Tolerance::default();
        let a = diagonal();
        let b = translate(&a, [0.5; 3], t).unwrap().model;
        match boolean(&a, &b, BooleanOp::Union, t) {
            Err(KernelError::ResourceLimit(message)) => {
                assert!(message.contains("coordinate grid"))
            }
            result => panic!("expected bounded interacting grid refusal, got {result:?}"),
        }
    }
    #[test]
    fn disjoint_difference_intersection_swapping_and_later_local_cut() {
        let t = Tolerance::default();
        let a = extrude_rectangle(2., 3., 4., t).unwrap().model;
        let b = translate(&a, [5., 0., 0.], t).unwrap().model;
        for (a, b) in [(&a, &b), (&b, &a)] {
            let d = boolean(a, b, BooleanOp::Difference, t).unwrap();
            assert_eq!(&d.model, a);
            assert!(d.history.input_faces["right"].values().all(Vec::is_empty));
            let i = boolean(a, b, BooleanOp::Intersection, t).unwrap();
            assert!(i.model.solids.is_empty());
            assert!(i
                .history
                .input_faces
                .values()
                .all(|m| m.values().all(Vec::is_empty)));
        }
        let mut rows = vec![];
        for y in 0..4 {
            let mut row = translate(&a, [0., 7. * y as f64, 0.], t).unwrap().model;
            for x in 1..4 {
                let next = translate(&a, [5. * x as f64, 7. * y as f64, 0.], t)
                    .unwrap()
                    .model;
                row = boolean(&row, &next, BooleanOp::Union, t).unwrap().model;
            }
            rows.push(row);
        }
        let mut batch = rows.remove(0);
        for row in rows {
            batch = boolean(&batch, &row, BooleanOp::Union, t).unwrap().model;
        }
        let cutter = translate(
            &extrude_rectangle(1.5, 5., 6., t).unwrap().model,
            [-0.5, -1., -1.],
            t,
        )
        .unwrap()
        .model;
        let cut = boolean(&batch, &cutter, BooleanOp::Difference, t).unwrap();
        assert_eq!(cut.model.solids.len(), 16);
        assert!((cut.model.volume() - 372.).abs() < 1e-10);
        assert!(cut.model.validate(t).valid);
    }
    #[test]
    fn near_planes_touch_overlap_curves_and_raw_supports_never_get_disjoint_shortcut() {
        let t = Tolerance::default();
        let a = extrude_rectangle(1., 1., 1., t).unwrap().model;
        for offset in [[1. + t.linear / 2., 0., 0.], [10., t.linear / 2., 0.]] {
            let b = translate(&a, offset, t).unwrap().model;
            assert!(boolean(&a, &b, BooleanOp::Union, t).is_err());
        }
        for (offset, volume) in [([1., 0., 0.], 2.), ([0.5; 3], 1.875)] {
            let b = translate(&a, offset, t).unwrap().model;
            let m = boolean(&a, &b, BooleanOp::Union, t).unwrap().model;
            assert_eq!(m.solids.len(), 1);
            assert!((m.volume() - volume).abs() < 1e-12);
        }
        let b = translate(&a, [10., 0., 0.], t).unwrap().model;
        let mut bad = a.clone();
        bad.faces[0].try_plane_mut().unwrap().origin[2] += t.linear / 10.;
        assert!(boolean(&bad, &b, BooleanOp::Union, t).is_err());
        bad = a.clone();
        let edge = &mut bad.edges[0];
        let line = edge.try_line_mut().unwrap();
        let axis = (0..3).find(|&i| line.direction[i] == 0.).unwrap();
        line.origin[axis] += t.linear / 10.;
        assert!(boolean(&bad, &b, BooleanOp::Union, t).is_err());
        let cylinder = extrude_circle(1., 2., t).unwrap().model;
        let other = translate(&cylinder, [10., 0., 0.], t).unwrap().model;
        assert!(boolean(&cylinder, &other, BooleanOp::Union, t).is_err());
    }
}
