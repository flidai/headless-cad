//! AP214 occurrence relationships preserve reusable parts and rigid placements.
//! STL necessarily expands occurrences and carries no assembly semantics.
use super::*;
use cad_assembly::{BuiltAssembly, RigidTransform};
use std::collections::BTreeMap;

// STEP string literals use apostrophe doubling and hexadecimal Unicode escapes.
fn string(value: &str) -> String {
    let mut output = String::from("'");
    for character in value.chars() {
        match character {
            '\'' => output.push_str("''"),
            ' '..='~' if character != '\\' => output.push(character),
            _ => {
                output.push_str("\\X2\\");
                for unit in character.encode_utf16(&mut [0; 2]) {
                    write!(output, "{unit:04X}").unwrap();
                }
                output.push_str("\\X0\\");
            }
        }
    }
    output.push('\'');
    output
}

fn axis(step: &mut Step, transform: &RigidTransform) -> usize {
    let origin = step.add(format!(
        "CARTESIAN_POINT('',({}))",
        triple(transform.translation)
    ));
    let z = step.add(format!(
        "DIRECTION('',({}))",
        triple(transform.vector([0., 0., 1.]))
    ));
    let x = step.add(format!(
        "DIRECTION('',({}))",
        triple(transform.vector([1., 0., 0.]))
    ));
    step.add(format!("AXIS2_PLACEMENT_3D('',#{origin},#{z},#{x})"))
}

fn product(
    step: &mut Step,
    name: &str,
    product_context: usize,
    definition_context: usize,
) -> (usize, usize) {
    let name = string(name);
    let product = step.add(format!("PRODUCT({name},{name},'',(#{product_context}))"));
    let formation = step.add(format!("PRODUCT_DEFINITION_FORMATION('1','',#{product})"));
    let definition = step.add(format!(
        "PRODUCT_DEFINITION('design','',#{formation},#{definition_context})"
    ));
    let shape = step.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"));
    (definition, shape)
}

/// Export reusable part definitions and assembly occurrences with their placements.
/// Each part is validated in its native frame. No Boolean union is performed.
pub fn assembly_to_step(
    assembly: &BuiltAssembly,
    tolerance: Tolerance,
) -> Result<String, ExportError> {
    assembly
        .validate(tolerance)
        .map_err(|e| ExportError::InvalidModel(e.to_string()))?;
    if assembly.instances.is_empty() {
        return Err(ExportError::UnsupportedModel(
            "STEP assembly requires at least one instance",
        ));
    }
    let mut step = Step::default();
    let application = step.add("APPLICATION_CONTEXT('automotive design')".into());
    step.add(format!("APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{application})"));
    let product_context = step.add(format!("PRODUCT_CONTEXT('',#{application},'mechanical')"));
    let definition_context = step.add(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{application},'design')"
    ));
    let length = step.add("(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.))".into());
    let angle = step.add("(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.))".into());
    let solid_angle = step.add("(NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT())".into());
    let uncertainty = step.add(format!("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({}),#{length},'distance_accuracy_value','model tolerance')",real(tolerance.linear)));
    let context = step.add(format!("(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{uncertainty})) GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length},#{angle},#{solid_angle})) REPRESENTATION_CONTEXT('','3D'))"));
    let identity = axis(&mut step, &RigidTransform::default());
    let placements: Vec<_> = assembly
        .instances
        .iter()
        .map(|instance| axis(&mut step, &instance.transform))
        .collect();
    let (root_definition, root_shape) = product(
        &mut step,
        &assembly.assembly_id,
        product_context,
        definition_context,
    );
    let mut root_items = vec![identity];
    root_items.extend(&placements);
    let root_representation = step.add(format!(
        "SHAPE_REPRESENTATION('',({}),#{context})",
        refs(&root_items)
    ));
    step.add(format!(
        "SHAPE_DEFINITION_REPRESENTATION(#{root_shape},#{root_representation})"
    ));
    let mut definitions = BTreeMap::new();
    let mut geometry = BTreeMap::new();
    for (name, part) in &assembly.parts {
        let (definition, shape) = product(&mut step, name, product_context, definition_context);
        let representation = if let Some(&cached) = geometry.get(&part.content_hash) {
            cached
        } else {
            let model = &part.result.model;
            if model.solids.is_empty() {
                return Err(ExportError::UnsupportedModel(
                    "STEP assembly parts must have at least one solid",
                ));
            }
            let polygons = polygons(model, tolerance)?;
            let mut items = vec![identity];
            items.extend(step_solids(&mut step, model, polygons));
            let representation = step.add(format!(
                "FACETED_BREP_SHAPE_REPRESENTATION('',({}),#{context})",
                refs(&items)
            ));
            geometry.insert(part.content_hash.clone(), representation);
            representation
        };
        step.add(format!(
            "SHAPE_DEFINITION_REPRESENTATION(#{shape},#{representation})"
        ));
        definitions.insert(name, (definition, representation));
    }
    for (instance, placement) in assembly.instances.iter().zip(placements) {
        let (definition, representation) = definitions[&instance.part];
        let name = string(&instance.id);
        let occurrence = step.add(format!("NEXT_ASSEMBLY_USAGE_OCCURRENCE({name},{name},'',#{root_definition},#{definition},{name})"));
        let usage_shape = step.add(format!(
            "PRODUCT_DEFINITION_SHAPE('Placement','',#{occurrence})"
        ));
        let transformation = step.add(format!(
            "ITEM_DEFINED_TRANSFORMATION('','',#{identity},#{placement})"
        ));
        let relationship = step.add(format!("(REPRESENTATION_RELATIONSHIP('','',#{representation},#{root_representation}) REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{transformation}) SHAPE_REPRESENTATION_RELATIONSHIP())"));
        step.add(format!(
            "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{relationship},#{usage_shape})"
        ));
    }
    let mut output = String::from("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('headless-cad instanced planar assembly'),'2;1');\nFILE_NAME('assembly.step','1970-01-01T00:00:00',(''),(''),'headless-cad','headless-cad','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n");
    for (index, entity) in step.entities.iter().enumerate() {
        writeln!(output, "#{}={entity};", index + 1).unwrap();
    }
    output.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(output)
}

/// STL expands instances into placed triangles. Overlaps remain overlaps and
/// must not be interpreted as a fused, manufacturable single solid.
pub fn assembly_to_stl(
    assembly: &BuiltAssembly,
    tolerance: Tolerance,
) -> Result<String, ExportError> {
    assembly
        .validate(tolerance)
        .map_err(|e| ExportError::InvalidModel(e.to_string()))?;
    let mut local = BTreeMap::new();
    for (name, part) in &assembly.parts {
        local.insert(name, polygons(&part.result.model, tolerance)?);
    }
    let mut output = String::new();
    for instance in &assembly.instances {
        let label: String = instance
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        writeln!(output, "solid {label}").unwrap();
        for polygon in &local[&instance.part] {
            let normal = instance.transform.vector(polygon.normal);
            for i in 1..polygon.points.len() - 1 {
                writeln!(
                    output,
                    "  facet normal {} {} {}\n    outer loop",
                    real(normal[0]),
                    real(normal[1]),
                    real(normal[2])
                )
                .unwrap();
                for point in [polygon.points[0], polygon.points[i], polygon.points[i + 1]] {
                    let p = instance.transform.point(point);
                    if p.iter().any(|v| !v.is_finite()) {
                        return Err(ExportError::InvalidModel(
                            "placed vertex is nonfinite".into(),
                        ));
                    }
                    writeln!(
                        output,
                        "      vertex {} {} {}",
                        real(p[0]),
                        real(p[1]),
                        real(p[2])
                    )
                    .unwrap();
                }
                output.push_str("    endloop\n  endfacet\n");
            }
        }
        writeln!(output, "endsolid {label}").unwrap();
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_assembly::{BuiltPart, Instance};
    use std::sync::Arc;

    fn two_instances() -> BuiltAssembly {
        let result =
            Arc::new(cad_kernel::extrude_rectangle(2., 3., 4., Tolerance::default()).unwrap());
        BuiltAssembly::new(
            "fixture's assembly".into(),
            BTreeMap::from([(
                "block".into(),
                BuiltPart {
                    feature_id: "box".into(),
                    content_hash: "block-geometry".into(),
                    result,
                },
            )]),
            vec![
                Instance {
                    id: "first".into(),
                    part: "block".into(),
                    transform: RigidTransform::default(),
                },
                Instance {
                    id: "second".into(),
                    part: "block".into(),
                    transform: RigidTransform {
                        translation: [10., 0., 0.],
                        rotation: [0., 0., 0.3826834323650898, 0.9238795325112867],
                    },
                },
            ],
            Tolerance::default(),
        )
        .unwrap()
    }

    #[test]
    fn step_preserves_definition_reuse_and_named_occurrences() {
        let assembly = two_instances();
        let step = assembly_to_step(&assembly, Tolerance::default()).unwrap();
        assert_eq!(
            step,
            assembly_to_step(&assembly, Tolerance::default()).unwrap()
        );
        assert_eq!(step.matches("FACETED_BREP('").count(), 1);
        assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 2);
        assert_eq!(
            step.matches("CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(")
                .count(),
            2
        );
        assert!(step.contains("fixture''s assembly"));
        assert!(step.contains("NEXT_ASSEMBLY_USAGE_OCCURRENCE('second','second'"));
    }

    #[test]
    fn stl_expands_rigid_occurrences_with_expected_volume_and_bounds() {
        let output = assembly_to_stl(&two_instances(), Tolerance::default()).unwrap();
        let points: Vec<[f64; 3]> = output
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some("vertex"))
                    .then(|| std::array::from_fn(|_| words.next().unwrap().parse().unwrap()))
            })
            .collect();
        assert_eq!(output.matches("facet normal").count(), 24);
        let volume: f64 = points
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| dot(t[0], cross(t[1], t[2])) / 6.)
            .sum();
        assert!((volume - 48.).abs() < 1e-10);
        let max: [f64; 3] = std::array::from_fn(|axis| {
            points
                .iter()
                .map(|p| p[axis])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        assert!((max[0] - (10. + 2. * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-12);
        assert!((max[1] - 5. * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert_eq!(max[2], 4.);
    }

    #[test]
    fn string_literals_cannot_inject_step_entities() {
        assert_eq!(string("a'b"), "'a''b'");
        assert_eq!(string("ä"), "'\\X2\\00E4\\X0\\'");
        assert_eq!(string("\\"), "'\\X2\\005C\\X0\\'");
        assert!(!string("\n#1=PRODUCT('injected');").contains('\n'));
    }
}
