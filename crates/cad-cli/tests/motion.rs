use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
fn good(o: Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}
fn bytes(p: &Path) -> BTreeMap<String, Vec<u8>> {
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
fn studies_publish_atomically_preserve_nominal_geometry_and_replay_captured_sources() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("motion_four_bar.star");
    fs::write(
        &source,
        include_str!("../../../examples/motion_four_bar.star"),
    )
    .unwrap();
    let module = temp.path().join("solved_four_bar.star");
    fs::write(
        &module,
        include_str!("../../../examples/solved_four_bar.star"),
    )
    .unwrap();
    let lock = temp.path().join("motion_four_bar.star.lock.json");
    fs::write(
        &lock,
        include_str!("../../../examples/motion_four_bar.star.lock.json"),
    )
    .unwrap();
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
    let before = bytes(&out);
    assert!(before.contains_key("motion-crank_cycle.json"));
    assert!(before.contains_key("motion-crank_cycle.study.html"));
    for (value, has_prefix) in [("4", false), ("1.3", true)] {
        let flags = if has_prefix {
            vec![
                "--param",
                "max_pin_step=0.000000000001",
                "--output",
                "coupler",
                "--display-only",
            ]
        } else {
            vec!["--param", "high_angle=4", "--output", "coupler"]
        };
        let failure = run(&flags);
        assert!(!failure.status.success(), "{value}");
        let e: Value = serde_json::from_slice(&failure.stderr).unwrap();
        assert_eq!(e["code"], "motion_study_failed", "{e}");
        if has_prefix {
            assert!(!e["motion_failure"]["accepted"]["frames"]
                .as_array()
                .unwrap()
                .is_empty())
        }
        assert_eq!(bytes(&out), before);
    }
    let moved = good(run(&["--param", "max_pin_step=1"]));
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_eq!(
        moved["outputs"]["solved_four_bar"]["content_hash"],
        first["outputs"]["solved_four_bar"]["content_hash"]
    );
    assert_ne!(
        moved["mechanism_context_hash"],
        first["mechanism_context_hash"]
    );
    assert_ne!(moved["recipe_context_hash"], first["recipe_context_hash"]);
    let published: Value =
        serde_json::from_slice(&fs::read(out.join("motion-crank_cycle.json")).unwrap()).unwrap();
    assert_eq!(
        published["snapshot"]["recipe_context_hash"],
        moved["recipe_context_hash"]
    );
    assert_eq!(
        published["study"]["solution"]["completed_requested_frames"],
        4
    );
    fs::remove_file(source).unwrap();
    fs::remove_file(module).unwrap();
    fs::remove_file(lock).unwrap();
    let replay = temp.path().join("replay");
    let result = good(
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                out.join("recipe.bundle.json").to_str().unwrap(),
                "--param",
                "max_pin_step=1",
                "--out-dir",
                replay.to_str().unwrap(),
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(
        result["motion_studies"]["crank_cycle"]["solution"],
        moved["motion_studies"]["crank_cycle"]["solution"]
    );
    assert_eq!(
        result["mechanism_context_hash"],
        moved["mechanism_context_hash"]
    );
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
