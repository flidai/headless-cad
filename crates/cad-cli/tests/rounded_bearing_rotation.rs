use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn run(source: &Path, out: &Path, cache: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
        ])
        .args(args)
        .output()
        .unwrap()
}
fn success(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn verify(report: &Value, gap: f64, posed: bool, swapped: bool) {
    let q = &report["clearance_checks"]["running_gap"];
    assert_eq!(q["status"], "passed");
    assert_eq!(q["rotation_group"], "shaft_rotation");
    assert_eq!(q["moving_endpoint"], if swapped { "left" } else { "right" });
    assert_eq!(q["query"]["method"], "bore_trim_witness");
    assert!(q["lower_bound_mm"].as_f64().unwrap() >= 0.1);
    assert!(
        q["lower_bound_mm"].as_f64().unwrap() <= gap
            && q["upper_bound_mm"].as_f64().unwrap() >= gap
    );
    let sweep = &q["rotation_sweep"];
    assert_eq!(sweep["proof_kind"], "bore_witness_orbit");
    assert_eq!(sweep["query_count"], 3);
    assert!(sweep["intervals"].as_array().unwrap().is_empty());
    assert!(sweep["symmetry"].is_null());
    let proof = &sweep["bore_orbit"];
    assert_eq!(proof["start"], 0.);
    assert_eq!(proof["end"], 1.);
    assert_eq!(proof["reference_sample_index"], 0);
    assert!(proof["lower_bound_mm"].as_f64().unwrap() >= 0.1);
    assert!(proof["lower_bound_mm"].as_f64().unwrap() <= gap);
    assert!(proof["upper_bound_mm"].as_f64().unwrap() >= gap);
    assert!(proof["bore_face_index"].as_u64().is_some());
    let samples = sweep["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 3);
    for time in [0., 0.5, 1.] {
        assert!(samples.iter().any(|s| s["time"].as_f64() == Some(time)));
    }
    for sample in samples {
        let points = |name: &str| {
            let a = sample["query"][name].as_array().unwrap();
            let p = [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ];
            if posed {
                [p[1] + 7., p[2] - 15., p[0] - 13.]
            } else {
                p
            }
        };
        // Kernel sweep convention always uses stationary housing on the left.
        let housing = points("left_point");
        let shaft = points("right_point");
        assert!((housing[0].hypot(housing[1]) - (6. + gap)).abs() < 1e-6);
        assert!(housing[2] >= -1e-6 && housing[2] <= 12. + 1e-6);
        let angle = sample["time"].as_f64().unwrap() * sweep["angle_radians"].as_f64().unwrap();
        let (sin, cos) = angle.sin_cos();
        let x = cos * shaft[0] + sin * shaft[1];
        let y = -sin * shaft[0] + cos * shaft[1];
        assert!((x.hypot(y) - 6.).abs() < 1e-6);
        assert!(
            x <= 4. + 1e-6
                || y.abs() >= 2. - 1e-6
                || shaft[2] <= 6. + 1e-6
                || shaft[2] >= 14. - 1e-6
        );
    }
    let group = &report["rigid_rotation_groups"]["shaft_rotation"];
    assert_eq!(group["status"], "named_pairs_passed");
    assert_eq!(group["members"].as_array().unwrap().len(), 1);
    assert_eq!(
        group["members"][0]["instance_path"],
        serde_json::json!(["fixture", "shaft"])
    );
}
#[test]
fn bore_orbit_preserves_actual_witnesses_and_global_source_obligations() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("rotation.star");
    let text = include_str!("../../../examples/rounded_bearing_rotation.star");
    fs::write(&source, text).unwrap();
    let out = dir.path().join("out");
    let cache = dir.path().join("cache");
    let first = success(run(&source, &out, &cache, &[]));
    verify(&first, 0.15, false, false);
    let warm = success(run(&source, &out, &cache, &[]));
    assert_eq!(warm["stats"]["computed_solids"], 0);
    assert_eq!(warm["clearance_checks"], first["clearance_checks"]);
    let replay = success(run(
        &out.join("recipe.star"),
        &dir.path().join("replay"),
        &dir.path().join("fresh"),
        &[],
    ));
    assert_eq!(replay["clearance_checks"], first["clearance_checks"]);
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    for args in [
        vec!["--param", "bore_gap=0.05"],
        vec!["--param", "required_clearance=0.1500001"],
    ] {
        let mut all = args;
        all.extend(["--output", "shaft", "--display-only"]);
        let bad = run(&source, &out, &cache, &all);
        assert!(!bad.status.success());
        let error: Value = serde_json::from_slice(&bad.stderr).unwrap();
        assert_eq!(error["code"], "clearance_check_failed");
        assert_eq!(manifest, fs::read(out.join("manifest.json")).unwrap());
    }
    // Noncoaxial orbit has identical free endpoints but loses .06mm at half-turn.
    // Explicit domain refusal is required; no general offset-orbit claim is made.
    fs::write(
        &source,
        text.replace("pivot = [0,0,20]", "pivot = [0.03,0,20]"),
    )
    .unwrap();
    let bad = run(
        &source,
        &out,
        &cache,
        &["--output", "housing", "--display-only"],
    );
    assert!(!bad.status.success());
    assert_eq!(manifest, fs::read(out.join("manifest.json")).unwrap());
    let swapped=text.replace("left_instance_path = [\"fixture\",\"housing\"], right_instance_path = [\"fixture\",\"shaft\"]","left_instance_path = [\"fixture\",\"shaft\"], right_instance_path = [\"fixture\",\"housing\"]");
    fs::write(&source, swapped).unwrap();
    let opposite = success(run(
        &source,
        &dir.path().join("opposite"),
        &cache,
        &[
            "--param",
            "pose=1",
            "--param",
            "turn_radians=-6.283185307179586",
        ],
    ));
    verify(&opposite, 0.15, true, true);
}
