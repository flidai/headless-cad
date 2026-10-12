use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
const SOURCE: &str = include_str!("../../../examples/countersink.star");
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn ok(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn build(root: &Path, flags: &[&str]) -> Output {
    let source = root.join("source.star");
    let dest = root.join("out");
    let cache = root.join("cache");
    let mut args = vec![
        "--cache-dir",
        cache.to_str().unwrap(),
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        dest.to_str().unwrap(),
    ];
    args.extend_from_slice(flags);
    cli(&args)
}
fn report(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("out/section-seat.json")).unwrap()).unwrap()
}
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(p).unwrap(),
            )
        })
        .collect()
}
fn near(value: &Value, expected: f64) {
    assert!(
        (value.as_f64().unwrap() - expected).abs() < 1e-8,
        "{value} != {expected}"
    )
}
#[test]
fn semantic_circle_dimensions_bind_actual_native_sections_and_replay_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    let build_result = ok(build(root, &[]));
    let first = report(root);
    let dims = &first["section_view"]["circle_dimensions"];
    assert_eq!(
        dims,
        &build_result["section_views"]["seat"]["circle_dimensions"]
    );
    let dim = &dims[0];
    near(&dim["value_mm"], 10.);
    near(&dim["radius"], 5.);
    assert_eq!(dim["selection"]["status"], "resolved");
    assert_eq!(dim["selection"]["selector"]["anchor_feature"], "sink_body");
    assert_eq!(
        dim["target_content_hash"],
        build_result["outputs"]["countersunk"]["content_hash"]
    );
    let svg = fs::read_to_string(root.join("out/section-seat.svg")).unwrap();
    assert!(svg.contains("seat_diameter: 10 mm diameter"));
    let metadata = svg
        .split("<metadata>")
        .nth(1)
        .unwrap()
        .split("</metadata>")
        .next()
        .unwrap()
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    assert_eq!(serde_json::from_str::<Value>(&metadata).unwrap(), first);
    let original = files(&root.join("out"));
    let replay = ok(build(root, &[]));
    assert_eq!(replay["stats"]["computed_solids"], 0);
    let repeated = files(&root.join("out"));
    assert_eq!(original["section-seat.json"], repeated["section-seat.json"]);
    assert_eq!(original["section-seat.svg"], repeated["section-seat.svg"]);
    ok(build(root, &["--param", "depth=2"]));
    let edited = report(root);
    near(
        &edited["section_view"]["circle_dimensions"][0]["value_mm"],
        8.,
    );
    assert_ne!(
        first["snapshot"]["query_context_hash"],
        edited["snapshot"]["query_context_hash"]
    );
    let moved = SOURCE.replace("label_offset = [0,30]", "label_offset = [15,30]");
    fs::write(root.join("source.star"), moved).unwrap();
    let changed = ok(build(root, &["--param", "depth=2"]));
    assert_eq!(changed["stats"]["computed_solids"], 0);
    assert_ne!(
        edited["snapshot"]["query_context_hash"],
        report(root)["snapshot"]["query_context_hash"]
    );
    assert!(
        ok(cli(&["verify", root.join("out").to_str().unwrap()]))["valid"]
            .as_bool()
            .unwrap()
    );
}
#[test]
fn circle_dimension_failures_preserve_all_artifacts_in_every_publication_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    ok(build(root, &[]));
    let before = files(&root.join("out"));
    for source in [
        SOURCE.replace("max = 6+depth+0.000001", "max = 6+depth-0.0000001"),
        SOURCE.replace("role = \"conical_faces\", kind", "role = \"missing\", kind"),
        SOURCE.replace("label_offset = [0,30]", "label_offset = [0,1]"),
    ] {
        fs::write(root.join("source.star"), source).unwrap();
        for flags in [
            vec![],
            vec!["--output", "stock"],
            vec!["--output", "stock", "--display-only"],
        ] {
            let out = build(root, &flags);
            assert!(!out.status.success());
            let error: Value = serde_json::from_slice(&out.stderr).unwrap();
            assert_eq!(error["code"], "section_dimension_failed");
            assert_eq!(files(&root.join("out")), before);
        }
    }
}
