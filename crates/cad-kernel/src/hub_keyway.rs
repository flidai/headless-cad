//! Bounded exact internal through-keyway in an annular hub, recovered from B-rep.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    origin: [f64; 3],
    axis: [f64; 3],
    outer_x: [f64; 3],
    x: [f64; 3],
    outer: f64,
    inner: f64,
    height: f64,
    roof: f64,
    half_width: f64,
}
fn bad() -> KernelError {
    KernelError::Unsupported("hub keyway requires an annular cylinder and a centered arithmetic-aligned rectangular tool covering both caps, starting inside the bore and ending beyond the bore but strictly inside the outer wall".into())
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
            outer: self.outer,
            inner: self.inner,
            height: self.height,
            untouched_limit: Some(self.q()),
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
        ((self.inner - self.half_width) * (self.inner + self.half_width)).sqrt()
    }
    fn error(&self, t: Tolerance) -> Result<f64, KernelError> {
        if !t.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        let scale = norm(self.origin)
            .max(self.outer)
            .max(self.inner)
            .max(self.height)
            .max(self.roof.abs())
            .max(self.half_width.abs());
        let error = 4096. * f64::EPSILON * scale;
        if !error.is_normal()
            || error > t.linear / 16.
            || [
                self.outer,
                self.inner,
                self.height,
                self.roof,
                self.half_width,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || [self.axis, self.x, self.outer_x]
                .iter()
                .any(|v| (norm(*v) - 1.).abs() > 64. * f64::EPSILON)
            || dot(self.axis, self.x).abs() > 64. * f64::EPSILON
            || dot(self.axis, self.outer_x).abs() > 64. * f64::EPSILON
        {
            return Err(KernelError::NumericRange);
        }
        let margin = t.linear + error;
        if self.height <= margin
            || self.inner <= margin
            || self.half_width <= margin
            || self.inner - self.half_width <= margin
            || self.roof - self.inner <= margin
            || self.outer - self.roof.hypot(self.half_width) <= margin
        {
            return Err(bad());
        }
        Ok(error)
    }
    fn removed(&self) -> ArcPrismGeometry {
        let q = self.q();
        let b = self.half_width;
        let c = self.roof;
        ArcPrismGeometry {
            path: PlanarPath {
                start: [q, -b],
                segments: vec![
                    PathSegment::LineTo { end: [c, -b] },
                    PathSegment::LineTo { end: [c, b] },
                    PathSegment::LineTo { end: [q, b] },
                    PathSegment::ArcTo {
                        center: [0., 0.],
                        end: [q, -b],
                        clockwise: true,
                    },
                ],
            },
            circular_holes: vec![],
            slot_holes: vec![],
            origin: self.origin,
            axis: self.axis,
            x_direction: self.x,
            height: self.height,
        }
    }
    pub(crate) fn properties(&self, t: Tolerance) -> Result<MassProperties, KernelError> {
        self.error(t)?;
        let base = CylinderGeometry {
            origin: self.origin,
            axis: self.axis,
            x_direction: self.outer_x,
            radius: self.outer,
            height: self.height,
            inner_radius: self.inner,
            inner_wall: Some(FaceId(3)),
            start_cap: FaceId(0),
            end_cap: FaceId(1),
            wall: FaceId(2),
        }
        .properties()?;
        let removed = self.removed().properties(t)?;
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
                tensor[i][j] -= volume
                    * ((if i == j { dot(center, center) } else { 0. }) - center[i] * center[j]);
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
    pub(crate) fn face_centroid(&self, id: FaceId, t: Tolerance) -> Result<[f64; 3], KernelError> {
        match id.0 {
            0 | 1 => {
                let center = self.properties(t)?.centroid.ok_or_else(bad)?;
                Ok(add(
                    center,
                    mul(
                        self.axis,
                        if id.0 == 0 {
                            -self.height / 2.
                        } else {
                            self.height / 2.
                        },
                    ),
                ))
            }
            5 => Ok(self.point([self.roof, 0., self.height / 2.])),
            6 | 7 => Ok(self.point([
                (self.roof + self.q()) / 2.,
                if id.0 == 6 {
                    -self.half_width
                } else {
                    self.half_width
                },
                self.height / 2.,
            ])),
            _ => Err(bad()),
        }
    }
}
fn construct(g: &Geometry) -> ModelSnapshot {
    let outer_point = |z| add(g.origin, add(mul(g.outer_x, g.outer), mul(g.axis, z)));
    let q = g.q();
    let b = g.half_width;
    let c = g.roof;
    let theta = b.atan2(q);
    let profile = [[q, -b], [c, -b], [c, b], [q, b], [-g.inner, 0.]];
    let mut m = ModelSnapshot {
        vertices: vec![
            Vertex {
                position: outer_point(0.),
            },
            Vertex {
                position: outer_point(g.height),
            },
        ],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    for z in [0., g.height] {
        for p in profile {
            m.vertices.push(Vertex {
                position: g.point([p[0], p[1], z]),
            });
        }
    }
    for (id, z) in [(0, 0.), (1, g.height)] {
        m.edges.push(Edge {
            vertices: [VertexId(id), VertexId(id)],
            curve: Curve::Circle(Circle {
                origin: g.point([0., 0., z]),
                axis: g.axis,
                x_direction: g.outer_x,
                radius: g.outer,
            }),
        });
    }
    m.edges.push(Edge {
        vertices: [VertexId(0), VertexId(1)],
        curve: Curve::Line(Line {
            origin: outer_point(0.),
            direction: g.axis,
        }),
    });
    for (layer, z) in [0., g.height].into_iter().enumerate() {
        for i in 0..5 {
            let u = 2 + layer * 5 + i;
            let v = 2 + layer * 5 + (i + 1) % 5;
            let curve = if i < 3 {
                let delta = sub(m.vertices[v].position, m.vertices[u].position);
                Curve::Line(Line {
                    origin: m.vertices[u].position,
                    direction: mul(delta, 1. / norm(delta)),
                })
            } else {
                let angle = if i == 3 { theta } else { std::f64::consts::PI };
                let (s, c) = angle.sin_cos();
                Curve::CircularArc(CircularArc {
                    circle: Circle {
                        origin: g.point([0., 0., z]),
                        axis: g.axis,
                        x_direction: g.vector([c, s, 0.]),
                        radius: g.inner,
                    },
                    sweep_radians: std::f64::consts::PI - theta,
                })
            };
            m.edges.push(Edge {
                vertices: [VertexId(u), VertexId(v)],
                curve,
            });
        }
    }
    for i in 0..5 {
        m.edges.push(Edge {
            vertices: [VertexId(2 + i), VertexId(7 + i)],
            curve: Curve::Line(Line {
                origin: m.vertices[2 + i].position,
                direction: g.axis,
            }),
        });
    }
    let eu = |i, reversed| EdgeUse {
        edge: EdgeId(i),
        reversed,
    };
    m.loops = vec![
        Loop {
            edges: vec![eu(0, true)],
        },
        Loop {
            edges: vec![eu(1, false)],
        },
        Loop {
            edges: vec![eu(0, false), eu(2, false), eu(1, true), eu(2, true)],
        },
        Loop {
            edges: (3..8).map(|i| eu(i, false)).collect(),
        },
        Loop {
            edges: (8..13).rev().map(|i| eu(i, true)).collect(),
        },
    ];
    for (i, z, sign) in [(0, 0., -1.), (1, g.height, 1.)] {
        m.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: g.point([0., 0., z]),
                normal: mul(g.axis, sign),
            }),
            loops: vec![LoopId(i), LoopId(i + 3)],
        });
    }
    m.faces.push(Face {
        surface: Surface::Cylinder(Cylinder {
            origin: g.origin,
            axis: g.axis,
            x_direction: g.outer_x,
            radius: g.outer,
            inward: false,
        }),
        loops: vec![LoopId(2)],
    });
    for (face, segment) in [(3, 3), (4, 4), (5, 1), (6, 0), (7, 2)] {
        let loop_id = m.loops.len();
        m.loops.push(Loop {
            edges: vec![
                eu(3 + segment, true),
                eu(13 + segment, false),
                eu(8 + segment, false),
                eu(13 + (segment + 1) % 5, true),
            ],
        });
        let surface = if face < 5 {
            Surface::Cylinder(Cylinder {
                origin: g.origin,
                axis: g.axis,
                x_direction: g.x,
                radius: g.inner,
                inward: true,
            })
        } else {
            let (p, n) = match face {
                5 => ([c, 0., g.height / 2.], [-1., 0., 0.]),
                6 => ([(c + q) / 2., -b, g.height / 2.], [0., 1., 0.]),
                _ => ([(c + q) / 2., b, g.height / 2.], [0., -1., 0.]),
            };
            Surface::Plane(Plane {
                origin: g.point(p),
                normal: g.vector(n),
            })
        };
        m.faces.push(Face {
            surface,
            loops: vec![LoopId(loop_id)],
        });
    }
    m.shells.push(Shell {
        faces: (0..8).map(FaceId).collect(),
    });
    m.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    m
}
pub(crate) fn recognize(m: &ModelSnapshot, t: Tolerance) -> Result<Geometry, KernelError> {
    if m.vertices.len() != 12
        || m.edges.len() != 18
        || m.faces.len() != 8
        || m.loops.len() != 10
        || m.shells.len() != 1
        || m.solids.len() != 1
    {
        return Err(bad());
    }
    let (Surface::Cylinder(outer), Surface::Cylinder(inner)) =
        (&m.faces[2].surface, &m.faces[3].surface)
    else {
        return Err(bad());
    };
    let origin = outer.origin;
    let axis = outer.axis;
    let x = inner.x_direction;
    let y = cross(axis, x);
    let point = sub(m.vertices[4].position, origin);
    let g = Geometry {
        origin,
        axis,
        outer_x: outer.x_direction,
        x,
        outer: outer.radius,
        inner: inner.radius,
        height: dot(sub(m.vertices[1].position, origin), axis),
        roof: dot(point, x),
        half_width: dot(point, y),
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
        let circle = |a: &Circle, b: &Circle| {
            near(a.origin, b.origin)
                && (a.radius - b.radius).abs() <= error
                && dir(a.axis, b.axis)
                && dir(a.x_direction, b.x_direction)
        };
        let valid = match (&a.curve, &b.curve) {
            (Curve::Circle(a), Curve::Circle(b)) => circle(a, b),
            (Curve::CircularArc(a), Curve::CircularArc(b)) => {
                circle(&a.circle, &b.circle)
                    && (a.sweep_radians - b.sweep_radians).abs() * g.inner <= error
            }
            (Curve::Line(line), Curve::Line(expected_line)) => {
                // Short native edges magnify endpoint subtraction roundoff in
                // their unit direction. Bound physical support departure over
                // the entire trimmed segment, rather than its bare angle.
                let length = norm(sub(
                    m.vertices[a.vertices[1].0].position,
                    m.vertices[a.vertices[0].0].position,
                ));
                near(line.origin, expected_line.origin)
                    && (norm(line.direction) - 1.).abs() <= 64. * f64::EPSILON
                    && norm(sub(line.direction, expected_line.direction)) * length <= error
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
                    && a.inward == b.inward
            }
            _ => false,
        };
        if !valid {
            return Err(bad());
        }
    }
    Ok(g)
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    let g = recognize(m, Tolerance::default())?;
    let a = 2. * g.half_width * g.roof
        - g.half_width * g.q()
        - g.inner * g.inner * (g.half_width / g.inner).asin();
    Ok((std::f64::consts::PI * (g.outer * g.outer - g.inner * g.inner) - a) * g.height)
}
pub(crate) fn bounds(m: &ModelSnapshot) -> Result<Bounds, KernelError> {
    let g = recognize(m, Tolerance::default())?;
    let y = cross(g.axis, g.outer_x);
    let pad = 32768. * f64::EPSILON * (norm(g.origin) + g.outer + g.height);
    Ok(Bounds {
        min: std::array::from_fn(|i| {
            (g.origin[i] + (g.axis[i] * g.height).min(0.)
                - g.outer * g.outer_x[i].hypot(y[i])
                - pad)
                .next_down()
        }),
        max: std::array::from_fn(|i| {
            (g.origin[i]
                + (g.axis[i] * g.height).max(0.)
                + g.outer * g.outer_x[i].hypot(y[i])
                + pad)
                .next_up()
        }),
    })
}
pub(crate) fn cut(
    stock: &ModelSnapshot,
    tool: &ModelSnapshot,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let cylinder = crate::analytic::recognize(stock, Some(t))?;
    if cylinder.inner_radius <= 0.
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
    for f in &tool.faces {
        let Some(p) = f.try_plane() else {
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
        if norm(sub(line.origin, a)) > error
            || (norm(line.direction) - 1.).abs() > 64. * f64::EPSILON
            || norm(sub(mul(line.direction, norm(delta)), delta)) > error
        {
            return Err(KernelError::NumericRange);
        }
    }
    for f in &tool.faces {
        let x = f.try_plane().ok_or_else(bad)?.normal;
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
        let g = Geometry {
            origin: cylinder.origin,
            axis: z,
            outer_x: cylinder.x_direction,
            x,
            outer: cylinder.radius,
            inner: cylinder.inner_radius,
            height: cylinder.height,
            roof: max[0],
            half_width: max[1],
        };
        if g.error(t).is_err()
            || min[0] < -error
            || min[0] >= g.q() - t.linear - error
            || (min[1] + max[1]).abs() > error
            || min[2] >= -t.linear - error
            || max[2] <= g.height + t.linear + error
        {
            continue;
        }
        let mut masks = BTreeSet::new();
        let mut valid = true;
        for p in &points {
            let mut mask = 0;
            for i in 0..3 {
                if (p[i] - min[i]).abs() <= error {
                } else if (p[i] - max[i]).abs() <= error {
                    mask |= 1 << i
                } else {
                    valid = false
                }
            }
            masks.insert(mask);
        }
        if !valid || masks.len() != 8 {
            continue;
        }
        let mut sources = BTreeMap::new();
        for (i, f) in tool.faces.iter().enumerate() {
            let plane = f.try_plane().ok_or_else(bad)?;
            let basis = [x, y, z];
            let mut mapped = None;
            for k in 0..3 {
                for sign in [-1., 1.] {
                    if norm(sub(plane.normal, mul(basis[k], sign))) <= 128. * f64::EPSILON {
                        let bound = if sign < 0. { min[k] } else { max[k] };
                        if (dot(sub(plane.origin, g.origin), basis[k]) - bound).abs() > error {
                            return Err(bad());
                        }
                        let target = match (k, sign > 0.) {
                            (0, true) => Some(5),
                            (1, false) => Some(6),
                            (1, true) => Some(7),
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
            (cylinder.wall, vec![FaceId(2)]),
            (cylinder.inner_wall.unwrap(), vec![FaceId(3), FaceId(4)]),
        ]);
        let generated = BTreeMap::from([
            ("start_cap".into(), vec![FaceId(0)]),
            ("end_cap".into(), vec![FaceId(1)]),
            ("wall".into(), vec![FaceId(2)]),
            ("inner_wall".into(), vec![FaceId(3), FaceId(4)]),
            ("hub_keyway_roof".into(), vec![FaceId(5)]),
            ("hub_keyway_side_negative".into(), vec![FaceId(6)]),
            ("hub_keyway_side_positive".into(), vec![FaceId(7)]),
        ]);
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
    let allowed = policy.chordal_error - reserve;
    if !allowed.is_finite() || allowed <= 0. {
        return Err(KernelError::Unsupported(
            "hub keyway support reconciliation exceeds display error".into(),
        ));
    }
    let canonical = construct(g);
    let gap = g.outer - g.roof.hypot(g.half_width);
    // Outer chords must remain outside the entire keyed opening. Refine by the
    // native wall margin before certified cap triangulation; never drop the hole.
    let outer_policy = TessellationPolicy {
        chordal_error: allowed.min(gap / 4.),
        max_segments: policy.max_segments,
    };
    let (outer_n, outer_bound) =
        crate::tessellation::circle_resolution(g.outer, g.height, g.origin, outer_policy)?;
    let (inner_n, inner_bound) = crate::tessellation::circle_resolution(
        g.inner,
        g.height,
        g.origin,
        TessellationPolicy {
            chordal_error: allowed,
            max_segments: policy.max_segments,
        },
    )?;
    let mut samples = BTreeMap::<EdgeId, Vec<[f64; 3]>>::new();
    let mut segments = 0usize;
    for id in [0, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
        let edge = &canonical.edges[id];
        let values = match &edge.curve {
            Curve::Line(_) => edge
                .vertices
                .map(|v| canonical.vertices[v.0].position)
                .to_vec(),
            Curve::Circle(c) => {
                let y = cross(c.axis, c.x_direction);
                let mut points = vec![];
                for i in 0..outer_n {
                    let (s, co) = (std::f64::consts::TAU * i as f64 / outer_n as f64).sin_cos();
                    points.push(add(
                        c.origin,
                        add(mul(c.x_direction, c.radius * co), mul(y, c.radius * s)),
                    ));
                }
                points[0] = canonical.vertices[edge.vertices[0].0].position;
                points.push(points[0]);
                if id == 0 {
                    segments += outer_n;
                }
                points
            }
            Curve::CircularArc(arc) => {
                let c = &arc.circle;
                let count = ((inner_n as f64 * arc.sweep_radians.abs() / std::f64::consts::TAU)
                    .ceil() as usize)
                    .max(1);
                let y = cross(c.axis, c.x_direction);
                let mut points = vec![];
                for i in 0..=count {
                    let (s, co) = (arc.sweep_radians * i as f64 / count as f64).sin_cos();
                    points.push(add(
                        c.origin,
                        add(mul(c.x_direction, c.radius * co), mul(y, c.radius * s)),
                    ));
                }
                points[0] = canonical.vertices[edge.vertices[0].0].position;
                points[count] = canonical.vertices[edge.vertices[1].0].position;
                if id < 8 {
                    segments += count;
                }
                points
            }
        };
        samples.insert(EdgeId(id), values);
    }
    if segments > policy.max_segments {
        return Err(KernelError::ResourceLimit(
            "hub keyway combined circle sampling exceeds segment budget".into(),
        ));
    }
    let mut patches = vec![];
    for (face, lower, upper, inward) in [
        (2, 0, 1, false),
        (3, 6, 11, true),
        (4, 7, 12, true),
        (5, 4, 9, true),
        (6, 3, 8, true),
        (7, 5, 10, true),
    ] {
        let bottom = &samples[&EdgeId(lower)];
        let top = &samples[&EdgeId(upper)];
        if bottom.len() != top.len() {
            return Err(bad());
        }
        for i in 0..bottom.len() - 1 {
            let j = i + 1;
            let points = if inward {
                vec![bottom[j], bottom[i], top[i], top[j]]
            } else {
                vec![bottom[i], bottom[j], top[j], top[i]]
            };
            let normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
            let length = norm(normal);
            if !length.is_normal() {
                return Err(KernelError::NumericRange);
            }
            let vertical = |point: [f64; 3]| {
                canonical
                    .edges
                    .iter()
                    .enumerate()
                    .find(|(_, e)| {
                        matches!(e.curve, Curve::Line(_))
                            && canonical.vertices[e.vertices[0].0].position == point
                            && dot(
                                sub(canonical.vertices[e.vertices[1].0].position, point),
                                g.axis,
                            ) > g.height / 2.
                    })
                    .map(|(id, _)| EdgeId(id))
            };
            let (start, end) = if inward {
                (bottom[i], bottom[j])
            } else {
                (bottom[j], bottom[i])
            };
            patches.push(MeshPatch {
                face: FaceId(face),
                points,
                normal: mul(normal, 1. / length),
                boundary_edges: vec![
                    Some(EdgeId(lower)),
                    vertical(start),
                    Some(EdgeId(upper)),
                    vertical(end),
                ],
            });
        }
    }
    for id in 0..2 {
        let normal = canonical.faces[id].try_plane().unwrap().normal;
        let x = g.x;
        let y = cross(normal, x);
        let mut world = vec![];
        let mut rings = vec![];
        let mut boundary = BTreeMap::new();
        for loop_id in &canonical.faces[id].loops {
            let offset = world.len();
            let mut ids = vec![];
            for eu in &canonical.loops[loop_id.0].edges {
                let values = &samples[&eu.edge];
                let count = values.len() - 1;
                for k in 0..count {
                    world.push(values[if eu.reversed { count - k } else { k }]);
                    ids.push(eu.edge);
                }
            }
            if world.len() > 2048 {
                return Err(KernelError::ResourceLimit(
                    "hub keyway exceeds2048 sampled cap points".into(),
                ));
            }
            let mut ring = vec![];
            for (k, edge) in ids.iter().enumerate() {
                boundary.insert((offset + k, offset + (k + 1) % ids.len()), *edge);
                let d = sub(world[offset + k], g.origin);
                ring.push([dot(d, x), dot(d, y)]);
            }
            rings.push(ring);
        }
        for triangle in crate::triangulation::triangulate_rings_2d(&rings)? {
            patches.push(MeshPatch {
                face: FaceId(id),
                points: triangle.map(|i| world[i]).to_vec(),
                normal,
                boundary_edges: (0..3)
                    .map(|i| boundary.get(&(triangle[i], triangle[(i + 1) % 3])).copied())
                    .collect(),
            });
        }
    }
    // All display boundaries share canonical samples; accepted native arithmetic
    // reconciliation is charged explicitly, never admitted as a mesh crack.
    let _ = m;
    Ok(CylinderMesh {
        patches,
        segments,
        chordal_error_bound: outer_bound.max(inner_bound) + reserve,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(b: f64, c: f64) -> (ModelSnapshot, ModelSnapshot) {
        let t = Tolerance::default();
        let hub = extrude_annulus(10., 6., 14., t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(c, 2. * b, 16., t).unwrap().model,
            [0., -b, -1.],
            t,
        )
        .unwrap()
        .model;
        (hub, tool)
    }
    #[test]
    fn hub_keyway_independent_integrals_and_transverse_section() {
        let t = Tolerance::default();
        let b = 2_f64;
        let c = 8_f64;
        let r = 6_f64;
        let h = 14_f64;
        let (stock, tool) = fixture(b, c);
        let m = boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        assert!(m.validate(t).valid);
        assert_eq!(
            (m.vertices.len(), m.edges.len(), m.faces.len()),
            (12, 18, 8)
        );
        let theta = (b / r).asin();
        let q = (r * r - b * b).sqrt();
        let area = std::f64::consts::PI * (100. - 36.) - (2. * b * c - b * q - r * r * theta);
        let mx = -(b * (c * c - r * r) + b.powi(3) / 3.) * h;
        let volume = area * h;
        let x2 = std::f64::consts::PI * (10000. - 1296.) * h / 4.
            - h * (2. * b * c.powi(3)
                - r.powi(4)
                    * (3. * theta / 4. + (2. * theta).sin() / 2. + (4. * theta).sin() / 16.))
                / 3.;
        let y2 = std::f64::consts::PI * (10000. - 1296.) * h / 4.
            - h * (2. * c * b.powi(3) / 3. - r.powi(4) * (theta / 4. - (4. * theta).sin() / 16.));
        let z2 = volume * h * h / 12.;
        let x2 = x2 - mx * mx / volume;
        let p = m.mass_properties(t).unwrap();
        assert!((p.volume - volume).abs() < 1e-9);
        assert!((p.centroid.unwrap()[0] - mx / volume).abs() < 1e-12);
        let expected = [[y2 + z2, 0., 0.], [0., x2 + z2, 0.], [0., 0., x2 + y2]];
        for (actual, row) in p.central_inertia.unwrap().iter().zip(expected) {
            for (a, b) in actual.iter().zip(row) {
                assert!((a - b).abs() < 1e-7, "{a} {b}");
            }
        }
        let section = m
            .section(
                SectionPlane {
                    origin: [0., 0., 7.],
                    normal: [0., 0., 1.],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        assert!((section.area - area).abs() < 1e-10);
        assert!(m.validate_clearance_operand(t).is_ok());
    }
    #[test]
    fn hub_keyway_shallow_oblique_operands_and_result_recover() {
        let t = Tolerance::default();
        let (a, b) = fixture(0.2, 6.004);
        let pose = [0.5, 0.5, 0., 0.5_f64.sqrt()];
        let offset = [25., -12., 7.];
        let stock = transform(&a, offset, pose, t).unwrap().model;
        let tool = transform(&b, offset, pose, t).unwrap().model;
        let result = cut(&stock, &tool, t).unwrap().model;
        assert!(result.validate(t).valid);
        let native = cut(&a, &b, t).unwrap().model;
        assert!(
            transform(&native, offset, pose, t)
                .unwrap()
                .model
                .validate(t)
                .valid
        );
    }
    #[test]
    fn hub_keyway_native_and_tool_mutations_fail_closed() {
        let t = Tolerance::default();
        let (stock, tool) = fixture(2., 8.);
        let mut badtool = tool.clone();
        if let Curve::Line(line) = &mut badtool.edges[0].curve {
            line.origin[0] += 1e-8;
        }
        assert!(cut(&stock, &badtool, t).is_err());
        let m = cut(&stock, &tool, t).unwrap().model;
        for kind in 0..4 {
            let mut changed = m.clone();
            match kind {
                0 => changed.vertices[4].position[0] += 1e-8,
                1 => {
                    if let Curve::CircularArc(a) = &mut changed.edges[6].curve {
                        a.sweep_radians += 1e-8;
                    }
                }
                2 => {
                    if let Curve::Circle(c) = &mut changed.edges[0].curve {
                        c.x_direction[1] += 1e-8;
                    }
                }
                _ => changed.loops[3].edges[0].reversed = true,
            }
            assert!(!changed.validate(t).valid, "mutation {kind}");
        }
        for (b, c) in [(6., 8.), (2., 6.), (2., 10.)] {
            let (s, tool) = fixture(b, c);
            assert!(boolean(&s, &tool, BooleanOp::Difference, t).is_err());
        }
    }
    #[test]
    fn hub_keyway_narrow_wall_and_shallow_mesh_are_closed_oriented_and_bounded() {
        let t = Tolerance::default();
        for (b, c) in [(2., 8.), (0.2, 6.004), (2., 96_f64.sqrt() - 0.0001)] {
            let (stock, tool) = fixture(b, c);
            let m = cut(&stock, &tool, t).unwrap().model;
            let mesh = tessellate_analytic(&m, t, TessellationPolicy::default()).unwrap();
            let mut edges = BTreeMap::new();
            let key = |p: [f64; 3]| p.map(|x| (x * 1e10).round() as i64);
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
                for i in 0..patch.points.len() {
                    let a = key(patch.points[i]);
                    let b = key(patch.points[(i + 1) % patch.points.len()]);
                    let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                    let e = edges.entry(edge).or_insert((0, 0));
                    e.0 += 1;
                    e.1 += sign;
                }
            }
            assert!(edges.values().all(|v| *v == (2, 0)));
            assert!(mesh.chordal_error_bound <= TessellationPolicy::default().chordal_error);
            assert!(tessellate_analytic(
                &m,
                t,
                TessellationPolicy {
                    chordal_error: 0.01,
                    max_segments: 8
                }
            )
            .is_err());
        }
    }
}
