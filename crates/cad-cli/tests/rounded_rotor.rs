use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn invoke(source: &Path, out: &Path, cache: &Path, extra: &[&str]) -> std::process::Output {
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
}
fn successful(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn certificate(report: &Value, radius: f64) {
    let check = &report["clearance_checks"]["rotor_roller_path"];
    assert_eq!(check["status"], "passed");
    assert_eq!(check["query"]["method"], "convex_support");
    let actual = 16. - ((10. - radius).hypot(6. - radius) + radius);
    assert!(check["lower_bound_mm"].as_f64().unwrap() >= 5.);
    assert!(check["lower_bound_mm"].as_f64().unwrap() <= actual);
    assert!(check["upper_bound_mm"].as_f64().unwrap() >= actual);
    let sweep = &check["rotation_sweep"];
    assert_eq!(sweep["proof_kind"], "lipschitz_subdivision");
    assert!(sweep["symmetry"].is_null());
    let samples = sweep["samples"].as_array().unwrap();
    assert_eq!(
        samples.len(),
        sweep["query_count"].as_u64().unwrap() as usize
    );
    assert!(samples.len() > 3 && samples.len() <= 1025);
    let mut end = 0.;
    for interval in sweep["intervals"].as_array().unwrap() {
        assert_eq!(interval["start"].as_f64().unwrap(), end);
        end = interval["end"].as_f64().unwrap();
        assert!(interval["lower_bound_mm"].as_f64().unwrap() >= 5.);
        let sample = &samples[interval["sample_index"].as_u64().unwrap() as usize];
        assert_eq!(sample["time"], interval["midpoint"]);
        assert_eq!(sample["query"]["method"], "convex_support");
    }
    assert_eq!(end, 1.);
    for t in [0., 0.5, 1.] {
        let sample = samples
            .iter()
            .find(|s| s["time"].as_f64() == Some(t))
            .unwrap();
        assert!((sample["query"]["upper_bound_mm"].as_f64().unwrap() - 6.).abs() < 1e-6);
    }
}
#[test]
fn real_rounded_rotor_whole_turn_proof_replays_and_failed_interior_edit_is_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.star");
    let text = include_str!("../../../examples/rounded_rotor_fixture.star");
    fs::write(&source, text).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let original = successful(invoke(&source, &out, &cache, &[]));
    certificate(&original, 2.);
    assert_eq!(original["face_assertions"].as_object().unwrap().len(), 4);
    let section: Value =
        serde_json::from_slice(&fs::read(out.join("section-rotor_midsection.json")).unwrap())
            .unwrap();
    let dims = section["section_view"]["circle_dimensions"]
        .as_array()
        .unwrap();
    assert_eq!(dims.len(), 4);
    assert!(dims
        .iter()
        .all(|d| d["value_mm"] == 2. && d["arc"].is_object()));
    let warm = successful(invoke(&source, &out, &cache, &[]));
    assert_eq!(warm["stats"]["computed_solids"], 0);
    assert_eq!(warm["clearance_checks"], original["clearance_checks"]);
    let replay = successful(invoke(
        &out.join("recipe.star"),
        &temp.path().join("replay"),
        &temp.path().join("fresh_cache"),
        &[],
    ));
    assert_eq!(replay["clearance_checks"], original["clearance_checks"]);
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    // R1 retains a 6mm gap at t=0,.5,1 but dips to4.70436986 inside the arc.
    let failed = invoke(
        &source,
        &out,
        &cache,
        &[
            "--param",
            "corner_radius=1",
            "--output",
            "rotor_inspection",
            "--display-only",
        ],
    );
    assert!(!failed.status.success());
    let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "clearance_check_failed");
    assert_eq!(manifest, fs::read(out.join("manifest.json")).unwrap());
    for (radius, pose, turn) in [
        (3., "0", "6.283185307179586"),
        (2., "1", "-6.283185307179586"),
    ] {
        let r = format!("corner_radius={radius}");
        let p = format!("pose={pose}");
        let a = format!("turn_radians={turn}");
        let variant = successful(invoke(
            &source,
            &temp.path().join(format!("variant{pose}")),
            &cache,
            &["--param", &r, "--param", &p, "--param", &a],
        ));
        certificate(&variant, radius);
    }
}
