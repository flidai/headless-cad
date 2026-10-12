//! Exact finite rectangular keyseats in solid circular shafts. Native trims only.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    pub(crate) origin: [f64; 3],
    pub(crate) axis: [f64; 3],
    pub(crate) x: [f64; 3],
    pub(crate) r: f64,
    pub(crate) h: f64,
    pub(crate) a: f64,
    pub(crate) b: f64,
    pub(crate) c: f64,
    pub(crate) d: f64,
}
fn bad() -> KernelError {
    KernelError::Unsupported("keyseat requires a solid circular shaft and an arithmetic-aligned rectangular cutter crossing only its positive radial exterior, with centered width, positive floor radius and two intact end journals".into())
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], v: f64) -> [f64; 3] {
    a.map(|x| x * v)
}
impl Geometry {
    pub(crate) fn clearance_envelope(&self) -> crate::trim_clearance::Envelope {
        crate::trim_clearance::Envelope {
            origin: self.origin,
            axis: self.axis,
            radial: self.x,
            outer: self.r,
            inner: 0.,
            height: self.h,
            untouched_limit: Some(self.a),
        }
    }

    fn y(&self) -> [f64; 3] {
        cross(self.axis, self.x)
    }
    fn point(&self, p: [f64; 3]) -> [f64; 3] {
        add(
            self.origin,
            add(
                mul(self.x, p[0]),
                add(mul(self.y(), p[1]), mul(self.axis, p[2])),
            ),
        )
    }
    fn vector(&self, p: [f64; 3]) -> [f64; 3] {
        add(
            mul(self.x, p[0]),
            add(mul(self.y(), p[1]), mul(self.axis, p[2])),
        )
    }
    fn q(&self) -> f64 {
        ((self.r - self.b) * (self.r + self.b)).sqrt()
    }
    fn error(&self, t: Tolerance) -> Result<f64, KernelError> {
        if !t.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        let scale = norm(self.origin)
            .max(self.r)
            .max(self.h)
            .max(self.a.abs())
            .max(self.b.abs())
            .max(self.c.abs())
            .max(self.d.abs());
        let e = 4096. * f64::EPSILON * scale;
        if !e.is_normal()
            || e > t.linear / 16.
            || [self.r, self.h, self.a, self.b, self.c, self.d]
                .iter()
                .any(|v| !v.is_finite())
            || (norm(self.axis) - 1.).abs() > 64. * f64::EPSILON
            || (norm(self.x) - 1.).abs() > 64. * f64::EPSILON
            || dot(self.axis, self.x).abs() > 64. * f64::EPSILON
        {
            return Err(KernelError::NumericRange);
        }
        let margin = t.linear + e;
        if self.a <= margin
            || self.b <= margin
            || self.b >= self.r - margin
            || self.c <= margin
            || self.d - self.c <= margin
            || self.h - self.d <= margin
            || self.q() - self.a <= margin
        {
            return Err(bad());
        }
        Ok(e)
    }
    pub(crate) fn face_centroid(&self, id: FaceId, t: Tolerance) -> Result<[f64; 3], KernelError> {
        match id.0 {
            0 => Ok(self.origin),
            1 => Ok(self.point([0., 0., self.h])),
            6 => Ok(self.point([self.a, 0., (self.c + self.d) / 2.])),
            7 | 8 => Ok(self.point([
                (self.a + self.q()) / 2.,
                if id.0 == 7 { -self.b } else { self.b },
                (self.c + self.d) / 2.,
            ])),
            9 | 10 => {
                let p = self.slot().properties(t)?.centroid.ok_or_else(bad)?;
                Ok(add(
                    p,
                    mul(
                        self.axis,
                        if id.0 == 9 {
                            -(self.d - self.c) / 2.
                        } else {
                            (self.d - self.c) / 2.
                        },
                    ),
                ))
            }
            _ => Err(bad()),
        }
    }
    pub(crate) fn slot(&self) -> ArcPrismGeometry {
        ArcPrismGeometry {
            path: PlanarPath {
                start: [self.a, -self.b],
                segments: vec![
                    PathSegment::LineTo {
                        end: [self.q(), -self.b],
                    },
                    PathSegment::ArcTo {
                        center: [0., 0.],
                        end: [self.q(), self.b],
                        clockwise: false,
                    },
                    PathSegment::LineTo {
                        end: [self.a, self.b],
                    },
                    PathSegment::LineTo {
                        end: [self.a, -self.b],
                    },
                ],
            },
            circular_holes: vec![],
            origin: self.point([0., 0., self.c]),
            axis: self.axis,
            x_direction: self.x,
            height: self.d - self.c,
        }
    }
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        self.error(t)?;
        let base = CylinderGeometry {
            origin: self.origin,
            axis: self.axis,
            x_direction: self.x,
            radius: self.r,
            height: self.h,
            inner_radius: 0.,
            inner_wall: None,
            start_cap: FaceId(0),
            end_cap: FaceId(1),
            wall: FaceId(2),
        }
        .properties()?;
        let removed = self.slot().properties(t)?;
        subtract_properties(base, removed)
    }
}
pub(crate) fn construct(g: &Geometry) -> ModelSnapshot {
    let q = g.q();
    let b = g.b;
    let a = g.a;
    let c = g.c;
    let d = g.d;
    let h = g.h;
    let theta = b.atan2(q);
    let points = [
        [q, -b, 0.],
        [q, b, 0.],
        [-g.r, 0., 0.],
        [q, -b, h],
        [q, b, h],
        [-g.r, 0., h],
        [q, -b, c],
        [q, b, c],
        [q, -b, d],
        [q, b, d],
        [a, -b, c],
        [a, b, c],
        [a, -b, d],
        [a, b, d],
    ];
    let mut m = ModelSnapshot {
        vertices: points
            .map(|p| Vertex {
                position: g.point(p),
            })
            .to_vec(),
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    for (u, v, z, angle, sweep) in [
        (0, 1, 0., -theta, 2. * theta),
        (1, 2, 0., theta, std::f64::consts::PI - theta),
        (2, 0, 0., std::f64::consts::PI, std::f64::consts::PI - theta),
        (3, 4, h, -theta, 2. * theta),
        (4, 5, h, theta, std::f64::consts::PI - theta),
        (5, 3, h, std::f64::consts::PI, std::f64::consts::PI - theta),
        (6, 7, c, -theta, 2. * theta),
        (8, 9, d, -theta, 2. * theta),
    ] {
        let (s, co) = angle.sin_cos();
        m.edges.push(Edge {
            vertices: [VertexId(u), VertexId(v)],
            curve: Curve::CircularArc(CircularArc {
                circle: Circle {
                    origin: g.point([0., 0., z]),
                    axis: g.axis,
                    x_direction: g.vector([co, s, 0.]),
                    radius: g.r,
                },
                sweep_radians: sweep,
            }),
        });
    }
    for (u, v) in [
        (0, 6),
        (6, 8),
        (8, 3),
        (1, 7),
        (7, 9),
        (9, 4),
        (2, 5),
        (10, 12),
        (11, 13),
        (10, 11),
        (12, 13),
        (6, 10),
        (7, 11),
        (8, 12),
        (9, 13),
    ] {
        let direction = sub(m.vertices[v].position, m.vertices[u].position);
        m.edges.push(Edge {
            vertices: [VertexId(u), VertexId(v)],
            curve: Curve::Line(Line {
                origin: m.vertices[u].position,
                direction: mul(direction, 1. / norm(direction)),
            }),
        });
    }
    let loops: [Vec<(usize, bool)>; 11] = [
        vec![(2, true), (1, true), (0, true)],
        vec![(3, false), (4, false), (5, false)],
        vec![
            (1, false),
            (14, false),
            (4, true),
            (13, true),
            (12, true),
            (11, true),
        ],
        vec![
            (2, false),
            (8, false),
            (9, false),
            (10, false),
            (5, true),
            (14, true),
        ],
        vec![(0, false), (11, false), (6, true), (8, true)],
        vec![(7, false), (13, false), (3, true), (10, true)],
        vec![(17, false), (16, false), (18, true), (15, true)],
        vec![(19, false), (15, false), (21, true), (9, true)],
        vec![(20, true), (12, false), (22, false), (16, true)],
        vec![(19, true), (6, false), (20, false), (17, true)],
        vec![(18, false), (22, true), (7, true), (21, false)],
    ];
    let area = b * q + g.r * g.r * theta - 2. * a * b;
    let mx = b * (g.r * g.r - a * a) - b * b * b / 3.;
    let centroid = mx / area;
    for (i, edges) in loops.into_iter().enumerate() {
        let plane = match i {
            0 => Some(([0., 0., 0.], [0., 0., -1.])),
            1 => Some(([0., 0., h], [0., 0., 1.])),
            6 => Some(([a, 0., (c + d) / 2.], [1., 0., 0.])),
            7 => Some(([(a + q) / 2., -b, (c + d) / 2.], [0., 1., 0.])),
            8 => Some(([(a + q) / 2., b, (c + d) / 2.], [0., -1., 0.])),
            9 => Some(([centroid, 0., c], [0., 0., 1.])),
            10 => Some(([centroid, 0., d], [0., 0., -1.])),
            _ => None,
        };
        let surface = if let Some((p, n)) = plane {
            Surface::Plane(Plane {
                origin: g.point(p),
                normal: g.vector(n),
            })
        } else {
            Surface::Cylinder(Cylinder {
                origin: g.origin,
                axis: g.axis,
                x_direction: g.x,
                radius: g.r,
                inward: false,
            })
        };
        m.loops.push(Loop {
            edges: edges
                .into_iter()
                .map(|(id, reversed)| EdgeUse {
                    edge: EdgeId(id),
                    reversed,
                })
                .collect(),
        });
        m.faces.push(Face {
            surface,
            loops: vec![LoopId(i)],
        });
    }
    m.shells.push(Shell {
        faces: (0..11).map(FaceId).collect(),
    });
    m.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    m
}
pub(crate) fn recognize(m: &ModelSnapshot, t: Tolerance) -> Result<Geometry, KernelError> {
    if m.vertices.len() != 14
        || m.edges.len() != 23
        || m.faces.len() != 11
        || m.loops.len() != 11
        || m.shells.len() != 1
        || m.solids.len() != 1
    {
        return Err(bad());
    }
    let Surface::Cylinder(s) = &m.faces[2].surface else {
        return Err(bad());
    };
    let origin = s.origin;
    let axis = s.axis;
    let x = s.x_direction;
    let y = cross(axis, x);
    let local = |i: usize| {
        let p = sub(m.vertices[i].position, origin);
        [dot(p, x), dot(p, y), dot(p, axis)]
    };
    let g = Geometry {
        origin,
        axis,
        x,
        r: s.radius,
        h: local(3)[2],
        a: local(10)[0],
        b: local(1)[1],
        c: local(6)[2],
        d: local(8)[2],
    };
    let error = g.error(t)?;
    let expected = construct(&g);
    let near = |a, b| norm(sub(a, b)) <= error;
    let dir = |a, b| norm(sub(a, b)) <= 128. * f64::EPSILON;
    if m.loops != expected.loops || m.shells != expected.shells || m.solids != expected.solids {
        return Err(bad());
    }
    for (a, b) in m.vertices.iter().zip(&expected.vertices) {
        if !near(a.position, b.position) {
            return Err(KernelError::NumericRange);
        }
    }
    for (a, b) in m.edges.iter().zip(&expected.edges) {
        if a.vertices != b.vertices {
            return Err(bad());
        }
        let valid = match (&a.curve, &b.curve) {
            (Curve::CircularArc(a), Curve::CircularArc(b)) => {
                near(a.circle.origin, b.circle.origin)
                    && (a.circle.radius - b.circle.radius).abs() <= error
                    && dir(a.circle.axis, b.circle.axis)
                    && dir(a.circle.x_direction, b.circle.x_direction)
                    && (a.sweep_radians - b.sweep_radians).abs() * g.r <= error
            }
            (Curve::Line(line), Curve::Line(expected)) => {
                // Certify physical departure over the trimmed edge. Endpoint
                // subtraction on a short placed edge amplifies angular roundoff.
                let length = norm(sub(
                    m.vertices[a.vertices[1].0].position,
                    m.vertices[a.vertices[0].0].position,
                ));
                near(line.origin, expected.origin)
                    && (norm(line.direction) - 1.).abs() <= 64. * f64::EPSILON
                    && norm(sub(line.direction, expected.direction)) * length <= error
            }
            _ => false,
        };
        if !valid {
            return Err(bad());
        }
    }
    for (a, b) in m.faces.iter().zip(&expected.faces) {
        if a.loops != b.loops {
            return Err(bad());
        }
        let valid = match (&a.surface, &b.surface) {
            (Surface::Plane(a), Surface::Plane(b)) => {
                near(a.origin, b.origin) && dir(a.normal, b.normal)
            }
            (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                near(a.origin, b.origin)
                    && (a.radius - b.radius).abs() <= error
                    && dir(a.axis, b.axis)
                    && dir(a.x_direction, b.x_direction)
                    && !a.inward
            }
            _ => false,
        };
        if !valid {
            return Err(bad());
        }
    }
    Ok(g)
}
pub(crate) fn bounds(m: &ModelSnapshot) -> Result<Bounds, KernelError> {
    let g = recognize(m, Tolerance::default())?;
    let y = g.y();
    let pad = 32768. * f64::EPSILON * (norm(g.origin) + g.r + g.h);
    Ok(Bounds {
        min: std::array::from_fn(|i| {
            (g.origin[i] + (g.axis[i] * g.h).min(0.) - g.r * g.x[i].hypot(y[i]) - pad).next_down()
        }),
        max: std::array::from_fn(|i| {
            (g.origin[i] + (g.axis[i] * g.h).max(0.) + g.r * g.x[i].hypot(y[i]) + pad).next_up()
        }),
    })
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    let g = recognize(m, Tolerance::default())?;
    let q = g.q();
    let area = g.b * q + g.r * g.r * g.b.atan2(q) - 2. * g.a * g.b;
    Ok(std::f64::consts::PI * g.r * g.r * g.h - area * (g.d - g.c))
}
pub(crate) fn cut(
    stock: &ModelSnapshot,
    tool: &ModelSnapshot,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let cylinder = crate::analytic::recognize(stock, Some(t))?;
    if cylinder.inner_radius != 0.
        || tool.vertices.len() != 8
        || tool.edges.len() != 12
        || tool.faces.len() != 6
        || tool.loops.len() != 6
        || tool.faces.iter().any(|f| f.loops.len() != 1)
        || tool.loops.iter().any(|l| l.edges.len() != 4)
    {
        return Err(bad());
    }
    let mut scale = tool
        .vertices
        .iter()
        .chain(&stock.vertices)
        .map(|v| norm(v.position))
        .fold(cylinder.radius.max(cylinder.height), f64::max);
    for face in &tool.faces {
        let Some(p) = face.try_plane() else {
            return Err(bad());
        };
        scale = scale.max(norm(p.origin));
    }
    let error = 4096. * f64::EPSILON * scale;
    if !error.is_normal() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    crate::convex_clearance::support::circular_consistency(stock, &cylinder, error)?;
    for edge in &tool.edges {
        let Curve::Line(line) = &edge.curve else {
            return Err(bad());
        };
        let a = tool.vertices[edge.vertices[0].0].position;
        let b = tool.vertices[edge.vertices[1].0].position;
        let delta = sub(b, a);
        let length = norm(delta);
        if norm(sub(line.origin, a)) > error
            || (norm(line.direction) - 1.).abs() > 64. * f64::EPSILON
            || norm(sub(mul(line.direction, length), delta)) > error
        {
            return Err(KernelError::NumericRange);
        }
    }

    for face in &tool.faces {
        let Some(p) = face.try_plane() else {
            return Err(bad());
        };
        let x = mul(p.normal, -1.);
        let z = cylinder.axis;
        let y = cross(z, x);
        if dot(x, z).abs() > 128. * f64::EPSILON || (norm(x) - 1.).abs() > 64. * f64::EPSILON {
            continue;
        }
        let points: Vec<_> = tool
            .vertices
            .iter()
            .map(|v| {
                let p = sub(v.position, cylinder.origin);
                [dot(p, x), dot(p, y), dot(p, z)]
            })
            .collect();
        let min: [f64; 3] =
            std::array::from_fn(|i| points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
        let max: [f64; 3] = std::array::from_fn(|i| {
            points
                .iter()
                .map(|p| p[i])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        if (min[1] + max[1]).abs() > error || max[0] <= cylinder.radius + t.linear + error {
            continue;
        }
        let g = Geometry {
            origin: cylinder.origin,
            axis: z,
            x,
            r: cylinder.radius,
            h: cylinder.height,
            a: min[0],
            b: max[1],
            c: min[2],
            d: max[2],
        };
        if g.error(t).is_err() {
            continue;
        }
        let mut masks = std::collections::BTreeSet::new();
        let mut box_valid = true;
        for p in &points {
            let mut mask = 0;
            for i in 0..3 {
                if (p[i] - min[i]).abs() <= error {
                } else if (p[i] - max[i]).abs() <= error {
                    mask |= 1 << i;
                } else {
                    box_valid = false
                }
            }
            masks.insert(mask);
        }
        if !box_valid || masks.len() != 8 {
            continue;
        }
        let mut sources = BTreeMap::new();
        for (i, f) in tool.faces.iter().enumerate() {
            let Some(plane) = f.try_plane() else {
                return Err(bad());
            };
            let basis = [x, y, z];
            let mut mapped = None;
            for k in 0..3 {
                for sign in [-1., 1.] {
                    if norm(sub(plane.normal, mul(basis[k], sign))) <= 128. * f64::EPSILON {
                        let bound = if sign < 0. { min[k] } else { max[k] };
                        if (dot(sub(plane.origin, g.origin), basis[k]) - bound).abs() > error {
                            return Err(bad());
                        }
                        let target = match (k, sign < 0.) {
                            (0, true) => Some(6),
                            (0, false) => None,
                            (1, true) => Some(7),
                            (1, false) => Some(8),
                            (2, true) => Some(9),
                            (2, false) => Some(10),
                            _ => None,
                        };
                        mapped = Some(target);
                    }
                }
            }
            let Some(target) = mapped else {
                return Err(bad());
            };
            sources.insert(FaceId(i), target.into_iter().map(FaceId).collect());
        }
        let model = construct(&g);
        recognize(&model, t)?;
        g.properties(t)?;
        let left = BTreeMap::from([
            (cylinder.start_cap, vec![FaceId(0)]),
            (cylinder.end_cap, vec![FaceId(1)]),
            (cylinder.wall, (2..6).map(FaceId).collect()),
        ]);
        let mut generated = BTreeMap::from([
            ("walls".into(), (2..6).map(FaceId).collect()),
            ("start_cap".into(), vec![FaceId(0)]),
            ("end_cap".into(), vec![FaceId(1)]),
        ]);
        for (role, id) in [
            ("keyseat_floor", 6),
            ("keyseat_side_negative", 7),
            ("keyseat_side_positive", 8),
            ("keyseat_start", 9),
            ("keyseat_end", 10),
        ] {
            generated.insert(role.into(), vec![FaceId(id)]);
        }
        return Ok(OperationResult {
            model,
            history: TopologyHistory {
                generated_faces: generated,
                input_faces: BTreeMap::from([("left".into(), left), ("right".into(), sources)]),
            },
        });
    }
    Err(bad())
}

pub(crate) fn tessellate(
    m: &ModelSnapshot,
    g: &Geometry,
    t: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let reserve = 8. * g.error(t)?;
    let canonical = construct(g);
    if !policy.chordal_error.is_finite() || policy.chordal_error <= reserve {
        return Err(KernelError::Unsupported(
            "keyseat native support reconciliation exceeds requested display error".into(),
        ));
    }
    let (resolution, bound) = crate::tessellation::circle_resolution(
        g.r,
        g.h,
        g.origin,
        TessellationPolicy {
            chordal_error: policy.chordal_error - reserve,
            max_segments: policy.max_segments,
        },
    )?;
    let theta = g.b.atan2(g.q());
    let intervals = [
        (-theta, 2. * theta),
        (theta, std::f64::consts::PI - theta),
        (std::f64::consts::PI, std::f64::consts::PI - theta),
    ];
    let mut samples = vec![];
    let mut segments = 0;
    for (group, (start, sweep)) in intervals.into_iter().enumerate() {
        let count = ((resolution as f64 * sweep / std::f64::consts::TAU).ceil() as usize).max(1);
        segments += count;
        if segments > policy.max_segments {
            return Err(KernelError::ResourceLimit(
                "keyseat arc sampling exceeds segment budget".into(),
            ));
        }
        let mut ring = vec![];
        for i in 0..=count {
            let angle = start + sweep * i as f64 / count as f64;
            let (s, c) = angle.sin_cos();
            let p = if i == 0 {
                [[g.q(), -g.b], [g.q(), g.b], [-g.r, 0.]][group]
            } else if i == count {
                [[g.q(), g.b], [-g.r, 0.], [g.q(), -g.b]][group]
            } else {
                [g.r * c, g.r * s]
            };
            ring.push(p);
        }
        samples.push(ring);
    }
    let mut patches = vec![];
    let z = [0., g.c, g.d, g.h];
    for (group, ring) in samples.iter().enumerate() {
        for pair in ring.windows(2) {
            let lo = [
                g.point([pair[0][0], pair[0][1], 0.]),
                g.point([pair[1][0], pair[1][1], 0.]),
            ];
            let hi = [
                g.point([pair[0][0], pair[0][1], g.h]),
                g.point([pair[1][0], pair[1][1], g.h]),
            ];
            patches.push(MeshPatch {
                face: FaceId(0),
                points: vec![g.origin, lo[1], lo[0]],
                normal: mul(g.axis, -1.),
                boundary_edges: vec![None, Some(EdgeId(group)), None],
            });
            patches.push(MeshPatch {
                face: FaceId(1),
                points: vec![g.point([0., 0., g.h]), hi[0], hi[1]],
                normal: g.axis,
                boundary_edges: vec![None, Some(EdgeId(group + 3)), None],
            });
        }
        for k in 0..3 {
            if group == 0 && k == 1 {
                continue;
            }
            let face = if group == 0 {
                if k == 0 {
                    4
                } else {
                    5
                }
            } else {
                group + 1
            };
            for (i, pair) in ring.windows(2).enumerate() {
                let points = vec![
                    g.point([pair[0][0], pair[0][1], z[k]]),
                    g.point([pair[1][0], pair[1][1], z[k]]),
                    g.point([pair[1][0], pair[1][1], z[k + 1]]),
                    g.point([pair[0][0], pair[0][1], z[k + 1]]),
                ];
                let radial = g.vector([pair[0][0] + pair[1][0], pair[0][1] + pair[1][1], 0.]);
                let normal = mul(radial, 1. / norm(radial));
                let low = if k == 0 {
                    Some(EdgeId(group))
                } else if group == 0 {
                    Some(EdgeId(7))
                } else {
                    None
                };
                let high = if k == 2 {
                    Some(EdgeId(group + 3))
                } else if group == 0 {
                    Some(EdgeId(6))
                } else {
                    None
                };
                let edge_at = |end: bool| {
                    let id = match (group, end) {
                        (0, false) | (2, true) => 8 + k,
                        (0, true) | (1, false) => 11 + k,
                        _ => 14,
                    };
                    Some(EdgeId(id))
                };
                patches.push(MeshPatch {
                    face: FaceId(face),
                    points,
                    normal,
                    boundary_edges: vec![
                        low,
                        if i + 2 == ring.len() {
                            edge_at(true)
                        } else {
                            None
                        },
                        high,
                        if i == 0 { edge_at(false) } else { None },
                    ],
                });
            }
        }
    }
    for id in 6..9 {
        let face = &m.faces[id];
        patches.push(MeshPatch {
            face: FaceId(id),
            points: canonical.loop_points(face.loops[0]).ok_or_else(bad)?,
            normal: face.try_plane().unwrap().normal,
            boundary_edges: m.loops[face.loops[0].0]
                .edges
                .iter()
                .map(|e| Some(e.edge))
                .collect(),
        });
    }
    for (face_id, z, arc, radial_start, radial_end, floor) in
        [(9, g.c, 6, 19, 20, 17), (10, g.d, 7, 21, 22, 18)]
    {
        let face = &m.faces[face_id];
        let mut ring = vec![[g.a, -g.b]];
        ring.extend(samples[0].iter().copied());
        ring.push([g.a, g.b]);
        // The analytic face centroid can lie outside a coarse chord polygon.
        // A convex combination of this polygon's vertices stays inside the
        // actual display domain even for a narrow shallow seat with one chord.
        let base = ring[0];
        let center2: [f64; 2] = std::array::from_fn(|k| {
            base[k]
                + ring
                    .iter()
                    .map(|p| (p[k] - base[k]) / ring.len() as f64)
                    .sum::<f64>()
        });
        let center = g.point([center2[0], center2[1], z]);
        let normal = face.try_plane().unwrap().normal;
        for i in 0..ring.len() {
            let j = (i + 1) % ring.len();
            let mut points = vec![
                center,
                g.point([ring[i][0], ring[i][1], z]),
                g.point([ring[j][0], ring[j][1], z]),
            ];
            let edge = if i == 0 {
                radial_start
            } else if j == 0 {
                floor
            } else if j + 1 == ring.len() {
                radial_end
            } else {
                arc
            };
            if face_id == 10 {
                points.swap(1, 2)
            }
            if dot(
                cross(sub(points[1], points[0]), sub(points[2], points[0])),
                normal,
            ) <= 0.
            {
                return Err(KernelError::NumericRange);
            }
            patches.push(MeshPatch {
                face: FaceId(face_id),
                points,
                normal,
                boundary_edges: vec![None, Some(EdgeId(edge)), None],
            });
        }
    }
    Ok(CylinderMesh {
        patches,
        segments,
        chordal_error_bound: bound + reserve,
    })
}

pub(crate) fn subtract_properties(
    base: MassProperties,
    removed: MassProperties,
) -> Result<MassProperties, KernelError> {
    let volume = base.volume - removed.volume;
    if !volume.is_normal() || volume <= base.volume * 4096. * f64::EPSILON {
        return Err(KernelError::NumericRange);
    }
    let reference = base.centroid.unwrap();
    let displacement = sub(removed.centroid.unwrap(), reference);
    let center = mul(displacement, -removed.volume / volume);
    let mut tensor = base.central_inertia.unwrap();
    for i in 0..3 {
        for j in 0..3 {
            tensor[i][j] -= removed.central_inertia.unwrap()[i][j]
                + removed.volume
                    * ((if i == j {
                        dot(displacement, displacement)
                    } else {
                        0.
                    }) - displacement[i] * displacement[j]);
            tensor[i][j] -=
                volume * ((if i == j { dot(center, center) } else { 0. }) - center[i] * center[j]);
        }
    }
    if tensor.iter().flatten().any(|v| !v.is_finite()) || (0..3).any(|i| tensor[i][i] <= 0.) {
        return Err(KernelError::NumericRange);
    }
    Ok(MassProperties {
        volume,
        centroid: Some(add(reference, center)),
        central_inertia: Some(tensor),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ModelSnapshot, ModelSnapshot) {
        let t = Tolerance::default();
        let shaft = extrude_circle(6., 40., t).unwrap().model;
        let tool = extrude_rectangle(4., 4., 14., t).unwrap().model;
        let tool = translate(&tool, [4., -2., 10.], t).unwrap().model;
        (shaft, tool)
    }
    #[test]
    fn finite_keyseat_native_moments_lineage_and_rigid_recovery() {
        let t = Tolerance::default();
        let (shaft, tool) = fixture();
        let result = boolean(&shaft, &tool, BooleanOp::Difference, t).unwrap();
        let m = &result.model;
        assert!(m.validate(t).valid);
        assert_eq!(
            (m.vertices.len(), m.edges.len(), m.faces.len()),
            (14, 23, 11)
        );
        let theta = (1_f64 / 3.).asin();
        let area = 2. * 32_f64.sqrt() + 36. * theta - 16.;
        let volume = std::f64::consts::PI * 36. * 40. - 14. * area;
        let removed_mx = (2. * (36. - 16.) - 8. / 3.) * 14.;
        let properties = m.mass_properties(t).unwrap();
        assert!((properties.volume - volume).abs() < 1e-8);
        assert!((m.volume() - volume).abs() < 1e-8);
        assert!((properties.centroid.unwrap()[0] + removed_mx / volume).abs() < 1e-10);
        assert!(
            (properties.centroid.unwrap()[2]
                - (std::f64::consts::PI * 36. * 40. * 20. - 14. * area * 17.) / volume)
                .abs()
                < 1e-10
        );
        assert_eq!(result.history.input_faces["left"][&FaceId(2)].len(), 4);
        assert_eq!(result.history.input_faces["right"].len(), 6);
        assert_eq!(
            result.history.input_faces["right"]
                .values()
                .filter(|v| v.is_empty())
                .count(),
            1
        );
        for (actual, expected) in m.bounds().min.into_iter().zip([-6., -6., 0.]) {
            assert!(actual <= expected && expected - actual < 1e-9);
        }
        for (actual, expected) in m.bounds().max.into_iter().zip([6., 6., 40.]) {
            assert!(actual >= expected && actual - expected < 1e-9);
        }
        let rotation = [0.2, 0.3, 0.1, 0.9273618495495703];
        let a = transform(&shaft, [10., -8., 3.], rotation, t)
            .unwrap()
            .model;
        let b = transform(&tool, [10., -8., 3.], rotation, t).unwrap().model;
        let moved = boolean(&a, &b, BooleanOp::Difference, t).unwrap().model;
        assert!(moved.validate(t).valid);
        assert!((moved.volume() - volume).abs() < 1e-8);
        let direct = transform(m, [10., -8., 3.], rotation, t).unwrap().model;
        let p = direct.mass_properties(t).unwrap();
        let q = moved.mass_properties(t).unwrap();
        for i in 0..3 {
            assert!((p.centroid.unwrap()[i] - q.centroid.unwrap()[i]).abs() < 1e-9);
            for j in 0..3 {
                assert!(
                    (p.central_inertia.unwrap()[i][j] - q.central_inertia.unwrap()[i][j]).abs()
                        < 1e-6
                );
            }
        }
        let frame = m.face_frame(FaceId(6), [0., 0., 1.], [0.; 3], t).unwrap();
        assert_eq!(frame.origin, [4., 0., 17.]);
    }
    #[test]
    fn keyseat_full_tensor_matches_independent_circle_strip_integrals_and_sections() {
        let t = Tolerance::default();
        for (r, a, b, c, d, h) in [
            (6_f64, 4_f64, 2_f64, 10_f64, 24_f64, 40_f64),
            (7., 5., 2.02, 30., 43.5, 80.),
            (3., 1.5, 1., 2., 8., 12.),
        ] {
            let shaft = extrude_circle(r, h, t).unwrap().model;
            let tool = translate(
                &extrude_rectangle(r + 2. - a, 2. * b, d - c, t)
                    .unwrap()
                    .model,
                [a, -b, c],
                t,
            )
            .unwrap()
            .model;
            let model = boolean(&shaft, &tool, BooleanOp::Difference, t)
                .unwrap()
                .model;
            let angle = (b / r).asin();
            let q = (r * r - b * b).sqrt();
            let area = b * q + r * r * angle - 2. * a * b;
            let mx = b * (r * r - a * a) - b.powi(3) / 3.;
            let xx = (r.powi(4)
                * (3. * angle / 4. + (2. * angle).sin() / 2. + (4. * angle).sin() / 16.)
                - 2. * b * a.powi(3))
                / 3.;
            let yy = r.powi(4) * (angle / 4. - (4. * angle).sin() / 16.) - 2. * a * b.powi(3) / 3.;
            let pi = std::f64::consts::PI;
            let vol = pi * r * r * h - area * (d - c);
            let centroid = [
                -mx * (d - c) / vol,
                0.,
                (pi * r * r * h * h / 2. - area * (d * d - c * c) / 2.) / vol,
            ];
            let x2 = pi * r.powi(4) * h / 4. - xx * (d - c);
            let y2 = pi * r.powi(4) * h / 4. - yy * (d - c);
            let z2 = pi * r * r * h.powi(3) / 3. - area * (d.powi(3) - c.powi(3)) / 3.;
            let xz = -mx * (d * d - c * c) / 2.;
            let expected = [
                [
                    y2 + z2 - vol * centroid[2].powi(2),
                    0.,
                    -xz + vol * centroid[0] * centroid[2],
                ],
                [
                    0.,
                    x2 + z2 - vol * (centroid[0].powi(2) + centroid[2].powi(2)),
                    0.,
                ],
                [
                    -xz + vol * centroid[0] * centroid[2],
                    0.,
                    x2 + y2 - vol * centroid[0].powi(2),
                ],
            ];
            let p = model.mass_properties(t).unwrap();
            for (actual, row) in p.central_inertia.unwrap().iter().zip(expected) {
                for (actual, expected) in actual.iter().zip(row) {
                    assert!((actual - expected).abs() < expected.abs().max(1.) * 1e-10);
                }
            }
            for (z, expected_area) in [
                (c / 2., pi * r * r),
                ((c + d) / 2., pi * r * r - area),
                ((d + h) / 2., pi * r * r),
            ] {
                let section = model
                    .section(
                        SectionPlane {
                            origin: [0., 0., z],
                            normal: [0., 0., 1.],
                            x_direction: [1., 0., 0.],
                        },
                        t,
                    )
                    .unwrap();
                assert!((section.area - expected_area).abs() < 1e-9);
            }
            assert!(model
                .section(
                    SectionPlane {
                        origin: [0., 0., (c + d) / 2.],
                        normal: [1., 0., 0.],
                        x_direction: [0., 1., 0.]
                    },
                    t
                )
                .is_err());
        }
    }
    #[test]
    fn keyseat_mesh_is_closed_and_native_end_journals_survive() {
        let t = Tolerance::default();
        let (a, b) = fixture();
        let m = boolean(&a, &b, BooleanOp::Difference, t).unwrap().model;
        let mesh = tessellate_analytic(&m, t, TessellationPolicy::default()).unwrap();
        let mut edges = BTreeMap::new();
        let key = |p: [f64; 3]| p.map(|x| (x * 1e9).round() as i64);
        for patch in &mesh.patches {
            for i in 0..patch.points.len() {
                let a = key(patch.points[i]);
                let b = key(patch.points[(i + 1) % patch.points.len()]);
                let entry = if a < b { (a, b) } else { (b, a) };
                *edges.entry(entry).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|n| *n == 2),
            "{:?}",
            edges.iter().filter(|(_, n)| **n != 2).collect::<Vec<_>>()
        );
        assert!(mesh.chordal_error_bound <= 0.01);
        assert!(mesh
            .patches
            .iter()
            .filter(|p| p.face == FaceId(4))
            .all(|p| p.points.iter().all(|v| v[2] <= 10.)));
    }
    #[test]
    fn narrow_shallow_keyseat_coarse_cap_mesh_has_positive_oriented_triangles() {
        let t = Tolerance::default();
        let shaft = extrude_circle(6., 40., t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(2.004, 0.4, 14., t).unwrap().model,
            [5.996, -0.2, 10.],
            t,
        )
        .unwrap()
        .model;
        let m = boolean(&shaft, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let mesh = tessellate_analytic(
            &m,
            t,
            TessellationPolicy {
                chordal_error: 0.05,
                max_segments: 4096,
            },
        )
        .unwrap();
        for patch in &mesh.patches {
            for pair in patch.points[1..].windows(2) {
                assert!(
                    dot(
                        cross(sub(pair[0], patch.points[0]), sub(pair[1], patch.points[0])),
                        patch.normal
                    ) > 0.,
                    "face {:?}",
                    patch.face
                );
            }
        }
    }
    #[test]
    fn unsupported_keyseat_extents_and_native_mutations_fail_closed() {
        let t = Tolerance::default();
        let (a, b) = fixture();
        for (origin, size) in [
            ([-1., -2., 10.], [9., 4., 14.]),
            ([4., -5., 10.], [4., 10., 14.]),
            ([4., -2., 0.], [4., 4., 14.]),
            ([4., -2., 30.], [4., 4., 14.]),
        ] {
            let tool = translate(
                &extrude_rectangle(size[0], size[1], size[2], t)
                    .unwrap()
                    .model,
                origin,
                t,
            )
            .unwrap()
            .model;
            assert!(boolean(&a, &tool, BooleanOp::Difference, t).is_err());
        }
        let mut inconsistent = b.clone();
        if let Curve::Line(line) = &mut inconsistent.edges[0].curve {
            line.origin[0] += 1e-8;
        }
        assert!(cut(&a, &inconsistent, t).is_err());
        assert!(boolean(&a, &inconsistent, BooleanOp::Difference, t).is_err());
        let m = boolean(&a, &b, BooleanOp::Difference, t).unwrap().model;
        let mut changed = m.clone();
        changed.vertices[10].position[0] += 1e-8;
        assert!(!changed.validate(t).valid);
        let mut changed = m.clone();
        if let Curve::CircularArc(arc) = &mut changed.edges[6].curve {
            arc.sweep_radians += 1e-8
        }
        assert!(!changed.validate(t).valid);
        let mut changed = m;
        changed.loops[9].edges[1].reversed = true;
        assert!(!changed.validate(t).valid);
    }
}
