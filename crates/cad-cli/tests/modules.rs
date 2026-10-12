use serde_json::{json, Value};
use std::{
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
const ROOT: &str = "load(\"lib/part.star\", \"make\")\nb=make()\noutput(name=\"part\",feature=b)\nsection_view(id=\"slice\",output=\"part\",origin=[0,0,1])\n";
fn module(width: usize) -> String {
    format!(
        "def make():\n    p=rectangle(id=\"p\",width={width},depth=4)\n    return extrude(id=\"b\",profile=p,height=2)\n"
    )
}
fn lock(root: &Path, text: &str) {
    fs::write(root.join("part.star.lock.json"),serde_json::to_vec(&json!({"schema_version":1,"modules":{"lib/part.star":{"blake3":blake3::hash(text.as_bytes()).to_hex().to_string()}}})).unwrap()).unwrap();
}
fn setup(root: &Path) {
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(root.join("part.star"), ROOT).unwrap();
    fs::write(root.join("lib/part.star"), module(10)).unwrap();
    lock(root, &module(10));
}
#[test]
fn locked_bundle_publishes_queries_and_replays_offline_with_identical_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let path = root.join("part.star");
    let out = root.join("out");
    let built = good(cli(&[
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]));
    let hash = built["source_bundle_hash"].clone();
    assert!(hash.as_str().unwrap().len() == 64);
    let props = good(cli(&[
        "properties",
        path.to_str().unwrap(),
        "--output",
        "part",
    ]));
    assert_eq!(props["snapshot"]["source_bundle_hash"], hash);
    let section = good(cli(&[
        "section",
        path.to_str().unwrap(),
        "--output",
        "part",
        "--origin",
        "0,0,1",
    ]));
    assert_eq!(section["snapshot"]["source_bundle_hash"], hash);
    let view: Value =
        serde_json::from_slice(&fs::read(out.join("section-slice.json")).unwrap()).unwrap();
    assert_eq!(view["snapshot"]["source_bundle_hash"], hash);
    good(cli(&["verify", out.to_str().unwrap()]));
    fs::remove_file(&path).unwrap();
    fs::remove_dir_all(root.join("lib")).unwrap();
    fs::remove_file(root.join("part.star.lock.json")).unwrap();
    let replay = root.join("replay");
    let bundle = out.join("recipe.bundle.json");
    let rebuilt = good(cli(&[
        "build",
        bundle.to_str().unwrap(),
        "--out-dir",
        replay.to_str().unwrap(),
    ]));
    assert_eq!(rebuilt["source_bundle_hash"], hash);
    assert_eq!(rebuilt["recipe_context_hash"], built["recipe_context_hash"]);
    assert_eq!(
        rebuilt["outputs"]["part"]["content_hash"],
        built["outputs"]["part"]["content_hash"]
    );
}
#[test]
fn module_changes_require_explicit_repin_and_failed_build_preserves_generation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let path = root.join("part.star");
    let out = root.join("out");
    let args = [
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ];
    let first = good(cli(&args));
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    fs::write(root.join("lib/part.star"), module(20)).unwrap();
    let failed = cli(&args);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("digest mismatch"));
    assert_eq!(manifest, fs::read(out.join("manifest.json")).unwrap());
    lock(root, &module(20));
    let second = good(cli(&args));
    assert_ne!(first["source_bundle_hash"], second["source_bundle_hash"]);
    assert_eq!(first["recipe_source_hash"], second["recipe_source_hash"]);
    let overlap = cli(&[
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        root.join("lib").to_str().unwrap(),
    ]);
    assert!(!overlap.status.success());
    assert!(root.join("lib/part.star").exists());
}
#[test]
fn strict_locks_reject_escape_symlinks_duplicates_missing_and_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let path = root.join("part.star");
    let lockpath = root.join("part.star.lock.json");
    for text in [
        r#"{"schema_version":1,"modules":{"../evil.star":{"blake3":"x"}}}"#,
        r#"{"schema_version":1,"modules":{},"network":true}"#,
        r#"{"schema_version":1,"modules":{"a.star":{"blake3":"x"},"a.star":{"blake3":"x"}}}"#,
    ] {
        fs::write(&lockpath, text).unwrap();
        assert!(!cli(&["compile", path.to_str().unwrap()]).status.success());
    }
    fs::remove_file(&lockpath).unwrap();
    assert!(!cli(&["compile", path.to_str().unwrap()]).status.success());
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("part.star"), module(10)).unwrap();
        fs::remove_file(root.join("lib/part.star")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("part.star"), root.join("lib/part.star"))
            .unwrap();
        lock(root, &module(10));
        let result = cli(&["compile", path.to_str().unwrap()]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("symlink escapes"));
    }
}

#[test]
#[cfg(unix)]
fn publication_protects_module_symlink_alias_as_well_as_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    fs::rename(root.join("lib/part.star"), root.join("shared.star")).unwrap();
    std::os::unix::fs::symlink("../shared.star", root.join("lib/part.star")).unwrap();
    let path = root.join("part.star");
    let result = cli(&[
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        root.join("lib").to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("output directory cannot contain"));
    assert!(fs::symlink_metadata(root.join("lib/part.star"))
        .unwrap()
        .file_type()
        .is_symlink());
    good(cli(&["check", path.to_str().unwrap()]));
}

#[test]
fn comment_only_module_edit_changes_source_evidence_while_reusing_geometry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let path = root.join("part.star");
    let out = root.join("out");
    let cache = root.join("cache");
    let args = [
        "--cache-dir",
        cache.to_str().unwrap(),
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ];
    let first = good(cli(&args));
    let query_args = [
        "section",
        path.to_str().unwrap(),
        "--output",
        "part",
        "--origin",
        "0,0,1",
    ];
    let query = good(cli(&query_args));
    let edited = format!("{}\n# reviewed implementation\n", module(10));
    fs::write(root.join("lib/part.star"), &edited).unwrap();
    lock(root, &edited);
    let second = good(cli(&args));
    let changed_query = good(cli(&query_args));
    assert_eq!(
        first["outputs"]["part"]["content_hash"],
        second["outputs"]["part"]["content_hash"]
    );
    assert_eq!(second["stats"]["computed_solids"], 0);
    assert_ne!(first["source_bundle_hash"], second["source_bundle_hash"]);
    assert_ne!(first["recipe_context_hash"], second["recipe_context_hash"]);
    assert_ne!(
        query["snapshot"]["query_context_hash"],
        changed_query["snapshot"]["query_context_hash"]
    );
    assert_eq!(query["section"], changed_query["section"]);
}
