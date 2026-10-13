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
fn machining_tables_bind_native_faces_replay_and_reject_bad_references_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("drive_module_rotation.star");
    let original = include_str!("../../../examples/drive_module_rotation.star");
    fs::write(&source, original).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |input: &Path, directory: &Path, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                input.to_str().unwrap(),
                "--out-dir",
                directory.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    let first = run(&source, &out, &[]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let manifest: Value = serde_json::from_slice(&first.stdout).unwrap();
    let datum_bytes = fs::read(out.join("datum-dimensions.json")).unwrap();
    let native: Value = serde_json::from_slice(&datum_bytes).unwrap();
    let drawing_bytes = fs::read(out.join("drawing-end_cover_machining.json")).unwrap();
    let drawing: Value = serde_json::from_slice(&drawing_bytes).unwrap();
    let svg_bytes = fs::read(out.join("drawing-end_cover_machining.svg")).unwrap();
    let svg = String::from_utf8(svg_bytes.clone()).unwrap();
    assert_eq!(drawing["schema_version"], 2);
    assert_eq!(
        drawing["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(
        drawing["datum_dimension_report"]["digest"],
        blake3::hash(&datum_bytes).to_hex().to_string()
    );
    assert_eq!(
        drawing["svg"]["digest"],
        blake3::hash(&svg_bytes).to_hex().to_string()
    );
    let tables = drawing["dimension_tables"].as_array().unwrap();
    assert_eq!(tables.len(), 2);
    let mut count = 0;
    let mut bands = 0;
    let mut unspecified = 0;
    for table in tables {
        assert_eq!(table["output"], "drive_module_release");
        assert_eq!(table["show_size_limits"], true);
        let rows = table["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 7);
        for row in rows {
            count += 1;
            let id = row["dimension"].as_str().unwrap();
            let report = &row["native_dimension"];
            assert_eq!(report, &native["dimensions"][id]);
            assert_eq!(report["declaration"]["output"], table["output"]);
            assert_eq!(report["status"], "passed");
            assert!(report["from"]["semantic_datum"].is_object());
            assert!(report["to"]["semantic_datum"].is_object());
            assert!(report["from"]["part_content_hash"].as_str().unwrap().len() >= 32);
            let value = report["value_mm"].as_f64().unwrap();
            let lower = report["lower_bound_mm"].as_f64().unwrap();
            let upper = report["upper_bound_mm"].as_f64().unwrap();
            assert!(lower <= value && value <= upper);
            let row_id = format!(
                "id=\"drawing-table-{}-{id}\"",
                table["id"].as_str().unwrap()
            );
            let rendered = svg.lines().find(|line| line.contains(&row_id)).unwrap();
            assert!(rendered.contains(&format!("≈ {value:.3} mm")));
            if let Some(tolerance) = report["tolerance"].as_object() {
                bands += 1;
                let design_lower = tolerance["lower_limit_mm"].as_f64().unwrap();
                let design_upper = tolerance["upper_limit_mm"].as_f64().unwrap();
                assert!(design_lower < design_upper);
                assert_eq!(
                    row["authored_size_limits_mm"],
                    serde_json::json!([design_lower, design_upper])
                );
                assert!(rendered.contains(&format!("{design_lower}..{design_upper} mm")));
                assert!(!rendered.contains("not specified"));
            } else {
                unspecified += 1;
                assert!(row["authored_size_limits_mm"].is_null());
                assert!(rendered.contains(">not specified</text>"));
            }
        }
    }
    assert_eq!(count, 14);
    assert_eq!(bands, 10);
    assert_eq!(unspecified, 4);
    assert_eq!(svg.matches("DESIGN LIMITS").count(), 2);
    assert_eq!(svg.matches("Nominal native geometry").count(), 0);
    assert!(svg.contains("Revision: A"));
    assert!(svg.contains("Source context:"));

    let stable = inventory(&out);
    let mut invalid = original.to_owned();
    let position = invalid.rfind("_cover_minus_depth").unwrap();
    invalid.replace_range(
        position..position + "_cover_minus_depth".len(),
        "_missing_dimension",
    );
    fs::write(&source, invalid).unwrap();
    let failed = run(&source, &out, &["--output", "drive_module_release"]);
    assert!(!failed.status.success());
    assert_eq!(stable, inventory(&out));

    fs::write(&source, original).unwrap();
    let replay = temp.path().join("replay");
    fs::remove_file(&source).unwrap();
    let captured_artifact = manifest["recipe_source"]["artifact"]
        .as_str()
        .expect("captured source artifact");
    let offline_source = temp
        .path()
        .join("offline")
        .join("drive_module_rotation.star");
    fs::create_dir(offline_source.parent().unwrap()).unwrap();
    fs::copy(out.join(captured_artifact), &offline_source).unwrap();
    let replayed = run(&offline_source, &replay, &[]);
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    assert_eq!(
        drawing_bytes,
        fs::read(replay.join("drawing-end_cover_machining.json")).unwrap()
    );
    assert_eq!(
        svg_bytes,
        fs::read(replay.join("drawing-end_cover_machining.svg")).unwrap()
    );
    assert!(Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .status()
        .unwrap()
        .success());
    let tampered = replay.join("drawing-end_cover_machining.svg");
    fs::write(&tampered, b"<svg/>").unwrap();
    assert!(!Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .status()
        .unwrap()
        .success());
}
