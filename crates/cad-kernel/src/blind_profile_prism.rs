//! Bounded serial line/arc profile pockets. Capsule certificates continue to
//! use the separate strict capsule adapter; this module adds modeling admission.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::BTreeMap;
#[derive(Clone)]
pub(crate) struct Geometry {
    pub stock: ArcPrismGeometry,
    pub profiles: Vec<PlanarPath>,
    pub opening: FaceId,
    pub depths: Vec<f64>,
    pub surrogate: ModelSnapshot,
}
fn bad() -> KernelError {
    KernelError::Unsupported("blind profiles require up to four strictly separated simple CCW line/arc pockets on one cap, 32 segments per profile and 64 total, with positive web".into())
}
fn point(g: &ArcPrismGeometry, p: [f64; 2], z: f64) -> [f64; 3] {
    let y = cross(g.axis, g.x_direction);
    std::array::from_fn(|i| g.origin[i] + g.x_direction[i] * p[0] + y[i] * p[1] + g.axis[i] * z)
}
fn shift(p: &mut [f64; 3], axis: [f64; 3], delta: f64) {
    for i in 0..3 {
        p[i] += axis[i] * delta;
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
fn floor_anchor(p: &PlanarPath) -> [f64; 2] {
    // Preserve legacy capsule floor support origins exactly.
    if p.segments.len() == 4 {
        let centers: Vec<_> = p
            .segments
            .iter()
            .filter_map(|s| {
                if let PathSegment::ArcTo { center, .. } = s {
                    Some(*center)
                } else {
                    None
                }
            })
            .collect();
        if centers.len() == 2 {
            return [
                (centers[0][0] + centers[1][0]) / 2.,
                (centers[0][1] + centers[1][1]) / 2.,
            ];
        }
    }
    p.start
}
fn construct(g: &Geometry) -> Result<ModelSnapshot, KernelError> {
    let mut model = g.surrogate.clone();
    let (base_v, base_e, base_l, base_f) = sizes(&g.stock);
    let count = g.profiles.len();
    if g.depths.len() != count || count == 0 || count > 4 {
        return Err(bad());
    }
    let floor_layer = 1 - g.opening.0;
    let mut offset = 0;
    let mut loop_offset = 0;
    for (index, profile) in g.profiles.iter().enumerate() {
        let n = profile.segments.len();
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
        let v = base_v + 2 * offset;
        let e = base_e + 3 * offset;
        let l = base_l + loop_offset;
        for vertex in &mut model.vertices[v + n * floor_layer..v + n * floor_layer + n] {
            shift(&mut vertex.position, g.stock.axis, delta);
        }
        for edge in &mut model.edges[e + n * floor_layer..e + n * floor_layer + n] {
            match &mut edge.curve {
                Curve::Line(line) => shift(&mut line.origin, g.stock.axis, delta),
                Curve::CircularArc(arc) => shift(&mut arc.circle.origin, g.stock.axis, delta),
                _ => return Err(bad()),
            }
        }
        if floor_layer == 0 {
            for edge in &mut model.edges[e + 2 * n..e + 3 * n] {
                if let Curve::Line(line) = &mut edge.curve {
                    shift(&mut line.origin, g.stock.axis, delta)
                } else {
                    return Err(bad());
                }
            }
            for face in &mut model.faces[base_f + offset..base_f + offset + n] {
                match &mut face.surface {
                    Surface::Plane(plane) => shift(&mut plane.origin, g.stock.axis, delta),
                    Surface::Cylinder(cyl) => shift(&mut cyl.origin, g.stock.axis, delta),
                    _ => return Err(bad()),
                }
            }
        }
        let loop_id = LoopId(l + floor_layer);
        // The former opposite-cap inner loop already winds outward for
        // the pocket floor and opposite to each adjacent cavity wall.
        // Preserve its edge uses when moving it between face owners.
        model.faces[floor_layer].loops.retain(|id| *id != loop_id);
        model.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: point(&g.stock, floor_anchor(profile), z_floor),
                normal: if g.opening == FaceId(0) {
                    g.stock.axis.map(|v| -v)
                } else {
                    g.stock.axis
                },
            }),
            loops: vec![loop_id],
        });
        offset += n;
        loop_offset += n + 2;
    }
    if model.faces.len() != base_f + offset + count {
        return Err(bad());
    }
    model.shells[0].faces = (0..model.faces.len()).map(FaceId).collect();
    Ok(model)
}
fn stock(model: &ModelSnapshot, t: Tolerance) -> Result<ArcPrismGeometry, KernelError> {
    crate::arc_prism::recognize(model, t)
        .or_else(|_| crate::obround_slot::geometry(model, t).map(|v| v.0))
}
fn validate(g: &Geometry, t: Tolerance) -> Result<(), KernelError> {
    if g.opening.0 > 1
        || g.profiles.is_empty()
        || g.profiles.len() > 4
        || g.depths.len() != g.profiles.len()
        || g.profiles
            .iter()
            .any(|p| p.segments.len() < 2 || p.segments.len() > 32)
        || g.profiles.iter().map(|p| p.segments.len()).sum::<usize>() > 64
    {
        return Err(bad());
    }
    let scale = g.stock.height.max(norm(g.stock.origin));
    let guard = t.linear + 65536. * f64::EPSILON * scale;
    if g.depths
        .iter()
        .any(|d| !d.is_finite() || *d <= guard || g.stock.height - *d <= guard)
    {
        return Err(bad());
    }
    crate::arc_prism::validate_profile_regions(&g.stock, &g.profiles, t)
}
fn layout(
    stock: ArcPrismGeometry,
    profiles: Vec<PlanarPath>,
    opening: FaceId,
    depths: Vec<f64>,
    t: Tolerance,
) -> Result<Geometry, KernelError> {
    if profiles.is_empty()
        || profiles.len() > 4
        || depths.len() != profiles.len()
        || opening.0 > 1
        || profiles
            .iter()
            .any(|p| p.segments.len() < 2 || p.segments.len() > 32)
        || profiles.iter().map(|p| p.segments.len()).sum::<usize>() > 64
    {
        return Err(bad());
    }
    if depths
        .iter()
        .any(|d| !d.is_finite() || *d <= t.linear || stock.height - *d <= t.linear)
    {
        return Err(bad());
    }
    crate::arc_prism::validate_profile_regions(&stock, &profiles, t)?;
    let surrogate = crate::arc_prism::construct_with_profiles(&stock, &profiles, t)?;
    let g = Geometry {
        stock,
        profiles,
        opening,
        depths,
        surrogate,
    };
    validate(&g, t)?;
    Ok(g)
}
pub(crate) fn recognize(model: &ModelSnapshot, t: Tolerance) -> Result<Geometry, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if model.vertices.len() > 1024
        || model.edges.len() > 1536
        || model.faces.len() > 512
        || model.loops.len() > 1024
        || model
            .loops
            .iter()
            .fold(0usize, |n, l| n.saturating_add(l.edges.len()))
            > 4096
        || model.shells.len() != 1
        || model.solids.len() != 1
        || model.solids[0].shells != vec![ShellId(0)]
    {
        return Err(bad());
    }
    let caps = model.faces.get(..2).ok_or_else(bad)?;
    let opening = if caps[0].loops.len() > caps[1].loops.len() {
        0
    } else {
        1
    };
    let count = caps[opening]
        .loops
        .len()
        .checked_sub(caps[1 - opening].loops.len())
        .ok_or_else(bad)?;
    if count == 0 || count > 4 {
        return Err(bad());
    }
    let opposite = &caps[1 - opening].loops;
    let n = model
        .loops
        .get(opposite.first().ok_or_else(bad)?.0)
        .ok_or_else(bad)?
        .edges
        .len();
    let mut circles = 0;
    let mut slots = 0;
    for id in opposite.iter().skip(1) {
        match model.loops.get(id.0).ok_or_else(bad)?.edges.len() {
            1 if slots == 0 => circles += 1,
            4 => slots += 1,
            _ => return Err(bad()),
        }
    }
    if slots > 4 || circles > 32 || n > 256 {
        return Err(bad());
    }
    let (bv, be, bl, bf) = (
        2 * n + 2 * circles + 8 * slots,
        3 * n + 3 * circles + 12 * slots,
        2 + n + 3 * circles + 6 * slots,
        2 + n + circles + 4 * slots,
    );
    if model.vertices.len() < bv
        || model.edges.len() < be
        || model.loops.len() < bl
        || model.faces.len() < bf
    {
        return Err(bad());
    }
    let mut original = model.clone();
    original.vertices.truncate(bv);
    original.edges.truncate(be);
    original.loops.truncate(bl);
    original.faces.truncate(bf);
    for cap in &mut original.faces[..2] {
        cap.loops.retain(|id| id.0 < bl);
    }
    original.shells[0].faces = (0..bf).map(FaceId).collect();
    let stock = stock(&original, t)?;
    let y = cross(stock.axis, stock.x_direction);
    let local = |p: [f64; 3]| {
        let d = sub(p, stock.origin);
        [dot(d, stock.x_direction), dot(d, y)]
    };
    let mut profiles = Vec::new();
    let mut offset = 0;
    let mut loffset = 0;
    for i in 0..count {
        let lid = *caps[opening]
            .loops
            .get(opposite.len() + i)
            .ok_or_else(bad)?;
        if lid.0 != bl + loffset + opening {
            return Err(bad());
        }
        let k = model.loops.get(lid.0).ok_or_else(bad)?.edges.len();
        if !(2..=32).contains(&k) || offset + k > 64 {
            return Err(bad());
        }
        let edge_start = be + 3 * offset + opening * k;
        let mut segments = Vec::new();
        let mut start = None;
        for j in 0..k {
            let edge = model.edges.get(edge_start + j).ok_or_else(bad)?;
            let a = local(
                model
                    .vertices
                    .get(edge.vertices[0].0)
                    .ok_or_else(bad)?
                    .position,
            );
            let end = local(
                model
                    .vertices
                    .get(edge.vertices[1].0)
                    .ok_or_else(bad)?
                    .position,
            );
            start.get_or_insert(a);
            segments.push(match &edge.curve {
                Curve::Line(_) => PathSegment::LineTo { end },
                Curve::CircularArc(arc) => PathSegment::ArcTo {
                    center: local(arc.circle.origin),
                    end,
                    clockwise: arc.sweep_radians < 0.,
                },
                _ => return Err(bad()),
            });
        }
        // Authoritative shared vertex closes the contour exactly in its frame.
        if let Some(last) = segments.last_mut() {
            match last {
                PathSegment::LineTo { end } | PathSegment::ArcTo { end, .. } => {
                    *end = start.unwrap()
                }
            }
        }
        profiles.push(PlanarPath {
            start: start.unwrap(),
            segments,
        });
        offset += k;
        loffset += k + 2;
    }
    if model.faces.len() != bf + offset + count
        || model.vertices.len() != bv + 2 * offset
        || model.edges.len() != be + 3 * offset
        || model.loops.len() != bl + loffset
    {
        return Err(bad());
    }
    let mut depths = Vec::new();
    for i in 0..count {
        let floor = model.faces[bf + offset + i].try_plane().ok_or_else(bad)?;
        let z = dot(sub(floor.origin, stock.origin), stock.axis);
        depths.push(if opening == 0 { z } else { stock.height - z });
    }
    let g = layout(stock, profiles, FaceId(opening), depths, t)?;
    let expected = construct(&g)?;
    let scale = model
        .vertices
        .iter()
        .map(|v| norm(v.position))
        .fold(1., f64::max)
        .max(g.stock.height);
    let error = 65536. * f64::EPSILON * scale;
    if error > t.linear / 4.
        || !crate::blind_slot_prism::equivalent(model, &expected, error, 4096. * f64::EPSILON)
    {
        return Err(bad());
    }
    Ok(g)
}
type MachiningBody = (ArcPrismGeometry, Vec<PlanarPath>, Vec<f64>, Option<FaceId>);

fn from_body(body: &ModelSnapshot, t: Tolerance) -> Result<MachiningBody, KernelError> {
    if let Ok(g) = crate::blind_slot_prism::recognize(body, t) {
        return Ok((
            g.stock,
            g.slots.iter().map(SlotHole::path).collect(),
            g.depths,
            Some(g.opening),
        ));
    }
    if let Ok(g) = recognize(body, t) {
        return Ok((g.stock, g.profiles, g.depths, Some(g.opening)));
    }
    Ok((stock(body, t)?, vec![], vec![], None))
}
pub fn blind_profile_frame(
    body: &ModelSnapshot,
    t: Tolerance,
) -> Result<SlotStockFrame, KernelError> {
    let (g, _, _, _) = from_body(body, t)?;
    Ok(SlotStockFrame {
        origin: g.origin,
        axis: g.axis,
        x_direction: g.x_direction,
        y_direction: cross(g.axis, g.x_direction),
        height: g.height,
    })
}
pub fn cut_blind_profile(
    body: &ModelSnapshot,
    profile: &PlanarPath,
    cap: FaceId,
    depth: f64,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    if !(2..=32).contains(&profile.segments.len()) {
        return Err(bad());
    }
    let (stock, mut profiles, mut depths, oldcap) = from_body(body, t)?;
    if oldcap.is_some_and(|c| c != cap) {
        return Err(bad());
    }
    let (_, _, _, base_f) = sizes(&stock);
    let previous_segments = profiles.iter().map(|p| p.segments.len()).sum::<usize>();
    let previous_count = profiles.len();
    profiles.push(profile.clone());
    depths.push(depth);
    let g = layout(stock, profiles, cap, depths, t)?;
    let model = construct(&g)?;
    recognize(&model, t)?.properties(t)?;
    let report = model.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let n = profile.segments.len();
    let wall_start = base_f + previous_segments;
    let floor = FaceId(base_f + previous_segments + n + previous_count);
    let mut generated = BTreeMap::from([
        ("start_cap".into(), vec![FaceId(0)]),
        ("end_cap".into(), vec![FaceId(1)]),
        ("opening_cap".into(), vec![cap]),
        ("pocket_floor".into(), vec![floor]),
        (
            "pocket_walls".into(),
            (wall_start..wall_start + n).map(FaceId).collect(),
        ),
    ]);
    for i in 0..n {
        generated.insert(format!("pocket_wall_{i}"), vec![FaceId(wall_start + i)]);
    }
    let mapping = (0..body.faces.len())
        .map(|i| {
            (
                FaceId(i),
                vec![FaceId(if i >= base_f + previous_segments {
                    i + n
                } else {
                    i
                })],
            )
        })
        .collect();
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: generated,
            input_faces: BTreeMap::from([("body".into(), mapping)]),
        },
    })
}
impl Geometry {
    fn removed(&self, index: usize, t: Tolerance) -> Result<MassProperties, KernelError> {
        let mut g = self.stock.clone();
        g.path = self.profiles[index].clone();
        g.circular_holes.clear();
        g.slot_holes.clear();
        g.height = self.depths[index];
        if self.opening == FaceId(1) {
            shift(&mut g.origin, g.axis, self.stock.height - g.height);
        }
        g.properties(t)
    }
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        let mut result = self.stock.properties(t)?;
        for i in 0..self.profiles.len() {
            result = crate::keyseat::subtract_properties(result, self.removed(i, t)?)?;
        }
        Ok(result)
    }
    pub(crate) fn face_centroid(
        &self,
        model: &ModelSnapshot,
        id: FaceId,
        t: Tolerance,
    ) -> Result<[f64; 3], KernelError> {
        let floor_start = model.faces.len() - self.profiles.len();
        if id.0 >= floor_start && id.0 < model.faces.len() {
            let p = self
                .removed(id.0 - floor_start, t)?
                .centroid
                .ok_or_else(bad)?;
            let plane = model.faces[id.0].try_plane().ok_or_else(bad)?;
            let d = dot(sub(p, plane.origin), plane.normal);
            return Ok(std::array::from_fn(|i| p[i] - d * plane.normal[i]));
        }
        if id.0 < 2 {
            let center = self.stock.cap_centroid(id, t)?;
            if id != self.opening {
                return Ok(center);
            }
            let mut area = self.stock.properties(t)?.volume / self.stock.height;
            let mut sum = center.map(|x| x * area);
            for i in 0..self.profiles.len() {
                let p = self.removed(i, t)?;
                let a = p.volume / self.depths[i];
                let c = p.centroid.ok_or_else(bad)?;
                area -= a;
                for k in 0..3 {
                    sum[k] -= a * c[k];
                }
            }
            let mut c = sum.map(|v| v / area);
            let plane = model.faces[id.0].try_plane().ok_or_else(bad)?;
            let d = dot(sub(c, plane.origin), plane.normal);
            for (value, normal) in c.iter_mut().zip(plane.normal) {
                *value -= d * normal;
            }
            return Ok(c);
        }
        let f = model.faces.get(id.0).ok_or_else(bad)?;
        if f.try_plane().is_none() || f.loops.len() != 1 {
            return Err(bad());
        }
        let points = model.loop_points(f.loops[0]).ok_or_else(bad)?;
        if points.len() != 4 {
            return Err(bad());
        }
        let r = points[0];
        Ok(std::array::from_fn(|k| {
            r[k] + points.iter().map(|p| (p[k] - r[k]) / 4.).sum::<f64>()
        }))
    }
}
pub(crate) fn volume(model: &ModelSnapshot) -> Result<f64, KernelError> {
    let scale = model
        .vertices
        .iter()
        .map(|v| norm(v.position))
        .fold(1., f64::max);
    let t = Tolerance {
        linear: scale * 1048576. * f64::EPSILON,
    };
    recognize(model, t)?.properties(t).map(|p| p.volume)
}

pub(crate) fn tessellate(
    model: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let mut mesh = tessellate_native(model, g, t, policy)?;
    crate::blind_slot_prism::repair_cap_diagonals(&mut mesh)?;
    Ok(mesh)
}
fn tessellate_native(
    model: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let mut mesh = crate::arc_prism::tessellate_profile_caps(
        &g.surrogate,
        &g.stock,
        &g.profiles,
        policy,
        Some(FaceId(1 - g.opening.0)),
    )?;
    let (_, base_e, _, base_f) = sizes(&g.stock);
    let floor_layer = 1 - g.opening.0;
    let other = FaceId(floor_layer);
    let y = cross(g.stock.axis, g.stock.x_direction);
    let mut extra = Vec::new();
    let mut offset = 0;
    for slot_index in 0..g.profiles.len() {
        let n = g.profiles[slot_index].segments.len();
        let depth = g.depths[slot_index];
        let delta = if floor_layer == 0 {
            g.stock.height - depth
        } else {
            depth - g.stock.height
        };
        let face_start = base_f + offset;
        let edge_start = base_e + 3 * offset;
        let horizontal: Vec<EdgeId> = (0..n)
            .map(|j| EdgeId(edge_start + n * floor_layer + j))
            .collect();
        let mut ring = Vec::<[f64; 3]>::new();
        let mut ring_edges = Vec::<EdgeId>::new();
        for patch in &mut mesh.patches {
            if (face_start..face_start + n).contains(&patch.face.0) {
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
        if ring.len() < 3
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
        let world = order.iter().map(|&i| ring[i]).collect::<Vec<_>>();
        let tris =
            crate::triangulation::triangulate_rings_with_world(&[planar], &world, g.stock.axis)?;
        let native_floor = FaceId(
            base_f + g.profiles.iter().map(|p| p.segments.len()).sum::<usize>() + slot_index,
        );
        let floor_normal = model.faces[native_floor.0]
            .try_plane()
            .ok_or_else(bad)?
            .normal;

        for tri in tris {
            let indices = tri.map(|j| order[j]);
            for (face, normal, translated) in [(native_floor, floor_normal, true)] {
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
        offset += n;
    }
    // The opposite cap was never pocketed. Render its actual stock loops
    // directly instead of patching artificial full-height surrogate holes.
    let stock_model = crate::arc_prism::construct(&g.stock, t)?;
    let stock_mesh = crate::arc_prism::tessellate(&stock_model, &g.stock, policy)?;
    mesh.chordal_error_bound = mesh.chordal_error_bound.max(stock_mesh.chordal_error_bound);
    mesh.patches
        .extend(stock_mesh.patches.into_iter().filter(|p| p.face == other));
    mesh.patches.extend(extra);
    let _ = t;
    Ok(mesh)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f64, y: f64, w: f64, h: f64) -> PlanarPath {
        PlanarPath {
            start: [x, y],
            segments: vec![
                PathSegment::LineTo { end: [x + w, y] },
                PathSegment::LineTo {
                    end: [x + w, y + h],
                },
                PathSegment::LineTo { end: [x, y + h] },
                PathSegment::LineTo { end: [x, y] },
            ],
        }
    }
    fn rounded(x: f64, y: f64, w: f64, h: f64, r: f64) -> PlanarPath {
        PlanarPath {
            start: [x + r, y],
            segments: vec![
                PathSegment::LineTo {
                    end: [x + w - r, y],
                },
                PathSegment::ArcTo {
                    center: [x + w - r, y + r],
                    end: [x + w, y + r],
                    clockwise: false,
                },
                PathSegment::LineTo {
                    end: [x + w, y + h - r],
                },
                PathSegment::ArcTo {
                    center: [x + w - r, y + h - r],
                    end: [x + w - r, y + h],
                    clockwise: false,
                },
                PathSegment::LineTo {
                    end: [x + r, y + h],
                },
                PathSegment::ArcTo {
                    center: [x + r, y + h - r],
                    end: [x, y + h - r],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [x, y + r] },
                PathSegment::ArcTo {
                    center: [x + r, y + r],
                    end: [x + r, y],
                    clockwise: false,
                },
            ],
        }
    }
    fn local(path: &PlanarPath, f: SlotStockFrame) -> PlanarPath {
        let p = |v: [f64; 2]| {
            let d = sub([v[0], v[1], 0.], f.origin);
            [dot(d, f.x_direction), dot(d, f.y_direction)]
        };
        PlanarPath {
            start: p(path.start),
            segments: path
                .segments
                .iter()
                .map(|s| match s {
                    PathSegment::LineTo { end } => PathSegment::LineTo { end: p(*end) },
                    PathSegment::ArcTo {
                        center,
                        end,
                        clockwise,
                    } => PathSegment::ArcTo {
                        center: p(*center),
                        end: p(*end),
                        clockwise: *clockwise,
                    },
                })
                .collect(),
        }
    }
    fn assert_native_edge_incidence(model: &ModelSnapshot) {
        let mut uses = vec![Vec::new(); model.edges.len()];
        for face in &model.faces {
            for id in &face.loops {
                for edge in &model.loops[id.0].edges {
                    uses[edge.edge.0].push(edge.reversed);
                }
            }
        }
        for (id, edge) in uses.iter().enumerate() {
            assert_eq!(edge.len(), 2, "edge {id}");
            assert_ne!(edge[0], edge[1], "edge {id}");
        }
    }
    #[test]
    fn all_planar_pocket_has_oriented_native_floor() {
        let t = Tolerance::default();
        let body = extrude_rectangle(50., 50., 20., t).unwrap().model;
        for cap in [FaceId(0), FaceId(1)] {
            let profile = local(
                &rect(10., 10., 8., 12.),
                blind_profile_frame(&body, t).unwrap(),
            );
            let result = cut_blind_profile(&body, &profile, cap, 5., t).unwrap();
            assert_native_edge_incidence(&result.model);
            assert!(result.model.validate(t).valid);
            assert!((result.model.volume() - (50000. - 480.)).abs() < 1e-7);
        }
    }
    #[test]
    fn serial_variable_profiles_preserve_native_moments_and_floor_lineage() {
        let t = Tolerance::default();
        let body = extrude_rectangle(120., 100., 24., t).unwrap().model;
        for cap in [FaceId(0), FaceId(1)] {
            let profile = local(
                &rounded(10., 20., 40., 40., 4.),
                blind_profile_frame(&body, t).unwrap(),
            );
            let first = cut_blind_profile(&body, &profile, cap, 12., t).unwrap();
            let first_floor = first.history.generated_faces["pocket_floor"][0];
            let profile = local(
                &rect(70., 20., 20., 50.),
                blind_profile_frame(&first.model, t).unwrap(),
            );
            let second = cut_blind_profile(&first.model, &profile, cap, 18., t).unwrap();
            assert_eq!(
                second.history.input_faces["body"][&first_floor],
                [FaceId(first_floor.0 + 4)]
            );
            assert_native_edge_incidence(&second.model);
            let expected =
                120. * 100. * 24. - (1600. - (4. - std::f64::consts::PI) * 16.) * 12. - 1000. * 18.;
            assert!((second.model.volume() - expected).abs() < 1e-6);
            assert!((second.model.mass_properties(t).unwrap().volume - expected).abs() < 1e-6);
            assert!(crate::blind_slot_prism::recognize(&second.model, t).is_err());
            let g = recognize(&second.model, t).unwrap();
            assert_eq!(g.profiles.len(), 2);
            let mesh = tessellate(&second.model, &g, t, TessellationPolicy::default()).unwrap();
            assert!(!mesh.patches.is_empty());
            for p in mesh.patches {
                let c = cross(sub(p.points[1], p.points[0]), sub(p.points[2], p.points[0]));
                let n = norm(c);
                assert!(n > 0.);
                assert!((0..3).all(|i| (c[i] / n - p.normal[i]).abs() < 1e-9));
            }
        }
    }
    #[test]
    fn legacy_capsule_construction_is_bit_identical() {
        let t = Tolerance::default();
        let body = extrude_rectangle(100., 80., 12., t).unwrap().model;
        for cap in [FaceId(0), FaceId(1)] {
            let slot = ObroundSlot {
                id: "legacy".into(),
                center: [30., 40.],
                direction: [0., 1.],
                centerline_length: 20.,
                radius: 5.,
            };
            let old =
                cut_blind_obround_slots(&body, std::slice::from_ref(&slot), cap, 4., t).unwrap();
            let p = SlotHole {
                center: slot.center,
                direction: slot.direction,
                centerline_length: slot.centerline_length,
                radius: slot.radius,
            }
            .path();
            let new = cut_blind_profile(&body, &p, cap, 4., t).unwrap();
            assert_native_edge_incidence(&old.model);
            assert_native_edge_incidence(&new.model);
            let mut reversed = old.model.clone();
            let floor = new.history.generated_faces["pocket_floor"][0];
            let loop_id = reversed.faces[floor.0].loops[0];
            reversed.loops[loop_id.0].edges.reverse();
            for edge in &mut reversed.loops[loop_id.0].edges {
                edge.reversed = !edge.reversed;
            }
            assert!(crate::blind_slot_prism::recognize(&reversed, t).is_err());
            assert!(recognize(&reversed, t).is_err());
            assert!(!reversed.validate(t).valid);
            assert_eq!(
                serde_json::to_value(&old.model).unwrap(),
                serde_json::to_value(&new.model).unwrap()
            );
        }
    }
    fn mesh_closed(mesh: &CylinderMesh) {
        let key = |p: [f64; 3]| p.map(|v| (v * 1e8).round() as i64);
        let mut edges = BTreeMap::new();
        for patch in &mesh.patches {
            for i in 0..patch.points.len() {
                let a = key(patch.points[i]);
                let b = key(patch.points[(i + 1) % patch.points.len()]);
                assert_ne!(a, b);
                let (pair, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let e = edges.entry(pair).or_insert((0, 0));
                e.0 += 1;
                e.1 += sign;
            }
        }
        assert!(edges.values().all(|&(count, sign)| count == 2 && sign == 0));
    }
    #[test]
    fn concave_profiles_mixed_stock_poses_sections_and_refusals() {
        let t = Tolerance::default();
        let stock = extrude_path(
            &rounded(0., 0., 120., 100., 6.),
            &[
                CircleHole {
                    center: [10., 10.],
                    radius: 3.,
                },
                CircleHole {
                    center: [110., 90.],
                    radius: 3.,
                },
            ],
            24.,
            t,
        )
        .unwrap()
        .model;
        let f = obround_slot_frame(&stock, t).unwrap();
        let d = sub([10., 70., 0.], f.origin);
        let stock = cut_obround_slots(
            &stock,
            &[ObroundSlot {
                id: "mount".into(),
                center: [dot(d, f.x_direction), dot(d, f.y_direction)],
                direction: [f.x_direction[1], f.y_direction[1]],
                centerline_length: 8.,
                radius: 3.,
            }],
            t,
        )
        .unwrap()
        .model;
        // A CCW outline with a genuine clockwise inward semicircle.
        let concave = PlanarPath {
            start: [30., 20.],
            segments: vec![
                PathSegment::LineTo { end: [60., 20.] },
                PathSegment::LineTo { end: [60., 50.] },
                PathSegment::LineTo { end: [50., 50.] },
                PathSegment::ArcTo {
                    center: [45., 50.],
                    end: [40., 50.],
                    clockwise: true,
                },
                PathSegment::LineTo { end: [30., 50.] },
                PathSegment::LineTo { end: [30., 20.] },
            ],
        };
        let area = 900. - std::f64::consts::PI * 25. / 2.;
        for cap in [FaceId(0), FaceId(1)] {
            let path = local(&concave, blind_profile_frame(&stock, t).unwrap());
            let first = cut_blind_profile(&stock, &path, cap, 8., t).unwrap();
            let second_path = local(
                &rounded(75., 25., 20., 35., 3.),
                blind_profile_frame(&first.model, t).unwrap(),
            );
            let second = cut_blind_profile(&first.model, &second_path, cap, 16., t).unwrap();
            let props = second.model.mass_properties(t).unwrap();
            assert!(
                (props.volume
                    - (stock.volume()
                        - area * 8.
                        - (700. - (4. - std::f64::consts::PI) * 9.) * 16.))
                    .abs()
                    < 1e-6
            );
            for depth in [4., 12., 20.] {
                let z = if cap == FaceId(1) { 24. - depth } else { depth };
                let sec = second
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
                let expected = stock.volume() / 24.
                    - if depth < 8. { area } else { 0. }
                    - if depth < 16. {
                        700. - (4. - std::f64::consts::PI) * 9.
                    } else {
                        0.
                    };
                assert!((sec.area - expected).abs() < 1e-7);
            }
            for posed in [false, true] {
                let model = if posed {
                    transform(
                        &second.model,
                        [20., -7., 4.],
                        [0.5, 0.5, 0., std::f64::consts::FRAC_1_SQRT_2],
                        t,
                    )
                    .unwrap()
                    .model
                } else {
                    second.model.clone()
                };
                let g = recognize(&model, t).unwrap();
                let mesh = tessellate(&model, &g, t, TessellationPolicy::default())
                    .unwrap_or_else(|e| panic!("cap {cap:?} posed {posed} {e}"));
                mesh_closed(&mesh);
                for id in second.history.generated_faces["pocket_floor"].iter() {
                    model.face_frame(*id, [1., 0., 0.], [0.; 3], t).unwrap();
                }
            }
            assert!(cut_blind_profile(&second.model, &path, cap, 4., t).is_err());
            assert!(cut_blind_profile(
                &second.model,
                &local(
                    &rect(80., 70., 10., 10.),
                    blind_profile_frame(&second.model, t).unwrap()
                ),
                FaceId(1 - cap.0),
                4.,
                t
            )
            .is_err());
            assert!(cut_blind_obround_slots(
                &second.model,
                &[ObroundSlot {
                    id: "legacy".into(),
                    center: [80., 80.],
                    direction: [0., 1.],
                    centerline_length: 4.,
                    radius: 2.
                }],
                cap,
                2.,
                t
            )
            .is_err());
            let mut corrupt = second.model.clone();
            let id = second.history.generated_faces["pocket_floor"][0];
            let Surface::Plane(p) = &mut corrupt.faces[id.0].surface else {
                panic!()
            };
            p.origin[2] += 0.001;
            assert!(recognize(&corrupt, t).is_err());
        }
        for path in [
            rect(-1., 20., 5., 5.),
            rect(8., 8., 4., 4.),
            rect(10., 70., 5., 5.),
            PlanarPath {
                start: [30., 20.],
                segments: vec![
                    PathSegment::LineTo { end: [50., 40.] },
                    PathSegment::LineTo { end: [30., 40.] },
                    PathSegment::LineTo { end: [50., 20.] },
                    PathSegment::LineTo { end: [30., 20.] },
                ],
            },
        ] {
            assert!(cut_blind_profile(
                &stock,
                &local(&path, blind_profile_frame(&stock, t).unwrap()),
                FaceId(1),
                8.,
                t
            )
            .is_err());
        }
    }
}
