//! Protocol replay of an externally proposed source correction; no model service.
use serde_json::Value;
use std::{fs, process::Command};
const PROPOSAL: &str = r#"
tool_depth = 4
stock = extrude(id = "stock", profile = rectangle(id = "plate_profile", width = 40, depth = 30), height = 8)
tool = extrude(id = "tool", profile = circle(id = "bore_profile", radius = 3), height = tool_depth)
positioned = translate(id = "positioned", body = tool, offset = [20,15,-2])
drilled = cut(id = "drill", left = stock, right = positioned)
output(name = "plate", feature = drilled)
"#;
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn good(args: &[&str]) -> Value {
    let result = cli(args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
#[test]
fn discover_diagnose_patch_and_replay_an_engineering_revision() {
    let catalog = good(&["capabilities"]);
    assert_eq!(catalog["schema_version"], 1);
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("part.star");
    let source = recipe.to_str().unwrap();
    fs::write(&recipe, PROPOSAL).unwrap();
    // A syntactically valid proposal can still violate the geometry contract.
    let ir = good(&["compile", source]);
    assert!(catalog["recipe_schema_versions"]
        .as_array()
        .unwrap()
        .contains(&ir["schema_version"]));
    let rejected = cli(&["check", source]);
    assert!(!rejected.status.success());
    let failure: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(failure["code"], "kernel_operation_failed");
    assert_eq!(failure["feature"], "drill");
    assert!(
        failure["snapshot"]["kernel_build_id"]
            .as_str()
            .unwrap()
            .len()
            > 10
    );
    assert_eq!(fs::read_to_string(&recipe).unwrap(), PROPOSAL);
    // Simulate an accepted, reviewable source patch. No geometry or viewer mutation.
    let corrected = PROPOSAL.replace("tool_depth = 4", "tool_depth = 12");
    fs::write(&recipe, &corrected).unwrap();
    good(&["check", source]);
    let properties = good(&["properties", source, "--output", "plate"]);
    let expected = (1200. - 9. * std::f64::consts::PI) * 8.;
    assert!((properties["volume"].as_f64().unwrap() - expected).abs() < 1e-7);
    let first = dir.path().join("first");
    let second = dir.path().join("replay");
    let a = good(&["build", source, "--out-dir", first.to_str().unwrap()]);
    let b = good(&["build", source, "--out-dir", second.to_str().unwrap()]);
    assert!(a["outputs"]["plate"]["content_hash"].is_string());
    assert_eq!(
        a["outputs"]["plate"]["content_hash"],
        b["outputs"]["plate"]["content_hash"]
    );
    for name in ["recipe.star", "plate.brep.json", "plate.step", "plate.stl"] {
        assert_eq!(
            fs::read(first.join(name)).unwrap(),
            fs::read(second.join(name)).unwrap(),
            "{name}"
        );
    }
    assert_eq!(
        fs::read_to_string(first.join("recipe.star")).unwrap(),
        corrected
    );
}
