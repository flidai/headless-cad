//! Analytic STEP entities for the validated circular-extrusion domain.
use super::*;
use cad_kernel::{Curve, Surface};

fn placement(step: &mut Step, origin: Point, axis: Point, x: Point) -> usize {
    let p = step.add(format!("CARTESIAN_POINT('',({}))", triple(origin)));
    let z = step.add(format!("DIRECTION('',({}))", triple(axis)));
    let x = step.add(format!("DIRECTION('',({}))", triple(x)));
    step.add(format!("AXIS2_PLACEMENT_3D('',#{p},#{z},#{x})"))
}
fn seam_pcurve(step: &mut Step, surface: usize, u: f64) -> usize {
    let p = step.add(format!("CARTESIAN_POINT('',({},0.))", real(u)));
    let d = step.add("DIRECTION('',(0.,1.))".into());
    let v = step.add(format!("VECTOR('',#{d},1.)"));
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
    let geometry = model
        .cylinder_geometry(tolerance)
        .map_err(|e| ExportError::InvalidModel(e.to_string()))?
        .ok_or(ExportError::UnsupportedModel(
            "expected an analytic cylinder",
        ))?;
    let mut surfaces = Vec::new();
    for face in &model.faces {
        let surface = match &face.surface {
            Surface::Plane(p) => {
                let axis = placement(step, p.origin, p.normal, geometry.x_direction);
                step.add(format!("PLANE('',#{axis})"))
            }
            Surface::Cylinder(c) => {
                let axis = placement(step, c.origin, c.axis, c.x_direction);
                step.add(format!(
                    "CYLINDRICAL_SURFACE('',#{axis},{})",
                    real(c.radius)
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
                let wall = model
                    .faces
                    .iter()
                    .enumerate()
                    .find_map(|(i, face)| {
                        matches!(face.surface, Surface::Cylinder(_))
                            .then_some(i)
                            .filter(|_| {
                                face.loops.iter().any(|id| {
                                    model.loops[id.0]
                                        .edges
                                        .iter()
                                        .any(|u| u.edge.0 == edge_index)
                                })
                            })
                    })
                    .ok_or(ExportError::UnsupportedModel(
                        "analytic seam has no cylinder surface",
                    ))?;
                let a = seam_pcurve(step, surfaces[wall], 0.);
                let b = seam_pcurve(step, surfaces[wall], std::f64::consts::TAU);
                step.add(format!("SEAM_CURVE('',#{line},(#{a},#{b}),.CURVE_3D.)"))
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
            if matches!(&face.surface, Surface::Cylinder(c) if c.inward) {
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
