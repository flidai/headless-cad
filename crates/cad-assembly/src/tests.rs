use super::*;
use cad_kernel::extrude_rectangle;

fn parts() -> BTreeMap<String, BuiltPart> {
    BTreeMap::from([(
        "plate".into(),
        BuiltPart {
            feature_id: "blank".into(),
            content_hash: "geometry-123".into(),
            result: Arc::new(extrude_rectangle(2.0, 3.0, 4.0, Tolerance::default()).unwrap()),
        },
    )])
}
fn instance(id: &str, translation: [f64; 3]) -> Instance {
    Instance {
        id: id.into(),
        part: "plate".into(),
        transform: RigidTransform {
            translation,
            ..Default::default()
        },
    }
}
fn build(parts: BTreeMap<String, BuiltPart>, instances: Vec<Instance>) -> BuiltAssembly {
    BuiltAssembly::new("fixture".into(), parts, instances, Tolerance::default()).unwrap()
}

#[test]
fn quaternion_rotations_match_independent_analytic_coordinates() {
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let transform = RigidTransform {
        translation: [10.0, 20.0, 30.0],
        rotation: [0.0, 0.0, half, half],
    };
    transform.validate().unwrap();
    for point in [[0.0; 3], [2.0, 3.0, 4.0], [-4.0, 5.0, 1.0]] {
        let expected = [10.0 - point[1], 20.0 + point[0], 30.0 + point[2]];
        for (actual, expected) in transform.point(point).into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-13);
        }
    }
    let bounds = transform
        .transformed_bounds(&Bounds {
            min: [0.0; 3],
            max: [2.0, 3.0, 4.0],
        })
        .unwrap();
    for (actual, expected) in bounds
        .min
        .into_iter()
        .zip([7.0, 20.0, 30.0])
        .chain(bounds.max.into_iter().zip([10.0, 22.0, 34.0]))
    {
        assert!((actual - expected).abs() < 1e-13);
    }
    let axis = 1.0 / 3.0_f64.sqrt();
    let angle = 0.73_f64;
    let arbitrary = RigidTransform {
        translation: [-3.0, 6.0, 2.0],
        rotation: [
            axis * (angle / 2.0).sin(),
            axis * (angle / 2.0).sin(),
            axis * (angle / 2.0).sin(),
            (angle / 2.0).cos(),
        ],
    };
    let p = [2.0, -5.0, 4.0];
    let vector = [axis; 3];
    let cross = [
        vector[1] * p[2] - vector[2] * p[1],
        vector[2] * p[0] - vector[0] * p[2],
        vector[0] * p[1] - vector[1] * p[0],
    ];
    let expected: [f64; 3] = std::array::from_fn(|i| {
        p[i] * angle.cos()
            + cross[i] * angle.sin()
            + vector[i] * dot(vector, p) * (1.0 - angle.cos())
    });
    for (actual, expected) in arbitrary.vector(p).into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-13);
    }
}

#[test]
fn instances_share_geometry_and_sum_material_including_overlaps() {
    let definitions = parts();
    let original = Arc::clone(&definitions["plate"].result);
    let assembly = build(
        definitions,
        vec![
            instance("first", [0.0; 3]),
            instance("overlapping", [0.0; 3]),
            instance("distant", [10.0, 0.0, 0.0]),
        ],
    );
    assert!(Arc::ptr_eq(&original, &assembly.parts["plate"].result));
    assert_eq!(Arc::strong_count(&original), 2);
    assert_eq!(assembly.instance_count(), 3);
    assert_eq!(assembly.solid_count(), 3);
    assert_eq!(assembly.material_volume(), 72.0);
    assert_eq!(
        assembly.bounds(),
        Some(Bounds {
            min: [0.0; 3],
            max: [12.0, 3.0, 4.0]
        })
    );
    assembly.validate(Tolerance::default()).unwrap();
    assert_eq!(original.model.bounds().min, [0.0; 3]);
}

#[test]
fn placement_changes_assembly_identity_without_changing_definition() {
    let definitions = parts();
    let a = build(
        definitions.clone(),
        vec![
            instance("first", [0.0; 3]),
            instance("second", [5.0, 0.0, 0.0]),
        ],
    );
    let b = build(
        definitions.clone(),
        vec![
            instance("second", [5.0, 0.0, 0.0]),
            instance("first", [0.0; 3]),
        ],
    );
    assert_eq!(
        a.content_hash, b.content_hash,
        "input ordering is not assembly identity"
    );
    let moved = build(
        definitions,
        vec![
            instance("first", [1.0; 3]),
            instance("second", [5.0, 0.0, 0.0]),
        ],
    );
    assert_ne!(a.content_hash, moved.content_hash);
    assert_eq!(
        a.parts["plate"].content_hash,
        moved.parts["plate"].content_hash
    );
    assert!(Arc::ptr_eq(
        &a.parts["plate"].result,
        &moved.parts["plate"].result
    ));
    let mut renamed = a.clone();
    renamed.instances[0].id = "renamed".into();
    assert!(matches!(
        renamed.validate(Tolerance::default()),
        Err(AssemblyError::DerivedMismatch)
    ));
    let mut changed = parts();
    changed.get_mut("plate").unwrap().result =
        Arc::new(extrude_rectangle(3.0, 3.0, 4.0, Tolerance::default()).unwrap());
    let changed = build(changed, a.instances.clone());
    assert_ne!(
        a.content_hash, changed.content_hash,
        "actual definition geometry is bound even if caller reused its declared hash"
    );
}

#[test]
fn strict_recipe_types_have_identity_defaults() {
    let instance: Instance =
        serde_json::from_str(r#"{"id":"one","part":"plate","transform":{"translation":[1,2,3]}}"#)
            .unwrap();
    assert_eq!(instance.transform.rotation, [0.0, 0.0, 0.0, 1.0]);
    let instance: Instance = serde_json::from_str(r#"{"id":"one","part":"plate"}"#).unwrap();
    assert_eq!(instance.transform, RigidTransform::default());
    assert!(serde_json::from_str::<Instance>(
        r#"{"id":"one","part":"plate","transform":{"scale":2}}"#
    )
    .is_err());
    assert!(serde_json::from_str::<PartDefinition>(
        r#"{"id":"plate","feature":"blank","typo":true}"#
    )
    .is_err());
    assert!(
        serde_json::from_str::<AssemblyDefinition>(r#"{"id":"a","instances":[],"typo":true}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<AssemblyOutput>(r#"{"name":"a","assembly":"a","typo":true}"#)
            .is_err()
    );
}

#[test]
fn malformed_sources_transforms_and_identity_fail_explicitly() {
    for rotation in [
        [0.0; 4],
        [0.0, 0.0, 0.0, 2.0],
        [0.0, 0.0, f64::NAN, 1.0],
        [0.0, 0.0, 0.0, 1.000001],
    ] {
        assert!(RigidTransform {
            rotation,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    assert!(RigidTransform {
        translation: [f64::INFINITY, 0.0, 0.0],
        ..Default::default()
    }
    .validate()
    .is_err());
    let mut missing = instance("one", [0.0; 3]);
    missing.part = "absent".into();
    assert!(matches!(
        BuiltAssembly::new("a".into(), parts(), vec![missing], Tolerance::default()),
        Err(AssemblyError::MissingPart { .. })
    ));
    assert!(matches!(
        BuiltAssembly::new(
            "a".into(),
            parts(),
            vec![instance("same", [0.0; 3]), instance("same", [0.0; 3])],
            Tolerance::default()
        ),
        Err(AssemblyError::DuplicateInstance(_))
    ));
    let mut invalid = parts();
    Arc::make_mut(&mut invalid.get_mut("plate").unwrap().result)
        .model
        .edges
        .clear();
    assert!(matches!(
        BuiltAssembly::new(
            "a".into(),
            invalid,
            vec![instance("one", [0.0; 3])],
            Tolerance::default()
        ),
        Err(AssemblyError::InvalidPart { .. })
    ));
    assert!(matches!(
        BuiltAssembly::new(
            "a".into(),
            parts(),
            vec![instance("far", [1e100, 0.0, 0.0])],
            Tolerance::default()
        ),
        Err(AssemblyError::NumericRange)
    ));
    let invalid_bounds = Bounds {
        min: [0.0; 3],
        max: [f64::INFINITY; 3],
    };
    assert!(RigidTransform::default()
        .transformed_bounds(&invalid_bounds)
        .is_err());
}

#[test]
fn serialization_preserves_assembly_identity_and_empty_assemblies_work() {
    let assembly = build(parts(), vec![instance("first", [0.0; 3])]);
    let restored: BuiltAssembly =
        serde_json::from_str(&serde_json::to_string(&assembly).unwrap()).unwrap();
    restored.validate(Tolerance::default()).unwrap();
    assert_eq!(restored.content_hash, assembly.content_hash);
    let empty = build(BTreeMap::new(), vec![]);
    assert_eq!(empty.material_volume(), 0.0);
    assert_eq!(empty.bounds(), None);
    assert_eq!(empty.solid_count(), 0);
}

#[test]
fn colliding_declared_definition_hashes_cannot_alias_different_geometry() {
    let mut definitions = parts();
    let mut other = definitions["plate"].clone();
    other.result = Arc::new(extrude_rectangle(5.0, 3.0, 4.0, Tolerance::default()).unwrap());
    definitions.insert("different".into(), other);
    let occurrences = vec![
        instance("one", [0.0; 3]),
        Instance {
            id: "two".into(),
            part: "different".into(),
            transform: Default::default(),
        },
    ];
    assert!(matches!(
        BuiltAssembly::new(
            "collision".into(),
            definitions,
            occurrences,
            Tolerance::default()
        ),
        Err(AssemblyError::DefinitionHashCollision(_))
    ));
    let mut identical = parts();
    identical.insert("alias".into(), identical["plate"].clone());
    let assembly = build(
        identical,
        vec![
            instance("one", [0.0; 3]),
            Instance {
                id: "two".into(),
                part: "alias".into(),
                transform: Default::default(),
            },
        ],
    );
    assembly.validate(Tolerance::default()).unwrap();
}

#[test]
fn aggregate_placement_budget_is_checked_before_per_occurrence_geometry() {
    let tolerance = Tolerance::default();
    let base = extrude_rectangle(10.0, 10.0, 2.0, tolerance).unwrap().model;
    let tool = extrude_rectangle(2.0, 2.0, 4.0, tolerance).unwrap().model;
    let tool = cad_kernel::translate(&tool, [1.0, 1.0, -1.0], tolerance)
        .unwrap()
        .model;
    let once =
        cad_kernel::boolean(&base, &tool, cad_kernel::BooleanOp::Difference, tolerance).unwrap();
    let second = cad_kernel::translate(&tool, [4.0, 4.0, 0.0], tolerance)
        .unwrap()
        .model;
    let complex = cad_kernel::boolean(
        &once.model,
        &second,
        cad_kernel::BooleanOp::Difference,
        tolerance,
    )
    .unwrap();
    let cost = complex.model.vertices.len() + complex.model.edges.len();
    let count = MAX_PLACEMENT_ENTITIES / cost + 1;
    assert!(
        count <= MAX_INSTANCES,
        "test definition must exceed placement budget within instance-count limit"
    );
    let mut definitions = parts();
    definitions.get_mut("plate").unwrap().result = Arc::new(complex);
    let instances = (0..count)
        .map(|i| instance(&format!("part-{i}"), [0.0; 3]))
        .collect();
    assert!(matches!(
        BuiltAssembly::new("oversized".into(), definitions, instances, tolerance),
        Err(AssemblyError::ResourceLimit)
    ));
}

fn definition(
    id: &str,
    instances: Vec<Instance>,
    assemblies: Vec<AssemblyInstance>,
) -> AssemblyDefinition {
    AssemblyDefinition {
        id: id.into(),
        instances,
        assemblies,
    }
}
fn child(id: &str, assembly: &str, transform: RigidTransform) -> AssemblyInstance {
    AssemblyInstance {
        id: id.into(),
        assembly: assembly.into(),
        transform,
    }
}

#[test]
fn nested_noncommuting_rotations_match_analytic_world_coordinates_and_reuse_definitions() {
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let local = RigidTransform {
        translation: [1.0, 2.0, 3.0],
        rotation: [half, 0.0, 0.0, half],
    };
    let parent = RigidTransform {
        translation: [10.0, 20.0, 30.0],
        rotation: [0.0, 0.0, half, half],
    };
    let mut leaf = instance("fastener", [0.0; 3]);
    leaf.transform = local;
    let definitions = BTreeMap::from([
        ("unit".into(), definition("unit", vec![leaf], vec![])),
        (
            "root".into(),
            definition(
                "root",
                vec![],
                vec![
                    child("left", "unit", parent),
                    child("right", "unit", RigidTransform::default()),
                ],
            ),
        ),
        ("unused".into(), definition("unused", vec![], vec![])),
    ]);
    let source = parts();
    let built = BuiltAssembly::from_definitions(
        "root".into(),
        source.clone(),
        definitions,
        Tolerance::default(),
    )
    .unwrap();
    assert_eq!(built.hierarchy.len(), 2);
    assert_eq!(built.parts.len(), 1);
    assert_eq!(built.instances.len(), 2);
    assert_eq!(built.material_volume(), 48.0);
    assert!(Arc::ptr_eq(
        &built.parts["plate"].result,
        &source["plate"].result
    ));
    let left = &built
        .instances
        .iter()
        .find(|i| built.instance_paths[&i.id][0] == "left")
        .unwrap()
        .transform;
    // Rx90 maps (2,3,4)->(2,-4,3); local translate ->(3,-2,6);
    // Rz90 ->(2,3,6); parent translate ->(12,23,36).
    for (actual, expected) in left
        .point([2.0, 3.0, 4.0])
        .into_iter()
        .zip([12.0, 23.0, 36.0])
    {
        assert!((actual - expected).abs() < 1e-12);
    }
    let bounds = built.bounds().unwrap();
    // Left rotated box bounds x[8,12], y[21,23], z[33,36].
    // Right local-only box bounds x[1,3], y[-2,2], z[3,6].
    for (actual, expected) in bounds
        .min
        .into_iter()
        .chain(bounds.max)
        .zip([1.0, -2.0, 3.0, 12.0, 23.0, 36.0])
    {
        assert!((actual - expected).abs() < 1e-12);
    }
    let roundtrip: BuiltAssembly =
        serde_json::from_slice(&serde_json::to_vec(&built).unwrap()).unwrap();
    roundtrip.validate(Tolerance::default()).unwrap();
}

#[test]
fn nested_paths_cannot_collide_with_delimiters_or_encoded_user_ids() {
    let definitions = BTreeMap::from([
        (
            "root".into(),
            definition(
                "root",
                vec![instance(r#"["a","b/c"]"#, [0.0; 3])],
                vec![
                    child("a", "first", RigidTransform::default()),
                    child("a/b", "second", RigidTransform::default()),
                ],
            ),
        ),
        (
            "first".into(),
            definition("first", vec![instance("b/c", [0.0; 3])], vec![]),
        ),
        (
            "second".into(),
            definition("second", vec![instance("c", [0.0; 3])], vec![]),
        ),
    ]);
    let built =
        BuiltAssembly::from_definitions("root".into(), parts(), definitions, Tolerance::default())
            .unwrap();
    assert_eq!(built.instances.len(), 3);
    assert_eq!(built.instance_paths.len(), 3);
    for instance in &built.instances {
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&instance.id).unwrap(),
            built.instance_paths[&instance.id]
        );
    }
    let mut damaged = built.clone();
    damaged.instances[0].transform.translation[0] += 1.0;
    assert!(matches!(
        damaged.validate(Tolerance::default()),
        Err(AssemblyError::DerivedMismatch)
    ));
    let mut damaged = built.clone();
    damaged
        .instance_paths
        .values_mut()
        .next()
        .unwrap()
        .push("forged".into());
    assert!(matches!(
        damaged.validate(Tolerance::default()),
        Err(AssemblyError::DerivedMismatch)
    ));
    let mut changed = built.hierarchy.clone();
    changed.get_mut("root").unwrap().assemblies[0].id = "renamed".into();
    let changed =
        BuiltAssembly::from_definitions("root".into(), parts(), changed, Tolerance::default())
            .unwrap();
    assert_ne!(built.content_hash, changed.content_hash);
}

#[test]
fn nested_cycles_depth_and_exponential_expansion_fail_before_geometry() {
    let known_parts = BTreeSet::from(["plate".to_owned()]);
    let mut definitions = BTreeMap::from([
        (
            "a".into(),
            definition(
                "a",
                vec![],
                vec![child("to_b", "b", RigidTransform::default())],
            ),
        ),
        (
            "b".into(),
            definition(
                "b",
                vec![],
                vec![child("to_a", "a", RigidTransform::default())],
            ),
        ),
    ]);
    assert!(matches!(
        validate_definitions(&known_parts, &definitions),
        Err(AssemblyError::Hierarchy(_))
    ));
    definitions.clear();
    for i in 0..65 {
        let id = format!("node{i:02}");
        let children = if i == 64 {
            vec![]
        } else {
            vec![child(
                "next",
                &format!("node{:02}", i + 1),
                RigidTransform::default(),
            )]
        };
        definitions.insert(id.clone(), definition(&id, vec![], children));
    }
    assert!(matches!(
        validate_definitions(&known_parts, &definitions),
        Err(AssemblyError::ResourceLimit)
    ));
    definitions.clear();
    definitions.insert(
        "node00".into(),
        definition("node00", vec![instance("leaf", [0.0; 3])], vec![]),
    );
    for i in 1..18 {
        let id = format!("node{i:02}");
        let children = ["left", "right"]
            .map(|name| {
                child(
                    name,
                    &format!("node{:02}", i - 1),
                    RigidTransform::default(),
                )
            })
            .to_vec();
        definitions.insert(id.clone(), definition(&id, vec![], children));
    }
    assert!(matches!(
        validate_definitions(&known_parts, &definitions),
        Err(AssemblyError::ResourceLimit)
    ));
}

#[test]
fn nested_duplicate_names_missing_references_and_numeric_cancellation_are_rejected() {
    let mut definitions = BTreeMap::from([
        (
            "root".into(),
            definition(
                "root",
                vec![instance("same", [0.0; 3])],
                vec![child("same", "unit", RigidTransform::default())],
            ),
        ),
        ("unit".into(), definition("unit", vec![], vec![])),
    ]);
    assert!(matches!(
        validate_definitions(&BTreeSet::from(["plate".into()]), &definitions),
        Err(AssemblyError::DuplicateInstance(_))
    ));
    definitions.get_mut("root").unwrap().instances.clear();
    definitions.remove("unit");
    assert!(matches!(
        validate_definitions(&BTreeSet::new(), &definitions),
        Err(AssemblyError::Hierarchy(_))
    ));
    let parent = RigidTransform {
        translation: [1e20, 0.0, 0.0],
        ..Default::default()
    };
    let local = RigidTransform {
        translation: [1.0, 0.0, 0.0],
        ..Default::default()
    };
    assert!(matches!(
        parent.compose(&local, Tolerance::default()),
        Err(AssemblyError::NumericRange)
    ));
    let invalid = RigidTransform {
        rotation: [0.0, 0.0, 0.0, 2.0],
        ..Default::default()
    };
    assert!(parent.compose(&invalid, Tolerance::default()).is_err());
}

#[test]
fn empty_subassembly_paths_and_rotated_offset_roundoff_have_explicit_budgets() {
    // No geometry and no leaves: expanded assembly-node path text still costs
    // resources and must be rejected before the occurrence traversal allocates.
    let mut definitions = BTreeMap::from([("n00".into(), definition("n00", vec![], vec![]))]);
    for i in 1..14 {
        let id = format!("n{i:02}");
        let children = ["a", "b"]
            .map(|prefix| {
                child(
                    &format!("{prefix}{}", "x".repeat(1000)),
                    &format!("n{:02}", i - 1),
                    RigidTransform::default(),
                )
            })
            .to_vec();
        definitions.insert(id.clone(), definition(&id, vec![], children));
    }
    assert!(matches!(
        validate_definitions(&BTreeSet::new(), &definitions),
        Err(AssemblyError::ResourceLimit)
    ));
    let half_angle = std::f64::consts::FRAC_PI_8;
    let parent = RigidTransform {
        rotation: [0.0, 0.0, half_angle.sin(), half_angle.cos()],
        ..Default::default()
    };
    let far = RigidTransform {
        translation: [1e12, 1e12, 0.0],
        ..Default::default()
    };
    assert!(matches!(
        parent.compose(&far, Tolerance::default()),
        Err(AssemblyError::NumericRange)
    ));
    // Exact identity placement does not incur arbitrary-rotation uncertainty.
    assert!(RigidTransform::default()
        .compose(&far, Tolerance::default())
        .is_ok());
}
