use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
const SOURCE: &str = r#"
distance=param(name="distance",default=1)
stock=extrude(id="stock",profile=rectangle(id="profile",width=12,depth=8),height=6)
edge=chamfer(id="edge",body=stock,face_normals=[[-1,0,0],[0,-1,0]],distance=distance)
output(name="block",feature=edge)
output(name="stock",feature=stock)
assert_faces(id="chamfer_face",from_feature=edge,role="chamfer",in_feature=edge,count=1)
"#;
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
#[test]
fn native_chamfer_publishes_and_replays_with_global_atomic_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("source.star"), SOURCE).unwrap();
    let before = ok(build(root, &[]));
    let replay = ok(build(root, &[]));
    assert_eq!(replay["stats"]["computed_solids"], 0);
    assert_eq!(
        before["outputs"]["block"]["content_hash"],
        replay["outputs"]["block"]["content_hash"]
    );
    let prior = files(&root.join("out"));
    for flags in [
        vec!["--param", "distance=8"],
        vec!["--param", "distance=8", "--output", "stock"],
        vec![
            "--param",
            "distance=8",
            "--output",
            "stock",
            "--display-only",
        ],
        vec!["--param", "distance=0"],
    ] {
        let result = build(root, &flags);
        assert!(!result.status.success());
        let _: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(files(&root.join("out")), prior);
    }
    let nonadjacent = SOURCE.replace("[0,-1,0]", "[1,0,0]");
    fs::write(root.join("source.star"), nonadjacent).unwrap();
    assert!(!build(root, &["--output", "stock"]).status.success());
    assert_eq!(files(&root.join("out")), prior);
    fs::write(root.join("source.star"), SOURCE).unwrap();
    let revised = ok(build(root, &["--param", "distance=2"]));
    assert_ne!(
        revised["outputs"]["block"]["content_hash"],
        before["outputs"]["block"]["content_hash"]
    );
    assert!(
        ok(cli(&["verify", root.join("out").to_str().unwrap()]))["valid"]
            .as_bool()
            .unwrap()
    );
}
