use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
const SOURCE: &str = r#"
pitch = param(name="pitch",default=20)
body=extrude(id="body",profile=rectangle(id="profile",width=8,depth=6),height=10)
p=part(id="part",body=body,datums={"top":face_datum(from_feature=body,role="end_cap",x_direction=[1,0,0])})
scene=assembly(id="scene",instances=[instance(id="left",part=p),instance(id="right",part=p,offset=[pitch,12,0])])
assembly_output(name="scene",assembly=scene)
output(name="body",feature=body)
section_view(id="plan",output="scene",origin=[0,0,5])
section_dimension(view="plan",id="pitch",from_instance_path=["left"],from_datum="top",to_instance_path=["right"],to_datum="top",axis="x",offset=-8,min=10,max=30)
section_dimension(view="plan",id="rise",from_instance_path=["right"],from_datum="top",to_instance_path=["left"],to_datum="top",axis="y",offset=40,min=12,max=12)
"#;
fn command(args: &[&str]) -> Output {
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
    command(&args)
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
fn report(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("out/section-plan.json")).unwrap()).unwrap()
}
#[test]
fn dimensions_bind_projected_measurements_and_svg_and_follow_cached_placement() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    let before = ok(build(root, &[]));
    let first = report(root);
    let dims = &first["section_view"]["dimensions"];
    assert_eq!(dims, &before["section_views"]["plan"]["dimensions"]);
    assert_eq!(dims[0]["value_mm"], 20.);
    assert_eq!(dims[1]["value_mm"], 12.);
    assert_eq!(dims[1]["signed_delta_mm"], -12.);
    assert_eq!(dims[0]["from_point"], serde_json::json!([4., 3.]));
    assert_eq!(dims[0]["to_point"], serde_json::json!([24., 15.]));
    assert_eq!(
        dims[0]["from_witness"]["semantic_datum"]["selection"]["status"],
        "resolved"
    );
    let svg = fs::read_to_string(root.join("out/section-plan.svg")).unwrap();
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
    assert!(svg.contains("pitch: 20 mm"));
    assert!(svg.contains("rise: 12 mm"));
    let original = files(&root.join("out"));
    ok(build(root, &[]));
    let repeated = files(&root.join("out"));
    assert_eq!(original["section-plan.json"], repeated["section-plan.json"]);
    assert_eq!(original["section-plan.svg"], repeated["section-plan.svg"]);
    let moved = ok(build(root, &["--param", "pitch=25"]));
    assert_eq!(moved["stats"]["computed_solids"], 0);
    let next = report(root);
    assert_eq!(next["section_view"]["dimensions"][0]["value_mm"], 25.);
    assert_ne!(
        first["snapshot"]["query_context_hash"],
        next["snapshot"]["query_context_hash"]
    );
    assert!(
        ok(command(&["verify", root.join("out").to_str().unwrap()]))["valid"]
            .as_bool()
            .unwrap()
    );
}
#[test]
fn dimension_failure_is_global_atomic_and_structured() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    ok(build(root, &[]));
    let original = files(&root.join("out"));
    for flags in [
        vec!["--param", "pitch=31"],
        vec!["--param", "pitch=31", "--output", "body"],
        vec!["--param", "pitch=31", "--display-only"],
    ] {
        let out = build(root, &flags);
        assert!(!out.status.success());
        let err: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(err["code"], "section_dimension_failed");
        assert_eq!(err["dimension_id"], "pitch");
        assert_eq!(err["actual_value_mm"], 31.);
        assert_eq!(files(&root.join("out")), original);
    }
    for source in [
        SOURCE.replace("to_datum=\"top\"", "to_datum=\"missing\""),
        SOURCE.replace(
            "to_instance_path=[\"right\"]",
            "to_instance_path=[\"left\"]",
        ),
    ] {
        fs::write(root.join("source.star"), source).unwrap();
        assert!(!build(root, &["--output", "body"]).status.success());
        assert_eq!(files(&root.join("out")), original);
    }
}
