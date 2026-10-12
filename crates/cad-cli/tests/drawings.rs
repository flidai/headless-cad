use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn snapshot(path: &Path) -> BTreeMap<String, Vec<u8>> {
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
fn drawing_sheets_bind_sections_replay_and_preserve_failed_generations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("bearing_drawing.star");
    let original = include_str!("../../../examples/bearing_drawing.star");
    fs::write(&source, original).unwrap();
    fs::write(
        temp.path().join("datum_section_fixture.star"),
        include_str!("../../../examples/datum_section_fixture.star"),
    )
    .unwrap();
    fs::write(
        temp.path().join("bearing_drawing.star.lock.json"),
        include_str!("../../../examples/bearing_drawing.star.lock.json"),
    )
    .unwrap();
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
    let sheet: Value =
        serde_json::from_slice(&fs::read(out.join("drawing-bearing_sections.json")).unwrap())
            .unwrap();
    assert_eq!(
        sheet["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(sheet["status"], "published");
    assert_eq!(
        manifest["drawing_sheets"]["bearing_sections"]["status"],
        "published"
    );
    let renderer = sheet["renderer_build_id"].as_str().unwrap();
    assert_eq!(renderer.len(), 64);
    assert!(renderer.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let svg_bytes = fs::read(out.join("drawing-bearing_sections.svg")).unwrap();
    assert_eq!(
        sheet["svg"]["digest"],
        blake3::hash(&svg_bytes).to_hex().to_string()
    );
    assert_eq!(sheet["views"].as_array().unwrap().len(), 2);
    for view in sheet["views"].as_array().unwrap() {
        let bytes = fs::read(out.join(view["section_report"].as_str().unwrap())).unwrap();
        assert_eq!(view["digest"], blake3::hash(&bytes).to_hex().to_string());
    }
    let svg = fs::read_to_string(out.join("drawing-bearing_sections.svg")).unwrap();
    assert!(svg.contains("width=\"420mm\" height=\"297mm\""));
    assert_eq!(svg.matches("data-scale=\"0.8\"").count(), 2);
    let second = run(&source, &out, &[]);
    assert!(second.status.success());
    assert_eq!(
        svg,
        fs::read_to_string(out.join("drawing-bearing_sections.svg")).unwrap()
    );
    let before = snapshot(&out);
    let bad_scale = run(
        &source,
        &out,
        &["--param", "sheet_scale=10", "--output", "housing"],
    );
    assert!(!bad_scale.status.success());
    assert!(String::from_utf8_lossy(&bad_scale.stderr).contains("viewport"));
    assert_eq!(before, snapshot(&out));
    fs::write(&source, original.replace("\"x\":215", "\"x\":20")).unwrap();
    let overlap = run(&source, &out, &[]);
    assert!(!overlap.status.success());
    assert_eq!(before, snapshot(&out));
    fs::write(
        &source,
        original.replace("revision = \"A\"", "revision = \"B\""),
    )
    .unwrap();
    let revision = run(&source, &out, &[]);
    assert!(
        revision.status.success(),
        "{}",
        String::from_utf8_lossy(&revision.stderr)
    );
    let updated: Value = serde_json::from_slice(&revision.stdout).unwrap();
    assert_eq!(updated["stats"]["computed_solids"], 0);
    assert_ne!(
        updated["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert!(fs::read_to_string(out.join("drawing-bearing_sections.svg"))
        .unwrap()
        .contains("Revision: B"));
    let replay = temp.path().join("replay");
    fs::remove_file(&source).unwrap();
    fs::remove_file(temp.path().join("datum_section_fixture.star")).unwrap();
    let replayed = run(&out.join("recipe.bundle.json"), &replay, &[]);
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    assert_eq!(
        fs::read(out.join("drawing-bearing_sections.svg")).unwrap(),
        fs::read(replay.join("drawing-bearing_sections.svg")).unwrap()
    );
    let verified = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(verified.status.success());
}
