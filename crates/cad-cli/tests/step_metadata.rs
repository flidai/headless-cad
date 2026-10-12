use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

const SOURCE: &str = include_str!("../../../examples/step_metadata.star");
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
fn step_metadata_requirements_pass_while_stl_or_removed_metadata_preserve_prior_generation() {
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
        3
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
        ("part_numbers", "numbers"),
        ("part_revisions", "revisions"),
        ("part_descriptions", "descriptions"),
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
    let changed = SOURCE.replace(", part_number = \"SHARED-α'42\"", "");
    assert_ne!(changed, SOURCE);
    fs::write(temp.path().join("source.star"), changed).unwrap();
    let output = build(temp.path());
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "exchange_requirement_failed", "{error}");
    assert_eq!(error["requirement"]["id"], "numbers");
    assert_eq!(error["observed_status"], "not_applicable");
    assert_eq!(error["source_count"], 0);
    assert_eq!(files(&out), original);
}
