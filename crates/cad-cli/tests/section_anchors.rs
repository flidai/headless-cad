use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
const SOURCE: &str = r#"
shift = param(name = "shift", default = 0)
datum_z = param(name = "datum_z", default = 4)
body = extrude(id = "body", profile = rectangle(id = "profile", width = 8, depth = 6), height = 10)
p = part(id = "part", body = body, datums = {"slice":datum(offset = [0,0,datum_z]), "alias":datum(offset = [0,0,datum_z])})
module = assembly(id = "module", instances = [instance(id = "same", part = p, offset = [2,0,0])])
assembly_output(name = "scene", assembly = assembly(id = "scene", instances = [assembly_instance(id = "left", assembly = module, offset = [shift,0,0]), assembly_instance(id = "right", assembly = module, offset = [20,0,0]), instance(id = "leaf", part = p, offset = [0,20,0])]))
output(name = "body", feature = body)
section_view(id = "nested_left", output = "scene", origin = [0,0,1], instance_path = ["left","same"], datum = "slice", min_area = 143.999999, max_area = 144.000001)
section_view(id = "nested_right", output = "scene", origin = [0,0,1], instance_path = ["right","same"], datum = "slice")
section_view(id = "leaf_view", output = "scene", origin = [0,0,1], instance_path = ["leaf"], datum = "slice")
"#;
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn success(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn build(root: &Path, flags: &[&str]) -> Output {
    let source = root.join("source.star");
    let destination = root.join("out");
    let cache = root.join("cache");
    let mut args = vec![
        "--cache-dir",
        cache.to_str().unwrap(),
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        destination.to_str().unwrap(),
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
fn report(root: &Path, id: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(format!("out/section-{id}.json"))).unwrap()).unwrap()
}
fn near(actual: &Value, expected: f64) {
    assert!(
        (actual.as_f64().unwrap() - expected).abs() < 1e-9,
        "{actual} != {expected}"
    );
}
fn witness(root: &Path, summary: &Value, id: &str, path: &[&str], origin: [f64; 3]) -> Value {
    let report = report(root, id);
    let witness = report["section_view"]["anchor_witness"].clone();
    assert_eq!(witness["instance_path"], serde_json::json!(path));
    assert_eq!(witness["part_id"], "part");
    assert_eq!(witness, summary["section_views"][id]["anchor_witness"]);
    assert_eq!(witness["resolved_plane"], report["section"]["plane"]);
    for (actual, expected) in report["section"]["plane"]["origin"]
        .as_array()
        .unwrap()
        .iter()
        .zip(origin)
    {
        near(actual, expected);
    }
    near(&report["section"]["area"], 144.);
    assert_eq!(report["snapshot"], summary["section_views"][id]["snapshot"]);
    let svg = fs::read_to_string(root.join(format!("out/section-{id}.svg"))).unwrap();
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
    assert_eq!(serde_json::from_str::<Value>(&metadata).unwrap(), report);
    witness
}
#[test]
fn nested_and_leaf_anchors_bind_exact_occurrence_frames_and_follow_cached_edits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    let before = success(build(root, &[]));
    let left = witness(
        root,
        &before,
        "nested_left",
        &["left", "same"],
        [2., 0., 5.],
    );
    witness(
        root,
        &before,
        "nested_right",
        &["right", "same"],
        [22., 0., 5.],
    );
    witness(root, &before, "leaf_view", &["leaf"], [0., 20., 5.]);
    let before_report = report(root, "nested_left");
    near(&before_report["section"]["centroid"][0], 10.);
    near(&before_report["section"]["centroid"][1], 29. / 3.);
    let moved = success(build(root, &["--param", "shift=3"]));
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_eq!(
        before["outputs"]["body"]["content_hash"],
        moved["outputs"]["body"]["content_hash"]
    );
    near(&report(root, "nested_left")["section"]["centroid"][0], 8.);
    let moved_witness = witness(root, &moved, "nested_left", &["left", "same"], [5., 0., 5.]);
    assert_ne!(left["occurrence_frame"], moved_witness["occurrence_frame"]);
    assert_eq!(left["datum_frame"], moved_witness["datum_frame"]);
    assert_ne!(
        before_report["snapshot"]["query_context_hash"],
        report(root, "nested_left")["snapshot"]["query_context_hash"]
    );
    let changed = success(build(root, &["--param", "datum_z=5"]));
    assert_eq!(changed["stats"]["computed_solids"], 0);
    assert_eq!(
        before["outputs"]["body"]["content_hash"],
        changed["outputs"]["body"]["content_hash"]
    );
    let datum_witness = witness(
        root,
        &changed,
        "nested_left",
        &["left", "same"],
        [2., 0., 6.],
    );
    assert_ne!(left["datum_frame"], datum_witness["datum_frame"]);
    assert_eq!(left["occurrence_frame"], datum_witness["occurrence_frame"]);
    assert_ne!(
        before_report["snapshot"]["query_context_hash"],
        report(root, "nested_left")["snapshot"]["query_context_hash"]
    );
    let selected = success(build(root, &["--output", "body", "--display-only"]));
    assert!(selected["outputs"].get("scene").is_none());
    witness(
        root,
        &selected,
        "nested_left",
        &["left", "same"],
        [2., 0., 5.],
    );
}
#[test]
fn invalid_exact_paths_datums_and_event_planes_preserve_publication_globally() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    success(build(root, &[]));
    let original = files(&root.join("out"));
    for changed in [
        SOURCE.replace("[\"left\",\"same\"]", "[\"Left\",\"same\"]"),
        SOURCE.replace("[\"left\",\"same\"]", "[\"left\"]"),
        SOURCE.replace("datum = \"slice\"", "datum = \"missing\""),
        SOURCE.replace("origin = [0,0,1]", "origin = [0,0,-datum_z]"),
    ] {
        fs::write(root.join("source.star"), changed).unwrap();
        for flags in [
            vec![],
            vec!["--output", "body"],
            vec!["--output", "body", "--display-only"],
        ] {
            let result = build(root, &flags);
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            let error: Value = serde_json::from_slice(&result.stderr).unwrap();
            assert_eq!(error["code"], "section_view_failed", "{error}");
            assert_eq!(files(&root.join("out")), original);
        }
    }
}
#[test]
fn anchor_intent_roundtrips_and_diffs_even_when_resolved_plane_is_identical() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = root.join("source.star");
    let alias = root.join("alias.star");
    fs::write(&source, SOURCE).unwrap();
    fs::write(
        &alias,
        SOURCE.replace("datum = \"slice\"", "datum = \"alias\""),
    )
    .unwrap();
    let original = success(build(root, &[]));
    let original_report = report(root, "nested_left");
    let diff = success(cli(&[
        "diff",
        source.to_str().unwrap(),
        alias.to_str().unwrap(),
    ]));
    assert_eq!(diff["outputs"]["scene"]["status"], "unchanged");
    assert_eq!(diff["section_views"]["nested_left"]["status"], "changed");
    fs::write(&source, fs::read(&alias).unwrap()).unwrap();
    let changed = success(build(root, &[]));
    let changed_report = report(root, "nested_left");
    assert_eq!(
        original["outputs"]["scene"]["content_hash"],
        changed["outputs"]["scene"]["content_hash"]
    );
    assert_eq!(
        original_report["section"]["plane"],
        changed_report["section"]["plane"]
    );
    assert_ne!(
        original_report["snapshot"]["query_context_hash"],
        changed_report["snapshot"]["query_context_hash"]
    );
    let ir = success(cli(&["compile", source.to_str().unwrap()]));
    let json = root.join("recipe.json");
    fs::write(&json, serde_json::to_vec(&ir).unwrap()).unwrap();
    let checked = success(cli(&["check", json.to_str().unwrap()]));
    assert_eq!(
        checked["section_views"]["nested_left"]["anchor_witness"]["instance_path"],
        serde_json::json!(["left", "same"])
    );
    let standalone = success(cli(&[
        "section",
        source.to_str().unwrap(),
        "--output",
        "scene",
        "--origin",
        "0,0,5",
    ]));
    assert!(standalone.get("section_view").is_none() && standalone.get("anchor_witness").is_none());
    assert_eq!(
        standalone["section"]["plane"]["origin"],
        serde_json::json!([0., 0., 5.])
    );
}

#[test]
fn unicode_datum_witnesses_remain_valid_json_inside_svg_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(
        root.join("source.star"),
        SOURCE.replace("slice", "slice\u{fffe}"),
    )
    .unwrap();
    let summary = success(build(root, &[]));
    let value = witness(
        root,
        &summary,
        "nested_left",
        &["left", "same"],
        [2., 0., 5.],
    );
    assert_eq!(value["datum"], "slice\u{fffe}");
    let svg = fs::read_to_string(root.join("out/section-nested_left.svg")).unwrap();
    assert!(!svg.contains('\u{fffe}'));
}
