use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn cli(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn discovery_is_deterministic_without_a_recipe_or_output_directory() {
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    let first = cli(first_dir.path(), &["capabilities"]);
    let second = cli(second_dir.path(), &["capabilities"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());
    assert_eq!(
        first.stdout, second.stdout,
        "catalog must not depend on process or working directory"
    );
    let document: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(document["schema_version"], 2);
    assert_eq!(
        document["cli"]["authoring"]["viewer"],
        "read_only_inspection"
    );
    assert_eq!(
        document["cli"]["execution"]["capabilities_has_side_effects"],
        false
    );
    for directory in [first_dir, second_dir] {
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

#[test]
fn discovery_rejects_recipe_parameters_and_cache_paths_without_creating_them() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache-must-not-exist");
    for args in [
        vec!["capabilities", "--param", "radius=4"],
        vec!["capabilities", "--cache-dir", cache.to_str().unwrap()],
    ] {
        let result = cli(directory.path(), &args);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert!(error.is_object());
        assert!(!cache.exists());
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn advertised_examples_compile_and_exercise_every_feature_operation() {
    use std::collections::{BTreeMap, BTreeSet};
    let directory = tempfile::tempdir().unwrap();
    let result = cli(directory.path(), &["capabilities"]);
    assert!(result.status.success());
    let document: Value = serde_json::from_slice(&result.stdout).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut compiled = BTreeMap::<String, Value>::new();
    let mut advertised = BTreeSet::new();
    for operation in document["operations"].as_array().unwrap() {
        let op = operation["json_op"].as_str().unwrap();
        assert!(advertised.insert(op.to_owned()), "duplicate operation {op}");
        let examples = operation["examples"].as_array().unwrap();
        assert!(!examples.is_empty(), "{op} has no authoring example");
        for example in examples {
            let example = example.as_str().unwrap();
            let recipe = compiled.entry(example.to_owned()).or_insert_with(|| {
                let result = cli(
                    directory.path(),
                    &["compile", root.join(example).to_str().unwrap()],
                );
                assert!(
                    result.status.success(),
                    "{example}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                serde_json::from_slice(&result.stdout).unwrap()
            });
            assert!(
                recipe["features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|feature| feature["op"] == op),
                "{example} does not demonstrate advertised operation {op}"
            );
        }
    }
    let expected: BTreeSet<_> = [
        "circle",
        "rectangle",
        "polygon",
        "path",
        "sketch",
        "extrude",
        "revolve",
        "translate",
        "transform",
        "union",
        "cut",
        "intersect",
        "chamfer",
        "chamfer_faces",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(advertised, expected);
    assert_eq!(document["sketch_policy"]["report_schema_version"], 3);
    let constraints = document["object_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|object| object["id"] == "sketch_constraint")
        .unwrap();
    let actual: BTreeSet<_> = constraints["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| variant["tag_value"].as_str().unwrap())
        .collect();
    let expected = [
        "fixed",
        "coincident",
        "horizontal",
        "vertical",
        "distance",
        "equal_length",
        "parallel",
        "perpendicular",
        "angle",
        "circle_line_tangent",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);
}

fn check_arguments(value: &Value, arguments: &[Value], catalog: &Value) {
    for argument in arguments {
        let name = argument["name"].as_str().unwrap();
        match value.get(name) {
            Some(value) => check_type(value, &argument["value_type"], catalog),
            _ => assert_ne!(argument["required"], true, "missing {name} in {value}"),
        }
    }
}

fn check_type(value: &Value, spec: &Value, catalog: &Value) {
    let valid = match spec["kind"].as_str().unwrap() {
        "string" | "reference" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "enum" => spec["values"].as_array().unwrap().contains(value),
        "nullable" => {
            if !value.is_null() {
                check_type(value, &spec["value"], catalog);
            }
            true
        }
        "map" => {
            for entry in value.as_object().unwrap().values() {
                check_type(entry, &spec["values"], catalog);
            }
            true
        }
        "array" => {
            let values = value.as_array().unwrap();
            if let Some(length) = spec["length"].as_u64() {
                assert_eq!(values.len(), length as usize, "{value}");
            }
            for item in values {
                check_type(item, &spec["items"], catalog);
            }
            true
        }
        "object" => {
            assert!(value.is_object(), "{value}");
            let object = catalog["object_types"]
                .as_array()
                .unwrap()
                .iter()
                .find(|object| object["id"] == spec["type_id"])
                .unwrap();
            check_arguments(value, object["arguments"].as_array().unwrap(), catalog);
            let mut declared: std::collections::BTreeSet<&str> = object["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|argument| argument["name"].as_str().unwrap())
                .collect();
            if let Some(discriminator) = object["discriminator"].as_str() {
                declared.insert(discriminator);
                let variant = object["variants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|variant| variant["tag_value"] == value[discriminator])
                    .unwrap_or_else(|| panic!("unrecognized {discriminator} in {value}"));
                check_arguments(value, variant["arguments"].as_array().unwrap(), catalog);
                declared.extend(
                    variant["arguments"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|argument| argument["name"].as_str().unwrap()),
                );
            }
            for key in value.as_object().unwrap().keys() {
                assert!(
                    declared.contains(key.as_str()),
                    "uncatalogued field {key} in {}: {value}",
                    spec["type_id"]
                );
            }
            true
        }
        unknown => panic!("unsupported catalog type {unknown}"),
    };
    assert!(valid, "{value} does not satisfy {spec}");
}

#[test]
fn compiled_corpus_matches_recursive_catalog_argument_types() {
    use std::collections::BTreeSet;
    let directory = tempfile::tempdir().unwrap();
    let result = cli(directory.path(), &["capabilities"]);
    assert!(result.status.success());
    let catalog: Value = serde_json::from_slice(&result.stdout).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut covered_constraints = BTreeSet::new();
    let mut covered_objects = BTreeSet::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("star") {
            continue;
        }
        let result = cli(directory.path(), &["compile", path.to_str().unwrap()]);
        assert!(
            result.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&result.stderr)
        );
        let recipe: Value = serde_json::from_slice(&result.stdout).unwrap();
        check_type(
            &recipe,
            &serde_json::json!({"kind":"object","type_id":"recipe"}),
            &catalog,
        );
        for feature in recipe["features"].as_array().unwrap() {
            let operation = catalog["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|operation| operation["json_op"] == feature["op"])
                .unwrap();
            check_arguments(
                feature,
                operation["arguments"].as_array().unwrap(),
                &catalog,
            );
            if let Some(constraints) = feature["sketch"]["constraints"].as_array() {
                for constraint in constraints {
                    covered_constraints.insert(constraint["kind"].as_str().unwrap().to_owned());
                }
            }
            if !feature["curve_outline"].is_null() {
                covered_objects.insert("sketch_path");
            }
            if feature["op"] == "path" {
                covered_objects.insert("path_segment");
            }
        }
    }
    // Exercise the remaining variant from the public typed API, without adding
    // a redundant constraint to a product example solely for coverage.
    let coincidence = cad_build::Constraint {
        id: "coincidence".into(),
        kind: cad_build::ConstraintKind::Coincident {
            a: "a".into(),
            b: "b".into(),
        },
    };
    let coincidence = serde_json::to_value(coincidence).unwrap();
    check_type(
        &coincidence,
        &serde_json::json!({"kind":"object","type_id":"sketch_constraint"}),
        &catalog,
    );
    covered_constraints.insert("coincident".into());
    let expected: BTreeSet<_> = [
        "fixed",
        "coincident",
        "horizontal",
        "vertical",
        "distance",
        "equal_length",
        "parallel",
        "perpendicular",
        "angle",
        "circle_line_tangent",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert!(
        expected.is_subset(&covered_constraints),
        "{covered_constraints:?}"
    );
    assert_eq!(
        covered_objects,
        BTreeSet::from(["sketch_path", "path_segment"])
    );
}

#[test]
fn recipe_envelope_defaults_and_populated_metadata_roundtrip_match_catalog() {
    let directory = tempfile::tempdir().unwrap();
    let result = cli(directory.path(), &["capabilities"]);
    assert!(result.status.success());
    let catalog: Value = serde_json::from_slice(&result.stdout).unwrap();
    let recipe_type = serde_json::json!({"kind":"object","type_id":"recipe"});
    let envelope = catalog["object_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|spec| spec["id"] == "recipe")
        .unwrap();
    for field in [
        "parts",
        "assemblies",
        "assembly_outputs",
        "face_assertions",
        "property_assertions",
        "section_views",
        "face_datums",
    ] {
        let argument = envelope["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|argument| argument["name"] == field)
            .unwrap();
        assert_eq!(argument["recipe_schema_versions"], serde_json::json!([2]));
        let mut forbidden = serde_json::json!({"schema_version":1,"units":"mm","features":[]});
        forbidden[field] = serde_json::json!([]);
        assert!(
            serde_json::from_value::<cad_build::Recipe>(forbidden).is_err(),
            "schema 1 accepted {field}"
        );
        let mut null_array = serde_json::json!({"schema_version":2,"units":"mm","features":[]});
        null_array[field] = Value::Null;
        assert!(
            serde_json::from_value::<cad_build::Recipe>(null_array).is_err(),
            "null array accepted: {field}"
        );
    }
    for schema in [1, 2] {
        let source = serde_json::json!({"schema_version":schema,"units":"mm","features":[]});
        check_type(&source, &recipe_type, &catalog);
        let recipe: cad_build::Recipe = serde_json::from_value(source).unwrap();
        let serialized = serde_json::to_value(recipe).unwrap();
        assert_eq!(serialized["tolerance"], Value::Null);
        assert_eq!(serialized["outputs"], serde_json::json!([]));
        check_type(&serialized, &recipe_type, &catalog);
        let _: cad_build::Recipe = serde_json::from_value(serialized).unwrap();
    }
    let source = serde_json::json!({
        "schema_version":2,"units":"mm","tolerance":{"linear":1e-7},
        "features":[{"op":"rectangle","id":"outline","width":20,"depth":10},
                    {"op":"extrude","id":"body","profile":"outline","height":4}],
        "outputs":[{"name":"body","feature":"body"}],
        "parts":[{"id":"part","feature":"body","datums":{"origin":{}},
                  "metadata":{"material":null,"revision":null}}],
        "assemblies":[
            {"id":"child","instances":[{"id":"leaf","part":"part","attachment":null}]},
            {"id":"root","assemblies":[
                {"id":"first","assembly":"child"},
                {"id":"second","assembly":"child","attachment":{
                    "instance":"first","target_path":["leaf"],"source_path":["leaf"],
                    "datum":"origin","target_datum":"origin"}}],
             "instances":[{"id":"attached_part","part":"part","attachment":{
                "instance":"first","target_path":["leaf"],"datum":"origin","target_datum":"origin"}}]}
        ],
        "assembly_outputs":[{"name":"assembly","assembly":"root"}]
    });
    check_type(&source, &recipe_type, &catalog);
    let mut recipe: cad_build::Recipe = serde_json::from_value(source).unwrap();
    recipe.parts[0].metadata = cad_build::PartMetadata {
        part_number: Some("PN-001".into()),
        revision: Some("A".into()),
        description: Some("Acceptance fixture with explicit material".into()),
        material: Some(cad_build::Material {
            name: "Aluminum".into(),
            density_kg_per_m3: 2700.,
        }),
    };
    recipe.parts[0].metadata.validate().unwrap();
    let serialized = serde_json::to_value(recipe).unwrap();
    check_type(&serialized, &recipe_type, &catalog);
    assert_eq!(
        serialized["parts"][0]["metadata"]["material"]["density_kg_per_m3"],
        2700.
    );
    assert_eq!(
        serialized["parts"][0]["datums"]["origin"]["rotation"],
        serde_json::json!([0., 0., 0., 1.])
    );
    let reparsed: cad_build::Recipe = serde_json::from_value(serialized.clone()).unwrap();
    assert_eq!(serde_json::to_value(reparsed).unwrap(), serialized);
    // Metadata has named, typed extension fields; arbitrary extra attributes
    // are not a supported material/part contract.
    let mut unknown_metadata = serialized;
    unknown_metadata["parts"][0]["metadata"]["attributes"] =
        serde_json::json!({"finish":"anodized"});
    assert!(serde_json::from_value::<cad_build::Recipe>(unknown_metadata).is_err());
}

#[test]
fn focused_operation_discovery_returns_only_inline_type_and_domain_closure() {
    let directory = tempfile::tempdir().unwrap();
    let full = cli(directory.path(), &["capabilities"]);
    assert!(full.status.success());
    let full: Value = serde_json::from_slice(&full.stdout).unwrap();
    assert_eq!(full["scope"], serde_json::json!({"kind":"full"}));
    for (id, expected_types) in [
        (
            "feature.sketch",
            vec![
                "sketch",
                "sketch_circle_hole",
                "sketch_constraint",
                "sketch_path",
                "sketch_path_segment",
                "sketch_point",
            ],
        ),
        ("feature.cut", vec![]),
    ] {
        let result = cli(directory.path(), &["capabilities", "--operation", id]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let focused: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            focused["scope"],
            serde_json::json!({"kind":"operation","id":id})
        );
        assert_eq!(focused["identities"], full["identities"]);
        assert_eq!(focused["schema_version"], full["schema_version"]);
        for (key, value) in full.as_object().unwrap() {
            if !["scope", "operations", "object_types", "domains"].contains(&key.as_str()) {
                assert_eq!(
                    &focused[key], value,
                    "focused catalog changed metadata {key}"
                );
            }
        }
        let operation = full["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["id"] == id)
            .unwrap();
        assert_eq!(focused["operations"], serde_json::json!([operation]));
        let actual_types: Vec<_> = focused["object_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|spec| spec["id"].as_str().unwrap())
            .collect();
        assert_eq!(actual_types, expected_types);
        for spec in focused["object_types"].as_array().unwrap() {
            assert!(full["object_types"].as_array().unwrap().contains(spec));
        }
        let mut actual_domains: Vec<_> = focused["domains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|domain| domain["id"].as_str().unwrap())
            .collect();
        let mut expected_domains: Vec<_> = operation["domain_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap())
            .collect();
        actual_domains.sort();
        expected_domains.sort();
        assert_eq!(actual_domains, expected_domains);
        assert_eq!(
            cli(directory.path(), &["capabilities", "--operation", id]).stdout,
            result.stdout
        );
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn focused_assembly_type_discovery_follows_nullable_inline_objects_not_references() {
    let directory = tempfile::tempdir().unwrap();
    for (id, expected_types) in [
        (
            "assembly_definition",
            vec![
                "assembly_attachment",
                "assembly_definition",
                "assembly_instance",
                "datum_attachment",
                "part_instance",
                "rigid_transform",
            ],
        ),
        (
            "part_definition",
            vec!["datum", "material", "part_definition", "part_metadata"],
        ),
    ] {
        let result = cli(directory.path(), &["capabilities", "--type", id]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let focused: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            focused["scope"],
            serde_json::json!({"kind":"object_type","id":id})
        );
        assert_eq!(focused["operations"], serde_json::json!([]));
        assert_eq!(focused["domains"], serde_json::json!([]));
        let actual: Vec<_> = focused["object_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|spec| spec["id"].as_str().unwrap())
            .collect();
        assert_eq!(actual, expected_types);
        assert_eq!(
            cli(directory.path(), &["capabilities", "--type", id]).stdout,
            result.stdout
        );
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn focused_discovery_rejects_unknown_ids_and_conflicting_flags_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["capabilities", "--operation", "feature.fillet"],
        vec!["capabilities", "--type", "missing_type"],
    ] {
        let result = cli(directory.path(), &args);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["code"], "invalid_request", "{error}");
    }
    let conflict = cli(
        directory.path(),
        &[
            "capabilities",
            "--operation",
            "feature.cut",
            "--type",
            "assembly_definition",
        ],
    );
    assert!(!conflict.status.success());
    assert!(conflict.stdout.is_empty());
    assert!(!conflict.stderr.is_empty());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
