use std::{collections::BTreeMap, fs, process::Command};
#[test]
fn failed_blind_machining_and_lost_floor_obligations_preserve_last_generation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fixture.star");
    let out = dir.path().join("out");
    let source = include_str!("../../../examples/blind_bore_pallet.star");
    let build = |selected: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cad"));
        command.arg("build");
        if selected {
            command.args(["--output", "stock"]);
        }
        command
            .arg(&file)
            .arg("--out-dir")
            .arg(&out)
            .output()
            .unwrap()
    };
    fs::write(&file, source).unwrap();
    let good = build(false);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let artifacts: BTreeMap<_, _> = value["artifact_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let name = v.as_str().unwrap().to_owned();
            let bytes = fs::read(out.join(&name)).unwrap();
            (name, bytes)
        })
        .collect();
    for changed in [
        source.replace("[[20,20],[60,20]", "[[8,20],[60,20]"),
        source.replace("[[20,20],[60,20]", "[[20,20],[24,20]"),
        source.replace("top_centers, d-h)", "top_centers, d-h-3)"),
        source
            .replace("height = h+2", "height = d+4")
            .replace("top_centers, d-h)", "top_centers, -2)"),
    ] {
        assert_ne!(changed, source);
        fs::write(&file, changed).unwrap();
        let failed = build(true);
        assert!(!failed.status.success());
        let report: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert!(report["code"].is_string(), "{report}");
        assert!(report.to_string().contains("top_"), "{report}");
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), manifest);
        for (name, bytes) in &artifacts {
            assert_eq!(&fs::read(out.join(name)).unwrap(), bytes, "{name}");
        }
    }
}
