//! Analytic STEP entities for validated planar/cylindrical/conical B-reps.
use super::*;
use cad_kernel::{Curve, Surface};

fn placement(step: &mut Step, origin: Point, axis: Point, x: Point) -> usize {
    let p = step.add(format!("CARTESIAN_POINT('',({}))", triple(origin)));
    let z = step.add(format!("DIRECTION('',({}))", triple(axis)));
    let x = step.add(format!("DIRECTION('',({}))", triple(x)));
    step.add(format!("AXIS2_PLACEMENT_3D('',#{p},#{z},#{x})"))
}
fn seam_pcurve(step: &mut Step, surface: usize, u: f64, v_scale: f64) -> usize {
    let p = step.add(format!("CARTESIAN_POINT('',({},0.))", real(u)));
    let d = step.add(format!(
        "DIRECTION('',(0.,{}))",
        if v_scale < 0. { "-1." } else { "1." }
    ));
    let v = step.add(format!("VECTOR('',#{d},{})", real(v_scale.abs())));
    let line = step.add(format!("LINE('',#{p},#{v})"));
    let context = step.add("(GEOMETRIC_REPRESENTATION_CONTEXT(2) PARAMETRIC_REPRESENTATION_CONTEXT() REPRESENTATION_CONTEXT('',''))".into());
    let representation = step.add(format!(
        "DEFINITIONAL_REPRESENTATION('',(#{line}),#{context})"
    ));
    step.add(format!("PCURVE('',#{surface},#{representation})"))
}

pub(super) fn solids(
    step: &mut Step,
    model: &ModelSnapshot,
    tolerance: Tolerance,
) -> Result<Vec<usize>, ExportError> {
    let validation = model.validate(tolerance);
    if !validation.valid {
        return Err(ExportError::InvalidModel(format!(
            "{:?}",
            validation.issues
        )));
    }
    if model.solids.len() != 1 || model.solids[0].shells.len() != 1 {
        return Err(ExportError::UnsupportedModel(
            "analytic STEP requires one validated single-shell solid",
        ));
    }
    let mut surfaces = Vec::new();
    for face in &model.faces {
        let surface = match &face.surface {
            Surface::Plane(p) => {
                // Choose a tangent independently for each plane. A prism side
                // may be parallel to the extrusion frame's reference direction.
                let least = (0..3)
                    .min_by(|&a, &b| p.normal[a].abs().total_cmp(&p.normal[b].abs()))
                    .unwrap();
                let mut reference = [0.; 3];
                reference[least] = 1.;
                let x = normalized(cross(reference, p.normal))
                    .ok_or_else(|| ExportError::InvalidModel("invalid plane normal".into()))?;
                let axis = placement(step, p.origin, p.normal, x);
                step.add(format!("PLANE('',#{axis})"))
            }
            Surface::Cylinder(c) => {
                let axis = placement(step, c.origin, c.axis, c.x_direction);
                step.add(format!(
                    "CYLINDRICAL_SURFACE('',#{axis},{})",
                    real(c.radius)
                ))
            }
            Surface::Cone(c) => {
                // Use positive semi-angles for importer interoperability. Axis
                // reversal preserves the support, with reversed seam V below.
                let z = if c.slope < 0. {
                    c.axis.map(|v| -v)
                } else {
                    c.axis
                };
                let axis = placement(step, c.origin, z, c.x_direction);
                step.add(format!(
                    "CONICAL_SURFACE('',#{axis},{},{})",
                    real(c.radius),
                    real(c.slope.abs().atan())
                ))
            }
        };
        surfaces.push(surface);
    }
    let vertices: Vec<_> = model
        .vertices
        .iter()
        .map(|v| {
            let p = step.add(format!("CARTESIAN_POINT('',({}))", triple(v.position)));
            step.add(format!("VERTEX_POINT('',#{p})"))
        })
        .collect();
    let mut edges = Vec::new();
    for (edge_index, edge) in model.edges.iter().enumerate() {
        let curve = match &edge.curve {
            Curve::Circle(c) => {
                let axis = placement(step, c.origin, c.axis, c.x_direction);
                step.add(format!("CIRCLE('',#{axis},{})", real(c.radius)))
            }
            Curve::Line(l) => {
                let p = step.add(format!("CARTESIAN_POINT('',({}))", triple(l.origin)));
                let d = step.add(format!("DIRECTION('',({}))", triple(l.direction)));
                let v = step.add(format!("VECTOR('',#{d},1.)"));
                let line = step.add(format!("LINE('',#{p},#{v})"));
                // A tube has two periodic surfaces. Each seam must carry its
                // pcurves on its own surface, including the inward bore.
                let wall = model.faces.iter().enumerate().find_map(|(i, face)| {
                    matches!(face.surface, Surface::Cylinder(_) | Surface::Cone(_))
                        .then_some(i)
                        .filter(|_| {
                            face.loops.iter().any(|id| {
                                model.loops[id.0]
                                    .edges
                                    .iter()
                                    .any(|u| u.edge.0 == edge_index)
                            })
                        })
                });
                if let Some(wall) = wall {
                    // STEP's conical V parameter is axial distance. Match the
                    // unit-length 3D generatrix with dV/dt = cos(semi_angle).
                    let scale = match &model.faces[wall].surface {
                        Surface::Cone(c) => c.slope.signum() / c.slope.hypot(1.),
                        _ => 1.,
                    };
                    let a = seam_pcurve(step, surfaces[wall], 0., scale);
                    let b = seam_pcurve(step, surfaces[wall], std::f64::consts::TAU, scale);
                    step.add(format!("SEAM_CURVE('',#{line},(#{a},#{b}),.CURVE_3D.)"))
                } else {
                    line
                }
            }
        };
        edges.push(step.add(format!(
            "EDGE_CURVE('',#{},#{},#{curve},.T.)",
            vertices[edge.vertices[0].0], vertices[edge.vertices[1].0]
        )));
    }
    let mut faces = Vec::new();
    for (i, face) in model.faces.iter().enumerate() {
        let mut bounds = Vec::new();
        for (loop_index, loop_id) in face.loops.iter().enumerate() {
            let oriented: Vec<_> = model.loops[loop_id.0]
                .edges
                .iter()
                .map(|usage| {
                    step.add(format!(
                        "ORIENTED_EDGE('',*,*,#{},{})",
                        edges[usage.edge.0],
                        if usage.reversed { ".F." } else { ".T." }
                    ))
                })
                .collect();
            let boundary = step.add(format!("EDGE_LOOP('',({}))", refs(&oriented)));
            let kind = if loop_index == 0 {
                "FACE_OUTER_BOUND"
            } else {
                "FACE_BOUND"
            };
            bounds.push(step.add(format!("{kind}('',#{boundary},.T.)")));
        }
        faces.push(step.add(format!(
            "ADVANCED_FACE('',({}),#{},{})",
            refs(&bounds),
            surfaces[i],
            if matches!(&face.surface, Surface::Cylinder(c) if c.inward)
                || matches!(&face.surface, Surface::Cone(c) if c.inward)
            {
                ".F."
            } else {
                ".T."
            }
        )));
    }
    let shell = step.add(format!("CLOSED_SHELL('',({}))", refs(&faces)));
    Ok(vec![
        step.add(format!("MANIFOLD_SOLID_BREP('part',#{shell})"))
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drilled_prism_step_preserves_mixed_geometry_and_only_bore_seams() {
        let t = Tolerance::default();
        let model = cad_kernel::extrude_polygon_with_circular_holes(
            &[[0., 0.], [80., 0.], [80., 50.], [0., 50.]],
            &[],
            &[
                cad_kernel::CircleHole {
                    center: [20., 25.],
                    radius: 5.,
                },
                cad_kernel::CircleHole {
                    center: [60., 25.],
                    radius: 8.,
                },
            ],
            12.,
            t,
        )
        .unwrap()
        .model;
        let step = to_step(&model, t).unwrap();
        assert_eq!(step.matches("CIRCLE('").count(), 4);
        assert_eq!(step.matches("CYLINDRICAL_SURFACE('").count(), 2);
        assert_eq!(step.matches("PLANE('").count(), 6);
        assert_eq!(step.matches("SEAM_CURVE('").count(), 2);
        assert_eq!(step.matches("PCURVE('").count(), 4);
        assert_eq!(step.matches("FACE_OUTER_BOUND('").count(), 8);
        assert_eq!(step.matches("FACE_BOUND('").count(), 4);
        assert_eq!(step.matches("EDGE_CURVE('").count(), 18);
        let faces: Vec<_> = step
            .lines()
            .filter(|line| line.contains("ADVANCED_FACE('"))
            .collect();
        assert_eq!(faces.len(), 8);
        assert!(faces[..6].iter().all(|line| line.ends_with(",.T.);")));
        assert!(faces[6..].iter().all(|line| line.ends_with(",.F.);")));
        assert!(!step.contains("FACETED_BREP("));
        assert_eq!(step, to_step(&model, t).unwrap());
    }

    #[test]
    fn annular_step_preserves_inner_bounds_inward_surface_and_distinct_seams() {
        let t = Tolerance::default();
        let m = cad_kernel::extrude_annulus(12., 8., 40., t).unwrap().model;
        let step = to_step(&m, t).unwrap();
        assert_eq!(step.matches("CIRCLE('").count(), 4);
        assert_eq!(step.matches("CYLINDRICAL_SURFACE('").count(), 2);
        assert_eq!(step.matches("SEAM_CURVE('").count(), 2);
        assert_eq!(step.matches("PCURVE('").count(), 4);
        assert_eq!(step.matches("FACE_OUTER_BOUND('").count(), 4);
        assert_eq!(step.matches("FACE_BOUND('").count(), 2);
        let faces: Vec<_> = step
            .lines()
            .filter(|line| line.contains("ADVANCED_FACE('"))
            .collect();
        assert_eq!(faces.len(), 4);
        assert!(faces[..3].iter().all(|line| line.ends_with(",.T.);")));
        assert!(faces[3].ends_with(",.F.);"));
        let surfaces: Vec<_> = step
            .lines()
            .filter(|line| line.contains("PCURVE('"))
            .map(|line| {
                line.split("PCURVE('',")
                    .nth(1)
                    .unwrap()
                    .split(',')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(surfaces[0], surfaces[1]);
        assert_eq!(surfaces[2], surfaces[3]);
        assert_ne!(surfaces[0], surfaces[2]);
        assert!(!step.contains("FACETED_BREP("));
        assert_eq!(step, to_step(&m, t).unwrap());
    }

    #[test]
    fn revolved_shoulder_and_bore_exports_preserve_closed_boundaries() {
        let t = Tolerance::default();
        for (profile, expected_volume, cylinders, cones) in [
            (
                vec![
                    [0., 0.],
                    [8., 0.],
                    [8., 20.],
                    [15., 20.],
                    [15., 26.],
                    [6., 26.],
                    [6., 50.],
                    [0., 50.],
                ],
                3494. * std::f64::consts::PI,
                3,
                0,
            ),
            (
                vec![
                    [8., 0.],
                    [25., 0.],
                    [25., 8.],
                    [20., 8.],
                    [20., 32.],
                    [12., 32.],
                    [12., 12.],
                    [8., 12.],
                ],
                10952. * std::f64::consts::PI,
                4,
                0,
            ),
            (
                vec![
                    [0., 0.],
                    [20., 0.],
                    [20., 30.],
                    [8., 30.],
                    [8., 10.],
                    [0., 10.],
                ],
                10720. * std::f64::consts::PI,
                2,
                0,
            ),
            (
                vec![[2., 0.], [8., 0.], [4., 12.], [3., 12.]],
                372. * std::f64::consts::PI,
                0,
                2,
            ),
            (
                vec![[1., 0.], [4., 0.], [8., 12.], [1., 12.]],
                436. * std::f64::consts::PI,
                1,
                1,
            ),
        ] {
            let model = cad_kernel::revolve_polygon(&profile, t).unwrap().model;
            for model in [
                model.clone(),
                cad_kernel::transform(&model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                    .unwrap()
                    .model,
            ] {
                let step = to_step(&model, t).unwrap();
                assert_eq!(step.matches("CYLINDRICAL_SURFACE(").count(), cylinders);
                assert_eq!(step.matches("CONICAL_SURFACE(").count(), cones);
                assert_eq!(step.matches("SEAM_CURVE(").count(), cylinders + cones);
                for line in step
                    .lines()
                    .filter(|line| line.contains("CONICAL_SURFACE("))
                {
                    let angle: f64 = line
                        .rsplit(',')
                        .next()
                        .unwrap()
                        .trim_end_matches(");")
                        .parse()
                        .unwrap();
                    assert!(angle > 0. && angle < std::f64::consts::FRAC_PI_2);
                }
                assert!(!step.contains("FACETED_BREP("));
                let stl = to_stl(&model, t).unwrap();
                let points: Vec<Point> = stl
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("vertex "))
                    .map(|line| {
                        let p: Vec<f64> = line
                            .split_whitespace()
                            .map(|n| n.parse().unwrap())
                            .collect();
                        [p[0], p[1], p[2]]
                    })
                    .collect();
                let normals: Vec<Point> = stl
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("facet normal "))
                    .map(|line| {
                        let p: Vec<f64> = line
                            .split_whitespace()
                            .map(|n| n.parse().unwrap())
                            .collect();
                        [p[0], p[1], p[2]]
                    })
                    .collect();
                let mut incidence = std::collections::BTreeMap::new();
                let mut volume = 0.;
                for (triangle, declared) in points.as_chunks::<3>().0.iter().zip(&normals) {
                    let normal =
                        cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
                    assert!(dot(normal, normal) > 0.);
                    let norm = dot(normal, normal).sqrt();
                    assert!(normal
                        .iter()
                        .zip(declared)
                        .all(|(a, b)| (a / norm - b).abs() < 1e-10));
                    volume += dot(triangle[0], cross(triangle[1], triangle[2])) / 6.;
                    for i in 0..3 {
                        let key = |p: Point| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
                        let a = key(triangle[i]);
                        let b = key(triangle[(i + 1) % 3]);
                        let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                        let entry = incidence.entry(edge).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += sign;
                    }
                }
                assert!(incidence
                    .values()
                    .all(|&(count, sign)| count == 2 && sign == 0));
                assert!(
                    volume < expected_volume
                        && (expected_volume - volume) / expected_volume < 0.002
                );
            }
        }
    }

    #[test]
    fn drilled_stl_is_closed_and_cap_triangles_do_not_fill_bores() {
        let t = Tolerance::default();
        let circles = [
            cad_kernel::CircleHole {
                center: [20., 25.],
                radius: 5.,
            },
            cad_kernel::CircleHole {
                center: [60., 25.],
                radius: 8.,
            },
        ];
        let model = cad_kernel::extrude_polygon_with_circular_holes(
            &[[0., 0.], [80., 0.], [80., 50.], [0., 50.]],
            &[],
            &circles,
            12.,
            t,
        )
        .unwrap()
        .model;
        let stl = to_stl(&model, t).unwrap();
        let points: Vec<Point> = stl
            .lines()
            .filter_map(|line| line.trim().strip_prefix("vertex "))
            .map(|line| {
                let p: Vec<f64> = line
                    .split_whitespace()
                    .map(|n| n.parse().unwrap())
                    .collect();
                [p[0], p[1], p[2]]
            })
            .collect();
        let mut incidence = std::collections::BTreeMap::new();
        let mut volume = 0.;
        for triangle in points.as_chunks::<3>().0 {
            let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
            assert!(dot(normal, normal) > 0.);
            if normal[2].abs() > 1e-10 {
                let center: Point = std::array::from_fn(|i| {
                    (triangle[0][i] + triangle[1][i] + triangle[2][i]) / 3.
                });
                for circle in &circles {
                    // A chordal bore is slightly smaller than its exact circle.
                    assert!(
                        (center[0] - circle.center[0]).hypot(center[1] - circle.center[1])
                            > circle.radius - 0.011
                    );
                }
            }
            volume += dot(triangle[0], cross(triangle[1], triangle[2])) / 6.;
            for i in 0..3 {
                let key = |p: Point| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
                let a = key(triangle[i]);
                let b = key(triangle[(i + 1) % 3]);
                let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let entry = incidence.entry(edge).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += sign;
            }
        }
        assert!(incidence
            .values()
            .all(|&(count, sign)| count == 2 && sign == 0));
        let exact = (4000. - std::f64::consts::PI * 89.) * 12.;
        assert!(volume > exact && (volume - exact) / exact < 0.002);
    }

    #[test]
    fn annular_stl_is_closed_with_inward_bore_and_bounded_volume_error() {
        let t = Tolerance::default();
        let model = cad_kernel::extrude_annulus(12., 8., 40., t).unwrap().model;
        let stl = to_stl(&model, t).unwrap();
        let points: Vec<[f64; 3]> = stl
            .lines()
            .filter_map(|l| l.trim().strip_prefix("vertex "))
            .map(|l| {
                let v: Vec<f64> = l.split_whitespace().map(|n| n.parse().unwrap()).collect();
                [v[0], v[1], v[2]]
            })
            .collect();
        let mut incidence = std::collections::BTreeMap::new();
        let mut volume = 0.;
        let mut bore_triangles = 0;
        for triangle in points.as_chunks::<3>().0 {
            let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
            let center: Point =
                std::array::from_fn(|i| (triangle[0][i] + triangle[1][i] + triangle[2][i]) / 3.);
            let radius = |p: Point| p[0].hypot(p[1]);
            if triangle.iter().all(|p| (radius(*p) - 8.).abs() < 1e-10) {
                assert!(normal[0] * center[0] + normal[1] * center[1] < 0.);
                bore_triangles += 1;
            }
            // Cap triangles stay outside the inscribed bore; no fan fills it.
            if normal[2].abs() > 0. {
                assert!(radius(center) > 7.99);
            }
            volume += dot(triangle[0], cross(triangle[1], triangle[2])) / 6.;
            for i in 0..3 {
                let key = |p: Point| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
                let a = key(triangle[i]);
                let b = key(triangle[(i + 1) % 3]);
                let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let entry = incidence.entry(edge).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += sign;
            }
        }
        assert!(bore_triangles > 0);
        assert!(incidence
            .values()
            .all(|&(count, sign)| count == 2 && sign == 0));
        let exact = std::f64::consts::PI * 80. * 40.;
        assert!(volume < exact && (exact - volume) / exact < 0.002);
    }

    #[test]
    fn analytic_entities_and_mesh_mass_have_distinct_contracts() {
        let t = Tolerance::default();
        let m = cad_kernel::extrude_circle(8., 60., t).unwrap().model;
        let step = to_step(&m, t).unwrap();
        assert_eq!(step.matches("CIRCLE('").count(), 2);
        assert_eq!(step.matches("CYLINDRICAL_SURFACE('").count(), 1);
        assert_eq!(step.matches("SEAM_CURVE('").count(), 1);
        assert_eq!(step.matches("PCURVE('").count(), 2);
        assert!(!step.contains("FACETED_BREP("));
        assert_eq!(step, to_step(&m, t).unwrap());
        let stl = to_stl(&m, t).unwrap();
        let points: Vec<[f64; 3]> = stl
            .lines()
            .filter_map(|l| l.trim().strip_prefix("vertex "))
            .map(|l| {
                let v: Vec<f64> = l.split_whitespace().map(|n| n.parse().unwrap()).collect();
                [v[0], v[1], v[2]]
            })
            .collect();
        let volume: f64 = points
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| dot(p[0], cross(p[1], p[2])) / 6.)
            .sum();
        let exact = std::f64::consts::PI * 64. * 60.;
        assert!(volume < exact && (exact - volume) / exact < 0.002);
    }
}
