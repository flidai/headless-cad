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
    pub depths: Vec<f64>,
    pub surrogate: ModelSnapshot,
}

fn bad() -> KernelError {
    KernelError::Unsupported("blind obround pockets require 1..4 separated native capsules opening one canonical prism cap, each with a resolved depth and positive remaining web".into())
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
    let s = g.slot_holes.len();
    (
        2 * n + 2 * c + 8 * s,
        3 * n + 3 * c + 12 * s,
        2 + n + 3 * c + 6 * s,
        2 + n + c + 4 * s,
    )
}

fn construct(g: &Geometry) -> Result<ModelSnapshot, KernelError> {
    let mut model = g.surrogate.clone();
    let (base_v, base_e, base_l, base_f) = sizes(&g.stock);
    let count = g.slots.len();
    if g.depths.len() != count || count == 0 || count > 4 {
        return Err(bad());
    }
    let floor_layer = 1 - g.opening.0;
    for (index, slot) in g.slots.iter().enumerate() {
        let depth = g.depths[index];
        let delta = if floor_layer == 0 {
            g.stock.height - depth
        } else {
            depth - g.stock.height
        };
        let z_floor = if g.opening == FaceId(0) {
            depth
        } else {
            g.stock.height - depth
        };
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
    let (stock, mut prior_slots, mut depths) = if let Ok(old) = recognize(body, t) {
        if old.opening != cap {
            return Err(bad());
        }
        (old.stock, old.slots, old.depths)
    } else if let Ok(through) = crate::arc_prism::recognize(body, t) {
        if through.slot_holes.is_empty() || through.slot_holes.len() > 4 {
            return Err(bad());
        }
        (through, vec![], vec![])
    } else {
        let (stock, _) = crate::obround_slot::geometry(body, t)?;
        (stock, vec![], vec![])
    };
    if prior_slots.len() + slots.len() > 4 {
        return Err(KernelError::ResourceLimit(
            "at most four blind obround slots".into(),
        ));
    }
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
        prior_slots.push(SlotHole {
            center: slot.center,
            direction: slot.direction,
            centerline_length: slot.centerline_length,
            radius: slot.radius,
        });
    }
    let mut all_slots = stock.slot_holes.clone();
    all_slots.extend(prior_slots.iter().cloned());
    crate::obround_slot::validate_slots_bounded(
        &stock.path,
        &stock.circular_holes,
        &all_slots,
        8,
        t,
    )?;
    depths.extend(std::iter::repeat_n(depth, slots.len()));
    let mut through = stock.clone();
    through.slot_holes = all_slots;
    let surrogate = crate::arc_prism::construct(&through, t)?;
    let slots = prior_slots;
    Ok(Geometry {
        stock,
        slots,
        opening: cap,
        depths,
        surrogate,
    })
}

/// Cut a same-cap pattern at one depth per operation. Earlier disjoint pockets
/// on that cap retain their individual depths; coordinates use the canonical
/// stock frame resolved by the build layer.
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
    let prior_count = g.slots.len() - slots.len();
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
        let index = prior_count + i;
        let walls = (0..4)
            .map(|j| FaceId(base_f + 4 * index + j))
            .collect::<Vec<_>>();
        generated.insert(format!("slot_{}_walls", slot.id), walls.clone());
        for (j, wall) in walls.into_iter().enumerate() {
            generated.insert(format!("slot_{}_wall_{j}", slot.id), vec![wall]);
        }
        generated.insert(
            format!("slot_{}_floor", slot.id),
            vec![FaceId(base_f + 4 * g.slots.len() + index)],
        );
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: generated,
            input_faces: BTreeMap::from([(
                "body".into(),
                (0..body.faces.len())
                    .map(|i| {
                        let mapped = if prior_count > 0 && i >= base_f + 4 * prior_count {
                            i + 4 * slots.len()
                        } else {
                            i
                        };
                        (FaceId(i), vec![FaceId(mapped)])
                    })
                    .collect(),
            )]),
        },
    })
}

/// Resolve the canonical stock frame from original or already pocketed native
/// stock. Existing floor and wall supports must reconstruct exactly.
pub fn blind_obround_slot_frame(
    body: &ModelSnapshot,
    t: Tolerance,
) -> Result<SlotStockFrame, KernelError> {
    if let Ok(g) = recognize(body, t) {
        Ok(SlotStockFrame {
            origin: g.stock.origin,
            axis: g.stock.axis,
            x_direction: g.stock.x_direction,
            y_direction: cross(g.stock.axis, g.stock.x_direction),
            height: g.stock.height,
        })
    } else if let Ok(g) = crate::arc_prism::recognize(body, t) {
        if g.slot_holes.is_empty() || g.slot_holes.len() > 4 {
            return Err(bad());
        }
        Ok(SlotStockFrame {
            origin: g.origin,
            axis: g.axis,
            x_direction: g.x_direction,
            y_direction: cross(g.axis, g.x_direction),
            height: g.height,
        })
    } else {
        crate::obround_slot::obround_slot_frame(body, t)
    }
}

/// Resolve actual full-height slot witnesses only after the complete mixed
/// model reconstructs. That strict comparison covers every support in the
/// original raw model; the returned through-only model is only a redundant
/// consistency reference, never a substitute for actual section evidence.
/// Original through wall IDs form the prefix of the native face array, while
/// blind walls and floors are outside this admitted prefix.
pub(crate) fn through_slot_witness(
    model: &ModelSnapshot,
    t: Tolerance,
) -> Result<(ArcPrismGeometry, ModelSnapshot), KernelError> {
    let mixed = recognize(model, t)?;
    if mixed.stock.slot_holes.is_empty() {
        return Err(bad());
    }
    // Keep the original surviving supports: reconstructing canonical stock here
    // would erase small raw direction departures before the stricter query
    // consistency check. Only remove the already-recognized blind topology.
    let (vertices, edges, loops, faces) = sizes(&mixed.stock);
    let mut through = model.clone();
    through.vertices.truncate(vertices);
    through.edges.truncate(edges);
    through.loops.truncate(loops);
    through.faces.truncate(faces);
    for cap in &mut through.faces[..2] {
        cap.loops.retain(|id| id.0 < loops);
    }
    through.shells[0].faces = (0..faces).map(FaceId).collect();
    crate::arc_prism::recognize(&through, t)?;
    Ok((mixed.stock, through))
}

fn equivalent(a: &ModelSnapshot, b: &ModelSnapshot, error: f64, direction_error: f64) -> bool {
    let near = |x: f64, y: f64, e: f64| x.is_finite() && y.is_finite() && (x - y).abs() <= e;
    let point = |x: [f64; 3], y: [f64; 3], e: f64| x.into_iter().zip(y).all(|(a, b)| near(a, b, e));
    let direction = |x: [f64; 3], y: [f64; 3]| point(x, y, direction_error);
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
                    && near(a.sweep_radians, b.sweep_radians, direction_error)
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
    let opposite = &model.faces[1 - opening.0].loops;
    if opposite.is_empty() {
        return Err(bad());
    }
    let mut circles = 0;
    let mut through_count = 0;
    for &loop_id in opposite.iter().skip(1) {
        let size = model.loops.get(loop_id.0).ok_or_else(bad)?.edges.len();
        match size {
            1 if through_count == 0 => circles += 1,
            4 => through_count += 1,
            _ => return Err(bad()),
        }
    }
    if through_count > 4 || through_count + count > 8 {
        return Err(bad());
    }
    let n = base_f
        .checked_sub(2 + circles + 4 * through_count)
        .ok_or_else(bad)?;
    if n < 3 {
        return Err(bad());
    }
    let (base_v, base_e, base_l, _) = (
        2 * n + 2 * circles + 8 * through_count,
        3 * n + 3 * circles + 12 * through_count,
        2 + n + 3 * circles + 6 * through_count,
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
    let mut depths = Vec::with_capacity(count);
    for i in 0..count {
        let floor_face = model.faces.get(base_f + 4 * count + i).ok_or_else(bad)?;
        let floor = floor_face.try_plane().ok_or_else(bad)?;
        let z = dot(sub(floor.origin, lower.origin), axis);
        let actual = if opening == FaceId(0) { z } else { height - z };
        if !actual.is_normal() || actual <= t.linear + error || height - actual <= t.linear + error
        {
            return Err(bad());
        }
        depths.push(actual);
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
    let mut through = crate::arc_prism::recognize_bounded(&surrogate, t, 8)?;
    if through.slot_holes.len() != through_count + count {
        return Err(bad());
    }
    let slots = through.slot_holes.split_off(through_count);
    let stock = through;
    let g = Geometry {
        stock,
        slots,
        opening,
        depths,
        surrogate,
    };
    let expected = construct(&g)?;
    if !equivalent(model, &expected, error, 4096. * f64::EPSILON) {
        return Err(bad());
    }
    Ok(g)
}

/// Reconcile every actual mixed support to canonical geometry at a query's
/// own arithmetic budget; broad recognition alone is not a certificate.
pub(crate) fn consistency(
    model: &ModelSnapshot,
    g: &Geometry,
    error: f64,
    t: Tolerance,
) -> Result<(), KernelError> {
    if (crate::math::norm(g.stock.axis) - 1.).abs() > 64. * f64::EPSILON
        || (crate::math::norm(g.stock.x_direction) - 1.).abs() > 64. * f64::EPSILON
        || dot(g.stock.axis, g.stock.x_direction).abs() > 64. * f64::EPSILON
    {
        return Err(KernelError::NumericRange);
    }
    let mut canonical = g.clone();
    let mut through = g.stock.clone();
    through.slot_holes.extend(g.slots.iter().cloned());
    canonical.surrogate = crate::arc_prism::construct(&through, t)?;
    let expected = construct(&canonical)?;
    let extent = expected
        .vertices
        .iter()
        .map(|v| crate::math::norm(sub(v.position, g.stock.origin)))
        .fold(g.stock.height, f64::max);
    if !error.is_normal()
        || !extent.is_normal()
        || !equivalent(
            model,
            &expected,
            error / 3f64.sqrt(),
            (128. * f64::EPSILON).min(error / extent) / 3f64.sqrt(),
        )
    {
        return Err(KernelError::NumericRange);
    }
    Ok(())
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
        let ystock = cross(self.stock.axis, self.stock.x_direction);
        for (slot, &h) in self.slots.iter().zip(&self.depths) {
            let r = slot.radius;
            let l = slot.centerline_length;
            let z = if self.opening == FaceId(0) {
                h / 2.
            } else {
                self.stock.height - h / 2.
            };
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
    pub(crate) fn face_centroid(
        &self,
        model: &ModelSnapshot,
        id: FaceId,
        t: Tolerance,
    ) -> Result<[f64; 3], KernelError> {
        let (_, _, _, base_f) = sizes(&self.stock);
        if id.0 >= base_f + 4 * self.slots.len() {
            let index = id.0 - (base_f + 4 * self.slots.len());
            return self
                .slots
                .get(index)
                .zip(self.depths.get(index))
                .map(|(slot, &depth)| {
                    point(
                        &self.stock,
                        slot.center,
                        if self.opening == FaceId(0) {
                            depth
                        } else {
                            self.stock.height - depth
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
        // Recognition proves these are rectangular extrusion walls, including
        // shortened blind walls. Use their actual trimmed corners, never the
        // uncut stock wall or its arbitrary supporting-plane origin.
        let face = model.faces.get(id.0).ok_or_else(bad)?;
        if face.try_plane().is_none() || face.loops.len() != 1 {
            return Err(bad());
        }
        let boundary = model.loops.get(face.loops[0].0).ok_or_else(bad)?;
        if boundary.edges.len() != 4
            || boundary.edges.iter().any(|e| {
                !matches!(
                    model.edges.get(e.edge.0).map(|e| &e.curve),
                    Some(Curve::Line(_))
                )
            })
        {
            return Err(bad());
        }
        let points = model.loop_points(face.loops[0]).ok_or_else(bad)?;
        if points.len() != 4 || points.iter().any(|p| !finite(*p)) {
            return Err(bad());
        }
        let reference = points[0];
        let center = std::array::from_fn(|i| {
            reference[i]
                + points
                    .iter()
                    .map(|p| (p[i] - reference[i]) / 4.)
                    .sum::<f64>()
        });
        if !finite(center) {
            return Err(KernelError::NumericRange);
        }
        Ok(center)
    }
}

pub(crate) fn tessellate(
    model: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let mut mesh = tessellate_native(model, g, t, policy)?;
    repair_cap_diagonals(&mut mesh)?;
    Ok(mesh)
}

// Earcut may choose an almost collinear bridge among aligned holes. Repair
// only internal diagonals between two triangles of the same native face;
// retain every rim vertex and boundary-edge identity exactly.
fn repair_cap_diagonals(mesh: &mut CylinderMesh) -> Result<(), KernelError> {
    let quality = |p: &[[f64; 3]], normal: [f64; 3]| {
        let a = sub(p[1], p[0]);
        let b = sub(p[2], p[0]);
        let c = cross(a, b);
        let area = crate::math::norm(c);
        let scale = dot(a, a)
            .max(dot(b, b))
            .max(dot(sub(p[2], p[1]), sub(p[2], p[1])));
        if area > 256. * f64::EPSILON * scale
            && (0..3).all(|i| (c[i] / area - normal[i]).abs() < 1e-9)
        {
            area
        } else {
            0.
        }
    };
    for _ in 0..256 {
        let Some(i) = mesh
            .patches
            .iter()
            .position(|p| p.points.len() == 3 && quality(&p.points, p.normal) == 0.)
        else {
            return Ok(());
        };
        let a = mesh.patches[i].clone();
        let mut replacement = None;
        for j in 0..mesh.patches.len() {
            let b = &mesh.patches[j];
            if i == j || b.face != a.face || b.points.len() != 3 {
                continue;
            }
            for e in 0..3 {
                let u = a.points[e];
                let v = a.points[(e + 1) % 3];
                let c = a.points[(e + 2) % 3];
                if a.boundary_edges[e].is_some() {
                    continue;
                }
                let Some(f) = (0..3).find(|&f| {
                    b.points[f] == v && b.points[(f + 1) % 3] == u && b.boundary_edges[f].is_none()
                }) else {
                    continue;
                };
                let d = b.points[(f + 2) % 3];
                let points = [vec![c, d, v], vec![d, c, u]];
                if points.iter().any(|p| quality(p, a.normal) == 0.) {
                    continue;
                }
                let make = |points: Vec<[f64; 3]>| {
                    let boundary_edges = (0..3)
                        .map(|k| {
                            [&a, b].into_iter().find_map(|old| {
                                (0..3).find_map(|n| {
                                    (old.points[n] == points[k]
                                        && old.points[(n + 1) % 3] == points[(k + 1) % 3])
                                        .then_some(old.boundary_edges[n])
                                        .flatten()
                                })
                            })
                        })
                        .collect();
                    MeshPatch {
                        face: a.face,
                        points,
                        normal: a.normal,
                        boundary_edges,
                    }
                };
                replacement = Some((j, make(points[0].clone()), make(points[1].clone())));
                break;
            }
            if replacement.is_some() {
                break;
            }
        }
        let Some((j, first, second)) = replacement else {
            return Err(KernelError::NumericRange);
        };
        mesh.patches[i] = first;
        mesh.patches[j] = second;
    }
    Err(KernelError::ResourceLimit(
        "mixed cap diagonal repair exceeds 256 flips".into(),
    ))
}

fn tessellate_native(
    model: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let mut through = g.stock.clone();
    through.slot_holes.extend(g.slots.iter().cloned());
    let mut mesh = crate::arc_prism::tessellate(&g.surrogate, &through, policy)?;
    let (_, base_e, _, base_f) = sizes(&g.stock);
    let floor_layer = 1 - g.opening.0;
    let other = FaceId(floor_layer);
    let y = cross(g.stock.axis, g.stock.x_direction);
    let mut extra = Vec::new();
    for slot_index in 0..g.slots.len() {
        let depth = g.depths[slot_index];
        let delta = if floor_layer == 0 {
            g.stock.height - depth
        } else {
            depth - g.stock.height
        };
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

    #[test]
    fn serial_pockets_retain_distinct_floors_mass_sections_and_lineage() {
        let t = Tolerance::default();
        let body = stock(t);
        let pair = slots();
        let area = 2. * 6. * 24. + std::f64::consts::PI * 36.;
        for cap in [FaceId(0), FaceId(1)] {
            let first = cut_blind_obround_slots(&body, &pair[..1], cap, 4., t).unwrap();
            let old_floor = first.history.generated_faces["slot_left_floor"][0];
            let old_wall = first.history.generated_faces["slot_left_wall_0"][0];
            let frame = blind_obround_slot_frame(&first.model, t).unwrap();
            assert_eq!(frame.height, 12.);
            let world_center = [80., 40., 0.];
            let shifted = sub(world_center, frame.origin);
            let local_right = ObroundSlot {
                center: [
                    dot(shifted, frame.x_direction),
                    dot(shifted, frame.y_direction),
                ],
                direction: [frame.x_direction[1], frame.y_direction[1]],
                ..pair[1].clone()
            };
            let second = cut_blind_obround_slots(&first.model, &[local_right], cap, 7., t).unwrap();
            let new_floor = second.history.generated_faces["slot_right_floor"][0];
            let mapped_old_floor = second.history.input_faces["body"][&old_floor][0];
            assert_eq!(mapped_old_floor, FaceId(old_floor.0 + 4));
            assert_eq!(
                second.history.input_faces["body"][&old_wall],
                vec![old_wall]
            );
            assert!(!second
                .history
                .generated_faces
                .contains_key("slot_left_floor"));
            assert!(second.model.validate(t).valid);
            let g = recognize(&second.model, t).unwrap();
            assert_eq!(g.depths, vec![4., 7.]);
            assert!((second.model.volume() - (body.volume() - area * 11.)).abs() < 1e-7);
            assert!(
                (second.model.mass_properties(t).unwrap().volume - second.model.volume()).abs()
                    < 1e-7
            );
            closed_mesh(
                &tessellate_analytic(&second.model, t, TessellationPolicy::default()).unwrap(),
            );
            for (floor, depth) in [(mapped_old_floor, 4.), (new_floor, 7.)] {
                let expected_z = if cap == FaceId(0) { depth } else { 12. - depth };
                let frame = second
                    .model
                    .face_frame(floor, [0., 1., 0.], [0.; 3], t)
                    .unwrap();
                assert!((frame.origin[2] - expected_z).abs() < 1e-9);
            }
            for (z, removed) in if cap == FaceId(0) {
                [(2., 2.), (6., 1.), (9., 0.)]
            } else {
                [(10., 2.), (6., 1.), (3., 0.)]
            } {
                let section = second
                    .model
                    .section(
                        SectionPlane {
                            origin: [0., 0., z],
                            normal: [0., 0., 1.],
                            x_direction: [1., 0., 0.],
                        },
                        t,
                    )
                    .unwrap();
                assert!((section.area - (120. * 80. - removed * area)).abs() < 1e-7);
            }
            assert!(
                cut_blind_obround_slots(&second.model, &pair[..1], FaceId(1 - cap.0), 3., t)
                    .is_err()
            );
            assert!(cut_blind_obround_slots(&second.model, &pair[..1], cap, 3., t).is_err());
            let mut corrupt = second.model.clone();
            if let Surface::Plane(p) = &mut corrupt.faces[mapped_old_floor.0].surface {
                p.origin[2] += 0.001;
            }
            assert!(blind_obround_slot_frame(&corrupt, t).is_err());
            assert!(cut_blind_obround_slots(&corrupt, &pair[..1], cap, 3., t).is_err());
        }
    }

    #[test]
    fn actual_planar_wall_frames_survive_blind_and_mixed_machining() {
        let t = Tolerance::default();
        let stock = extrude_rectangle(100., 80., 12., t).unwrap().model;
        let outer = FaceId(2);
        let original = stock.face_frame(outer, [0., 0., 1.], [0.; 3], t).unwrap();
        let through = cut_obround_slots(
            &stock,
            &[ObroundSlot {
                id: "mount".into(),
                center: [25., 20.],
                direction: [1., 0.],
                centerline_length: 8.,
                radius: 3.,
            }],
            t,
        )
        .unwrap();
        for mixed in [false, true] {
            let (body, outer) = if mixed {
                (
                    &through.model,
                    through.history.input_faces["body"][&outer][0],
                )
            } else {
                (&stock, outer)
            };
            let frame = blind_obround_slot_frame(body, t).unwrap();
            let delta = sub([70., 50., 0.], frame.origin);
            let slot = ObroundSlot {
                id: "pocket".into(),
                center: [dot(delta, frame.x_direction), dot(delta, frame.y_direction)],
                direction: [frame.x_direction[0], frame.y_direction[0]],
                centerline_length: 10.,
                radius: 3.,
            };
            for cap in [FaceId(0), FaceId(1)] {
                let op =
                    cut_blind_obround_slots(body, std::slice::from_ref(&slot), cap, 3., t).unwrap();
                let outer = op.history.input_faces["body"][&outer][0];
                let wall = op.history.generated_faces["slot_pocket_wall_0"][0];
                let outer_frame = op
                    .model
                    .face_frame(outer, [0., 0., 1.], [0.; 3], t)
                    .unwrap();
                for k in 0..3 {
                    assert!((outer_frame.origin[k] - original.origin[k]).abs() < 1e-9);
                }
                let wall_frame = op.model.face_frame(wall, [0., 0., 1.], [0.; 3], t).unwrap();
                assert!((wall_frame.origin[0] - 70.).abs() < 1e-9);
                assert!(((wall_frame.origin[1] - 50.).abs() - 3.).abs() < 1e-9);
                assert!(
                    (wall_frame.origin[2] - if cap == FaceId(0) { 1.5 } else { 10.5 }).abs() < 1e-9
                );
                let q = [0.5, 0.5, 0., std::f64::consts::FRAC_1_SQRT_2];
                let offset = [20., -7., 11.];
                let posed = transform(&op.model, offset, q, t).unwrap().model;
                let s = std::f64::consts::FRAC_1_SQRT_2;
                for (id, expected) in [(outer, outer_frame.origin), (wall, wall_frame.origin)] {
                    let actual = posed.face_frame(id, [s, -s, 0.], [0.; 3], t).unwrap();
                    let independent = [
                        offset[0] + 0.5 * expected[0] + 0.5 * expected[1] + s * expected[2],
                        offset[1] + 0.5 * expected[0] + 0.5 * expected[1] - s * expected[2],
                        offset[2] - s * expected[0] + s * expected[1],
                    ];
                    for (actual, expected) in actual.origin.into_iter().zip(independent) {
                        assert!((actual - expected).abs() < 1e-9);
                    }
                    let mut corrupt = op.model.clone();
                    if let Surface::Plane(p) = &mut corrupt.faces[id.0].surface {
                        p.origin[0] += 0.001;
                    }
                    assert!(corrupt.face_frame(id, [0., 0., 1.], [0.; 3], t).is_err());
                }
            }
        }
    }

    #[test]
    fn preposed_mixed_caps_have_consistent_triangle_normals_and_shared_rims() {
        let t = Tolerance::default();
        let mut body = extrude_polygon_with_circular_holes(
            &[[-25., -18.], [25., -18.], [25., 18.], [-25., 18.]],
            &[],
            &[CircleHole {
                center: [0., 0.],
                radius: 3.,
            }],
            8.,
            t,
        )
        .unwrap()
        .model;
        let mut walls = [FaceId(2), FaceId(3), FaceId(4), FaceId(5)];
        for pair in [[3, 0], [0, 1], [1, 2], [2, 3]] {
            let op = fillet_longitudinal(&body, [walls[pair[0]], walls[pair[1]]], 4., t).unwrap();
            walls = walls.map(|id| op.history.input_faces["body"][&id][0]);
            body = op.model;
        }
        let q = [0.5, 0.5, 0., std::f64::consts::FRAC_1_SQRT_2];
        let translation = [25., -12., 7.];
        body = transform(&body, translation, q, t).unwrap().model;
        let world_x = [0.5, 0.5, -std::f64::consts::FRAC_1_SQRT_2];
        let world_y = [0.5, 0.5, std::f64::consts::FRAC_1_SQRT_2];
        let map_slot = |frame: &SlotStockFrame, x: f64, y: f64, id: &str| {
            let p = std::array::from_fn(|i| translation[i] + x * world_x[i] + y * world_y[i]);
            let d = sub(p, frame.origin);
            ObroundSlot {
                id: id.into(),
                center: [dot(d, frame.x_direction), dot(d, frame.y_direction)],
                direction: [
                    dot(world_x, frame.x_direction),
                    dot(world_x, frame.y_direction),
                ],
                centerline_length: 6.,
                radius: 2.,
            }
        };
        let frame = obround_slot_frame(&body, t).unwrap();
        body = cut_obround_slots(
            &body,
            &[
                map_slot(&frame, -14., 8., "left"),
                map_slot(&frame, 14., 8., "right"),
            ],
            t,
        )
        .unwrap()
        .model;
        for cap in [FaceId(0), FaceId(1)] {
            let mut mixed = body.clone();
            for (x, depth, id) in [(-14., 3., "first"), (14., 5., "second")] {
                let frame = blind_obround_slot_frame(&mixed, t).unwrap();
                mixed =
                    cut_blind_obround_slots(&mixed, &[map_slot(&frame, x, -8., id)], cap, depth, t)
                        .unwrap()
                        .model;
            }
            let mesh = tessellate_analytic(&mixed, t, TessellationPolicy::default()).unwrap();
            closed_mesh(&mesh);
            for patch in &mesh.patches {
                for i in 1..patch.points.len() - 1 {
                    let cross_product = cross(
                        sub(patch.points[i], patch.points[0]),
                        sub(patch.points[i + 1], patch.points[0]),
                    );
                    let length = crate::math::norm(cross_product);
                    assert!(length.is_normal());
                    assert!(
                        (0..3).all(|k| (cross_product[k] / length - patch.normal[k]).abs() < 1e-9),
                        "face {:?}: {:?}",
                        patch.face,
                        patch.points
                    );
                }
            }
        }
    }

    #[test]
    fn mixed_through_queries_preserve_raw_support_direction_checks() {
        let t = Tolerance::default();
        let stock = extrude_rectangle(100., 80., 12., t).unwrap().model;
        let slot = ObroundSlot {
            id: "mount".into(),
            center: [25., 20.],
            direction: [1., 0.],
            centerline_length: 8.,
            radius: 3.,
        };
        let through = cut_obround_slots(&stock, &[slot], t).unwrap();
        let walls: [FaceId; 4] = through.history.generated_faces["slot_mount_walls"]
            .clone()
            .try_into()
            .unwrap();
        let frame = blind_obround_slot_frame(&through.model, t).unwrap();
        let offset = sub([70., 60., 0.], frame.origin);
        let pocket = ObroundSlot {
            id: "pocket".into(),
            center: [
                dot(offset, frame.x_direction),
                dot(offset, frame.y_direction),
            ],
            direction: [frame.x_direction[0], frame.y_direction[0]],
            centerline_length: 10.,
            radius: 4.,
        };
        for cap in [FaceId(0), FaceId(1)] {
            let mixed =
                cut_blind_obround_slots(&through.model, std::slice::from_ref(&pocket), cap, 3., t)
                    .unwrap()
                    .model;
            for body in [&through.model, &mixed] {
                let section = body
                    .section(
                        SectionPlane {
                            origin: [0., 0., 6.],
                            normal: [0., 0., 1.],
                            x_direction: [1., 0., 0.],
                        },
                        t,
                    )
                    .unwrap();
                assert!(slot_center_frame(body, walls, cap, t).is_ok());
                assert!(measure_section_slot(body, &section, walls, t).is_ok());
                for cylinder in [false, true] {
                    let mut corrupt = body.clone();
                    if cylinder {
                        let surface = walls
                            .iter()
                            .find_map(|id| {
                                matches!(corrupt.faces[id.0].surface, Surface::Cylinder(_))
                                    .then_some(*id)
                            })
                            .unwrap();
                        if let Surface::Cylinder(c) = &mut corrupt.faces[surface.0].surface {
                            c.axis[0] += 1e-13;
                        }
                    } else {
                        let edge = walls
                            .iter()
                            .flat_map(|id| &corrupt.faces[id.0].loops)
                            .flat_map(|id| &corrupt.loops[id.0].edges)
                            .find(|e| matches!(corrupt.edges[e.edge.0].curve, Curve::Line(_)))
                            .unwrap()
                            .edge;
                        if let Curve::Line(line) = &mut corrupt.edges[edge.0].curve {
                            let component = if line.direction[0].abs() < 0.5 { 0 } else { 1 };
                            line.direction[component] += 1e-13;
                        }
                    }
                    // The bounded shape recognizer intentionally has a looser
                    // reconstruction threshold than these engineering queries.
                    if std::ptr::eq(body, &mixed) {
                        assert!(recognize(&corrupt, t).is_ok());
                    } else {
                        assert!(crate::arc_prism::recognize(&corrupt, t).is_ok());
                    }
                    assert!(slot_center_frame(&corrupt, walls, cap, t).is_err());
                    assert!(measure_section_slot(&corrupt, &section, walls, t).is_err());
                }
            }
        }
    }

    #[test]
    fn four_actual_through_slots_and_four_serial_blind_floors_reconstruct() {
        let t = Tolerance::default();
        let body = extrude_rectangle(160., 100., 12., t).unwrap().model;
        let through_slots = [25., 60., 100., 135.].map(|x| ObroundSlot {
            id: format!("through_{x}"),
            center: [x, 20.],
            direction: [1., 0.],
            centerline_length: 8.,
            radius: 3.,
        });
        let through = cut_obround_slots(&body, &through_slots, t).unwrap();
        let first_wall: [FaceId; 4] = through.history.generated_faces["slot_through_25_walls"]
            .clone()
            .try_into()
            .unwrap();
        let frame = blind_obround_slot_frame(&through.model, t).unwrap();
        let blind_slots = [25., 60., 100., 135.].map(|x| {
            let shifted = sub([x, 70., 0.], frame.origin);
            ObroundSlot {
                id: format!("blind_{x}"),
                center: [
                    dot(shifted, frame.x_direction),
                    dot(shifted, frame.y_direction),
                ],
                direction: [frame.x_direction[1], frame.y_direction[1]],
                centerline_length: 12.,
                radius: 4.,
            }
        });
        let through_area = 2. * 3. * 8. + std::f64::consts::PI * 9.;
        let blind_area = 2. * 4. * 12. + std::f64::consts::PI * 16.;
        for cap in [FaceId(0), FaceId(1)] {
            let first =
                cut_blind_obround_slots(&through.model, &blind_slots[..2], cap, 3., t).unwrap();
            let old_floor = first.history.generated_faces["slot_blind_25_floor"][0];
            let final_op =
                cut_blind_obround_slots(&first.model, &blind_slots[2..], cap, 7., t).unwrap();
            let mixed = &final_op.model;
            assert!(mixed.validate(t).valid);
            let g = recognize(mixed, t).unwrap();
            assert_eq!(g.stock.slot_holes.len(), 4);
            assert_eq!(g.slots.len(), 4);
            assert_eq!(g.depths, vec![3., 3., 7., 7.]);
            for total in [7, 8] {
                let mut private_surrogate = g.stock.clone();
                private_surrogate
                    .slot_holes
                    .extend(g.slots[..total - 4].iter().cloned());
                let raw = crate::arc_prism::construct(&private_surrogate, t).unwrap();
                assert!(crate::arc_prism::recognize_bounded(&raw, t, 8).is_ok());
                assert!(crate::arc_prism::recognize(&raw, t).is_err());
                assert!(!raw.validate(t).valid);
            }
            assert_eq!(
                final_op.history.input_faces["body"][&old_floor],
                vec![FaceId(old_floor.0 + 8)]
            );
            assert_eq!(
                final_op.history.input_faces["body"][&first_wall[0]],
                vec![first_wall[0]]
            );
            let expected = body.volume() - 4. * through_area * 12. - 2. * blind_area * (3. + 7.);
            assert!((mixed.mass_properties(t).unwrap().volume - expected).abs() < 1e-7);
            assert!((mixed.volume() - expected).abs() < 1e-7);
            closed_mesh(&tessellate_analytic(mixed, t, TessellationPolicy::default()).unwrap());
            let through_datum = slot_center_frame(mixed, first_wall, cap, t).unwrap();
            assert!((through_datum.frame.origin[0] - 25.).abs() < 1e-8);
            let plane = SectionPlane {
                origin: [0., 0., 6.],
                normal: [0., 0., 1.],
                x_direction: [1., 0., 0.],
            };
            let section = mixed.section(plane, t).unwrap();
            let measured = measure_section_slot(mixed, &section, first_wall, t).unwrap();
            assert!((measured.width_mm - 6.).abs() < 1e-8);
            assert!((measured.overall_length_mm - 14.).abs() < 1e-8);
            let blind_walls: [FaceId; 4] = final_op.history.generated_faces["slot_blind_100_walls"]
                .clone()
                .try_into()
                .unwrap();
            assert!(slot_center_frame(mixed, blind_walls, cap, t).is_err());
            assert!(measure_section_slot(mixed, &section, blind_walls, t).is_err());
            for (z, blind_count) in if cap == FaceId(0) {
                [(1., 4.), (5., 2.), (10., 0.)]
            } else {
                [(11., 4.), (7., 2.), (2., 0.)]
            } {
                let area = mixed
                    .section(
                        SectionPlane {
                            origin: [0., 0., z],
                            normal: [0., 0., 1.],
                            x_direction: [1., 0., 0.],
                        },
                        t,
                    )
                    .unwrap()
                    .area;
                assert!(
                    (area - (160. * 100. - 4. * through_area - blind_count * blind_area)).abs()
                        < 1e-7
                );
            }
            let moved = transform(mixed, [17., -31., 9.], [0.2, 0.3, 0.4, 0.71_f64.sqrt()], t)
                .unwrap()
                .model;
            assert!(recognize(&moved, t).is_ok());
            assert!(slot_center_frame(&moved, first_wall, cap, t).is_ok());
            let mut corrupt = mixed.clone();
            if let Surface::Plane(p) =
                &mut corrupt.faces[final_op.history.input_faces["body"][&old_floor][0].0].surface
            {
                p.origin[2] += 0.001;
            }
            assert!(recognize(&corrupt, t).is_err());
            assert!(slot_center_frame(&corrupt, first_wall, cap, t).is_err());
            let mut corrupt = mixed.clone();
            corrupt.faces[cap.0].loops.pop();
            assert!(recognize(&corrupt, t).is_err());
            assert!(measure_section_slot(&corrupt, &section, first_wall, t).is_err());
            let mut corrupt = mixed.clone();
            if let Surface::Cylinder(c) = &mut corrupt.faces[first_wall[1].0].surface {
                c.radius += 0.001;
            }
            assert!(through_slot_witness(&corrupt, t).is_err());
            assert!(blind_obround_slot_frame(&corrupt, t).is_err());
            assert!(cut_blind_obround_slots(mixed, &blind_slots[..1], cap, 2., t).is_err());
            assert!(certify_planar_seating(mixed, cap, &body, FaceId(1 - cap.0), 0.5, t).is_err());
        }
    }
}
