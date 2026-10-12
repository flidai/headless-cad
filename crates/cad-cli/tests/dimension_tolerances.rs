use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn snapshot(path: &Path) -> BTreeMap<String, Vec<u8>> {
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
fn manufacturing_limits_follow_native_dimensions_without_weakening_nominal_checks() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("bearing_limits.star");
    let original = include_str!("../../../examples/bearing_limits.star");
    fs::write(&source, original).unwrap();
    fs::write(
        temp.path().join("datum_section_fixture.star"),
        include_str!("../../../examples/datum_section_fixture.star"),
    )
    .unwrap();
    fs::write(
        temp.path().join("bearing_limits.star.lock.json"),
        include_str!("../../../examples/bearing_limits.star.lock.json"),
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
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("section-pilot_limits.json")).unwrap()).unwrap();
    let circles = report["section_view"]["circle_dimensions"]
        .as_array()
        .unwrap();
    for (id, nominal, lower, upper) in [("bore_size", 6., 6., 6.02), ("pin_size", 5.6, 5.58, 5.6)] {
        let dimension = circles
            .iter()
            .find(|d| d["declaration"]["id"] == id)
            .unwrap();
        assert!((dimension["value_mm"].as_f64().unwrap() - nominal).abs() < 1e-9);
        let tolerance = &dimension["tolerance"];
        assert!((tolerance["lower_limit_mm"].as_f64().unwrap() - lower).abs() < 1e-12);
        assert!((tolerance["upper_limit_mm"].as_f64().unwrap() - upper).abs() < 1e-12);
    }
    let svg = fs::read(out.join("drawing-bearing_limits.svg")).unwrap();
    let revised = run(&source, &out, &["--param", "bore_allowance=0.04"]);
    assert!(
        revised.status.success(),
        "{}",
        String::from_utf8_lossy(&revised.stderr)
    );
    let manifest: Value = serde_json::from_slice(&revised.stdout).unwrap();
    assert_eq!(manifest["stats"]["computed_solids"], 0);
    assert_ne!(
        svg,
        fs::read(out.join("drawing-bearing_limits.svg")).unwrap()
    );
    let after: Value =
        serde_json::from_slice(&fs::read(out.join("section-pilot_limits.json")).unwrap()).unwrap();
    assert_eq!(report["section"], after["section"]);
    let stable = snapshot(&out);
    // A wide manufacturing band must not authorize a wrong nominal dimension.
    fs::write(
        &source,
        original.replace("\"nominal\":6,\"lower\":0", "\"nominal\":6.01,\"lower\":-1"),
    )
    .unwrap();
    let mismatch = run(&source, &out, &["--output", "housing"]);
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("nominal"));
    assert_eq!(stable, snapshot(&out));
    fs::remove_file(&source).unwrap();
    fs::remove_file(temp.path().join("datum_section_fixture.star")).unwrap();
    let replay = temp.path().join("replay");
    let replayed = run(
        &out.join("recipe.bundle.json"),
        &replay,
        &["--param", "bore_allowance=0.04"],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    assert_eq!(
        fs::read(out.join("drawing-bearing_limits.svg")).unwrap(),
        fs::read(replay.join("drawing-bearing_limits.svg")).unwrap()
    );
}
