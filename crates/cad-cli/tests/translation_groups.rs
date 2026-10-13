use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn translation_publication_binds_native_proof_source_and_read_only_inspector() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("adjustment.star");
    fs::write(
        &source,
        include_str!("../../../examples/slotted_clamp_adjustment.star"),
    )
    .unwrap();
    let out = temp.path().join("out");
    let built = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let manifest: Value = serde_json::from_slice(&built.stdout).unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("rigid-translation-groups.json")).unwrap())
            .unwrap();
    assert_eq!(report["groups"], manifest["rigid_translation_groups"]);
    assert_eq!(
        report["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(
        report["groups"]["adjustment"]["translation_world"],
        serde_json::json!([6., 0., 0.])
    );
    let check = &manifest["clearance_checks"]["slot_shank_running_gap"];
    assert_eq!(check["status"], "passed");
    assert_eq!(check["query"]["method"], "slot_trim_witness");
    assert_eq!(check["sweep"]["query_count"], 2);
    assert!(check["sweep"]["slot_motion_proof"].is_object());
    let html = fs::read_to_string(out.join("translation-adjustment.html")).unwrap();
    assert!(html.contains("cad-rigid-translation-data"));
    assert!(html.contains("Read-only translation phase"));
    assert!(html.contains("All members share one rigid translation"));
    let verified = Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("verify")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
}
