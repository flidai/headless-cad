use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

const SOURCE: &str = r#"
height = param(name = "height", default = 6)
body = extrude(id = "body", profile = circle(id = "profile", radius = 2), height = height)
output(name = "body", feature = body)
piece = part(id = "piece", body = body, part_number = "EVIDENCE-42", material = "Test steel", density_kg_per_m3 = 7800, datums = {"base":datum(), "end":datum(offset = [0,0,height])})
module = assembly(id = "module", instances = [instance(id = "piece", part = piece)])
assembly_output(name = "machine", assembly = assembly(id = "machine", instances = [
    assembly_instance(id = "left", assembly = module, offset = [10,0,0]),
    assembly_instance(id = "right", assembly = module, offset = [0,12,0], rotation = [0,0,0.7071067811865475,0.7071067811865476]),
]))
"#;

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn success(result: std::process::Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn build(root: &Path, options: &[&str]) -> Value {
    let source = root.join("source.star");
    if !source.exists() {
        fs::write(&source, SOURCE).unwrap();
    }
    let out = root.join("published");
    let cache = root.join("cache");
    let mut args = vec![
        "--cache-dir",
        cache.to_str().unwrap(),
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(options);
    success(cli(&args))
}
fn reports(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".exchange.json")
        })
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}
fn aspect<'a>(report: &'a Value, name: &str) -> &'a Value {
    let entries: Vec<_> = report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["aspect"] == name)
        .collect();
    assert_eq!(entries.len(), 1, "expected one {name} entry in {report}");
    entries[0]
}

#[test]
fn exchange_reports_bind_each_export_to_source_and_complete_integrity_inventory() {
    let temp = tempfile::tempdir().unwrap();
    build(temp.path(), &[]);
    let out = temp.path().join("published");
    let manifest = read(out.join("manifest.json"));
    assert_eq!(reports(&out).len(), 2);
    for name in ["body", "machine"] {
        let filename = format!("{name}.exchange.json");
        let document = read(out.join(&filename));
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["output_name"], name);
        assert_eq!(
            document["source_content_hash"],
            manifest["outputs"][name]["content_hash"]
        );
        assert_eq!(
            document["recipe_source_hash"],
            manifest["recipe_source_hash"]
        );
        assert_eq!(
            document["recipe_context_hash"],
            manifest["recipe_context_hash"]
        );
        assert!(manifest["artifact_files"]
            .as_array()
            .unwrap()
            .contains(&Value::String(filename.clone())));
        assert!(manifest["outputs"][name]["available_files"]
            .as_array()
            .unwrap()
            .contains(&Value::String(filename.clone())));
        let bytes = fs::read(out.join(&filename)).unwrap();
        let integrity = &manifest["artifact_integrity"]["files"][&filename];
        assert_eq!(integrity["bytes"], bytes.len());
        assert_eq!(
            integrity["digest"],
            blake3::hash(&bytes).to_hex().to_string()
        );
        for format in ["step", "stl"] {
            let artifact = &document["artifacts"][format];
            assert_eq!(artifact["file"], format!("{name}.{format}"));
            assert_eq!(artifact["algorithm"], "blake3");
            let bytes = fs::read(out.join(artifact["file"].as_str().unwrap())).unwrap();
            assert_eq!(artifact["bytes"], bytes.len());
            assert_eq!(
                artifact["digest"],
                blake3::hash(&bytes).to_hex().to_string()
            );
            assert_eq!(artifact["report"]["format"], format);
            assert_eq!(
                artifact["report"]["source_kind"],
                if name == "body" { "body" } else { "assembly" }
            );
        }
    }
    assert_eq!(
        success(cli(&["verify", out.to_str().unwrap()]))["valid"],
        true
    );
}

#[test]
fn reports_are_deterministic_on_cached_replay_and_follow_selected_display_only_publications() {
    let temp = tempfile::tempdir().unwrap();
    build(temp.path(), &[]);
    let out = temp.path().join("published");
    let original = reports(&out);
    build(temp.path(), &[]);
    assert_eq!(reports(&out), original);
    build(temp.path(), &["--param", "height=8"]);
    let edited = reports(&out);
    for name in original.keys() {
        let old: Value = serde_json::from_slice(&original[name]).unwrap();
        let new: Value = serde_json::from_slice(&edited[name]).unwrap();
        assert_ne!(old["source_content_hash"], new["source_content_hash"]);
        assert_eq!(old["recipe_source_hash"], new["recipe_source_hash"]);
        assert_ne!(old["recipe_context_hash"], new["recipe_context_hash"]);
        for format in ["step", "stl"] {
            assert_ne!(
                old["artifacts"][format]["digest"],
                new["artifacts"][format]["digest"]
            );
        }
    }
    build(temp.path(), &["--output", "machine"]);
    assert_eq!(
        reports(&out).into_keys().collect::<Vec<_>>(),
        ["machine.exchange.json"]
    );
    build(temp.path(), &["--display-only"]);
    assert!(reports(&out).is_empty());
    let manifest = read(out.join("manifest.json"));
    for name in manifest["artifact_files"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        assert!(
            !name.ends_with(".exchange.json")
                && !name.ends_with(".step")
                && !name.ends_with(".stl")
        );
    }
    for output in manifest["outputs"].as_object().unwrap().values() {
        assert!(!output["available_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name.as_str().unwrap().ends_with(".exchange.json")));
    }
    assert_eq!(
        success(cli(&["verify", out.to_str().unwrap()]))["valid"],
        true
    );
}

#[test]
fn changed_exchange_report_is_rejected_by_generation_integrity_verification() {
    let temp = tempfile::tempdir().unwrap();
    build(temp.path(), &[]);
    let out = temp.path().join("published");
    let path = out.join("machine.exchange.json");
    let mut report = read(&path);
    report["source_content_hash"] = Value::String("wrong source".into());
    fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let result = cli(&["verify", out.to_str().unwrap()]);
    assert!(!result.status.success());
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["code"], "artifact_integrity_failed");
}

#[test]
fn reports_distinguish_exchange_mapping_from_native_metadata_and_manufacturing_intent() {
    let temp = tempfile::tempdir().unwrap();
    build(temp.path(), &[]);
    let out = temp.path().join("published");
    let machine = read(out.join("machine.exchange.json"));
    let native = read(out.join("machine.assembly.json"));
    assert_eq!(native["definitions"].as_object().unwrap().len(), 1);
    let piece = &native["definitions"]["piece"];
    assert_eq!(piece["metadata"]["part_number"], "EVIDENCE-42");
    assert_eq!(piece["metadata"]["material"]["name"], "Test steel");
    assert_eq!(piece["datums"].as_object().unwrap().len(), 2);
    assert_eq!(native["instances"].as_array().unwrap().len(), 2);
    assert_eq!(native["hierarchy"].as_object().unwrap().len(), 2);
    for format in ["step", "stl"] {
        let report = &machine["artifacts"][format]["report"];
        assert_eq!(report["source_units"], "mm");
        assert_eq!(
            aspect(report, "geometry")["status"],
            if format == "step" {
                "supported"
            } else {
                "approximate"
            }
        );
        assert_eq!(
            aspect(report, "units")["status"],
            if format == "step" {
                "supported"
            } else {
                "not_carried"
            }
        );
        let units = aspect(report, "units")["detail"].as_str().unwrap();
        if format == "step" {
            assert!(units.contains("Explicit") && units.contains("millimetre"));
        } else {
            assert!(units.contains("no unit declaration") && units.contains("consumers"));
        }
        for (name, count) in [
            ("assembly_definitions", 1),
            ("assembly_hierarchy", 2),
            ("occurrence_paths", 2),
            ("occurrence_placements", 2),
        ] {
            let entry = aspect(report, name);
            assert_eq!(
                entry["status"],
                if format == "step" {
                    "supported"
                } else {
                    "not_carried"
                }
            );
            assert_eq!(entry["source_count"], count);
        }
        for (name, count) in [
            ("part_numbers", 1),
            ("material_names", 1),
            ("material_density", 1),
            ("datums", 2),
        ] {
            let entry = aspect(report, name);
            assert_eq!(entry["status"], "not_carried");
            assert_eq!(entry["source_count"], count);
        }
        assert_eq!(
            aspect(report, "manufacturing_pmi")["status"],
            "not_applicable"
        );
        assert_eq!(aspect(report, "manufacturing_pmi")["source_count"], 0);
        assert_eq!(
            aspect(report, "modeling_tolerance")["status"],
            if format == "step" {
                "supported"
            } else {
                "not_carried"
            }
        );
        for name in [
            "semantic_face_references",
            "recipe_history",
            "source_revision",
        ] {
            assert_eq!(aspect(report, name)["status"], "not_carried");
            assert!(aspect(report, name)["source_count"].is_null());
        }
    }
    let step = fs::read_to_string(out.join("machine.step")).unwrap();
    assert!(step.contains("SI_UNIT(.MILLI.,.METRE.)"));
    // One shared part definition, one shared subassembly, and the root.
    assert_eq!(step.matches("PRODUCT(").count(), 3);
    assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 3);
    assert!(!step.contains("EVIDENCE-42") && !step.contains("Test steel"));
    let stl = fs::read_to_string(out.join("machine.stl")).unwrap();
    assert_eq!(
        stl.lines()
            .filter(|line| line.starts_with("solid "))
            .count(),
        2
    );
    assert!(!stl.contains("EVIDENCE-42") && !stl.contains("Test steel"));
    let body = read(out.join("body.exchange.json"));
    for format in ["step", "stl"] {
        let report = &body["artifacts"][format]["report"];
        for name in [
            "assembly_definitions",
            "assembly_hierarchy",
            "occurrence_paths",
            "occurrence_placements",
            "datums",
            "material_names",
            "part_numbers",
        ] {
            assert_eq!(aspect(report, name)["status"], "not_applicable");
            assert_eq!(aspect(report, name)["source_count"], 0);
        }
        assert_eq!(
            aspect(report, "operation_face_history")["status"],
            "not_carried"
        );
        assert!(aspect(report, "operation_face_history")["source_count"].is_null());
    }
}
