//! Native bounded simple CCW line/arc right prisms. No sampled modeling geometry.
use crate::math::{finite, norm};
use crate::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanarPath {
    pub start: [f64; 2],
    pub segments: Vec<PathSegment>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSegment {
    LineTo {
        end: [f64; 2],
    },
    ArcTo {
        center: [f64; 2],
        end: [f64; 2],
        clockwise: bool,
    },
}
impl PathSegment {
    fn end(&self) -> [f64; 2] {
        match self {
            Self::LineTo { end } | Self::ArcTo { end, .. } => *end,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ArcPrismGeometry {
    pub path: PlanarPath,
    pub circular_holes: Vec<CircleHole>,
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub height: f64,
}
#[derive(Clone)]
struct Segment {
    a: [f64; 2],
    b: [f64; 2],
    arc: Option<([f64; 2], f64, f64, f64)>,
}
#[path = "arc_prism_validation.rs"]
mod path_validation;
fn bad() -> KernelError {
    KernelError::Unsupported("expected a bounded simple CCW line/arc prism".into())
}
fn cross2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn segments(path: &PlanarPath, tol: Tolerance) -> Result<Vec<Segment>, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !(2..=MAX_POLYGON_POINTS).contains(&path.segments.len()) {
        return Err(KernelError::ResourceLimit(
            "path requires 2..=256 segments".into(),
        ));
    }
    if path.segments.last().unwrap().end() != path.start {
        return Err(bad());
    }
    let values = std::iter::once(path.start).chain(path.segments.iter().flat_map(|s| match s {
        PathSegment::LineTo { end } => vec![*end],
        PathSegment::ArcTo { center, end, .. } => vec![*center, *end],
    }));
    let mut scale = 0f64;
    for p in values {
        for v in p {
            if !v.is_finite() || v.abs() > 1e100 || (v != 0. && v.abs() < 1e-100) {
                return Err(KernelError::NumericRange);
            }
            scale = scale.max(v.abs());
        }
    }
    let error = scale * 1024. * f64::EPSILON;
    if error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let mut result = vec![];
    let mut a = path.start;
    for s in &path.segments {
        let b = s.end();
        if len(sub2(b, a)) <= tol.linear + error {
            return Err(bad());
        }
        let arc = if let PathSegment::ArcTo {
            center, clockwise, ..
        } = s
        {
            let va = sub2(a, *center);
            let vb = sub2(b, *center);
            let r = len(va);
            if !r.is_normal() || r <= tol.linear || (r - len(vb)).abs() > error {
                return Err(bad());
            }
            let start = va[1].atan2(va[0]);
            let mut sweep = cross2(va, vb).atan2(dot2(va, vb));
            if *clockwise {
                if sweep >= 0. {
                    sweep -= std::f64::consts::TAU;
                }
            } else if sweep <= 0. {
                sweep += std::f64::consts::TAU;
            }
            if sweep.abs() > std::f64::consts::PI + 64. * f64::EPSILON
                || r * sweep.abs() <= tol.linear
            {
                return Err(bad());
            }
            Some((*center, r, start, sweep))
        } else {
            None
        };
        result.push(Segment { a, b, arc });
        a = b;
    }
    // Signed tangent turning establishes CCW orientation after analytic
    // simplicity certification; neither predicate samples the curve.
    let tangent = |s: &Segment, end: bool| {
        if let Some((c, r, _, sweep)) = s.arc {
            let v = sub2(if end { s.b } else { s.a }, c);
            [-v[1] / r * sweep.signum(), v[0] / r * sweep.signum()]
        } else {
            let d = sub2(s.b, s.a);
            let n = len(d);
            d.map(|v| v / n)
        }
    };
    let mut turning = 0.;
    for i in 0..result.len() {
        let s = &result[i];
        if let Some((_, _, _, sweep)) = s.arc {
            turning += sweep;
        }
        let a = tangent(s, true);
        let b = tangent(&result[(i + 1) % result.len()], false);
        let angle = cross2(a, b).atan2(dot2(a, b));
        if angle.abs() >= std::f64::consts::PI - 4096. * f64::EPSILON {
            return Err(bad());
        }
        turning += angle;
    }
    if (turning - std::f64::consts::TAU).abs() > result.len() as f64 * 8192. * f64::EPSILON {
        return Err(bad());
    }
    path_validation::simple(&result, tol, error)?;
    Ok(result)
}
pub fn validate_path(
    path: &PlanarPath,
    holes: &[CircleHole],
    tol: Tolerance,
) -> Result<(), KernelError> {
    let seg = segments(path, tol)?;
    if holes.len() > MAX_POLYGON_HOLES {
        return Err(KernelError::ResourceLimit(
            "path supports 32 circular holes".into(),
        ));
    }
    let scale = path
        .start
        .into_iter()
        .chain(seg.iter().flat_map(|s| s.a))
        .chain(
            seg.iter()
                .filter_map(|s| s.arc)
                .flat_map(|(c, r, _, _)| [c[0], c[1], r]),
        )
        .chain(
            holes
                .iter()
                .flat_map(|h| [h.center[0], h.center[1], h.radius]),
        )
        .map(f64::abs)
        .fold(0., f64::max);
    let error = scale * 2048. * f64::EPSILON;
    if !error.is_finite() || error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    for (i, h) in holes.iter().enumerate() {
        validate_circle(h.radius, tol)?;
        if !h.center.iter().all(|v| v.is_finite()) {
            return Err(KernelError::NumericRange);
        }
        for s in &seg {
            if path_validation::distance(s, h.center, error) - h.radius <= tol.linear + error {
                return Err(bad());
            }
        }
        if !path_validation::inside(&seg, h.center, error)? {
            return Err(bad());
        }
        for other in &holes[..i] {
            if len(sub2(h.center, other.center)) - h.radius - other.radius <= tol.linear + error {
                return Err(bad());
            }
        }
    }
    Ok(())
}
fn point(g: &ArcPrismGeometry, p: [f64; 2], z: f64) -> [f64; 3] {
    let y = cross(g.axis, g.x_direction);
    std::array::from_fn(|i| g.origin[i] + p[0] * g.x_direction[i] + p[1] * y[i] + z * g.axis[i])
}
fn vector(g: &ArcPrismGeometry, p: [f64; 2]) -> [f64; 3] {
    let y = cross(g.axis, g.x_direction);
    std::array::from_fn(|i| p[0] * g.x_direction[i] + p[1] * y[i])
}
fn construct(g: &ArcPrismGeometry, tol: Tolerance) -> Result<ModelSnapshot, KernelError> {
    let seg = segments(&g.path, tol)?;
    let n = seg.len();
    let mut m = ModelSnapshot {
        vertices: vec![],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    for z in [0., g.height] {
        for s in &seg {
            m.vertices.push(Vertex {
                position: point(g, s.a, z),
            });
        }
    }
    for (layer, z) in [0., g.height].into_iter().enumerate() {
        for (i, s) in seg.iter().enumerate() {
            let curve = if let Some((c, r, _, sweep)) = s.arc {
                Curve::CircularArc(CircularArc {
                    circle: Circle {
                        origin: point(g, c, z),
                        axis: g.axis,
                        x_direction: vector(g, sub2(s.a, c).map(|v| v / r)),
                        radius: r,
                    },
                    sweep_radians: sweep,
                })
            } else {
                let d = sub2(s.b, s.a);
                let length = len(d);
                Curve::Line(Line {
                    origin: point(g, s.a, z),
                    direction: vector(g, d.map(|v| v / length)),
                })
            };
            m.edges.push(Edge {
                vertices: [VertexId(layer * n + i), VertexId(layer * n + (i + 1) % n)],
                curve,
            });
        }
    }
    for (i, s) in seg.iter().enumerate() {
        m.edges.push(Edge {
            vertices: [VertexId(i), VertexId(n + i)],
            curve: Curve::Line(Line {
                origin: point(g, s.a, 0.),
                direction: g.axis,
            }),
        });
    }
    let eu = |i, r| EdgeUse {
        edge: EdgeId(i),
        reversed: r,
    };
    m.loops.push(Loop {
        edges: (0..n).rev().map(|i| eu(i, true)).collect(),
    });
    m.loops.push(Loop {
        edges: (0..n).map(|i| eu(n + i, false)).collect(),
    });
    m.faces.push(Face {
        surface: Surface::Plane(Plane {
            origin: point(g, g.path.start, 0.),
            normal: g.axis.map(|v| -v),
        }),
        loops: vec![LoopId(0)],
    });
    m.faces.push(Face {
        surface: Surface::Plane(Plane {
            origin: point(g, g.path.start, g.height),
            normal: g.axis,
        }),
        loops: vec![LoopId(1)],
    });
    for (i, s) in seg.iter().enumerate() {
        m.loops.push(Loop {
            edges: vec![
                eu(i, false),
                eu(2 * n + (i + 1) % n, false),
                eu(n + i, true),
                eu(2 * n + i, true),
            ],
        });
        let surface = if let Some((c, r, _, sweep)) = s.arc {
            Surface::Cylinder(Cylinder {
                origin: point(g, c, 0.),
                axis: g.axis,
                x_direction: vector(g, sub2(s.a, c).map(|v| v / r)),
                radius: r,
                inward: sweep < 0.,
            })
        } else {
            let d = sub2(s.b, s.a);
            let length = len(d);
            Surface::Plane(Plane {
                origin: point(g, s.a, 0.),
                normal: vector(g, [d[1] / length, -d[0] / length]),
            })
        };
        m.faces.push(Face {
            surface,
            loops: vec![LoopId(i + 2)],
        });
    }
    // Full circular holes retain a genuine periodic seam and opposite cap uses.
    for h in &g.circular_holes {
        let v = m.vertices.len();
        let e = m.edges.len();
        let l = m.loops.len();
        for z in [0., g.height] {
            m.vertices.push(Vertex {
                position: point(g, [h.center[0] + h.radius, h.center[1]], z),
            });
            m.edges.push(Edge {
                vertices: [VertexId(m.vertices.len() - 1); 2],
                curve: Curve::Circle(Circle {
                    origin: point(g, h.center, z),
                    axis: g.axis,
                    x_direction: g.x_direction,
                    radius: h.radius,
                }),
            });
        }
        m.edges.push(Edge {
            vertices: [VertexId(v), VertexId(v + 1)],
            curve: Curve::Line(Line {
                origin: m.vertices[v].position,
                direction: g.axis,
            }),
        });
        m.loops.push(Loop {
            edges: vec![eu(e, false)],
        });
        m.loops.push(Loop {
            edges: vec![eu(e + 1, true)],
        });
        m.loops.push(Loop {
            edges: vec![
                eu(e, true),
                eu(e + 2, false),
                eu(e + 1, false),
                eu(e + 2, true),
            ],
        });
        m.faces[0].loops.push(LoopId(l));
        m.faces[1].loops.push(LoopId(l + 1));
        m.faces.push(Face {
            surface: Surface::Cylinder(Cylinder {
                origin: point(g, h.center, 0.),
                axis: g.axis,
                x_direction: g.x_direction,
                radius: h.radius,
                inward: true,
            }),
            loops: vec![LoopId(l + 2)],
        });
    }
    m.shells.push(Shell {
        faces: (0..m.faces.len()).map(FaceId).collect(),
    });
    m.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    Ok(m)
}
pub fn extrude_path(
    path: &PlanarPath,
    holes: &[CircleHole],
    height: f64,
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_path(path, holes, tol)?;
    if !height.is_finite() || height <= tol.linear {
        return Err(bad());
    }
    if path
        .segments
        .iter()
        .all(|s| matches!(s, PathSegment::LineTo { .. }))
    {
        let points: Vec<_> = std::iter::once(path.start)
            .chain(
                path.segments
                    .iter()
                    .take(path.segments.len() - 1)
                    .map(PathSegment::end),
            )
            .collect();
        let mut op = extrude_polygon_with_circular_holes(&points, &[], holes, height, tol)?;
        for i in 0..path.segments.len() {
            let faces = op.history.generated_faces[&format!("wall_{i}")].clone();
            op.history
                .generated_faces
                .insert(format!("segment_{i}"), faces);
        }
        return Ok(op);
    }
    let g = ArcPrismGeometry {
        path: path.clone(),
        circular_holes: holes.to_vec(),
        origin: [0.; 3],
        axis: [0., 0., 1.],
        x_direction: [1., 0., 0.],
        height,
    };
    g.properties(tol)?;
    let model = construct(&g, tol)?;
    let report = model.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let n = path.segments.len();
    let mut roles = BTreeMap::from([
        ("start_cap".into(), vec![FaceId(0)]),
        ("end_cap".into(), vec![FaceId(1)]),
        ("walls".into(), (2..model.faces.len()).map(FaceId).collect()),
    ]);
    for i in 0..n {
        roles.insert(format!("segment_{i}"), vec![FaceId(i + 2)]);
    }
    for i in 0..holes.len() {
        roles.insert(format!("circular_hole_{i}_wall"), vec![FaceId(n + 2 + i)]);
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: roles,
            input_faces: BTreeMap::new(),
        },
    })
}

// Fourier polynomial integration is analytic: powers of x/y and dx/dy have
// degree at most four. No quadrature or display samples enter native moments.
fn trig_integral(
    center: [f64; 2],
    r: f64,
    start: f64,
    sweep: f64,
    px: usize,
    py: usize,
    dy: bool,
) -> f64 {
    let [cx, cy] = center;
    let mut coefficients = [(0., 0.); 11];
    coefficients[5] = (1., 0.);
    let mut multiply = |constant: f64, cosine: f64, sine: f64| {
        let old = coefficients;
        coefficients = [(0., 0.); 11];
        for (i, (re, im)) in old.into_iter().enumerate() {
            coefficients[i].0 += constant * re;
            coefficients[i].1 += constant * im;
            if i + 1 < 11 {
                coefficients[i + 1].0 += (cosine * re + sine * im) / 2.;
                coefficients[i + 1].1 += (cosine * im - sine * re) / 2.;
            }
            if i > 0 {
                coefficients[i - 1].0 += (cosine * re - sine * im) / 2.;
                coefficients[i - 1].1 += (cosine * im + sine * re) / 2.;
            }
        }
    };
    for _ in 0..px {
        multiply(cx, r, 0.)
    }
    for _ in 0..py {
        multiply(cy, 0., r)
    }
    if dy {
        multiply(0., r, 0.)
    } else {
        multiply(0., 0., -r)
    }
    coefficients
        .iter()
        .enumerate()
        .map(|(i, (re, im))| {
            let k = i as f64 - 5.;
            if k == 0. {
                re * sweep
            } else {
                let scale = 2. * (k * sweep / 2.).sin() / k;
                let angle = k * (start + sweep / 2.);
                scale * (re * angle.cos() - im * angle.sin())
            }
        })
        .sum()
}
fn line_integral(a: [f64; 2], b: [f64; 2], px: usize, py: usize, dy: bool) -> f64 {
    let d = sub2(b, a);
    let mut c = [0.; 6];
    c[0] = 1.;
    let mut degree = 0;
    for (axis, power) in [(0, px), (1, py)] {
        for _ in 0..power {
            let old = c;
            c = [0.; 6];
            for i in 0..=degree {
                c[i] += old[i] * a[axis];
                c[i + 1] += old[i] * d[axis];
            }
            degree += 1;
        }
    }
    c.iter()
        .enumerate()
        .map(|(i, v)| v / (i + 1) as f64)
        .sum::<f64>()
        * d[usize::from(dy)]
}
fn integrals(
    seg: &[Segment],
    holes: &[CircleHole],
    reference: [f64; 2],
) -> Result<([f64; 6], [f64; 6]), KernelError> {
    let mut sums = [0.; 6];
    let mut corrections = [0.; 6];
    let mut absolute = [0.; 6];
    let mut add = |v: [f64; 6]| -> Result<(), KernelError> {
        for i in 0..6 {
            if !v[i].is_finite() {
                return Err(KernelError::NumericRange);
            }
            let adjusted = v[i] - corrections[i];
            let next = sums[i] + adjusted;
            corrections[i] = (next - sums[i]) - adjusted;
            sums[i] = next;
            absolute[i] += v[i].abs();
        }
        Ok(())
    };
    for s in seg {
        let boundary = |px, py, dy| {
            if let Some((center, r, start, sweep)) = s.arc {
                let c = sub2(center, reference);
                trig_integral(c, r, start, sweep, px, py, dy)
            } else {
                line_integral(sub2(s.a, reference), sub2(s.b, reference), px, py, dy)
            }
        };
        add([
            boundary(1, 0, true),
            boundary(2, 0, true) / 2.,
            -boundary(0, 2, false) / 2.,
            boundary(3, 0, true) / 3.,
            -boundary(0, 3, false) / 3.,
            boundary(2, 1, true) / 2.,
        ])?;
    }
    for h in holes {
        let a = std::f64::consts::PI * h.radius * h.radius;
        let c = sub2(h.center, reference);
        let q = h.radius * h.radius / 4.;
        add([
            -a,
            -a * c[0],
            -a * c[1],
            -a * (c[0] * c[0] + q),
            -a * (c[1] * c[1] + q),
            -a * c[0] * c[1],
        ])?;
    }
    if sums.iter().chain(absolute.iter()).any(|v| !v.is_finite())
        || sums[0] <= absolute[0] * 65536. * f64::EPSILON
    {
        return Err(KernelError::NumericRange);
    }
    Ok((sums, absolute))
}
impl ArcPrismGeometry {
    pub(crate) fn properties(&self, tol: Tolerance) -> Result<MassProperties, KernelError> {
        let seg = segments(&self.path, tol)?;
        let (first, first_absolute) = integrals(&seg, &self.circular_holes, self.path.start)?;
        for i in [1, 2] {
            let error = (first_absolute[i] + first[i].abs() * first_absolute[0] / first[0])
                * 4096.
                * f64::EPSILON
                / first[0];
            if !error.is_finite() || error > tol.linear / 16. {
                return Err(KernelError::NumericRange);
            }
        }
        let center = [
            self.path.start[0] + first[1] / first[0],
            self.path.start[1] + first[2] / first[0],
        ];
        let (centered, centered_absolute) = integrals(&seg, &self.circular_holes, center)?;
        if [3, 4]
            .iter()
            .any(|&i| centered[i] <= centered_absolute[i] * 65536. * f64::EPSILON)
        {
            return Err(KernelError::NumericRange);
        }
        let h = self.height;
        let volume = first[0] * h;
        let local = [
            [centered[4] * h + volume * h * h / 12., -centered[5] * h, 0.],
            [-centered[5] * h, centered[3] * h + volume * h * h / 12., 0.],
            [0., 0., (centered[3] + centered[4]) * h],
        ];
        if !volume.is_normal()
            || volume <= 0.
            || local.iter().flatten().any(|v| !v.is_finite())
            || (0..3).any(|i| local[i][i] <= 0.)
        {
            return Err(KernelError::NumericRange);
        }
        let basis = [
            self.x_direction,
            cross(self.axis, self.x_direction),
            self.axis,
        ];
        let tensor = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                (0..3)
                    .flat_map(|k| (0..3).map(move |l| basis[k][i] * local[k][l] * basis[l][j]))
                    .sum()
            })
        });
        let centroid = point(self, center, h / 2.);
        if !finite(centroid) {
            return Err(KernelError::NumericRange);
        }
        Ok(MassProperties {
            volume,
            centroid: Some(centroid),
            central_inertia: Some(tensor),
        })
    }
    pub(crate) fn cap_centroid(&self, id: FaceId, tol: Tolerance) -> Result<[f64; 3], KernelError> {
        let c = self.properties(tol)?.centroid.unwrap();
        let sign = if id.0 == 0 { -1. } else { 1. };
        Ok(std::array::from_fn(|i| {
            c[i] + sign * self.axis[i] * self.height / 2.
        }))
    }
}

pub(crate) fn recognize(
    m: &ModelSnapshot,
    tol: Tolerance,
) -> Result<ArcPrismGeometry, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if m.faces.len() < 4
        || m.faces.len() > MAX_POLYGON_POINTS + MAX_POLYGON_HOLES + 2
        || m.vertices.len() > 2 * (MAX_POLYGON_POINTS + MAX_POLYGON_HOLES)
        || m.edges.len() > 3 * (MAX_POLYGON_POINTS + MAX_POLYGON_HOLES)
        || m.loops.len() > MAX_POLYGON_POINTS + 3 * MAX_POLYGON_HOLES + 2
    {
        return Err(bad());
    }
    let top = m.faces[1].try_plane().ok_or_else(bad)?;
    let axis = top.normal;
    let first_arc = m
        .edges
        .iter()
        .find_map(|e| {
            if let Curve::CircularArc(a) = &e.curve {
                Some(a)
            } else {
                None
            }
        })
        .ok_or_else(bad)?;
    let origin = first_arc.circle.origin;
    let x = m
        .edges
        .iter()
        .find_map(|e| {
            if let Curve::Circle(c) = &e.curve {
                Some(c.x_direction)
            } else {
                None
            }
        })
        .unwrap_or(first_arc.circle.x_direction);
    let y = cross(axis, x);
    let unit = 4096. * f64::EPSILON;
    if (norm(axis) - 1.).abs() > unit || (norm(x) - 1.).abs() > unit || dot(axis, x).abs() > unit {
        return Err(bad());
    }
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(m.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => vec![l.origin[0], l.origin[1], l.origin[2]],
            Curve::Circle(c) => vec![c.radius, c.origin[0], c.origin[1], c.origin[2]],
            Curve::CircularArc(a) => vec![
                a.circle.radius,
                a.circle.origin[0],
                a.circle.origin[1],
                a.circle.origin[2],
            ],
        }))
        .map(f64::abs)
        .fold(0., f64::max);
    let error = scale * 8192. * f64::EPSILON;
    if !error.is_finite() || error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let lower = m.faces[0]
        .loops
        .first()
        .and_then(|id| m.loops.get(id.0))
        .ok_or_else(bad)?;
    let n = lower.edges.len();
    if !(2..=MAX_POLYGON_POINTS).contains(&n) {
        return Err(bad());
    }
    let project = |p: [f64; 3]| {
        let d = sub(p, origin);
        [dot(d, x), dot(d, y)]
    };
    let mut path = PlanarPath {
        start: project(m.vertices.first().ok_or_else(bad)?.position),
        segments: vec![],
    };
    for i in 0..n {
        let e = m.edges.get(i).ok_or_else(bad)?;
        let end = project(m.vertices.get(e.vertices[1].0).ok_or_else(bad)?.position);
        path.segments.push(match &e.curve {
            Curve::Line(_) => PathSegment::LineTo { end },
            Curve::CircularArc(a) => PathSegment::ArcTo {
                center: project(a.circle.origin),
                end,
                clockwise: a.sweep_radians < 0.,
            },
            _ => return Err(bad()),
        });
    }
    let height = dot(sub(top.origin, origin), axis);
    if !height.is_finite() || height <= tol.linear {
        return Err(bad());
    }
    let mut holes = vec![];
    for id in m.faces[0].loops.iter().skip(1) {
        let l = m.loops.get(id.0).ok_or_else(bad)?;
        if l.edges.len() != 1 {
            return Err(bad());
        }
        let e = m.edges.get(l.edges[0].edge.0).ok_or_else(bad)?;
        let Curve::Circle(c) = &e.curve else {
            return Err(bad());
        };
        holes.push(CircleHole {
            center: project(c.origin),
            radius: c.radius,
        });
    }
    validate_path(&path, &holes, tol)?;
    let g = ArcPrismGeometry {
        path,
        circular_holes: holes,
        origin,
        axis,
        x_direction: x,
        height,
    };
    let expected = construct(&g, tol)?;
    if m.vertices.len() != expected.vertices.len()
        || m.edges.len() != expected.edges.len()
        || m.loops != expected.loops
        || m.faces.len() != expected.faces.len()
        || m.shells != expected.shells
        || m.solids != expected.solids
    {
        return Err(bad());
    }
    let near = |a: [f64; 3], b: [f64; 3], e: f64| {
        a.into_iter()
            .zip(b)
            .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= e)
    };
    let min_length = expected
        .edges
        .iter()
        .filter(|e| e.vertices[0] != e.vertices[1])
        .map(|e| {
            norm(sub(
                expected.vertices[e.vertices[0].0].position,
                expected.vertices[e.vertices[1].0].position,
            ))
        })
        .fold(f64::INFINITY, f64::min);
    let circle = |a: &Circle, b: &Circle| {
        near(a.origin, b.origin, error)
            && near(a.axis, b.axis, unit)
            && near(a.x_direction, b.x_direction, unit + error / b.radius)
            && a.radius.is_finite()
            && (a.radius - b.radius).abs() <= error
    };
    for (a, b) in m.vertices.iter().zip(&expected.vertices) {
        if !near(a.position, b.position, error) {
            return Err(bad());
        }
    }
    for (a, b) in m.edges.iter().zip(&expected.edges) {
        if a.vertices != b.vertices {
            return Err(bad());
        }
        let valid = match (&a.curve, &b.curve) {
            (Curve::Line(a), Curve::Line(b)) => {
                near(a.origin, b.origin, error)
                    && near(a.direction, b.direction, unit + error / min_length)
            }
            (Curve::Circle(a), Curve::Circle(b)) => circle(a, b),
            (Curve::CircularArc(a), Curve::CircularArc(b)) => {
                circle(&a.circle, &b.circle)
                    && a.sweep_radians.is_finite()
                    && (a.sweep_radians - b.sweep_radians).abs() <= unit + error / b.circle.radius
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
                near(a.origin, b.origin, error)
                    && near(a.normal, b.normal, unit + error / min_length)
            }
            (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                near(a.origin, b.origin, error)
                    && near(a.axis, b.axis, unit)
                    && near(a.x_direction, b.x_direction, unit + error / b.radius)
                    && (a.radius - b.radius).abs() <= error
                    && a.inward == b.inward
            }
            _ => false,
        };
        if !valid {
            return Err(bad());
        }
    }
    g.properties(tol)?;
    Ok(g)
}

impl ModelSnapshot {
    pub fn arc_prism_geometry(
        &self,
        tol: Tolerance,
    ) -> Result<Option<ArcPrismGeometry>, KernelError> {
        if !tol.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if self
            .edges
            .iter()
            .any(|e| matches!(e.curve, Curve::CircularArc(_)))
        {
            recognize(self, tol)
                .map(Some)
                .or_else(|_| crate::bored_prism::recognize(self, tol).map(|_| None))
                .or_else(|_| crate::stepped_bore::recognize(self, tol).map(|_| None))
        } else {
            let report = self.validate(tol);
            if report.valid {
                Ok(None)
            } else {
                Err(KernelError::InvalidInput(report))
            }
        }
    }
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .map(f64::abs)
        .fold(0., f64::max);
    let tol = Tolerance {
        linear: scale * 1048576. * f64::EPSILON,
    };
    recognize(m, tol)?.properties(tol).map(|p| p.volume)
}

pub(crate) fn tessellate(
    m: &ModelSnapshot,
    g: &ArcPrismGeometry,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let n = g.path.segments.len();
    let mut sampled = BTreeMap::<EdgeId, Vec<[f64; 3]>>::new();
    let mut bound = 0f64;
    let mut segments = 0usize;
    let mut counts = vec![0usize; m.edges.len()];
    for (i, e) in m.edges.iter().enumerate() {
        let (c, sweep) = match &e.curve {
            Curve::CircularArc(a) => (&a.circle, a.sweep_radians),
            Curve::Circle(c) => (c, std::f64::consts::TAU),
            _ => continue,
        };
        let y = cross(c.axis, c.x_direction);
        let endpoint_error = [0., sweep]
            .into_iter()
            .enumerate()
            .map(|(index, angle)| {
                let (sin, cos) = angle.sin_cos();
                let exact = std::array::from_fn(|a| {
                    c.origin[a] + c.radius * (cos * c.x_direction[a] + sin * y[a])
                });
                norm(sub(m.vertices[e.vertices[index].0].position, exact))
            })
            .fold(0., f64::max);
        // Shared native endpoints can differ from evaluated supports by the
        // certified arithmetic envelope. Reserve that discrepancy as well
        // as chord sagitta, rather than silently spending the display budget.
        if policy.chordal_error.is_finite()
            && policy.chordal_error > 0.
            && endpoint_error >= policy.chordal_error
        {
            return Err(KernelError::NumericRange);
        }
        let bounded_policy = TessellationPolicy {
            chordal_error: policy.chordal_error - endpoint_error,
            ..policy
        };
        let (full, error) =
            crate::tessellation::circle_resolution(c.radius, g.height, c.origin, bounded_policy)?;
        bound = bound.max(error + endpoint_error);
        let count = ((full as f64 * sweep.abs() / std::f64::consts::TAU).ceil() as usize).max(1);
        counts[i] = count;
    }
    // Paired lower/upper trims share angular samples even when only one endpoint
    // spends more of the arithmetic budget.
    for i in 0..n + g.circular_holes.len() {
        let (a, b) = if i < n {
            (i, n + i)
        } else {
            let j = i - n;
            (3 * n + 3 * j, 3 * n + 3 * j + 1)
        };
        let count = counts[a].max(counts[b]);
        counts[a] = count;
        counts[b] = count;
    }
    for (i, e) in m.edges.iter().enumerate() {
        let (circle, sweep) = match &e.curve {
            Curve::CircularArc(a) => (Some(&a.circle), a.sweep_radians),
            Curve::Circle(c) => (Some(c), std::f64::consts::TAU),
            _ => (None, 0.),
        };
        let points = if let Some(c) = circle {
            let count = counts[i];
            segments = segments.checked_add(count).ok_or_else(bad)?;
            if segments > 4096 {
                return Err(KernelError::ResourceLimit(
                    "arc prism exceeds 2048 sampled cap points".into(),
                ));
            }
            let y = cross(c.axis, c.x_direction);
            let mut points: Vec<_> = (0..=count)
                .map(|k| {
                    let (s, co) = (sweep * k as f64 / count as f64).sin_cos();
                    std::array::from_fn(|a| {
                        c.origin[a] + c.radius * (co * c.x_direction[a] + s * y[a])
                    })
                })
                .collect();
            points[0] = m.vertices[e.vertices[0].0].position;
            points[count] = m.vertices[e.vertices[1].0].position;
            points
        } else {
            vec![
                m.vertices[e.vertices[0].0].position,
                m.vertices[e.vertices[1].0].position,
            ]
        };
        sampled.insert(EdgeId(i), points);
    }
    let mut patches = vec![];
    for i in 0..n + g.circular_holes.len() {
        let (lower, upper, face, inward) = if i < n {
            (i, n + i, i + 2, false)
        } else {
            let j = i - n;
            (3 * n + 3 * j, 3 * n + 3 * j + 1, n + 2 + j, true)
        };
        let bottom = &sampled[&EdgeId(lower)];
        let top = &sampled[&EdgeId(upper)];
        if bottom.len() != top.len() {
            return Err(bad());
        }
        for k in 0..bottom.len() - 1 {
            let j = k + 1;
            let points = if inward {
                vec![bottom[j], bottom[k], top[k], top[j]]
            } else {
                vec![bottom[k], bottom[j], top[j], top[k]]
            };
            let normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
            let length = norm(normal);
            if !length.is_normal() {
                return Err(KernelError::NumericRange);
            }
            patches.push(MeshPatch {
                face: FaceId(face),
                points,
                normal: normal.map(|v| v / length),
                boundary_edges: vec![Some(EdgeId(lower)), None, Some(EdgeId(upper)), None],
            });
        }
    }
    for id in 0..2 {
        let normal = m.faces[id].try_plane().unwrap().normal;
        let x = g.x_direction;
        let y = cross(normal, x);
        let mut rings = vec![];
        let mut world = vec![];
        let mut boundary = BTreeMap::new();
        for l in &m.faces[id].loops {
            let offset = world.len();
            let mut ids = vec![];
            for eu in &m.loops[l.0].edges {
                let samples = &sampled[&eu.edge];
                let count = samples.len() - 1;
                for k in 0..count {
                    world.push(samples[if eu.reversed { count - k } else { k }]);
                    ids.push(eu.edge);
                }
            }
            if world.len() > 2048 {
                return Err(KernelError::ResourceLimit(
                    "arc prism exceeds 2048 sampled cap points".into(),
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
        for t in crate::triangulation::triangulate_rings_2d(&rings)? {
            patches.push(MeshPatch {
                face: FaceId(id),
                points: t.map(|i| world[i]).to_vec(),
                normal,
                boundary_edges: (0..3)
                    .map(|k| boundary.get(&(t[k], t[(k + 1) % 3])).copied())
                    .collect(),
            });
        }
    }
    Ok(CylinderMesh {
        patches,
        segments: segments / 2,
        chordal_error_bound: bound,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capsule() -> PlanarPath {
        PlanarPath {
            start: [-10., -5.],
            segments: vec![
                PathSegment::LineTo { end: [10., -5.] },
                PathSegment::ArcTo {
                    center: [10., 0.],
                    end: [10., 5.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [-10., 5.] },
                PathSegment::ArcTo {
                    center: [-10., 0.],
                    end: [-10., -5.],
                    clockwise: false,
                },
            ],
        }
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9 * b.abs().max(1.), "{a} != {b}");
    }
    #[test]
    fn capsule_has_exact_moments_and_shared_display_boundaries() {
        let t = Tolerance::default();
        let op = extrude_path(&capsule(), &[], 8., t).unwrap();
        let m = op.model;
        let p = m.mass_properties(t).unwrap();
        let pi = std::f64::consts::PI;
        let area = 200. + 25. * pi;
        let volume = area * 8.;
        near(p.volume, volume);
        near(m.volume(), volume);
        near(p.centroid.unwrap()[0], 0.);
        near(p.centroid.unwrap()[1], 0.);
        near(p.centroid.unwrap()[2], 4.);
        // Rectangle plus the two outward semicircles, integrated about origin.
        let xx = 200. * 25. / 3. + 625. * pi / 4.;
        let yy = 200. * 100. / 3. + 2500. * pi + 8. * 10. * 125. / 3. + 625. * pi / 4.;
        near(
            p.central_inertia.unwrap()[0][0],
            8. * xx + volume * 64. / 12.,
        );
        near(
            p.central_inertia.unwrap()[1][1],
            8. * yy + volume * 64. / 12.,
        );
        near(p.central_inertia.unwrap()[2][2], 8. * (xx + yy));
        assert_eq!(
            m.bounds(),
            Bounds {
                min: [-15., -5., 0.],
                max: [15., 5., 8.]
            }
        );
        assert!(m.cylinder_geometry(t).unwrap().is_none());
        assert!(m.profile_prism_geometry(t).unwrap().is_none());
        assert!(m.revolve_geometry(t).unwrap().is_none());
        let mesh = tessellate_analytic(&m, t, TessellationPolicy::default()).unwrap();
        let mut edges = BTreeMap::<([u64; 3], [u64; 3]), i32>::new();
        for patch in &mesh.patches {
            for i in 0..patch.points.len() {
                let a = patch.points[i].map(|v| if v == 0. { 0 } else { v.to_bits() });
                let b = patch.points[(i + 1) % patch.points.len()].map(|v| {
                    if v == 0. {
                        0
                    } else {
                        v.to_bits()
                    }
                });
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                *edges.entry(key).or_default() += sign;
            }
        }
        assert!(edges.values().all(|v| *v == 0));
        let hole = CircleHole {
            center: [1., 0.],
            radius: 2.,
        };
        let m = extrude_path(&capsule(), &[hole], 8., t).unwrap().model;
        let p = m.mass_properties(t).unwrap();
        near(p.volume, volume - 32. * pi);
        near(p.centroid.unwrap()[0], -4. * pi / (area - 4. * pi));
        let cap = m.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], t).unwrap();
        near(cap.origin[0], p.centroid.unwrap()[0]);
        near(cap.origin[2], 8.);
        assert!(tessellate_analytic(&m, t, TessellationPolicy::default()).is_ok());
    }
}

#[cfg(test)]
mod acceptance {
    use super::*;
    fn d_shape() -> PlanarPath {
        PlanarPath {
            start: [0., -5.],
            segments: vec![
                PathSegment::ArcTo {
                    center: [0., 0.],
                    end: [0., 5.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [0., -5.] },
            ],
        }
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9 * b.abs().max(1.), "{a} != {b}");
    }
    #[test]
    fn semicircle_has_independent_centroid_tensor_and_rotated_bounds() {
        let t = Tolerance::default();
        let m = extrude_path(&d_shape(), &[], 8., t).unwrap().model;
        let p = m.mass_properties(t).unwrap();
        let pi = std::f64::consts::PI;
        let v = 100. * pi;
        let cx = 20. / (3. * pi);
        near(p.volume, v);
        near(p.centroid.unwrap()[0], cx);
        near(p.centroid.unwrap()[1], 0.);
        near(p.centroid.unwrap()[2], 4.);
        let inertia = [
            v * (25. / 4. + 64. / 12.),
            v * (25. / 4. - cx * cx + 64. / 12.),
            v * (25. / 2. - cx * cx),
        ];
        for (i, value) in inertia.iter().enumerate() {
            near(p.central_inertia.unwrap()[i][i], *value)
        }
        assert_eq!(
            m.bounds(),
            Bounds {
                min: [0., -5., 0.],
                max: [5., 5., 8.]
            }
        );
        for angle in [0.2_f64, 0.7, 1.1] {
            let q = [0., 0., (angle / 2.).sin(), (angle / 2.).cos()];
            let moved = transform(&m, [12., -7., 4.], q, t).unwrap().model;
            let mp = moved.mass_properties(t).unwrap();
            near(mp.volume, v);
            near(mp.centroid.unwrap()[0], 12. + cx * angle.cos());
            near(mp.centroid.unwrap()[1], -7. + cx * angle.sin());
            let tensor = mp.central_inertia.unwrap();
            near(
                tensor[0][0],
                inertia[0] * angle.cos().powi(2) + inertia[1] * angle.sin().powi(2),
            );
            near(
                tensor[0][1],
                (inertia[0] - inertia[1]) * angle.cos() * angle.sin(),
            );
            let bounds = moved.bounds();
            near(bounds.min[0], 12. - 5. * angle.sin());
            near(bounds.max[0], 17.);
            near(bounds.min[1], -7. - 5. * angle.cos());
            near(bounds.max[1], -2.);
            assert!(tessellate_analytic(&moved, t, TessellationPolicy::default()).is_ok());
        }
    }
    #[test]
    fn path_rejects_unsupported_and_corrupted_geometry() {
        let t = Tolerance::default();
        let p = d_shape();
        let mut clockwise = p.clone();
        if let PathSegment::ArcTo { clockwise, .. } = &mut clockwise.segments[0] {
            *clockwise = true;
        }
        let mut unequal = p.clone();
        if let PathSegment::ArcTo { end, .. } = &mut unequal.segments[0] {
            end[1] += 0.001;
        }
        let mut open = p.clone();
        if let PathSegment::LineTo { end } = &mut open.segments[1] {
            end[0] = 1.;
        }
        let mut major = p.clone();
        if let PathSegment::ArcTo { end, .. } = &mut major.segments[0] {
            *end = [-5., 0.];
        }
        for invalid in [clockwise, unequal, open, major] {
            assert!(extrude_path(&invalid, &[], 8., t).is_err());
        }
        for hole in [
            CircleHole {
                center: [0., 0.],
                radius: 1.,
            },
            CircleHole {
                center: [4., 0.],
                radius: 1.,
            },
            CircleHole {
                center: [2., 0.],
                radius: 3.,
            },
        ] {
            assert!(extrude_path(&p, &[hole], 8., t).is_err());
        }
        let m = extrude_path(&p, &[], 8., t).unwrap().model;
        for i in 0..8 {
            let mut bad = m.clone();
            match i {
                0 => bad.vertices[0].position[0] += 0.001,
                1 => {
                    if let Curve::CircularArc(a) = &mut bad.edges[0].curve {
                        a.sweep_radians += 0.01;
                    }
                }
                2 => {
                    if let Curve::CircularArc(a) = &mut bad.edges[0].curve {
                        a.circle.radius += 0.01;
                    }
                }
                3 => bad.edges[0].vertices.swap(0, 1),
                4 => bad.loops[0].edges[0].reversed ^= true,
                5 => {
                    if let Surface::Cylinder(c) = &mut bad.faces[2].surface {
                        c.inward = true;
                    }
                }
                6 => bad.solids.push(bad.solids[0].clone()),
                _ => bad.faces[0].loops.push(LoopId(0)),
            }
            assert!(!bad.validate(t).valid, "mutation{i}");
            assert!(bad.mass_properties(t).is_err());
        }
    }
    #[test]
    fn scale_budget_and_display_clearance_do_not_change_native_geometry() {
        for scale in [1e-6, 1., 1e3] {
            let mut p = d_shape();
            p.start = p.start.map(|v| v * scale);
            for segment in &mut p.segments {
                match segment {
                    PathSegment::LineTo { end } => *end = end.map(|v| v * scale),
                    PathSegment::ArcTo { center, end, .. } => {
                        *center = center.map(|v| v * scale);
                        *end = end.map(|v| v * scale);
                    }
                }
            }
            let t = Tolerance {
                linear: scale * 1e-8,
            };
            let m = extrude_path(&p, &[], 8. * scale, t).unwrap().model;
            near(
                m.mass_properties(t).unwrap().volume / scale.powi(3),
                100. * std::f64::consts::PI,
            );
            assert!(tessellate_analytic(
                &m,
                t,
                TessellationPolicy {
                    chordal_error: 0.01 * scale,
                    max_segments: 4096
                }
            )
            .is_ok());
        }
        let t = Tolerance::default();
        let angle = std::f64::consts::PI / 12.;
        let m = extrude_path(
            &d_shape(),
            &[CircleHole {
                center: [4. * angle.cos(), 4. * angle.sin()],
                radius: 0.99,
            }],
            8.,
            t,
        )
        .unwrap()
        .model;
        let before = m.clone();
        let volume = m.mass_properties(t).unwrap().volume;
        assert!(tessellate_analytic(
            &m,
            t,
            TessellationPolicy {
                chordal_error: 1.,
                max_segments: 12
            }
        )
        .is_err());
        assert!(tessellate_analytic(
            &m,
            t,
            TessellationPolicy {
                chordal_error: 0.001,
                max_segments: 4096
            }
        )
        .is_ok());
        assert_eq!(m, before);
        near(m.mass_properties(t).unwrap().volume, volume);
        let mut unresolved = d_shape();
        unresolved.start[0] += 1e12;
        for s in &mut unresolved.segments {
            match s {
                PathSegment::LineTo { end } => end[0] += 1e12,
                PathSegment::ArcTo { center, end, .. } => {
                    center[0] += 1e12;
                    end[0] += 1e12;
                }
            }
        }
        assert!(validate_path(&unresolved, &[], t).is_err());
    }

    #[test]
    fn display_budget_includes_accepted_endpoint_support_roundoff() {
        let t = Tolerance::default();
        let scale = 2e-5;
        let mut p = d_shape();
        p.start = p.start.map(|v| v * scale);
        for segment in &mut p.segments {
            match segment {
                PathSegment::LineTo { end } => *end = end.map(|v| v * scale),
                PathSegment::ArcTo { center, end, .. } => {
                    *center = center.map(|v| v * scale);
                    *end = end.map(|v| v * scale);
                }
            }
        }
        let m = extrude_path(&p, &[], 8. * scale, t).unwrap().model;
        let mut moved = translate(&m, [1000., 0., 0.], t).unwrap().model;
        moved.vertices[2].position[1] -= 1.5e-9;
        assert!(moved.validate(t).valid);
        assert!(matches!(
            tessellate_analytic(
                &moved,
                t,
                TessellationPolicy {
                    chordal_error: 1e-9,
                    max_segments: 4096
                }
            ),
            Err(KernelError::NumericRange)
        ));
        let mesh = tessellate_analytic(
            &moved,
            t,
            TessellationPolicy {
                chordal_error: 1e-8,
                max_segments: 4096,
            },
        )
        .unwrap();
        assert!(mesh.chordal_error_bound >= 1.5e-9 && mesh.chordal_error_bound <= 1e-8);
    }

    #[test]
    fn line_only_path_uses_native_polygon_and_segment_roles() {
        let p = PlanarPath {
            start: [0., 0.],
            segments: vec![
                PathSegment::LineTo { end: [10., 0.] },
                PathSegment::LineTo { end: [10., 8.] },
                PathSegment::LineTo { end: [0., 8.] },
                PathSegment::LineTo { end: [0., 0.] },
            ],
        };
        let op = extrude_path(&p, &[], 3., Tolerance::default()).unwrap();
        near(op.model.volume(), 240.);
        assert_eq!(op.history.generated_faces["segment_2"].len(), 1);
    }
}
