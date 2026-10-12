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
fn stack_publication_replays_and_preserves_failed_selected_build() {
    let temp = tempfile::tempdir().unwrap();
    for (name, body) in [
        (
            "bearing_clearance.star",
            include_str!("../../../examples/bearing_clearance.star"),
        ),
        (
            "bearing_clearance.star.lock.json",
            include_str!("../../../examples/bearing_clearance.star.lock.json"),
        ),
        (
            "bearing_limits.star",
            include_str!("../../../examples/bearing_limits.star"),
        ),
        (
            "datum_section_fixture.star",
            include_str!("../../../examples/datum_section_fixture.star"),
        ),
    ] {
        fs::write(temp.path().join(name), body).unwrap();
    }
    let source = temp.path().join("bearing_clearance.star");
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |source: &Path, out: &Path, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                source.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
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
    let evidence: Value =
        serde_json::from_slice(&fs::read(out.join("tolerance-stacks.json")).unwrap()).unwrap();
    assert_eq!(
        evidence["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(evidence["stacks"], manifest["tolerance_stacks"]);
    let r = &evidence["stacks"]["diametral_clearance"];
    assert!((r["lower_limit_mm"].as_f64().unwrap() - 0.4).abs() < 1e-12);
    assert!((r["upper_limit_mm"].as_f64().unwrap() - 0.44).abs() < 1e-12);
    for report in evidence["section_reports"].as_object().unwrap().values() {
        let bytes = fs::read(out.join(report["file"].as_str().unwrap())).unwrap();
        assert_eq!(report["digest"], blake3::hash(&bytes).to_hex().to_string());
    }
    let stable = files(&out);
    let fail = run(
        &source,
        &out,
        &["--param", "bore_allowance=0.04", "--output", "housing"],
    );
    assert!(!fail.status.success());
    let failure: Value = serde_json::from_slice(&fail.stderr).unwrap();
    assert_eq!(failure["code"], "tolerance_stack_failed");
    assert_eq!(failure["stack_id"], "diametral_clearance");
    assert_eq!(stable, files(&out));
    let revised = run(&source, &out, &["--param", "bore_allowance=0.01"]);
    assert!(revised.status.success());
    let revised: Value = serde_json::from_slice(&revised.stdout).unwrap();
    assert_eq!(revised["stats"]["computed_solids"], 0);
    assert_ne!(
        manifest["recipe_context_hash"],
        revised["recipe_context_hash"]
    );
    for name in [
        "bearing_clearance.star",
        "bearing_limits.star",
        "datum_section_fixture.star",
    ] {
        fs::remove_file(temp.path().join(name)).unwrap();
    }
    let replay = temp.path().join("replay");
    let replayed = run(
        &out.join("recipe.bundle.json"),
        &replay,
        &["--param", "bore_allowance=0.01"],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let original: Value =
        serde_json::from_slice(&fs::read(out.join("tolerance-stacks.json")).unwrap()).unwrap();
    let reproduced: Value =
        serde_json::from_slice(&fs::read(replay.join("tolerance-stacks.json")).unwrap()).unwrap();
    assert_eq!(original["stacks"], reproduced["stacks"]);
    let verified = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(verified.status.success());
}
