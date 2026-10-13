use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

const MODULE: &str = "def make():\n    profile = rectangle(id=\"profile\", width=10, depth=4)\n    return extrude(id=\"body\", profile=profile, height=2)\n";
const ROOT: &str = "load(\"lib/shape.star\", \"make\")\nbody = make()\npart(id=\"sample\", body=body, part_number=\"SAMPLE\", material=\"Aluminum\", density_kg_per_m3=2700)\noutput(name=\"sample\", feature=body)\n";

fn build(source: &Path, out: &Path) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn captured_span(report: &Value, item: &Value, root: &str) -> String {
    let span = &item["declaration"];
    let source = if span["file"] == "sample.star" {
        root
    } else {
        MODULE
    };
    let start = span["start_byte"].as_u64().unwrap() as usize;
    let end = span["end_byte"].as_u64().unwrap() as usize;
    assert!(report["source_inventory"][span["file"].as_str().unwrap()].is_object());
    source[start..end].to_owned()
}

#[test]
fn locked_module_declarations_bind_exact_source_and_comment_edits_only_change_context() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(root.join("sample.star"), ROOT).unwrap();
    fs::write(root.join("lib/shape.star"), MODULE).unwrap();
    fs::write(root.join("sample.star.lock.json"), serde_json::to_vec(&json!({
        "schema_version":1,
        "modules":{"lib/shape.star":{"blake3":blake3::hash(MODULE.as_bytes()).to_hex().to_string()}}
    })).unwrap()).unwrap();
    let first_out = root.join("first");
    let first = build(&root.join("sample.star"), &first_out);
    let first_report: Value =
        serde_json::from_slice(&fs::read(first_out.join("source-provenance.json")).unwrap())
            .unwrap();
    assert_eq!(
        first["source_provenance"]["artifact"],
        "source-provenance.json"
    );
    assert_eq!(
        first_report["recipe_context_hash"],
        first["recipe_context_hash"]
    );
    assert_eq!(
        first_report["source_inventory"]["sample.star"]["bytes"],
        json!(ROOT.len())
    );
    assert_eq!(
        first_report["source_inventory"]["lib/shape.star"]["blake3"],
        blake3::hash(MODULE.as_bytes()).to_hex().to_string()
    );
    assert_eq!(
        first_report["source_inventory"]["lib/shape.star"]["sha256"],
        Sha256::digest(MODULE.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert!(
        captured_span(&first_report, &first_report["features"]["body"], ROOT)
            .starts_with("extrude(")
    );
    assert!(
        captured_span(&first_report, &first_report["parts"]["sample"], ROOT).starts_with("part(")
    );
    assert_eq!(first_report["part_body_features"]["sample"], "body");
    assert!(first_report["features"]["body"]["call_stack"]
        .as_array()
        .unwrap()
        .iter()
        .any(|frame| frame["file"] == "sample.star"));
    assert!(fs::read_to_string(first_out.join("sample.preview.html"))
        .unwrap()
        .contains("source_provenance"));

    let replay_out = root.join("replay");
    let replay = build(&first_out.join("recipe.bundle.json"), &replay_out);
    assert_eq!(first["source_provenance"], replay["source_provenance"]);
    assert_eq!(
        fs::read(first_out.join("source-provenance.json")).unwrap(),
        fs::read(replay_out.join("source-provenance.json")).unwrap()
    );

    let prefix = "# α annotation only\n";
    let commented = format!("{prefix}{ROOT}");
    fs::write(root.join("sample.star"), &commented).unwrap();
    let second_out = root.join("second");
    let second = build(&root.join("sample.star"), &second_out);
    let second_report: Value =
        serde_json::from_slice(&fs::read(second_out.join("source-provenance.json")).unwrap())
            .unwrap();
    assert_eq!(
        first["outputs"]["sample"]["content_hash"],
        second["outputs"]["sample"]["content_hash"]
    );
    assert_ne!(first["recipe_context_hash"], second["recipe_context_hash"]);
    assert_ne!(
        first["source_provenance"]["digest"],
        second["source_provenance"]["digest"]
    );
    assert!(captured_span(
        &second_report,
        &second_report["parts"]["sample"],
        &commented
    )
    .starts_with("part("));

    let diff = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "diff",
            first_out.join("recipe.bundle.json").to_str().unwrap(),
            root.join("sample.star").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let diff: Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_ne!(
        diff["source_provenance"]["before"]["recipe_source_hash"],
        diff["source_provenance"]["after"]["recipe_source_hash"]
    );
    assert_eq!(
        diff["source_provenance"]["before"]["parts"]["sample"]["declaration"]["start_byte"]
            .as_u64()
            .unwrap()
            + prefix.len() as u64,
        diff["source_provenance"]["after"]["parts"]["sample"]["declaration"]["start_byte"]
            .as_u64()
            .unwrap()
    );
}

#[test]
fn json_recipe_has_no_starlark_navigation_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plate.json");
    let out = dir.path().join("json");
    let manifest = build(&source, &out);
    assert!(manifest.get("source_provenance").is_none());
    assert!(!out.join("source-provenance.json").exists());
}
