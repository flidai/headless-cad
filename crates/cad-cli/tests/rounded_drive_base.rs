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
fn verify(report: &Value, out: &Path, radius: f64, pitch: f64) {
    assert_eq!(report["outputs"]["drive_module_release"]["instances"], 24);
    assert_eq!(report["clearance_checks"].as_object().unwrap().len(), 11);
    assert_eq!(
        report["rigid_rotation_groups"]["rotor"]["members"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    assert_eq!(
        report["rigid_rotation_groups"]["rotor"]["status"],
        "named_pairs_passed"
    );
    assert!(report["clearance_checks"]
        .as_object()
        .unwrap()
        .values()
        .all(|c| c["status"] == "passed"));
    let volume = 6.
        * (80. * (pitch + 90.)
            - 36. * std::f64::consts::PI
            - (4. - std::f64::consts::PI) * radius * radius);
    assert!((report["outputs"]["base"]["volume_mm3"].as_f64().unwrap() - volume).abs() < 1e-7);
    let section: Value =
        serde_json::from_slice(&fs::read(out.join("section-mounting_plan.json")).unwrap()).unwrap();
    let dims = section["section_view"]["circle_dimensions"]
        .as_array()
        .unwrap();
    assert_eq!(dims.len(), 4);
    for dimension in dims {
        assert_eq!(dimension["value_mm"], radius);
        assert_eq!(dimension["instance_path"], serde_json::json!(["base"]));
        assert_eq!(
            dimension["selection"]["witnesses"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            dimension["arc"]["target_face"],
            dimension["selection"]["witnesses"][0]["target_face"]
        );
    }
    for feature in [
        "base_front_left",
        "base_front_right",
        "base_back_right",
        "base_body",
    ] {
        let witness = &report["face_assertions"][format!("{feature}_survives")];
        assert_eq!(witness["status"], "resolved");
        assert_eq!(witness["witnesses"].as_array().unwrap().len(), 1);
    }
    assert!(out.join("drawing-drive_module_sections.svg").exists());
}
#[test]
fn rounded_main_drive_preserves_engineering_and_publishes_fitting_native_radius_sheet() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("drive.star");
    let text = include_str!("../../../examples/drive_module_rotation.star");
    fs::write(&source, text).unwrap();
    let out = dir.path().join("release");
    let cache = dir.path().join("cache");
    let first = success(run(&source, &out, &cache, &[]));
    verify(&first, &out, 6., 60.);
    let warm = success(run(&source, &out, &cache, &[]));
    assert_eq!(warm["stats"]["computed_solids"], 0);
    assert_eq!(warm["clearance_checks"], first["clearance_checks"]);
    let replay_out = dir.path().join("replay");
    let replay = success(run(
        &out.join("recipe.star"),
        &replay_out,
        &dir.path().join("fresh"),
        &[],
    ));
    assert_eq!(
        replay["outputs"]["drive_module_release"]["content_hash"],
        first["outputs"]["drive_module_release"]["content_hash"]
    );
    let radius_out = dir.path().join("radius_only");
    let radius_only = success(run(
        &source,
        &radius_out,
        &cache,
        &["--param", "base_corner_radius=2"],
    ));
    verify(&radius_only, &radius_out, 2., 60.);
    assert_eq!(radius_only["stats"]["computed_solids"], 4);
    assert_ne!(
        radius_only["outputs"]["base"]["content_hash"],
        first["outputs"]["base"]["content_hash"]
    );
    for part in [
        "shaft",
        "drive_disc",
        "housing",
        "sleeve",
        "parallel_key",
        "washer",
        "retaining_ring",
    ] {
        assert_eq!(
            radius_only["outputs"][part]["content_hash"],
            first["outputs"][part]["content_hash"]
        );
    }
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    for args in [
        vec!["--param", "base_corner_radius=20"],
        vec!["--output", "base", "--display-only"],
    ] {
        if args[0] == "--output" {
            fs::write(
                &source,
                text.replace(
                    "max = base_corner_radius+0.000001",
                    "max = base_corner_radius-0.000001",
                ),
            )
            .unwrap();
        }
        let bad = run(&source, &out, &cache, &args);
        assert!(!bad.status.success());
        if args[0] == "--output" {
            let diagnostic: Value = serde_json::from_slice(&bad.stderr).unwrap();
            assert_eq!(diagnostic["code"], "section_dimension_failed");
        }
        assert_eq!(manifest, fs::read(out.join("manifest.json")).unwrap());
    }
    fs::write(&source, text).unwrap();
    for (radius, pitch, diameter) in [(2., 72., 14.), (10., 90., 10.)] {
        let r = format!("base_corner_radius={radius}");
        let p = format!("mounting_pitch={pitch}");
        let d = format!("shaft_diameter={diameter}");
        let output = dir.path().join(format!("r{radius}"));
        let variant = success(run(
            &source,
            &output,
            &cache,
            &["--param", &r, "--param", &p, "--param", &d],
        ));
        verify(&variant, &output, radius, pitch);
    }
}
