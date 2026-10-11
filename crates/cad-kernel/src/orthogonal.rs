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
        edge.line.origin = shift(edge.line.origin)?;
    }
    for face in &mut moved.faces {
        face.plane.origin = shift(face.plane.origin)?;
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
            let axis = (0..3)
                .find(|&axis| {
                    face.plane.normal[axis].abs() == 1.0
                        && (0..3)
                            .filter(|&i| i != axis)
                            .all(|i| face.plane.normal[i] == 0.0)
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
                sign: if face.plane.normal[axis] > 0.0 { 1 } else { -1 },
                bounds,
            })
        })
        .collect()
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
pub fn boolean(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    operation: BooleanOp,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    validated(a, tolerance)?;
    validated(b, tolerance)?;
    let a_faces = rectangular_faces(a)?;
    let b_faces = rectangular_faces(b)?;
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
                        line: Line {
                            origin,
                            direction: delta.map(|v| v / length),
                        },
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
                plane: Plane {
                    origin: model.vertices[ring[0].0].position,
                    normal,
                },
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
