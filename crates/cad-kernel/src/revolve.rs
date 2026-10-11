//! Exact full-turn solids from bounded piecewise-linear meridians.
use crate::math::{dot, norm, sub};
use crate::*;

#[derive(Debug, Clone, PartialEq)]
pub struct RevolveSegment {
    pub face: FaceId,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub start_circle: Option<EdgeId>,
    pub end_circle: Option<EdgeId>,
    pub seam: Option<EdgeId>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RevolveGeometry {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub profile: Vec<[f64; 2]>,
    pub segments: Vec<RevolveSegment>,
}
fn unsupported() -> KernelError {
    KernelError::Unsupported(
        "expected a canonical full-turn piecewise-linear meridian solid".into(),
    )
}
pub fn validate_revolve_profile(points: &[[f64; 2]], tol: Tolerance) -> Result<(), KernelError> {
    crate::simple_polygon::validate_ring(points, tol)?;
    let mut axis_edges = 0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if a[0] < 0. || (a[0] > 0. && a[0] <= tol.linear) {
            return Err(unsupported());
        }
        if a[0] != b[0]
            && a[1] != b[1]
            && (a[0] == 0.
                || b[0] == 0.
                || (b[0] - a[0]).abs() <= tol.linear
                || (b[1] - a[1]).abs() <= tol.linear)
        {
            return Err(KernelError::Unsupported("conical edges require positive endpoint radii and radial/axial spans above tolerance; apex singularities unsupported".into()));
        }
        if a[0] != b[0] && a[1] != b[1] {
            let slope = (b[0] - a[0]) / (b[1] - a[1]);
            let angle = slope.atan();
            if !slope.is_normal()
                || angle == 0.
                || angle.abs() >= std::f64::consts::FRAC_PI_2
                || (angle.tan() - slope).abs() * (b[1] - a[1]).abs() > tol.linear / 16.
            {
                return Err(KernelError::NumericRange);
            }
        }
        if a[0] == 0. && b[0] == 0. {
            axis_edges += 1;
        }
    }
    // A single axis interval is a regular disk-ended solid. Multiple disjoint
    // contacts would create touching components or a noncanonical axial cavity.
    if axis_edges > 1 || points.iter().filter(|p| p[0] == 0.).count() != axis_edges * 2 {
        return Err(unsupported());
    }
    Ok(())
}
fn world(g: &RevolveGeometry, r: f64, z: f64) -> [f64; 3] {
    std::array::from_fn(|i| g.origin[i] + g.axis[i] * z + g.x_direction[i] * r)
}
fn construct(mut g: RevolveGeometry) -> (ModelSnapshot, RevolveGeometry) {
    let mut m = ModelSnapshot {
        vertices: vec![],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    let mut circles = vec![None; g.profile.len()];
    for (i, &p) in g.profile.iter().enumerate() {
        if p[0] == 0. {
            continue;
        }
        let v = VertexId(m.vertices.len());
        m.vertices.push(Vertex {
            position: world(&g, p[0], p[1]),
        });
        circles[i] = Some(EdgeId(m.edges.len()));
        m.edges.push(Edge {
            vertices: [v, v],
            curve: Curve::Circle(Circle {
                origin: world(&g, 0., p[1]),
                axis: g.axis,
                x_direction: g.x_direction,
                radius: p[0],
            }),
        });
    }
    let eu = |edge, reversed| EdgeUse { edge, reversed };
    for i in 0..g.profile.len() {
        let j = (i + 1) % g.profile.len();
        let a = g.profile[i];
        let b = g.profile[j];
        if a[0] == 0. && b[0] == 0. {
            continue;
        }
        let mut loops = vec![];
        let mut seam = None;
        let surface = if a[1] == b[1] {
            let positive = a[0] > b[0];
            let mut boundaries = vec![];
            if let Some(e) = circles[i] {
                boundaries.push((a[0], e, false));
            }
            if let Some(e) = circles[j] {
                boundaries.push((b[0], e, true));
            }
            boundaries.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (_, edge, reversed) in boundaries {
                loops.push(LoopId(m.loops.len()));
                m.loops.push(Loop {
                    edges: vec![eu(edge, reversed)],
                });
            }
            Surface::Plane(Plane {
                origin: world(&g, 0., a[1]),
                normal: g.axis.map(|v| if positive { v } else { -v }),
            })
        } else {
            let ca = circles[i].unwrap();
            let cb = circles[j].unwrap();
            let s = EdgeId(m.edges.len());
            seam = Some(s);
            let down = b[1] < a[1];
            let (lo, hi) = if down { (cb, ca) } else { (ca, cb) };
            let lower = if down { b } else { a };
            let slope = (b[0] - a[0]) / (b[1] - a[1]);
            let length = slope.hypot(1.);
            m.edges.push(Edge {
                vertices: [m.edges[lo.0].vertices[0], m.edges[hi.0].vertices[0]],
                curve: Curve::Line(Line {
                    origin: world(&g, lower[0], lower[1]),
                    direction: std::array::from_fn(|i| {
                        (g.axis[i] + g.x_direction[i] * slope) / length
                    }),
                }),
            });
            loops.push(LoopId(m.loops.len()));
            m.loops.push(Loop {
                edges: vec![eu(ca, false), eu(s, down), eu(cb, true), eu(s, !down)],
            });
            if a[0] != b[0] {
                Surface::Cone(Cone {
                    origin: world(&g, 0., lower[1]),
                    axis: g.axis,
                    x_direction: g.x_direction,
                    radius: lower[0],
                    slope,
                    inward: down,
                })
            } else {
                Surface::Cylinder(Cylinder {
                    origin: world(&g, 0., a[1].min(b[1])),
                    axis: g.axis,
                    x_direction: g.x_direction,
                    radius: a[0],
                    inward: b[1] < a[1],
                })
            }
        };
        let face = FaceId(m.faces.len());
        m.faces.push(Face { surface, loops });
        g.segments.push(RevolveSegment {
            face,
            start: a,
            end: b,
            start_circle: circles[i],
            end_circle: circles[j],
            seam,
        });
    }
    m.shells.push(Shell {
        faces: (0..m.faces.len()).map(FaceId).collect(),
    });
    m.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    (m, g)
}
pub fn revolve_polygon(
    points: &[[f64; 2]],
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_revolve_profile(points, tol)?;
    let (model, g) = construct(RevolveGeometry {
        origin: [0.; 3],
        axis: [0., 0., 1.],
        x_direction: [1., 0., 0.],
        profile: points.to_vec(),
        segments: vec![],
    });
    g.properties()?;
    let report = model.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let mut generated_faces: BTreeMap<String, Vec<FaceId>> = BTreeMap::new();
    let mut k = 0;
    let min = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let max = points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if a[0] == 0. && b[0] == 0. {
            continue;
        }
        let f = FaceId(k);
        k += 1;
        generated_faces.insert(format!("segment_{i}"), vec![f]);
        let mut roles = vec![if a[1] == b[1] {
            "planar_faces"
        } else {
            if a[0] == b[0] {
                "cylindrical_faces"
            } else {
                "conical_faces"
            }
        }];
        if a[1] != b[1] {
            roles.push(if b[1] > a[1] {
                "outer_walls"
            } else {
                "inner_walls"
            });
        } else if a[1] == min {
            roles.push("start_cap");
        } else if a[1] == max {
            roles.push("end_cap");
        }
        for role in roles {
            generated_faces.entry(role.into()).or_default().push(f);
        }
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces,
            input_faces: BTreeMap::new(),
        },
    })
}

pub(crate) fn recognize(m: &ModelSnapshot, tol: Tolerance) -> Result<RevolveGeometry, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if m.faces.len() > MAX_POLYGON_POINTS
        || m.edges.len() > MAX_POLYGON_POINTS * 2
        || m.vertices.len() > MAX_POLYGON_POINTS
        || m.loops.len() > MAX_POLYGON_POINTS * 2
    {
        return Err(unsupported());
    }
    let c = m
        .edges
        .iter()
        .find_map(|e| {
            if let Curve::Circle(c) = &e.curve {
                Some(c)
            } else {
                None
            }
        })
        .ok_or_else(unsupported)?;
    let mut g = RevolveGeometry {
        origin: c.origin,
        axis: c.axis,
        x_direction: c.x_direction,
        profile: vec![],
        segments: vec![],
    };
    if (norm(g.axis) - 1.).abs() > 64. * f64::EPSILON
        || (norm(g.x_direction) - 1.).abs() > 64. * f64::EPSILON
        || dot(g.axis, g.x_direction).abs() > 64. * f64::EPSILON
    {
        return Err(unsupported());
    }
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(m.edges.iter().filter_map(|e| {
            if let Curve::Circle(c) = &e.curve {
                Some(c.radius)
            } else {
                None
            }
        }))
        .map(f64::abs)
        .fold(0., f64::max);
    let error = scale * 2048. * f64::EPSILON;
    if !error.is_finite() || error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let get_circle = |id: EdgeId| -> Result<&Circle, KernelError> {
        match m.edges.get(id.0).map(|e| &e.curve) {
            Some(Curve::Circle(c)) => Ok(c),
            _ => Err(unsupported()),
        }
    };
    let mut pairs = vec![];
    for f in &m.faces {
        let (a, b) = match &f.surface {
            Surface::Plane(p) => {
                if !(1..=2).contains(&f.loops.len()) {
                    return Err(unsupported());
                }
                let mut radii = vec![];
                for l in &f.loops {
                    let l = m.loops.get(l.0).ok_or_else(unsupported)?;
                    if l.edges.len() != 1 {
                        return Err(unsupported());
                    }
                    radii.push(get_circle(l.edges[0].edge)?.radius);
                }
                if radii.len() == 1 {
                    radii.push(0.);
                }
                let z = dot(sub(p.origin, g.origin), g.axis);
                if dot(p.normal, g.axis) > 0. {
                    ([radii[0], z], [radii[1], z])
                } else {
                    ([radii[1], z], [radii[0], z])
                }
            }
            Surface::Cylinder(_) | Surface::Cone(_) => {
                if f.loops.len() != 1 {
                    return Err(unsupported());
                }
                let l = m.loops.get(f.loops[0].0).ok_or_else(unsupported)?;
                if l.edges.len() != 4 {
                    return Err(unsupported());
                }
                let ca = get_circle(l.edges[0].edge)?;
                let cb = get_circle(l.edges[2].edge)?;
                (
                    [ca.radius, dot(sub(ca.origin, g.origin), g.axis)],
                    [cb.radius, dot(sub(cb.origin, g.origin), g.axis)],
                )
            }
        };
        pairs.push((a, b));
    }
    if pairs.len() < 3 {
        return Err(unsupported());
    }
    // Recover shared axial levels from circle centers. Roundoff from projecting
    // the same supporting plane is reconciled only within the arithmetic bound.
    let mut levels: Vec<f64> = vec![];
    for (a, b) in &mut pairs {
        for p in [a, b] {
            if let Some(z) = levels.iter().find(|z| (**z - p[1]).abs() <= error) {
                p[1] = *z;
            } else {
                levels.push(p[1]);
            }
        }
    }
    for i in 0..pairs.len() {
        let (a, b) = pairs[i];
        g.profile.push(a);
        let next = pairs[(i + 1) % pairs.len()].0;
        if b != next {
            if b[0] != 0. || next[0] != 0. {
                return Err(unsupported());
            }
            g.profile.push(b);
        }
    }
    validate_revolve_profile(&g.profile, tol)?;
    let (expected, g) = construct(g);
    if m.vertices.len() != expected.vertices.len()
        || m.edges.len() != expected.edges.len()
        || m.loops != expected.loops
        || m.shells != expected.shells
        || m.solids != expected.solids
        || m.faces.len() != expected.faces.len()
    {
        return Err(unsupported());
    }
    let near = |a: [f64; 3], b: [f64; 3], e: f64| {
        a.into_iter()
            .zip(b)
            .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= e)
    };
    let unit = 128. * f64::EPSILON;
    for (a, b) in m.vertices.iter().zip(&expected.vertices) {
        if !near(a.position, b.position, error) {
            return Err(unsupported());
        }
    }
    for (a, b) in m.edges.iter().zip(&expected.edges) {
        if a.vertices != b.vertices {
            return Err(unsupported());
        }
        let b_edge_vertices = b.vertices;
        let valid = match (&a.curve, &b.curve) {
            (Curve::Circle(a), Curve::Circle(b)) => {
                near(a.origin, b.origin, error)
                    && near(a.axis, b.axis, unit)
                    && near(a.x_direction, b.x_direction, unit)
                    && a.radius == b.radius
            }
            (Curve::Line(a), Curve::Line(b)) => {
                let edge_length = norm(sub(
                    expected.vertices[b_edge_vertices[1].0].position,
                    expected.vertices[b_edge_vertices[0].0].position,
                ));
                near(a.origin, b.origin, error)
                    && (norm(a.direction) - 1.).abs() <= unit
                    && near(a.direction, b.direction, unit + 2. * error / edge_length)
            }
            _ => false,
        };
        if !valid {
            return Err(unsupported());
        }
    }
    for (index, (a, b)) in m.faces.iter().zip(&expected.faces).enumerate() {
        if a.loops != b.loops {
            return Err(unsupported());
        }
        let valid = match (&a.surface, &b.surface) {
            (Surface::Plane(a), Surface::Plane(b)) => {
                near(a.origin, b.origin, error) && near(a.normal, b.normal, unit)
            }
            (Surface::Cone(a), Surface::Cone(b)) => {
                near(a.origin, b.origin, error)
                    && near(a.axis, b.axis, unit)
                    && near(a.x_direction, b.x_direction, unit)
                    && a.radius == b.radius
                    && a.inward == b.inward
                    && a.slope.is_finite()
                    && b.slope.is_finite()
                    && (a.slope - b.slope).abs() / a.slope.hypot(1.)
                        <= 4096. * f64::EPSILON * b.slope.abs() / a.slope.hypot(1.)
                            + 2. * error
                                / (g.segments[index].end[1] - g.segments[index].start[1]).abs()
            }
            (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                near(a.origin, b.origin, error)
                    && near(a.axis, b.axis, unit)
                    && near(a.x_direction, b.x_direction, unit)
                    && a.radius == b.radius
                    && a.inward == b.inward
            }
            _ => false,
        };
        if !valid {
            return Err(unsupported());
        }
    }
    g.properties()?;
    Ok(g)
}
#[derive(Default)]
struct Sum {
    value: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, value: f64) -> Result<(), KernelError> {
        if !value.is_finite() || (value != 0. && !value.is_normal()) {
            return Err(KernelError::NumericRange);
        }
        let adjusted = value - self.correction;
        let next = self.value + adjusted;
        self.correction = (next - self.value) - adjusted;
        self.value = next;
        if !next.is_finite() {
            return Err(KernelError::NumericRange);
        }
        Ok(())
    }
}
impl RevolveGeometry {
    pub(crate) fn properties(&self) -> Result<MassProperties, KernelError> {
        let mut levels: Vec<f64> = self.profile.iter().map(|p| p[1]).collect();
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        let mut slices = vec![];
        let reference = *levels.first().ok_or_else(unsupported)?;
        let mut volume_sum = Sum::default();
        let mut first = Sum::default();
        for w in levels.windows(2) {
            let mid = w[0] / 2. + w[1] / 2.;
            let h = w[1] - w[0];
            let mut radial_segments = vec![];
            for i in 0..self.profile.len() {
                let a = self.profile[i];
                let b = self.profile[(i + 1) % self.profile.len()];
                if mid > a[1].min(b[1]) && mid < a[1].max(b[1]) {
                    let (lo, hi) = if a[1] < b[1] { (a, b) } else { (b, a) };
                    let radius = |z: f64| {
                        if z == lo[1] {
                            lo[0]
                        } else if z == hi[1] {
                            hi[0]
                        } else {
                            let t = (z - lo[1]) / (hi[1] - lo[1]);
                            lo[0] * (1. - t) + hi[0] * t
                        }
                    };
                    radial_segments.push([radius(w[0]), radius(w[1])]);
                }
            }
            radial_segments
                .sort_by(|a, b| (a[0] / 2. + a[1] / 2.).total_cmp(&(b[0] / 2. + b[1] / 2.)));
            if radial_segments.len() % 2 != 0 {
                return Err(unsupported());
            }
            for pair in radial_segments.as_chunks::<2>().0 {
                let [inner, outer] = *pair;
                let d = [outer[0] - inner[0], outer[1] - inner[1]];
                let s = [outer[0] + inner[0], outer[1] + inner[1]];
                if d.iter().any(|x| *x < 0.) {
                    return Err(unsupported());
                }
                // Positive Bernstein coefficients of outer²-inner² and
                // outer²+inner² retain thin annular walls without cancellation.
                let area = [d[0] * s[0], (d[0] * s[1] + d[1] * s[0]) / 2., d[1] * s[1]];
                let squared = [
                    outer[0] * outer[0] + inner[0] * inner[0],
                    outer[0] * outer[1] + inner[0] * inner[1],
                    outer[1] * outer[1] + inner[1] * inner[1],
                ];
                let a0 = area.iter().sum::<f64>() / 3.;
                let a1 = area
                    .iter()
                    .enumerate()
                    .map(|(i, a)| a * (i + 1) as f64 / 12.)
                    .sum::<f64>();
                let a2 = area
                    .iter()
                    .enumerate()
                    .map(|(i, a)| a * ((i + 1) * (i + 2)) as f64 / 60.)
                    .sum::<f64>();
                let v = std::f64::consts::PI * h * a0;
                let mean = a1 / a0;
                let variance = a2 / a0 - mean * mean;
                if !v.is_normal() || v <= 0. || !variance.is_normal() || variance <= 0. {
                    return Err(KernelError::NumericRange);
                }
                let mut fourth = Sum::default();
                let binomial2 = [1., 2., 1.];
                let binomial4 = [1., 4., 6., 4., 1.];
                for i in 0..3 {
                    for j in 0..3 {
                        fourth.add(
                            area[i] * squared[j] * binomial2[i] * binomial2[j]
                                / binomial4[i + j]
                                / 5.,
                        )?;
                    }
                }
                let axial = std::f64::consts::PI * h * fourth.value / 2.;
                let center = w[0] + h * mean;
                volume_sum.add(v)?;
                first.add(v * (center - reference))?;
                slices.push((v, center, h * h * variance, axial));
            }
        }
        let volume = volume_sum.value;
        let z = reference + first.value / volume;
        let mut axial_sum = Sum::default();
        let mut transverse_sum = Sum::default();
        for (v, center, variance, axial) in slices {
            axial_sum.add(axial)?;
            transverse_sum.add(axial / 2. + v * (variance + (center - z) * (center - z)))?;
        }
        let axial = axial_sum.value;
        let transverse = transverse_sum.value;
        if [volume, axial, transverse]
            .iter()
            .any(|v| !v.is_normal() || *v <= 0.)
        {
            return Err(KernelError::NumericRange);
        }
        let centroid = world(self, 0., z);
        let tensor = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                let diagonal = if i == j { transverse } else { 0. };
                diagonal + (axial - transverse) * self.axis[i] * self.axis[j]
            })
        });
        if centroid
            .iter()
            .chain(tensor.iter().flatten())
            .any(|v| !v.is_finite() || (*v != 0. && !v.is_normal()))
        {
            return Err(KernelError::NumericRange);
        }
        Ok(MassProperties {
            volume,
            centroid: Some(centroid),
            central_inertia: Some(tensor),
        })
    }
}
pub(crate) fn volume(m: &ModelSnapshot) -> Result<f64, KernelError> {
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(m.edges.iter().flat_map(|edge| match &edge.curve {
            Curve::Circle(circle) => [
                circle.origin[0],
                circle.origin[1],
                circle.origin[2],
                circle.radius,
            ],
            Curve::Line(line) => [line.origin[0], line.origin[1], line.origin[2], 0.],
        }))
        .map(f64::abs)
        .fold(0., f64::max);
    recognize(
        m,
        Tolerance {
            linear: scale * 65536. * f64::EPSILON,
        },
    )?
    .properties()
    .map(|p| p.volume)
}
impl ModelSnapshot {
    pub fn revolve_geometry(&self, tol: Tolerance) -> Result<Option<RevolveGeometry>, KernelError> {
        if !tol.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if self.has_curved_geometry() {
            if let Ok(g) = recognize(self, tol) {
                return Ok(Some(g));
            }
        }
        let r = self.validate(tol);
        if r.valid {
            Ok(None)
        } else {
            Err(KernelError::InvalidInput(r))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shaft() -> Vec<[f64; 2]> {
        vec![
            [0., 0.],
            [5., 0.],
            [5., 10.],
            [3., 10.],
            [3., 20.],
            [0., 20.],
        ]
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() <= 1e-10 * b.abs().max(1.), "{a} != {b}");
    }
    #[test]
    fn stepped_shaft_properties_and_planar_datums() {
        let t = Tolerance::default();
        let op = revolve_polygon(&shaft(), t).unwrap();
        let m = &op.model;
        let p = m.mass_properties(t).unwrap();
        let pi = std::f64::consts::PI;
        near(p.volume, 340. * pi);
        near(m.volume(), 340. * pi);
        let z = 2600. / 340.;
        near(p.centroid.unwrap()[2], z);
        let axial = (250. * 25. / 2. + 90. * 9. / 2.) * pi;
        let transverse = (250. * (25. / 4. + 100. / 12. + (5. - z) * (5. - z))
            + 90. * (9. / 4. + 100. / 12. + (15. - z) * (15. - z)))
            * pi;
        let tensor = p.central_inertia.unwrap();
        near(tensor[0][0], transverse);
        near(tensor[2][2], axial);
        assert_eq!(m.faces.len(), 5);
        assert_eq!(m.edges.len(), 6);
        assert_eq!(m.vertices.len(), 4);
        let f = op.history.generated_faces["segment_2"][0];
        let frame = m.face_frame(f, [1., 0., 0.], [0.; 3], t).unwrap();
        assert_eq!(frame.origin, [0., 0., 10.]);
        assert_eq!(frame.z_axis, [0., 0., 1.]);
        assert_eq!(
            m.bounds(),
            Bounds {
                min: [-5., -5., 0.],
                max: [5., 5., 20.]
            }
        );
    }
    #[test]
    fn blind_counterbore_and_rotated_full_tensor() {
        // One solid outer cylinder minus two coaxial depth ranges.
        let points = [
            [0., 0.],
            [10., 0.],
            [10., 30.],
            [6., 30.],
            [6., 20.],
            [3., 20.],
            [3., 5.],
            [0., 5.],
        ];
        let t = Tolerance::default();
        let m = revolve_polygon(&points, t).unwrap().model;
        let p = m.mass_properties(t).unwrap();
        let pi = std::f64::consts::PI;
        let v = (3000. - 360. - 135.) * pi;
        let z = (3000. * 15. - 360. * 25. - 135. * 12.5) / 2505.;
        near(p.volume, v);
        near(p.centroid.unwrap()[2], z);
        let axial = (3000. * 100. / 2. - 360. * 36. / 2. - 135. * 9. / 2.) * pi;
        let transverse = (3000. * (100. / 4. + 900. / 12. + (15. - z) * (15. - z))
            - 360. * (36. / 4. + 100. / 12. + (25. - z) * (25. - z))
            - 135. * (9. / 4. + 225. / 12. + (12.5 - z) * (12.5 - z)))
            * pi;
        near(p.central_inertia.unwrap()[0][0], transverse);
        near(p.central_inertia.unwrap()[2][2], axial);
        let angle = 0.71_f64;
        let q = [0., (angle / 2.).sin(), 0., (angle / 2.).cos()];
        let moved = transform(&m, [12., -7., 4.], q, t).unwrap().model;
        let prop = moved.mass_properties(t).unwrap();
        let axis = [angle.sin(), 0., angle.cos()];
        for i in 0..3 {
            near(prop.centroid.unwrap()[i], [12., -7., 4.][i] + z * axis[i]);
            for j in 0..3 {
                near(
                    prop.central_inertia.unwrap()[i][j],
                    (if i == j { transverse } else { 0. })
                        + (axial - transverse) * axis[i] * axis[j],
                );
            }
        }
        assert!(moved.revolve_geometry(t).unwrap().is_some());
        assert!(moved.cylinder_geometry(t).unwrap().is_none());
        assert!(moved.profile_prism_geometry(t).unwrap().is_none());
    }
    #[test]
    fn malformed_supports_topology_and_profiles_fail() {
        let t = Tolerance::default();
        let m = revolve_polygon(&shaft(), t).unwrap().model;
        for i in 0..8 {
            let mut bad = m.clone();
            match i {
                0 => bad.loops[0].edges[0].reversed = !bad.loops[0].edges[0].reversed,
                1 => bad.edges[0].vertices[1] = VertexId(999),
                2 => bad.vertices[0].position[0] += 0.01,
                3 => {
                    if let Curve::Circle(c) = &mut bad.edges[0].curve {
                        c.axis = [0., 1., 0.]
                    }
                }
                4 => {
                    if let Surface::Cylinder(c) = &mut bad.faces[1].surface {
                        c.inward = !c.inward
                    }
                }
                5 => bad.shells[0].faces.pop().map(|_| ()).unwrap(),
                6 => bad.faces[0].loops.push(LoopId(0)),
                _ => bad.solids.push(bad.solids[0].clone()),
            }
            assert!(!bad.validate(t).valid, "mutation {i}");
            assert!(bad.revolve_geometry(t).is_err());
        }
        let invalid = [
            vec![[0., 0.], [5., 0.], [0., 10.]],
            vec![[-1., 0.], [5., 0.], [5., 10.], [-1., 10.]],
            vec![[0., 0.], [0., 10.], [5., 10.], [5., 0.]],
        ];
        for p in invalid {
            assert!(revolve_polygon(&p, t).is_err());
        }
    }
    #[test]
    fn volume_precision_budget_includes_unsampled_circle_extent() {
        let t = Tolerance::default();
        let m = revolve_polygon(&[[0., 0.], [100., 0.], [100., 1.], [0., 1.]], t)
            .unwrap()
            .model;
        let moved = translate(&m, [-100., 0., 0.], t).unwrap().model;
        near(moved.volume(), 10000. * std::f64::consts::PI);
        near(moved.volume(), moved.mass_properties(t).unwrap().volume);
    }
    #[test]
    fn conical_solids_have_independent_polynomial_properties() {
        let t = Tolerance::default();
        let pi = std::f64::consts::PI;
        for hollow in [false, true] {
            let profile = if hollow {
                vec![[1., 0.], [2., 0.], [5., 3.], [3., 3.]]
            } else {
                vec![[0., 0.], [2., 0.], [5., 3.], [0., 3.]]
            };
            let m = revolve_polygon(&profile, t).unwrap().model;
            let p = m.mass_properties(t).unwrap();
            let volume = if hollow { 26. } else { 39. };
            let first = if hollow { 48.75 } else { 74.25 };
            let second = if hollow { 108. } else { 165.6 };
            let axial = if hollow { 273. } else { 309.3 };
            near(p.volume, volume * pi);
            near(p.centroid.unwrap()[2], first / volume);
            let transverse = (axial / 2. + second - first * first / volume) * pi;
            near(p.central_inertia.unwrap()[0][0], transverse);
            near(p.central_inertia.unwrap()[2][2], axial * pi);
            let angle = 0.71_f64;
            let axis = [angle.sin(), 0., angle.cos()];
            let moved = transform(
                &m,
                [12., -7., 4.],
                [0., (angle / 2.).sin(), 0., (angle / 2.).cos()],
                t,
            )
            .unwrap()
            .model;
            let p = moved.mass_properties(t).unwrap();
            for i in 0..3 {
                for j in 0..3 {
                    near(
                        p.central_inertia.unwrap()[i][j],
                        (if i == j { transverse } else { 0. })
                            + (axial * pi - transverse) * axis[i] * axis[j],
                    );
                }
            }
            assert!(moved.validate(t).valid);
            near(moved.volume(), volume * pi);
        }
        let m = revolve_polygon(&[[0., 0.], [2., 0.], [5., 3.], [0., 3.]], t)
            .unwrap()
            .model;
        let angle = 0.71_f64;
        assert!(transform(
            &m,
            [10000., -7000., 4000.],
            [0., (angle / 2.).sin(), 0., (angle / 2.).cos()],
            t
        )
        .is_ok());
        let profile = [
            [0., 0.],
            [10., 0.],
            [12., 2.],
            [12., 8.],
            [8., 18.],
            [6., 18.],
            [3., 12.],
            [3., 4.],
            [0., 4.],
        ];
        assert!(
            revolve_polygon(&profile, t)
                .unwrap()
                .model
                .validate(t)
                .valid
        );
    }
    #[test]
    fn conical_support_corruption_and_singularities_fail() {
        let t = Tolerance::default();
        let m = revolve_polygon(&[[0., 0.], [2., 0.], [5., 3.], [0., 3.]], t)
            .unwrap()
            .model;
        for i in 0..5 {
            let mut bad = m.clone();
            let Surface::Cone(c) = &mut bad.faces[1].surface else {
                panic!()
            };
            match i {
                0 => c.slope = -c.slope,
                1 => c.radius += 0.01,
                2 => c.inward = true,
                3 => c.axis = [0., 1., 0.],
                _ => c.slope = f64::NAN,
            }
            assert!(!bad.validate(t).valid);
            assert!(bad.revolve_geometry(t).is_err());
        }
        for p in [
            vec![[0., 0.], [2., 0.], [0., 3.]],
            vec![[0., 0.], [2., 0.], [2. + t.linear / 2., 3.], [0., 3.]],
            vec![[0., 0.], [2., 0.], [3., t.linear / 2.], [0., t.linear / 2.]],
        ] {
            assert!(revolve_polygon(&p, t).is_err());
        }
        for scale in [1e-6, 1., 1e3] {
            let p = [[0., 0.], [5., 0.], [2., 3.], [0., 3.]].map(|p| p.map(|v| v * scale));
            let m = revolve_polygon(
                &p,
                Tolerance {
                    linear: scale * 1e-8,
                },
            )
            .unwrap()
            .model;
            near(m.volume(), 39. * std::f64::consts::PI * scale.powi(3));
        }
    }
    #[test]
    fn annular_steps_and_small_scale() {
        for scale in [1e-6, 1., 1e3] {
            let p: Vec<_> = [
                [2., 0.],
                [5., 0.],
                [5., 10.],
                [3., 10.],
                [3., 20.],
                [2., 20.],
            ]
            .map(|p| p.map(|v| v * scale))
            .to_vec();
            let t = Tolerance {
                linear: scale * 1e-8,
            };
            let m = revolve_polygon(&p, t).unwrap().model;
            near(
                m.mass_properties(t).unwrap().volume,
                260. * std::f64::consts::PI * scale.powi(3),
            );
            assert!(m.volume().is_finite());
        }
    }
}
