use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

#[test]
fn native_axial_obligations_publish_replay_and_preserve_failed_selected_builds() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("retention.json");
    let recipe = cad_recipe::from_starlark(
        "retained_shaft.star",
        include_str!("../../../examples/retained_shaft.star"),
    )
    .unwrap();
    let original = serde_json::to_value(recipe).unwrap();
    fs::write(&source, serde_json::to_vec(&original).unwrap()).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |input: &Path, output: &Path, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                input.to_str().unwrap(),
                "--out-dir",
                output.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    let first = run(&source, &out, &[]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let manifest: Value = serde_json::from_slice(&first.stdout).unwrap();
    let bytes = fs::read(out.join("datum-dimensions.json")).unwrap();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["kind"], "native_directed_datum_dimensions");
    assert_eq!(report["dimensions"], manifest["datum_dimensions"]);
    assert_eq!(
        report["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(report["dimensions"].as_object().unwrap().len(), 4);
    for dimension in report["dimensions"].as_object().unwrap().values() {
        let lo = dimension["lower_bound_mm"].as_f64().unwrap();
        let hi = dimension["upper_bound_mm"].as_f64().unwrap();
        assert!(lo > 0. && hi >= lo && hi - lo < 1e-7);
        assert_eq!(dimension["status"], "passed");
        assert!(dimension["from"]["semantic_datum"].is_object());
        assert!(dimension["to"]["world_plane"].is_object());
    }
    let stacks: Value =
        serde_json::from_slice(&fs::read(out.join("tolerance-stacks.json")).unwrap()).unwrap();
    assert_eq!(stacks["schema_version"], 2);
    assert_eq!(
        stacks["datum_dimension_report"]["digest"],
        blake3::hash(&bytes).to_hex().to_string()
    );
    assert!(stacks["section_reports"].as_object().unwrap().is_empty());
    let stable = files(&out);
    for mutation in 0..3 {
        let mut invalid = original.clone();
        let expected = match mutation {
            0 => {
                invalid["datum_dimensions"][0]["direction"] = json!(-invalid["datum_dimensions"]
                    [0]["direction"]
                    .as_i64()
                    .unwrap_or(1));
                "datum_dimension_failed"
            }
            1 => {
                invalid["datum_dimensions"][0]["min"] = json!(10000.);
                invalid["datum_dimensions"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("max");
                "datum_dimension_failed"
            }
            _ => {
                invalid["tolerance_stacks"][0]["min"] = json!(0.25);
                invalid["tolerance_stacks"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("max");
                "tolerance_stack_failed"
            }
        };
        fs::write(&source, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let failed = run(&source, &out, &["--output", "bush"]);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], expected, "{error}");
        assert_eq!(files(&out), stable);
    }
    fs::remove_file(&source).unwrap();
    let replay = temp.path().join("replay");
    let replayed = run(&out.join("recipe.json"), &replay, &[]);
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let replayed: Value =
        serde_json::from_slice(&fs::read(replay.join("datum-dimensions.json")).unwrap()).unwrap();
    assert_eq!(report["dimensions"], replayed["dimensions"]);
    assert!(Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .status()
        .unwrap()
        .success());
}
