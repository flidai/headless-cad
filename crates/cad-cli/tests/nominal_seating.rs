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
fn seating_publication_replays_and_failed_changes_preserve_the_generation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("nominal_seating.star");
    let original = include_str!("../../../examples/nominal_seating.star");
    fs::write(&source, original).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |input: &Path, directory: &Path| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .arg("build")
            .arg(input)
            .arg("--out-dir")
            .arg(directory)
            .arg("--cache-dir")
            .arg(&cache)
            .output()
            .unwrap()
    };
    let first = run(&source, &out);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let manifest: Value = serde_json::from_slice(&first.stdout).unwrap();
    let bytes = fs::read(out.join("nominal-seating-checks.json")).unwrap();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["checks"], manifest["nominal_seating_checks"]);
    assert_eq!(
        report["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    let check = &report["checks"]["flange_plate_seat"];
    assert_eq!(check["status"], "passed");
    assert!(check["signed_gap_lower_mm"].as_f64().unwrap() <= 0.);
    assert!(check["signed_gap_upper_mm"].as_f64().unwrap() >= 0.);
    assert!(check["disk_radius_lower_mm"].as_f64().unwrap() >= 0.2);
    assert!(check["disk_area_lower_mm2"].as_f64().unwrap() > 0.);
    for end in ["left", "right"] {
        assert!(check[end]["semantic_datum"].is_object());
        assert_eq!(check[end]["part_content_hash"].as_str().unwrap().len(), 64);
    }
    let html = fs::read_to_string(out.join("nominal-seating-checks.html")).unwrap();
    assert!(html.contains("flange_plate_seat"));
    assert!(html.contains("href=\"recipe.star\""));
    assert!(!html.contains("<script"));
    let before = inventory(&out);
    for (old, replacement) in [
        ("offset = [0,0,-2]", "offset = [0,0,-2.01]"),
        ("offset = [0,0,-2]", "offset = [0,0,-1.99]"),
        (
            "min_shared_disk_radius_mm = 0.2",
            "min_shared_disk_radius_mm = 4",
        ),
        ("right_datum = \"inside\"", "right_datum = \"missing\""),
    ] {
        assert!(original.contains(old));
        fs::write(&source, original.replace(old, replacement)).unwrap();
        let failed = run(&source, &out);
        assert!(!failed.status.success(), "accepted {replacement}");
        assert_eq!(inventory(&out), before);
    }
    let offline = temp.path().join("offline");
    fs::create_dir(&offline).unwrap();
    let captured = offline.join("nominal_seating.star");
    fs::copy(out.join("recipe.star"), &captured).unwrap();
    fs::remove_file(&source).unwrap();
    let rebuilt = offline.join("rebuilt");
    let replay = run(&captured, &rebuilt);
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(
        fs::read(rebuilt.join("nominal-seating-checks.json")).unwrap(),
        bytes
    );
    let verify = Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("verify")
        .arg(&rebuilt)
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    fs::write(rebuilt.join("nominal-seating-checks.json"), b"{}").unwrap();
    assert!(!Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("verify")
        .arg(&rebuilt)
        .status()
        .unwrap()
        .success());
}
