//! Analytic circular extrusions. Validation certifies a bounded periodic B-rep
//! family, including its seam, surface orientations, and all references.
use crate::math::{dot, finite, norm, sub};
use crate::*;

/// Geometry recovered from the validated analytic boundary, never recipe data.
#[derive(Debug, Clone, PartialEq)]
pub struct CylinderGeometry {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub radius: f64,
    pub height: f64,
    pub inner_radius: f64,
    pub inner_wall: Option<FaceId>,
    pub start_cap: FaceId,
    pub end_cap: FaceId,
    pub wall: FaceId,
}
impl CylinderGeometry {
    pub(crate) fn volume(&self) -> f64 {
        std::f64::consts::PI
            * (self.radius - self.inner_radius)
            * (self.radius + self.inner_radius)
            * self.height
    }
    pub(crate) fn properties(&self) -> Result<MassProperties, KernelError> {
        let volume = self.volume();
        let squared_radii = self.radius * self.radius + self.inner_radius * self.inner_radius;
        let axial = volume * (squared_radii / 2.);
        let transverse = volume * (squared_radii / 4. + self.height * self.height / 12.);
        if [volume, axial, transverse]
            .iter()
            .any(|v| !v.is_normal() || *v <= 0.)
        {
            return Err(KernelError::NumericRange);
        }
        let centroid = std::array::from_fn(|i| self.origin[i] + self.axis[i] * (self.height / 2.));
        let tensor = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                let diagonal = if i == j { transverse } else { 0. };
                diagonal + (axial - transverse) * self.axis[i] * self.axis[j]
            })
        });
        if !finite(centroid)
            || tensor
                .iter()
                .flatten()
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

pub fn validate_circle(radius: f64, tolerance: Tolerance) -> Result<(), KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !radius.is_finite() || radius <= tolerance.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "radius",
        });
    }
    if !(std::f64::consts::PI * radius * radius).is_normal() {
        return Err(KernelError::NumericRange);
    }
    Ok(())
}

/// Extrude a true circle centered at the origin along +Z. The periodic wall
/// has one seam used twice in opposite directions. Caps retain full circles;
/// display tessellation never becomes authoritative geometry.
pub fn extrude_circle(
    radius: f64,
    height: f64,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_circle(radius, tolerance)?;
    if !height.is_finite() || height <= tolerance.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "height",
        });
    }
    let origin = [0., 0., 0.];
    let top = [0., 0., height];
    let axis = [0., 0., 1.];
    let x_direction = [1., 0., 0.];
    let circle = |origin| {
        Curve::Circle(Circle {
            origin,
            axis,
            x_direction,
            radius,
        })
    };
    let eu = |edge, reversed| EdgeUse {
        edge: EdgeId(edge),
        reversed,
    };
    let model = ModelSnapshot {
        vertices: vec![
            Vertex {
                position: [radius, 0., 0.],
            },
            Vertex {
                position: [radius, 0., height],
            },
        ],
        edges: vec![
            Edge {
                vertices: [VertexId(0), VertexId(0)],
                curve: circle(origin),
            },
            Edge {
                vertices: [VertexId(1), VertexId(1)],
                curve: circle(top),
            },
            Edge {
                vertices: [VertexId(0), VertexId(1)],
                curve: Curve::Line(Line {
                    origin: [radius, 0., 0.],
                    direction: axis,
                }),
            },
        ],
        loops: vec![
            Loop {
                edges: vec![eu(0, true)],
            },
            Loop {
                edges: vec![eu(1, false)],
            },
            Loop {
                edges: vec![eu(0, false), eu(2, false), eu(1, true), eu(2, true)],
            },
        ],
        faces: vec![
            Face {
                surface: Surface::Plane(Plane {
                    origin,
                    normal: [0., 0., -1.],
                }),
                loops: vec![LoopId(0)],
            },
            Face {
                surface: Surface::Plane(Plane {
                    origin: top,
                    normal: axis,
                }),
                loops: vec![LoopId(1)],
            },
            Face {
                surface: Surface::Cylinder(Cylinder {
                    origin,
                    axis,
                    x_direction,
                    radius,
                    inward: false,
                }),
                loops: vec![LoopId(2)],
            },
        ],
        shells: vec![Shell {
            faces: vec![FaceId(0), FaceId(1), FaceId(2)],
        }],
        solids: vec![Solid {
            shells: vec![ShellId(0)],
        }],
    };
    let report = model.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: BTreeMap::from([
                ("start_cap".into(), vec![FaceId(0)]),
                ("end_cap".into(), vec![FaceId(1)]),
                ("wall".into(), vec![FaceId(2)]),
            ]),
            input_faces: BTreeMap::new(),
        },
    })
}

/// Validate a concentric circular profile. Zero inner radius denotes a disk;
/// a positive inner radius and the remaining wall must exceed tolerance.
pub fn validate_annulus(
    radius: f64,
    inner_radius: f64,
    tolerance: Tolerance,
) -> Result<(), KernelError> {
    validate_circle(radius, tolerance)?;
    if !inner_radius.is_finite() || inner_radius < 0. {
        return Err(KernelError::InvalidDimension {
            dimension: "inner_radius",
        });
    }
    if inner_radius != 0. {
        validate_circle(inner_radius, tolerance)?;
        if radius - inner_radius <= tolerance.linear {
            return Err(KernelError::InvalidDimension {
                dimension: "wall_thickness",
            });
        }
    }
    Ok(())
}

/// Exact extrusion of a disk or annulus, with explicit inward bore surface.
pub fn extrude_annulus(
    radius: f64,
    inner_radius: f64,
    height: f64,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_annulus(radius, inner_radius, tolerance)?;
    let mut result = extrude_circle(radius, height, tolerance)?;
    if inner_radius == 0. {
        return Ok(result);
    }
    let inner = extrude_circle(inner_radius, height, tolerance)?.model;
    let m = &mut result.model;
    m.vertices.extend(inner.vertices);
    m.edges.extend(inner.edges.into_iter().map(|mut e| {
        e.vertices = e.vertices.map(|v| VertexId(v.0 + 2));
        e
    }));
    let eu = |edge, reversed| EdgeUse {
        edge: EdgeId(edge),
        reversed,
    };
    m.loops.extend([
        Loop {
            edges: vec![eu(3, false)],
        },
        Loop {
            edges: vec![eu(4, true)],
        },
        Loop {
            edges: vec![eu(3, true), eu(5, false), eu(4, false), eu(5, true)],
        },
    ]);
    m.faces[0].loops.push(LoopId(3));
    m.faces[1].loops.push(LoopId(4));
    let mut wall = inner.faces[2].clone();
    if let Surface::Cylinder(c) = &mut wall.surface {
        c.inward = true;
    }
    wall.loops = vec![LoopId(5)];
    m.faces.push(wall);
    m.shells[0].faces.push(FaceId(3));
    let report = m.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    result
        .history
        .generated_faces
        .insert("inner_wall".into(), vec![FaceId(3)]);
    Ok(result)
}

// Certify both support cylinders independently, then require a common exact
// parameter frame. Reducing each boundary to the certified disk family is a
// validation technique only; the authoritative annular topology is retained.
fn recognize_annulus(
    model: &ModelSnapshot,
    tolerance: Option<Tolerance>,
) -> Result<CylinderGeometry, KernelError> {
    if model.vertices.len() != 4
        || model.edges.len() != 6
        || model.loops.len() != 6
        || model.faces.len() != 4
        || model.shells.len() != 1
        || model.solids.len() != 1
    {
        return Err(unsupported(
            "annular extrusion requires complete periodic tube topology",
        ));
    }
    let eu = |edge, reversed| EdgeUse {
        edge: EdgeId(edge),
        reversed,
    };
    if model.faces[0].loops != [LoopId(0), LoopId(3)]
        || model.faces[1].loops != [LoopId(1), LoopId(4)]
        || model.faces[2].loops != [LoopId(2)]
        || model.faces[3].loops != [LoopId(5)]
        || model.loops[3].edges != [eu(3, false)]
        || model.loops[4].edges != [eu(4, true)]
        || model.loops[5].edges != [eu(3, true), eu(5, false), eu(4, false), eu(5, true)]
        || model.edges[3].vertices != [VertexId(2), VertexId(2)]
        || model.edges[4].vertices != [VertexId(3), VertexId(3)]
        || model.edges[5].vertices != [VertexId(2), VertexId(3)]
        || model.shells[0].faces != [FaceId(0), FaceId(1), FaceId(2), FaceId(3)]
        || model.solids[0].shells != [ShellId(0)]
    {
        return Err(unsupported(
            "invalid annular extrusion topology or orientation",
        ));
    }
    let Surface::Cylinder(inner_surface) = &model.faces[3].surface else {
        return Err(unsupported(
            "annular extrusion requires an inner cylindrical surface",
        ));
    };
    if !inner_surface.inward {
        return Err(unsupported("inner cylindrical wall must face inward"));
    }
    let mut outer = model.clone();
    outer.vertices.truncate(2);
    outer.edges.truncate(3);
    outer.loops.truncate(3);
    outer.faces.truncate(3);
    outer.faces[0].loops = vec![LoopId(0)];
    outer.faces[1].loops = vec![LoopId(1)];
    outer.shells[0].faces.truncate(3);
    let mut geometry = recognize(&outer, tolerance)?;
    let mut inner = outer.clone();
    inner.vertices = model.vertices[2..].to_vec();
    inner.edges = model.edges[3..]
        .iter()
        .cloned()
        .map(|mut e| {
            e.vertices = e.vertices.map(|v| VertexId(v.0 - 2));
            e
        })
        .collect();
    let mut support = inner_surface.clone();
    support.inward = false;
    inner.faces[2].surface = Surface::Cylinder(support);
    let bore = recognize(&inner, tolerance)?;
    if geometry.origin != bore.origin
        || geometry.axis != bore.axis
        || geometry.x_direction != bore.x_direction
        || geometry.height != bore.height
    {
        return Err(unsupported(
            "annular extrusion requires concentric circles in the same parameter frame",
        ));
    }
    let thickness = geometry.radius - bore.radius;
    if !thickness.is_normal() || thickness <= tolerance.map_or(0., |t| t.linear) {
        return Err(unsupported(
            "annular extrusion wall thickness must exceed tolerance",
        ));
    }
    geometry.inner_radius = bore.radius;
    geometry.inner_wall = Some(FaceId(3));
    geometry.properties()?;
    Ok(geometry)
}

fn unsupported(message: &str) -> KernelError {
    KernelError::Unsupported(message.into())
}

/// This analytic domain accepts complete canonical disk and annular
/// extrusion topology (including rigid placements). Other curve/surface B-reps
/// fail explicitly; merely containing a Cylinder surface is never sufficient.
pub(crate) fn recognize(
    model: &ModelSnapshot,
    tolerance: Option<Tolerance>,
) -> Result<CylinderGeometry, KernelError> {
    if model.faces.len() == 4 {
        return recognize_annulus(model, tolerance);
    }
    if model.vertices.len() != 2
        || model.edges.len() != 3
        || model.loops.len() != 3
        || model.faces.len() != 3
        || model.shells.len() != 1
        || model.solids.len() != 1
    {
        return Err(unsupported(
            "analytic geometry currently requires one complete periodic circular extrusion",
        ));
    }
    let (Curve::Circle(bottom), Curve::Circle(top), Curve::Line(seam)) = (
        &model.edges[0].curve,
        &model.edges[1].curve,
        &model.edges[2].curve,
    ) else {
        return Err(unsupported(
            "circular extrusion requires two circles and one seam line",
        ));
    };
    let (Surface::Plane(cap0), Surface::Plane(cap1), Surface::Cylinder(wall)) = (
        &model.faces[0].surface,
        &model.faces[1].surface,
        &model.faces[2].surface,
    ) else {
        return Err(unsupported(
            "circular extrusion requires two cap planes and one cylinder surface",
        ));
    };
    let eu = |edge, reversed| EdgeUse {
        edge: EdgeId(edge),
        reversed,
    };
    if model.edges.iter().map(|e| e.vertices).collect::<Vec<_>>()
        != [
            [VertexId(0), VertexId(0)],
            [VertexId(1), VertexId(1)],
            [VertexId(0), VertexId(1)],
        ]
        || model.loops[0].edges != [eu(0, true)]
        || model.loops[1].edges != [eu(1, false)]
        || model.loops[2].edges != [eu(0, false), eu(2, false), eu(1, true), eu(2, true)]
        || model
            .faces
            .iter()
            .enumerate()
            .any(|(i, f)| f.loops != [LoopId(i)])
        || model.shells[0].faces != [FaceId(0), FaceId(1), FaceId(2)]
        || model.solids[0].shells != [ShellId(0)]
    {
        return Err(unsupported(
            "invalid periodic circular extrusion topology or orientation",
        ));
    }
    if let Some(tol) = tolerance {
        validate_circle(bottom.radius, tol)?;
    }
    let vectors = [
        bottom.axis,
        bottom.x_direction,
        top.axis,
        top.x_direction,
        seam.direction,
        cap0.normal,
        cap1.normal,
        wall.axis,
        wall.x_direction,
    ];
    let unit_error = 64. * f64::EPSILON;
    if vectors
        .iter()
        .any(|v| !finite(*v) || (norm(*v) - 1.).abs() > unit_error)
        || dot(bottom.axis, bottom.x_direction).abs() > unit_error
        || !bottom.radius.is_normal()
        || bottom.radius <= 0.
        || top.radius != bottom.radius
        || wall.radius != bottom.radius
        || wall.inward
    {
        return Err(unsupported(
            "invalid analytic radius or orthonormal parameter frame",
        ));
    }
    if [
        sub(top.axis, bottom.axis),
        sub(top.x_direction, bottom.x_direction),
        sub(wall.axis, bottom.axis),
        sub(wall.x_direction, bottom.x_direction),
        sub(seam.direction, bottom.axis),
        sub(cap1.normal, bottom.axis),
        std::array::from_fn(|i| cap0.normal[i] + bottom.axis[i]),
    ]
    .iter()
    .any(|v| norm(*v) > unit_error)
    {
        return Err(unsupported(
            "circular extrusion surface orientations or frames disagree",
        ));
    }
    let points = [
        bottom.origin,
        top.origin,
        seam.origin,
        cap0.origin,
        cap1.origin,
        wall.origin,
        model.vertices[0].position,
        model.vertices[1].position,
    ];
    if points.iter().any(|p| !finite(*p)) {
        return Err(KernelError::NumericRange);
    }
    let delta = sub(top.origin, bottom.origin);
    let height = dot(delta, bottom.axis);
    if !height.is_normal() || height <= tolerance.map_or(0., |t| t.linear) {
        return Err(unsupported(
            "circular extrusion height must exceed tolerance",
        ));
    }
    let scale = points
        .iter()
        .flatten()
        .copied()
        .map(f64::abs)
        .fold(bottom.radius.max(height), f64::max);
    let error = scale * (256. * f64::EPSILON);
    if !error.is_normal() || tolerance.is_some_and(|t| error > t.linear / 16.) {
        return Err(KernelError::NumericRange);
    }
    let expected0 =
        std::array::from_fn(|i| bottom.origin[i] + bottom.radius * bottom.x_direction[i]);
    let expected1 = std::array::from_fn(|i| top.origin[i] + bottom.radius * bottom.x_direction[i]);
    for residual in [
        sub(cap0.origin, bottom.origin),
        sub(cap1.origin, top.origin),
        sub(wall.origin, bottom.origin),
        sub(seam.origin, model.vertices[0].position),
        sub(model.vertices[0].position, expected0),
        sub(model.vertices[1].position, expected1),
        std::array::from_fn(|i| delta[i] - height * bottom.axis[i]),
    ] {
        if norm(residual) > error {
            return Err(unsupported(
                "circle, plane, cylinder, and seam geometry do not agree",
            ));
        }
    }
    let geometry = CylinderGeometry {
        origin: bottom.origin,
        axis: bottom.axis,
        x_direction: bottom.x_direction,
        radius: bottom.radius,
        height,
        inner_radius: 0.,
        inner_wall: None,
        start_cap: FaceId(0),
        end_cap: FaceId(1),
        wall: FaceId(2),
    };
    geometry.properties()?;
    let bounds = model.bounds();
    if !finite(bounds.min) || !finite(bounds.max) {
        return Err(KernelError::NumericRange);
    }
    Ok(geometry)
}

impl ModelSnapshot {
    /// Returns validated analytic disk/tube extrusion geometry, or None for a validated
    /// planar model. No curve is replaced by a polygon for this query.
    pub fn cylinder_geometry(
        &self,
        tolerance: Tolerance,
    ) -> Result<Option<CylinderGeometry>, KernelError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if self.has_curved_geometry() {
            match recognize(self, Some(tolerance)) {
                Ok(g) => Ok(Some(g)),
                Err(_) => crate::profile_prism::recognize(self, tolerance)
                    .map(|_| None)
                    .or_else(|_| crate::revolve::recognize(self, tolerance).map(|_| None)),
            }
        } else {
            let report = self.validate(tolerance);
            if !report.valid {
                Err(KernelError::InvalidInput(report))
            } else {
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-10 * expected.abs().max(1.),
            "{actual} != {expected}"
        );
    }
    #[test]
    fn annular_extrusion_matches_independent_disk_difference_and_rotated_tensor() {
        let tol = Tolerance::default();
        for (r, bore, h) in [(10., 7., 25.), (3., 1., 0.5), (100., 99.99, 12.)] {
            let result = extrude_annulus(r, bore, h, tol).unwrap();
            let m = result.model;
            assert!(m.validate(tol).valid);
            assert_eq!(
                (
                    m.vertices.len(),
                    m.edges.len(),
                    m.loops.len(),
                    m.faces.len()
                ),
                (4, 6, 6, 4)
            );
            assert_eq!(result.history.generated_faces["inner_wall"], [FaceId(3)]);
            assert_eq!(m.faces[0].loops.len(), 2);
            assert_eq!(m.faces[1].loops.len(), 2);
            let expected_volume = std::f64::consts::PI * (r * r - bore * bore) * h;
            close(m.volume(), expected_volume);
            let p = m.mass_properties(tol).unwrap();
            let outer = extrude_circle(r, h, tol)
                .unwrap()
                .model
                .mass_properties(tol)
                .unwrap();
            let inner = extrude_circle(bore, h, tol)
                .unwrap()
                .model
                .mass_properties(tol)
                .unwrap();
            for i in 0..3 {
                for j in 0..3 {
                    close(
                        p.central_inertia.unwrap()[i][j],
                        outer.central_inertia.unwrap()[i][j] - inner.central_inertia.unwrap()[i][j],
                    );
                }
            }
            assert_eq!(p.centroid, Some([0., 0., h / 2.]));
            assert_eq!(
                m.bounds(),
                Bounds {
                    min: [-r, -r, 0.],
                    max: [r, r, h]
                }
            );
            let moved = transform(&m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], tol)
                .unwrap()
                .model;
            let axis = [0.5_f64.sqrt(), -0.5_f64.sqrt(), 0.];
            let tensor = moved.mass_properties(tol).unwrap().central_inertia.unwrap();
            let axial = expected_volume * (r * r + bore * bore) / 2.;
            let transverse = expected_volume * (3. * (r * r + bore * bore) + h * h) / 12.;
            for i in 0..3 {
                for j in 0..3 {
                    close(
                        tensor[i][j],
                        if i == j { transverse } else { 0. }
                            + (axial - transverse) * axis[i] * axis[j],
                    );
                }
            }
            let frame = m.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], tol).unwrap();
            assert_eq!(frame.origin, [0., 0., h]); // The annular centroid lies inside the empty bore.
            assert!(m.face_frame(FaceId(3), [1., 0., 0.], [0.; 3], tol).is_err());
            let decoded: ModelSnapshot =
                serde_json::from_slice(&serde_json::to_vec(&m).unwrap()).unwrap();
            assert_eq!(decoded, m);
            assert!(decoded.validate(tol).valid);
        }
        assert_eq!(
            extrude_annulus(4., 0., 12., tol).unwrap(),
            extrude_circle(4., 12., tol).unwrap()
        );
    }
    #[test]
    fn annular_dimensions_and_corrupted_bore_boundaries_fail_closed() {
        let tol = Tolerance::default();
        for bore in [
            -1.,
            f64::NAN,
            f64::INFINITY,
            tol.linear,
            10.,
            11.,
            10. - tol.linear / 2.,
        ] {
            assert!(extrude_annulus(10., bore, 20., tol).is_err());
        }
        let model = extrude_annulus(10., 7., 20., tol).unwrap().model;
        let changes: Vec<fn(&mut ModelSnapshot)> = vec![
            |m| m.vertices[2].position[0] += 1e-9,
            |m| m.edges[3].vertices[0] = VertexId(0),
            |m| m.loops[3].edges[0].reversed = true,
            |m| m.loops[4].edges[0].reversed = false,
            |m| m.loops[5].edges[0].reversed = false,
            |m| m.faces[0].loops.pop().map(drop).unwrap(),
            |m| m.faces[3].loops = vec![LoopId(2)],
            |m| m.edges[5].try_line_mut().unwrap().origin[1] += 1e-9,
            |m| {
                if let Curve::Circle(c) = &mut m.edges[3].curve {
                    c.radius = 11.;
                }
            },
            |m| {
                if let Curve::Circle(c) = &mut m.edges[4].curve {
                    c.origin[2] += 1e-9;
                }
            },
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[3].surface {
                    c.inward = false;
                }
            },
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[2].surface {
                    c.inward = true;
                }
            },
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[3].surface {
                    c.axis[2] = -1.;
                }
            },
            |m| m.shells[0].faces.pop().map(drop).unwrap(),
            |m| m.solids[0].shells.clear(),
            |m| {
                m.vertices.clear();
            },
        ];
        for (i, change) in changes.into_iter().enumerate() {
            let mut invalid = model.clone();
            change(&mut invalid);
            assert!(!invalid.validate(tol).valid, "mutation {i}");
            assert!(invalid.mass_properties(tol).is_err(), "mutation {i}");
            assert!(invalid.cylinder_geometry(tol).is_err(), "mutation {i}");
            assert!(
                transform(&invalid, [0.; 3], [0., 0., 0., 1.], tol).is_err(),
                "mutation {i}"
            );
            assert!(invalid.volume().is_nan(), "mutation {i}");
        }
    }
    #[test]
    fn circle_extrusion_has_true_periodic_boundary_and_exact_properties() {
        let tol = Tolerance::default();
        for (radius, height) in [(1., 2.), (7.5, 32.), (50., 0.75)] {
            let result = extrude_circle(radius, height, tol).unwrap();
            let model = result.model;
            assert!(model.validate(tol).valid);
            assert_eq!(
                (model.vertices.len(), model.edges.len(), model.faces.len()),
                (2, 3, 3)
            );
            assert_eq!(
                model
                    .edges
                    .iter()
                    .filter(|e| matches!(e.curve, Curve::Circle(_)))
                    .count(),
                2
            );
            assert_eq!(
                model
                    .faces
                    .iter()
                    .filter(|f| matches!(f.surface, Surface::Cylinder(_)))
                    .count(),
                1
            );
            assert_eq!(
                model.bounds(),
                Bounds {
                    min: [-radius, -radius, 0.],
                    max: [radius, radius, height]
                }
            );
            let volume = std::f64::consts::PI * radius.powi(2) * height;
            close(model.volume(), volume);
            let props = model.mass_properties(tol).unwrap();
            close(props.volume, volume);
            assert_eq!(props.centroid, Some([0., 0., height / 2.]));
            let tensor = props.central_inertia.unwrap();
            close(
                tensor[0][0],
                volume * (3. * radius * radius + height * height) / 12.,
            );
            close(tensor[1][1], tensor[0][0]);
            close(tensor[2][2], volume * radius * radius / 2.);
            assert_eq!([tensor[0][1], tensor[0][2], tensor[1][2]], [0.; 3]);
            assert_eq!(result.history.generated_faces["wall"], [FaceId(2)]);
        }
    }
    #[test]
    fn oblique_cylinder_bounds_and_full_tensor_follow_analytic_axis() {
        let tol = Tolerance::default();
        let model = extrude_circle(3., 10., tol).unwrap().model;
        // 90 degrees about [1,1,0]/sqrt(2): axis becomes [sqrt(1/2),-sqrt(1/2),0].
        let moved = transform(&model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], tol).unwrap();
        let axis = [0.5_f64.sqrt(), -0.5_f64.sqrt(), 0.];
        let center = [12., -7., 4.];
        let bounds = moved.model.bounds();
        for i in 0..3 {
            let extent = 3. * (1. - axis[i] * axis[i]).sqrt();
            close(bounds.min[i], center[i] + (10. * axis[i]).min(0.) - extent);
            close(bounds.max[i], center[i] + (10. * axis[i]).max(0.) + extent);
        }
        let p = moved.model.mass_properties(tol).unwrap();
        let volume = 90. * std::f64::consts::PI;
        let transverse = volume * 127. / 12.;
        let axial = volume * 4.5;
        let tensor = p.central_inertia.unwrap();
        close(tensor[0][0], (transverse + axial) / 2.);
        close(tensor[1][1], (transverse + axial) / 2.);
        close(tensor[2][2], transverse);
        close(tensor[0][1], (transverse - axial) / 2.);
        for (i, &coordinate) in p.centroid.unwrap().iter().enumerate() {
            close(coordinate, center[i] + 5. * axis[i]);
        }
        assert_eq!(moved.history.input_faces["input"][&FaceId(2)], [FaceId(2)]);
        let translated = translate(&moved.model, [-4., 3., 2.], tol).unwrap();
        close(translated.model.volume(), volume);
        assert!(translated
            .history
            .generated_faces
            .contains_key("translated"));
        let t = translated
            .model
            .mass_properties(tol)
            .unwrap()
            .central_inertia
            .unwrap();
        for i in 0..3 {
            for j in 0..3 {
                close(t[i][j], tensor[i][j]);
            }
        }
    }
    #[test]
    fn circular_cap_frames_preserve_exact_centers_and_reject_wall() {
        let tol = Tolerance::default();
        let model = extrude_circle(7., 20., tol).unwrap().model;
        let bottom = model
            .face_frame(FaceId(0), [1., 0., 0.], [2., 3., 4.], tol)
            .unwrap();
        assert_eq!(bottom.origin, [2., -3., -4.]);
        let top = model
            .face_frame(FaceId(1), [1., 0., 0.], [2., 3., 4.], tol)
            .unwrap();
        assert_eq!(top.origin, [2., 3., 24.]);
        assert!(matches!(
            model.face_frame(FaceId(2), [1., 0., 0.], [0.; 3], tol),
            Err(KernelError::InvalidFaceFrame(_))
        ));
        let moved = transform(&model, [3., 4., 5.], [0.5, 0.5, 0.5, 0.5], tol)
            .unwrap()
            .model;
        let frame = moved
            .face_frame(FaceId(1), [0., 1., 0.], [0.; 3], tol)
            .unwrap();
        assert_eq!(frame.origin, [23., 4., 5.]);
        assert_eq!(frame.z_axis, [1., 0., 0.]);
    }
    #[test]
    fn analytic_validator_rejects_corrupted_supports_seams_and_references() {
        let tol = Tolerance::default();
        let model = extrude_circle(3., 10., tol).unwrap().model;
        let changes: Vec<fn(&mut ModelSnapshot)> = vec![
            |m| m.vertices[0].position[1] += 1e-9,
            |m| m.edges[0].vertices[1] = VertexId(1),
            |m| m.edges[2].try_line_mut().unwrap().direction = [1., 0., 0.],
            |m| m.edges[2].try_line_mut().unwrap().origin[1] += 1e-9,
            |m| m.loops[2].edges[3].reversed = false,
            |m| m.loops[0].edges[0].edge = EdgeId(99),
            |m| m.faces[0].try_plane_mut().unwrap().normal[2] = 1.,
            |m| m.faces[1].try_plane_mut().unwrap().origin[0] += 1e-9,
            |m| m.faces[2].loops = vec![LoopId(99)],
            |m| m.shells[0].faces.push(FaceId(0)),
            |m| m.solids[0].shells.clear(),
            |m| {
                if let Curve::Circle(c) = &mut m.edges[0].curve {
                    c.radius = f64::NAN;
                }
            },
            |m| {
                if let Curve::Circle(c) = &mut m.edges[1].curve {
                    c.origin[0] += 1e-9;
                }
            },
            |m| {
                if let Curve::Circle(c) = &mut m.edges[1].curve {
                    c.x_direction = [0., 1., 0.];
                }
            },
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[2].surface {
                    c.radius += 1e-9;
                }
            },
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[2].surface {
                    c.axis = [0., 0., -1.];
                }
            },
        ];
        for (i, change) in changes.into_iter().enumerate() {
            let mut invalid = model.clone();
            change(&mut invalid);
            assert!(!invalid.validate(tol).valid, "mutation {i}");
            assert!(invalid.mass_properties(tol).is_err(), "mutation {i}");
            assert!(
                transform(&invalid, [0.; 3], [0., 0., 0., 1.], tol).is_err(),
                "mutation {i}"
            );
        }
    }
    #[test]
    fn curved_operations_fail_explicitly_without_polygon_substitution() {
        let tol = Tolerance::default();
        let model = extrude_circle(3., 10., tol).unwrap().model;
        let box_model = extrude_rectangle(8., 8., 8., tol).unwrap().model;
        for op in [
            BooleanOp::Union,
            BooleanOp::Difference,
            BooleanOp::Intersection,
        ] {
            assert!(matches!(
                boolean(&model, &box_model, op, tol),
                Err(KernelError::Unsupported(_))
            ));
            assert!(matches!(
                boolean(&box_model, &model, op, tol),
                Err(KernelError::Unsupported(_))
            ));
        }
        for face in 0..3 {
            assert!(!face_is_convex(&model, FaceId(face)));
            assert!(matches!(
                triangulate_face(&model, FaceId(face), tol),
                Err(KernelError::Unsupported(_))
            ));
        }
    }
    #[test]
    fn partial_or_mixed_analytic_snapshots_fail_public_operations_without_panics() {
        let tol = Tolerance::default();
        let model = extrude_circle(3., 10., tol).unwrap().model;
        let box_model = extrude_rectangle(8., 8., 8., tol).unwrap().model;
        let changes: Vec<fn(&mut ModelSnapshot)> = vec![
            |m| m.vertices.clear(),
            |m| m.edges.clear(),
            |m| m.loops.clear(),
            |m| m.faces.clear(),
            |m| m.shells.clear(),
            |m| m.solids.clear(),
            |m| {
                m.faces[2].surface = Surface::Plane(Plane {
                    origin: [0.; 3],
                    normal: [1., 0., 0.],
                })
            },
            |m| {
                m.edges[0].curve = Curve::Line(Line {
                    origin: [0.; 3],
                    direction: [1., 0., 0.],
                })
            },
            |m| m.vertices[0].position[0] = f64::INFINITY,
        ];
        for change in changes {
            let mut invalid = model.clone();
            change(&mut invalid);
            assert!(!invalid.validate(tol).valid);
            assert!(invalid.cylinder_geometry(tol).is_err());
            assert!(invalid.mass_properties(tol).is_err());
            assert!(invalid
                .face_frame(FaceId(0), [1., 0., 0.], [0.; 3], tol)
                .is_err());
            assert!(translate(&invalid, [0.; 3], tol).is_err());
            assert!(transform(&invalid, [0.; 3], [0., 0., 0., 1.], tol).is_err());
            assert!(triangulate_face(&invalid, FaceId(0), tol).is_err());
            assert!(tessellate_cylinder(&invalid, tol, TessellationPolicy::default()).is_err());
            assert!(boolean(&invalid, &box_model, BooleanOp::Intersection, tol).is_err());
            for id in [FaceId(0), FaceId(2), FaceId(usize::MAX)] {
                assert!(!face_is_convex(&invalid, id));
            }
            assert!(invalid.volume().is_nan());
            let _ = invalid.bounds(); // Unvalidated convenience query must not panic.
        }
    }

    #[test]
    fn dimension_precision_and_inertia_range_are_enforced() {
        let tol = Tolerance::default();
        for radius in [0., -1., f64::NAN, f64::INFINITY, tol.linear] {
            assert!(extrude_circle(radius, 10., tol).is_err());
        }
        for height in [0., -1., f64::NAN, f64::INFINITY, tol.linear] {
            assert!(extrude_circle(3., height, tol).is_err());
        }
        assert!(extrude_circle(1e100, 1e100, tol).is_err());
        assert!(extrude_circle(1e-100, 1e-100, Tolerance { linear: 1e-110 }).is_err());
        let model = extrude_circle(3., 10., tol).unwrap().model;
        assert!(translate(&model, [1e12, 0., 0.], tol).is_err());
        assert!(model.cylinder_geometry(Tolerance { linear: 0. }).is_err());
    }
    #[test]
    fn tagged_native_serialization_round_trips_and_rejects_legacy_support_fields() {
        let tol = Tolerance::default();
        let model = extrude_circle(3., 10., tol).unwrap().model;
        let bytes = serde_json::to_vec(&model).unwrap();
        let decoded: ModelSnapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(model, decoded);
        assert!(decoded.validate(tol).valid);
        let mut value =
            serde_json::to_value(extrude_rectangle(1., 2., 3., tol).unwrap().model).unwrap();
        let edge = value["edges"][0].as_object_mut().unwrap();
        let line = edge.remove("curve").unwrap()["geometry"].clone();
        edge.insert("line".into(), line);
        assert!(serde_json::from_value::<ModelSnapshot>(value).is_err());
    }
}
