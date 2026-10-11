use super::*;

fn recipe() -> Recipe {
    serde_json::from_str(
        r#"{
      "schema_version":1,"units":"mm",
      "features":[
        {"op":"rectangle","id":"outline","width":80,"depth":40},
        {"op":"extrude","id":"plate","profile":"outline","height":6}
      ],
      "outputs":[{"name":"plate","feature":"plate"}]
    }"#,
    )
    .unwrap()
}

#[test]
fn builds_dependency_order_independently() {
    let mut input = recipe();
    let first = build(&input).unwrap();
    input.features.reverse();
    let second = build(&input).unwrap();
    assert_eq!(first.feature_hashes, second.feature_hashes);
    assert!((second.outputs["plate"].result.model.volume() - 19200.0).abs() < 1e-8);
}

#[test]
fn cache_invalidates_inputs_and_policy_but_not_names() {
    let mut engine = BuildEngine::default();
    let mut input = recipe();
    assert_eq!(engine.build(&input).unwrap().stats.computed_solids, 1);
    assert_eq!(engine.build(&input).unwrap().stats.cache_hits, 1);
    if let Feature::Rectangle { id, .. } = &mut input.features[0] {
        *id = "renamed".into();
    }
    if let Feature::Extrude { profile, .. } = &mut input.features[1] {
        *profile = "renamed".into();
    }
    let renamed = engine.build(&input).unwrap();
    assert_eq!(renamed.stats.cache_hits, 1);
    assert_eq!(
        renamed.outputs["plate"].provenance.input_features[0],
        "renamed"
    );
    if let Feature::Rectangle { width, .. } = &mut input.features[0] {
        *width = 81.0;
    }
    assert_eq!(engine.build(&input).unwrap().stats.computed_solids, 1);
    input.tolerance = Some(Tolerance {
        linear: Tolerance::default().linear * 2.0,
    });
    assert_eq!(engine.build(&input).unwrap().stats.computed_solids, 1);
    if let Feature::Extrude { height, .. } = &mut input.features[1] {
        *height = 7.0;
    }
    assert_eq!(engine.build(&input).unwrap().stats.computed_solids, 1);
}

#[test]
fn identical_solids_share_geometry_but_keep_logical_provenance() {
    let mut input = recipe();
    input.features.push(Feature::Rectangle {
        id: "other_outline".into(),
        width: 80.0,
        depth: 40.0,
        op_version: 1,
    });
    input.features.push(Feature::Extrude {
        id: "other_plate".into(),
        profile: "other_outline".into(),
        height: 6.0,
        op_version: 1,
    });
    input.outputs.push(Output {
        name: "other".into(),
        feature: "other_plate".into(),
    });
    let result = build(&input).unwrap();
    assert_eq!(result.stats.computed_solids, 1);
    assert_eq!(result.stats.cache_hits, 1);
    assert!(Arc::ptr_eq(
        &result.outputs["plate"].result,
        &result.outputs["other"].result
    ));
    assert_eq!(result.outputs["plate"].provenance.feature_id, "plate");
    assert_eq!(result.outputs["other"].provenance.feature_id, "other_plate");
    assert_eq!(
        result.outputs["other"].provenance.input_features[0],
        "other_outline"
    );
}

#[test]
fn reports_cycles_before_input_type_errors() {
    let mut input = recipe();
    input.features = vec![
        Feature::Extrude {
            id: "a".into(),
            profile: "b".into(),
            height: 1.0,
            op_version: 1,
        },
        Feature::Extrude {
            id: "b".into(),
            profile: "a".into(),
            height: 1.0,
            op_version: 1,
        },
    ];
    assert!(matches!(build(&input), Err(BuildError::Cycle(path)) if path == "a -> b -> a"));
    input.features = vec![Feature::Extrude {
        id: "self".into(),
        profile: "self".into(),
        height: 1.0,
        op_version: 1,
    }];
    assert!(matches!(build(&input), Err(BuildError::Cycle(_))));
}

#[test]
fn rejects_missing_duplicate_and_wrongly_typed_references() {
    let mut input = recipe();
    if let Feature::Extrude { profile, .. } = &mut input.features[1] {
        *profile = "absent".into();
    }
    assert!(matches!(
        build(&input),
        Err(BuildError::MissingDependency { .. })
    ));
    let mut input = recipe();
    input.features.push(input.features[0].clone());
    assert!(matches!(
        build(&input),
        Err(BuildError::DuplicateFeature(_))
    ));
    let mut input = recipe();
    input.features.push(Feature::Extrude {
        id: "invalid".into(),
        profile: "plate".into(),
        height: 1.0,
        op_version: 1,
    });
    assert!(matches!(build(&input), Err(BuildError::InputType { .. })));
    let mut input = recipe();
    input.outputs[0].feature = "outline".into();
    assert!(matches!(build(&input), Err(BuildError::OutputType { .. })));
    input.outputs[0].feature = "absent".into();
    assert!(matches!(
        build(&input),
        Err(BuildError::MissingOutput { .. })
    ));
    let mut input = recipe();
    input.outputs.push(input.outputs[0].clone());
    assert!(matches!(build(&input), Err(BuildError::DuplicateOutput(_))));
}

#[test]
fn rejects_invalid_numeric_inputs_versions_and_units() {
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut input = recipe();
        if let Feature::Rectangle { width, .. } = &mut input.features[0] {
            *width = invalid;
        }
        assert!(matches!(build(&input), Err(BuildError::Dimension { .. })));
    }
    let mut input = recipe();
    input.tolerance = Some(Tolerance { linear: f64::NAN });
    assert!(matches!(build(&input), Err(BuildError::Tolerance)));
    let mut input = recipe();
    input.schema_version = 3;
    assert!(matches!(build(&input), Err(BuildError::SchemaVersion(3))));
    let mut input = recipe();
    input.units = "inch".into();
    assert!(matches!(build(&input), Err(BuildError::Units(_))));
    let mut input = recipe();
    if let Feature::Rectangle { op_version, .. } = &mut input.features[0] {
        *op_version = 2;
    }
    assert!(matches!(
        build(&input),
        Err(BuildError::OperationVersion { .. })
    ));
}

#[test]
fn strict_json_rejects_misspelled_fields() {
    let original = serde_json::to_value(recipe()).unwrap();
    for path in [0, 1, 2] {
        let mut input = original.clone();
        match path {
            0 => input["typo"] = true.into(),
            1 => input["features"][0]["typo"] = true.into(),
            _ => input["outputs"][0]["typo"] = true.into(),
        }
        assert!(serde_json::from_value::<Recipe>(input).is_err());
    }
}

#[test]
fn rejects_empty_identifiers_and_nonfinite_extrusion_height() {
    let mut input = recipe();
    if let Feature::Rectangle { id, .. } = &mut input.features[0] {
        *id = " ".into();
    }
    assert!(matches!(build(&input), Err(BuildError::Identifier { .. })));
    let mut input = recipe();
    if let Feature::Extrude { height, .. } = &mut input.features[1] {
        *height = f64::INFINITY;
    }
    assert!(matches!(
        build(&input),
        Err(BuildError::Dimension {
            parameter: "height",
            ..
        })
    ));
    let mut input = recipe();
    input.outputs[0].name.clear();
    assert!(matches!(build(&input), Err(BuildError::Identifier { .. })));
}

#[test]
fn failed_kernel_operation_is_not_cached() {
    let mut input = recipe();
    if let Feature::Extrude { height, .. } = &mut input.features[1] {
        *height = Tolerance::default().linear / 2.0;
    }
    let mut engine = BuildEngine::default();
    for _ in 0..2 {
        assert!(matches!(
            engine.build(&input),
            Err(BuildError::Kernel { .. })
        ));
        assert!(engine.solids.is_empty());
    }
    assert_eq!(engine.build(&recipe()).unwrap().stats.computed_solids, 1);
}

#[test]
fn decimal_and_scientific_recipe_numbers_survive_json_round_trip() {
    // Also runs with serde_json/arbitrary_precision enabled by cad-recipe's
    // Starlark dependency in the full workspace build.
    let input: Recipe = serde_json::from_str(
        r#"{
        "schema_version":1,"units":"mm","tolerance":{"linear":1e-7},
        "features":[
            {"width":0.5,"depth":1e-3,"id":"outline","op":"rectangle"},
            {"height":2.5,"profile":"outline","op":"extrude","id":"plate"}
        ],
        "outputs":[{"name":"plate","feature":"plate"}]
    }"#,
    )
    .unwrap();
    let first = build(&input).unwrap();
    let encoded = serde_json::to_string_pretty(&input).unwrap();
    let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
    let second = build(&decoded).unwrap();
    assert_eq!(first.feature_hashes, second.feature_hashes);
    assert!((second.outputs["plate"].result.model.volume() - 0.00125).abs() < 1e-16);
    let as_value = serde_json::to_value(&input).unwrap();
    let from_value: Recipe = serde_json::from_value(as_value).unwrap();
    assert_eq!(
        build(&from_value).unwrap().feature_hashes,
        first.feature_hashes
    );
}

#[test]
fn feature_deserialization_rejects_duplicate_and_operation_inappropriate_fields() {
    for json in [
        r#"{"op":"rectangle","id":"p","width":0.5,"depth":1e-3,"height":2.0}"#,
        r#"{"op":"rectangle","id":"p","width":0.5,"depth":1e-3,"profile":"p"}"#,
        r#"{"op":"extrude","id":"p","profile":"p","height":2.0,"width":0.5}"#,
        r#"{"op":"extrude","id":"p","profile":"p","height":2.0,"depth":0.5}"#,
        r#"{"op":"rectangle","id":"p","width":0.5,"width":1.0,"depth":1e-3}"#,
        r#"{"op":"rectangle","op":"rectangle","id":"p","width":0.5,"depth":1e-3}"#,
        r#"{"op":"rectangle","id":"p","width":0.5,"depth":1e-3,"typo":true}"#,
        r#"{"op":"unknown","id":"p"}"#,
        r#"{"op":"rectangle","id":"p","width":0.5}"#,
    ] {
        assert!(
            serde_json::from_str::<Feature>(json).is_err(),
            "accepted {json}"
        );
    }
}

fn boolean_recipe(operation: &str, offset: [f64; 3]) -> Recipe {
    serde_json::from_value(serde_json::json!({
        "schema_version":1,"units":"mm",
        "features":[
            {"op":operation,"id":"result","left":"box","right":"moved"},
            {"op":"translate","id":"moved","body":"box","offset":offset},
            {"op":"extrude","id":"box","profile":"profile","height":10.0},
            {"op":"rectangle","id":"profile","width":10.0,"depth":10.0}
        ],
        "outputs":[{"name":"result","feature":"result"}]
    }))
    .unwrap()
}

#[test]
fn boolean_graphs_execute_both_dependencies_in_any_order() {
    for (op, volume) in [("union", 1500.0), ("cut", 500.0), ("intersect", 500.0)] {
        let input = boolean_recipe(op, [5.0, 0.0, 0.0]);
        let built = build(&input).unwrap();
        assert!((built.outputs["result"].result.model.volume() - volume).abs() < 1e-8);
        assert_eq!(
            built.outputs["result"].provenance.input_features,
            ["box", "moved"]
        );
        let encoded = serde_json::to_string(&input).unwrap();
        let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            built.feature_hashes,
            build(&decoded).unwrap().feature_hashes
        );
    }
}

#[test]
fn empty_and_disconnected_results_are_valid_outputs() {
    let empty = build(&boolean_recipe("cut", [0.0; 3])).unwrap();
    assert_eq!(empty.outputs["result"].result.model.solids.len(), 0);
    assert_eq!(empty.outputs["result"].result.model.volume(), 0.0);
    let separated = build(&boolean_recipe("union", [20.0, 0.0, 0.0])).unwrap();
    assert_eq!(separated.outputs["result"].result.model.solids.len(), 2);
    assert_eq!(separated.outputs["result"].result.model.volume(), 2000.0);
}

#[test]
fn multi_input_cycles_wrong_types_and_nonfinite_offsets_are_rejected() {
    let mut input = boolean_recipe("union", [5.0, 0.0, 0.0]);
    if let Feature::Union { right, .. } = &mut input.features[0] {
        *right = "result".into();
    }
    assert!(matches!(build(&input), Err(BuildError::Cycle(_))));
    let mut input = boolean_recipe("union", [5.0, 0.0, 0.0]);
    if let Feature::Union { right, .. } = &mut input.features[0] {
        *right = "profile".into();
    }
    assert!(matches!(build(&input), Err(BuildError::InputType { .. })));
    let mut input = boolean_recipe("union", [5.0, 0.0, 0.0]);
    if let Feature::Translate { body, .. } = &mut input.features[1] {
        *body = "profile".into();
    }
    assert!(matches!(build(&input), Err(BuildError::InputType { .. })));
    let input = boolean_recipe("union", [5.0, 0.0, 0.0]);
    let mut input = input;
    if let Feature::Translate { offset, .. } = &mut input.features[1] {
        offset[0] = f64::INFINITY;
    }
    assert!(matches!(build(&input), Err(BuildError::Offset { .. })));
}

#[test]
fn changing_a_second_input_invalidates_only_affected_solids() {
    let mut engine = BuildEngine::default();
    let mut input = boolean_recipe("cut", [5.0, 0.0, 0.0]);
    let initial = engine.build(&input).unwrap();
    assert_eq!(initial.stats.computed_solids, 3);
    if let Feature::Translate { offset, .. } = &mut input.features[1] {
        offset[0] = 7.0;
    }
    let changed = engine.build(&input).unwrap();
    assert_eq!(changed.stats.cache_hits, 1);
    assert_eq!(changed.stats.computed_solids, 2);
    assert_eq!(initial.feature_hashes["box"], changed.feature_hashes["box"]);
    assert_ne!(
        initial.feature_hashes["result"],
        changed.feature_hashes["result"]
    );
    assert_eq!(changed.outputs["result"].result.model.volume(), 700.0);
}

#[test]
fn disk_cache_reuses_verified_artifacts_across_engines() {
    let directory = tempfile::tempdir().unwrap();
    let input = recipe();
    let first = BuildEngine::with_cache_dir(directory.path())
        .build(&input)
        .unwrap();
    let second = BuildEngine::with_cache_dir(directory.path())
        .build(&input)
        .unwrap();
    assert_eq!(first.stats.computed_solids, 1);
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(second.stats.cache_hits, 1);
    assert_eq!(second.stats.disk_cache_hits, 1);
    assert_eq!(
        first.outputs["plate"].result.model,
        second.outputs["plate"].result.model
    );
    assert_eq!(
        first.outputs["plate"].result.history,
        second.outputs["plate"].result.history
    );
}

#[test]
fn stale_corrupt_and_invalid_disk_entries_are_safe_misses() {
    let directory = tempfile::tempdir().unwrap();
    let input = recipe();
    let first = BuildEngine::with_cache_dir(directory.path())
        .build(&input)
        .unwrap();
    let hash = &first.outputs["plate"].content_hash;
    let path = BuildEngine::with_cache_dir(directory.path())
        .cache_path(hash)
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    for mutation in 0..8 {
        let mut entry: CacheEntry = serde_json::from_slice(&original).unwrap();
        match mutation {
            0 => entry.kernel_version = "stale".into(),
            1 => entry.feature_hash = "wrong-key".into(),
            2 => entry.result_digest = "wrong-digest".into(),
            3 => entry.cache_version = 0,
            4 => {
                entry.result.model.edges[0].vertices[0] = cad_kernel::VertexId(usize::MAX);
                entry.result_digest = blake3::hash(&serde_json::to_vec(&entry.result).unwrap())
                    .to_hex()
                    .to_string();
            }
            5 => {
                entry
                    .result
                    .history
                    .generated_faces
                    .insert("invalid".into(), vec![cad_kernel::FaceId(usize::MAX)]);
                entry.result_digest = blake3::hash(&serde_json::to_vec(&entry.result).unwrap())
                    .to_hex()
                    .to_string();
            }
            6 => entry.kernel_build_id = "different-source-same-semver".into(),
            _ => entry.executor_build_id = "different-executor-same-schema".into(),
        }
        std::fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();
        let result = BuildEngine::with_cache_dir(directory.path())
            .build(&input)
            .unwrap();
        assert_eq!(result.stats.computed_solids, 1, "mutation {mutation}");
        assert_eq!(result.stats.disk_cache_hits, 0);
    }
    std::fs::write(&path, b"truncated {").unwrap();
    assert_eq!(
        BuildEngine::with_cache_dir(directory.path())
            .build(&input)
            .unwrap()
            .stats
            .computed_solids,
        1
    );
}

#[test]
fn inaccessible_disk_cache_does_not_prevent_building() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let result = BuildEngine::with_cache_dir(file.path())
        .build(&recipe())
        .unwrap();
    assert_eq!(result.stats.computed_solids, 1);
}

#[test]
fn new_operations_preserve_strict_json_fields_and_offset_shape() {
    for json in [
        r#"{"op":"translate","id":"t","body":"b","offset":[0.5,0.0,1e-3],"width":1}"#,
        r#"{"op":"translate","id":"t","body":"b","offset":[0.5,0.0]}"#,
        r#"{"op":"union","id":"u","left":"a","right":"b","offset":[0,0,0]}"#,
        r#"{"op":"cut","id":"c","left":"a","right":"b","right":"c"}"#,
        r#"{"op":"intersect","id":"i","left":"a"}"#,
    ] {
        assert!(
            serde_json::from_str::<Feature>(json).is_err(),
            "accepted {json}"
        );
    }
    let feature: Feature =
        serde_json::from_str(r#"{"offset":[0.5,-2.0,1e-3],"body":"b","id":"t","op":"translate"}"#)
            .unwrap();
    assert!(matches!(
        feature,
        Feature::Translate {
            offset: [0.5, -2.0, 0.001],
            ..
        }
    ));
}

fn assembly_recipe() -> Recipe {
    let mut input = recipe();
    input.schema_version = 2;
    input.outputs.clear();
    input.parts = vec![PartDefinition {
        datums: BTreeMap::new(),
        metadata: Default::default(),
        id: "plate-part".into(),
        feature: "plate".into(),
    }];
    input.assemblies = vec![AssemblyDefinition {
        assemblies: vec![],
        id: "pair".into(),
        instances: vec![
            Instance {
                attachment: None,
                id: "left".into(),
                part: "plate-part".into(),
                transform: RigidTransform::default(),
            },
            Instance {
                attachment: None,
                id: "right".into(),
                part: "plate-part".into(),
                transform: RigidTransform {
                    translation: [100.5, 0.0, 0.0],
                    ..Default::default()
                },
            },
        ],
    }];
    input.assembly_outputs = vec![AssemblyOutput {
        name: "pair".into(),
        assembly: "pair".into(),
    }];
    input
}

#[test]
fn assembly_instances_share_definitions_and_placement_edits_reuse_geometry() {
    let mut input = assembly_recipe();
    let mut engine = BuildEngine::default();
    let initial = engine.build(&input).unwrap();
    assert!(initial.outputs.is_empty());
    assert_eq!(initial.stats.computed_solids, 1);
    let pair = &initial.assemblies["pair"];
    assert_eq!(pair.parts.len(), 1);
    assert_eq!(pair.instance_count(), 2);
    assert_eq!(pair.material_volume(), 38400.0);
    assert_eq!(pair.bounds().unwrap().max, [180.5, 40.0, 6.0]);
    input.assemblies[0].instances[1].transform.translation = [150.25, 0.0, 0.0];
    let changed = engine.build(&input).unwrap();
    assert_eq!(changed.stats.computed_solids, 0);
    assert_eq!(changed.stats.cache_hits, 1);
    assert_eq!(initial.feature_hashes, changed.feature_hashes);
    assert_ne!(pair.content_hash, changed.assemblies["pair"].content_hash);
    assert!(Arc::ptr_eq(
        &pair.parts["plate-part"].result,
        &changed.assemblies["pair"].parts["plate-part"].result
    ));
    assert_eq!(
        pair.parts["plate-part"].result.model.bounds().max,
        [80.0, 40.0, 6.0]
    );
    if let Feature::Rectangle { width, .. } = &mut input.features[0] {
        *width = 81.0;
    }
    let resized = engine.build(&input).unwrap();
    assert_eq!(resized.stats.computed_solids, 1);
    assert_ne!(
        changed.assemblies["pair"].content_hash,
        resized.assemblies["pair"].content_hash
    );
}

#[test]
fn identical_geometry_keeps_distinct_part_feature_and_instance_identities() {
    let mut input = assembly_recipe();
    input.features.push(Feature::Extrude {
        id: "other-solid".into(),
        profile: "outline".into(),
        height: 6.0,
        op_version: 1,
    });
    input.parts.push(PartDefinition {
        datums: BTreeMap::new(),
        metadata: Default::default(),
        id: "other-part".into(),
        feature: "other-solid".into(),
    });
    input.assemblies[0].instances[1].part = "other-part".into();
    input.assembly_outputs.push(AssemblyOutput {
        name: "alias".into(),
        assembly: "pair".into(),
    });
    let result = build(&input).unwrap();
    let pair = &result.assemblies["pair"];
    assert_eq!(result.stats.computed_solids, 1);
    assert_eq!(pair.parts["other-part"].feature_id, "other-solid");
    assert_eq!(pair.parts["plate-part"].feature_id, "plate");
    assert_eq!(pair.instances[1].id, "right");
    assert!(Arc::ptr_eq(
        &pair.parts["plate-part"].result,
        &pair.parts["other-part"].result
    ));
    assert!(Arc::ptr_eq(
        &pair.parts["plate-part"].result,
        &result.assemblies["alias"].parts["plate-part"].result
    ));
}

#[test]
fn assembly_schema_is_strict_and_decimal_transforms_roundtrip() {
    let input = assembly_recipe();
    let encoded = serde_json::to_string(&input).unwrap();
    let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        build(&input).unwrap().assemblies["pair"].content_hash,
        build(&decoded).unwrap().assemblies["pair"].content_hash
    );
    let mut value = serde_json::to_value(&input).unwrap();
    value["parts"][0]["typo"] = true.into();
    assert!(serde_json::from_value::<Recipe>(value).is_err());
    for field in ["parts", "assemblies", "assembly_outputs"] {
        for data in [serde_json::json!([]), serde_json::Value::Null] {
            let mut value = serde_json::to_value(recipe()).unwrap();
            value[field] = data;
            assert!(serde_json::from_value::<Recipe>(value).is_err());
        }
    }
    let mut direct = input;
    direct.schema_version = 1;
    assert!(matches!(build(&direct), Err(BuildError::AssemblySchema)));
}

#[test]
fn assembly_references_duplicates_types_and_output_collisions_are_rejected() {
    let check = |input: Recipe, fragment: &str| {
        let error = build(&input).unwrap_err().to_string();
        assert!(error.contains(fragment), "{error}");
    };
    let mut input = assembly_recipe();
    input.parts[0].feature = "missing".into();
    check(input, "missing feature");
    let mut input = assembly_recipe();
    input.parts[0].feature = "outline".into();
    check(input, "profile");
    let mut input = assembly_recipe();
    input.parts.push(input.parts[0].clone());
    check(input, "duplicate part");
    let mut input = assembly_recipe();
    input.assemblies.push(input.assemblies[0].clone());
    check(input, "duplicate assembly");
    let mut input = assembly_recipe();
    input.assemblies[0].instances[1].id = "left".into();
    check(input, "repeats instance");
    let mut input = assembly_recipe();
    input.assemblies[0].instances[1].part = "pair".into();
    check(input, "missing part");
    let mut input = assembly_recipe();
    input.assemblies[0].instances[1].transform.rotation = [0.0; 4];
    check(input, "quaternion");
    let mut input = assembly_recipe();
    input.assembly_outputs[0].assembly = "missing".into();
    check(input, "missing assembly");
    let mut input = assembly_recipe();
    input.outputs.push(Output {
        name: "pair".into(),
        feature: "plate".into(),
    });
    check(input, "duplicate output");
    let mut input = assembly_recipe();
    input.parts = vec![input.parts[0].clone(); MAX_PARTS + 1];
    assert!(matches!(
        build(&input),
        Err(BuildError::ResourceLimit {
            resource: "parts",
            ..
        })
    ));
}

fn nested_recipe() -> Recipe {
    serde_json::from_value(serde_json::json!({
        "schema_version":2,"units":"mm",
        "features":[{"op":"rectangle","id":"p","width":2,"depth":3},{"op":"extrude","id":"body","profile":"p","height":4}],
        "parts":[{"id":"part","feature":"body"}],
        "assemblies":[
            {"id":"leaf","instances":[{"id":"bolt","part":"part","transform":{"translation":[1,0,0]}}]},
            {"id":"mid","instances":[],"assemblies":[{"id":"inner","assembly":"leaf","transform":{"translation":[0,5,0],"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]}}]},
            {"id":"root","instances":[],"assemblies":[
                {"id":"left","assembly":"mid","transform":{"translation":[10,0,0],"rotation":[0,0,1,0]}},
                {"id":"right","assembly":"mid","transform":{"translation":[30,0,0]}}
            ]}
        ],"assembly_outputs":[{"name":"assembly","assembly":"root"}]
    })).unwrap()
}

#[test]
fn nested_placements_compose_noncommuting_frames_and_reuse_exact_geometry() {
    let mut recipe = nested_recipe();
    let mut engine = BuildEngine::default();
    let first = engine.build(&recipe).unwrap();
    let assembly = &first.assemblies["assembly"];
    assert_eq!(assembly.parts.len(), 1);
    assert_eq!(assembly.instances.len(), 2);
    assert_eq!(assembly.hierarchy.len(), 3);
    assert_eq!(assembly.material_volume(), 48.0);
    let paths: std::collections::BTreeSet<_> = assembly.instance_paths.values().cloned().collect();
    assert_eq!(
        paths,
        std::collections::BTreeSet::from([
            vec!["left".to_owned(), "inner".to_owned(), "bolt".to_owned()],
            vec!["right".to_owned(), "inner".to_owned(), "bolt".to_owned()]
        ])
    );
    let bounds = assembly.bounds().unwrap();
    for axis in 0..3 {
        assert!((bounds.min[axis] - [10.0, -8.0, 0.0][axis]).abs() < 1e-12);
        assert!((bounds.max[axis] - [30.0, 8.0, 4.0][axis]).abs() < 1e-12);
    }
    recipe.assemblies[2].assemblies[1].transform.translation[0] = 40.0;
    let changed = engine.build(&recipe).unwrap();
    assert_eq!(changed.stats.computed_solids, 0);
    assert_eq!(first.feature_hashes, changed.feature_hashes);
    assert_ne!(
        assembly.content_hash,
        changed.assemblies["assembly"].content_hash
    );
    assert!(Arc::ptr_eq(
        &assembly.parts["part"].result,
        &changed.assemblies["assembly"].parts["part"].result
    ));
    let roundtrip: Recipe = serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
    assert_eq!(
        build(&roundtrip).unwrap().assemblies["assembly"].content_hash,
        changed.assemblies["assembly"].content_hash
    );
}

#[test]
fn nested_invalid_references_cycles_and_expansion_are_rejected_before_geometry() {
    let mut recipe = nested_recipe();
    if let Feature::Rectangle { width, .. } = &mut recipe.features[0] {
        *width = 1e308;
    }
    recipe.assemblies.push(AssemblyDefinition {
        id: "orphan".into(),
        instances: vec![],
        assemblies: vec![AssemblyInstance {
            id: "self".into(),
            assembly: "orphan".into(),
            transform: Default::default(),
        }],
    });
    assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    recipe.assemblies[3].assemblies[0].assembly = "missing".into();
    assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    let mut recipe = nested_recipe();
    recipe.assemblies[0].assemblies.push(AssemblyInstance {
        id: "bolt".into(),
        assembly: "mid".into(),
        transform: Default::default(),
    });
    assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    let mut recipe = nested_recipe();
    recipe.assemblies.truncate(1);
    for level in 1..19 {
        let previous = if level == 1 {
            "leaf".to_owned()
        } else {
            format!("level{}", level - 1)
        };
        recipe.assemblies.push(AssemblyDefinition {
            id: format!("level{level}"),
            instances: vec![],
            assemblies: vec![
                AssemblyInstance {
                    id: "a".into(),
                    assembly: previous.clone(),
                    transform: Default::default(),
                },
                AssemblyInstance {
                    id: "b".into(),
                    assembly: previous,
                    transform: Default::default(),
                },
            ],
        });
    }
    recipe.assembly_outputs[0].assembly = "leaf".into(); // Invalid unreachable graph still rejected.
    assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
}

#[test]
fn nested_depth_and_aggregate_export_expansion_have_preflight_limits() {
    let mut recipe = nested_recipe();
    recipe.assemblies.truncate(1);
    for level in 1..=65 {
        let previous = if level == 1 {
            "leaf".to_owned()
        } else {
            format!("level{}", level - 1)
        };
        recipe.assemblies.push(AssemblyDefinition {
            id: format!("level{level}"),
            instances: vec![],
            assemblies: vec![AssemblyInstance {
                id: "child".into(),
                assembly: previous,
                transform: Default::default(),
            }],
        });
    }
    recipe.assembly_outputs[0].assembly = "leaf".into();
    assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    recipe.assemblies.truncate(16);
    for definition in &mut recipe.assemblies[1..] {
        let mut sibling = definition.assemblies[0].clone();
        sibling.id = "sibling".into();
        definition.assemblies.push(sibling);
    }
    recipe.assembly_outputs = vec![
        AssemblyOutput {
            name: "one".into(),
            assembly: "level15".into(),
        },
        AssemblyOutput {
            name: "two".into(),
            assembly: "level15".into(),
        },
    ];
    assert!(matches!(
        build(&recipe),
        Err(BuildError::ResourceLimit {
            resource: "expanded assembly outputs",
            ..
        })
    ));
    let mut raw = serde_json::to_value(nested_recipe()).unwrap();
    raw["assemblies"][1]["assemblies"][0]["part"] = "part".into();
    assert!(serde_json::from_value::<Recipe>(raw).is_err());
}

#[test]
fn metadata_edits_preserve_geometry_cache_and_change_assembly_identity() {
    let mut recipe = nested_recipe();
    let mut engine = BuildEngine::default();
    let initial = engine.build(&recipe).unwrap();
    recipe.parts[0].metadata = PartMetadata {
        part_number: Some("BRACKET-001".into()),
        revision: Some("B".into()),
        description: Some("Support bracket".into()),
        material: Some(Material {
            name: "Aluminum".into(),
            density_kg_per_m3: 2700.0,
        }),
    };
    let edited = engine.build(&recipe).unwrap();
    assert_eq!(edited.stats.computed_solids, 0);
    assert_eq!(initial.feature_hashes, edited.feature_hashes);
    let before = &initial.assemblies["assembly"];
    let after = &edited.assemblies["assembly"];
    assert_ne!(before.content_hash, after.content_hash);
    assert!(Arc::ptr_eq(
        &before.parts["part"].result,
        &after.parts["part"].result
    ));
    assert_eq!(after.parts["part"].metadata, recipe.parts[0].metadata);
    let decoded: Recipe = serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
    assert_eq!(
        engine.build(&decoded).unwrap().assemblies["assembly"].content_hash,
        after.content_hash
    );
}

#[test]
fn invalid_metadata_on_unused_parts_is_rejected_before_geometry() {
    let mut recipe = nested_recipe();
    recipe.parts.push(PartDefinition {
        datums: BTreeMap::new(),
        id: "unused".into(),
        feature: "body".into(),
        metadata: Default::default(),
    });
    if let Feature::Rectangle { width, .. } = &mut recipe.features[0] {
        *width = 1e308;
    }
    for metadata in [
        PartMetadata {
            part_number: Some(" ".into()),
            ..Default::default()
        },
        PartMetadata {
            revision: Some("A ".into()),
            ..Default::default()
        },
        PartMetadata {
            description: Some("".into()),
            ..Default::default()
        },
        PartMetadata {
            material: Some(Material {
                name: "Steel".into(),
                density_kg_per_m3: 0.0,
            }),
            ..Default::default()
        },
        PartMetadata {
            material: Some(Material {
                name: "Steel".into(),
                density_kg_per_m3: f64::INFINITY,
            }),
            ..Default::default()
        },
    ] {
        recipe.parts[1].metadata = metadata;
        assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    }
    let mut raw = serde_json::to_value(nested_recipe()).unwrap();
    raw["parts"][0]["metadata"] = serde_json::json!({"part_nubmer":"typo"});
    assert!(serde_json::from_value::<Recipe>(raw.clone()).is_err());
    raw["parts"][0]["metadata"] =
        serde_json::json!({"material":{"name":"Steel","density_kg_per_m3":7850,"density":7850}});
    assert!(serde_json::from_value::<Recipe>(raw).is_err());
}

fn polygon_recipe() -> Recipe {
    serde_json::from_value(serde_json::json!({
        "schema_version":1,"units":"mm",
        "features":[
            {"op":"polygon","id":"triangle","points":[[0,0],[4,0],[1,3]]},
            {"op":"extrude","id":"prism","profile":"triangle","height":2},
            {"op":"translate","id":"moved","body":"prism","offset":[10,-2,5]},
            {"op":"rectangle","id":"square","width":1,"depth":1},
            {"op":"extrude","id":"other","profile":"square","height":1}
        ],"outputs":[{"name":"prism","feature":"prism"},{"name":"moved","feature":"moved"},{"name":"other","feature":"other"}]
    })).unwrap()
}

#[test]
fn polygon_profiles_extrude_translate_and_resolve_wall_roles_with_incremental_cache() {
    let mut recipe = polygon_recipe();
    recipe.schema_version = 2;
    recipe.face_assertions.push(FaceAssertion {
        id: "side".into(),
        selector: FaceSelector {
            anchor_feature: "prism".into(),
            role: "wall_0".into(),
            target_feature: "moved".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        },
    });
    let mut engine = BuildEngine::default();
    let initial = engine.build(&recipe).unwrap();
    assert_eq!(initial.stats.computed_solids, 3);
    assert!((initial.outputs["prism"].result.model.volume() - 12.0).abs() < 1e-12);
    assert_eq!(initial.outputs["prism"].result.model.faces.len(), 5);
    let bounds = initial.outputs["moved"].result.model.bounds();
    assert_eq!(bounds.min, [10.0, -2.0, 5.0]);
    assert_eq!(bounds.max, [14.0, 1.0, 7.0]);
    assert_eq!(
        initial.face_assertions["side"].status,
        FaceResolutionStatus::Resolved
    );
    assert_eq!(engine.build(&recipe).unwrap().stats.computed_solids, 0);
    if let Feature::Polygon { points, .. } = &mut recipe.features[0] {
        points[1][0] = 6.0;
    }
    let changed = engine.build(&recipe).unwrap();
    assert_eq!(changed.stats.computed_solids, 2);
    assert_eq!(changed.stats.cache_hits, 1);
    assert!((changed.outputs["prism"].result.model.volume() - 18.0).abs() < 1e-12);
    assert_eq!(
        initial.feature_hashes["other"],
        changed.feature_hashes["other"]
    );
    assert_ne!(
        initial.feature_hashes["triangle"],
        changed.feature_hashes["triangle"]
    );
    recipe.features.reverse();
    assert_eq!(
        engine.build(&recipe).unwrap().feature_hashes,
        changed.feature_hashes
    );
}

#[test]
fn polygon_json_has_strict_fields_coordinate_pairs_and_arbitrary_precision_roundtrip() {
    for raw in [
        r#"{"op":"polygon","id":"p","points":[[0,0],[1,0],[0,1]],"width":1}"#,
        r#"{"op":"polygon","id":"p","points":[[0,0],[1,0],[0,1]],"points":[]}"#,
        r#"{"op":"polygon","id":"p","points":[[0,0,0],[1,0],[0,1]]}"#,
        r#"{"op":"polygon","id":"p","points":[[0,0],[true,0],[0,1]]}"#,
    ] {
        assert!(serde_json::from_str::<Feature>(raw).is_err());
    }
    let mut recipe = polygon_recipe();
    recipe.features[0] = serde_json::from_str(
        r#"{"op":"polygon","id":"triangle","points":[[0.5,1e-3],[4.5,1e-3],[1.5,3.001]]}"#,
    )
    .unwrap();
    let decoded: Recipe = serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
    assert_eq!(
        build(&recipe).unwrap().feature_hashes,
        build(&decoded).unwrap().feature_hashes
    );
}

#[test]
fn invalid_unused_polygons_and_unsupported_booleans_fail_explicitly() {
    for points in [
        vec![],
        vec![[0.0, 0.0]; 3],
        vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]],
        vec![[0.0, 0.0], [2.0, 0.0], [1.0, 0.5], [2.0, 2.0], [0.0, 2.0]],
        vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
        vec![[0.0, 0.0], [f64::NAN, 0.0], [0.0, 1.0]],
        vec![[0.0, 0.0]; MAX_POLYGON_POINTS + 1],
    ] {
        let mut recipe = recipe();
        recipe.features.push(Feature::Polygon {
            id: "unused".into(),
            points,
            holes: vec![],
            op_version: 1,
        });
        assert!(
            matches!(build(&recipe),Err(BuildError::PolygonProfile{feature,..}) if feature=="unused")
        );
    }
    let mut recipe = polygon_recipe();
    recipe.features.push(Feature::Union {
        id: "unsupported".into(),
        left: "prism".into(),
        right: "other".into(),
        op_version: 1,
    });
    assert!(matches!(build(&recipe),Err(BuildError::Kernel{feature,..}) if feature=="unsupported"));
    let mut recipe = polygon_recipe();
    if let Feature::Polygon { op_version, .. } = &mut recipe.features[0] {
        *op_version = 2;
    }
    assert!(matches!(
        build(&recipe),
        Err(BuildError::OperationVersion { .. })
    ));
}

fn attached_recipe() -> Recipe {
    serde_json::from_value(serde_json::json!({
        "schema_version":2,"units":"mm",
        "features":[{"op":"rectangle","id":"p","width":2,"depth":3},{"op":"extrude","id":"body","profile":"p","height":4}],
        "parts":[
            {"id":"base","feature":"body","datums":{"mount":{"translation":[0,2,0]}}},
            {"id":"bracket","feature":"body","datums":{"foot":{"translation":[1,0,0],"rotation":[0,0,1,0]}}}
        ],"assemblies":[{"id":"assembly","instances":[
            {"id":"attached","part":"bracket","transform":{"translation":[0,0,3]},"attachment":{"instance":"fixed","datum":"foot","target_datum":"mount"}},
            {"id":"fixed","part":"base","transform":{"translation":[10,0,0],"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]}}
        ]}],"assembly_outputs":[{"name":"assembly","assembly":"assembly"}]
    })).unwrap()
}

#[test]
fn datum_attachments_resolve_local_frames_without_rebuilding_exact_geometry() {
    let mut recipe = attached_recipe();
    let mut engine = BuildEngine::default();
    let first = engine.build(&recipe).unwrap();
    assert_eq!(first.stats.computed_solids, 1);
    let assembly = &first.assemblies["assembly"];
    let child = assembly
        .instances
        .iter()
        .find(|instance| instance.id == "attached")
        .unwrap();
    assert!(child.attachment.is_none());
    for axis in 0..3 {
        assert!((child.transform.translation[axis] - [8.0, 1.0, 3.0][axis]).abs() < 1e-12);
    }
    let placed = child.transform.point([1.0, 0.0, 0.0]);
    for axis in 0..3 {
        assert!((placed[axis] - [8.0, 0.0, 3.0][axis]).abs() < 1e-12);
    }
    assert!(assembly.hierarchy["assembly"]
        .instances
        .iter()
        .any(|instance| instance.attachment.is_some()));
    recipe.parts[0].datums.get_mut("mount").unwrap().translation[1] = 4.0;
    let changed = engine.build(&recipe).unwrap();
    assert_eq!(changed.stats.computed_solids, 0);
    assert_eq!(first.feature_hashes, changed.feature_hashes);
    assert_ne!(
        assembly.content_hash,
        changed.assemblies["assembly"].content_hash
    );
    let child = changed.assemblies["assembly"]
        .instances
        .iter()
        .find(|instance| instance.id == "attached")
        .unwrap();
    assert!((child.transform.translation[0] - 6.0).abs() < 1e-12);
    assert!(Arc::ptr_eq(
        &assembly.parts["base"].result,
        &changed.assemblies["assembly"].parts["bracket"].result
    ));
    let decoded: Recipe = serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
    assert_eq!(
        build(&decoded).unwrap().assemblies["assembly"].content_hash,
        changed.assemblies["assembly"].content_hash
    );
}

#[test]
fn datum_and_attachment_errors_fail_even_in_unused_definitions_before_geometry() {
    let mut recipe = attached_recipe();
    recipe.assembly_outputs.clear();
    if let Feature::Rectangle { width, .. } = &mut recipe.features[0] {
        *width = 1e308;
    }
    let original = recipe.clone();
    for change in 0..4 {
        let mut recipe = original.clone();
        match change {
            0 => {
                recipe.assemblies[0].instances[0]
                    .attachment
                    .as_mut()
                    .unwrap()
                    .instance = "missing".into()
            }
            1 => {
                recipe.assemblies[0].instances[0]
                    .attachment
                    .as_mut()
                    .unwrap()
                    .datum = "missing".into()
            }
            2 => {
                recipe.assemblies[0].instances[0]
                    .attachment
                    .as_mut()
                    .unwrap()
                    .target_datum = "missing".into()
            }
            _ => {
                recipe.assemblies[0].instances[1].attachment = Some(DatumAttachment {
                    instance: "attached".into(),
                    datum: "mount".into(),
                    target_datum: "foot".into(),
                })
            }
        }
        assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    }
    recipe.parts[0]
        .datums
        .insert(" bad ".into(), Default::default());
    assert!(matches!(build(&recipe), Err(BuildError::AssemblyGraph(_))));
    let mut raw = serde_json::to_value(attached_recipe()).unwrap();
    raw["assemblies"][0]["instances"][0]["attachment"]["typo"] = "oops".into();
    assert!(serde_json::from_value::<Recipe>(raw).is_err());
}

fn transformed_recipe() -> Recipe {
    serde_json::from_value(serde_json::json!({
        "schema_version":2,"units":"mm",
        "features":[
            {"op":"rectangle","id":"p","width":8,"depth":4},
            {"op":"extrude","id":"b","profile":"p","height":2},
            {"op":"transform","id":"spin","body":"b","offset":[3,5,7],"rotation":[0,0,(std::f64::consts::PI/8.0).sin(),(std::f64::consts::PI/8.0).cos()]},
            {"op":"transform","id":"tilt","body":"spin","offset":[11,13,17],"rotation":[0,(std::f64::consts::PI/12.0).sin(),0,(std::f64::consts::PI/12.0).cos()]}
        ],
        "outputs":[{"name":"result","feature":"tilt"}],
        "face_assertions":[{"id":"cap","selector":{"anchor_feature":"b","role":"end_cap","target_feature":"tilt","cardinality":{"kind":"exactly","count":1}}}]
    })).unwrap()
}

#[test]
fn rigid_transform_chain_preserves_volume_semantic_faces_and_analytic_bounds() {
    let input = transformed_recipe();
    let first = build(&input).unwrap();
    let model = &first.outputs["result"].result.model;
    assert!((model.volume() - 64.0).abs() < 1e-9);
    assert!(model.validate(Tolerance::default()).valid);
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    // Independent R_y(30) * (R_z(45) * p + [3,5,7]) + [11,13,17].
    for x in [0.0, 8.0] {
        for y in [0.0, 4.0] {
            for z in [0.0, 2.0] {
                let a = (x - y) * std::f64::consts::FRAC_1_SQRT_2 + 3.0;
                let b = (x + y) * std::f64::consts::FRAC_1_SQRT_2 + 5.0;
                let c = z + 7.0;
                let p = [
                    3.0_f64.sqrt() / 2.0 * a + 0.5 * c + 11.0,
                    b + 13.0,
                    -0.5 * a + 3.0_f64.sqrt() / 2.0 * c + 17.0,
                ];
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
        }
    }
    let bounds = model.bounds();
    for i in 0..3 {
        assert!((bounds.min[i] - min[i]).abs() < 1e-10);
        assert!((bounds.max[i] - max[i]).abs() < 1e-10);
    }
    let cap = &first.face_assertions["cap"];
    assert_eq!(cap.status, FaceResolutionStatus::Resolved);
    assert_eq!(cap.witnesses.len(), 1);
    let mut reordered = input.clone();
    reordered.features.reverse();
    let second = build(&reordered).unwrap();
    assert_eq!(first.feature_hashes, second.feature_hashes);
    assert_eq!(
        serde_json::to_string(&*first.outputs["result"].result).unwrap(),
        serde_json::to_string(&*second.outputs["result"].result).unwrap()
    );
}

#[test]
fn rigid_transform_cache_tracks_frames_but_not_logical_aliases() {
    let mut input = transformed_recipe();
    let mut engine = BuildEngine::default();
    let first = engine.build(&input).unwrap();
    assert_eq!(first.stats.computed_solids, 3);
    assert_eq!(engine.build(&input).unwrap().stats.computed_solids, 0);
    if let Feature::Transform { offset, .. } = &mut input.features[3] {
        offset[0] += 0.25;
    }
    let moved = engine.build(&input).unwrap();
    assert_eq!(moved.stats.computed_solids, 1);
    assert_eq!(moved.stats.cache_hits, 2);
    assert_ne!(first.feature_hashes["tilt"], moved.feature_hashes["tilt"]);
    if let Feature::Transform { id, .. } = &mut input.features[3] {
        *id = "alias".into();
    }
    input.outputs[0].feature = "alias".into();
    input.face_assertions[0].selector.target_feature = "alias".into();
    let alias = engine.build(&input).unwrap();
    assert_eq!(alias.stats.computed_solids, 0);
    assert_eq!(alias.feature_hashes["alias"], moved.feature_hashes["tilt"]);
    assert!(Arc::ptr_eq(
        &alias.outputs["result"].result,
        &moved.outputs["result"].result
    ));
    assert_eq!(
        alias.face_assertions["cap"].witnesses[0].target_feature,
        "alias"
    );
    if let Feature::Transform { rotation, .. } = &mut input.features[2] {
        *rotation = [0.0, 0.0, 0.0, 1.0];
    }
    let rotated = engine.build(&input).unwrap();
    assert_eq!(rotated.stats.computed_solids, 2);
    assert_eq!(rotated.stats.cache_hits, 1);
    assert_ne!(rotated.feature_hashes["spin"], alias.feature_hashes["spin"]);
}

#[test]
fn rigid_transform_strict_json_and_invalid_frames_fail_before_geometry() {
    let valid =
        r#"{"op":"transform","id":"t","body":"b","offset":[0.5,1e-3,0],"rotation":[0,0,0,1]}"#;
    let feature: Feature = serde_json::from_str(valid).unwrap();
    assert!(matches!(feature,Feature::Transform{offset,..} if offset==[0.5,0.001,0.0]));
    for bad in [
        valid.replace("\"body\"", "\"unknown\""),
        valid.replace("[0,0,0,1]", "[0,0,1]"),
        valid.replace("\"op\":\"transform\"", "\"op\":\"translate\""),
        valid.replace(
            "\"rotation\":[0,0,0,1]",
            "\"rotation\":[0,0,0,1],\"rotation\":[0,0,0,1]",
        ),
    ] {
        assert!(serde_json::from_str::<Feature>(&bad).is_err(), "{bad}");
    }
    for rotation in [
        [0.0; 4],
        [0.0, 0.0, 0.0, 2.0],
        [f64::NAN, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0, 1.0 + 1e-10],
    ] {
        let mut input = transformed_recipe();
        if let Feature::Transform { rotation: q, .. } = &mut input.features[2] {
            *q = rotation;
        }
        assert!(matches!(
            build(&input),
            Err(BuildError::TransformFrame { .. })
        ));
    }
    let mut input = transformed_recipe();
    if let Feature::Transform { offset, .. } = &mut input.features[2] {
        offset[2] = f64::INFINITY;
    }
    assert!(matches!(
        build(&input),
        Err(BuildError::TransformFrame { .. })
    ));
    let mut input = transformed_recipe();
    if let Feature::Transform { body, .. } = &mut input.features[2] {
        *body = "p".into();
    }
    assert!(matches!(build(&input), Err(BuildError::InputType { .. })));
    let mut input = transformed_recipe();
    if let Feature::Transform { op_version, .. } = &mut input.features[2] {
        *op_version = 2;
    }
    assert!(matches!(
        build(&input),
        Err(BuildError::OperationVersion { .. })
    ));
}

fn convex_intersection_recipe(radius: f64) -> Recipe {
    // A square [0,4]^2 clipped by a centered diamond of L1 radius r in (2,4).
    // Four removed corner triangles each have leg 4-r; both prisms have height2.
    serde_json::from_value(serde_json::json!({
        "schema_version":2,"units":"mm",
        "features":[
            {"op":"rectangle","id":"square_profile","width":4,"depth":4},
            {"op":"extrude","id":"square","profile":"square_profile","height":2},
            {"op":"rectangle","id":"tool_profile","width":radius*2.0_f64.sqrt(),"depth":radius*2.0_f64.sqrt()},
            {"op":"extrude","id":"tool","profile":"tool_profile","height":2},
            {"op":"transform","id":"diamond","body":"tool","offset":[2,2.0-radius,0],"rotation":[0,0,(std::f64::consts::PI/8.0).sin(),(std::f64::consts::PI/8.0).cos()]},
            {"op":"intersect","id":"overlap","left":"square","right":"diamond"}
        ],
        "outputs":[{"name":"overlap","feature":"overlap"}],
        "face_assertions":[
            {"id":"left_cap","selector":{"anchor_feature":"square","role":"end_cap","target_feature":"overlap","cardinality":{"kind":"exactly","count":1}}},
            {"id":"right_cap","selector":{"anchor_feature":"tool","role":"end_cap","target_feature":"overlap","cardinality":{"kind":"exactly","count":1}}},
            {"id":"left_walls","selector":{"anchor_feature":"square","role":"walls","target_feature":"overlap","cardinality":{"kind":"exactly","count":4}}},
            {"id":"right_walls","selector":{"anchor_feature":"tool","role":"walls","target_feature":"overlap","cardinality":{"kind":"exactly","count":4}}}
        ]
    })).unwrap()
}

#[test]
fn rotated_convex_intersection_has_analytic_volume_bounds_and_both_operand_lineage() {
    let mut engine = BuildEngine::default();
    for (index, radius) in [2.2, 2.5, 3.0, 3.5, 3.8].into_iter().enumerate() {
        let recipe = convex_intersection_recipe(radius);
        let built = engine.build(&recipe).unwrap();
        let model = &built.outputs["overlap"].result.model;
        let expected = 32.0 - 4.0 * (4.0 - radius).powi(2);
        assert!((model.volume() - expected).abs() < 1e-8, "radius {radius}");
        let bounds = model.bounds();
        for axis in 0..3 {
            assert!(bounds.min[axis].abs() < 1e-8);
            assert!((bounds.max[axis] - [4.0, 4.0, 2.0][axis]).abs() < 1e-8);
        }
        assert_eq!(model.faces.len(), 10);
        assert_eq!(built.stats.computed_solids, if index == 0 { 4 } else { 3 });
        assert_eq!(built.stats.cache_hits, if index == 0 { 0 } else { 1 });
        let left = &built.face_assertions["left_cap"].witnesses;
        let right = &built.face_assertions["right_cap"].witnesses;
        // Coincident caps have dual provenance, not two fabricated output faces.
        assert_eq!(left[0].target_face, right[0].target_face);
        let left_walls: std::collections::BTreeSet<_> = built.face_assertions["left_walls"]
            .witnesses
            .iter()
            .map(|w| w.target_face)
            .collect();
        let right_walls: std::collections::BTreeSet<_> = built.face_assertions["right_walls"]
            .witnesses
            .iter()
            .map(|w| w.target_face)
            .collect();
        assert_eq!(left_walls.len(), 4);
        assert_eq!(right_walls.len(), 4);
        assert!(left_walls.is_disjoint(&right_walls));
        assert_eq!(engine.build(&recipe).unwrap().stats.computed_solids, 0);
    }
}

#[test]
fn general_convex_union_and_cut_remain_explicitly_unsupported() {
    for operation in ["union", "cut"] {
        let mut json = serde_json::to_value(convex_intersection_recipe(3.0)).unwrap();
        json["features"][5]["op"] = operation.into();
        let recipe: Recipe = serde_json::from_value(json).unwrap();
        match build(&recipe) {
            Err(BuildError::Kernel { feature, message }) => {
                assert_eq!(feature, "overlap");
                assert!(message.to_lowercase().contains("unsupported"), "{message}");
            }
            other => panic!("Expected explicit unsupported {operation}, got {other:?}"),
        }
    }
}

#[test]
fn convex_intersection_lineage_distinguishes_deleted_tool_walls_and_empty_overlap() {
    let mut recipe = convex_intersection_recipe(5.0);
    recipe
        .face_assertions
        .retain(|assertion| assertion.id != "right_walls");
    let contained = build(&recipe).unwrap();
    assert!((contained.outputs["overlap"].result.model.volume() - 32.0).abs() < 1e-8);
    let deleted = contained.resolve_faces(&FaceSelector {
        anchor_feature: "tool".into(),
        role: "walls".into(),
        target_feature: "overlap".into(),
        cardinality: FaceCardinality::All,
    });
    assert_eq!(deleted.status, FaceResolutionStatus::Deleted);
    assert!(deleted.witnesses.is_empty());
    let mut disjoint = convex_intersection_recipe(3.0);
    disjoint.face_assertions.clear();
    if let Feature::Transform { offset, .. } = &mut disjoint.features[4] {
        offset[0] = 50.0;
    }
    let empty = build(&disjoint).unwrap();
    assert!(empty.outputs["overlap"].result.model.solids.is_empty());
    for anchor in ["square", "tool"] {
        let deleted = empty.resolve_faces(&FaceSelector {
            anchor_feature: anchor.into(),
            role: "end_cap".into(),
            target_feature: "overlap".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        });
        assert_eq!(deleted.status, FaceResolutionStatus::Deleted);
        assert!(deleted.witnesses.is_empty());
    }
}

fn face_datum_recipe(height: f64, cut_x: f64, cut_width: f64) -> Recipe {
    serde_json::from_value(serde_json::json!({"schema_version":2,"units":"mm","features":[
        {"op":"rectangle","id":"p","width":10,"depth":8},
        {"op":"extrude","id":"base","profile":"p","height":height},
        {"op":"rectangle","id":"cp","width":cut_width,"depth":10},
        {"op":"extrude","id":"c","profile":"cp","height":height+2.0},
        {"op":"translate","id":"tool","body":"c","offset":[cut_x,-1,-1]},
        {"op":"cut","id":"trimmed","left":"base","right":"tool"},
        {"op":"rectangle","id":"bp","width":2,"depth":2},
        {"op":"extrude","id":"block","profile":"bp","height":2}
    ],"outputs":[],"parts":[{"id":"basepart","feature":"trimmed"},{"id":"blockpart","feature":"block","datums":{"foot":{"translation":[1,1,0]}}}],
    "face_datums":[{"part":"basepart","name":"seat","anchor_feature":"base","role":"end_cap","x_direction":[1,0,0],"offset":[1,2,3]}],
    "assemblies":[{"id":"assembly","instances":[{"id":"base","part":"basepart"},{"id":"block","part":"blockpart","attachment":{"instance":"base","datum":"foot","target_datum":"seat"}}]}],
    "assembly_outputs":[{"name":"assembly","assembly":"assembly"}]})).unwrap()
}

#[test]
fn associative_datums_follow_edited_face_centroids_and_drive_attachments_without_geometry_edits() {
    let mut engine = BuildEngine::default();
    for height in [4.0, 6.0] {
        let mut recipe = face_datum_recipe(height, -1.0, 3.0);
        let built = engine.build(&recipe).unwrap();
        let report = &built.face_datums["basepart"]["seat"];
        assert_eq!(report.selection.status, FaceResolutionStatus::Resolved);
        assert_eq!(report.frame.translation, [7.0, 6.0, height + 3.0]);
        assert_eq!(report.frame.rotation, [0.0, 0.0, 0.0, 1.0]);
        let child = built.assemblies["assembly"]
            .instances
            .iter()
            .find(|i| i.id == "block")
            .unwrap();
        assert_eq!(child.transform.translation, [6.0, 5.0, height + 3.0]);
        assert!(
            recipe.parts[0].datums.is_empty(),
            "resolved frames must not overwrite author intent"
        );
        recipe.face_datums[0].x_direction = [2.0, 0.0, 0.0];
        let equivalent = engine.build(&recipe).unwrap();
        assert_eq!(equivalent.stats.computed_solids, 0);
        assert_eq!(
            equivalent.face_datums["basepart"]["seat"].frame,
            report.frame
        );
        assert_eq!(
            equivalent.assemblies["assembly"].content_hash,
            built.assemblies["assembly"].content_hash
        );
        assert_ne!(
            equivalent.face_datums["basepart"]["seat"].definition,
            report.definition
        );
        recipe.face_datums[0].offset[2] += 2.0;
        let moved = engine.build(&recipe).unwrap();
        assert_eq!(moved.stats.computed_solids, 0);
        assert_eq!(moved.feature_hashes, built.feature_hashes);
        assert_ne!(
            moved.assemblies["assembly"].content_hash,
            built.assemblies["assembly"].content_hash
        );
    }
}
#[test]
fn associative_datums_reject_split_deleted_unknown_roles_and_parallel_hint() {
    for (x, width, status) in [
        (4.0, 2.0, FaceResolutionStatus::Ambiguous),
        (-1.0, 12.0, FaceResolutionStatus::Deleted),
    ] {
        match build(&face_datum_recipe(4.0, x, width)) {
            Err(BuildError::FaceDatumFailed {
                part,
                name,
                resolution,
                ..
            }) => {
                assert_eq!(part, "basepart");
                assert_eq!(name, "seat");
                assert_eq!(resolution.status, status);
            }
            result => panic!("unexpected result: {result:?}"),
        }
    }
    let mut recipe = face_datum_recipe(4.0, -1.0, 3.0);
    recipe.face_datums[0].role = "misspelled".into();
    assert!(
        matches!(build(&recipe),Err(BuildError::FaceDatumFailed{resolution,..}) if resolution.status==FaceResolutionStatus::Unsupported)
    );
    recipe.face_datums[0].role = "end_cap".into();
    recipe.face_datums[0].x_direction = [0.0, 0.0, 1.0];
    assert!(matches!(
        build(&recipe),
        Err(BuildError::FaceDatumFrame { .. })
    ));
}
#[test]
fn face_datum_schema_names_limits_and_coordinate_fields_are_strict() {
    let mut recipe = face_datum_recipe(4.0, -1.0, 3.0);
    recipe.parts[0]
        .datums
        .insert("seat".into(), RigidTransform::default());
    assert!(matches!(
        build(&recipe),
        Err(BuildError::InvalidFaceDatum { .. })
    ));
    recipe.parts[0].datums.clear();
    recipe.face_datums.push(recipe.face_datums[0].clone());
    assert!(matches!(
        build(&recipe),
        Err(BuildError::InvalidFaceDatum { .. })
    ));
    recipe.face_datums = vec![recipe.face_datums[0].clone(); MAX_FACE_DATUMS + 1];
    assert!(matches!(
        build(&recipe),
        Err(BuildError::ResourceLimit {
            resource: "face datums",
            ..
        })
    ));
    let mut json = serde_json::to_value(face_datum_recipe(4.0, -1.0, 3.0)).unwrap();
    json["face_datums"][0]["typo"] = 1.into();
    assert!(serde_json::from_value::<Recipe>(json).is_err());
    assert!(serde_json::from_str::<Recipe>(
        r#"{"schema_version":1,"units":"mm","features":[],"face_datums":[]}"#
    )
    .is_err());
}
#[test]
fn associative_face_datums_resolve_through_convex_intersections_and_rigid_transform_chains() {
    let mut recipe = convex_intersection_recipe(3.0);
    recipe.parts.push(PartDefinition {
        id: "part".into(),
        feature: "overlap".into(),
        metadata: Default::default(),
        datums: BTreeMap::new(),
    });
    recipe.face_datums.push(FaceDatumDefinition {
        part: "part".into(),
        name: "top".into(),
        anchor_feature: "tool".into(),
        role: "end_cap".into(),
        x_direction: [1.0, 0.0, 0.0],
        offset: [0.0; 3],
    });
    let result = build(&recipe).unwrap();
    for (a, b) in result.face_datums["part"]["top"]
        .frame
        .translation
        .into_iter()
        .zip([2.0, 2.0, 2.0])
    {
        assert!((a - b).abs() < 1e-9);
    }
    let mut recipe = transformed_recipe();
    recipe.parts.push(PartDefinition {
        id: "part".into(),
        feature: "tilt".into(),
        metadata: Default::default(),
        datums: BTreeMap::new(),
    });
    recipe.face_datums.push(FaceDatumDefinition {
        part: "part".into(),
        name: "top".into(),
        anchor_feature: "b".into(),
        role: "end_cap".into(),
        x_direction: [1.0, 0.0, 0.0],
        offset: [0.0; 3],
    });
    let result = build(&recipe).unwrap();
    let frame = result.face_datums["part"]["top"].frame;
    let a = 2.0_f64.sqrt() + 3.0;
    let expected = [
        a * 3.0_f64.sqrt() / 2.0 + 4.5 + 11.0,
        3.0 * 2.0_f64.sqrt() + 5.0 + 13.0,
        -0.5 * a + 9.0 * 3.0_f64.sqrt() / 2.0 + 17.0,
    ];
    for (a, b) in frame.translation.into_iter().zip(expected) {
        assert!((a - b).abs() < 1e-9);
    }
    for (a, b) in frame
        .vector([0.0, 0.0, 1.0])
        .into_iter()
        .zip([0.5, 0.0, 3.0_f64.sqrt() / 2.0])
    {
        assert!((a - b).abs() < 1e-12);
    }
}

#[test]
fn selected_outputs_skip_runtime_unsupported_branches_but_never_structural_validation() {
    let mut recipe = convex_intersection_recipe(3.0);
    recipe.face_assertions.clear();
    recipe.features[5] = Feature::Union {
        id: "overlap".into(),
        left: "square".into(),
        right: "diamond".into(),
        op_version: 1,
    };
    recipe.outputs.push(Output {
        name: "simple".into(),
        feature: "square".into(),
    });
    let mut engine = BuildEngine::default();
    let selected = engine
        .build_outputs(&recipe, &["simple".into(), "simple".into()])
        .unwrap();
    assert_eq!(selected.stats.computed_solids, 1);
    assert_eq!(selected.scope.mode, BuildMode::Selected);
    assert_eq!(selected.scope.requested_outputs, vec!["simple"]);
    assert_eq!(
        selected.scope.evaluated_features,
        vec!["square", "square_profile"]
    );
    assert_eq!(
        selected.scope.skipped_features,
        vec!["diamond", "overlap", "tool", "tool_profile"]
    );
    assert_eq!(selected.feature_hashes.len(), 2);
    for (anchor, target) in [("diamond", "diamond"), ("square", "overlap")] {
        let report = selected.resolve_faces(&FaceSelector {
            anchor_feature: anchor.into(),
            role: "end_cap".into(),
            target_feature: target.into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        });
        assert_eq!(report.status, FaceResolutionStatus::Unsupported);
        assert!(report
            .diagnostic
            .contains("not evaluated in this build scope"));
    }
    assert!(matches!(
        engine.build(&recipe),
        Err(BuildError::Kernel { .. })
    ));
    assert!(matches!(
        engine.build_outputs(&recipe, &[]),
        Err(BuildError::EmptyOutputSelection)
    ));
    assert!(matches!(
        engine.build_outputs(&recipe, &["typo".into()]),
        Err(BuildError::UnknownSelectedOutput(_))
    ));
    let original = recipe.features.clone();
    recipe.features[5] = Feature::Union {
        id: "overlap".into(),
        left: "overlap".into(),
        right: "diamond".into(),
        op_version: 1,
    };
    assert!(matches!(
        engine.build_outputs(&recipe, &["simple".into()]),
        Err(BuildError::Cycle(_))
    ));
    recipe.features = original;
    if let Feature::Rectangle { width, .. } = &mut recipe.features[2] {
        *width = -1.0;
    }
    assert!(matches!(
        engine.build_outputs(&recipe, &["simple".into()]),
        Err(BuildError::Dimension { .. })
    ));
}

#[test]
fn selected_nested_assembly_closure_shares_parts_and_unrelated_edits_compute_nothing() {
    let mut recipe = nested_recipe();
    recipe.features.extend([
        Feature::Rectangle {
            id: "unused_profile".into(),
            width: 7.0,
            depth: 9.0,
            op_version: 1,
        },
        Feature::Extrude {
            id: "unused_body".into(),
            profile: "unused_profile".into(),
            height: 2.0,
            op_version: 1,
        },
    ]);
    let mut part = recipe.parts[0].clone();
    part.id = "unused_part".into();
    part.feature = "unused_body".into();
    recipe.parts.push(part);
    let mut assembly = recipe.assemblies[0].clone();
    assembly.id = "unused_assembly".into();
    assembly.instances[0].part = "unused_part".into();
    recipe.assemblies.push(assembly);
    recipe.assembly_outputs.push(AssemblyOutput {
        name: "unused".into(),
        assembly: "unused_assembly".into(),
    });
    let mut engine = BuildEngine::default();
    let first = engine.build_outputs(&recipe, &["assembly".into()]).unwrap();
    assert_eq!(first.stats.computed_solids, 1);
    assert_eq!(first.assemblies.len(), 1);
    assert_eq!(first.assemblies["assembly"].instances.len(), 2);
    assert_eq!(first.assemblies["assembly"].parts.len(), 1);
    assert_eq!(first.assemblies["assembly"].hierarchy.len(), 3);
    assert_eq!(
        first.scope.skipped_features,
        vec!["unused_body", "unused_profile"]
    );
    if let Feature::Extrude { height, .. } = &mut recipe.features[3] {
        *height = 3.0;
    }
    let second = engine.build_outputs(&recipe, &["assembly".into()]).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(second.stats.cache_hits, 1);
    assert_eq!(first.feature_hashes, second.feature_hashes);
    assert_eq!(
        first.assemblies["assembly"].content_hash,
        second.assemblies["assembly"].content_hash
    );
    let eager = engine.build(&recipe).unwrap();
    assert_eq!(eager.scope.mode, BuildMode::All);
    assert!(eager.scope.skipped_features.is_empty());
    assert_eq!(eager.assemblies.len(), 2);
}

#[test]
fn selected_outputs_keep_all_global_assertion_and_datum_obligations() {
    let mut recipe = convex_intersection_recipe(3.0);
    recipe.outputs.push(Output {
        name: "simple".into(),
        feature: "square".into(),
    });
    let result = BuildEngine::default()
        .build_outputs(&recipe, &["simple".into()])
        .unwrap();
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.face_assertions.len(), 4);
    assert_eq!(result.scope.evaluated_features.len(), 6);
    assert_eq!(
        result.scope.validation_roots,
        vec!["overlap", "square", "tool"]
    );
    recipe.face_assertions[0].selector.role = "absent".into();
    assert!(matches!(
        BuildEngine::default().build_outputs(&recipe, &["simple".into()]),
        Err(BuildError::FaceAssertionFailed { .. })
    ));
    recipe.face_assertions[0].selector.anchor_feature = "missing".into();
    assert!(matches!(
        BuildEngine::default().build_outputs(&recipe, &["simple".into()]),
        Err(BuildError::FaceAssertionFailed { .. })
    ));
    for (cut_x, cut_width, success) in [(-1.0, 3.0, true), (4.0, 2.0, false), (-1.0, 12.0, false)] {
        let mut recipe = face_datum_recipe(4.0, cut_x, cut_width);
        recipe.outputs.push(Output {
            name: "block".into(),
            feature: "block".into(),
        });
        let built = BuildEngine::default().build_outputs(&recipe, &["block".into()]);
        if success {
            let built = built.unwrap();
            assert!(built.assemblies.is_empty());
            assert_eq!(
                built.face_datums["basepart"]["seat"].frame.translation,
                [7.0, 6.0, 7.0]
            );
            assert!(built.scope.validation_roots.contains(&"trimmed".to_owned()));
        } else {
            assert!(matches!(built, Err(BuildError::FaceDatumFailed { .. })));
        }
    }
}

fn holed_profile_recipe() -> Recipe {
    serde_json::from_value(serde_json::json!({"schema_version":2,"units":"mm","features":[
        {"op":"polygon","id":"profile","points":[[0,0],[10,0],[10,8],[0,8]],"holes":[[[2,2],[2,4],[4,4],[4,2]],[[6,2],[6,5],[8,5],[8,2]]]},
        {"op":"extrude","id":"body","profile":"profile","height":3},
        {"op":"translate","id":"moved","body":"body","offset":[1,2,3]},
        {"op":"rectangle","id":"other_profile","width":1,"depth":1},
        {"op":"extrude","id":"other","profile":"other_profile","height":1}
    ],"outputs":[{"name":"plate","feature":"moved"},{"name":"other","feature":"other"}],"face_assertions":[
        {"id":"cap","selector":{"anchor_feature":"body","role":"end_cap","target_feature":"moved","cardinality":{"kind":"exactly","count":1}}},
        {"id":"bore","selector":{"anchor_feature":"body","role":"hole_0_wall_1","target_feature":"moved","cardinality":{"kind":"exactly","count":1}}}
    ]})).unwrap()
}

#[test]
fn polygon_holes_preserve_semantics_and_incremental_demand_validation() {
    let mut recipe = holed_profile_recipe();
    let mut engine = BuildEngine::default();
    let first = engine.build_outputs(&recipe, &["plate".into()]).unwrap();
    assert_eq!(first.outputs["plate"].result.model.volume(), 210.0);
    assert_eq!(
        first.outputs["plate"].result.model.bounds().min,
        [1.0, 2.0, 3.0]
    );
    assert_eq!(
        first.outputs["plate"].result.model.bounds().max,
        [11.0, 10.0, 6.0]
    );
    assert_eq!(first.face_assertions.len(), 2);
    assert_eq!(first.scope.skipped_features, vec!["other", "other_profile"]);
    let all_holes = first.resolve_faces(&FaceSelector {
        anchor_feature: "body".into(),
        role: "hole_walls".into(),
        target_feature: "moved".into(),
        cardinality: FaceCardinality::Exactly { count: 8 },
    });
    assert_eq!(all_holes.status, FaceResolutionStatus::Resolved);
    assert_eq!(
        engine
            .build_outputs(&recipe, &["plate".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    if let Feature::Polygon { holes, .. } = &mut recipe.features[0] {
        holes[0][2][0] = 5.0;
        holes[0][3][0] = 5.0;
    }
    let changed = engine.build_outputs(&recipe, &["plate".into()]).unwrap();
    assert_eq!(changed.stats.computed_solids, 2);
    assert_eq!(changed.outputs["plate"].result.model.volume(), 204.0);
    assert_ne!(
        changed.feature_hashes["profile"],
        first.feature_hashes["profile"]
    );
    // An unrelated requested output still evaluates the global hole/cap contracts.
    let other = engine.build_outputs(&recipe, &["other".into()]).unwrap();
    assert_eq!(other.face_assertions.len(), 2);
    assert!(other.feature_hashes.contains_key("moved"));
    recipe.face_assertions[1].selector.role = "hole_8_wall_0".into();
    assert!(matches!(
        engine.build_outputs(&recipe, &["other".into()]),
        Err(BuildError::FaceAssertionFailed { .. })
    ));
}

#[test]
fn polygon_holes_json_is_strict_and_invalid_unused_holes_cannot_be_skipped() {
    let recipe = holed_profile_recipe();
    let json = serde_json::to_string(&recipe).unwrap();
    assert_eq!(
        serde_json::to_string(&serde_json::from_str::<Recipe>(&json).unwrap()).unwrap(),
        json
    );
    for invalid in [
        json.replace("\"holes\":", "\"misspelled_holes\":"),
        json.replace("\"holes\":[", "\"holes\":[],\"holes\":["),
    ] {
        assert!(serde_json::from_str::<Recipe>(&invalid).is_err());
    }
    for hole in [
        vec![[2.0, 2.0], [4.0, 2.0], [4.0, 4.0], [2.0, 4.0]],
        vec![[0.0, 2.0], [0.0, 4.0], [4.0, 4.0], [4.0, 2.0]],
        vec![[9.0, 2.0], [9.0, 4.0], [11.0, 4.0], [11.0, 2.0]],
    ] {
        let mut recipe = recipe.clone();
        recipe.face_assertions.clear();
        if let Feature::Polygon { holes, .. } = &mut recipe.features[0] {
            holes[0] = hole;
        }
        assert!(matches!(
            BuildEngine::default().build_outputs(&recipe, &["other".into()]),
            Err(BuildError::PolygonProfile { .. })
        ));
    }
    let mut reversed = recipe.clone();
    if let Feature::Polygon { holes, .. } = &mut reversed.features[0] {
        holes.swap(0, 1);
    }
    assert_ne!(
        build(&recipe).unwrap().feature_hashes["profile"],
        build(&reversed).unwrap().feature_hashes["profile"]
    );
}
