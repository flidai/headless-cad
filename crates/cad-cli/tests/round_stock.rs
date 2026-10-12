use std::{collections::BTreeMap, fs, process::Command};
#[test]
fn invalid_round_stock_machining_preserves_the_complete_generation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fixture.star");
    let out = dir.path().join("out");
    let source = include_str!("../../../examples/round_bearing_cartridge.star");
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
        source.replace("[[30,0]", "[[22,0]"),
        source.replace("[[30,0]", "[[58,0]"),
        source.replace(
            "offset = [xy[0],xy[1],z]",
            "offset = [xy[0]+(1 if prefix == \"seat\" else 0),xy[1],z]",
        ),
        source
            .replace("height = 7)", "height = d+4)")
            .replace("rod_centers, d-5)", "rod_centers, -2)"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_ne!(changed, source);
        fs::write(&file, changed).unwrap();
        let failed = build(true);
        assert!(!failed.status.success());
        let report: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(
            report["code"],
            if case == 3 {
                "face_assertion_failed"
            } else {
                "kernel_operation_failed"
            },
            "{report}"
        );
        assert!(
            report
                .to_string()
                .contains(if case == 2 { "seat_" } else { "socket_" }),
            "{report}"
        );
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), manifest);
        for (name, bytes) in &artifacts {
            assert_eq!(&fs::read(out.join(name)).unwrap(), bytes, "{name}");
        }
    }
}
