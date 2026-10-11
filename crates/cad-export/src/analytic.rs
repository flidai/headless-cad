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
    for edge in &model.edges {
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
                let a = seam_pcurve(step, surfaces[geometry.wall.0], 0.);
                let b = seam_pcurve(step, surfaces[geometry.wall.0], std::f64::consts::TAU);
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
        for loop_id in &face.loops {
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
            bounds.push(step.add(format!("FACE_BOUND('',#{boundary},.T.)")));
        }
        faces.push(step.add(format!(
            "ADVANCED_FACE('',({}),#{},.T.)",
            refs(&bounds),
            surfaces[i]
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
