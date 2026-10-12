use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
const SOURCE: &str = include_str!("../../../examples/solved_four_bar.star");
fn good(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn artifacts(p: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(p)
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
fn failed_mechanisms_preserve_publication_and_valid_motion_reuses_geometry() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("mechanism.star");
    fs::write(&source, SOURCE).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |flags: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                source.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
            ])
            .args(flags)
            .output()
            .unwrap()
    };
    let first = good(run(&[]));
    let before = artifacts(&out);
    for (text, flags) in [
        (
            SOURCE.replace(
                "drives = [{\"id\":\"input_angle\", \"link\":\"crank\", \"angle_radians\":angle}]",
                "drives = []",
            ),
            vec!["--output", "coupler"],
        ),
        (
            SOURCE.replace("\"side\":\"left\"", "\"side\":\"right\""),
            vec!["--display-only", "--output", "coupler"],
        ),
        (SOURCE.to_owned(), vec!["--param", "coupler_length=200"]),
    ] {
        fs::write(&source, text).unwrap();
        let failed = run(&flags);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "mechanism_solve_failed", "{error}");
        assert!(error["solver_report"].is_object(), "{error}");
        assert_eq!(artifacts(&out), before);
    }
    fs::write(&source, SOURCE).unwrap();
    let moved = good(run(&["--param", "angle=1.1"]));
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_eq!(
        first["outputs"]["coupler"]["content_hash"],
        moved["outputs"]["coupler"]["content_hash"]
    );
    assert_ne!(
        first["outputs"]["solved_four_bar"]["content_hash"],
        moved["outputs"]["solved_four_bar"]["content_hash"]
    );
    assert_ne!(
        first["mechanism_context_hash"],
        moved["mechanism_context_hash"]
    );
    assert!(moved["mechanism_reports"]["four_bar"].is_object());
    assert_eq!(
        good(
            Command::new(env!("CARGO_BIN_EXE_cad"))
                .args(["verify", out.to_str().unwrap()])
                .output()
                .unwrap()
        )["valid"],
        true
    );
}
