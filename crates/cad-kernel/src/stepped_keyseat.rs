//! A finite rectangular seat grafted into one cylindrical segment of a stepped shaft.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
const MAX_STEPS: usize = 16;
#[derive(Clone)]
pub(crate) struct Geometry {
    pub stock: RevolveGeometry,
    pub seat: crate::keyseat::Geometry,
    selected: usize,
}
fn bad() -> KernelError {
    KernelError::Unsupported("stepped keyseat requires a solid monotone orthogonal meridian with at most 16 axial segments and a finite seat strictly inside one constant-radius segment".into())
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}
fn point(g: &RevolveGeometry, p: [f64; 2]) -> [f64; 3] {
    add(g.origin, add(mul(g.x_direction, p[0]), mul(g.axis, p[1])))
}
fn certify_stock(g: &RevolveGeometry, t: Tolerance) -> Result<(), KernelError> {
    let p = &g.profile;
    if p.len() < 6
        || p.len() > 2 * MAX_STEPS + 2
        || p[0][0] != 0.
        || p.last().unwrap()[0] != 0.
        || p[0][1] != p[1][1]
        || p[p.len() - 2][1] != p[p.len() - 1][1]
    {
        return Err(bad());
    }
    crate::revolve::validate_revolve_profile(p, t)?;
    for w in p.windows(2) {
        if w[1][1] < w[0][1] || (w[0][0] != w[1][0] && w[0][1] != w[1][1]) {
            return Err(bad());
        }
    }
    if p[1..p.len() - 1].iter().any(|p| p[0] <= t.linear) {
        return Err(bad());
    }
    Ok(())
}
struct Graft {
    model: ModelSnapshot,
    stock_edges: Vec<Vec<EdgeId>>,
    seat_edges: Vec<EdgeId>,
    seat_faces: Vec<FaceId>,
}
fn construct(g: &Geometry) -> Graft {
    let (stock, sg) = crate::revolve::construct(RevolveGeometry {
        segments: vec![],
        ..g.stock.clone()
    });
    let seat = crate::keyseat::construct(&g.seat);
    let seg = &sg.segments[g.selected];
    let removed = [
        seg.start_circle.unwrap(),
        seg.end_circle.unwrap(),
        seg.seam.unwrap(),
    ];
    let mut m = ModelSnapshot {
        vertices: stock.vertices.clone(),
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    let vo = m.vertices.len();
    m.vertices.extend(seat.vertices.iter().cloned());
    let mut stock_edges = vec![vec![]; stock.edges.len()];
    for (i, e) in stock.edges.iter().enumerate() {
        if !removed.contains(&EdgeId(i)) {
            stock_edges[i].push(EdgeId(m.edges.len()));
            m.edges.push(e.clone());
        }
    }
    let seat_edges: Vec<_> = seat
        .edges
        .iter()
        .map(|e| {
            let id = EdgeId(m.edges.len());
            m.edges.push(Edge {
                vertices: e.vertices.map(|v| VertexId(vo + v.0)),
                curve: e.curve.clone(),
            });
            id
        })
        .collect();
    stock_edges[removed[0].0] = seat_edges[0..3].to_vec();
    stock_edges[removed[1].0] = seat_edges[3..6].to_vec();
    let mut seat_faces = vec![FaceId(usize::MAX); 11];
    seat_faces[2] = FaceId(g.selected);
    for (i, face) in seat_faces.iter_mut().enumerate().skip(3) {
        *face = FaceId(stock.faces.len() + i - 3);
    }
    let append =
        |m: &mut ModelSnapshot, face: &Face, loops: &[Loop], mapping: &Vec<Vec<EdgeId>>| {
            let mut ids = vec![];
            for l in &face.loops {
                let mut edges = vec![];
                for e in &loops[l.0].edges {
                    let mapped = &mapping[e.edge.0];
                    if e.reversed {
                        edges.extend(mapped.iter().rev().map(|id| EdgeUse {
                            edge: *id,
                            reversed: true,
                        }));
                    } else {
                        edges.extend(mapped.iter().map(|id| EdgeUse {
                            edge: *id,
                            reversed: false,
                        }));
                    }
                }
                ids.push(LoopId(m.loops.len()));
                m.loops.push(Loop { edges });
            }
            m.faces.push(Face {
                surface: face.surface.clone(),
                loops: ids,
            });
        };
    let smap = seat_edges.iter().map(|id| vec![*id]).collect();
    for (i, f) in stock.faces.iter().enumerate() {
        if i == g.selected {
            append(&mut m, &seat.faces[2], &seat.loops, &smap);
        } else {
            append(&mut m, f, &stock.loops, &stock_edges);
        }
    }
    for f in &seat.faces[3..] {
        append(&mut m, f, &seat.loops, &smap);
    }
    // Deleted periodic seam endpoints must not survive as unused native vertices.
    let used: BTreeSet<_> = m
        .edges
        .iter()
        .flat_map(|e| e.vertices.map(|v| v.0))
        .collect();
    let remap: BTreeMap<_, _> = used
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, VertexId(new)))
        .collect();
    m.vertices = used.iter().map(|i| m.vertices[*i].clone()).collect();
    for e in &mut m.edges {
        e.vertices = e.vertices.map(|v| remap[&v.0]);
    }
    m.shells.push(Shell {
        faces: (0..m.faces.len()).map(FaceId).collect(),
    });
    m.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    Graft {
        model: m,
        stock_edges,
        seat_edges,
        seat_faces,
    }
}
fn ring_supports(m: &ModelSnapshot, f: &Face) -> Option<Vec<Circle>> {
    let mut out = vec![];
    for l in &f.loops {
        for e in &m.loops.get(l.0)?.edges {
            let edge = m.edges.get(e.edge.0)?;
            let circle = match &edge.curve {
                Curve::Circle(c) => Some(c),
                Curve::CircularArc(a) => Some(&a.circle),
                _ => None,
            };
            if let Some(c) = circle {
                if !out
                    .iter()
                    .any(|old: &Circle| old.radius == c.radius && old.origin == c.origin)
                {
                    out.push(c.clone());
                }
            }
        }
    }
    Some(out)
}
pub(crate) fn recognize(m: &ModelSnapshot, t: Tolerance) -> Result<Geometry, KernelError> {
    if m.faces.len() < 13
        || m.faces.len() > 2 * MAX_STEPS + 9
        || m.vertices.len() > 2 * MAX_STEPS + 20
        || m.edges.len() > 4 * MAX_STEPS + 30
        || m.loops.len() > 4 * MAX_STEPS + 20
        || m.shells.len() != 1
        || m.solids.len() != 1
        || m.shells[0].faces.len() != m.faces.len()
        || m.solids[0].shells != [ShellId(0)]
        || m.faces.iter().any(|f| f.loops.len() > 2)
        || m.loops.iter().any(|l| l.edges.len() > 6)
    {
        return Err(bad());
    }
    let n = m.faces.len() - 8;
    let selected = (0..n)
        .find(|i| {
            matches!(m.faces[*i].surface, Surface::Cylinder(_))
                && m.faces[*i].loops.len() == 1
                && m.loops
                    .get(m.faces[*i].loops[0].0)
                    .is_some_and(|l| l.edges.len() == 6)
        })
        .ok_or_else(bad)?;
    let Surface::Cylinder(cyl) = &m.faces[selected].surface else {
        return Err(bad());
    };
    let axis = cyl.axis;
    let origin = m.faces[0].try_plane().ok_or_else(bad)?.origin;
    let x = m.faces[..n]
        .iter()
        .enumerate()
        .find_map(|(i, f)| {
            if i != selected {
                if let Surface::Cylinder(c) = &f.surface {
                    Some(c.x_direction)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(bad)?;
    let mut profile = vec![];
    for (i, f) in m.faces[..n].iter().enumerate() {
        let rings = ring_supports(m, f).ok_or_else(bad)?;
        let mut points: Vec<_> = rings
            .iter()
            .map(|c| [c.radius, dot(sub(c.origin, origin), axis)])
            .collect();
        let pair = match &f.surface {
            Surface::Plane(p) => {
                if points.is_empty() || points.len() > 2 {
                    return Err(bad());
                }
                if points.len() == 1 {
                    points.push([0., points[0][1]]);
                }
                points.sort_by(|a, b| a[0].total_cmp(&b[0]));
                if dot(p.normal, axis) > 0. {
                    points.reverse();
                }
                [points[0], points[1]]
            }
            Surface::Cylinder(_) => {
                if points.len() != 2 {
                    return Err(bad());
                }
                points.sort_by(|a, b| a[1].total_cmp(&b[1]));
                [points[0], points[1]]
            }
            _ => return Err(bad()),
        };
        if i == 0 {
            profile.push(pair[0]);
        }
        profile.push(pair[1]);
    }
    // Shared shoulder planes can project to neighboring floating values after
    // rigid placement. Reconcile only the arithmetic envelope, then certify
    // every native circle, plane, vertex and loop against the reconstruction.
    let support_scale = m
        .vertices
        .iter()
        .map(|v| norm(v.position))
        .fold(norm(origin), f64::max)
        .max(profile.iter().map(|p| p[0] + p[1].abs()).fold(0., f64::max));
    let level_error = 4096. * f64::EPSILON * support_scale;
    if !level_error.is_normal() || level_error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let mut levels: Vec<f64> = vec![];
    for p in &mut profile {
        if let Some(level) = levels.iter().find(|z| (**z - p[1]).abs() <= level_error) {
            p[1] = *level;
        } else {
            levels.push(p[1]);
        }
    }
    let (_, stock) = crate::revolve::construct(RevolveGeometry {
        origin,
        axis,
        x_direction: x,
        profile,
        segments: vec![],
    });
    certify_stock(&stock, t)?;
    let seg = &stock.segments[selected];
    let floor = m.faces[n + 3].try_plane().ok_or_else(bad)?;
    let negative = m.faces[n + 4].try_plane().ok_or_else(bad)?;
    let start = m.faces[n + 6].try_plane().ok_or_else(bad)?;
    let end = m.faces[n + 7].try_plane().ok_or_else(bad)?;
    let sx = floor.normal;
    let sy = cross(axis, sx);
    let local = point(&stock, [0., seg.start[1]]);
    let seat = crate::keyseat::Geometry {
        origin: local,
        axis,
        x: sx,
        r: seg.start[0],
        h: seg.end[1] - seg.start[1],
        a: dot(sub(floor.origin, local), sx),
        b: -dot(sub(negative.origin, local), sy),
        c: dot(sub(start.origin, local), axis),
        d: dot(sub(end.origin, local), axis),
    };
    seat.properties(t)?;
    let g = Geometry {
        stock,
        seat,
        selected,
    };
    let expected = construct(&g).model;
    let error = 4096.
        * f64::EPSILON
        * (norm(g.stock.origin)
            + g.stock
                .profile
                .iter()
                .map(|p| p[0] + p[1].abs())
                .fold(0., f64::max));
    if !error.is_normal() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    compare(m, &expected, error)?;
    Ok(g)
}
fn compare(m: &ModelSnapshot, e: &ModelSnapshot, error: f64) -> Result<(), KernelError> {
    if m.vertices.len() != e.vertices.len()
        || m.edges.len() != e.edges.len()
        || m.loops != e.loops
        || m.shells != e.shells
        || m.solids != e.solids
    {
        return Err(bad());
    }
    let near = |a, b| norm(sub(a, b)) <= error;
    let dir = |a, b| norm(sub(a, b)) <= 128. * f64::EPSILON;
    let circle = |a: &Circle, b: &Circle| {
        near(a.origin, b.origin)
            && (a.radius - b.radius).abs() <= error
            && dir(a.axis, b.axis)
            && dir(a.x_direction, b.x_direction)
    };
    for (a, b) in m.vertices.iter().zip(&e.vertices) {
        if !near(a.position, b.position) {
            return Err(bad());
        }
    }
    for (a, b) in m.edges.iter().zip(&e.edges) {
        if a.vertices != b.vertices {
            return Err(bad());
        }
        let okay = match (&a.curve, &b.curve) {
            (Curve::Circle(a), Curve::Circle(b)) => circle(a, b),
            (Curve::CircularArc(a), Curve::CircularArc(b)) => {
                circle(&a.circle, &b.circle)
                    && (a.sweep_radians - b.sweep_radians).abs() * a.circle.radius <= error
            }
            (Curve::Line(x), Curve::Line(y)) => {
                near(x.origin, y.origin)
                    && (norm(x.direction) - 1.).abs() <= 64. * f64::EPSILON
                    && norm(sub(x.direction, y.direction))
                        * norm(sub(
                            m.vertices[a.vertices[1].0].position,
                            m.vertices[a.vertices[0].0].position,
                        ))
                        <= error
            }
            _ => false,
        };
        if !okay {
            return Err(bad());
        }
    }
    for (a, b) in m.faces.iter().zip(&e.faces) {
        if a.loops != b.loops {
            return Err(bad());
        }
        let okay = match (&a.surface, &b.surface) {
            (Surface::Plane(a), Surface::Plane(b)) => {
                near(a.origin, b.origin) && dir(a.normal, b.normal)
            }
            (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                near(a.origin, b.origin)
                    && dir(a.axis, b.axis)
                    && dir(a.x_direction, b.x_direction)
                    && (a.radius - b.radius).abs() <= error
                    && a.inward == b.inward
            }
            _ => false,
        };
        if !okay {
            return Err(bad());
        }
    }
    Ok(())
}
fn cylinder(
    g: &RevolveGeometry,
    s: &RevolveSegment,
    t: Tolerance,
) -> Result<ModelSnapshot, KernelError> {
    let mut m = extrude_circle(s.start[0], s.end[1] - s.start[1], t)?.model;
    let origin = point(g, [0., s.start[1]]);
    let y = cross(g.axis, g.x_direction);
    let vector = |p: [f64; 3]| {
        add(
            mul(g.x_direction, p[0]),
            add(mul(y, p[1]), mul(g.axis, p[2])),
        )
    };
    let world = |p| add(origin, vector(p));
    for v in &mut m.vertices {
        v.position = world(v.position);
    }
    for e in &mut m.edges {
        match &mut e.curve {
            Curve::Circle(c) => {
                c.origin = world(c.origin);
                c.axis = vector(c.axis);
                c.x_direction = vector(c.x_direction);
            }
            Curve::Line(l) => {
                l.origin = world(l.origin);
                l.direction = vector(l.direction);
            }
            _ => return Err(bad()),
        }
    }
    for f in &mut m.faces {
        match &mut f.surface {
            Surface::Plane(p) => {
                p.origin = world(p.origin);
                p.normal = vector(p.normal);
            }
            Surface::Cylinder(c) => {
                c.origin = world(c.origin);
                c.axis = vector(c.axis);
                c.x_direction = vector(c.x_direction);
            }
            _ => return Err(bad()),
        }
    }
    Ok(m)
}
pub(crate) fn cut(
    stock: &ModelSnapshot,
    tool: &ModelSnapshot,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let g = crate::revolve::recognize(stock, t)?;
    certify_stock(&g, t)?;
    let error = 4096.
        * f64::EPSILON
        * (norm(g.origin)
            + g.profile
                .iter()
                .map(|p| p[0] + p[1].abs())
                .fold(0., f64::max));
    if error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    crate::convex_clearance::support::revolution_consistency(stock, &g, error)?;
    for (selected, s) in g.segments.iter().enumerate() {
        if s.start[0] != s.end[0] || s.end[1] <= s.start[1] {
            continue;
        }
        let Ok(local) = crate::keyseat::cut(&cylinder(&g, s, t)?, tool, t) else {
            continue;
        };
        let seat = crate::keyseat::recognize(&local.model, t)?;
        let geometry = Geometry {
            stock: g.clone(),
            seat,
            selected,
        };
        let graft = construct(&geometry);
        recognize(&graft.model, t)?;
        geometry.properties(t)?;
        let mut left: BTreeMap<_, _> = (0..stock.faces.len())
            .map(|i| (FaceId(i), vec![FaceId(i)]))
            .collect();
        left.insert(
            FaceId(selected),
            (2..6).map(|i| graft.seat_faces[i]).collect(),
        );
        let right = local
            .history
            .input_faces
            .get("right")
            .ok_or_else(bad)?
            .iter()
            .map(|(id, faces)| (*id, faces.iter().map(|f| graft.seat_faces[f.0]).collect()))
            .collect();
        let mut generated = BTreeMap::new();
        for (role, faces) in &local.history.generated_faces {
            if role.starts_with("keyseat_") {
                generated.insert(
                    role.clone(),
                    faces.iter().map(|f| graft.seat_faces[f.0]).collect(),
                );
            }
        }
        generated.insert(
            "walls".into(),
            (2..6).map(|i| graft.seat_faces[i]).collect(),
        );
        return Ok(OperationResult {
            model: graft.model,
            history: TopologyHistory {
                generated_faces: generated,
                input_faces: BTreeMap::from([("left".into(), left), ("right".into(), right)]),
            },
        });
    }
    Err(bad())
}
impl Geometry {
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        let base = self.stock.properties()?;
        let removed = self.seat.slot().properties(t)?;
        crate::keyseat::subtract_properties(base, removed)
    }
    pub(crate) fn face_centroid(&self, id: FaceId, t: Tolerance) -> Result<[f64; 3], KernelError> {
        let n = self.stock.segments.len();
        if id.0 < n && id.0 != self.selected {
            let s = &self.stock.segments[id.0];
            if s.start[1] == s.end[1] {
                return Ok(point(&self.stock, [0., s.start[1]]));
            }
            return Err(bad());
        }
        self.seat.face_centroid(
            FaceId(if id.0 == self.selected {
                2
            } else {
                id.0 - n + 3
            }),
            t,
        )
    }
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    recognize(m, Tolerance::default())?
        .properties(Tolerance::default())
        .map(|p| p.volume)
}
pub(crate) fn bounds(m: &ModelSnapshot) -> Result<Bounds, KernelError> {
    let g = recognize(m, Tolerance::default())?;
    let pad = 32768.
        * f64::EPSILON
        * (norm(g.stock.origin)
            + g.stock
                .profile
                .iter()
                .map(|p| p[0] + p[1].abs())
                .fold(0., f64::max));
    let mut bounds = crate::revolve::construct(RevolveGeometry {
        segments: vec![],
        ..g.stock
    })
    .0
    .bounds();
    for i in 0..3 {
        bounds.min[i] = (bounds.min[i] - pad).next_down();
        bounds.max[i] = (bounds.max[i] + pad).next_up();
    }
    Ok(bounds)
}
pub(crate) fn tessellate(
    _m: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let graft = construct(g);
    let local = crate::keyseat::construct(&g.seat);
    let maxr = g.stock.profile.iter().map(|p| p[0]).fold(0., f64::max);
    let error = 4096.
        * f64::EPSILON
        * (norm(g.stock.origin)
            + g.stock
                .profile
                .iter()
                .map(|p| p[0] + p[1].abs())
                .fold(0., f64::max));
    let reserve = 16. * error;
    let allowed = policy.chordal_error - reserve;
    if !allowed.is_normal() || allowed <= 0. {
        return Err(KernelError::NumericRange);
    }
    let display = crate::keyseat::tessellate(
        &local,
        &g.seat,
        t,
        TessellationPolicy {
            chordal_error: allowed * g.seat.r / maxr / 2.,
            max_segments: policy.max_segments,
        },
    )?;
    let mut rings = vec![];
    for face in [0, 1] {
        let center = add(
            g.seat.origin,
            mul(g.seat.axis, if face == 0 { 0. } else { g.seat.h }),
        );
        let mut points: Vec<_> = display
            .patches
            .iter()
            .filter(|p| p.face == FaceId(face))
            .flat_map(|p| p.points.iter().copied())
            .filter(|p| norm(sub(*p, center)) > g.seat.r / 2.)
            .collect();
        let y = cross(g.seat.axis, g.seat.x);
        points.sort_by(|a, b| {
            let a = sub(*a, center);
            let b = sub(*b, center);
            dot(a, y)
                .atan2(dot(a, g.seat.x))
                .total_cmp(&dot(b, y).atan2(dot(b, g.seat.x)))
        });
        points.dedup();
        rings.push(points);
    }
    if rings[0].len() != rings[1].len() || rings[0].len() < 3 {
        return Err(KernelError::NumericRange);
    }
    let lowz = g.stock.segments[g.selected].start[1];
    let hiz = g.stock.segments[g.selected].end[1];
    let ringpoint = |p: [f64; 2], i: usize| {
        if p[0] == g.seat.r && p[1] == lowz {
            return rings[0][i];
        }
        if p[0] == g.seat.r && p[1] == hiz {
            return rings[1][i];
        }
        let radial = mul(sub(rings[0][i], g.seat.origin), p[0] / g.seat.r);
        add(point(&g.stock, [0., p[1]]), radial)
    };
    let mut patches: Vec<_> = display
        .patches
        .into_iter()
        .filter(|p| p.face.0 >= 2)
        .map(|mut p| {
            p.face = graft.seat_faces[p.face.0];
            for id in p.boundary_edges.iter_mut().flatten() {
                *id = graft.seat_edges[id.0];
            }
            p
        })
        .collect();
    if rings[0].len().saturating_mul(g.stock.segments.len()) > 200000 {
        return Err(KernelError::ResourceLimit(
            "stepped keyseat display exceeds 200000 patches".into(),
        ));
    }
    for (id, s) in g.stock.segments.iter().enumerate() {
        if id == g.selected {
            continue;
        }
        for i in 0..rings[0].len() {
            let j = (i + 1) % rings[0].len();
            let mut points = vec![
                ringpoint(s.start, i),
                ringpoint(s.start, j),
                ringpoint(s.end, j),
                ringpoint(s.end, i),
            ];
            // Meridian traversal produces outward quads; axis vertices collapse to
            // triangle fans only on the two original disk ends.
            if s.start[0] == 0. {
                points.remove(0);
            }
            if s.end[0] == 0. {
                points.pop();
            }
            let n = cross(sub(points[1], points[0]), sub(points[2], points[0]));
            let length = norm(n);
            if !length.is_normal() {
                return Err(KernelError::NumericRange);
            }
            let normal = mul(n, 1. / length);
            let circle_edge = |e: Option<EdgeId>, radius: f64| {
                e.and_then(|e| {
                    let ids = &graft.stock_edges[e.0];
                    if ids.len() == 1 {
                        Some(ids[0])
                    } else {
                        let mid = mul(
                            add(
                                sub(rings[0][i], g.seat.origin),
                                sub(rings[0][j], g.seat.origin),
                            ),
                            0.5,
                        );
                        let theta =
                            dot(mid, cross(g.seat.axis, g.seat.x)).atan2(dot(mid, g.seat.x));
                        let angle = (g.seat.b / g.seat.r).asin();
                        let group = if theta >= -angle && theta <= angle {
                            0
                        } else if theta > angle {
                            1
                        } else {
                            2
                        };
                        (radius > 0.).then_some(ids[group])
                    }
                })
            };
            let lo = circle_edge(s.start_circle, s.start[0]);
            let hi = circle_edge(s.end_circle, s.end[0]);
            let boundary_edges = if s.start[0] == 0. {
                vec![None, hi, None]
            } else if s.end[0] == 0. {
                vec![lo, None, None]
            } else {
                vec![lo, None, hi, None]
            };
            patches.push(MeshPatch {
                face: FaceId(id),
                points,
                normal,
                boundary_edges,
            });
        }
    }
    let bound = display.chordal_error_bound * maxr / g.seat.r + reserve;
    if bound > policy.chordal_error {
        return Err(KernelError::NumericRange);
    }
    Ok(CylinderMesh {
        patches,
        segments: display.segments,
        chordal_error_bound: bound,
    })
}
impl Geometry {
    pub(crate) fn clearance_envelope(&self) -> crate::trim_clearance::Envelope {
        crate::trim_clearance::Envelope {
            origin: self.stock.origin,
            axis: self.stock.axis,
            radial: self.seat.x,
            outer: self.stock.profile.iter().map(|p| p[0]).fold(0., f64::max),
            inner: 0.,
            height: self.stock.profile.last().unwrap()[1],
            untouched_limit: Some(self.seat.a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ModelSnapshot, ModelSnapshot) {
        fixture_scaled(1.)
    }
    fn fixture_scaled(scale: f64) -> (ModelSnapshot, ModelSnapshot) {
        let t = Tolerance::default();
        let stock = revolve_polygon(
            &[
                [0., 0.],
                [8., 0.],
                [8., 12.9],
                [6., 12.9],
                [6., 94.55],
                [5.4, 94.55],
                [5.4, 95.8],
                [6., 95.8],
                [6., 130.],
                [0., 130.],
            ]
            .map(|p| p.map(|v| v * scale)),
            t,
        )
        .unwrap()
        .model;
        let tool = translate(
            &extrude_rectangle(4. * scale, 4.04 * scale, 13.5 * scale, t)
                .unwrap()
                .model,
            [4. * scale, -2.02 * scale, 45. * scale],
            t,
        )
        .unwrap()
        .model;
        (stock, tool)
    }
    #[test]
    fn stepped_keyseat_scaled_transformed_source_and_display_budget() {
        let t = Tolerance::default();
        let (a, b) = fixture();
        let volume = cut(&a, &b, t).unwrap().model.volume();
        for scale in [0.01_f64, 0.5, 2., 10.] {
            let (a, b) = fixture_scaled(scale);
            let q = [0.5, 0.5, 0., 0.5_f64.sqrt()];
            let a = transform(&a, [25., -12., 7.], q, t).unwrap().model;
            let b = transform(&b, [25., -12., 7.], q, t).unwrap().model;
            let result = cut(&a, &b, t)
                .unwrap_or_else(|e| panic!("scale {scale}: {e}"))
                .model;
            assert!(
                (result.volume() - volume * scale.powi(3)).abs() < 1e-8 * scale.powi(3).max(1.)
            );
            assert!(tessellate_analytic(
                &result,
                t,
                TessellationPolicy {
                    chordal_error: 0.01,
                    max_segments: 8
                }
            )
            .is_err());
        }
    }
    #[test]
    fn stepped_keyseat_full_moments_match_piecewise_cylinders_minus_strip() {
        let t = Tolerance::default();
        let (a, b) = fixture();
        let m = cut(&a, &b, t).unwrap().model;
        let mut volume = 0.;
        let mut mz = 0.;
        let mut xx = 0.;
        let mut zz = 0.;
        for (r, lo, hi) in [
            (8_f64, 0_f64, 12.9_f64),
            (6., 12.9, 94.55),
            (5.4, 94.55, 95.8),
            (6., 95.8, 130.),
        ] {
            let area = std::f64::consts::PI * r * r;
            volume += area * (hi - lo);
            mz += area * (hi * hi - lo * lo) / 2.;
            xx += area * r * r * (hi - lo) / 4.;
            zz += area * (hi.powi(3) - lo.powi(3)) / 3.;
        }
        let r = 6_f64;
        let b = 2.02_f64;
        let a = 4_f64;
        let c = 45_f64;
        let d = 58.5_f64;
        let q = (r * r - b * b).sqrt();
        let theta = (b / r).asin();
        let area = b * q + r * r * theta - 2. * a * b;
        let mx = b * (r * r - a * a) - b.powi(3) / 3.;
        let sx = (r.powi(4)
            * (3. * theta / 4. + (2. * theta).sin() / 2. + (4. * theta).sin() / 16.)
            - 2. * b * a.powi(3))
            / 3.;
        let sy = r.powi(4) * (theta / 4. - (4. * theta).sin() / 16.) - 2. * a * b.powi(3) / 3.;
        volume -= area * (d - c);
        let centroid = [
            -mx * (d - c) / volume,
            0.,
            (mz - area * (d * d - c * c) / 2.) / volume,
        ];
        let yy = xx - sy * (d - c);
        xx -= sx * (d - c);
        zz -= area * (d.powi(3) - c.powi(3)) / 3.;
        let xz = -mx * (d * d - c * c) / 2.;
        let second = [[xx, 0., xz], [0., yy, 0.], [xz, 0., zz]];
        let actual = m.mass_properties(t).unwrap();
        assert!((actual.volume - volume).abs() < 1e-8);
        for i in 0..3 {
            assert!((actual.centroid.unwrap()[i] - centroid[i]).abs() < 1e-10);
            for j in 0..3 {
                let expected = if i == j {
                    (0..3)
                        .filter(|k| *k != i)
                        .map(|k| second[k][k] - volume * centroid[k] * centroid[k])
                        .sum()
                } else {
                    -second[i][j] + volume * centroid[i] * centroid[j]
                };
                assert!(
                    (actual.central_inertia.unwrap()[i][j] - expected).abs() < 1e-5,
                    "{i} {j} {} {expected}",
                    actual.central_inertia.unwrap()[i][j]
                );
            }
        }
        for (z, r) in [(6., 8.), (90., 6.), (95., 5.4), (100., 6.)] {
            let section = m
                .section(
                    SectionPlane {
                        origin: [0., 0., z],
                        normal: [0., 0., 1.],
                        x_direction: [1., 0., 0.],
                    },
                    t,
                )
                .unwrap();
            assert!((section.area - std::f64::consts::PI * r * r).abs() < 1e-9);
        }
        let section = m
            .section(
                SectionPlane {
                    origin: [0., 0., 50.],
                    normal: [0., 0., 1.],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        assert!((section.area - (std::f64::consts::PI * 36. - area)).abs() < 1e-9);
    }
    #[test]
    fn stepped_keyseat_pose_native_mutations_and_meridian_clearance() {
        let t = Tolerance::default();
        let (a, b) = fixture();
        let original = cut(&a, &b, t).unwrap().model;
        for pose in [[0., 0., 0., 1.], [0.5, 0.5, 0., 0.5_f64.sqrt()]] {
            let offset = [25., -12., 7.];
            let a = transform(&a, offset, pose, t).unwrap().model;
            let b = transform(&b, offset, pose, t).unwrap().model;
            let m = cut(&a, &b, t).unwrap().model;
            let direct = transform(&original, offset, pose, t).unwrap().model;
            assert!((m.volume() - direct.volume()).abs() < 1e-8);
            let mesh = tessellate_analytic(&m, t, TessellationPolicy::default()).unwrap();
            assert!(mesh.chordal_error_bound <= 0.01);
            let sleeve = translate(
                &extrude_annulus(10., 6.15, 10., t).unwrap().model,
                [0., 0., 45.],
                t,
            )
            .unwrap()
            .model;
            let sleeve = transform(&sleeve, offset, pose, t).unwrap().model;
            for (l, r) in [(&m, &sleeve), (&sleeve, &m)] {
                let q = l.clearance(r, t).unwrap();
                assert!(q.lower_bound_mm <= 0.15 && q.upper_bound_mm >= 0.15);
            }
            // This ring fits the actual groove; the maximum-radius stock hull
            // intersects it and cannot certify this strictly positive gap.
            let groove = translate(
                &extrude_annulus(5.9, 5.5, 0.5, t).unwrap().model,
                [0., 0., 94.9],
                t,
            )
            .unwrap()
            .model;
            let groove = transform(&groove, offset, pose, t).unwrap().model;
            let q = m.clearance(&groove, t).unwrap();
            assert!(q.lower_bound_mm <= 0.1 && q.upper_bound_mm >= 0.1);
        }
        let mut changed = original.clone();
        if let Curve::Circle(c) = &mut changed.edges[0].curve {
            c.radius += 1e-8;
        }
        assert!(!changed.validate(t).valid);
        let mut changed = original.clone();
        changed.loops[0].edges[0].reversed = !changed.loops[0].edges[0].reversed;
        assert!(!changed.validate(t).valid);
        let mut huge = original;
        huge.faces.resize(1000, huge.faces[0].clone());
        assert!(recognize(&huge, t).is_err());
        for z in [10., 90., 94.] {
            let tool = translate(
                &extrude_rectangle(4., 4.04, 13.5, t).unwrap().model,
                [4., -2.02, z],
                t,
            )
            .unwrap()
            .model;
            assert!(cut(&a, &tool, t).is_err());
        }
    }
    #[test]
    fn stepped_keyseat_preserves_stock_and_recovers() {
        let t = Tolerance::default();
        let (a, b) = fixture();
        let r = cut(&a, &b, t).unwrap();
        assert!(r.model.validate(t).valid);
        assert!(recognize(&r.model, t).is_ok());
        let mesh = tessellate_analytic(&r.model, t, TessellationPolicy::default()).unwrap();
        let mut edges = BTreeMap::new();
        let key = |p: [f64; 3]| p.map(|x| (x * 1e9).round() as i64);
        for patch in &mesh.patches {
            for i in 0..patch.points.len() {
                let a = key(patch.points[i]);
                let b = key(patch.points[(i + 1) % patch.points.len()]);
                let (e, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let q = edges.entry(e).or_insert((0, 0));
                q.0 += 1;
                q.1 += sign;
            }
        }
        assert!(
            edges.values().all(|v| *v == (2, 0)),
            "{:?}",
            edges
                .iter()
                .filter(|(_, v)| **v != (2, 0))
                .collect::<Vec<_>>()
        );
    }
}
