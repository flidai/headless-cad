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
        metadata: Default::default(),
        id: "plate-part".into(),
        feature: "plate".into(),
    }];
    input.assemblies = vec![AssemblyDefinition {
        assemblies: vec![],
        id: "pair".into(),
        instances: vec![
            Instance {
                id: "left".into(),
                part: "plate-part".into(),
                transform: RigidTransform::default(),
            },
            Instance {
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
