use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const SOURCE: &str = include_str!("../../../examples/sketched_blind_housing.star");

fn cad(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(arguments)
        .output()
        .expect("run CAD CLI")
}

fn good(arguments: &[&str]) -> Value {
    let result = cad(arguments);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).expect("CAD JSON result")
}

fn inventory(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .expect("published generation")
        .map(|entry| {
            let entry = entry.expect("artifact entry");
            assert!(entry.file_type().expect("artifact type").is_file());
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("artifact bytes"),
            )
        })
        .collect()
}

#[test]
fn face_attached_solved_blind_holes_publish_and_failure_keeps_entire_generation() {
    let directory = tempfile::tempdir().unwrap();
    let recipe = directory.path().join("housing.star");
    let output = directory.path().join("generation");
    fs::write(&recipe, SOURCE).unwrap();
    let recipe_arg = recipe.to_str().unwrap();
    let output_arg = output.to_str().unwrap();
    let manifest = good(&["build", recipe_arg, "--out-dir", output_arg]);
    let expected_volume =
        (50.0 * 36.0 - (4.0 - std::f64::consts::PI) * 16.0 - std::f64::consts::PI * 49.0) * 12.0
            - 2.0 * std::f64::consts::PI * 4.0 * 5.0;
    let volume = manifest["outputs"]["housing"]["volume_mm3"]
        .as_f64()
        .unwrap();
    assert!((volume - expected_volume).abs() < 1e-6, "{volume}");
    assert_eq!(
        manifest["property_assertions"]["exact_housing_volume"]["status"],
        "passed"
    );
    assert_eq!(manifest["section_views"]["clamp_holes"]["status"], "passed");
    assert_eq!(
        manifest["section_views"]["clamp_holes"]["circle_dimensions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(manifest["sketch_reports"]["housing"].is_object());
    for id in [
        "clamp_left_wall",
        "clamp_left_floor",
        "clamp_right_wall",
        "clamp_right_floor",
        "central_bore_survives",
    ] {
        assert_eq!(
            manifest["face_assertions"][id]["status"], "resolved",
            "{id}"
        );
    }
    assert!(good(&["verify", output_arg])["valid"].as_bool().unwrap());
    let original = inventory(&output);
    assert!(original.contains_key("housing.step"));
    assert!(original.contains_key("housing.brep.json"));
    assert!(original.contains_key("section-clamp_holes.svg"));

    let invalid_floor = cad(&[
        "build",
        recipe_arg,
        "--out-dir",
        output_arg,
        "--param",
        "hole_depth=12",
    ]);
    assert!(
        !invalid_floor.status.success(),
        "floor must remain inside stock"
    );
    assert_eq!(
        inventory(&output),
        original,
        "invalid floor changed generation"
    );

    let incomplete = SOURCE.replace(
        "        {\"id\":\"center_axis\",\"kind\":\"horizontal\",\"a\":\"center_left\",\"b\":\"center_right\"},\n",
        "",
    );
    assert_ne!(incomplete, SOURCE);
    let invalid_recipe = directory.path().join("underconstrained.star");
    fs::write(&invalid_recipe, incomplete).unwrap();
    let invalid_result = cad(&[
        "build",
        invalid_recipe.to_str().unwrap(),
        "--out-dir",
        output_arg,
    ]);
    assert!(
        !invalid_result.status.success(),
        "underconstrained sketch must fail"
    );
    assert_eq!(
        inventory(&output),
        original,
        "invalid sketch changed generation"
    );
    assert!(good(&["verify", output_arg])["valid"].as_bool().unwrap());
}
