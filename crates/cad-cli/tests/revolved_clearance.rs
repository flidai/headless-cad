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
fn analytic_revolved_clearance_replays_reuses_and_preserves_failures() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("revolved_clearance.star");
    fs::write(
        &source,
        include_str!("../../../examples/revolved_clearance.star"),
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
        assert!(check["lower_bound_mm"].as_f64().unwrap() >= 2.);
        assert!(
            check["upper_bound_mm"].as_f64().unwrap() - check["lower_bound_mm"].as_f64().unwrap()
                < 1e-7
        );
    }
    let stable = files(&out);
    for parameters in [
        vec!["--param", "shape=3", "--output", "tool"],
        vec!["--param", "shape=4", "--output", "tool"],
        vec!["--param", "gap=-10", "--param", "turn=0"],
        vec!["--param", "required_clearance=100", "--output", "tool"],
    ] {
        let failed = run(&source, &out, &parameters);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "clearance_check_failed");
        assert_eq!(stable, files(&out));
    }
    let moved = run(&source, &out, &["--param", "gap=20"]);
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let moved: Value = serde_json::from_slice(&moved.stdout).unwrap();
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_ne!(
        moved["clearance_checks"]["tool_pair"]["lower_bound_mm"],
        manifest["clearance_checks"]["tool_pair"]["lower_bound_mm"]
    );
    let analytic = run(
        &source,
        &out,
        &[
            "--param",
            "turn=0",
            "--param",
            "guard_turn=0",
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
    for (name, distance) in [("tool_pair", 10.), ("tool_guard", 18.)] {
        let c = &analytic["clearance_checks"][name];
        assert!(
            c["lower_bound_mm"].as_f64().unwrap() <= distance
                && c["upper_bound_mm"].as_f64().unwrap() >= distance
        );
    }
    for shape in ["shape=1", "shape=2"] {
        let shaped = run(&source, &temp.path().join(shape), &["--param", shape]);
        assert!(
            shaped.status.success(),
            "{}",
            String::from_utf8_lossy(&shaped.stderr)
        );
        let shaped: Value = serde_json::from_slice(&shaped.stdout).unwrap();
        assert_eq!(shaped["clearance_checks"]["tool_pair"]["status"], "passed");
    }
    fs::remove_file(&source).unwrap();
    let replay = temp.path().join("replay");
    let replayed = run(
        &out.join("recipe.star"),
        &replay,
        &[
            "--param",
            "turn=0",
            "--param",
            "guard_turn=0",
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
