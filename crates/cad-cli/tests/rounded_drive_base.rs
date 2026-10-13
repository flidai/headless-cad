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
fn verify(report: &Value, out: &Path, radius: f64, pitch: f64, slot_length: f64) {
    assert_eq!(report["outputs"]["drive_module_release"]["instances"], 30);
    assert_eq!(report["outputs"]["drive_module_release"]["definitions"], 14);
    assert_eq!(report["clearance_checks"].as_object().unwrap().len(), 13);
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
    for (id, required, nominal) in [
        ("left_cover_shaft_gap", 2., 2.5),
        ("right_cover_shaft_gap", 4., 4.5),
    ] {
        let check = &report["clearance_checks"][id];
        assert_eq!(check["query"]["method"], "bore_trim_witness");
        assert_eq!(check["motion_proof"], "continuous_rotation");
        assert_eq!(check["moving_endpoint"], "right");
        assert_eq!(check["rotation_sweep"]["proof_kind"], "bore_witness_orbit");
        assert_eq!(check["rotation_sweep"]["query_count"], 3);
        assert!(check["rotation_sweep"]["bore_orbit"].is_object());
        assert!(check["lower_bound_mm"].as_f64().unwrap() >= required);
        assert!((check["lower_bound_mm"].as_f64().unwrap() - nominal).abs() < 1e-4);
    }
    let volume = 6.
        * (80. * (pitch + 90.)
            - 4. * (9. * std::f64::consts::PI + 6. * slot_length)
            - (4. - std::f64::consts::PI) * radius * radius);
    assert!((report["outputs"]["base"]["volume_mm3"].as_f64().unwrap() - volume).abs() < 1e-7);
    let section: Value =
        serde_json::from_slice(&fs::read(out.join("section-mounting_plan.json")).unwrap()).unwrap();
    let base_section = section["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["instance_path"] == serde_json::json!(["base"]))
        .unwrap();
    let slots: Vec<_> = base_section["section"]["loops"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|loop_| loop_["segments"].as_array().unwrap().len() == 4)
        .collect();
    assert_eq!(slots.len(), 4);
    for slot in slots {
        let segments = slot["segments"].as_array().unwrap();
        let arcs: Vec<_> = segments
            .iter()
            .filter(|s| s["curve"]["kind"] == "arc")
            .collect();
        assert_eq!(arcs.len(), 2);
        for arc in arcs {
            assert!((arc["curve"]["radius"].as_f64().unwrap() - 3.).abs() < 1e-9);
            assert!(
                (arc["curve"]["sweep_angle"].as_f64().unwrap().abs() - std::f64::consts::PI).abs()
                    < 1e-9
            );
        }
    }
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
    for side in ["left", "right"] {
        for tag in ["minus", "plus"] {
            for suffix in ["wall", "floor"] {
                let key = format!("{side}_cover_{tag}_{suffix}");
                assert_eq!(report["face_assertions"][&key]["status"], "resolved");
                assert_eq!(
                    report["face_assertions"][&key]["witnesses"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
            }
        }
        let section: Value = serde_json::from_slice(
            &fs::read(out.join(format!("section-{side}_cover_pilot_section.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            section["section_view"]["circle_dimensions"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    assert!(out.join("section-end_cover_section.json").exists());
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
    verify(&first, &out, 6., 60., 6.);
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
    verify(&radius_only, &radius_out, 2., 60., 6.);
    assert_eq!(radius_only["stats"]["computed_solids"], 5);
    assert_ne!(
        radius_only["outputs"]["base"]["content_hash"],
        first["outputs"]["base"]["content_hash"]
    );
    for part in [
        "shaft",
        "drive_disc",
        "housing",
        "right_housing",
        "end_cover",
        "cover_screw_envelope",
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
    let slot_out = dir.path().join("slot_revision");
    let slot_revision = success(run(
        &source,
        &slot_out,
        &cache,
        &["--param", "base_slot_length=8"],
    ));
    verify(&slot_revision, &slot_out, 6., 60., 8.);
    assert_eq!(slot_revision["stats"]["computed_solids"], 1);
    assert_eq!(
        slot_revision["feature_hashes"]["base_body"],
        first["feature_hashes"]["base_body"]
    );
    assert_ne!(
        slot_revision["outputs"]["base"]["content_hash"],
        first["outputs"]["base"]["content_hash"]
    );
    for part in [
        "shaft",
        "drive_disc",
        "housing",
        "right_housing",
        "end_cover",
        "cover_screw_envelope",
        "sleeve",
    ] {
        assert_eq!(
            slot_revision["outputs"][part]["content_hash"],
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
        verify(&variant, &output, radius, pitch, 6.);
    }
}
