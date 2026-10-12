use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

const SOURCE: &str = include_str!("../../../examples/step_materials.star");
fn build(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("build")
        .arg(root.join("source.star"))
        .arg("--out-dir")
        .arg(root.join("out"))
        .output()
        .unwrap()
}
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
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
fn step_material_requirements_pass_while_stl_or_removed_materials_preserve_prior_generation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("source.star"), SOURCE).unwrap();
    let output = build(temp.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["exchange_policy_status"], "passed");
    assert_eq!(
        summary["exchange_requirements"].as_object().unwrap().len(),
        2
    );
    for requirement in summary["exchange_requirements"]
        .as_object()
        .unwrap()
        .values()
    {
        assert_eq!(requirement["status"], "passed");
        assert_eq!(requirement["observed_status"], "supported");
        assert_eq!(requirement["source_count"], 2);
    }
    let out = temp.path().join("out");
    let original = files(&out);
    for (aspect, id) in [
        ("material_names", "materials"),
        ("material_density", "densities"),
    ] {
        let needle = format!("format = \"step\", aspect = \"{aspect}\"");
        let changed = SOURCE.replace(&needle, &format!("format = \"stl\", aspect = \"{aspect}\""));
        assert_ne!(changed, SOURCE);
        fs::write(temp.path().join("source.star"), changed).unwrap();
        let output = build(temp.path());
        assert!(!output.status.success());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "exchange_requirement_failed", "{error}");
        assert_eq!(error["requirement"]["id"], id);
        assert_eq!(error["observed_status"], "not_carried");
        assert_eq!(error["source_count"], 2);
        assert_eq!(files(&out), original);
    }
    let changed = SOURCE
        .replace(
            ", material = \"Steel's α\", density_kg_per_m3 = steel_density",
            "",
        )
        .replace(", material = \"Aluminium β\", density_kg_per_m3 = 2700", "");
    assert_ne!(changed, SOURCE);
    fs::write(temp.path().join("source.star"), changed).unwrap();
    let output = build(temp.path());
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "exchange_requirement_failed", "{error}");
    assert_eq!(error["requirement"]["id"], "materials");
    assert_eq!(error["observed_status"], "not_applicable");
    assert_eq!(error["source_count"], 0);
    assert_eq!(files(&out), original);
}

#[test]
fn density_edit_changes_engineering_identity_and_mass_but_preserves_geometry() {
    let temp = tempfile::tempdir().unwrap();
    let mut records = Vec::new();
    for density in [7800, 7900] {
        fs::write(
            temp.path().join("source.star"),
            SOURCE.replace("default = 7800", &format!("default = {density}")),
        )
        .unwrap();
        let result = build(temp.path());
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let build: Value = serde_json::from_slice(&result.stdout).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_cad"))
            .arg("properties")
            .arg(temp.path().join("source.star"))
            .args(["--output", "weighted"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let properties: Value = serde_json::from_slice(&result.stdout).unwrap();
        let mass = properties["mass_kg"].as_f64().unwrap();
        let expected = 72.0 * std::f64::consts::PI * f64::from(density + 2700) * 1e-9;
        assert!((mass - expected).abs() < 1e-14);
        assert_eq!(properties["mass_status"], "complete");
        records.push((
            build,
            mass,
            fs::read(temp.path().join("out/step_materials.stl")).unwrap(),
        ));
    }
    assert_eq!(
        records[0].0["outputs"]["body"]["content_hash"],
        records[1].0["outputs"]["body"]["content_hash"]
    );
    assert_ne!(
        records[0].0["outputs"]["step_materials"]["content_hash"],
        records[1].0["outputs"]["step_materials"]["content_hash"]
    );
    assert_eq!(records[0].2, records[1].2);
    assert!(
        (records[1].1 - records[0].1 - 72.0 * std::f64::consts::PI * 100.0 * 1e-9).abs() < 1e-14
    );
}
