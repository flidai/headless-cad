use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

const SOURCE: &str = include_str!("../../../examples/countersink.star");

fn build(root: &Path, selected: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cad"));
    command.arg("build").arg(root.join("source.star"));
    command.arg("--out-dir").arg(root.join("out"));
    command.arg("--cache-dir").arg(root.join("cache"));
    if selected {
        command.args(["--output", "stock"]);
    }
    command.output().unwrap()
}

fn artifacts(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

#[test]
fn countersink_edits_rebuild_affected_geometry_and_reject_invalid_publication() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("source.star");
    fs::write(&source_path, SOURCE).unwrap();
    let original = build(temp.path(), false);
    assert!(
        original.status.success(),
        "{}",
        String::from_utf8_lossy(&original.stderr)
    );
    let original: Value = serde_json::from_slice(&original.stdout).unwrap();
    let changed = SOURCE.replace("default = 4", "default = 5");
    fs::write(&source_path, &changed).unwrap();
    let edited = build(temp.path(), false);
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let edited: Value = serde_json::from_slice(&edited.stdout).unwrap();
    assert_eq!(
        original["outputs"]["stock"]["content_hash"],
        edited["outputs"]["stock"]["content_hash"]
    );
    assert_ne!(
        original["outputs"]["countersunk"]["content_hash"],
        edited["outputs"]["countersunk"]["content_hash"]
    );
    let replay = build(temp.path(), false);
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let replay: Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(replay["stats"]["computed_solids"], 0);
    let saved = artifacts(&temp.path().join("out"));
    for (label, invalid, expected_code) in [
        (
            "noncoaxial",
            changed.replace("offset = [30,20,12-depth]", "offset = [31,20,12-depth]"),
            "kernel_operation_failed",
        ),
        (
            "boundary",
            changed.replace("offset = [30,20,12-depth]", "offset = [2,20,12-depth]"),
            "kernel_operation_failed",
        ),
        (
            "lineage",
            changed.replace(
                "in_feature = finished, count = 1",
                "in_feature = finished, count = 2",
            ),
            "face_assertion_failed",
        ),
    ] {
        assert_ne!(invalid, changed);
        fs::write(&source_path, invalid).unwrap();
        let result = build(temp.path(), true);
        assert!(!result.status.success(), "accepted {label}");
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["code"], expected_code, "{label}: {error}");
        assert_eq!(saved, artifacts(&temp.path().join("out")), "{label}");
    }
}
