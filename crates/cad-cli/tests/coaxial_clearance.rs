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
fn analytic_coaxial_clearance_replays_reuses_and_preserves_failures() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("coaxial_clearance.star");
    fs::write(
        &source,
        include_str!("../../../examples/coaxial_clearance.star"),
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
    assert_eq!(manifest["clearance_checks"].as_object().unwrap().len(), 2);
    for check in manifest["clearance_checks"].as_object().unwrap().values() {
        assert_eq!(check["status"], "passed");
        assert_eq!(check["query"]["method"], "coaxial_meridian");
        assert!(check["lower_bound_mm"].as_f64().unwrap() >= 0.5);
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
        vec!["--param", "gap=0", "--output", "bearing"],
        vec!["--param", "gap=-0.5"],
        vec!["--param", "oversize=1", "--output", "bearing"],
        vec!["--param", "eccentric=1"],
        vec!["--param", "tilt=1"],
        vec!["--param", "axial_gap=0"],
        vec!["--param", "required_clearance=100", "--output", "bearing"],
    ] {
        let failed = run(&source, &out, &parameters);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "clearance_check_failed");
        assert_eq!(stable, files(&out));
    }
    let moved = run(&source, &out, &["--param", "slide=36"]);
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let moved: Value = serde_json::from_slice(&moved.stdout).unwrap();
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_ne!(
        moved["clearance_checks"]["shaft_bore"]["lower_bound_mm"],
        manifest["clearance_checks"]["shaft_bore"]["lower_bound_mm"]
    );
    let analytic = run(
        &source,
        &out,
        &[
            "--param",
            "shape=0",
            "--param",
            "opposed=1",
            "--param",
            "pose=0",
        ],
    );
    assert!(
        analytic.status.success(),
        "{}",
        String::from_utf8_lossy(&analytic.stderr)
    );
    let analytic: Value = serde_json::from_slice(&analytic.stdout).unwrap();
    for (name, distance) in [("shaft_bore", 1.), ("bearing_spacer", 6.)] {
        let c = &analytic["clearance_checks"][name];
        assert!(
            c["lower_bound_mm"].as_f64().unwrap() <= distance
                && c["upper_bound_mm"].as_f64().unwrap() >= distance
        );
    }
    fs::remove_file(&source).unwrap();
    let replay = temp.path().join("replay");
    let replayed = run(
        &out.join("recipe.star"),
        &replay,
        &[
            "--param",
            "shape=0",
            "--param",
            "opposed=1",
            "--param",
            "pose=0",
        ],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let replayed: Value = serde_json::from_slice(&replayed.stdout).unwrap();
    assert_eq!(analytic["clearance_checks"], replayed["clearance_checks"]);
    assert!(Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());
}
