use std::{collections::BTreeMap, fs, process::Command};
fn build(source: &std::path::Path, out: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}
#[test]
fn invalid_concave_revision_keeps_last_complete_generation() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("bearing.star");
    let out = dir.path().join("out");
    let source = include_str!("../../../examples/split_bearing_block.star");
    fs::write(&recipe, source).unwrap();
    let good = build(&recipe, &out);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    let artifacts: BTreeMap<String, Vec<u8>> = manifest["artifact_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| {
            let name = name.as_str().unwrap();
            (name.to_owned(), fs::read(out.join(name)).unwrap())
        })
        .collect();
    // The clockwise seat now crosses the rectangle boundary. Keep all policy
    // and publication inputs unchanged so the native validator must reject it.
    fs::write(
        &recipe,
        source.replace("r = seat_radius", "r = seat_radius+27"),
    )
    .unwrap();
    let failed = build(&recipe, &out);
    assert!(!failed.status.success());
    let report: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(report["code"], "invalid_polygon_profile");
    assert!(report.to_string().contains("jaw_profile"));
    for (name, bytes) in artifacts {
        assert_eq!(fs::read(out.join(name)).unwrap(), bytes);
    }
}
