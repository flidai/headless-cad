use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};
fn run(path: &std::path::Path, output: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["section", path.to_str().unwrap(), "--output", output])
        .args(extra)
        .output()
        .unwrap()
}
fn parsed(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
const BOX:&str="b=extrude(id='b',profile=rectangle(id='p',width=10,depth=12),height=8)\noutput(name='body',feature=b)\n";
#[test]
fn section_reports_exact_geometry_provenance_and_pending_policy() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("part.star");
    let source = format!(
        "{BOX}require_exchange(id='unit_policy',output='body',format='stl',aspect='units')\n"
    );
    fs::write(&path, &source).unwrap();
    let value = parsed(run(&path, "body", &["--origin", "0, 0, 4"]));
    assert_eq!(value["section"]["area"], 120.);
    assert_eq!(value["section"]["centroid"], serde_json::json!([5., 6.]));
    assert_eq!(
        value["snapshot"]["recipe_source_hash"],
        blake3::hash(source.as_bytes()).to_hex().to_string()
    );
    assert_eq!(
        value["exchange_requirements"]["unit_policy"]["status"],
        "pending_publication"
    );
    assert_eq!(value["exchange_policy_status"], "pending_publication");
    assert_eq!(value["units"]["area"], "mm2");
    let other = parsed(run(&path, "body", &["--origin", "0,0,5"]));
    assert_eq!(
        value["snapshot"]["content_hash"],
        other["snapshot"]["content_hash"]
    );
    assert_ne!(
        value["snapshot"]["query_context_hash"],
        other["snapshot"]["query_context_hash"]
    );
    let svg = run(&path, "body", &["--origin", "0,0,4", "--format", "svg"]);
    assert!(svg.status.success());
    let svg = String::from_utf8(svg.stdout).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("<metadata"));
    assert!(svg.contains(&blake3::hash(source.as_bytes()).to_hex().to_string()));
    for origin in ["0,0,0", "0,0,8", "0,0", "nan,0,4"] {
        let out = run(&path, "body", &["--origin", origin, "--format", "svg"]);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["code"], "section_failed");
        assert_eq!(error["output_name"], "body");
    }
    assert_eq!(
        parsed(run(&path, "body", &["--origin", "0,0,20"]))["section"]["area"],
        0.
    );
}
#[test]
fn assembly_sections_share_drawing_coordinates_and_count_overlaps() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("assembly.star");
    fs::write(&path,format!("{BOX}p=part(id='part',body=b)\nassembly_output(name='machine',assembly=assembly(id='root',instances=[instance(id='a',part=p),instance(id='b',part=p,offset=[5,0,0])]))\n")).unwrap();
    let value = parsed(run(&path, "machine", &["--origin", "0,0,4"]));
    assert_eq!(value["section"]["area"], 240.);
    assert_eq!(value["section"]["centroid"], serde_json::json!([7.5, 6.]));
    assert_eq!(value["area_kind"], "summed_occurrence_material_not_union");
    assert_eq!(value["occurrences"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["occurrences"][1]["section"]["centroid"],
        serde_json::json!([10., 6.])
    );
    assert!(value["section"].get("loops").is_none());
    let svg = run(&path, "machine", &["--origin", "0,0,4", "--format", "svg"]);
    assert!(svg.status.success());
    let text = String::from_utf8(svg.stdout).unwrap();
    assert!(text.contains("data-occurrence-index=\"0\""));
    assert!(text.contains("data-occurrence-index=\"1\""));
    assert_ne!(
        value["occurrences"][0]["instance_path"],
        value["occurrences"][1]["instance_path"]
    );
}
#[test]
fn selective_section_keeps_global_assertions_and_does_not_evaluate_unused_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("selected.star");
    let unused="c=extrude(id='c',profile=circle(id='cp',radius=3),height=8)\nd=translate(id='d',body=c,offset=[1,0,0])\nbad=union(id='bad',left=b,right=d)\noutput(name='unused',feature=bad)\n";
    fs::write(&path, format!("{BOX}{unused}")).unwrap();
    parsed(run(&path, "body", &["--origin", "0,0,4"]));
    let all = Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        !all.status.success(),
        "unselected fixture must fail if evaluated"
    );
    let failure: Value = serde_json::from_slice(&all.stderr).unwrap();
    assert_eq!(failure["code"], "kernel_operation_failed");
    fs::write(&path,format!("{BOX}{unused}assert_property(id='global',output='body',property='volume_mm3',min=1,max=2)\n")).unwrap();
    let out = run(&path, "body", &["--origin", "0,0,4"]);
    assert!(!out.status.success());
    let error: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["code"], "property_assertion_failed");
    assert!(out.stdout.is_empty());
}

#[test]
fn empty_assembly_still_validates_plane_frame() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("empty.star");
    fs::write(
        &path,
        "assembly_output(name='empty',assembly=assembly(id='root',instances=[]))\n",
    )
    .unwrap();
    let good = parsed(run(&path, "empty", &["--origin", "0,0,1"]));
    assert_eq!(good["section"]["area"], 0.);
    let bad = run(&path, "empty", &["--origin", "0,0,1", "--normal", "0,0,2"]);
    assert!(!bad.status.success());
    let error: Value = serde_json::from_slice(&bad.stderr).unwrap();
    assert_eq!(error["code"], "section_failed");
}

#[test]
fn rotated_assembly_section_uses_world_plane_and_local_face_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("rotated.star");
    fs::write(&path, format!("{BOX}p=part(id='part',body=b)\nassembly_output(name='machine',assembly=assembly(id='root',instances=[instance(id='rotated',part=p,offset=[12,7,4],rotation=[0.7071067811865476,0,0,0.7071067811865476])]))\n")).unwrap();
    let value = parsed(run(
        &path,
        "machine",
        &[
            "--origin",
            "12,3,4",
            "--normal",
            "0,-1,0",
            "--x-direction",
            "1,0,0",
        ],
    ));
    assert!((value["section"]["area"].as_f64().unwrap() - 120.).abs() < 1e-9);
    for (axis, expected) in [5., 6.].into_iter().enumerate() {
        assert!((value["section"]["centroid"][axis].as_f64().unwrap() - expected).abs() < 1e-9);
    }
    let occurrence = &value["occurrences"][0];
    assert_eq!(occurrence["instance_id"], "rotated");
    assert_eq!(occurrence["part_id"], "part");
    assert_eq!(occurrence["section"]["plane"], value["section"]["plane"]);
    assert_eq!(
        occurrence["section"]["loops"][0]["segments"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
