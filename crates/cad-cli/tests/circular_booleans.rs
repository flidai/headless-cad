use std::{collections::BTreeMap, fs, process::Command};

#[test]
fn unsupported_circular_revisions_preserve_every_published_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("fixture.star");
    let out = dir.path().join("out");
    let source = include_str!("../../../examples/twin_lobe_fixture.star");
    let build = || {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                recipe.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    fs::write(&recipe, source).unwrap();
    let good = build();
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
    // Contact, a partial-depth cut, and a sub-tolerance cap mismatch must fail
    // instead of perturbing the operand or publishing an approximation.
    for placement in ["[40,0,0]", "[spacing,0,1]", "[spacing,0,0.000000000001]"] {
        let changed = source.replace("offset = [spacing,0,0]", &format!("offset = {placement}"));
        assert_ne!(source, changed);
        fs::write(&recipe, changed).unwrap();
        let failed = build();
        assert!(!failed.status.success(), "accepted {placement}");
        let report: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        // A rejected difference may try both circular arrangements and round-stock
        // drilling. The stable API is the failure code and owning feature, not
        // whichever bounded geometry path supplies the final diagnostic text.
        assert_eq!(report["code"], "kernel_operation_failed", "{report}");
        assert_eq!(report["feature"], "left_lobe", "{report}");
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), manifest_bytes);
        for (name, bytes) in &artifacts {
            assert_eq!(
                &fs::read(out.join(name)).unwrap(),
                bytes,
                "{placement}: {name}"
            );
        }
    }
}
