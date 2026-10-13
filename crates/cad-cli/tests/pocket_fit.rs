use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn inventory(path: &Path) -> BTreeMap<String, Vec<u8>> {
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
fn native_fit_reports_bind_source_and_rejections_preserve_publication() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("seated_pocket_inserts.star");
    fs::write(
        &source,
        include_str!("../../../examples/seated_pocket_inserts.star"),
    )
    .unwrap();
    fs::write(
        temp.path().join("mixed_slot_fixture.star"),
        include_str!("../../../examples/mixed_slot_fixture.star"),
    )
    .unwrap();
    fs::write(
        temp.path().join("seated_pocket_inserts.star.lock.json"),
        include_str!("../../../examples/seated_pocket_inserts.star.lock.json"),
    )
    .unwrap();
    let out = temp.path().join("out");
    let run = |shift: &str| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .arg("build")
            .arg(&source)
            .arg("--out-dir")
            .arg(&out)
            .arg("--cache-dir")
            .arg(temp.path().join("cache"))
            .arg("--param")
            .arg(format!("first_insert_shift={shift}"))
            .output()
            .unwrap()
    };
    let accepted = run("0.1");
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let manifest: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("pocket-fit-checks.json")).unwrap()).unwrap();
    assert_eq!(report["checks"], manifest["pocket_fit_checks"]);
    assert_eq!(
        report["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    let fit = &report["checks"]["first_insert_pocket_fit"];
    assert_eq!(fit["status"], "passed");
    assert!(fit["side_clearance_lower_mm"].as_f64().unwrap() > 0.099999);
    assert_eq!(
        fit["query"]["pocket_wall_faces"].as_array().unwrap().len(),
        4
    );
    for endpoint in ["pocket", "insert"] {
        assert!(fit[endpoint]["semantic_datum"].is_object());
        assert_eq!(
            fit[endpoint]["part_content_hash"].as_str().unwrap().len(),
            64
        );
    }
    let html = fs::read_to_string(out.join("pocket-fit-checks.html")).unwrap();
    assert!(html.contains("first_insert_pocket_fit"));
    assert!(html.contains("recipe.bundle.json"));
    assert!(!html.contains("<script"));
    let before = inventory(&out);
    for shift in ["0.161", "0.3"] {
        let rejected = run(shift);
        assert!(!rejected.status.success());
        let diagnostic: Value = serde_json::from_slice(&rejected.stderr).unwrap();
        assert_eq!(diagnostic["code"], "pocket_fit_check_failed");
        assert_eq!(diagnostic["check_id"], "first_insert_pocket_fit");
        assert_eq!(inventory(&out), before);
    }
    let verified = Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("verify")
        .arg(&out)
        .output()
        .unwrap();
    assert!(verified.status.success());
    fs::write(out.join("pocket-fit-checks.json"), b"{}").unwrap();
    assert!(!Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("verify")
        .arg(&out)
        .output()
        .unwrap()
        .status
        .success());
}
