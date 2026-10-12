use std::{collections::BTreeMap, fs, process::Command};

#[test]
fn failed_machining_obligations_preserve_publication_even_when_stock_is_selected() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("fixture.star");
    let out = dir.path().join("out");
    let source = include_str!("../../../examples/machined_bearing_support.star");
    let build = |selected: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cad"));
        if selected {
            command.args(["build", "--output", "stock"]);
        } else {
            command.arg("build");
        }
        command
            .args([recipe.to_str().unwrap(), "--out-dir", out.to_str().unwrap()])
            .output()
            .unwrap()
    };
    fs::write(&recipe, source).unwrap();
    let good = build(false);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let manifest_bytes = fs::read(out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let artifacts: BTreeMap<String, Vec<u8>> = manifest["artifact_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| {
            let name = name.as_str().unwrap();
            (name.to_owned(), fs::read(out.join(name)).unwrap())
        })
        .collect();
    // Even an unchanged selected stock cannot bypass global obligations on
    // its machining sequence. Fail crossing/overlapping bores and fully enclosed cuts.
    for (old, new) in [
        ("[[12,12],[88,12]", "[[3,12],[88,12]"),
        ("[[12,12],[88,12]", "[[12,12],[14,12]"),
        ("height = d+4", "height = d-4"),
    ] {
        let mut changed = source.replace(old, new);
        if old == "height = d+4" {
            changed = changed.replace("offset = [p[0],p[1],-2]", "offset = [p[0],p[1],2]");
        }
        assert_ne!(source, changed);
        fs::write(&recipe, changed).unwrap();
        let failed = build(true);
        assert!(!failed.status.success(), "accepted {new}");
        let report: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert!(report["code"].is_string(), "{report}");
        assert!(report.to_string().contains("drill_"), "{report}");
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), manifest_bytes);
        for (name, bytes) in &artifacts {
            assert_eq!(&fs::read(out.join(name)).unwrap(), bytes, "{new}: {name}");
        }
    }
}
