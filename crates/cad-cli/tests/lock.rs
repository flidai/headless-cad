use serde_json::Value;
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
fn lock(path: &Path) -> Output {
    cli(&["lock", path.to_str().unwrap()])
}
fn fixture(root: &Path) -> std::path::PathBuf {
    fs::create_dir(root.join("lib")).unwrap();
    let path = root.join("root.star");
    fs::write(&path, "load('lib/a.star','a')\nfail('must not evaluate')\n").unwrap();
    fs::write(root.join("lib/a.star"), "load('lib/b.star','b')\na=b\n").unwrap();
    fs::write(root.join("lib/b.star"), "b=1\n").unwrap();
    path
}
#[test]
fn explicit_lock_discovers_without_evaluation_and_tracks_transitive_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = fixture(root);
    let lockpath = root.join("root.star.lock.json");
    let first = good(lock(&path));
    assert_eq!(first["module_count"], 2);
    assert_eq!(first["evaluated_recipe"], false);
    assert_eq!(
        first["added"],
        serde_json::json!(["lib/a.star", "lib/b.star"])
    );
    let bytes = fs::read(&lockpath).unwrap();
    let again = good(lock(&path));
    assert_eq!(again["updated"], false);
    assert_eq!(again["added"], serde_json::json!([]));
    assert_eq!(fs::read(&lockpath).unwrap(), bytes);
    fs::write(root.join("lib/b.star"), "b=2\n").unwrap();
    let changed = good(lock(&path));
    assert_eq!(changed["changed"], serde_json::json!(["lib/b.star"]));
    fs::write(root.join("lib/a.star"), "a=1\n").unwrap();
    let changed = good(lock(&path));
    assert_eq!(changed["removed"], serde_json::json!(["lib/b.star"]));
    assert_eq!(changed["changed"], serde_json::json!(["lib/a.star"]));
    assert!(!cli(&["--param", "x=1", "lock", path.to_str().unwrap()])
        .status
        .success());
    assert!(!cli(&[
        "--cache-dir",
        root.join("cache").to_str().unwrap(),
        "lock",
        path.to_str().unwrap()
    ])
    .status
    .success());
    assert!(!root.join("cache").exists());
}
#[test]
fn invalid_discovery_preserves_previous_lock_and_leaves_no_staging_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = fixture(root);
    good(lock(&path));
    let lockpath = root.join("root.star.lock.json");
    let bytes = fs::read(&lockpath).unwrap();
    for text in [
        "load('lib/a.star','a')\nb=a\n",
        "load('../outside.star','b')\n",
        "load('missing.star','b')\n",
        "def (bad syntax",
    ] {
        fs::write(root.join("lib/b.star"), text).unwrap();
        assert!(!lock(&path).status.success());
        assert_eq!(fs::read(&lockpath).unwrap(), bytes);
    }
    fs::write(root.join("lib/b.star"), "b=1\n").unwrap();
    fs::write(&lockpath, "unrelated user data").unwrap();
    assert!(!lock(&path).status.success());
    assert_eq!(
        fs::read_to_string(&lockpath).unwrap(),
        "unrelated user data"
    );
    assert!(!fs::read_dir(root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".cad-lock-")));
}
#[test]
#[cfg(unix)]
fn symlink_destinations_and_escaping_modules_are_rejected_but_local_aliases_work() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = fixture(root);
    let lockpath = root.join("root.star.lock.json");
    let target = root.join("target.json");
    std::os::unix::fs::symlink(&target, &lockpath).unwrap();
    assert!(!lock(&path).status.success());
    assert!(!target.exists());
    fs::remove_file(&lockpath).unwrap();
    fs::rename(root.join("lib/b.star"), root.join("shared.star")).unwrap();
    std::os::unix::fs::symlink("../shared.star", root.join("lib/b.star")).unwrap();
    good(lock(&path));
    let bytes = fs::read(&lockpath).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("b.star"), "b=1").unwrap();
    fs::remove_file(root.join("lib/b.star")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("b.star"), root.join("lib/b.star")).unwrap();
    assert!(!lock(&path).status.success());
    assert_eq!(fs::read(&lockpath).unwrap(), bytes);
}
