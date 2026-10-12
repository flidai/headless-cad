use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
fn inventory(path: &Path) -> BTreeMap<String, Vec<u8>> {
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
fn keyed_shaft_revisions_replay_and_preserve_failed_publication() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("keyed_shaft.star");
    fs::write(&source, include_str!("../../../examples/keyed_shaft.star")).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |source: &Path, output: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                source.to_str().unwrap(),
                "--out-dir",
                output.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
            ])
            .args(args)
            .output()
            .unwrap()
    };
    let result = run(&source, &out, &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let first: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        first["outputs"]["keyed_shaft"]["volume_mm3"]
            .as_f64()
            .unwrap()
            < first["outputs"]["stock"]["volume_mm3"].as_f64().unwrap()
    );
    let stable = inventory(&out);
    for args in [
        vec!["--param", "key_start=0"],
        vec!["--param", "key_depth=6", "--output", "stock"],
    ] {
        let result = run(&source, &out, &args);
        assert!(!result.status.success());
        assert_eq!(inventory(&out), stable);
    }
    let result = run(&source, &out, &["--param", "key_length=16"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let revised: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_ne!(
        first["outputs"]["keyed_shaft"]["content_hash"],
        revised["outputs"]["keyed_shaft"]["content_hash"]
    );
    assert_eq!(
        first["outputs"]["stock"]["content_hash"],
        revised["outputs"]["stock"]["content_hash"]
    );
    fs::remove_file(&source).unwrap();
    let replay = temp.path().join("replay");
    let result = run(
        &out.join("recipe.star"),
        &replay,
        &["--param", "key_length=16"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(out.join("keyed_shaft.step")).unwrap(),
        fs::read(replay.join("keyed_shaft.step")).unwrap()
    );
    assert!(Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());
}
