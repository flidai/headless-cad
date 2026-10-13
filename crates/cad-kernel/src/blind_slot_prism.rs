//! Bounded native obround pockets in one cap of a canonical right prism.
use crate::arc_prism::{ArcPrismGeometry, SlotHole};
use crate::math::{cross, dot, finite, sub};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub(crate) struct Geometry {
    pub stock: ArcPrismGeometry,
    pub slots: Vec<SlotHole>,
    pub opening: FaceId,
    pub depth: f64,
    pub surrogate: ModelSnapshot,
}

fn bad() -> KernelError {
    KernelError::Unsupported("blind obround pockets require 1..4 separated native capsules opening one canonical prism cap at one resolved depth, with positive remaining web".into())
}
fn point(g: &ArcPrismGeometry, p: [f64; 2], z: f64) -> [f64; 3] {
    let y = cross(g.axis, g.x_direction);
    std::array::from_fn(|i| g.origin[i] + g.x_direction[i] * p[0] + y[i] * p[1] + g.axis[i] * z)
}
fn shift(p: &mut [f64; 3], axis: [f64; 3], amount: f64) {
    for i in 0..3 {
        p[i] += axis[i] * amount;
    }
}
fn sizes(g: &ArcPrismGeometry) -> (usize, usize, usize, usize) {
    let n = g.path.segments.len();
    let c = g.circular_holes.len();
    (2 * n + 2 * c, 3 * n + 3 * c, 2 + n + 3 * c, 2 + n + c)
}

fn construct(g: &Geometry) -> Result<ModelSnapshot, KernelError> {
    let mut model = g.surrogate.clone();
    let (base_v, base_e, base_l, base_f) = sizes(&g.stock);
    let count = g.slots.len();
    let floor_layer = 1 - g.opening.0;
    let delta = if floor_layer == 0 {
        g.stock.height - g.depth
    } else {
        g.depth - g.stock.height
    };
    let z_floor = if g.opening == FaceId(0) {
        g.depth
    } else {
        g.stock.height - g.depth
    };
    for (index, slot) in g.slots.iter().enumerate() {
        let v = base_v + 8 * index;
        let e = base_e + 12 * index;
        let l = base_l + 6 * index;
        for vertex in &mut model.vertices[v + 4 * floor_layer..v + 4 * floor_layer + 4] {
            shift(&mut vertex.position, g.stock.axis, delta);
        }
        for edge in &mut model.edges[e + 4 * floor_layer..e + 4 * floor_layer + 4] {
            match &mut edge.curve {
                Curve::Line(line) => shift(&mut line.origin, g.stock.axis, delta),
                Curve::CircularArc(arc) => shift(&mut arc.circle.origin, g.stock.axis, delta),
                _ => return Err(bad()),
            }
        }
        if floor_layer == 0 {
            for edge in &mut model.edges[e + 8..e + 12] {
                if let Curve::Line(line) = &mut edge.curve {
                    shift(&mut line.origin, g.stock.axis, delta)
                } else {
                    return Err(bad());
                }
            }
            for face in &mut model.faces[base_f + 4 * index..base_f + 4 * index + 4] {
                match &mut face.surface {
                    Surface::Plane(plane) => shift(&mut plane.origin, g.stock.axis, delta),
                    Surface::Cylinder(cyl) => shift(&mut cyl.origin, g.stock.axis, delta),
                    _ => return Err(bad()),
                }
            }
        }
        let loop_id = LoopId(l + floor_layer);
        let old = &mut model.loops[loop_id.0].edges;
        *old = old
            .iter()
            .rev()
            .map(|edge| EdgeUse {
                edge: edge.edge,
                reversed: !edge.reversed,
            })
            .collect();
        model.faces[floor_layer].loops.retain(|id| *id != loop_id);
        model.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: point(&g.stock, slot.center, z_floor),
                normal: if g.opening == FaceId(0) {
                    g.stock.axis.map(|v| -v)
                } else {
                    g.stock.axis
                },
            }),
            loops: vec![loop_id],
        });
    }
    if model.faces.len() != base_f + 5 * count {
        return Err(bad());
    }
    model.shells[0].faces = (0..model.faces.len()).map(FaceId).collect();
    Ok(model)
}

fn input_geometry(
    body: &ModelSnapshot,
    slots: &[ObroundSlot],
    cap: FaceId,
    depth: f64,
    t: Tolerance,
) -> Result<Geometry, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if cap.0 > 1 || slots.is_empty() || slots.len() > 4 {
        return Err(bad());
    }
    let (mut stock, _) = crate::obround_slot::geometry(body, t)?;
    if !depth.is_finite() {
        return Err(bad());
    }
    let scale = body
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain([depth, stock.height])
        .map(f64::abs)
        .fold(1., f64::max);
    let error = (65536. * f64::EPSILON * scale).next_up();
    if !error.is_normal() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    if depth <= t.linear + error || stock.height - depth <= t.linear + error {
        return Err(bad());
    }
    let mut names = BTreeSet::new();
    for slot in slots {
        if slot.id.is_empty()
            || slot.id.len() > 64
            || !slot.id.as_bytes()[0].is_ascii_alphabetic()
            || !slot
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !names.insert(slot.id.to_ascii_lowercase())
        {
            return Err(bad());
        }
        stock.slot_holes.push(SlotHole {
            center: slot.center,
            direction: slot.direction,
            centerline_length: slot.centerline_length,
            radius: slot.radius,
        });
    }
    crate::obround_slot::validate_slots(&stock.path, &stock.circular_holes, &stock.slot_holes, t)?;
    let surrogate = crate::arc_prism::construct(&stock, t)?;
    let slots = std::mem::take(&mut stock.slot_holes);
    Ok(Geometry {
        stock,
        slots,
        opening: cap,
        depth,
        surrogate,
    })
}

/// Cut a same-cap, shared-depth pattern. Coordinates are in the canonical
/// stock frame returned by `obround_slot_frame`; authored frame conversion is
/// the build layer's obligation.
pub fn cut_blind_obround_slots(
    body: &ModelSnapshot,
    slots: &[ObroundSlot],
    cap: FaceId,
    depth: f64,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let g = input_geometry(body, slots, cap, depth, t)?;
    let model = construct(&g)?;
    let report = model.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let recognized = recognize(&model, t)?;
    recognized.properties(t)?;
    let (_, _, _, base_f) = sizes(&g.stock);
    let mut generated = BTreeMap::from([
        ("start_cap".into(), vec![FaceId(0)]),
        ("end_cap".into(), vec![FaceId(1)]),
        ("opening_cap".into(), vec![cap]),
        ("opposite_cap".into(), vec![FaceId(1 - cap.0)]),
        (
            "boundary".into(),
            (0..model.faces.len()).map(FaceId).collect(),
        ),
    ]);
    for i in 0..g.stock.path.segments.len() {
        generated.insert(format!("segment_{i}"), vec![FaceId(i + 2)]);
    }
    for i in 0..g.stock.circular_holes.len() {
        generated.insert(
            format!("circular_hole_{i}_wall"),
            vec![FaceId(g.stock.path.segments.len() + 2 + i)],
        );
    }
    for (i, slot) in slots.iter().enumerate() {
        let walls = (0..4)
            .map(|j| FaceId(base_f + 4 * i + j))
            .collect::<Vec<_>>();
        generated.insert(format!("slot_{}_walls", slot.id), walls.clone());
        for (j, wall) in walls.into_iter().enumerate() {
            generated.insert(format!("slot_{}_wall_{j}", slot.id), vec![wall]);
        }
        generated.insert(
            format!("slot_{}_floor", slot.id),
            vec![FaceId(base_f + 4 * slots.len() + i)],
        );
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: generated,
            input_faces: BTreeMap::from([(
                "body".into(),
                (0..body.faces.len())
                    .map(|i| (FaceId(i), vec![FaceId(i)]))
                    .collect(),
            )]),
        },
    })
}

fn equivalent(a: &ModelSnapshot, b: &ModelSnapshot, error: f64) -> bool {
    let near = |x: f64, y: f64, e: f64| x.is_finite() && y.is_finite() && (x - y).abs() <= e;
    let point = |x: [f64; 3], y: [f64; 3], e: f64| x.into_iter().zip(y).all(|(a, b)| near(a, b, e));
    let direction = |x: [f64; 3], y: [f64; 3]| point(x, y, 4096. * f64::EPSILON);
    if a.loops != b.loops
        || a.shells != b.shells
        || a.solids != b.solids
        || a.vertices.len() != b.vertices.len()
        || a.edges.len() != b.edges.len()
        || a.faces.len() != b.faces.len()
    {
        return false;
    }
    if a.vertices
        .iter()
        .zip(&b.vertices)
        .any(|(a, b)| !point(a.position, b.position, error))
    {
        return false;
    }
    for (a, b) in a.edges.iter().zip(&b.edges) {
        if a.vertices != b.vertices {
            return false;
        }
        let good = match (&a.curve, &b.curve) {
            (Curve::Line(a), Curve::Line(b)) => {
                point(a.origin, b.origin, error) && direction(a.direction, b.direction)
            }
            (Curve::CircularArc(a), Curve::CircularArc(b)) => {
                point(a.circle.origin, b.circle.origin, error)
                    && direction(a.circle.axis, b.circle.axis)
                    && direction(a.circle.x_direction, b.circle.x_direction)
                    && near(a.circle.radius, b.circle.radius, error)
                    && near(a.sweep_radians, b.sweep_radians, 4096. * f64::EPSILON)
            }
            (Curve::Circle(a), Curve::Circle(b)) => {
                point(a.origin, b.origin, error)
                    && direction(a.axis, b.axis)
                    && direction(a.x_direction, b.x_direction)
                    && near(a.radius, b.radius, error)
            }
            _ => false,
        };
        if !good {
            return false;
        }
    }
    for (a, b) in a.faces.iter().zip(&b.faces) {
        if a.loops != b.loops {
            return false;
        }
        let good = match (&a.surface, &b.surface) {
            (Surface::Plane(a), Surface::Plane(b)) => {
                point(a.origin, b.origin, error) && direction(a.normal, b.normal)
            }
            (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                point(a.origin, b.origin, error)
                    && direction(a.axis, b.axis)
                    && direction(a.x_direction, b.x_direction)
                    && near(a.radius, b.radius, error)
                    && a.inward == b.inward
            }
            _ => false,
        };
        if !good {
            return false;
        }
    }
    true
}

pub(crate) fn recognize(model: &ModelSnapshot, t: Tolerance) -> Result<Geometry, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if model.faces.len() < 7
        || model.shells.len() != 1
        || model.solids.len() != 1
        || model.solids[0].shells != vec![ShellId(0)]
    {
        return Err(bad());
    }
    let lower = model.faces[0].try_plane().ok_or_else(bad)?;
    let upper = model.faces[1].try_plane().ok_or_else(bad)?;
    let (opening, count) = if model.faces[0].loops.len() > model.faces[1].loops.len() {
        (
            FaceId(0),
            model.faces[0].loops.len() - model.faces[1].loops.len(),
        )
    } else if model.faces[1].loops.len() > model.faces[0].loops.len() {
        (
            FaceId(1),
            model.faces[1].loops.len() - model.faces[0].loops.len(),
        )
    } else {
        return Err(bad());
    };
    if !(1..=4).contains(&count) || model.faces.len() < 5 * count + 3 {
        return Err(bad());
    }
    let base_f = model.faces.len() - 5 * count;
    let circles = model.faces[1 - opening.0]
        .loops
        .len()
        .checked_sub(1)
        .ok_or_else(bad)?;
    let n = base_f.checked_sub(2 + circles).ok_or_else(bad)?;
    if n < 3 {
        return Err(bad());
    }
    let (base_v, base_e, base_l, _) = (
        2 * n + 2 * circles,
        3 * n + 3 * circles,
        2 + n + 3 * circles,
        base_f,
    );
    if model.vertices.len() != base_v + 8 * count
        || model.edges.len() != base_e + 12 * count
        || model.loops.len() != base_l + 6 * count
    {
        return Err(bad());
    }
    let axis = upper.normal;
    let height = dot(sub(upper.origin, lower.origin), axis);
    let scale = model
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(model.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => vec![l.origin[0], l.origin[1], l.origin[2]],
            Curve::Circle(c) => vec![c.origin[0], c.origin[1], c.origin[2], c.radius],
            Curve::CircularArc(a) => vec![
                a.circle.origin[0],
                a.circle.origin[1],
                a.circle.origin[2],
                a.circle.radius,
            ],
        }))
        .map(f64::abs)
        .fold(1., f64::max);
    let error = (65536. * f64::EPSILON * scale).next_up();
    if !error.is_normal() || error > t.linear / 16. || !height.is_normal() {
        return Err(KernelError::NumericRange);
    }
    let floor_layer = 1 - opening.0;
    let mut surrogate = model.clone();
    surrogate.faces.truncate(base_f + 4 * count);
    surrogate.shells[0].faces = (0..surrogate.faces.len()).map(FaceId).collect();
    let mut depth = None;
    for i in 0..count {
        let floor_face = model.faces.get(base_f + 4 * count + i).ok_or_else(bad)?;
        let floor = floor_face.try_plane().ok_or_else(bad)?;
        let z = dot(sub(floor.origin, lower.origin), axis);
        let actual = if opening == FaceId(0) { z } else { height - z };
        if !actual.is_normal()
            || actual <= t.linear + error
            || height - actual <= t.linear + error
            || depth.is_some_and(|d: f64| (d - actual).abs() > error)
        {
            return Err(bad());
        }
        depth = Some(actual);
        let delta = if floor_layer == 0 {
            height - actual
        } else {
            actual - height
        };
        let v = base_v + 8 * i;
        let e = base_e + 12 * i;
        let l = base_l + 6 * i;
        let loop_id = LoopId(l + floor_layer);
        if floor_face.loops != vec![loop_id]
            || !model.faces[opening.0]
                .loops
                .contains(&LoopId(l + opening.0))
            || model.faces[floor_layer].loops.contains(&loop_id)
        {
            return Err(bad());
        }
        for vertex in &mut surrogate.vertices[v + 4 * floor_layer..v + 4 * floor_layer + 4] {
            shift(&mut vertex.position, axis, -delta);
        }
        for edge in &mut surrogate.edges[e + 4 * floor_layer..e + 4 * floor_layer + 4] {
            match &mut edge.curve {
                Curve::Line(line) => shift(&mut line.origin, axis, -delta),
                Curve::CircularArc(arc) => shift(&mut arc.circle.origin, axis, -delta),
                _ => return Err(bad()),
            }
        }
        if floor_layer == 0 {
            for edge in &mut surrogate.edges[e + 8..e + 12] {
                if let Curve::Line(line) = &mut edge.curve {
                    shift(&mut line.origin, axis, -delta)
                } else {
                    return Err(bad());
                }
            }
            for face in &mut surrogate.faces[base_f + 4 * i..base_f + 4 * i + 4] {
                match &mut face.surface {
                    Surface::Plane(plane) => shift(&mut plane.origin, axis, -delta),
                    Surface::Cylinder(cyl) => shift(&mut cyl.origin, axis, -delta),
                    _ => return Err(bad()),
                }
            }
        }
        let old = &mut surrogate.loops[loop_id.0].edges;
        *old = old
            .iter()
            .rev()
            .map(|edge| EdgeUse {
                edge: edge.edge,
                reversed: !edge.reversed,
            })
            .collect();
        surrogate.faces[floor_layer].loops.push(loop_id);
    }
    let through = crate::arc_prism::recognize(&surrogate, t)?;
    if through.slot_holes.len() != count {
        return Err(bad());
    }
    let slots = through.slot_holes.clone();
    let mut stock = through;
    stock.slot_holes.clear();
    let g = Geometry {
        stock,
        slots,
        opening,
        depth: depth.unwrap(),
        surrogate,
    };
    let expected = construct(&g)?;
    if !equivalent(model, &expected, error) {
        return Err(bad());
    }
    Ok(g)
}

pub(crate) fn volume(model: &ModelSnapshot) -> Result<f64, KernelError> {
    let scale = model
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(model.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => vec![l.origin[0], l.origin[1], l.origin[2]],
            Curve::Circle(c) => vec![c.origin[0], c.origin[1], c.origin[2], c.radius],
            Curve::CircularArc(a) => vec![
                a.circle.origin[0],
                a.circle.origin[1],
                a.circle.origin[2],
                a.circle.radius,
            ],
        }))
        .map(f64::abs)
        .fold(0., f64::max);
    let t = Tolerance {
        linear: scale * (1048576. * f64::EPSILON) * (1. + 1e-12),
    };
    recognize(model, t)?.properties(t).map(|p| p.volume)
}

impl Geometry {
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        let mut result = self.stock.properties(t)?;
        let z = if self.opening == FaceId(0) {
            self.depth / 2.
        } else {
            self.stock.height - self.depth / 2.
        };
        let ystock = cross(self.stock.axis, self.stock.x_direction);
        for slot in &self.slots {
            let r = slot.radius;
            let l = slot.centerline_length;
            let h = self.depth;
            let area = 2. * r * l + std::f64::consts::PI * r * r;
            let volume = area * h;
            let j_axis = 2. * l * r.powi(3) / 3. + std::f64::consts::PI * r.powi(4) / 4.;
            let j_cross = r * l.powi(3) / 6.
                + std::f64::consts::PI * r.powi(4) / 4.
                + 4. * l * r.powi(3) / 3.
                + std::f64::consts::PI * r * r * l * l / 4.;
            let local = [
                h * j_axis + volume * h * h / 12.,
                h * j_cross + volume * h * h / 12.,
                h * (j_axis + j_cross),
            ];
            let along = std::array::from_fn(|i| {
                self.stock.x_direction[i] * slot.direction[0] + ystock[i] * slot.direction[1]
            });
            let across = cross(self.stock.axis, along);
            let basis = [along, across, self.stock.axis];
            let tensor = std::array::from_fn(|i| {
                std::array::from_fn(|j| (0..3).map(|k| basis[k][i] * local[k] * basis[k][j]).sum())
            });
            let removed = MassProperties {
                volume,
                centroid: Some(point(&self.stock, slot.center, z)),
                central_inertia: Some(tensor),
            };
            if !volume.is_normal()
                || !finite(removed.centroid.unwrap())
                || tensor.iter().flatten().any(|v| !v.is_finite())
            {
                return Err(KernelError::NumericRange);
            }
            result = crate::keyseat::subtract_properties(result, removed)?;
        }
        Ok(result)
    }
    pub(crate) fn face_centroid(&self, id: FaceId, t: Tolerance) -> Result<[f64; 3], KernelError> {
        let (_, _, _, base_f) = sizes(&self.stock);
        if id.0 >= base_f + 4 * self.slots.len() {
            let index = id.0 - (base_f + 4 * self.slots.len());
            return self
                .slots
                .get(index)
                .map(|slot| {
                    point(
                        &self.stock,
                        slot.center,
                        if self.opening == FaceId(0) {
                            self.depth
                        } else {
                            self.stock.height - self.depth
                        },
                    )
                })
                .ok_or_else(bad);
        }
        if id.0 < 2 {
            let base = self.stock.cap_centroid(id, t)?;
            if id != self.opening {
                return Ok(base);
            }
            let area = self.stock.properties(t)?.volume / self.stock.height;
            let removed = self
                .slots
                .iter()
                .map(|s| {
                    2. * s.radius * s.centerline_length + std::f64::consts::PI * s.radius * s.radius
                })
                .sum::<f64>();
            let remain = area - removed;
            if remain <= area * 4096. * f64::EPSILON {
                return Err(KernelError::NumericRange);
            }
            let mut moment = [0.; 3];
            for s in &self.slots {
                let a = 2. * s.radius * s.centerline_length
                    + std::f64::consts::PI * s.radius * s.radius;
                let center = point(
                    &self.stock,
                    s.center,
                    if id == FaceId(0) {
                        0.
                    } else {
                        self.stock.height
                    },
                );
                for k in 0..3 {
                    moment[k] -= a * (center[k] - base[k]);
                }
            }
            return Ok(std::array::from_fn(|k| base[k] + moment[k] / remain));
        }
        Err(bad())
    }
}

pub(crate) fn tessellate(
    model: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let mut through = g.stock.clone();
    through.slot_holes = g.slots.clone();
    let mut mesh = crate::arc_prism::tessellate(&g.surrogate, &through, policy)?;
    let (_, base_e, _, base_f) = sizes(&g.stock);
    let floor_layer = 1 - g.opening.0;
    let delta = if floor_layer == 0 {
        g.stock.height - g.depth
    } else {
        g.depth - g.stock.height
    };
    let other = FaceId(floor_layer);
    let y = cross(g.stock.axis, g.stock.x_direction);
    let mut extra = Vec::new();
    for slot_index in 0..g.slots.len() {
        let face_start = base_f + 4 * slot_index;
        let edge_start = base_e + 12 * slot_index;
        let horizontal: Vec<EdgeId> = (0..4)
            .map(|j| EdgeId(edge_start + 4 * floor_layer + j))
            .collect();
        let mut ring = Vec::<[f64; 3]>::new();
        let mut ring_edges = Vec::<EdgeId>::new();
        for patch in &mut mesh.patches {
            if (face_start..face_start + 4).contains(&patch.face.0) {
                let index = if floor_layer == 0 { 1 } else { 2 };
                ring.push(patch.points[index]);
                ring_edges.push(
                    patch.boundary_edges[if floor_layer == 0 { 0 } else { 2 }].ok_or_else(bad)?,
                );
                for point in &mut patch.points[2 * floor_layer..2 * floor_layer + 2] {
                    shift(point, g.stock.axis, delta);
                }
            }
        }
        if ring.len() < 4
            || ring.len() > 2048
            || ring_edges.iter().any(|id| !horizontal.contains(id))
        {
            return Err(bad());
        }
        let reference = g.stock.origin;
        let projected: Vec<_> = ring
            .iter()
            .map(|p| {
                let d = sub(*p, reference);
                [dot(d, g.stock.x_direction), dot(d, y)]
            })
            .collect();
        let mut order: Vec<usize> = (0..ring.len()).collect();
        let signed = projected
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let q = projected[(i + 1) % projected.len()];
                p[0] * q[1] - p[1] * q[0]
            })
            .sum::<f64>();
        if signed < 0. {
            order.reverse()
        }
        let planar = order.iter().map(|&i| projected[i]).collect::<Vec<_>>();
        let tris = crate::triangulation::triangulate_rings_2d(&[planar])?;
        let native_floor = FaceId(base_f + 4 * g.slots.len() + slot_index);
        let floor_normal = model.faces[native_floor.0]
            .try_plane()
            .ok_or_else(bad)?
            .normal;
        let cap_normal = model.faces[other.0].try_plane().ok_or_else(bad)?.normal;
        for tri in tris {
            let indices = tri.map(|j| order[j]);
            for (face, normal, translated) in [
                (other, cap_normal, false),
                (native_floor, floor_normal, true),
            ] {
                let mut points = indices.map(|j| ring[j]);
                if translated {
                    for p in &mut points {
                        shift(p, g.stock.axis, delta)
                    }
                }
                let mut edge_ids = [None; 3];
                if translated {
                    for k in 0..3 {
                        let a = indices[k];
                        let b = indices[(k + 1) % 3];
                        if (a + 1) % ring.len() == b {
                            edge_ids[k] = Some(ring_edges[a])
                        } else if (b + 1) % ring.len() == a {
                            edge_ids[k] = Some(ring_edges[b])
                        }
                    }
                }
                if dot(
                    cross(sub(points[1], points[0]), sub(points[2], points[0])),
                    normal,
                ) < 0.
                {
                    points.swap(1, 2);
                    edge_ids = [edge_ids[2], edge_ids[1], edge_ids[0]];
                }
                extra.push(MeshPatch {
                    face,
                    points: points.to_vec(),
                    normal,
                    boundary_edges: edge_ids.to_vec(),
                });
            }
        }
    }
    for patch in &mut mesh.patches {
        if patch.face == other {
            for edge in &mut patch.boundary_edges {
                if edge.is_some_and(|id| {
                    (0..g.slots.len()).any(|i| {
                        let start = base_e + 12 * i + 4 * floor_layer;
                        (start..start + 4).contains(&id.0)
                    })
                }) {
                    *edge = None
                }
            }
        }
    }
    mesh.patches.extend(extra);
    let _ = t;
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn slots() -> Vec<ObroundSlot> {
        vec![
            ObroundSlot {
                id: "left".into(),
                center: [40., 40.],
                direction: [0., 1.],
                centerline_length: 24.,
                radius: 6.,
            },
            ObroundSlot {
                id: "right".into(),
                center: [80., 40.],
                direction: [0., 1.],
                centerline_length: 24.,
                radius: 6.,
            },
        ]
    }
    fn stock(t: Tolerance) -> ModelSnapshot {
        extrude_rectangle(120., 80., 12., t).unwrap().model
    }
    fn closed_mesh(mesh: &CylinderMesh) {
        let point = |p: [f64; 3]| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
        let mut edges = BTreeMap::new();
        for patch in &mesh.patches {
            for j in 0..patch.points.len() {
                let a = point(patch.points[j]);
                let b = point(patch.points[(j + 1) % patch.points.len()]);
                assert_ne!(a, b);
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_insert(0usize) += 1;
            }
        }
        assert!(
            edges.values().all(|&n| n == 2),
            "nonmanifold mesh edges: {:?}",
            edges
                .values()
                .filter(|&&n| n != 2)
                .take(8)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn exact_top_and_bottom_pockets_have_floor_and_properties() {
        let t = Tolerance::default();
        let body = stock(t);
        let area = 2. * 6. * 24. + std::f64::consts::PI * 36.;
        for (cap, z) in [(FaceId(0), 4.), (FaceId(1), 8.)] {
            let out = cut_blind_obround_slots(&body, &slots(), cap, 4., t).unwrap();
            assert!(out.model.validate(t).valid);
            assert!((out.model.volume() - (body.volume() - 2. * area * 4.)).abs() < 1e-7);
            assert_eq!(
                out.model.mass_properties(t).unwrap().volume,
                out.model.volume()
            );
            let mesh = tessellate_analytic(&out.model, t, TessellationPolicy::default()).unwrap();
            closed_mesh(&mesh);
            assert!(mesh
                .patches
                .iter()
                .any(|p| p.face == out.history.generated_faces["slot_left_floor"][0]));
            for name in ["left", "right"] {
                let walls = &out.history.generated_faces[&format!("slot_{name}_walls")];
                let floor = out.history.generated_faces[&format!("slot_{name}_floor")][0];
                assert_eq!(walls.len(), 4);
                assert!((out.model.faces[floor.0].try_plane().unwrap().origin[2] - z).abs() < 1e-9);
                assert!(
                    (out.model
                        .face_frame(floor, [0., 1., 0.], [0.; 3], t)
                        .unwrap()
                        .origin[2]
                        - z)
                        .abs()
                        < 1e-9
                );
            }
            let cut_section = out
                .model
                .section(
                    SectionPlane {
                        origin: [0., 0., if cap == FaceId(0) { 2. } else { 10. }],
                        normal: [0., 0., 1.],
                        x_direction: [1., 0., 0.],
                    },
                    t,
                )
                .unwrap();
            let solid_section = out
                .model
                .section(
                    SectionPlane {
                        origin: [0., 0., if cap == FaceId(0) { 6. } else { 2. }],
                        normal: [0., 0., 1.],
                        x_direction: [1., 0., 0.],
                    },
                    t,
                )
                .unwrap();
            assert!((cut_section.area - (120. * 80. - 2. * area)).abs() < 1e-7);
            assert!((solid_section.area - 120. * 80.).abs() < 1e-7);
            assert!(out
                .model
                .section(
                    SectionPlane {
                        origin: [0., 0., z],
                        normal: [0., 0., 1.],
                        x_direction: [1., 0., 0.]
                    },
                    t
                )
                .is_err());
        }
    }

    #[test]
    fn distant_circular_bores_and_rigid_pose_keep_native_pockets() {
        let t = Tolerance::default();
        let outer = [[0., 0.], [120., 0.], [120., 80.], [0., 80.]];
        let circles = [[12., 12.], [108., 12.], [12., 68.], [108., 68.]]
            .map(|center| CircleHole { center, radius: 3. });
        let body = extrude_polygon_with_circular_holes(&outer, &[], &circles, 12., t)
            .unwrap()
            .model;
        let base = cut_blind_obround_slots(&body, &slots(), FaceId(1), 4., t).unwrap();
        assert!(base.model.validate(t).valid);
        assert_eq!(base.model.faces[0].loops.len(), body.faces[0].loops.len());
        assert_eq!(
            base.model.faces[1].loops.len(),
            body.faces[1].loops.len() + 2
        );
        closed_mesh(&tessellate_analytic(&base.model, t, TessellationPolicy::default()).unwrap());
        let q = [0.2, 0.3, 0.4, 0.71_f64.sqrt()];
        let moved = transform(&body, [19., -23., 11.], q, t).unwrap().model;
        let out = cut_blind_obround_slots(&moved, &slots(), FaceId(0), 4., t).unwrap();
        assert!(out.model.validate(t).valid);
        let expected = body.volume() - 2. * (2. * 6. * 24. + std::f64::consts::PI * 36.) * 4.;
        assert!((out.model.mass_properties(t).unwrap().volume - expected).abs() < 1e-7);
        closed_mesh(&tessellate_analytic(&out.model, t, TessellationPolicy::default()).unwrap());
    }

    #[test]
    fn raw_corruption_and_unsupported_recut_refuse_without_panics() {
        let t = Tolerance::default();
        let body = stock(t);
        let out = cut_blind_obround_slots(&body, &slots(), FaceId(1), 4., t).unwrap();
        assert!(cut_blind_obround_slots(&out.model, &slots(), FaceId(1), 2., t).is_err());
        let mut broken = out.model.clone();
        let floor = out.history.generated_faces["slot_left_floor"][0];
        if let Surface::Plane(p) = &mut broken.faces[floor.0].surface {
            p.origin[2] += 0.001
        }
        assert!(!broken.validate(t).valid);
        assert!(broken.mass_properties(t).is_err());
        let mut broken = out.model.clone();
        broken.shells.clear();
        assert!(!broken.validate(t).valid);
        assert!(recognize(&broken, t).is_err());
        let mut broken = out.model.clone();
        let wall = out.history.generated_faces["slot_left_wall_1"][0];
        if let Surface::Cylinder(c) = &mut broken.faces[wall.0].surface {
            c.radius += 0.001
        }
        assert!(!broken.validate(t).valid);
        assert!(
            cut_blind_obround_slots(&body, &slots(), FaceId(1), 12. - t.linear / 2., t).is_err()
        );
    }
}
