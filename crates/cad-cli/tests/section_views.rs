use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
const SOURCE: &str = r#"
radius = param(name = "radius", default = 2)
z = param(name = "z", default = 3)
body = extrude(id = "body", profile = circle(id = "profile", radius = radius), height = 6)
other = translate(id = "other", body = body, offset = [10,0,0])
output(name = "body", feature = body)
output(name = "other", feature = other)
area = 3.141592653589793*radius*radius
section_view(id = "mid", output = "body", origin = [0,0,z], min_area = area-0.000001, max_area = area+0.000001)
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
fn build(root: &Path, flags: &[&str]) -> Output {
    let source = root.join("source.star");
    let out = root.join("out");
    let cache = root.join("cache");
    let mut args = vec![
        "--cache-dir",
        cache.to_str().unwrap(),
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(flags);
    cli(&args)
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
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn assert_view(root: &Path, summary: &Value, expected: f64) {
    let out = root.join("out");
    let report = read(&out.join("section-mid.json"));
    assert!((report["section"]["area"].as_f64().unwrap() - expected).abs() < 1e-9);
    assert_eq!(report["section_view"]["status"], "passed");
    assert_eq!(
        report["snapshot"]["recipe_source_hash"],
        summary["recipe_source_hash"]
    );
    assert_eq!(
        report["publication"]["recipe_context_hash"],
        summary["recipe_context_hash"]
    );
    assert_eq!(
        report["snapshot"],
        summary["section_views"]["mid"]["snapshot"]
    );
    for format in ["json", "svg"] {
        let record = &summary["section_views"]["mid"]["artifacts"][format];
        let filename = record["file"].as_str().unwrap();
        let bytes = fs::read(out.join(filename)).unwrap();
        assert_eq!(record["algorithm"], "blake3");
        assert_eq!(record["bytes"], bytes.len());
        assert_eq!(record["digest"], blake3::hash(&bytes).to_hex().to_string());
        assert!(summary["artifact_files"]
            .as_array()
            .unwrap()
            .contains(&Value::String(filename.into())));
    }
    let svg = fs::read_to_string(out.join("section-mid.svg")).unwrap();
    let metadata = svg
        .split("<metadata>")
        .nth(1)
        .unwrap()
        .split("</metadata>")
        .next()
        .unwrap();
    let metadata = metadata
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    assert_eq!(serde_json::from_str::<Value>(&metadata).unwrap(), report);
    assert!(svg.contains("fill-rule=\"evenodd\""));
    assert_eq!(
        fs::read_to_string(out.join("recipe.star")).unwrap(),
        fs::read_to_string(root.join("source.star")).unwrap()
    );
    assert_eq!(
        success(cli(&["verify", out.to_str().unwrap()]))["valid"],
        true
    );
}
#[test]
fn source_views_publish_bound_native_artifacts_for_full_selected_and_display_generations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    let original = success(build(root, &[]));
    assert_view(root, &original, 4. * std::f64::consts::PI);
    let edited = success(build(root, &["--param", "radius=3"]));
    assert_view(root, &edited, 9. * std::f64::consts::PI);
    assert_ne!(
        original["section_views"]["mid"]["snapshot"]["content_hash"],
        edited["section_views"]["mid"]["snapshot"]["content_hash"]
    );
    let cached = success(build(root, &["--param", "radius=3"]));
    assert_eq!(cached["stats"]["computed_solids"], 0);
    assert_view(root, &cached, 9. * std::f64::consts::PI);
    let selected = success(build(root, &["--output", "other"]));
    assert!(selected["outputs"].get("body").is_none());
    assert_view(root, &selected, 4. * std::f64::consts::PI);
    let display = success(build(root, &["--display-only", "--output", "other"]));
    assert_view(root, &display, 4. * std::f64::consts::PI);
    assert!(!root.join("out/other.step").exists());
    let removed = SOURCE
        .lines()
        .filter(|line| !line.starts_with("section_view("))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join("source.star"), removed).unwrap();
    let summary = success(build(root, &[]));
    assert!(summary["section_views"].as_object().unwrap().is_empty());
    assert!(
        !root.join("out/section-mid.json").exists() && !root.join("out/section-mid.svg").exists()
    );
}
#[test]
fn global_section_failures_preserve_every_byte_of_previous_publication() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    success(build(root, &[]));
    let original = files(&root.join("out"));
    for flags in [
        vec![],
        vec!["--output", "other"],
        vec!["--display-only", "--output", "other"],
    ] {
        for changed in [
            SOURCE.replace(
                "min_area = area-0.000001, max_area = area+0.000001",
                "min_area = 0, max_area = area-1",
            ),
            SOURCE.replace("origin = [0,0,z]", "origin = [0,0,0]"),
        ] {
            fs::write(root.join("source.star"), changed).unwrap();
            let result = build(root, &flags);
            assert!(!result.status.success());
            let error: Value = serde_json::from_slice(&result.stderr).unwrap();
            assert_eq!(error["code"], "section_view_failed", "{error}");
            assert_eq!(error["section_view_id"], "mid");
            assert_eq!(error["output_name"], "body");
            assert_eq!(files(&root.join("out")), original);
        }
    }
    for changed in [
        SOURCE.replace(
            "section_view(id = \"mid\", output = \"body\"",
            "section_view(id = \"mid\", output = \"missing\"",
        ),
        SOURCE.replace("id = \"mid\"", "id = \"../escaped\""),
    ] {
        fs::write(root.join("source.star"), changed).unwrap();
        assert!(!build(root, &[]).status.success());
        assert_eq!(files(&root.join("out")), original);
    }
    assert!(!root.join("escaped.json").exists());
}
#[test]
fn view_only_edits_diff_without_geometry_changes_and_roundtrip_through_ir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let before = root.join("before.star");
    let after = root.join("after.star");
    fs::write(&before, SOURCE).unwrap();
    fs::write(
        &after,
        SOURCE.replace("origin = [0,0,z]", "origin = [0,0,4]"),
    )
    .unwrap();
    let diff = success(cli(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]));
    assert_eq!(diff["section_views"]["mid"]["status"], "changed");
    assert_eq!(diff["outputs"]["body"]["status"], "unchanged");
    fs::write(&after, SOURCE.replace("id = \"mid\"", "id = \"renamed\"")).unwrap();
    let renamed = success(cli(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]));
    assert_eq!(renamed["section_views"]["mid"]["status"], "removed");
    assert_eq!(renamed["section_views"]["renamed"]["status"], "added");
    let ir = success(cli(&["compile", before.to_str().unwrap()]));
    assert_eq!(ir["section_views"][0]["id"], "mid");
    let path = root.join("recipe.json");
    fs::write(&path, serde_json::to_vec_pretty(&ir).unwrap()).unwrap();
    let checked = success(cli(&["check", path.to_str().unwrap()]));
    assert_eq!(checked["section_views"]["mid"]["status"], "passed");
    assert!(
        (checked["section_views"]["mid"]["query"]["section"]["area"]
            .as_f64()
            .unwrap()
            - 4. * std::f64::consts::PI)
            .abs()
            < 1e-9
    );
}
