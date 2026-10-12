use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(p).unwrap(),
            )
        })
        .collect()
}
#[test]
fn analytic_retained_shaft_replays_reuses_and_preserves_failures() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("retained_shaft.star");
    fs::write(
        &source,
        include_str!("../../../examples/retained_shaft.star"),
    )
    .unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |input: &Path, target: &Path, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                input.to_str().unwrap(),
                "--out-dir",
                target.to_str().unwrap(),
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
    assert_eq!(manifest["clearance_checks"].as_object().unwrap().len(), 3);
    for check in manifest["clearance_checks"].as_object().unwrap().values() {
        assert_eq!(check["status"], "passed");
        assert!(matches!(
            check["query"]["method"].as_str(),
            Some("native_trim_witness" | "coaxial_meridian" | "parallel_annular")
        ));
        assert!(check["lower_bound_mm"].as_f64().unwrap() >= 0.09);
        assert!(
            check["upper_bound_mm"].as_f64().unwrap() - check["lower_bound_mm"].as_f64().unwrap()
                < 1e-7
        );
    }
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("clearance-checks.json")).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 3);
    assert_eq!(report["kind"], "native_geometric_clearance");
    let stable = files(&out);
    for parameters in [
        vec!["--param", "required_clearance=0.2", "--output", "bush"],
        vec!["--param", "required_clearance=1", "--output", "bush"],
    ] {
        let failed = run(&source, &out, &parameters);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "clearance_check_failed");
        assert_eq!(stable, files(&out));
    }
    let moved = run(&source, &out, &["--param", "mounting_pitch=72"]);
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let moved: Value = serde_json::from_slice(&moved.stdout).unwrap();
    assert_eq!(
        moved["outputs"]["bush"]["content_hash"],
        manifest["outputs"]["bush"]["content_hash"]
    );
    assert_ne!(
        moved["outputs"]["shaft"]["content_hash"],
        manifest["outputs"]["shaft"]["content_hash"]
    );
    fs::remove_file(&source).unwrap();
    let replay = temp.path().join("replay");
    let replayed = run(
        &out.join("recipe.star"),
        &replay,
        &["--param", "mounting_pitch=72"],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let replayed: Value = serde_json::from_slice(&replayed.stdout).unwrap();
    assert_eq!(moved["clearance_checks"], replayed["clearance_checks"]);
    assert!(Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());
}
