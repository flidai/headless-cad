use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::{Command, Output},
};

const SOURCE: &str = "width = param(name = 'width', default = 8)\np = rectangle(id = 'p', width = width, depth = 4)\na = extrude(id = 'a', profile = p, height = 2)\nb = translate(id = 'b', body = a, offset = [0,0,3])\noutput(name = 'base', feature = a)\noutput(name = 'raised', feature = b)\n";

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}
fn success(result: Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn rejected(result: Output) {
    assert!(
        !result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["code"], "artifact_integrity_failed", "{error}");
}
fn build(root: &Path, options: &[&str]) -> std::path::PathBuf {
    let source = root.join("source.star");
    let out = root.join("publication");
    fs::write(&source, SOURCE).unwrap();
    let mut args = vec![
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--param",
        "width=10",
    ];
    args.extend_from_slice(options);
    success(cli(&args));
    out
}
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, found: &mut BTreeMap<String, Vec<u8>>) {
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                visit(root, &path, found);
            } else {
                found.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut found = BTreeMap::new();
    visit(root, root, &mut found);
    found
}
fn manifest(out: &Path) -> Value {
    serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap()
}
fn write_manifest(out: &Path, value: &Value) {
    fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

#[test]
fn publications_have_complete_byte_integrity_and_verify_without_mutation() {
    for options in [
        vec![],
        vec!["--display-only"],
        vec!["--output", "raised"],
        vec!["--display-only", "--output", "raised"],
    ] {
        let root = tempfile::tempdir().unwrap();
        let out = build(root.path(), &options);
        let before = tree(root.path());
        let value = manifest(&out);
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["artifact_integrity"]["algorithm"], "blake3");
        let files = value["artifact_integrity"]["files"].as_object().unwrap();
        assert!(!files.contains_key("manifest.json"));
        let owned: BTreeSet<_> = value["artifact_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert!(owned.contains("manifest.json"));
        let actual: BTreeSet<_> = tree(&out).into_keys().collect();
        assert_eq!(owned, actual);
        assert_eq!(owned.len(), files.len() + 1);
        let mut total = 0u64;
        for (name, record) in files {
            assert!(owned.contains(name));
            let bytes = fs::read(out.join(name)).unwrap();
            assert_eq!(record["bytes"], bytes.len() as u64);
            assert_eq!(record["digest"], blake3::hash(&bytes).to_hex().to_string());
            total += bytes.len() as u64;
        }
        assert_eq!(fs::read_to_string(out.join("recipe.star")).unwrap(), SOURCE);
        assert_eq!(value["recipe_source"]["parameter_overrides"]["width"], 10.0);
        if options.contains(&"--display-only") {
            assert!(!files.keys().any(|name| name.ends_with(".step")
                || name.ends_with(".stl")
                || name.ends_with(".brep.json")));
        } else {
            assert!(files.contains_key("raised.step"));
        }
        if options.contains(&"--output") {
            assert!(!files.keys().any(|name| name.starts_with("base.")));
        } else {
            assert!(files.keys().any(|name| name.starts_with("base.")));
        }
        let hash = blake3::hash(&fs::read(out.join("manifest.json")).unwrap())
            .to_hex()
            .to_string();
        for anchored in [false, true] {
            let mut args = vec!["verify", out.to_str().unwrap()];
            if anchored {
                args.extend(["--manifest-hash", &hash]);
            }
            let report = success(cli(&args));
            assert_eq!(report["valid"], true);
            assert_eq!(report["algorithm"], "blake3");
            assert_eq!(report["manifest_hash"], hash);
            assert_eq!(report["verified_files"], files.len() as u64);
            assert_eq!(report["verified_bytes"], total);
        }
        assert_eq!(
            tree(root.path()),
            before,
            "verification changed the publication or source directory"
        );
    }
}

#[test]
fn verification_rejects_changed_missing_and_untrusted_manifest_bytes() {
    let root = tempfile::tempdir().unwrap();
    let out = build(root.path(), &[]);
    let artifact = out.join("raised.step");
    let original = fs::read(&artifact).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&artifact, changed).unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
    fs::remove_file(&artifact).unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
    fs::write(&artifact, original).unwrap();
    let original_manifest = fs::read(out.join("manifest.json")).unwrap();
    let hash = blake3::hash(&original_manifest).to_hex().to_string();
    // Whitespace keeps every manifest field and artifact digest valid, but changes
    // the release identity pinned by a recipient independently of this directory.
    let mut changed_manifest = original_manifest;
    changed_manifest.push(b'\n');
    fs::write(out.join("manifest.json"), changed_manifest).unwrap();
    success(cli(&["verify", out.to_str().unwrap()]));
    rejected(cli(&[
        "verify",
        out.to_str().unwrap(),
        "--manifest-hash",
        &hash,
    ]));
}

#[test]
fn verification_rejects_incomplete_unsafe_and_unknown_manifest_contracts() {
    let root = tempfile::tempdir().unwrap();
    let out = build(root.path(), &["--display-only"]);
    let original = manifest(&out);
    for case in 0..8 {
        let mut changed = original.clone();
        match case {
            0 => {
                changed
                    .as_object_mut()
                    .unwrap()
                    .remove("manifest_schema_version");
            }
            1 => {
                changed["manifest_schema_version"] = json!(2);
            }
            2 => {
                changed["artifact_integrity"]["files"]
                    .as_object_mut()
                    .unwrap()
                    .remove("recipe.star");
            }
            3 => {
                changed["artifact_files"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|name| name != "recipe.star");
            }
            4 => {
                changed["artifact_files"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("recipe.star"));
            }
            5 => {
                let record = changed["artifact_integrity"]["files"]
                    .as_object_mut()
                    .unwrap()
                    .remove("recipe.star")
                    .unwrap();
                changed["artifact_integrity"]["files"]["../source.star"] = record;
                for name in changed["artifact_files"].as_array_mut().unwrap() {
                    if name == "recipe.star" {
                        *name = json!("../source.star");
                    }
                }
            }
            6 => {
                changed["artifact_integrity"]["algorithm"] = json!("sha256");
            }
            _ => {
                changed["artifact_integrity"]["files"]["recipe.star"]["digest"] =
                    json!("not-a-digest");
            }
        }
        write_manifest(&out, &changed);
        rejected(cli(&["verify", out.to_str().unwrap()]));
    }
}

#[cfg(unix)]
#[test]
fn verification_rejects_symlinked_artifacts_even_with_matching_bytes() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let out = build(root.path(), &["--display-only"]);
    fs::remove_file(out.join("recipe.star")).unwrap();
    symlink(root.path().join("source.star"), out.join("recipe.star")).unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
    fs::remove_file(out.join("recipe.star")).unwrap();
    fs::write(out.join("recipe.star"), SOURCE).unwrap();
    let saved = root.path().join("external-manifest.json");
    fs::rename(out.join("manifest.json"), &saved).unwrap();
    symlink(saved, out.join("manifest.json")).unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
}

#[test]
fn verify_rejects_build_options_without_creating_directories() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("not-published");
    let cache = root.path().join("not-a-cache");
    let out = build(root.path(), &["--display-only"]);
    let before = tree(root.path());
    for directory in [&out, &missing] {
        for options in [
            vec!["--param", "width=12"],
            vec!["--cache-dir", cache.to_str().unwrap()],
        ] {
            let mut args = vec!["verify", directory.to_str().unwrap()];
            args.extend(options);
            let result = cli(&args);
            assert!(!result.status.success());
            let error: Value = serde_json::from_slice(&result.stderr).unwrap();
            assert_eq!(error["code"], "invalid_request", "{error}");
            assert!(!missing.exists());
            assert!(!cache.exists());
            assert_eq!(tree(root.path()), before);
        }
    }
}

#[test]
fn verification_rejects_extra_files_duplicate_json_keys_and_oversized_manifest() {
    let root = tempfile::tempdir().unwrap();
    let out = build(root.path(), &["--display-only"]);
    fs::write(
        out.join("untracked.txt"),
        b"not part of the published generation",
    )
    .unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
    fs::remove_file(out.join("untracked.txt")).unwrap();
    fs::create_dir(out.join("untracked-directory")).unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
    fs::remove_dir(out.join("untracked-directory")).unwrap();
    #[cfg(unix)]
    {
        let external = root.path().join("external-directory");
        fs::create_dir(&external).unwrap();
        std::os::unix::fs::symlink(&external, out.join("untracked-directory")).unwrap();
        rejected(cli(&["verify", out.to_str().unwrap()]));
        fs::remove_file(out.join("untracked-directory")).unwrap();
    }
    let original = manifest(&out);
    // A generic JSON parser silently accepts these duplicates. A recipient must
    // not get a different manifest meaning by choosing another JSON parser.
    let canonical = serde_json::to_string(&original).unwrap();
    let duplicate_top = format!(
        "{{\"artifact_integrity\":{},{}",
        original["artifact_integrity"],
        &canonical[1..]
    );
    let marker = "\"recipe.star\":";
    let position = canonical.find(marker).unwrap();
    let duplicate_file = format!(
        "{}\"recipe.star\":{},{}",
        &canonical[..position],
        original["artifact_integrity"]["files"]["recipe.star"],
        &canonical[position..]
    );
    for malformed in [duplicate_top, duplicate_file, "{\"unfinished\":".to_owned()] {
        fs::write(out.join("manifest.json"), malformed).unwrap();
        rejected(cli(&["verify", out.to_str().unwrap()]));
    }
    // Sparse allocation avoids a large fixture while exercising the published
    // manifest budget before parsing or walking any payload paths.
    let file = fs::File::create(out.join("manifest.json")).unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    rejected(cli(&["verify", out.to_str().unwrap()]));
}
