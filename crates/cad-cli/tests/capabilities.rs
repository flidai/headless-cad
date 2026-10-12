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
    assert_eq!(document["schema_version"], 1);
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
                let source = std::fs::read_to_string(root.join(example)).unwrap();
                let recipe = cad_recipe::from_starlark(example, &source)
                    .unwrap_or_else(|error| panic!("{example}: {error}"));
                serde_json::to_value(recipe).unwrap()
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
            Some(value) if !value.is_null() => check_type(value, &argument["value_type"], catalog),
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
            if let Some(discriminator) = object["discriminator"].as_str() {
                let variant = object["variants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|variant| variant["tag_value"] == value[discriminator])
                    .unwrap_or_else(|| panic!("unrecognized {discriminator} in {value}"));
                check_arguments(value, variant["arguments"].as_array().unwrap(), catalog);
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
        let source = std::fs::read_to_string(&path).unwrap();
        let recipe = cad_recipe::from_starlark(path.to_str().unwrap(), &source).unwrap();
        let recipe = serde_json::to_value(recipe).unwrap();
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
