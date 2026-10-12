use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn run(args: &[&str]) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
#[test]
fn separated_stock_batch_retains_exact_solids_lineage_and_later_boolean_replay() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/billet_batch.star");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("batch");
    let built = run(&[
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    let selection = &built["face_assertions"]["stock_top_lineage"];
    assert_eq!(selection["status"], "resolved");
    assert_eq!(selection["witnesses"].as_array().unwrap().len(), 64);
    assert!(selection["witnesses"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w["anchor_faces"] == serde_json::json!([1])));
    let batch = &built["outputs"]["batch"];
    assert_eq!(batch["topology"]["solids"], 64);
    assert_eq!(batch["topology"]["faces"], 384);
    assert!((batch["volume_mm3"].as_f64().unwrap() - 11520.).abs() < 1e-8);
    assert_eq!(built["outputs"]["trimmed"]["topology"]["solids"], 64);
    assert!((built["outputs"]["trimmed"]["volume_mm3"].as_f64().unwrap() - 11484.).abs() < 1e-8);
    let properties = run(&["properties", source.to_str().unwrap(), "--output", "batch"]);
    for (actual, expected) in properties["volume_centroid"]
        .as_array()
        .unwrap()
        .iter()
        .zip([47., 31., 1.5])
    {
        assert!((actual.as_f64().unwrap() - expected).abs() < 1e-9);
    }
    run(&["verify", out.to_str().unwrap()]);
    assert_eq!(
        fs::read_to_string(out.join("batch.step"))
            .unwrap()
            .matches("FACETED_BREP(")
            .count(),
        64
    );
    let replay = run(&["inspect", out.join("recipe.star").to_str().unwrap()]);
    for name in ["batch", "trimmed"] {
        assert_eq!(
            built["outputs"][name]["content_hash"],
            replay["outputs"][name]["content_hash"]
        );
    }
    let selected = run(&["inspect", source.to_str().unwrap(), "--output", "batch"]);
    assert_eq!(selected["face_assertions"], built["face_assertions"]);
    for (rows, columns) in [(1, 1), (12, 12)] {
        let r = format!("rows={rows}");
        let c = format!("columns={columns}");
        let variant = run(&[
            "inspect",
            source.to_str().unwrap(),
            "--param",
            &r,
            "--param",
            &c,
        ]);
        assert_eq!(
            variant["outputs"]["batch"]["topology"]["solids"],
            rows * columns
        );
        assert!(
            (variant["outputs"]["batch"]["volume_mm3"].as_f64().unwrap()
                - 180. * f64::from(rows * columns))
            .abs()
                < 1e-7
        );
    }
}
