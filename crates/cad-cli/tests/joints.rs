use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn good(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn artifacts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into(),
                fs::read(p).unwrap(),
            )
        })
        .collect()
}
#[test]
fn joint_limit_failures_preserve_generation_and_motion_edits_reuse_parts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = root.join("examples/jointed_linear_stage.star");
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |flags: &[&str]| {
        let mut args = vec![
            "--cache-dir",
            cache.to_str().unwrap(),
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(flags);
        cli(&args)
    };
    let first = good(run(&[]));
    let before = artifacts(&out);
    for flags in [
        vec!["--param", "travel=101"],
        vec!["--param", "travel=19", "--output", "carriage"],
        vec![
            "--param",
            "angle=4",
            "--output",
            "carriage",
            "--display-only",
        ],
    ] {
        let result = run(&flags);
        assert!(!result.status.success());
        let _: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(artifacts(&out), before);
    }
    let moved = good(run(&["--param", "travel=80", "--param", "angle=-1"]));
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_eq!(
        moved["outputs"]["carriage"]["content_hash"],
        first["outputs"]["carriage"]["content_hash"]
    );
    assert_ne!(
        moved["outputs"]["jointed_linear_stage"]["content_hash"],
        first["outputs"]["jointed_linear_stage"]["content_hash"]
    );
    let reports = moved["outputs"]["jointed_linear_stage"]["joints"]
        .as_array()
        .unwrap();
    assert_eq!(reports.len(), 2);
    for report in reports {
        assert_eq!(report["coordinate_frame"], "assembly_definition");
        assert_eq!(report["remaining_dof"], 0);
    }
    assert_eq!(good(cli(&["verify", out.to_str().unwrap()]))["valid"], true);
}
