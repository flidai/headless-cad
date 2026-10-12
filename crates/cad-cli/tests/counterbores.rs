use std::{collections::BTreeMap, fs, process::Command};
#[test]
fn failed_counterbore_and_shoulder_obligations_preserve_last_generation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fixture.star");
    let out = dir.path().join("out");
    let source = include_str!("../../../examples/counterbored_fixture.star");
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
    for (case, changed) in [
        source.replace("[[20,20],[60,20]", "[[6,20],[60,20]"),
        source.replace(
            "offset = [xy[0],xy[1],z]",
            "offset = [xy[0]+(1 if prefix == \"top\" else 0),xy[1],z]",
        ),
        source
            .replace("height = h+2", "height = d+4")
            .replace("top_tool, d-h)", "top_tool, -2)")
            // Keep the subsequent bottom cuts material-removing so this case
            // reaches the missing top-seat obligation, rather than a no-op cut.
            .replace("radius = 4.5", "radius = 9")
            .replace("assert_faces(id = \"shortened_pilot_walls\", from_feature = pilot_tool, role = \"wall\", in_feature = finished, count = 6)", ""),
    ].into_iter().enumerate() {
        assert_ne!(changed, source);
        fs::write(&file, changed).unwrap();
        let failed = build(true);
        assert!(!failed.status.success());
        let report: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(report["code"], if case == 2 { "face_assertion_failed" } else { "kernel_operation_failed" }, "{report}");
        assert!(report.to_string().contains("top_"), "{report}");
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), manifest);
        for (name, bytes) in &artifacts {
            assert_eq!(&fs::read(out.join(name)).unwrap(), bytes, "{name}");
        }
    }
}
