//! Native right prisms with disjoint circular through and blind cuts.
//! The boundary itself is the certificate: no construction metadata is trusted.
use crate::math::{finite, norm};
use crate::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct PrismBore {
    pub cylinder: CylinderGeometry,
    pub opening_caps: Vec<FaceId>,
    pub floor: Option<FaceId>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BoredPrismGeometry {
    pub base: ModelSnapshot,
    pub bores: Vec<PrismBore>,
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub height: f64,
    pub(crate) surrogate: ModelSnapshot,
}
fn bad() -> KernelError {
    KernelError::Unsupported("blind bore requires a canonical right prism, a parallel solid cylinder opening exactly one cap, positive depth and remaining floor thickness, and separated bore projections".into())
}
pub(crate) fn arithmetic(
    m: &ModelSnapshot,
    tool: &CylinderGeometry,
    t: Tolerance,
) -> Result<f64, KernelError> {
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(tool.origin)
        .chain([tool.radius, tool.height])
        .chain(m.edges.iter().flat_map(|e| match &e.curve {
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
    let error = scale * 4096. * f64::EPSILON;
    if !error.is_normal() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    Ok(error)
}
// This check distinguishes a true blind depth from a sub-tolerance gap. Axis-
// aligned cap equality is literal; oblique identity has an arithmetic bound.
pub(crate) fn interval(
    g: &crate::through_bore::Stock,
    c: &CylinderGeometry,
    error: f64,
    t: Tolerance,
) -> Result<(f64, f64, [bool; 2]), KernelError> {
    if c.inner_radius != 0. || norm(cross(c.axis, g.axis)) > 128. * f64::EPSILON {
        return Err(bad());
    }
    let z = dot(sub(c.origin, g.origin), g.axis);
    let (lo, hi) = if dot(c.axis, g.axis) > 0. {
        (z, z + c.height)
    } else {
        (z - c.height, z)
    };
    let exact = g.axis.iter().filter(|v| v.abs() == 1.).count() == 1
        && g.axis.iter().all(|v| *v == 0. || v.abs() == 1.);
    let same = |a: f64, b: f64| a == b || (!exact && (a - b).abs() <= error);
    let bottom = lo < -t.linear - error || same(lo, 0.);
    let top = hi > g.height + t.linear + error || same(hi, g.height);
    if !bottom && !(lo > t.linear + error && lo < g.height - t.linear - error) {
        return Err(bad());
    }
    if !top && !(hi > t.linear + error && hi < g.height - t.linear - error) {
        return Err(bad());
    }
    if !bottom && !top {
        return Err(bad());
    }
    let low = if bottom { 0. } else { lo };
    let high = if top { g.height } else { hi };
    if high - low <= t.linear + error {
        return Err(bad());
    }
    Ok((low, high, [bottom, top]))
}
pub(crate) fn disk_at(
    c: &CylinderGeometry,
    g: &crate::through_bore::Stock,
    lo: f64,
    hi: f64,
    t: Tolerance,
) -> Result<ModelSnapshot, KernelError> {
    let d = sub(c.origin, g.origin);
    let y = cross(g.axis, g.x);
    let x = dot(d, g.x);
    let ycoord = dot(d, y);
    let origin = std::array::from_fn(|i| g.origin[i] + x * g.x[i] + ycoord * y[i] + lo * g.axis[i]);
    Ok(transform(
        &extrude_circle(c.radius, hi - lo, t)?.model,
        origin,
        crate::circular_boolean::quaternion(g.x, g.axis),
        t,
    )?
    .model)
}
fn append(a: &ModelSnapshot, disk: &ModelSnapshot, opens: [bool; 2]) -> ModelSnapshot {
    let mut m = a.clone();
    let (v, e, l, f) = (
        m.vertices.len(),
        m.edges.len(),
        m.loops.len(),
        m.faces.len(),
    );
    m.vertices.extend(disk.vertices.clone());
    m.edges.extend(disk.edges.iter().cloned().map(|mut edge| {
        edge.vertices = edge.vertices.map(|id| VertexId(id.0 + v));
        edge
    }));
    let eu = |n, r| EdgeUse {
        edge: EdgeId(e + n),
        reversed: r,
    };
    m.loops.extend([
        Loop {
            edges: vec![eu(0, false)],
        },
        Loop {
            edges: vec![eu(1, true)],
        },
        Loop {
            edges: vec![eu(0, true), eu(2, false), eu(1, false), eu(2, true)],
        },
    ]);
    for (i, open) in opens.iter().enumerate() {
        if *open {
            m.faces[i].loops.push(LoopId(l + i));
        }
    }
    let mut wall = disk.faces[2].clone();
    if let Surface::Cylinder(c) = &mut wall.surface {
        c.inward = true;
    }
    wall.loops = vec![LoopId(l + 2)];
    m.faces.push(wall);
    m.shells[0].faces.push(FaceId(f));
    if let Some(i) = opens.iter().position(|open| !*open) {
        let mut floor = disk.faces[i].clone();
        if let Surface::Plane(p) = &mut floor.surface {
            p.normal = p.normal.map(|v| -v);
        }
        floor.loops = vec![LoopId(l + i)];
        m.faces.push(floor);
        m.shells[0].faces.push(FaceId(f + 1));
    }
    m
}
fn cut_surrogate(
    s: &ModelSnapshot,
    c: &CylinderGeometry,
    t: Tolerance,
) -> Result<ModelSnapshot, KernelError> {
    let g = crate::through_bore::stock(s, t)?;
    let disk = disk_at(c, &g, 0., g.height, t)?;
    crate::through_bore::cut(s, &disk, t).map(|r| r.model)
}
pub(crate) fn cut(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let surrogate = if let Ok(g) = recognize(a, t) {
        g.surrogate
    } else {
        crate::through_bore::stock(a, t)?;
        a.clone()
    };
    let g = crate::through_bore::stock(&surrogate, t)?;
    let tool = crate::analytic::recognize(b, Some(t))?;
    let error = arithmetic(a, &tool, t)?;
    let (lo, hi, opens) = interval(&g, &tool, error, t)?;
    cut_surrogate(&surrogate, &tool, t)?; // analytic material and separation certificate
    let disk = disk_at(&tool, &g, lo, hi, t)?;
    let c = crate::analytic::recognize(&disk, Some(t))?;
    for p in [
        c.origin,
        std::array::from_fn(|i| c.origin[i] + c.height * c.axis[i]),
    ] {
        if norm(cross(sub(p, tool.origin), tool.axis)) > error {
            return Err(KernelError::NumericRange);
        }
    }
    // The terminating floor retains the actual tool cap plane, including the
    // full circle's angular departure; centerline proximity alone is insufficient.
    if let Some(side) = opens.iter().position(|open| !*open) {
        let tool_cap = if (side == 0) == (dot(tool.axis, g.axis) > 0.) {
            tool.start_cap
        } else {
            tool.end_cap
        };
        let plane = b.faces[tool_cap.0].try_plane().ok_or_else(bad)?;
        let center = if side == 0 {
            c.origin
        } else {
            std::array::from_fn(|i| c.origin[i] + c.height * c.axis[i])
        };
        if dot(sub(center, plane.origin), plane.normal).abs()
            + norm(cross(c.axis, plane.normal)) * c.radius
            > error
        {
            return Err(KernelError::NumericRange);
        }
    }
    let model = append(a, &disk, opens);
    let report = model.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    model.mass_properties(t)?;
    let f = a.faces.len();
    let mut generated = BTreeMap::from([
        (
            "boundary".into(),
            (0..model.faces.len()).map(FaceId).collect(),
        ),
        ("bore_wall".into(), vec![FaceId(f)]),
    ]);
    if !opens[0] || !opens[1] {
        generated.insert("bore_floor".into(), vec![FaceId(f + 1)]);
    }
    let tool_floor = if !opens[0] {
        if dot(tool.axis, g.axis) > 0. {
            tool.start_cap
        } else {
            tool.end_cap
        }
    } else if dot(tool.axis, g.axis) > 0. {
        tool.end_cap
    } else {
        tool.start_cap
    };
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: generated,
            input_faces: BTreeMap::from([
                (
                    "left".into(),
                    (0..a.faces.len())
                        .map(|i| (FaceId(i), vec![FaceId(i)]))
                        .collect(),
                ),
                (
                    "right".into(),
                    (0..b.faces.len())
                        .map(|i| {
                            (
                                FaceId(i),
                                if FaceId(i) == tool.wall {
                                    vec![FaceId(f)]
                                } else if (!opens[0] || !opens[1]) && FaceId(i) == tool_floor {
                                    vec![FaceId(f + 1)]
                                } else {
                                    vec![]
                                },
                            )
                        })
                        .collect(),
                ),
            ]),
        },
    })
}
// Strip only exact canonical suffix blocks. Replaying and comparing the complete
// snapshot checks every reference, orientation, support, shell and unused entity.
pub(crate) fn recognize(
    m: &ModelSnapshot,
    t: Tolerance,
) -> Result<BoredPrismGeometry, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !m.has_curved_geometry()
        || m.edges
            .iter()
            .any(|e| e.vertices.iter().any(|v| v.0 >= m.vertices.len()))
        || m.loops
            .iter()
            .any(|l| l.edges.iter().any(|e| e.edge.0 >= m.edges.len()))
        || m.faces
            .iter()
            .any(|f| f.loops.iter().any(|l| l.0 >= m.loops.len()))
        || m.shells
            .iter()
            .any(|s| s.faces.iter().any(|f| f.0 >= m.faces.len()))
        || m.solids
            .iter()
            .any(|s| s.shells.iter().any(|s| s.0 >= m.shells.len()))
    {
        return Err(bad());
    }
    if m.shells.len() != 1 || m.solids.len() != 1 || m.faces.len() < 6 {
        return Err(bad());
    }
    let mut base = m.clone();
    let mut blocks = vec![];
    let mut has_blind = false;
    loop {
        if !base.has_curved_geometry() && !base.validate(t).valid {
            return Err(bad());
        }
        if crate::through_bore::stock(&base, t).is_ok() {
            break;
        }
        if blocks.len() >= MAX_POLYGON_HOLES
            || base.vertices.len() < 2
            || base.edges.len() < 3
            || base.loops.len() < 3
            || base.faces.len() < 6
        {
            return Err(bad());
        }
        let blind = matches!(base.faces.last().unwrap().surface, Surface::Plane(_));
        let nf = base.faces.len() - if blind { 2 } else { 1 };
        let Surface::Cylinder(wall) = &base.faces[nf].surface else {
            return Err(bad());
        };
        if !wall.inward {
            return Err(bad());
        }
        let (nv, ne, nl) = (
            base.vertices.len() - 2,
            base.edges.len() - 3,
            base.loops.len() - 3,
        );
        let Curve::Circle(top) = &base.edges[ne + 1].curve else {
            return Err(bad());
        };
        let mut disk = extrude_circle(wall.radius, norm(sub(top.origin, wall.origin)), t)?.model;
        disk.vertices = base.vertices[nv..].to_vec();
        disk.edges = base.edges[ne..]
            .iter()
            .cloned()
            .map(|mut e| {
                e.vertices = e
                    .vertices
                    .map(|id| VertexId(id.0.checked_sub(nv).unwrap_or(usize::MAX)));
                e
            })
            .collect();
        disk.faces[0].surface = Surface::Plane(Plane {
            origin: wall.origin,
            normal: wall.axis.map(|v| -v),
        });
        disk.faces[1].surface = Surface::Plane(Plane {
            origin: top.origin,
            normal: wall.axis,
        });
        let mut outward = wall.clone();
        outward.inward = false;
        disk.faces[2].surface = Surface::Cylinder(outward);
        crate::analytic::recognize(&disk, Some(t))?;
        let opens = [
            base.faces[0].loops.last() == Some(&LoopId(nl)),
            base.faces[1].loops.last() == Some(&LoopId(nl + 1)),
        ];
        if opens == [false, false] || blind == (opens == [true, true]) {
            return Err(bad());
        }
        let original = base.clone();
        base.vertices.truncate(nv);
        base.edges.truncate(ne);
        base.loops.truncate(nl);
        base.faces.truncate(nf);
        base.shells[0].faces.truncate(nf);
        for (i, open) in opens.iter().enumerate() {
            if *open {
                base.faces[i].loops.pop();
            }
        }
        if append(&base, &disk, opens) != original {
            return Err(bad());
        }
        blocks.push((disk, opens, nf));
        has_blind |= blind;
    }
    if !has_blind {
        return Err(bad());
    }
    if !base.validate(t).valid {
        return Err(bad());
    }
    let g = crate::through_bore::stock(&base, t)?;
    let mut surrogate = base.clone();
    let mut bores = vec![];
    for (disk, opens, f) in blocks.into_iter().rev() {
        let mut c = crate::analytic::recognize(&disk, Some(t))?;
        let error = arithmetic(m, &c, t)?;
        let (lo, hi, expected) = interval(&g, &c, error, t)?;
        if expected != opens
            || (hi - lo - c.height).abs() > error
            || norm(sub(c.axis, g.axis)) > 128. * f64::EPSILON
        {
            return Err(bad());
        }
        // Bound the full opening circles against their actual stock cap planes.
        for (i, open) in opens.iter().enumerate() {
            if *open {
                let plane = base.faces[i].try_plane().ok_or_else(bad)?;
                let center = if i == 0 {
                    c.origin
                } else {
                    std::array::from_fn(|j| c.origin[j] + c.height * c.axis[j])
                };
                if dot(sub(center, plane.origin), plane.normal).abs()
                    + norm(cross(c.axis, plane.normal)) * c.radius
                    > error
                {
                    return Err(KernelError::NumericRange);
                }
            }
        }
        surrogate = cut_surrogate(&surrogate, &c, t)?;
        c.wall = FaceId(f);
        c.start_cap = if opens[0] { FaceId(0) } else { FaceId(f + 1) };
        c.end_cap = if opens[1] { FaceId(1) } else { FaceId(f + 1) };
        bores.push(PrismBore {
            cylinder: c,
            opening_caps: opens
                .into_iter()
                .enumerate()
                .filter_map(|(i, o)| o.then_some(FaceId(i)))
                .collect(),
            floor: (!opens[0] || !opens[1]).then_some(FaceId(f + 1)),
        });
    }
    Ok(BoredPrismGeometry {
        base,
        bores,
        origin: g.origin,
        axis: g.axis,
        x_direction: g.x,
        height: g.height,
        surrogate,
    })
}
impl ModelSnapshot {
    pub fn bored_prism_geometry(
        &self,
        t: Tolerance,
    ) -> Result<Option<BoredPrismGeometry>, KernelError> {
        if !t.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if let Ok(g) = recognize(self, t) {
            return Ok(Some(g));
        }
        let r = self.validate(t);
        if r.valid {
            Ok(None)
        } else {
            Err(KernelError::InvalidInput(r))
        }
    }
}
impl BoredPrismGeometry {
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        let base = self.base.mass_properties(t)?;
        let reference = base.centroid.ok_or_else(bad)?;
        let mut volume = base.volume;
        let mut first = [0.; 3];
        let mut inertia = base.central_inertia.ok_or_else(bad)?;
        let mut absolute = inertia.map(|r| r.map(f64::abs));
        for bore in &self.bores {
            let p = bore.cylinder.properties()?;
            let d = sub(p.centroid.unwrap(), reference);
            volume -= p.volume;
            for i in 0..3 {
                first[i] -= p.volume * d[i];
                for j in 0..3 {
                    let own = p.central_inertia.unwrap()[i][j];
                    let shift = p.volume * (if i == j { dot(d, d) } else { 0. } - d[i] * d[j]);
                    inertia[i][j] -= own + shift;
                    absolute[i][j] += own.abs() + shift.abs();
                }
            }
        }
        if !volume.is_normal() || volume <= base.volume * 4096. * f64::EPSILON {
            return Err(KernelError::NumericRange);
        }
        let d = first.map(|v| v / volume);
        let centroid = std::array::from_fn(|i| reference[i] + d[i]);
        for i in 0..3 {
            for j in 0..3 {
                let shift = volume * (if i == j { dot(d, d) } else { 0. } - d[i] * d[j]);
                inertia[i][j] -= shift;
                absolute[i][j] += shift.abs();
            }
        }
        if !finite(centroid)
            || inertia
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || (*v != 0. && !v.is_normal()))
            || absolute.iter().flatten().any(|v| !v.is_finite())
            || (0..3).any(|i| inertia[i][i] <= absolute[i][i] * 4096. * f64::EPSILON)
        {
            return Err(KernelError::NumericRange);
        }
        Ok(MassProperties {
            volume,
            centroid: Some(centroid),
            central_inertia: Some(inertia),
        })
    }
    pub(crate) fn cap_centroid(&self, id: FaceId, t: Tolerance) -> Result<[f64; 3], KernelError> {
        if let Some(b) = self.bores.iter().find(|b| b.floor == Some(id)) {
            return Ok(if id == b.cylinder.start_cap {
                b.cylinder.origin
            } else {
                std::array::from_fn(|i| {
                    b.cylinder.origin[i] + b.cylinder.height * b.cylinder.axis[i]
                })
            });
        }
        if id.0 > 1 {
            return self
                .base
                .face_frame(id, self.x_direction, [0.; 3], t)
                .map(|f| f.origin);
        }
        let center = self
            .base
            .face_frame(id, self.x_direction, [0.; 3], t)?
            .origin;
        let area = self.base.mass_properties(t)?.volume / self.height;
        let mut remaining = area;
        let mut moment = [0.; 3];
        for b in &self.bores {
            if b.opening_caps.contains(&id) {
                let a = std::f64::consts::PI * b.cylinder.radius.powi(2);
                remaining -= a;
                let p = if id.0 == 0 {
                    b.cylinder.origin
                } else {
                    std::array::from_fn(|i| {
                        b.cylinder.origin[i] + b.cylinder.height * b.cylinder.axis[i]
                    })
                };
                for i in 0..3 {
                    moment[i] -= a * (p[i] - center[i]);
                }
            }
        }
        if remaining <= area * 4096. * f64::EPSILON {
            return Err(KernelError::NumericRange);
        }
        Ok(std::array::from_fn(|i| center[i] + moment[i] / remaining))
    }
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    // The convenience query has no caller tolerance. Derive its numeric margin
    // from stored support scale (including cap separation). Arc stock reserves
    // extra headroom for its signed-area first-moment integration certificate.
    let height = match (
        m.faces.first().and_then(Face::try_plane),
        m.faces.get(1).and_then(Face::try_plane),
    ) {
        (Some(a), Some(b)) => dot(sub(b.origin, a.origin), b.normal).abs(),
        _ => return Err(bad()),
    };
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(m.edges.iter().flat_map(|e| match &e.curve {
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
        .fold(height, f64::max);
    let factor = if m
        .edges
        .iter()
        .any(|e| matches!(e.curve, Curve::CircularArc(_)))
    {
        262144.
    } else {
        65536.
    };
    let t = Tolerance {
        linear: scale * (factor * f64::EPSILON) * (1. + 1e-12),
    };
    recognize(m, t)?.properties(t).map(|p| p.volume)
}
pub(crate) fn tessellate(
    g: &BoredPrismGeometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let mut mesh = tessellate_analytic(&g.surrogate, t, policy)?;
    let base_faces = g.base.faces.len();
    let original_faces: Vec<_> = mesh.patches.iter().map(|p| p.face).collect();
    let mut extra = vec![];
    for (i, b) in g.bores.iter().enumerate() {
        let surrogate_wall = FaceId(base_faces + i);
        let e = g.base.edges.len() + 3 * i;
        let low = dot(sub(b.cylinder.origin, g.origin), g.axis);
        let high = low + b.cylinder.height;
        let opens = [
            b.opening_caps.contains(&FaceId(0)),
            b.opening_caps.contains(&FaceId(1)),
        ];
        for (patch, original_face) in mesh.patches.iter_mut().zip(&original_faces) {
            if *original_face == surrogate_wall {
                patch.face = b.cylinder.wall;
                let old = patch.points.clone();
                for (index, p) in patch.points.iter_mut().enumerate() {
                    let side = usize::from(index >= 2);
                    if opens[side] {
                        continue;
                    }
                    let shift = if side == 0 { low } else { high - g.height };
                    for (j, coordinate) in p.iter_mut().enumerate() {
                        *coordinate += shift * g.axis[j];
                    }
                }
                if let Some(floor) = b.floor {
                    let side = usize::from(!opens[1]);
                    let (indices, center) = if side == 0 {
                        ([1, 0], b.cylinder.origin)
                    } else {
                        (
                            [2, 3],
                            std::array::from_fn(|j| {
                                b.cylinder.origin[j] + b.cylinder.height * g.axis[j]
                            }),
                        )
                    };
                    let normal = g.axis.map(|v| if side == 0 { v } else { -v });
                    let mut points =
                        vec![center, patch.points[indices[0]], patch.points[indices[1]]];
                    if dot(
                        cross(sub(points[1], center), sub(points[2], center)),
                        normal,
                    ) < 0.
                    {
                        points.swap(1, 2);
                    }
                    extra.push(MeshPatch {
                        face: floor,
                        points,
                        normal,
                        boundary_edges: vec![None, Some(EdgeId(e + side)), None],
                    });
                    let capnormal = normal.map(|v| -v);
                    let capcenter = std::array::from_fn(|j| {
                        b.cylinder.origin[j]
                            + (if side == 0 { -low } else { g.height - low }) * g.axis[j]
                    });
                    let mut points = vec![capcenter, old[indices[0]], old[indices[1]]];
                    if dot(
                        cross(sub(points[1], capcenter), sub(points[2], capcenter)),
                        capnormal,
                    ) < 0.
                    {
                        points.swap(1, 2);
                    }
                    extra.push(MeshPatch {
                        face: FaceId(side),
                        points,
                        normal: capnormal,
                        boundary_edges: vec![None; 3],
                    });
                }
            } else if patch.face.0 < 2 && !opens[patch.face.0] {
                for edge in &mut patch.boundary_edges {
                    if *edge == Some(EdgeId(e + patch.face.0)) {
                        *edge = None;
                    }
                }
            }
        }
    }
    mesh.patches.extend(extra);
    Ok(mesh)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < b.abs().max(1.) * 2e-9, "{a} != {b}");
    }
    fn tool(x: f64, y: f64, r: f64, z: f64, h: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        translate(&extrude_circle(r, h, t).unwrap().model, [x, y, z], t)
            .unwrap()
            .model
    }
    fn fixture() -> ModelSnapshot {
        let t = Tolerance::default();
        let mut m = extrude_rectangle(80., 40., 12., t).unwrap().model;
        for b in [
            tool(20., 13., 3., 8., 8.),
            tool(57., 26., 4., -2., 7.),
            tool(40., 20., 2., -2., 16.),
        ] {
            m = boolean(&m, &b, BooleanOp::Difference, t).unwrap().model;
        }
        m
    }
    #[test]
    fn scale_and_thin_depth_keep_native_volume_queries_consistent() {
        for scale in [1e-6, 1., 1e3] {
            let t = Tolerance {
                linear: scale * 1e-7,
            };
            let a = extrude_rectangle(80. * scale, 40. * scale, 12. * scale, t)
                .unwrap()
                .model;
            let cylinder = extrude_circle(3. * scale, 8. * scale, t).unwrap().model;
            let b = translate(&cylinder, [20. * scale, 13. * scale, 8. * scale], t)
                .unwrap()
                .model;
            let m = boolean(&a, &b, BooleanOp::Difference, t).unwrap().model;
            let expected = (38400. - 36. * std::f64::consts::PI) * scale.powi(3);
            assert!((m.volume() / expected - 1.).abs() < 1e-12);
        }
        let t = Tolerance::default();
        for width in [120., 1000., 4000.] {
            let a = extrude_rectangle(width, 40., 12., t).unwrap().model;
            let b = tool(20., 13., 3., 12. - 2e-7, 4.);
            let m = boolean(&a, &b, BooleanOp::Difference, t).unwrap().model;
            assert!((m.volume() / m.mass_properties(t).unwrap().volume - 1.).abs() < 1e-12);
        }
    }
    #[test]
    fn twelve_bore_pallet_identity_transform() {
        let t = Tolerance::default();
        let mut m = extrude_rectangle(120., 80., 28., t).unwrap().model;
        for x in [20., 60., 100.] {
            for y in [20., 60.] {
                m = boolean(&m, &tool(x, y, 8., 16., 14.), BooleanOp::Difference, t)
                    .unwrap()
                    .model;
            }
        }
        for x in [40., 80.] {
            m = boolean(&m, &tool(x, 40., 4., -2., 8.), BooleanOp::Difference, t)
                .unwrap()
                .model;
        }
        for x in [8., 112.] {
            for y in [8., 72.] {
                m = boolean(&m, &tool(x, y, 2.5, -2., 32.), BooleanOp::Difference, t)
                    .unwrap()
                    .model;
            }
        }
        let moved = transform(&m, [0.; 3], [0., 0., 0., 1.], t);
        assert!(moved.is_ok(), "{moved:?}");
        for b in recognize(&m, t).unwrap().bores {
            if let Some(f) = b.floor {
                assert!(m.face_frame(f, [1., 0., 0.], [0.; 3], t).is_ok());
            }
        }
    }
    #[test]
    fn exact_independent_full_tensor_and_native_floor_datums() {
        let t = Tolerance::default();
        let m = fixture();
        let p = m.mass_properties(t).unwrap();
        let pi = std::f64::consts::PI;
        let items = [
            (
                38400.,
                [40., 20., 6.],
                [80f64.powi(2) / 12., 40f64.powi(2) / 12., 12.],
            ),
            (-36. * pi, [20., 13., 10.], [2.25, 2.25, 16. / 12.]),
            (-80. * pi, [57., 26., 2.5], [4., 4., 25. / 12.]),
            (-48. * pi, [40., 20., 6.], [1., 1., 12.]),
        ];
        let v = items.iter().map(|(v, _, _)| v).sum::<f64>();
        let c: [f64; 3] =
            std::array::from_fn(|i| items.iter().map(|(v, c, _)| v * c[i]).sum::<f64>() / v);
        let tensor: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                items
                    .iter()
                    .map(|(v, p, variance)| {
                        let d = sub(*p, c);
                        v * ((if i == j {
                            variance.iter().sum::<f64>() - variance[i] + dot(d, d)
                        } else {
                            0.
                        }) - d[i] * d[j])
                    })
                    .sum()
            })
        });
        near(p.volume, v);
        near(m.volume(), v);
        for (i, row) in tensor.iter().enumerate() {
            near(p.centroid.unwrap()[i], c[i]);
            for (j, expected) in row.iter().enumerate() {
                near(p.central_inertia.unwrap()[i][j], *expected);
            }
        }
        let a = 0.4_f64;
        let r = [
            [1., 0., 0.],
            [0., a.cos(), -a.sin()],
            [0., a.sin(), a.cos()],
        ];
        let offset = [12., -7., 4.];
        let placed = transform(&m, offset, [(a / 2.).sin(), 0., 0., (a / 2.).cos()], t)
            .unwrap()
            .model;
        let q = placed.mass_properties(t).unwrap();
        near(q.volume, v);
        for i in 0..3 {
            near(
                q.centroid.unwrap()[i],
                offset[i] + (0..3).map(|j| r[i][j] * c[j]).sum::<f64>(),
            );
            for j in 0..3 {
                near(
                    q.central_inertia.unwrap()[i][j],
                    (0..3)
                        .flat_map(|k| (0..3).map(move |l| r[i][k] * tensor[k][l] * r[j][l]))
                        .sum(),
                );
            }
        }
        let g = m.bored_prism_geometry(t).unwrap().unwrap();
        assert_eq!(g.bores.len(), 3);
        for (id, origin, normal) in [
            (FaceId(7), [20., 13., 8.], [0., 0., 1.]),
            (FaceId(9), [57., 26., 5.], [0., 0., -1.]),
        ] {
            let frame = m.face_frame(id, [1., 0., 0.], [0.; 3], t).unwrap();
            assert_eq!(frame.origin, origin);
            assert_eq!(frame.z_axis, normal);
        }
        let top = m.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], t).unwrap();
        near(
            top.origin[0],
            (3200. * 40. - 9. * pi * 20. - 4. * pi * 40.) / (3200. - 13. * pi),
        );
        near(
            top.origin[1],
            (3200. * 20. - 9. * pi * 13. - 4. * pi * 20.) / (3200. - 13. * pi),
        );
        assert!(placed.cylinder_geometry(t).unwrap().is_none());
        assert!(placed.profile_prism_geometry(t).unwrap().is_none());
        assert!(placed.arc_prism_geometry(t).unwrap().is_none());
        assert!(placed.revolve_geometry(t).unwrap().is_none());
    }
    // Exact shared mesh vertices are essential: a close-looking display must
    // not hide cracks in STL or disagreement between cap and wall provenance.
    fn closed_mesh(m: &ModelSnapshot, t: Tolerance) {
        let mesh = tessellate_analytic(m, t, TessellationPolicy::default()).unwrap();
        let mut edges = BTreeMap::<([u64; 3], [u64; 3]), (usize, i32)>::new();
        let key = |p: [f64; 3]| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
        for patch in mesh.patches {
            assert!(patch.face.0 < m.faces.len());
            for i in 1..patch.points.len() - 1 {
                let tri = [patch.points[0], patch.points[i], patch.points[i + 1]];
                assert!(
                    dot(
                        cross(sub(tri[1], tri[0]), sub(tri[2], tri[0])),
                        patch.normal
                    ) > 0.
                );
                for j in 0..3 {
                    let a = key(tri[j]);
                    let b = key(tri[(j + 1) % 3]);
                    let (pair, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                    let e = edges.entry(pair).or_default();
                    e.0 += 1;
                    e.1 += sign;
                }
            }
        }
        assert!(
            edges.values().all(|v| *v == (2, 0)),
            "unmatched edges: {}",
            edges.values().filter(|v| **v != (2, 0)).count()
        );
    }
    #[test]
    fn shared_mesh_is_closed_in_oblique_frame_with_correct_face_ids() {
        let t = Tolerance::default();
        let m = fixture();
        closed_mesh(&m, t);
        let q = [0.5, 0.5, 0., 0.5_f64.sqrt()];
        closed_mesh(&transform(&m, [12., -7., 4.], q, t).unwrap().model, t);
    }
    #[test]
    fn lineage_preserves_caps_walls_and_attributes_tool_floor() {
        let t = Tolerance::default();
        let mut m = extrude_rectangle(80., 40., 12., t).unwrap().model;
        for (b, tool_cap) in [
            (tool(20., 13., 3., 8., 8.), 0),
            (tool(57., 26., 4., -2., 7.), 1),
        ] {
            let before = m.clone();
            let result = boolean(&m, &b, BooleanOp::Difference, t).unwrap();
            let f = m.faces.len();
            assert_eq!(
                result.history.generated_faces["bore_floor"],
                vec![FaceId(f + 1)]
            );
            assert_eq!(
                result.history.input_faces["right"][&FaceId(tool_cap)],
                vec![FaceId(f + 1)]
            );
            assert!(result.history.input_faces["right"][&FaceId(1 - tool_cap)].is_empty());
            for i in 0..f {
                assert_eq!(
                    result.history.input_faces["left"][&FaceId(i)],
                    vec![FaceId(i)]
                );
            }
            m = result.model;
            assert_eq!(m.vertices[..before.vertices.len()], before.vertices);
            assert_eq!(m.edges[..before.edges.len()], before.edges);
            assert_eq!(m.faces[2..f], before.faces[2..]);
        }
        let reverse = transform(
            &extrude_circle(2., 8., t).unwrap().model,
            [40., 20., 16.],
            [1., 0., 0., 0.],
            t,
        )
        .unwrap()
        .model;
        let r = boolean(&m, &reverse, BooleanOp::Difference, t).unwrap();
        assert_eq!(
            r.history.input_faces["right"][&FaceId(1)],
            vec![FaceId(m.faces.len() + 1)]
        );
        near(
            r.model.mass_properties(t).unwrap().volume,
            m.volume() - 16. * std::f64::consts::PI,
        );
    }
    #[test]
    fn boundary_corruptions_fail_closed_without_panics() {
        let t = Tolerance::default();
        let m = fixture();
        let mutations: Vec<fn(&mut ModelSnapshot)> = vec![
            |m| m.faces[7].try_plane_mut().unwrap().origin[2] += 1e-9,
            |m| m.faces[7].try_plane_mut().unwrap().normal[2] = -1.,
            |m| m.faces[7].loops[0] = LoopId(0),
            |m| m.faces[0].loops.push(LoopId(0)),
            |m| m.vertices[8].position[0] += 1e-9,
            |m| m.edges[12].vertices[0] = VertexId(usize::MAX),
            |m| m.loops[6].edges[0].reversed = true,
            |m| m.shells[0].faces.pop().map(drop).unwrap(),
            |m| m.solids[0].shells.clear(),
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[6].surface {
                    c.inward = false;
                }
            },
        ];
        for (i, mutate) in mutations.into_iter().enumerate() {
            let mut bad = m.clone();
            mutate(&mut bad);
            assert!(!bad.validate(t).valid, "mutation {i}");
            assert!(bad.mass_properties(t).is_err(), "mutation {i}");
            assert!(bad.volume().is_nan(), "mutation {i}");
            assert!(
                tessellate_analytic(&bad, t, TessellationPolicy::default()).is_err(),
                "mutation {i}"
            );
        }
    }
    #[test]
    fn contact_enclosed_overlap_and_uncertain_depth_are_rejected() {
        let t = Tolerance::default();
        let m = fixture();
        for b in [
            tool(20., 13., 1., -2., 4.),
            tool(21., 13., 3., 8., 8.),
            tool(0., 20., 2., 8., 8.),
            tool(65., 12., 2., 2., 3.),
            tool(65., 12., 2., 1e-12, 16.),
            tool(65., 12., 2., 12. - t.linear / 2., 8.),
            tool(65., 12., 2., -2., 2. + t.linear / 2.),
        ] {
            assert!(boolean(&m, &b, BooleanOp::Difference, t).is_err());
        }
        let skew = transform(
            &tool(65., 12., 2., 8., 8.),
            [0.; 3],
            [0., 1e-7, 0., (1. - 1e-14_f64).sqrt()],
            t,
        )
        .unwrap()
        .model;
        assert!(boolean(&m, &skew, BooleanOp::Difference, t).is_err());
    }
    #[test]
    fn two_arc_outer_profile_retains_exact_small_topology() {
        let t = Tolerance::default();
        let path = PlanarPath {
            start: [20., 0.],
            segments: vec![
                PathSegment::ArcTo {
                    end: [-20., 0.],
                    center: [0., 0.],
                    clockwise: false,
                },
                PathSegment::ArcTo {
                    end: [20., 0.],
                    center: [0., 0.],
                    clockwise: false,
                },
            ],
        };
        let a = extrude_path(&path, &[], 12., t).unwrap().model;
        assert_eq!(a.faces.len(), 4);
        let m = boolean(&a, &tool(0., 0., 3., 8., 8.), BooleanOp::Difference, t)
            .unwrap()
            .model;
        assert!(volume(&m).is_ok(), "{:?}", volume(&m));
        near(m.volume(), (4800. - 36.) * std::f64::consts::PI);
        closed_mesh(&m, t);
        let left = extrude_circle(20., 12., t).unwrap().model;
        let right = translate(&left, [20., 0., 0.], t).unwrap().model;
        let lens = boolean(&left, &right, BooleanOp::Intersection, t)
            .unwrap()
            .model;
        let semicircle = extrude_path(
            &PlanarPath {
                start: [20., 0.],
                segments: vec![
                    PathSegment::ArcTo {
                        end: [-20., 0.],
                        center: [0., 0.],
                        clockwise: false,
                    },
                    PathSegment::LineTo { end: [20., 0.] },
                ],
            },
            &[],
            12.,
            t,
        )
        .unwrap()
        .model;
        for (base, xy) in [(lens, [10., 0.]), (semicircle, [0., 8.])] {
            assert_eq!(base.faces.len(), 4);
            let volume = base.mass_properties(t).unwrap().volume;
            let result = boolean(
                &base,
                &tool(xy[0], xy[1], 3., 8., 8.),
                BooleanOp::Difference,
                t,
            )
            .unwrap()
            .model;
            near(result.volume(), volume - 36. * std::f64::consts::PI);
            closed_mesh(&result, t);
        }
    }
    #[test]
    fn existing_mixed_holes_and_concave_arc_profile_compose() {
        let t = Tolerance::default();
        let polygon = extrude_polygon_with_circular_holes(
            &[[0., 0.], [80., 0.], [80., 40.], [0., 40.]],
            &[vec![[4., 4.], [4., 8.], [8., 8.], [8., 4.]]],
            &[CircleHole {
                center: [70., 30.],
                radius: 2.,
            }],
            12.,
            t,
        )
        .unwrap()
        .model;
        let path = PlanarPath {
            start: [0., 0.],
            segments: vec![
                PathSegment::LineTo { end: [80., 0.] },
                PathSegment::LineTo { end: [80., 40.] },
                PathSegment::LineTo { end: [50., 40.] },
                PathSegment::ArcTo {
                    end: [30., 40.],
                    center: [40., 40.],
                    clockwise: true,
                },
                PathSegment::LineTo { end: [0., 40.] },
                PathSegment::LineTo { end: [0., 0.] },
            ],
        };
        let arc = extrude_path(
            &path,
            &[CircleHole {
                center: [70., 30.],
                radius: 2.,
            }],
            12.,
            t,
        )
        .unwrap()
        .model;
        for base in [polygon, arc] {
            let original = base.mass_properties(t).unwrap().volume;
            let m = boolean(&base, &tool(20., 13., 3., 8., 8.), BooleanOp::Difference, t)
                .unwrap()
                .model;
            let m = boolean(&m, &tool(57., 26., 3., -2., 7.), BooleanOp::Difference, t)
                .unwrap()
                .model;
            near(m.volume(), original - 81. * std::f64::consts::PI);
            closed_mesh(&m, t);
            closed_mesh(
                &transform(&m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                    .unwrap()
                    .model,
                t,
            );
        }
    }
}
