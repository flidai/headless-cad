use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};

const SOURCE: &str = r#"
body = extrude(id = "body", profile = circle(id = "profile", radius = 2), height = 6)
other = translate(id = "other", body = body, offset = [10,0,0])
output(name = "body", feature = body)
output(name = "other", feature = other)
"#;
const GOOD: &str = r#"
require_exchange(id = "units", output = "body", format = "step", aspect = "units")
require_exchange(id = "geometry", output = "body", format = "step", aspect = "geometry")
"#;
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn success(result: Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn build(root: &Path, options: &[&str]) -> Output {
    let source = root.join("source.star");
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
    cli(&args)
}
fn write(root: &Path, source: &str) {
    fs::write(root.join("source.star"), source).unwrap();
}
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
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
fn failure(result: Output, id: &str, output: &str, format: &str, aspect: &str) -> Value {
    assert!(
        !result.status.success(),
        "unexpected publication: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["code"], "exchange_requirement_failed", "{error}");
    assert_eq!(error["requirement"]["id"], id, "{error}");
    assert_eq!(error["requirement"]["output"], output, "{error}");
    assert_eq!(error["requirement"]["format"], format, "{error}");
    assert_eq!(error["requirement"]["aspect"], aspect, "{error}");
    assert!(
        error["reason"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "{error}"
    );
    error
}

#[test]
fn unsupported_approximate_absent_and_unknown_exchange_obligations_preserve_generation() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), &format!("{SOURCE}{GOOD}"));
    success(build(temp.path(), &[]));
    let out = temp.path().join("published");
    let before = files(&out);
    for (format, aspect, status) in [
        ("stl", "units", "not_carried"),
        ("stl", "geometry", "approximate"),
        ("step", "manufacturing_pmi", "not_applicable"),
        ("step", "datums", "not_applicable"),
        ("step", "semantic_face_references", "not_carried"),
    ] {
        write(temp.path(),&format!("{SOURCE}\nrequire_exchange(id = \"policy\", output = \"body\", format = \"{format}\", aspect = \"{aspect}\")\n"));
        for _ in 0..2 {
            // Warm geometry never bypasses a publication requirement.
            let error = failure(build(temp.path(), &[]), "policy", "body", format, aspect);
            assert_eq!(error["observed_status"], status, "{error}");
            assert_eq!(files(&out), before);
        }
    }
}

#[test]
fn output_selection_and_display_only_cannot_skip_global_exchange_requirements() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(),&format!("{SOURCE}\nrequire_exchange(id = \"other_units\", output = \"other\", format = \"step\", aspect = \"units\")\n"));
    success(build(temp.path(), &[]));
    let out = temp.path().join("published");
    let before = files(&out);
    for options in [
        vec!["--output", "body"],
        vec!["--display-only"],
        vec!["--display-only", "--output", "body"],
    ] {
        failure(
            build(temp.path(), &options),
            "other_units",
            "other",
            "step",
            "units",
        );
        assert_eq!(files(&out), before);
    }
    success(build(temp.path(), &["--output", "other"]));
    assert!(!out.join("body.exchange.json").exists());
    assert!(out.join("other.exchange.json").exists());
}

#[test]
fn deleting_required_assembly_semantics_does_not_turn_absence_into_success() {
    let temp = tempfile::tempdir().unwrap();
    let declaration="assembly_output(name = \"machine\", assembly = assembly(id = \"machine\", instances = [instance(id = \"piece\", part = part(id = \"piece\", body = body))]))";
    let source=format!("{SOURCE}\n{declaration}\nrequire_exchange(id = \"definitions\", output = \"machine\", format = \"step\", aspect = \"assembly_definitions\")\n");
    write(temp.path(), &source);
    success(build(temp.path(), &[]));
    let out = temp.path().join("published");
    let before = files(&out);
    let changed = source.replace(declaration, "output(name = \"machine\", feature = body)");
    assert_ne!(source, changed);
    write(temp.path(), &changed);
    let error = failure(
        build(temp.path(), &[]),
        "definitions",
        "machine",
        "step",
        "assembly_definitions",
    );
    assert_eq!(error["observed_status"], "not_applicable", "{error}");
    assert_eq!(error["source_count"], 0);
    assert_eq!(files(&out), before);
}

#[test]
fn check_stays_pending_and_successful_publication_binds_global_requirements_to_reports() {
    let temp = tempfile::tempdir().unwrap();
    let source = format!("{SOURCE}{GOOD}");
    write(temp.path(), &source);
    let source_path = temp.path().join("source.star");
    for command in ["check", "inspect"] {
        let summary = success(cli(&[command, source_path.to_str().unwrap()]));
        assert_eq!(summary["exchange_policy_status"], "pending_publication");
        assert_eq!(
            summary["exchange_requirements"].as_object().unwrap().len(),
            2
        );
        for id in ["units", "geometry"] {
            let requirement = &summary["exchange_requirements"][id];
            assert_eq!(requirement["status"], "pending_publication");
            assert_eq!(requirement["requirement"]["id"], id);
            assert!(requirement.get("report_digest").is_none());
        }
        assert!(!temp.path().join("published").exists());
    }
    let summary = success(build(temp.path(), &[]));
    let out = temp.path().join("published");
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(summary["exchange_policy_status"], "passed");
    assert_eq!(manifest["exchange_policy_status"], "passed");
    assert_eq!(fs::read_to_string(out.join("recipe.star")).unwrap(), source);
    assert_eq!(
        summary["exchange_requirements"],
        manifest["exchange_requirements"]
    );
    let report_bytes = fs::read(out.join("body.exchange.json")).unwrap();
    let report: Value = serde_json::from_slice(&report_bytes).unwrap();
    assert_eq!(
        report["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    for id in ["units", "geometry"] {
        let result = &manifest["exchange_requirements"][id];
        assert_eq!(result["status"], "passed");
        assert_eq!(result["requirement"]["id"], id);
        assert_eq!(result["requirement"]["output"], "body");
        assert_eq!(result["requirement"]["format"], "step");
        assert_eq!(result["requirement"]["aspect"], id);
        assert_eq!(result["observed_status"], "supported");
        assert!(result["source_count"].as_u64().unwrap() > 0);
        assert_eq!(
            result["source_content_hash"],
            manifest["outputs"]["body"]["content_hash"]
        );
        assert_eq!(result["report_file"], "body.exchange.json");
        assert_eq!(
            result["report_digest"],
            blake3::hash(&report_bytes).to_hex().to_string()
        );
        assert_eq!(
            result["report_digest"],
            manifest["artifact_integrity"]["files"]["body.exchange.json"]["digest"]
        );
    }
    let repeated = success(build(temp.path(), &[]));
    assert_eq!(repeated["exchange_policy_status"], "passed");
    assert_eq!(repeated["stats"]["computed_solids"], 0);
    assert_eq!(
        repeated["exchange_requirements"],
        manifest["exchange_requirements"]
    );
    assert_eq!(
        success(cli(&["verify", out.to_str().unwrap()]))["valid"],
        true
    );
}
