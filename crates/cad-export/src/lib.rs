//! Deterministic exports for the kernel's closed, planar polygon B-reps.
//!
//! STL is a unitless triangle format; coordinates are written in millimetres.
//! STEP is a narrow AP214 faceted B-rep writer, with explicit plane surfaces,
//! polygon boundaries, product context, and millimetre units. This is not an
//! arbitrary analytic/NURBS STEP translator. Both exporters reject invalid
//! models and non-convex face loops rather than silently tessellating them wrong.

use cad_kernel::{ModelSnapshot, Tolerance};
use std::fmt::Write;

mod assembly;
mod bom;
pub use assembly::{assembly_to_step, assembly_to_stl};
pub use bom::bom_to_csv;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("cannot export an invalid bill of materials: {0}")]
    InvalidBom(String),
    #[error("cannot export an invalid model: {0}")]
    InvalidModel(String),
    #[error("model is outside export scope: {0}")]
    UnsupportedModel(&'static str),
    #[error("face {face} is outside export scope: {reason}")]
    UnsupportedFace { face: usize, reason: &'static str },
}

type Point = [f64; 3];

struct Polygon {
    indices: Vec<usize>,
    points: Vec<Point>,
    normal: Point,
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalized(a: Point) -> Option<Point> {
    let length = a[0].hypot(a[1]).hypot(a[2]);
    (length.is_finite() && length > 0.0).then(|| a.map(|v| v / length))
}

// The shortest round-tripping decimal preserves the geometry. Appending a
// decimal point distinguishes STEP REALs from INTEGERs without rounding small
// dimensions to zero. Normalize negative zero for stable artifacts.
fn real(value: f64) -> String {
    let mut value = if value == 0.0 {
        "0".to_owned()
    } else {
        value.to_string()
    };
    if !value.contains('.') && !value.contains('e') && !value.contains('E') {
        value.push('.');
    }
    value
}

fn triple(point: Point) -> String {
    point.map(real).join(",")
}

fn polygons(model: &ModelSnapshot, tolerance: Tolerance) -> Result<Vec<Polygon>, ExportError> {
    let validation = model.validate(tolerance);
    if !validation.valid {
        return Err(ExportError::InvalidModel(format!(
            "{:?}",
            validation.issues
        )));
    }
    if model.solids.iter().any(|solid| solid.shells.len() != 1) {
        return Err(ExportError::UnsupportedModel(
            "each solid must have one outer shell; cavity shells are not supported",
        ));
    }
    model
        .faces
        .iter()
        .enumerate()
        .map(|(face_index, face)| {
            let unsupported = |reason| ExportError::UnsupportedFace {
                face: face_index,
                reason,
            };
            if face.loops.len() != 1 {
                return Err(unsupported("faces with holes are not supported"));
            }
            let indices: Vec<usize> = model.loops[face.loops[0].0]
                .edges
                .iter()
                .map(|edge_use| {
                    model.edges[edge_use.edge.0].vertices[usize::from(edge_use.reversed)].0
                })
                .collect();
            let points: Vec<Point> = indices
                .iter()
                .map(|&index| model.vertices[index].position)
                .collect();
            if points.len() < 3 {
                return Err(unsupported("fewer than three boundary points"));
            }
            let normal =
                normalized(face.plane.normal).ok_or_else(|| unsupported("invalid plane normal"))?;
            // A fan is valid only for convex, consistently wound polygons. Check
            // every point against every directed boundary edge, including nonlocal
            // points, rather than relying only on consecutive turn directions.
            for index in 0..points.len() {
                let a = points[index];
                let b = points[(index + 1) % points.len()];
                let edge =
                    normalized(sub(b, a)).ok_or_else(|| unsupported("degenerate boundary edge"))?;
                for point in &points {
                    let side = dot(cross(edge, sub(*point, a)), normal);
                    if !side.is_finite() || side < -tolerance.linear {
                        return Err(unsupported("non-convex or inconsistent boundary winding"));
                    }
                }
            }
            for triangle in points[1..].windows(2) {
                let a = normalized(sub(triangle[0], points[0]))
                    .ok_or_else(|| unsupported("degenerate triangle"))?;
                let b = normalized(sub(triangle[1], points[0]))
                    .ok_or_else(|| unsupported("degenerate triangle"))?;
                if dot(cross(a, b), normal) <= 0.0 {
                    return Err(unsupported(
                        "degenerate triangle or inconsistent boundary winding",
                    ));
                }
            }
            Ok(Polygon {
                indices,
                points,
                normal,
            })
        })
        .collect()
}

/// Export a validated solid as deterministic ASCII STL, in millimetres.
pub fn to_stl(model: &ModelSnapshot, tolerance: Tolerance) -> Result<String, ExportError> {
    let polygons = polygons(model, tolerance)?;
    let mut output = String::from("solid headless_cad\n");
    for polygon in polygons {
        for index in 1..polygon.points.len() - 1 {
            let [a, b, c] = [
                polygon.points[0],
                polygon.points[index],
                polygon.points[index + 1],
            ];
            writeln!(
                output,
                "  facet normal {} {} {}",
                real(polygon.normal[0]),
                real(polygon.normal[1]),
                real(polygon.normal[2])
            )
            .unwrap();
            output.push_str("    outer loop\n");
            for point in [a, b, c] {
                writeln!(
                    output,
                    "      vertex {} {} {}",
                    real(point[0]),
                    real(point[1]),
                    real(point[2])
                )
                .unwrap();
            }
            output.push_str("    endloop\n  endfacet\n");
        }
    }
    output.push_str("endsolid headless_cad\n");
    Ok(output)
}

#[derive(Default)]
struct Step {
    entities: Vec<String>,
}

impl Step {
    fn add(&mut self, entity: String) -> usize {
        self.entities.push(entity);
        self.entities.len()
    }
}

fn refs(ids: &[usize]) -> String {
    ids.iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn step_solids(step: &mut Step, model: &ModelSnapshot, polygons: Vec<Polygon>) -> Vec<usize> {
    let points: Vec<usize> = model
        .vertices
        .iter()
        .map(|vertex| step.add(format!("CARTESIAN_POINT('',({}))", triple(vertex.position))))
        .collect();
    let mut faces = Vec::new();
    for polygon in polygons {
        let point_ids: Vec<usize> = polygon.indices.iter().map(|&index| points[index]).collect();
        let loop_id = step.add(format!("POLY_LOOP('',({}))", refs(&point_ids)));
        let bound = step.add(format!("FACE_OUTER_BOUND('',#{loop_id},.T.)"));
        let normal = step.add(format!("DIRECTION('',({}))", triple(polygon.normal)));
        let x_direction =
            normalized(sub(polygon.points[1], polygon.points[0])).expect("validated edge");
        let reference = step.add(format!("DIRECTION('',({}))", triple(x_direction)));
        let placement = step.add(format!(
            "AXIS2_PLACEMENT_3D('',#{},#{normal},#{reference})",
            point_ids[0]
        ));
        let plane = step.add(format!("PLANE('',#{placement})"));
        faces.push(step.add(format!("FACE_SURFACE('',(#{bound}),#{plane},.T.)")));
    }
    let mut solids = Vec::new();
    for solid in &model.solids {
        let shell_faces: Vec<usize> = model.shells[solid.shells[0].0]
            .faces
            .iter()
            .map(|face| faces[face.0])
            .collect();
        let shell = step.add(format!("CLOSED_SHELL('',({}))", refs(&shell_faces)));
        solids.push(step.add(format!("FACETED_BREP('part',#{shell})")));
    }
    solids
}

/// Export a validated convex-polygon solid as an AP214 faceted B-rep.
///
/// The fixed header timestamp is intentional: exports have no clock dependency.
pub fn to_step(model: &ModelSnapshot, tolerance: Tolerance) -> Result<String, ExportError> {
    let polygons = polygons(model, tolerance)?;
    if model.solids.is_empty() {
        return Err(ExportError::UnsupportedModel(
            "STEP export requires at least one solid; the result is empty",
        ));
    }
    let mut step = Step::default();
    let application = step.add("APPLICATION_CONTEXT('automotive design')".into());
    step.add(format!("APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{application})"));
    let product_context = step.add(format!("PRODUCT_CONTEXT('',#{application},'mechanical')"));
    let product = step.add(format!(
        "PRODUCT('part','part','headless-cad generated part',(#{product_context}))"
    ));
    let formation = step.add(format!("PRODUCT_DEFINITION_FORMATION('1','',#{product})"));
    let definition_context = step.add(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{application},'design')"
    ));
    let definition = step.add(format!(
        "PRODUCT_DEFINITION('design','',#{formation},#{definition_context})"
    ));
    let definition_shape = step.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"));
    let length_unit = step.add("(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.))".into());
    let angle_unit = step.add("(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.))".into());
    let solid_angle_unit =
        step.add("(NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT())".into());
    let uncertainty = step.add(format!("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({}),#{length_unit},'distance_accuracy_value','model tolerance')", real(tolerance.linear)));
    let context = step.add(format!("(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{uncertainty})) GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length_unit},#{angle_unit},#{solid_angle_unit})) REPRESENTATION_CONTEXT('','3D'))"));

    let solids = step_solids(&mut step, model, polygons);
    let representation = step.add(format!(
        "FACETED_BREP_SHAPE_REPRESENTATION('',({}),#{context})",
        refs(&solids)
    ));
    step.add(format!(
        "SHAPE_DEFINITION_REPRESENTATION(#{definition_shape},#{representation})"
    ));
    let mut output = String::from("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('headless-cad planar faceted B-rep'),'2;1');\nFILE_NAME('part.step','1970-01-01T00:00:00',(''),(''),'headless-cad','headless-cad','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n");
    for (index, entity) in step.entities.iter().enumerate() {
        writeln!(output, "#{}={entity};", index + 1).unwrap();
    }
    output.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_kernel::extrude_rectangle;
    use std::collections::BTreeMap;

    fn plate() -> ModelSnapshot {
        extrude_rectangle(80.0, 40.0, 6.0, Tolerance::default())
            .unwrap()
            .model
    }

    #[test]
    fn stl_has_outward_closed_triangles_and_correct_volume() {
        let model = plate();
        let output = to_stl(&model, Tolerance::default()).unwrap();
        assert_eq!(output, to_stl(&model, Tolerance::default()).unwrap());
        let mut normals = Vec::new();
        let mut vertices = Vec::new();
        for line in output.lines() {
            let words: Vec<_> = line.split_whitespace().collect();
            let coords = if words.first() == Some(&"vertex") {
                Some((&words[1..], false))
            } else if words.first() == Some(&"facet") {
                Some((&words[2..], true))
            } else {
                None
            };
            if let Some((coords, is_normal)) = coords {
                let point = [
                    coords[0].parse::<f64>().unwrap(),
                    coords[1].parse().unwrap(),
                    coords[2].parse().unwrap(),
                ];
                if is_normal {
                    normals.push(point);
                } else {
                    vertices.push(point);
                }
            }
        }
        assert_eq!(normals.len(), 12);
        assert_eq!(vertices.len(), 36);
        let mut volume = 0.0;
        let mut incidence = BTreeMap::new();
        for (triangle, normal) in vertices.as_chunks::<3>().0.iter().zip(normals) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            let actual_normal = normalized(cross(sub(b, a), sub(c, a))).unwrap();
            assert!(dot(actual_normal, normal) > 1.0 - 1e-12);
            let centroid = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            assert!(dot(normal, sub(centroid, [40.0, 20.0, 3.0])) > 0.0);
            volume += dot(a, cross(b, c)) / 6.0;
            for (a, b) in [(a, b), (b, c), (c, a)] {
                let a = a.map(f64::to_bits);
                let b = b.map(f64::to_bits);
                let entry = incidence.entry((a.min(b), a.max(b))).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!((volume - 19200.0).abs() < 1e-9);
        assert!(incidence.values().all(|value| *value == (2, 0)));
    }

    #[test]
    fn step_has_closed_planar_brep_and_complete_references() {
        let output = to_step(&plate(), Tolerance::default()).unwrap();
        assert_eq!(output, to_step(&plate(), Tolerance::default()).unwrap());
        assert!(output.starts_with("ISO-10303-21;\n"));
        assert!(output.ends_with("END-ISO-10303-21;\n"));
        assert_eq!(output.matches("=POLY_LOOP(").count(), 6);
        assert_eq!(output.matches("=PLANE(").count(), 6);
        assert_eq!(output.matches("=FACE_SURFACE(").count(), 6);
        assert_eq!(output.matches("=CLOSED_SHELL(").count(), 1);
        assert_eq!(output.matches("=FACETED_BREP(").count(), 1);
        assert_eq!(output.matches("=CARTESIAN_POINT(").count(), 8);
        assert!(output.contains("SI_UNIT(.MILLI.,.METRE.)"));
        assert!(output.contains("SHAPE_DEFINITION_REPRESENTATION("));
        assert!(output.contains("CARTESIAN_POINT('',(80.,40.,6.))"));
        let entities: BTreeMap<usize, &str> = output
            .lines()
            .filter(|line| line.starts_with('#'))
            .map(|line| {
                let (id, body) = line.split_once('=').unwrap();
                (id[1..].parse().unwrap(), body)
            })
            .collect();
        for body in entities.values() {
            for reference in body.split('#').skip(1) {
                let id: String = reference.chars().take_while(char::is_ascii_digit).collect();
                assert!(
                    entities.contains_key(&id.parse::<usize>().unwrap()),
                    "unresolved reference #{id}"
                );
            }
        }
    }

    #[test]
    fn both_formats_reject_invalid_topology_without_panicking() {
        let mut model = plate();
        model.edges[0].vertices[0].0 = usize::MAX;
        assert!(matches!(
            to_stl(&model, Tolerance::default()),
            Err(ExportError::InvalidModel(_))
        ));
        assert!(matches!(
            to_step(&model, Tolerance::default()),
            Err(ExportError::InvalidModel(_))
        ));
    }

    #[test]
    fn coordinates_are_not_rounded_to_fixed_decimal_places() {
        assert_eq!(real(-0.0), "0.");
        for value in [1e-100, 1e100, 0.12345678901234567] {
            assert_eq!(real(value).parse::<f64>().unwrap(), value);
        }
    }

    #[test]
    fn multiple_solids_remain_separate_in_step_and_all_are_meshed() {
        let tolerance = Tolerance::default();
        let a = extrude_rectangle(2.0, 3.0, 4.0, tolerance).unwrap().model;
        let b = cad_kernel::translate(&a, [5.0, 0.0, 0.0], tolerance)
            .unwrap()
            .model;
        let pair = cad_kernel::boolean(&a, &b, cad_kernel::BooleanOp::Union, tolerance)
            .unwrap()
            .model;
        let step = to_step(&pair, tolerance).unwrap();
        assert_eq!(step.matches("=CLOSED_SHELL(").count(), 2);
        assert_eq!(step.matches("=FACETED_BREP(").count(), 2);
        assert_eq!(
            to_stl(&pair, tolerance)
                .unwrap()
                .matches("facet normal")
                .count(),
            24
        );
    }

    #[test]
    fn empty_result_has_empty_stl_but_explicit_step_error() {
        let tolerance = Tolerance::default();
        let a = plate();
        let empty = cad_kernel::boolean(&a, &a, cad_kernel::BooleanOp::Difference, tolerance)
            .unwrap()
            .model;
        assert_eq!(
            to_stl(&empty, tolerance).unwrap(),
            "solid headless_cad\nendsolid headless_cad\n"
        );
        assert!(matches!(
            to_step(&empty, tolerance),
            Err(ExportError::UnsupportedModel(_))
        ));
    }
}
