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
    namespace: &str,
    product_context: usize,
    definition_context: usize,
) -> (usize, usize) {
    let identifier = string(&format!("{namespace}:{name}"));
    let name = string(name);
    let product = step.add(format!(
        "PRODUCT({identifier},{name},'',(#{product_context}))"
    ));
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
    // Keep definitions distinct from occurrences. Each assembly representation
    // contains only its own local placements, never composed leaf placements.
    let mut assemblies = BTreeMap::new();
    let mut placements = BTreeMap::new();
    let mut resolved = BTreeMap::new();
    for (name, definition) in &assembly.hierarchy {
        let leaves =
            cad_assembly::resolve_definition_instances(definition, &assembly.parts, tolerance)
                .map_err(|error| ExportError::InvalidModel(error.to_string()))?;
        let local: Vec<_> = leaves
            .iter()
            .map(|i| &i.transform)
            .chain(definition.assemblies.iter().map(|i| &i.transform))
            .map(|transform| axis(&mut step, transform))
            .collect();
        let (product, shape) = product(
            &mut step,
            name,
            "assembly",
            product_context,
            definition_context,
        );
        let mut items = vec![identity];
        items.extend(&local);
        let representation = step.add(format!(
            "SHAPE_REPRESENTATION('',({}),#{context})",
            refs(&items)
        ));
        step.add(format!(
            "SHAPE_DEFINITION_REPRESENTATION(#{shape},#{representation})"
        ));
        assemblies.insert(name, (product, representation));
        placements.insert(name, local);
        resolved.insert(name, leaves);
    }
    let mut definitions = BTreeMap::new();
    let mut geometry = BTreeMap::new();
    for (name, part) in &assembly.parts {
        let (definition, shape) =
            product(&mut step, name, "part", product_context, definition_context);
        let representation = if let Some(&cached) = geometry.get(&part.content_hash) {
            cached
        } else {
            let model = &part.result.model;
            if model.solids.is_empty() {
                return Err(ExportError::UnsupportedModel(
                    "STEP assembly parts must have at least one solid",
                ));
            }
            let mut items = vec![identity];
            let kind = if model.has_curved_geometry() {
                items.extend(crate::analytic::solids(&mut step, model, tolerance)?);
                "ADVANCED_BREP_SHAPE_REPRESENTATION"
            } else {
                items.extend(step_solids(&mut step, model, polygons(model, tolerance)?));
                "FACETED_BREP_SHAPE_REPRESENTATION"
            };
            let representation = step.add(format!("{kind}('',({}),#{context})", refs(&items)));
            geometry.insert(part.content_hash.clone(), representation);
            representation
        };
        step.add(format!(
            "SHAPE_DEFINITION_REPRESENTATION(#{shape},#{representation})"
        ));
        definitions.insert(name, (definition, representation));
    }
    for (parent, definition) in &assembly.hierarchy {
        let (parent_definition, parent_representation) = assemblies[parent];
        let children = resolved[parent]
            .iter()
            .map(|i| (&i.id, definitions[&i.part]))
            .chain(
                definition
                    .assemblies
                    .iter()
                    .map(|i| (&i.id, assemblies[&i.assembly])),
            );
        for ((name, (child_definition, child_representation)), placement) in
            children.zip(&placements[parent])
        {
            let name = string(name);
            let occurrence = step.add(format!("NEXT_ASSEMBLY_USAGE_OCCURRENCE({name},{name},'',#{parent_definition},#{child_definition},{name})"));
            let usage_shape = step.add(format!(
                "PRODUCT_DEFINITION_SHAPE('Placement','',#{occurrence})"
            ));
            let transformation = step.add(format!(
                "ITEM_DEFINED_TRANSFORMATION('','',#{identity},#{placement})"
            ));
            let relationship = step.add(format!("(REPRESENTATION_RELATIONSHIP('','',#{child_representation},#{parent_representation}) REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{transformation}) SHAPE_REPRESENTATION_RELATIONSHIP())"));
            step.add(format!(
                "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{relationship},#{usage_shape})"
            ));
        }
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
            for triangle in &polygon.triangles {
                writeln!(
                    output,
                    "  facet normal {} {} {}\n    outer loop",
                    real(normal[0]),
                    real(normal[1]),
                    real(normal[2])
                )
                .unwrap();
                for &point in triangle {
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
                    datums: Default::default(),
                    metadata: Default::default(),
                    feature_id: "box".into(),
                    content_hash: "block-geometry".into(),
                    result,
                },
            )]),
            vec![
                Instance {
                    attachment: None,
                    id: "first".into(),
                    part: "block".into(),
                    transform: RigidTransform::default(),
                },
                Instance {
                    attachment: None,
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
    fn step_retains_shared_nested_definitions_and_local_noncommuting_rotations() {
        use cad_assembly::{AssemblyDefinition, AssemblyInstance};
        let flat = two_instances();
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let local = RigidTransform {
            translation: [1., 2., 3.],
            rotation: [q, 0., 0., q],
        };
        let root = AssemblyDefinition {
            id: "root".into(),
            instances: vec![],
            assemblies: vec![
                AssemblyInstance {
                    id: "left".into(),
                    assembly: "block".into(),
                    transform: RigidTransform::default(),
                },
                AssemblyInstance {
                    id: "right".into(),
                    assembly: "block".into(),
                    transform: RigidTransform {
                        translation: [20., 0., 0.],
                        rotation: [0., q, 0., q],
                    },
                },
            ],
        };
        // Deliberately share the textual ID with a part: they occupy different namespaces.
        let module = AssemblyDefinition {
            id: "block".into(),
            instances: vec![Instance {
                attachment: None,
                id: "leaf".into(),
                part: "block".into(),
                transform: local,
            }],
            assemblies: vec![],
        };
        let assembly = BuiltAssembly::from_definitions(
            "root".into(),
            flat.parts,
            BTreeMap::from([("root".into(), root), ("block".into(), module)]),
            Tolerance::default(),
        )
        .unwrap();
        let step = assembly_to_step(&assembly, Tolerance::default()).unwrap();
        assert_eq!(
            step,
            assembly_to_step(&assembly, Tolerance::default()).unwrap()
        );
        assert_eq!(step.matches("PRODUCT(").count(), 3);
        assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 3);
        assert_eq!(step.matches("FACETED_BREP('").count(), 1);
        assert!(step.contains("PRODUCT('assembly:block','block'"));
        assert!(step.contains("PRODUCT('part:block','block'"));
        assert_eq!(
            step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE('leaf'")
                .count(),
            1
        );
        assert!(step.contains("CARTESIAN_POINT('',(1.,2.,3.))"));
        assert!(step.contains("CARTESIAN_POINT('',(20.,0.,0.))"));
        assert_eq!(assembly.instance_count(), 2);
        assert!((assembly.material_volume() - 48.).abs() < 1e-12);
        let second = &assembly.instances[1];
        let placed = second.transform.point([2., 3., 4.]);
        for (actual, expected) in placed.into_iter().zip([26., -2., -3.]) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn step_uses_resolved_datum_placement_instead_of_source_offset() {
        use cad_assembly::DatumAttachment;
        let tolerance = Tolerance::default();
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let part = |width, depth, height, hash: &str, datum| BuiltPart {
            feature_id: hash.into(),
            content_hash: hash.into(),
            metadata: Default::default(),
            datums: BTreeMap::from([("mount".into(), datum)]),
            result: Arc::new(
                cad_kernel::extrude_rectangle(width, depth, height, tolerance).unwrap(),
            ),
        };
        let parts = BTreeMap::from([
            (
                "anchor".into(),
                part(
                    1.,
                    1.,
                    1.,
                    "anchor",
                    RigidTransform {
                        translation: [0., 2., 0.],
                        rotation: [0., q, 0., q],
                    },
                ),
            ),
            (
                "child".into(),
                part(
                    2.,
                    3.,
                    4.,
                    "child",
                    RigidTransform {
                        translation: [1., 0., 0.],
                        rotation: [q, 0., 0., q],
                    },
                ),
            ),
        ]);
        let assembly = BuiltAssembly::new(
            "datums".into(),
            parts,
            vec![
                Instance {
                    id: "target".into(),
                    part: "anchor".into(),
                    attachment: None,
                    transform: RigidTransform {
                        translation: [10., 20., 30.],
                        rotation: [0., 0., q, q],
                    },
                },
                Instance {
                    id: "attached".into(),
                    part: "child".into(),
                    attachment: Some(DatumAttachment {
                        instance: "target".into(),
                        datum: "mount".into(),
                        target_datum: "mount".into(),
                    }),
                    transform: RigidTransform {
                        translation: [0., 0., 3.],
                        ..Default::default()
                    },
                },
            ],
            tolerance,
        )
        .unwrap();
        let step = assembly_to_step(&assembly, tolerance).unwrap();
        // Independent equation gives p_world = [8-z, 23-y, 31-x]. The
        // attachment offset [0,0,3] is not the occurrence's resolved origin.
        let origins: Vec<[f64; 3]> = step
            .lines()
            .filter_map(|line| {
                let (_, tail) = line.split_once("CARTESIAN_POINT('',(")?;
                let values: Vec<f64> = tail
                    .trim_end_matches("));")
                    .split(',')
                    .map(|v| v.parse().unwrap())
                    .collect();
                Some([values[0], values[1], values[2]])
            })
            .collect();
        assert!(origins.iter().any(|origin| origin
            .iter()
            .zip([8., 23., 31.])
            .all(|(a, b)| (a - b).abs() < 1e-12)));
        assert!(!origins.contains(&[0., 0., 3.]));
        let attached = assembly
            .instances
            .iter()
            .find(|i| i.id == "attached")
            .unwrap();
        for point in [[0., 0., 0.], [2., 3., 4.]] {
            let wanted = [8. - point[2], 23. - point[1], 31. - point[0]];
            assert!(attached
                .transform
                .point(point)
                .into_iter()
                .zip(wanted)
                .all(|(a, b)| (a - b).abs() < 1e-12));
        }
        assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 2);
        assert!((assembly.material_volume() - 25.).abs() < 1e-12);
    }

    #[test]
    fn string_literals_cannot_inject_step_entities() {
        assert_eq!(string("a'b"), "'a''b'");
        assert_eq!(string("ä"), "'\\X2\\00E4\\X0\\'");
        assert_eq!(string("\\"), "'\\X2\\005C\\X0\\'");
        assert!(!string("\n#1=PRODUCT('injected');").contains('\n'));
    }
}
