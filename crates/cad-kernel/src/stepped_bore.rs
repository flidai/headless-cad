//! Coaxial cylindrical and conical machining. The native analytic boundary is
//! the certificate; no recipe history or hidden construction metadata is used.
use crate::math::{finite, norm};
use crate::*;
use std::collections::BTreeMap;

pub const MAX_STEPPED_BORE_SEGMENTS: usize = 128;
#[derive(Clone)]
struct Segment {
    disk: ModelSnapshot,
    cylinder: CylinderGeometry,
    upper_radius: f64,
    wall: FaceId,
    edges: [EdgeId; 2],
}
#[derive(Clone)]
struct Group {
    segments: Vec<Segment>,
}
#[derive(Clone)]
pub(crate) struct Geometry {
    base: ModelSnapshot,
    groups: Vec<Group>,
    surrogate: ModelSnapshot,
    origin: [f64; 3],
    axis: [f64; 3],
    x: [f64; 3],
    height: f64,
    floors: Vec<(FaceId, [f64; 3], f64, f64)>,
}
fn bad() -> KernelError {
    KernelError::Unsupported("stepped bores require separated coaxial circular hole groups, contiguous axial sections, resolvable positive radial steps, and a canonical right prism".into())
}
fn end(c: &CylinderGeometry) -> [f64; 3] {
    std::array::from_fn(|i| c.origin[i] + c.axis[i] * c.height)
}
fn aligned(a: &CylinderGeometry, b: &CylinderGeometry, error: f64) -> bool {
    if norm(cross(a.axis, b.axis)) > 128. * f64::EPSILON {
        return false;
    }
    if a.axis.iter().all(|v| *v == 0. || v.abs() == 1.) {
        (0..3).all(|i| a.axis[i] != 0. || a.origin[i] == b.origin[i])
    } else {
        [b.origin, end(b)]
            .into_iter()
            .all(|p| norm(cross(sub(p, a.origin), a.axis)) <= error)
    }
}
fn same_plane(a: f64, b: f64, axis: [f64; 3], error: f64) -> bool {
    a == b || (!axis.iter().all(|v| *v == 0. || v.abs() == 1.) && (a - b).abs() <= error)
}
// Recover a canonical solid cylinder from a wall's native support and ring
// curves. analytic::recognize validates every coordinate and seam reference.
fn segment(m: &ModelSnapshot, f: usize, t: Tolerance) -> Result<Segment, KernelError> {
    let face = m.faces.get(f).ok_or_else(bad)?;
    let (wall, cone_slope) = match &face.surface {
        Surface::Cylinder(c) => (c.clone(), 0.),
        Surface::Cone(c) => (
            Cylinder {
                origin: c.origin,
                axis: c.axis,
                x_direction: c.x_direction,
                radius: c.radius,
                inward: c.inward,
            },
            c.slope,
        ),
        _ => return Err(bad()),
    };
    if !wall.inward
        || face.loops.len() != 1
        || !finite(wall.origin)
        || !finite(wall.axis)
        || !finite(wall.x_direction)
        || !wall.radius.is_finite()
        || wall.radius <= t.linear
        || !cone_slope.is_finite()
        || (norm(wall.axis) - 1.).abs() > 128. * f64::EPSILON
        || (norm(wall.x_direction) - 1.).abs() > 128. * f64::EPSILON
        || dot(wall.axis, wall.x_direction).abs() > 128. * f64::EPSILON
    {
        return Err(bad());
    }
    let l = m.loops.get(face.loops[0].0).ok_or_else(bad)?;
    if l.edges.len() != 4 {
        return Err(bad());
    }
    let ids = [l.edges[0].edge, l.edges[2].edge, l.edges[1].edge];
    if l.edges
        != vec![
            EdgeUse {
                edge: ids[0],
                reversed: true,
            },
            EdgeUse {
                edge: ids[2],
                reversed: false,
            },
            EdgeUse {
                edge: ids[1],
                reversed: false,
            },
            EdgeUse {
                edge: ids[2],
                reversed: true,
            },
        ]
    {
        return Err(bad());
    }
    let edges = ids
        .map(|id| m.edges.get(id.0).cloned().ok_or_else(bad))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let Curve::Circle(top) = edges[1].curve.clone() else {
        return Err(bad());
    };
    let vertices = [edges[0].vertices[0], edges[1].vertices[0]];
    let mut disk = extrude_circle(wall.radius, norm(sub(top.origin, wall.origin)), t)?.model;
    disk.vertices = vertices
        .into_iter()
        .map(|v| m.vertices.get(v.0).cloned().ok_or_else(bad))
        .collect::<Result<_, _>>()?;
    disk.edges = edges
        .into_iter()
        .map(|mut e| {
            e.vertices = e.vertices.map(|v| {
                VertexId(
                    vertices
                        .iter()
                        .position(|id| *id == v)
                        .unwrap_or(usize::MAX),
                )
            });
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
    let mut surface = wall.clone();
    surface.inward = false;
    disk.faces[2].surface = Surface::Cylinder(surface);
    let upper_radius = top.radius;
    let mut cylinder = if cone_slope == 0. {
        crate::analytic::recognize(&disk, Some(t))?
    } else {
        // Reconstruct the frustum's complete native support from its boundary.
        let height = dot(sub(top.origin, wall.origin), wall.axis);
        let cylinder = CylinderGeometry {
            origin: wall.origin,
            axis: wall.axis,
            x_direction: wall.x_direction,
            radius: wall.radius,
            height,
            inner_radius: 0.,
            inner_wall: None,
            start_cap: FaceId(0),
            end_cap: FaceId(1),
            wall: FaceId(2),
        };
        let error = crate::bored_prism::arithmetic(m, &cylinder, t)?;
        if !height.is_finite()
            || height <= t.linear + error
            || !upper_radius.is_finite()
            || upper_radius <= t.linear + error
            || (upper_radius - wall.radius - cone_slope * height).abs() > error
        {
            return Err(bad());
        }
        let expected = frustum(&cylinder, upper_radius, t)?;
        if disk
            .vertices
            .iter()
            .zip(&expected.vertices)
            .any(|(a, b)| !finite(a.position) || norm(sub(a.position, b.position)) > error)
        {
            return Err(bad());
        }
        for (a, b) in disk.edges.iter().zip(&expected.edges) {
            if a.vertices != b.vertices {
                return Err(bad());
            }
            match (&a.curve, &b.curve) {
                (Curve::Circle(a), Curve::Circle(b))
                    if finite(a.origin)
                        && finite(a.axis)
                        && finite(a.x_direction)
                        && norm(sub(a.origin, b.origin)) <= error
                        && norm(sub(a.axis, b.axis)) <= 128. * f64::EPSILON
                        && norm(sub(a.x_direction, b.x_direction)) <= 128. * f64::EPSILON
                        && a.radius == b.radius => {}
                (Curve::Line(a), Curve::Line(b))
                    if finite(a.origin)
                        && finite(a.direction)
                        && norm(sub(a.origin, b.origin)) <= error
                        && norm(sub(a.direction, b.direction)) <= 128. * f64::EPSILON => {}
                _ => return Err(bad()),
            }
        }
        disk.faces[2].surface = Surface::Cone(Cone {
            origin: wall.origin,
            axis: wall.axis,
            x_direction: wall.x_direction,
            radius: wall.radius,
            slope: cone_slope,
            inward: false,
        });
        cylinder
    };
    cylinder.wall = FaceId(f);
    Ok(Segment {
        disk,
        cylinder,
        upper_radius,
        wall: FaceId(f),
        edges: [ids[0], ids[1]],
    })
}
fn frustum(c: &CylinderGeometry, upper: f64, t: Tolerance) -> Result<ModelSnapshot, KernelError> {
    let mut disk = extrude_circle(c.radius, c.height, t)?.model;
    for (side, radius) in [(0, c.radius), (1, upper)] {
        let origin = if side == 0 { c.origin } else { end(c) };
        disk.vertices[side].position =
            std::array::from_fn(|i| origin[i] + c.x_direction[i] * radius);
        disk.edges[side].curve = Curve::Circle(Circle {
            origin,
            axis: c.axis,
            x_direction: c.x_direction,
            radius,
        });
        disk.faces[side].surface = Surface::Plane(Plane {
            origin,
            normal: c.axis.map(|v| if side == 0 { -v } else { v }),
        });
    }
    disk.edges[2].curve = Curve::Line(Line {
        origin: disk.vertices[0].position,
        direction: c.axis,
    });
    disk.faces[2].surface = Surface::Cylinder(Cylinder {
        origin: c.origin,
        axis: c.axis,
        x_direction: c.x_direction,
        radius: c.radius,
        inward: false,
    });
    if upper == c.radius {
        return Ok(disk);
    }
    if !upper.is_finite() || upper <= t.linear || c.radius <= t.linear || c.height <= t.linear {
        return Err(bad());
    }
    disk.vertices[1].position =
        std::array::from_fn(|i| c.origin[i] + c.axis[i] * c.height + c.x_direction[i] * upper);
    if let Curve::Circle(circle) = &mut disk.edges[1].curve {
        circle.radius = upper;
    }
    if let Curve::Line(line) = &mut disk.edges[2].curve {
        let direction = sub(disk.vertices[1].position, disk.vertices[0].position);
        let length = norm(direction);
        line.direction = direction.map(|v| v / length);
    }
    disk.faces[2].surface = Surface::Cone(Cone {
        origin: c.origin,
        axis: c.axis,
        x_direction: c.x_direction,
        radius: c.radius,
        slope: (upper - c.radius) / c.height,
        inward: false,
    });
    Ok(disk)
}
// Canonical serialization: three loops per axial segment; continuous sections
// share a ring edge and vertex.
// wall faces first within each group, then its endpoint/annular plane faces.
fn build(
    base: &ModelSnapshot,
    groups: &[Group],
    height: f64,
    origin: [f64; 3],
    axis: [f64; 3],
) -> Result<ModelSnapshot, KernelError> {
    let mut m = base.clone();
    for group in groups {
        let mut rings = Vec::new();
        let mut previous_top: Option<(VertexId, EdgeId)> = None;
        for (index, s) in group.segments.iter().enumerate() {
            let (l, f) = (m.loops.len(), m.faces.len());
            let continuous =
                index > 0 && group.segments[index - 1].upper_radius == s.cylinder.radius;
            let bottom = if continuous {
                previous_top.unwrap().0
            } else {
                let v = VertexId(m.vertices.len());
                m.vertices.push(s.disk.vertices[0].clone());
                v
            };
            let top = VertexId(m.vertices.len());
            m.vertices.push(s.disk.vertices[1].clone());
            let low_edge = if continuous {
                previous_top.unwrap().1
            } else {
                let id = EdgeId(m.edges.len());
                let mut e = s.disk.edges[0].clone();
                e.vertices = [bottom, bottom];
                m.edges.push(e);
                id
            };
            let top_edge = EdgeId(m.edges.len());
            let mut e = s.disk.edges[1].clone();
            e.vertices = [top, top];
            m.edges.push(e);
            let seam = EdgeId(m.edges.len());
            let mut e = s.disk.edges[2].clone();
            e.vertices = [bottom, top];
            m.edges.push(e);
            previous_top = Some((top, top_edge));
            let eu = |edge, reversed| EdgeUse { edge, reversed };
            m.loops.extend([
                Loop {
                    edges: vec![eu(low_edge, false)],
                },
                Loop {
                    edges: vec![eu(top_edge, true)],
                },
                Loop {
                    edges: vec![
                        eu(low_edge, true),
                        eu(seam, false),
                        eu(top_edge, false),
                        eu(seam, true),
                    ],
                },
            ]);
            let mut wall = s.disk.faces[2].clone();
            match &mut wall.surface {
                Surface::Cylinder(c) => c.inward = true,
                Surface::Cone(c) => c.inward = true,
                _ => return Err(bad()),
            }
            wall.loops = vec![LoopId(l + 2)];
            m.faces.push(wall);
            m.shells[0].faces.push(FaceId(f));
            rings.push([LoopId(l), LoopId(l + 1)]);
        }
        let first = &group.segments[0].cylinder;
        let last = &group.segments.last().ok_or_else(bad)?.cylinder;
        let low = dot(sub(first.origin, origin), axis);
        let high = dot(sub(end(last), origin), axis);
        // At canonical creation opening coordinates are exact stock endpoints;
        // replay after placement uses their arithmetic-consistent support planes.
        let scale = origin
            .into_iter()
            .chain(first.origin)
            .chain(end(last))
            .map(f64::abs)
            .fold(height, f64::max);
        let error = scale * 4096. * f64::EPSILON;
        let mut plane = |surface: Surface, loops: Vec<LoopId>| {
            let f = m.faces.len();
            m.faces.push(Face { surface, loops });
            m.shells[0].faces.push(FaceId(f));
        };
        if !same_plane(low, 0., axis, error) {
            let mut face = group.segments[0].disk.faces[0].surface.clone();
            if let Surface::Plane(p) = &mut face {
                p.normal = p.normal.map(|v| -v);
            }
            plane(face, vec![rings[0][0]]);
        }
        for i in 1..group.segments.len() {
            let upper = &group.segments[i].cylinder;
            let lower_radius = group.segments[i - 1].upper_radius;
            if upper.radius == lower_radius {
                continue;
            }
            if upper.radius > lower_radius {
                plane(
                    Surface::Plane(Plane {
                        origin: upper.origin,
                        normal: axis,
                    }),
                    vec![rings[i][0], rings[i - 1][1]],
                );
            } else {
                // Use the actual lower upper-circle support as the plane origin.
                let Surface::Plane(p) = &group.segments[i - 1].disk.faces[1].surface else {
                    return Err(bad());
                };
                plane(
                    Surface::Plane(Plane {
                        origin: p.origin,
                        normal: axis.map(|v| -v),
                    }),
                    vec![rings[i - 1][1], rings[i][0]],
                );
            }
        }
        if !same_plane(high, height, axis, error) {
            let mut face = group.segments.last().unwrap().disk.faces[1].surface.clone();
            if let Surface::Plane(p) = &mut face {
                p.normal = p.normal.map(|v| -v);
            }
            plane(face, vec![rings.last().unwrap()[1]]);
        }
        if same_plane(low, 0., axis, error) {
            m.faces[0].loops.push(rings[0][0]);
        }
        if same_plane(high, height, axis, error) {
            m.faces[1].loops.push(rings.last().unwrap()[1]);
        }
    }
    Ok(m)
}
fn extract(m: &ModelSnapshot, t: Tolerance, replay: bool) -> Result<Geometry, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if m.edges
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
            .any(|s| s.shells.iter().any(|id| id.0 >= m.shells.len()))
    {
        return Err(bad());
    }
    if m.shells.len() != 1 || m.solids.len() != 1 {
        return Err(bad());
    }
    let first = (2..m.faces.len())
        .find(|i| segment(m, *i, t).is_ok())
        .ok_or_else(bad)?;
    let mut segments = Vec::new();
    for f in first..m.faces.len() {
        match m.faces[f].surface {
            Surface::Cylinder(_) | Surface::Cone(_) => segments.push(segment(m, f, t)?),
            Surface::Plane(_) => {}
        }
    }
    if segments.is_empty() {
        return Err(bad());
    }
    if segments.len() > MAX_STEPPED_BORE_SEGMENTS {
        return Err(KernelError::ResourceLimit(
            "at most 128 total stepped-bore axial sections".into(),
        ));
    }
    let ne = segments
        .iter()
        .flat_map(|s| s.edges)
        .map(|e| e.0)
        .min()
        .ok_or_else(bad)?;
    let nv = m.edges.get(ne).ok_or_else(bad)?.vertices[0].0;
    let nl = m.faces[first].loops[0].0.checked_sub(2).ok_or_else(bad)?;
    if nv > m.vertices.len() || ne > m.edges.len() || nl > m.loops.len() {
        return Err(bad());
    }
    let mut base = m.clone();
    base.vertices.truncate(nv);
    base.edges.truncate(ne);
    base.loops.truncate(nl);
    base.faces.truncate(first);
    base.shells[0].faces.truncate(first);
    for f in &mut base.faces[..2] {
        f.loops.retain(|l| l.0 < nl);
    }
    let stock = crate::through_bore::stock(&base, t)?;
    if !base.validate(t).valid {
        return Err(bad());
    }
    let mut groups: Vec<Group> = vec![];
    for s in segments {
        let error = crate::bored_prism::arithmetic(m, &s.cylinder, t)?;
        if norm(sub(s.cylinder.axis, stock.axis)) > 128. * f64::EPSILON {
            return Err(bad());
        }
        if let Some(g) = groups
            .iter_mut()
            .find(|g| aligned(&g.segments[0].cylinder, &s.cylinder, error))
        {
            g.segments.push(s);
        } else {
            groups.push(Group { segments: vec![s] });
        }
    }
    if groups.len() > MAX_POLYGON_HOLES {
        return Err(KernelError::ResourceLimit(
            "at most 32 separated bore groups".into(),
        ));
    }
    let mut surrogate = base.clone();
    for group in &mut groups {
        group.segments.sort_by(|a, b| {
            dot(sub(a.cylinder.origin, stock.origin), stock.axis)
                .total_cmp(&dot(sub(b.cylinder.origin, stock.origin), stock.axis))
        });
        for (i, s) in group.segments.iter().enumerate() {
            let error = crate::bored_prism::arithmetic(m, &s.cylinder, t)?;
            let lo = dot(sub(s.cylinder.origin, stock.origin), stock.axis);
            let hi = lo + s.cylinder.height;
            if !(same_plane(lo, 0., stock.axis, error) || lo > t.linear + error)
                || !(same_plane(hi, stock.height, stock.axis, error)
                    || hi < stock.height - t.linear - error)
            {
                return Err(bad());
            }
            for (side, center, z) in [(0, s.cylinder.origin, lo), (1, end(&s.cylinder), hi)] {
                if same_plane(
                    z,
                    if side == 0 { 0. } else { stock.height },
                    stock.axis,
                    error,
                ) {
                    let p = base.faces[side].try_plane().ok_or_else(bad)?;
                    if dot(sub(center, p.origin), p.normal).abs()
                        + norm(cross(s.cylinder.axis, p.normal)) * s.cylinder.radius
                        > error
                    {
                        return Err(KernelError::NumericRange);
                    }
                }
            }
            if i > 0 {
                let before = &group.segments[i - 1].cylinder;
                if !same_plane(
                    dot(sub(end(before), stock.origin), stock.axis),
                    lo,
                    stock.axis,
                    error,
                ) || ((group.segments[i - 1].upper_radius - s.cylinder.radius).abs()
                    <= t.linear + error
                    && group.segments[i - 1].upper_radius != s.cylinder.radius)
                    || (group.segments[i - 1].upper_radius == s.cylinder.radius
                        && group.segments[i - 1].upper_radius == before.radius
                        && s.upper_radius == s.cylinder.radius)
                {
                    return Err(bad());
                }
                // The complete adjacent rings must belong to one shoulder plane.
                if dot(sub(s.cylinder.origin, end(before)), stock.axis).abs()
                    + norm(cross(s.cylinder.axis, stock.axis)) * s.cylinder.radius
                    > error
                {
                    return Err(KernelError::NumericRange);
                }
            }
        }
        let mut envelope = group.segments[0].cylinder.clone();
        envelope.radius = group
            .segments
            .iter()
            .map(|s| s.cylinder.radius.max(s.upper_radius))
            .fold(0., f64::max);
        let tool = crate::bored_prism::disk_at(&envelope, &stock, 0., stock.height, t)?;
        surrogate = crate::through_bore::cut(&surrogate, &tool, t)?.model;
    }
    if replay && build(&base, &groups, stock.height, stock.origin, stock.axis)? != *m {
        return Err(bad());
    }
    let floors = m
        .faces
        .iter()
        .enumerate()
        .skip(first)
        .filter_map(|(f, face)| {
            let Surface::Plane(p) = &face.surface else {
                return None;
            };
            let radius = |l: LoopId| -> Option<f64> {
                let e = m.loops.get(l.0)?.edges.first()?.edge;
                match &m.edges.get(e.0)?.curve {
                    Curve::Circle(c) => Some(c.radius),
                    _ => None,
                }
            };
            Some((
                FaceId(f),
                p.origin,
                radius(*face.loops.first()?)?,
                if face.loops.len() > 1 {
                    radius(face.loops[1])?
                } else {
                    0.
                },
            ))
        })
        .collect();
    Ok(Geometry {
        base,
        groups,
        surrogate,
        origin: stock.origin,
        axis: stock.axis,
        x: stock.x,
        height: stock.height,
        floors,
    })
}
pub(crate) fn recognize(m: &ModelSnapshot, t: Tolerance) -> Result<Geometry, KernelError> {
    let has_cone = m
        .faces
        .iter()
        .any(|f| matches!(f.surface, Surface::Cone(_)));
    if !has_cone
        && !m
            .faces
            .iter()
            .skip(2)
            .any(|f| matches!(f.surface, Surface::Plane(_)) && f.loops.len() == 2)
    {
        return Err(bad());
    }
    let g = extract(m, t, true)?;
    if !has_cone && !g.groups.iter().any(|g| g.segments.len() > 1) {
        return Err(bad());
    }
    Ok(g)
}
fn cutter(m: &ModelSnapshot, t: Tolerance) -> Result<(CylinderGeometry, f64), KernelError> {
    if let Ok(c) = crate::analytic::recognize(m, Some(t)) {
        let r = c.radius;
        return Ok((c, r));
    }
    let g = crate::revolve::recognize(m, t)?;
    if g.profile.len() != 4 || g.profile.iter().filter(|p| p[0] == 0.).count() != 2 {
        return Err(bad());
    }
    let walls: Vec<_> = g
        .segments
        .iter()
        .filter(|s| matches!(m.faces[s.face.0].surface, Surface::Cone(_)))
        .collect();
    if walls.len() != 1 {
        return Err(bad());
    }
    let wall = walls[0];
    let (low, high) = if wall.start[1] < wall.end[1] {
        (wall.start, wall.end)
    } else {
        (wall.end, wall.start)
    };
    let Surface::Cone(cone) = &m.faces[wall.face.0].surface else {
        return Err(bad());
    };
    if cone.inward || low[0] <= t.linear || high[0] <= t.linear {
        return Err(bad());
    }
    let cap = |z: f64| {
        g.segments
            .iter()
            .find(|s| s.start[1] == z && s.end[1] == z)
            .map(|s| s.face)
            .ok_or_else(bad)
    };
    Ok((
        CylinderGeometry {
            origin: std::array::from_fn(|i| g.origin[i] + g.axis[i] * low[1]),
            axis: g.axis,
            x_direction: g.x_direction,
            radius: low[0],
            height: high[1] - low[1],
            inner_radius: 0.,
            inner_wall: None,
            start_cap: cap(low[1])?,
            end_cap: cap(high[1])?,
            wall: wall.face,
        },
        high[0],
    ))
}
fn segment_radius(s: &Segment, z: f64, origin: [f64; 3], axis: [f64; 3]) -> f64 {
    let lo = dot(sub(s.cylinder.origin, origin), axis);
    if z == lo {
        s.cylinder.radius
    } else if z == lo + s.cylinder.height {
        s.upper_radius
    } else {
        s.cylinder.radius + (s.upper_radius - s.cylinder.radius) * ((z - lo) / s.cylinder.height)
    }
}
pub(crate) fn cut(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let old = extract(a, t, false).or_else(|_| {
        let s = crate::through_bore::stock(a, t)?;
        Ok::<_, KernelError>(Geometry {
            base: a.clone(),
            groups: vec![],
            surrogate: a.clone(),
            origin: s.origin,
            axis: s.axis,
            x: s.x,
            height: s.height,
            floors: vec![],
        })
    })?;
    let stock = crate::through_bore::stock(&old.base, t)?;
    let (tool, tool_upper) = cutter(b, t)?;
    let error = crate::bored_prism::arithmetic(a, &tool, t)?;
    let (lo, hi, _) = crate::bored_prism::interval(&stock, &tool, error, t)?;
    let tool_z = dot(sub(tool.origin, stock.origin), stock.axis);
    let tool_slope = (tool_upper - tool.radius) / tool.height * dot(tool.axis, stock.axis).signum();
    let tool_radius = |z: f64| tool.radius + tool_slope * (z - tool_z);
    let mut groups = old.groups.clone();
    let index = groups
        .iter()
        .position(|g| aligned(&g.segments[0].cylinder, &tool, error));
    let index = if let Some(i) = index {
        i
    } else {
        groups.push(Group { segments: vec![] });
        groups.len() - 1
    };
    let group = &groups[index];
    let mut levels = vec![lo, hi];
    for s in &group.segments {
        levels.push(dot(sub(s.cylinder.origin, stock.origin), stock.axis));
        levels.push(dot(sub(end(&s.cylinder), stock.origin), stock.axis));
    }
    let mut crossings = vec![];
    for segment in &group.segments {
        let low = dot(sub(segment.cylinder.origin, stock.origin), stock.axis).max(lo);
        let high = (dot(sub(segment.cylinder.origin, stock.origin), stock.axis)
            + segment.cylinder.height)
            .min(hi);
        if low < high {
            let a = tool_radius(low) - segment_radius(segment, low, stock.origin, stock.axis);
            let b = tool_radius(high) - segment_radius(segment, high, stock.origin, stock.axis);
            if (a < 0. && b > 0.) || (a > 0. && b < 0.) {
                let z = low + (high - low) * (a / (a - b));
                if z > low && z < high {
                    levels.push(z);
                    crossings.push(z);
                }
            }
        }
    }
    levels.sort_by(f64::total_cmp);
    let mut clean: Vec<f64> = vec![];
    for z in levels {
        if let Some(p) = clean.last() {
            if same_plane(*p, z, stock.axis, error) {
                continue;
            }
            if z - p <= t.linear + error {
                return Err(bad());
            }
        }
        clean.push(z);
    }
    let mut sections: Vec<(f64, f64, f64, f64)> = vec![];
    let mut changed = false;
    for pair in clean.windows(2) {
        let mid = pair[0] / 2. + pair[1] / 2.;
        let previous = group.segments.iter().find(|s| {
            let z = dot(sub(s.cylinder.origin, stock.origin), stock.axis);
            mid > z && mid < z + s.cylinder.height
        });
        let old_radius = previous.map_or(0., |s| segment_radius(s, mid, stock.origin, stock.axis));
        let active = mid > lo && mid < hi && tool_radius(mid) > old_radius;
        if !active && previous.is_none() {
            return Err(bad());
        }
        changed |= active;
        let radius = |z| {
            if active {
                tool_radius(z)
            } else {
                segment_radius(previous.unwrap(), z, stock.origin, stock.axis)
            }
        };
        let (mut r0, r1) = (radius(pair[0]), radius(pair[1]));
        if let Some(last) = sections.last_mut() {
            // A crossing endpoint is shared only within a certified arithmetic
            // envelope, never because two radial supports lie within tolerance.
            if crossings.contains(&pair[0]) && (r0 - last.3).abs() <= error {
                r0 = last.3;
            }
            if r0 == r1 && last.2 == last.3 && r0 == last.3 {
                last.1 = pair[1];
                continue;
            }
            if r0 != last.3 && (r0 - last.3).abs() <= t.linear + error {
                return Err(bad());
            }
        }
        if r0 <= t.linear + error
            || r1 <= t.linear + error
            || (r0 != r1 && (r1 - r0).abs() <= t.linear + error)
        {
            return Err(bad());
        }
        if active
            && old_radius > 0.
            && tool_slope == 0.
            && tool_radius(mid) - old_radius <= t.linear + error
        {
            return Err(bad());
        }
        sections.push((pair[0], pair[1], r0, r1));
    }
    if !changed {
        return Err(bad());
    }
    groups[index].segments = sections
        .into_iter()
        .map(|(low, high, radius, upper_radius)| {
            let mut c = tool.clone();
            c.radius = radius;
            let disk = crate::bored_prism::disk_at(&c, &stock, low, high, t)?;
            let cylinder = crate::analytic::recognize(&disk, Some(t))?;
            if [cylinder.origin, end(&cylinder)]
                .into_iter()
                .any(|p| norm(cross(sub(p, tool.origin), tool.axis)) > error)
            {
                return Err(KernelError::NumericRange);
            }
            let disk = if upper_radius == radius {
                disk
            } else {
                frustum(&cylinder, upper_radius, t)?
            };
            Ok(Segment {
                disk,
                cylinder,
                upper_radius,
                wall: FaceId(usize::MAX),
                edges: [EdgeId(usize::MAX); 2],
            })
        })
        .collect::<Result<_, KernelError>>()?;
    let model = build(&old.base, &groups, stock.height, stock.origin, stock.axis)?;
    let result = extract(&model, t, true)?;
    if !model.validate(t).valid {
        return Err(bad());
    }
    // Retain the actual terminating cutter cap plane over the full circle.
    for (z, cap) in [
        (
            lo,
            if dot(tool.axis, stock.axis) > 0. {
                tool.start_cap
            } else {
                tool.end_cap
            },
        ),
        (
            hi,
            if dot(tool.axis, stock.axis) > 0. {
                tool.end_cap
            } else {
                tool.start_cap
            },
        ),
    ] {
        if z > 0. && z < stock.height {
            let plane = b.faces[cap.0].try_plane().ok_or_else(bad)?;
            let center = std::array::from_fn(|i| {
                tool.origin[i]
                    + stock.axis[i] * (z - dot(sub(tool.origin, stock.origin), stock.axis))
            });
            if dot(sub(center, plane.origin), plane.normal).abs()
                + norm(cross(stock.axis, plane.normal)) * tool_radius(z)
                > error
            {
                return Err(KernelError::NumericRange);
            }
        }
    }
    result.properties(t)?;
    let mut left: BTreeMap<_, _> = (0..a.faces.len()).map(|f| (FaceId(f), vec![])).collect();
    for f in 0..old.base.faces.len() {
        left.insert(FaceId(f), vec![FaceId(f)]);
    }
    for g in &old.groups {
        for s in &g.segments {
            let hits = result
                .groups
                .iter()
                .flat_map(|g| &g.segments)
                .filter(|n| {
                    if !aligned(&s.cylinder, &n.cylinder, error)
                        || (segment_radius(
                            s,
                            dot(sub(n.cylinder.origin, stock.origin), stock.axis),
                            stock.origin,
                            stock.axis,
                        ) - n.cylinder.radius)
                            .abs()
                            > error
                        || ((s.upper_radius - s.cylinder.radius) / s.cylinder.height
                            - (n.upper_radius - n.cylinder.radius) / n.cylinder.height)
                            .abs()
                            > 128. * f64::EPSILON
                    {
                        return false;
                    }
                    let z = dot(sub(n.cylinder.origin, s.cylinder.origin), s.cylinder.axis);
                    z.max(0.) < (z + n.cylinder.height).min(s.cylinder.height) - error
                })
                .map(|n| n.wall)
                .collect();
            left.insert(s.wall, hits);
        }
    }
    for (id, center, outer, inner) in &old.floors {
        let normal = a.faces[id.0].try_plane().ok_or_else(bad)?.normal;
        let hits = result
            .floors
            .iter()
            .filter(|(f, p, r, i)| {
                let n = model.faces[f.0].try_plane().unwrap().normal;
                norm(sub(normal, n)) <= 128. * f64::EPSILON
                    && norm(sub(*center, *p)) <= error
                    && outer.min(*r) > inner.max(*i) + t.linear + error
            })
            .map(|(f, _, _, _)| *f)
            .collect();
        left.insert(*id, hits);
    }
    let walls: Vec<_> = result
        .groups
        .iter()
        .flat_map(|g| &g.segments)
        .filter(|s| {
            let z = dot(sub(s.cylinder.origin, stock.origin), stock.axis);
            if (s.cylinder.radius - tool_radius(z)).abs() > error
                || (s.upper_radius - tool_radius(z + s.cylinder.height)).abs() > error
                || !aligned(&tool, &s.cylinder, error)
            {
                return false;
            }
            let low = dot(sub(s.cylinder.origin, stock.origin), stock.axis);
            low.max(lo) < (low + s.cylinder.height).min(hi) - error
        })
        .map(|s| s.wall)
        .collect();
    let mut right: BTreeMap<_, _> = (0..b.faces.len()).map(|f| (FaceId(f), vec![])).collect();
    right.insert(tool.wall, walls.clone());
    let mut floors = vec![];
    let mut shoulders = vec![];
    for (f, p, outer, inner) in &result.floors {
        for cap in [tool.start_cap, tool.end_cap] {
            let plane = b.faces[cap.0].try_plane().unwrap();
            let normal = model.faces[f.0].try_plane().unwrap().normal;
            if dot(sub(*p, plane.origin), plane.normal).abs() <= error
                && dot(normal, plane.normal) < -1. + 128. * f64::EPSILON
                && norm(cross(sub(*p, tool.origin), tool.axis)) <= error
                && *outer <= tool_radius(dot(sub(*p, stock.origin), stock.axis))
                && *inner < tool_radius(dot(sub(*p, stock.origin), stock.axis)) - t.linear - error
            {
                right.get_mut(&cap).unwrap().push(*f);
                floors.push(*f);
                if *inner > 0. {
                    shoulders.push(*f);
                }
            }
        }
    }
    let mut generated = BTreeMap::from([
        (
            "boundary".into(),
            (0..model.faces.len()).map(FaceId).collect(),
        ),
        ("bore_wall".into(), walls),
    ]);
    if !floors.is_empty() {
        generated.insert("bore_floor".into(), floors);
    }
    if !shoulders.is_empty() {
        generated.insert("bore_shoulder".into(), shoulders);
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: generated,
            input_faces: BTreeMap::from([("left".into(), left), ("right".into(), right)]),
        },
    })
}
impl Geometry {
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        let base = self.base.mass_properties(t)?;
        let reference = base.centroid.ok_or_else(bad)?;
        let mut volume = base.volume;
        let mut first = [0.; 3];
        let mut inertia = base.central_inertia.ok_or_else(bad)?;
        let mut absolute = inertia.map(|r| r.map(f64::abs));
        for segment in self.groups.iter().flat_map(|g| &g.segments) {
            let c = &segment.cylinder;
            let p = if c.radius == segment.upper_radius {
                c.properties()?
            } else {
                RevolveGeometry {
                    origin: c.origin,
                    axis: c.axis,
                    x_direction: c.x_direction,
                    profile: vec![
                        [0., 0.],
                        [c.radius, 0.],
                        [segment.upper_radius, c.height],
                        [0., c.height],
                    ],
                    segments: vec![],
                }
                .properties()?
            };
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
        if let Some((_, p, _, _)) = self.floors.iter().find(|(f, _, _, _)| *f == id) {
            return Ok(*p);
        }
        if id.0 > 1 {
            return Ok(self.base.face_frame(id, self.x, [0.; 3], t)?.origin);
        }
        let mut bores = vec![];
        for g in &self.groups {
            let s = if id.0 == 0 {
                &g.segments[0]
            } else {
                g.segments.last().unwrap()
            };
            let z = dot(
                sub(
                    if id.0 == 0 {
                        s.cylinder.origin
                    } else {
                        end(&s.cylinder)
                    },
                    self.origin,
                ),
                self.axis,
            );
            let error = crate::bored_prism::arithmetic(&self.base, &s.cylinder, t)?;
            if same_plane(
                z,
                if id.0 == 0 { 0. } else { self.height },
                self.axis,
                error,
            ) {
                let mut cylinder = s.cylinder.clone();
                if id.0 == 1 {
                    cylinder.radius = s.upper_radius;
                }
                bores.push(PrismBore {
                    cylinder,
                    opening_caps: vec![id],
                    floor: None,
                });
            }
        }
        BoredPrismGeometry {
            base: self.base.clone(),
            bores,
            origin: self.origin,
            axis: self.axis,
            x_direction: self.x,
            height: self.height,
            surrogate: self.surrogate.clone(),
        }
        .cap_centroid(id, t)
    }
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(m.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => l.origin.to_vec(),
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
        linear: scale * 262144. * f64::EPSILON * (1. + 1e-12),
    };
    recognize(m, t)?.properties(t).map(|p| p.volume)
}
pub(crate) fn tessellate(
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let support_errors: BTreeMap<FaceId, f64> = g
        .groups
        .iter()
        .flat_map(|group| &group.segments)
        .map(|s| {
            Ok((
                s.wall,
                crate::bored_prism::arithmetic(&g.surrogate, &s.cylinder, t)?,
            ))
        })
        .collect::<Result<_, KernelError>>()?;
    // Reserve coordinate arithmetic when deriving smaller rings from the shared
    // maximum-radius samples, in addition to their radial chord deviation.
    let roundoff = support_errors.values().copied().fold(0., f64::max) * 2.;
    let sampling_policy = TessellationPolicy {
        chordal_error: policy.chordal_error - roundoff,
        ..policy
    };
    let mut mesh = tessellate_analytic(&g.surrogate, t, sampling_policy)?;
    mesh.chordal_error_bound += roundoff;
    let mut extra = vec![];
    for (gi, group) in g.groups.iter().enumerate() {
        let surrogate_wall = FaceId(g.base.faces.len() + gi);
        let surrogate_edges = [
            EdgeId(g.base.edges.len() + gi * 3),
            EdgeId(g.base.edges.len() + gi * 3 + 1),
        ];
        let radius = group
            .segments
            .iter()
            .map(|s| s.cylinder.radius.max(s.upper_radius))
            .fold(0., f64::max);
        let first = &group.segments[0];
        let last = group.segments.last().unwrap();
        let c = &first.cylinder;
        let low = dot(sub(c.origin, g.origin), g.axis);
        let origin: [f64; 3] = std::array::from_fn(|i| c.origin[i] - low * g.axis[i]);
        let wall_patches: Vec<_> = mesh
            .patches
            .iter()
            .filter(|p| p.face == surrogate_wall)
            .cloned()
            .collect();
        for patch in &wall_patches {
            let ring = |s: &Segment, side: usize| -> [[f64; 3]; 2] {
                // Use the shared native circle, not independently reconstructed
                // endpoint arithmetic, so adjacent walls have identical samples.
                let Curve::Circle(circle) = &s.disk.edges[side].curve else {
                    unreachable!()
                };
                let center = circle.origin;
                let z = dot(sub(center, g.origin), g.axis);
                let ring_radius = if side == 0 {
                    s.cylinder.radius
                } else {
                    s.upper_radius
                };
                [0, 1].map(|k| {
                    if ring_radius == radius && same_plane(z, 0., g.axis, support_errors[&s.wall]) {
                        return patch.points[k];
                    }
                    if ring_radius == radius
                        && same_plane(z, g.height, g.axis, support_errors[&s.wall])
                    {
                        return patch.points[3 - k];
                    }
                    std::array::from_fn(|i| {
                        center[i] + (patch.points[k][i] - origin[i]) * ring_radius / radius
                    })
                })
            };
            let mut add = |face: FaceId,
                           mut points: Vec<[f64; 3]>,
                           normal: [f64; 3],
                           mut edges: Vec<Option<EdgeId>>| {
                if dot(
                    cross(sub(points[1], points[0]), sub(points[2], points[0])),
                    normal,
                ) < 0.
                {
                    points.reverse();
                    edges.reverse();
                    edges.rotate_left(1);
                }
                extra.push(MeshPatch {
                    face,
                    points,
                    normal,
                    boundary_edges: edges,
                });
            };
            for s in &group.segments {
                let a = ring(s, 0);
                let b = ring(s, 1);
                add(
                    s.wall,
                    vec![a[0], a[1], b[1], b[0]],
                    {
                        let mut n = cross(sub(a[1], a[0]), sub(b[1], a[0]));
                        if dot(n, patch.normal) < 0. {
                            n = n.map(|v| -v);
                        }
                        let length = norm(n);
                        n.map(|v| v / length)
                    },
                    vec![Some(s.edges[0]), None, Some(s.edges[1]), None],
                );
            }
            // Actual planar boundaries (disk floors and annular shoulders).
            for (id, center, outer, inner) in &g.floors {
                if norm(cross(sub(*center, c.origin), g.axis)) > t.linear / 16. {
                    continue;
                }
                let z = dot(sub(*center, g.origin), g.axis);
                let find = |r: f64| -> Result<([[f64; 3]; 2], EdgeId), KernelError> {
                    for s in &group.segments {
                        for side in 0..2 {
                            let p = if side == 0 {
                                s.cylinder.origin
                            } else {
                                end(&s.cylinder)
                            };
                            let error = crate::bored_prism::arithmetic(&g.base, &s.cylinder, t)?;
                            if r == (if side == 0 {
                                s.cylinder.radius
                            } else {
                                s.upper_radius
                            }) && same_plane(dot(sub(p, g.origin), g.axis), z, g.axis, error)
                            {
                                return Ok((ring(s, side), s.edges[side]));
                            }
                        }
                    }
                    Err(bad())
                };
                let (a, ae) = find(*outer)?;
                let normal = if (0..group.segments.len()).any(|i| {
                    let s = &group.segments[i];
                    s.cylinder.radius == *outer
                        && norm(sub(s.cylinder.origin, *center)) < t.linear / 16.
                }) {
                    g.axis
                } else {
                    g.axis.map(|v| -v)
                };
                if *inner == 0. {
                    add(
                        *id,
                        vec![*center, a[0], a[1]],
                        normal,
                        vec![None, Some(ae), None],
                    );
                } else {
                    let (b, be) = find(*inner)?;
                    add(
                        *id,
                        vec![a[0], a[1], b[1], b[0]],
                        normal,
                        vec![Some(ae), None, Some(be), None],
                    );
                }
            }
            // Fill surrogate cap annuli down to the actual endpoint opening.
            for (side, s) in [(0, first), (1, last)] {
                // Use the shared native circle, not independently reconstructed
                // endpoint arithmetic, so adjacent walls have identical samples.
                let Curve::Circle(circle) = &s.disk.edges[side].curve else {
                    unreachable!()
                };
                let center = circle.origin;
                let z = dot(sub(center, g.origin), g.axis);
                let error = crate::bored_prism::arithmetic(&g.base, &s.cylinder, t)?;
                let open = same_plane(z, if side == 0 { 0. } else { g.height }, g.axis, error);
                let a = if side == 0 {
                    [patch.points[0], patch.points[1]]
                } else {
                    [patch.points[3], patch.points[2]]
                };
                let normal = g.axis.map(|v| if side == 0 { -v } else { v });
                if open {
                    if (if side == 0 {
                        s.cylinder.radius
                    } else {
                        s.upper_radius
                    }) != radius
                    {
                        let b = ring(s, side);
                        add(
                            FaceId(side),
                            vec![a[0], a[1], b[1], b[0]],
                            normal,
                            vec![None, None, Some(s.edges[side]), None],
                        );
                    }
                } else {
                    let p = std::array::from_fn(|i| {
                        origin[i] + if side == 0 { 0. } else { g.height * g.axis[i] }
                    });
                    add(FaceId(side), vec![p, a[0], a[1]], normal, vec![None; 3]);
                }
            }
        }
        for patch in &mut mesh.patches {
            if patch.face.0 < 2 {
                let side = patch.face.0;
                let s = if side == 0 { first } else { last };
                let z = dot(
                    sub(
                        if side == 0 {
                            s.cylinder.origin
                        } else {
                            end(&s.cylinder)
                        },
                        g.origin,
                    ),
                    g.axis,
                );
                let error = crate::bored_prism::arithmetic(&g.base, &s.cylinder, t)?;
                let endpoint_radius = if side == 0 {
                    s.cylinder.radius
                } else {
                    s.upper_radius
                };
                let same = endpoint_radius == radius
                    && same_plane(z, if side == 0 { 0. } else { g.height }, g.axis, error);
                for e in &mut patch.boundary_edges {
                    if *e == Some(surrogate_edges[side]) {
                        *e = if same { Some(s.edges[side]) } else { None };
                    }
                }
            }
        }
    }
    mesh.patches.retain(|p| p.face.0 < g.base.faces.len());
    mesh.patches.extend(extra);
    Ok(mesh)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tool(r: f64, z: f64, h: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        translate(&extrude_circle(r, h, t).unwrap().model, [20., 13., z], t)
            .unwrap()
            .model
    }
    fn fixture() -> OperationResult {
        let t = Tolerance::default();
        let m = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let m = boolean(&m, &tool(3., -2., 16.), BooleanOp::Difference, t)
            .unwrap()
            .model;
        let m = boolean(&m, &tool(7., 8., 6.), BooleanOp::Difference, t)
            .unwrap()
            .model;
        boolean(&m, &tool(6., -2., 6.), BooleanOp::Difference, t).unwrap()
    }
    fn closed(m: &ModelSnapshot) {
        let mesh =
            tessellate_analytic(m, Tolerance::default(), TessellationPolicy::default()).unwrap();
        let mut edges = BTreeMap::<([u64; 3], [u64; 3]), (usize, i32)>::new();
        let key = |p: [f64; 3]| p.map(|v| if v == 0. { 0 } else { v.to_bits() });
        for p in mesh.patches {
            for i in 1..p.points.len() - 1 {
                let tri = [p.points[0], p.points[i], p.points[i + 1]];
                assert!(
                    dot(cross(sub(tri[1], tri[0]), sub(tri[2], tri[0])), p.normal) > 0.,
                    "degenerate or reversed mesh face {:?}",
                    p.face
                );
                for j in 0..3 {
                    let a = key(tri[j]);
                    let b = key(tri[(j + 1) % 3]);
                    let (e, s) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                    let v = edges.entry(e).or_default();
                    v.0 += 1;
                    v.1 += s;
                }
            }
        }
        let bad: Vec<_> = edges.iter().filter(|(_, v)| **v != (2, 0)).collect();
        assert!(
            bad.is_empty(),
            "{} unmatched edges, first {:?}",
            bad.len(),
            bad.iter().take(3).collect::<Vec<_>>()
        );
    }
    fn cone(r0: f64, z0: f64, r1: f64, z1: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        translate(
            &revolve_polygon(&[[0., z0], [r0, z0], [r1, z1], [0., z1]], t)
                .unwrap()
                .model,
            [20., 13., 0.],
            t,
        )
        .unwrap()
        .model
    }
    fn difference(a: &ModelSnapshot, b: &ModelSnapshot) -> ModelSnapshot {
        boolean(a, b, BooleanOp::Difference, Tolerance::default())
            .unwrap()
            .model
    }
    #[test]
    fn countersinks_compose_in_both_orders_and_cross_existing_supports() {
        let t = Tolerance::default();
        let base = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let drill = tool(3., -2., 16.);
        let sink = cone(3., 8., 9., 14.);
        let forward = difference(&difference(&base, &drill), &sink);
        let reverse = difference(&difference(&base, &sink), &drill);
        assert_eq!(forward, reverse);
        let g = recognize(&forward, t).unwrap();
        assert_eq!(g.groups[0].segments.len(), 2);
        assert_eq!(
            g.groups[0].segments[0].edges[1],
            g.groups[0].segments[1].edges[0]
        );
        assert!(g.floors.is_empty());
        let expected =
            80. * 40. * 12. - std::f64::consts::PI * (9. * 8. + 4. / 3. * (9. + 21. + 49.));
        assert!((forward.mass_properties(t).unwrap().volume - expected).abs() < 1e-8);
        for m in [
            forward.clone(),
            difference(&forward, &tool(5., -2., 16.)),
            difference(&forward, &cone(9., -2., 3., 4.)),
            difference(&difference(&base, &drill), &cone(1., 6., 9., 14.)),
            difference(&forward, &cone(10., -2., 2., 14.)),
        ] {
            assert!(m.validate(t).valid);
            closed(&m);
            closed(
                &transform(&m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                    .unwrap()
                    .model,
            );
        }
        // Same cutter support, with its axis and meridian reversed.
        let reversed = transform(&cone(9., -14., 3., -8.), [0., 26., 0.], [1., 0., 0., 0.], t)
            .unwrap()
            .model;
        let opposite = difference(&difference(&base, &drill), &reversed);
        assert!((opposite.mass_properties(t).unwrap().volume - expected).abs() < 1e-8);
        closed(&opposite);
    }
    #[test]
    fn malformed_countersink_support_frames_seams_and_shared_edges_fail_closed() {
        let t = Tolerance::default();
        let base = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let m = difference(
            &difference(&base, &tool(3., -2., 16.)),
            &cone(3., 8., 9., 14.),
        );
        let g = recognize(&m, t).unwrap();
        let wall = g.groups[0].segments[1].wall;
        for mutation in 0..12 {
            let mut forged = m.clone();
            let Surface::Cone(c) = &mut forged.faces[wall.0].surface else {
                panic!()
            };
            match mutation {
                0 => c.origin[0] = f64::NAN,
                1 => c.axis[1] = f64::NAN,
                2 => c.x_direction[2] = f64::INFINITY,
                3 => c.slope = f64::NAN,
                4 => c.axis[2] = 2.,
                5 => c.x_direction = c.axis,
                6 => c.slope += 0.01,
                7 => c.radius += 0.01,
                8 => {
                    let e = g.groups[0].segments[1].edges[0];
                    forged.edges[e.0].vertices[1] = VertexId(usize::MAX);
                }
                9 => {
                    let l = forged.faces[wall.0].loops[0];
                    let e = forged.loops[l.0].edges[1].edge;
                    if let Curve::Line(line) = &mut forged.edges[e.0].curve {
                        line.direction = [0., 0., 1.];
                    }
                }
                10 => {
                    let e = g.groups[0].segments[1].edges[0];
                    if let Curve::Circle(c) = &mut forged.edges[e.0].curve {
                        c.radius += 0.01;
                    }
                }
                _ => {
                    let l = forged.faces[wall.0].loops[0];
                    forged.loops[l.0].edges[0].reversed = false;
                }
            }
            assert!(recognize(&forged, t).is_err(), "mutation {mutation}");
            assert!(!forged.validate(t).valid, "mutation {mutation}");
        }
    }
    #[test]
    fn countersink_lineage_and_unsupported_contacts_remain_explicit() {
        let t = Tolerance::default();
        let base = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let pilot = difference(&base, &tool(3., -2., 16.));
        let tool_model = cone(3., 8., 9., 14.);
        let result = cut(&pilot, &tool_model, t).unwrap();
        let g = recognize(&result.model, t).unwrap();
        let cone_wall = g.groups[0].segments[1].wall;
        assert_eq!(result.history.generated_faces["bore_wall"], vec![cone_wall]);
        assert!(!result.history.generated_faces.contains_key("bore_shoulder"));
        assert!(result.history.input_faces["left"]
            .values()
            .any(|ids| ids == &vec![g.groups[0].segments[0].wall]));
        assert!(result.history.input_faces["right"]
            .values()
            .any(|ids| ids == &vec![cone_wall]));
        let offset = translate(&tool_model, [1., 0., 0.], t).unwrap().model;
        let skew = transform(&tool_model, [0.; 3], [0.1, 0., 0., 0.99_f64.sqrt()], t)
            .unwrap()
            .model;
        for invalid in [cone(3., 2., 5., 6.), cone(3., 8., 30., 14.), offset, skew] {
            assert!(cut(&pilot, &invalid, t).is_err());
        }
        assert!(cut(&result.model, &tool_model, t).is_err());
    }
    #[test]
    fn countersink_unresolved_radial_jump_is_not_snapped() {
        let t = Tolerance::default();
        let base = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let pilot = difference(&base, &tool(3., -2., 16.));
        assert!(cut(&pilot, &cone(3. + 1e-13, 8., 9., 14.), t).is_err());
    }
    #[test]
    fn native_stepped_mesh_shared_rings_are_closed() {
        let m = fixture().model;
        closed(&m);
        closed(
            &transform(
                &m,
                [12., -7., 4.],
                [0.5, 0.5, 0., 0.5_f64.sqrt()],
                Tolerance::default(),
            )
            .unwrap()
            .model,
        );
    }
    #[test]
    fn large_world_offsets_keep_cap_and_wall_samples_bit_identical() {
        let t = Tolerance::default();
        let m = fixture().model;
        for offset in [[1000., -1000., 1000.], [4000., -4000., 4000.]] {
            let moved = transform(&m, offset, [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                .unwrap()
                .model;
            closed(&moved);
        }
    }
    #[test]
    fn segment_budget_is_global_and_native_boundary_checked() {
        let t = Tolerance::default();
        for count in [MAX_STEPPED_BORE_SEGMENTS, MAX_STEPPED_BORE_SEGMENTS + 1] {
            let base = extrude_rectangle(600., 600., count as f64, t)
                .unwrap()
                .model;
            let segments = (0..count)
                .map(|i| {
                    let disk = translate(
                        &extrude_circle((i + 1) as f64, 1., t).unwrap().model,
                        [200., 200., i as f64],
                        t,
                    )
                    .unwrap()
                    .model;
                    let cylinder = crate::analytic::recognize(&disk, Some(t)).unwrap();
                    Segment {
                        upper_radius: cylinder.radius,
                        disk,
                        cylinder,
                        wall: FaceId(0),
                        edges: [EdgeId(0); 2],
                    }
                })
                .collect();
            let m = build(
                &base,
                &[Group { segments }],
                count as f64,
                [0.; 3],
                [0., 0., 1.],
            )
            .unwrap();
            let result = recognize(&m, t);
            if count == MAX_STEPPED_BORE_SEGMENTS {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(KernelError::ResourceLimit(_))));
            }
        }
    }
    #[test]
    fn disjoint_axial_sections_have_independent_full_tensor() {
        let t = Tolerance::default();
        let m = fixture().model;
        let p = m.mass_properties(t).unwrap();
        let pi = std::f64::consts::PI;
        let items = [
            (
                38400.,
                [40., 20., 6.],
                [80f64.powi(2) / 12., 40f64.powi(2) / 12., 12.],
            ),
            (-144. * pi, [20., 13., 2.], [9., 9., 16. / 12.]),
            (-36. * pi, [20., 13., 6.], [2.25, 2.25, 16. / 12.]),
            (-196. * pi, [20., 13., 10.], [12.25, 12.25, 16. / 12.]),
        ];
        let v = items.iter().map(|p| p.0).sum::<f64>();
        let c: [f64; 3] =
            std::array::from_fn(|i| items.iter().map(|(v, p, _)| v * p[i]).sum::<f64>() / v);
        let inertia: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                items
                    .iter()
                    .map(|(v, p, var)| {
                        let d = sub(*p, c);
                        v * ((if i == j {
                            var.iter().sum::<f64>() - var[i] + dot(d, d)
                        } else {
                            0.
                        }) - d[i] * d[j])
                    })
                    .sum()
            })
        });
        assert!((p.volume - v).abs() < 1e-8);
        assert!((m.volume() - v).abs() < 1e-8);
        for i in 0..3 {
            assert!((p.centroid.unwrap()[i] - c[i]).abs() < 1e-10);
            for (j, expected) in inertia[i].iter().enumerate() {
                assert!((p.central_inertia.unwrap()[i][j] - expected).abs() < 1e-7);
            }
        }
    }
    #[test]
    fn lineage_keeps_shoulders_and_shortens_existing_wall() {
        let t = Tolerance::default();
        let a = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let a = boolean(&a, &tool(3., -2., 16.), BooleanOp::Difference, t).unwrap();
        let old = a.history.generated_faces["bore_wall"][0];
        let b = boolean(&a.model, &tool(7., 8., 6.), BooleanOp::Difference, t).unwrap();
        assert_eq!(b.history.input_faces["left"][&old].len(), 1);
        let floor = b.history.generated_faces["bore_floor"][0];
        assert_eq!(b.history.generated_faces["bore_shoulder"], vec![floor]);
        let c = boolean(&b.model, &tool(6., -2., 6.), BooleanOp::Difference, t).unwrap();
        assert_eq!(c.history.input_faces["left"][&floor].len(), 1);
        let id = c.history.input_faces["left"][&floor][0];
        let frame = c.model.face_frame(id, [1., 0., 0.], [0.; 3], t).unwrap();
        assert_eq!(frame.origin, [20., 13., 8.]);
        assert_eq!(frame.z_axis, [0., 0., 1.]);
    }
    #[test]
    fn multiple_steps_reverse_tools_and_common_oblique_operands() {
        let t = Tolerance::default();
        let base = extrude_rectangle(80., 40., 12., t).unwrap().model;
        let tools = [tool(7., 9., 5.), tool(5., 6., 8.), tool(3., -2., 16.)];
        let mut direct = base.clone();
        for b in &tools {
            direct = boolean(&direct, b, BooleanOp::Difference, t).unwrap().model;
            closed(&direct);
        }
        assert_eq!(recognize(&direct, t).unwrap().groups[0].segments.len(), 3);
        closed(&direct);
        let q = [0.5, 0.5, 0., 0.5_f64.sqrt()];
        let offset = [12., -7., 4.];
        let mut moved = transform(&base, offset, q, t).unwrap().model;
        for b in tools.iter().rev() {
            let reverse = transform(
                &extrude_circle(3., 16., t).unwrap().model,
                [20., 13., 14.],
                [1., 0., 0., 0.],
                t,
            )
            .unwrap()
            .model;
            let b = if b == &tools[2] { &reverse } else { b };
            let b = transform(b, offset, q, t).unwrap().model;
            moved = boolean(&moved, &b, BooleanOp::Difference, t).unwrap().model;
        }
        let expected = transform(&direct, offset, q, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        let actual = moved.mass_properties(t).unwrap();
        assert!((expected.volume - actual.volume).abs() < 1e-7);
        for (a, b) in expected
            .central_inertia
            .unwrap()
            .iter()
            .flatten()
            .zip(actual.central_inertia.unwrap().iter().flatten())
        {
            assert!((a - b).abs() < 1e-7);
        }
        closed(&moved);
    }
    #[test]
    fn arc_stock_keeps_outer_profile_and_multiple_group_certificates() {
        let t = Tolerance::default();
        let disk_path = PlanarPath {
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
        let concave = PlanarPath {
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
        for (path, xy) in [(disk_path, [0., 0.]), (concave, [20., 13.])] {
            let base = extrude_path(&path, &[], 12., t).unwrap().model;
            let mut m = base.clone();
            for (r, z, h) in [(3., -2., 16.), (7., 8., 6.)] {
                let b = translate(
                    &extrude_circle(r, h, t).unwrap().model,
                    [xy[0], xy[1], z],
                    t,
                )
                .unwrap()
                .model;
                m = boolean(&m, &b, BooleanOp::Difference, t).unwrap().model;
            }
            assert!(
                (m.mass_properties(t).unwrap().volume - base.volume()
                    + 268. * std::f64::consts::PI)
                    .abs()
                    < 1e-7
            );
            closed(&m);
            closed(
                &transform(&m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                    .unwrap()
                    .model,
            );
        }
        let mut m = fixture().model;
        for (r, z, h) in [(2., -2., 16.), (4., 8., 6.)] {
            let b = translate(&extrude_circle(r, h, t).unwrap().model, [57., 26., z], t)
                .unwrap()
                .model;
            m = boolean(&m, &b, BooleanOp::Difference, t).unwrap().model;
        }
        assert_eq!(recognize(&m, t).unwrap().groups.len(), 2);
        closed(&m);
    }
    #[test]
    fn precision_contact_and_offset_axes_are_not_merged() {
        let t = Tolerance::default();
        let m = fixture().model;
        for b in [
            tool(7. + t.linear / 2., 8., 6.),
            tool(8., 8. + t.linear / 2., 6.),
            translate(&tool(8., 8., 6.), [1e-12, 0., 0.], t)
                .unwrap()
                .model,
            tool(40., 8., 6.),
        ] {
            assert!(boolean(&m, &b, BooleanOp::Difference, t).is_err());
        }
        let skew = transform(
            &tool(8., 8., 6.),
            [0.; 3],
            [0., 1e-7, 0., (1. - 1e-14_f64).sqrt()],
            t,
        )
        .unwrap()
        .model;
        assert!(boolean(&m, &skew, BooleanOp::Difference, t).is_err());
        let failed = translate(&m, [1e10, 0., 0.], t);
        assert!(failed.is_err());
    }
    #[test]
    fn enlarging_through_deletes_shoulders_and_merges_native_walls() {
        let t = Tolerance::default();
        let a = fixture().model;
        let previous = recognize(&a, t).unwrap();
        let result = boolean(&a, &tool(8., -2., 16.), BooleanOp::Difference, t).unwrap();
        assert!(result.model.profile_prism_geometry(t).unwrap().is_some());
        assert!((result.model.volume() - (38400. - 768. * std::f64::consts::PI)).abs() < 1e-7);
        assert!(!result.history.generated_faces.contains_key("bore_floor"));
        for (f, _, _, _) in previous.floors {
            assert!(result.history.input_faces["left"][&f].is_empty());
        }
        for s in &previous.groups[0].segments {
            assert!(result.history.input_faces["left"][&s.wall].is_empty());
        }
        closed(&result.model);
    }
    #[test]
    fn forged_snapshot_supports_or_topology_fail_closed() {
        let t = Tolerance::default();
        let m = fixture().model;
        let changes: Vec<fn(&mut ModelSnapshot)> = vec![
            |m| m.faces.last_mut().unwrap().try_plane_mut().unwrap().normal[2] *= -1.,
            |m| m.faces.last_mut().unwrap().try_plane_mut().unwrap().origin[0] += 1e-9,
            |m| m.faces.last_mut().unwrap().loops.pop().map(drop).unwrap(),
            |m| m.edges.last_mut().unwrap().vertices[1] = VertexId(usize::MAX),
            |m| m.vertices.last_mut().unwrap().position[0] += 1e-9,
            |m| m.shells[0].faces.pop().map(drop).unwrap(),
            |m| m.solids[0].shells.clear(),
        ];
        for (i, change) in changes.into_iter().enumerate() {
            let mut bad = m.clone();
            change(&mut bad);
            assert!(!bad.validate(t).valid, "mutation {i}");
            assert!(bad.mass_properties(t).is_err(), "mutation {i}");
            assert!(bad.volume().is_nan(), "mutation {i}");
            assert!(
                tessellate_analytic(&bad, t, TessellationPolicy::default()).is_err(),
                "mutation {i}"
            );
        }
    }
}
