use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}

fn example(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn nested_rack_exports_hierarchy_shared_assets_and_validated_leaf_paths() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("rack");
    let built = cli(&[
        "build",
        &example("modular_rack.star"),
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&built.stdout).unwrap();
    let assembly = &summary["outputs"]["modular_rack"];
    assert_eq!(assembly["instances"], 17);
    assert_eq!(assembly["definitions"], 4);
    assert_eq!(assembly["assembly_definitions"], 3);
    assert!((assembly["material_volume_mm3"].as_f64().unwrap() - 63648.0).abs() < 1e-7);
    let manifest_path = output.join("modular_rack.assembly.json");
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(
        original["hierarchy"]["row"]["assemblies"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let path = serde_json::json!(["rear", "carriage_b", "guide_right"]);
    let key = serde_json::to_string(&path).unwrap();
    assert_eq!(original["instance_paths"][&key], path);
    let display: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("modular_rack.display.json")).unwrap())
            .unwrap();
    assert_eq!(display["meshes"].as_object().unwrap().len(), 4);
    assert_eq!(display["instance_paths"], original["instance_paths"]);
    let step = fs::read_to_string(output.join("modular_rack.step")).unwrap();
    assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 9);
    assert_eq!(step.matches("FACETED_BREP('").count(), 4);
    let valid = cli(&["validate", manifest_path.to_str().unwrap()]);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    for mutation in 0..3 {
        let mut damaged = original.clone();
        match mutation {
            0 => damaged["instance_paths"][&key] = serde_json::json!(["wrong", "path"]),
            1 => {
                damaged["hierarchy"]["rack"]["assemblies"][0]["transform"]["translation"][0] =
                    serde_json::json!(10)
            }
            _ => damaged["instances"][0]["transform"]["translation"][0] = serde_json::json!(1000),
        }
        fs::write(&manifest_path, serde_json::to_vec(&damaged).unwrap()).unwrap();
        let rejected = cli(&["validate", manifest_path.to_str().unwrap()]);
        assert!(
            !rejected.status.success(),
            "accepted inconsistent nested manifest mutation {mutation}"
        );
    }
}

#[test]
fn starlark_build_writes_expected_geometry_and_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let result = cli(&[
        "build",
        &example("plate.star"),
        "--out-dir",
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let plate = &report["outputs"]["plate"];
    assert_eq!(plate["volume_mm3"], 19_200.0);
    assert_eq!(
        plate["bounds_mm"]["max"],
        serde_json::json!([80.0, 40.0, 6.0])
    );
    assert_eq!(plate["topology"]["vertices"], 8);
    assert_eq!(plate["topology"]["edges"], 12);
    assert_eq!(plate["topology"]["faces"], 6);
    assert_eq!(plate["validation"]["valid"], true);
    let stl = fs::read_to_string(dir.path().join("plate.stl")).unwrap();
    assert_eq!(stl.matches("facet normal").count(), 12);
    let step = fs::read_to_string(dir.path().join("plate.step")).unwrap();
    assert!(step.starts_with("ISO-10303-21;"));
    assert!(step.contains("FACETED_BREP"));
    assert!(dir.path().join("plate.brep.json").is_file());
    assert!(dir.path().join("manifest.json").is_file());
    let compiled = dir.path().join("recipe.json");
    assert!(compiled.is_file());
    assert!(cli(&["check", compiled.to_str().unwrap()]).status.success());
    assert_eq!(report["kernel_build_id"].as_str().unwrap().len(), 64);
    assert_eq!(report["executor_build_id"].as_str().unwrap().len(), 64);
}

#[test]
fn starlark_and_json_examples_have_the_same_content_hashes() {
    let a = cli(&["inspect", &example("plate.star")]);
    let b = cli(&["inspect", &example("plate.json")]);
    assert!(a.status.success() && b.status.success());
    let a: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(a, b);
}

#[test]
fn invalid_geometry_fails_without_writing_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("invalid.star");
    fs::write(&recipe, "p = rectangle(id = 'p', width = -1, depth = 40)\ns = extrude(id = 's', profile = p, height = 6)\noutput(name = 'plate', feature = s)\n").unwrap();
    let output_dir = dir.path().join("artifacts");
    let result = cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        output_dir.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(!output_dir.exists());
    let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("width"));
}

#[test]
fn output_names_cannot_escape_artifact_directory() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("escape.star");
    fs::write(&recipe, "p = rectangle(id = 'p', width = 1, depth = 2)\ns = extrude(id = 's', profile = p, height = 3)\noutput(name = '../escape', feature = s)\n").unwrap();
    let output_dir = dir.path().join("artifacts");
    let result = cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        output_dir.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(!output_dir.exists());
    assert!(!dir.path().join("escape.step").exists());
}

#[test]
fn compilation_produces_rebuildable_graph() {
    let dir = tempfile::tempdir().unwrap();
    let result = cli(&["compile", &example("plate.star")]);
    assert!(result.status.success());
    let recipe = dir.path().join("compiled.json");
    fs::write(&recipe, result.stdout).unwrap();
    let checked = cli(&["check", recipe.to_str().unwrap()]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn filesystem_name_collisions_and_reserved_names_fail_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    for (index, declarations) in [
        "output(name = 'CON', feature = s)",
        "output(name = 'part.', feature = s)",
        "output(name = 'Part', feature = s)\noutput(name = 'part', feature = s)",
    ]
    .iter()
    .enumerate()
    {
        let recipe = dir.path().join(format!("names{index}.star"));
        let source = format!("p = rectangle(id = 'p', width = 1, depth = 2)\ns = extrude(id = 's', profile = p, height = 3)\n{declarations}\n");
        fs::write(&recipe, source).unwrap();
        let output_dir = dir.path().join(format!("artifacts{index}"));
        let result = cli(&[
            "build",
            recipe.to_str().unwrap(),
            "--out-dir",
            output_dir.to_str().unwrap(),
        ]);
        assert!(!result.status.success());
        assert!(!output_dir.exists());
    }
}

#[test]
fn repeated_builds_have_byte_identical_exports() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for dir in [&a, &b] {
        let result = cli(&[
            "build",
            &example("plate.star"),
            "--out-dir",
            dir.path().to_str().unwrap(),
        ]);
        assert!(result.status.success());
    }
    for file in [
        "plate.step",
        "plate.stl",
        "plate.brep.json",
        "manifest.json",
    ] {
        assert_eq!(
            fs::read(a.path().join(file)).unwrap(),
            fs::read(b.path().join(file)).unwrap()
        );
    }
}

const BRACKET: &str = r#"
width = param(name = 'width', default = 80)
p = rectangle(id = 'base_profile', width = width, depth = 40)
base = extrude(id = 'base', profile = p, height = 6)
q = rectangle(id = 'wall_profile', width = 6, depth = 40)
wall = extrude(id = 'wall_raw', profile = q, height = 34)
wall = translate(id = 'wall', body = wall, offset = [0, 0, 6])
joined = union(id = 'joined', left = base, right = wall)
s = rectangle(id = 'slot_profile', width = 10, depth = 10)
slot = extrude(id = 'slot_raw', profile = s, height = 8)
slot = translate(id = 'slot', body = slot, offset = [20, 15, -1])
bracket = cut(id = 'bracket', left = joined, right = slot)
output(name = 'bracket', feature = bracket)
"#;

fn successful_json(result: Output) -> serde_json::Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

const SELECT_RECIPE: &str = r#"
p = rectangle(id = 'profile', width = param(name = 'width', default = 10), depth = 8)
base = extrude(id = 'base', profile = p, height = 3)
moved = translate(id = 'moved', body = base, offset = [20,0,0])
q = rectangle(id = 'cutter_profile', width = 2, depth = 10)
cutter = extrude(id = 'cutter', profile = q, height = 5)
cutter = translate(id = 'positioned_cutter', body = cutter, offset = [4,-1,-1])
split = cut(id = 'split', left = base, right = cutter)
empty = cut(id = 'empty', left = base, right = base)
output(name = 'part', feature = moved)
"#;

fn selection_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("part.star"), SELECT_RECIPE).unwrap();
    directory
}

fn select_in(directory: &Path, target: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .current_dir(directory)
        .args([
            "select",
            "part.star",
            "--from",
            "base",
            "--role",
            "end_cap",
            "--in",
            target,
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn semantic_selection_resolves_translation_without_mutating_source_or_publishing() {
    let directory = selection_fixture();
    let report = successful_json(select_in(directory.path(), "moved", &[]));
    assert_eq!(report["status"], "resolved");
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(report["witnesses"][0]["target_feature"], "moved");
    assert_eq!(
        report["snapshot"]["recipe_source_hash"],
        blake3::hash(SELECT_RECIPE.as_bytes()).to_hex().to_string()
    );
    assert!(report["snapshot"]["target_content_hash"].is_string());
    assert!(report["snapshot"]["anchor_content_hash"].is_string());
    assert_eq!(
        report["snapshot"]["kernel_build_id"],
        cad_kernel::KERNEL_BUILD_ID
    );
    assert!(!report["lineage"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(directory.path().join("part.star")).unwrap(),
        SELECT_RECIPE
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    assert!(!directory.path().join("out").exists());
}

#[test]
fn semantic_selection_requires_explicit_split_cardinality() {
    let directory = selection_fixture();
    let ambiguous = select_in(directory.path(), "split", &[]);
    assert!(!ambiguous.status.success());
    assert!(ambiguous.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&ambiguous.stderr).unwrap();
    assert_eq!(error["code"], "selection_ambiguous");
    assert_eq!(error["selection"]["status"], "ambiguous");
    assert!(error["selection"]["snapshot"]["target_content_hash"].is_string());
    assert_eq!(error["selection"]["witnesses"].as_array().unwrap().len(), 2);
    let all = successful_json(select_in(directory.path(), "split", &["--all"]));
    assert_eq!(all["status"], "resolved");
    assert_eq!(all["witnesses"].as_array().unwrap().len(), 2);
    let exact = successful_json(select_in(directory.path(), "split", &["--count", "2"]));
    assert_eq!(exact["witnesses"], all["witnesses"]);
    assert!(!select_in(directory.path(), "split", &["--count", "0"])
        .status
        .success());
    assert!(
        !select_in(directory.path(), "split", &["--count", "2", "--all"])
            .status
            .success()
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn semantic_selection_reports_deleted_and_unsupported_with_cache_and_params() {
    let directory = selection_fixture();
    let deleted = select_in(directory.path(), "empty", &["--all"]);
    assert!(!deleted.status.success());
    let error: serde_json::Value = serde_json::from_slice(&deleted.stderr).unwrap();
    assert_eq!(error["code"], "selection_deleted");
    assert_eq!(error["selection"]["status"], "deleted");
    assert!(error["selection"]["witnesses"]
        .as_array()
        .unwrap()
        .is_empty());
    let missing = select_in(directory.path(), "missing_feature", &[]);
    assert!(!missing.status.success());
    let error: serde_json::Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(error["code"], "selection_unsupported");
    assert!(error["selection"]["snapshot"]["target_content_hash"].is_null());
    let args = ["--param", "width=12", "--cache-dir", "cache"];
    successful_json(select_in(directory.path(), "moved", &args));
    let cached = successful_json(select_in(directory.path(), "moved", &args));
    assert_eq!(cached["status"], "resolved");
    assert!(cached["stats"]["disk_cache_hits"].as_u64().unwrap() > 0);
    let changed = successful_json(select_in(
        directory.path(),
        "moved",
        &["--param", "width=14"],
    ));
    assert_ne!(
        cached["snapshot"]["target_content_hash"],
        changed["snapshot"]["target_content_hash"]
    );
    assert_ne!(
        cached["snapshot"]["anchor_content_hash"],
        changed["snapshot"]["anchor_content_hash"]
    );
    assert_eq!(
        cached["snapshot"]["recipe_source_hash"],
        changed["snapshot"]["recipe_source_hash"]
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("part.star")).unwrap(),
        SELECT_RECIPE
    );
    assert!(!directory.path().join("out").exists());
}

#[test]
fn parameterized_bracket_builds_valid_export_and_previews() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("bracket.star");
    fs::write(&recipe, BRACKET).unwrap();
    let out = dir.path().join("out");
    let report = successful_json(cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--param",
        "width=100",
        "--out-dir",
        out.to_str().unwrap(),
    ]));
    assert_eq!(report["outputs"]["bracket"]["volume_mm3"], 31560.0);
    assert_eq!(
        report["outputs"]["bracket"]["bounds_mm"]["max"],
        serde_json::json!([100.0, 40.0, 40.0])
    );
    assert!(fs::read_to_string(out.join("bracket.preview.svg"))
        .unwrap()
        .contains("<svg"));
    assert!(fs::read_to_string(out.join("bracket.preview.html"))
        .unwrap()
        .contains("<html"));
    let validated = successful_json(cli(&[
        "validate",
        out.join("bracket.brep.json").to_str().unwrap(),
    ]));
    assert_eq!(validated["validation"]["valid"], true);
    assert_eq!(validated["volume_mm3"], 31560.0);
}

#[test]
fn init_preserves_existing_recipe_and_parameter_errors_are_json() {
    let dir = tempfile::tempdir().unwrap();
    successful_json(cli(&["init", dir.path().to_str().unwrap()]));
    let recipe = dir.path().join("main.star");
    let original = fs::read(&recipe).unwrap();
    assert!(!cli(&["init", dir.path().to_str().unwrap()])
        .status
        .success());
    assert_eq!(fs::read(&recipe).unwrap(), original);
    let report = successful_json(cli(&[
        "inspect",
        recipe.to_str().unwrap(),
        "--param",
        "width=100",
    ]));
    assert_eq!(report["outputs"]["plate"]["volume_mm3"], 24000.0);
    let bad = cli(&["check", recipe.to_str().unwrap(), "--param", "typo=1"]);
    assert!(!bad.status.success());
    let error: serde_json::Value = serde_json::from_slice(&bad.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("typo"));
}

#[test]
fn diff_reports_changed_and_removed_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before.star");
    let after = dir.path().join("after.star");
    let prefix = "p = rectangle(id = 'p', width = 2, depth = 3)\ns = extrude(id = 's', profile = p, height = 4)\n";
    fs::write(
        &before,
        format!(
            "{prefix}output(name = 'part', feature = s)\noutput(name = 'removed', feature = s)\n"
        ),
    )
    .unwrap();
    fs::write(
        &after,
        format!(
            "{}output(name = 'part', feature = s)\n",
            prefix.replace("width = 2", "width = 4")
        ),
    )
    .unwrap();
    let report = successful_json(cli(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]));
    assert_eq!(report["outputs"]["part"]["status"], "changed");
    assert_eq!(report["outputs"]["part"]["volume_delta_mm3"], 24.0);
    assert_eq!(report["outputs"]["removed"]["status"], "removed");
}

#[test]
fn artifact_replacement_removes_stale_outputs_and_failure_preserves_generation() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("part.star");
    let out = dir.path().join("out");
    let prefix = "p = rectangle(id = 'p', width = 2, depth = 3)\ns = extrude(id = 's', profile = p, height = 4)\n";
    fs::write(
        &recipe,
        format!("{prefix}output(name = 'old', feature = s)\n"),
    )
    .unwrap();
    successful_json(cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]));
    fs::write(
        &recipe,
        format!("{prefix}output(name = 'new', feature = s)\n"),
    )
    .unwrap();
    successful_json(cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]));
    assert!(!out.join("old.step").exists());
    let original = fs::read(out.join("manifest.json")).unwrap();
    fs::write(&recipe, format!("{prefix}empty = cut(id = 'empty', left = s, right = s)\noutput(name = 'empty', feature = empty)\n")).unwrap();
    let empty = successful_json(cli(&["inspect", recipe.to_str().unwrap()]));
    assert_eq!(empty["outputs"]["empty"]["volume_mm3"], 0.0);
    assert!(empty["outputs"]["empty"]["bounds_mm"].is_null());
    let failed = cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert!(!failed.status.success());
    assert_eq!(fs::read(out.join("manifest.json")).unwrap(), original);
    assert!(out.join("new.step").exists());
    assert!(!out.join("empty.step").exists());
}

#[test]
fn nonempty_unmanaged_output_directory_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "keep me").unwrap();
    let result = cli(&[
        "build",
        &example("plate.star"),
        "--out-dir",
        dir.path().to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert_eq!(
        fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "keep me"
    );
    assert!(!dir.path().join("plate.step").exists());
}

#[test]
fn corrupted_brep_validation_returns_structured_issues() {
    let dir = tempfile::tempdir().unwrap();
    let mut result =
        cad_kernel::extrude_rectangle(2.0, 3.0, 4.0, cad_kernel::Tolerance::default()).unwrap();
    result.model.edges[0].vertices[0].0 = usize::MAX;
    let path = dir.path().join("corrupt.brep.json");
    fs::write(&path, serde_json::to_vec(&result).unwrap()).unwrap();
    let response = cli(&["validate", path.to_str().unwrap()]);
    assert!(!response.status.success());
    let error: serde_json::Value = serde_json::from_slice(&response.stderr).unwrap();
    assert_eq!(error["code"], "invalid_brep");
    assert_eq!(error["validation"]["valid"], false);
    assert!(!error["validation"]["issues"].as_array().unwrap().is_empty());
}

#[test]
fn comment_only_edit_updates_exact_recipe_and_preview_without_geometry_change() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("source.star");
    let out = dir.path().join("out");
    let source = "# unique_comment_before\r\np = rectangle(id = 'p', width = param(name = 'width', default = 2), depth = 3)\r\ns = extrude(id = 's', profile = p, height = 4)\r\noutput(name = 'part', feature = s)\r\n";
    fs::write(&recipe, source).unwrap();
    let args = [
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--param",
        "width=5",
    ];
    let before = successful_json(cli(&args));
    assert_eq!(
        fs::read(out.join("recipe.star")).unwrap(),
        source.as_bytes()
    );
    assert_eq!(before["recipe_source"]["parameter_overrides"]["width"], 5.0);
    assert_eq!(before["recipe_source"]["name"], "source.star");
    let first_html = fs::read_to_string(out.join("part.preview.html")).unwrap();
    assert!(first_html.contains("unique_comment_before"));
    let embedded_context = |html: &str| -> serde_json::Value {
        let json = html
            .lines()
            .find_map(|line| {
                line.strip_prefix("<script id=\"cad-recipe-data\" type=\"application/json\">")
            })
            .unwrap()
            .trim_end_matches("</script>");
        serde_json::from_str(json).unwrap()
    };
    let first_context = embedded_context(&first_html);
    assert_eq!(
        first_context["content_hash"],
        before["outputs"]["part"]["content_hash"]
    );
    assert_eq!(
        first_context["recipe_context_hash"],
        before["recipe_context_hash"]
    );
    let after_source = source.replace("unique_comment_before", "unique_comment_after");
    fs::write(&recipe, &after_source).unwrap();
    let after = successful_json(cli(&args));
    assert_eq!(
        after["outputs"]["part"]["content_hash"],
        before["outputs"]["part"]["content_hash"]
    );
    assert_ne!(after["recipe_source_hash"], before["recipe_source_hash"]);
    assert_eq!(
        after["recipe_source_hash"],
        blake3::hash(after_source.as_bytes()).to_hex().to_string()
    );
    assert_eq!(
        fs::read(out.join("recipe.star")).unwrap(),
        after_source.as_bytes()
    );
    let second_html = fs::read_to_string(out.join("part.preview.html")).unwrap();
    assert!(second_html.contains("unique_comment_after"));
    assert!(!second_html.contains("unique_comment_before"));
    assert_ne!(first_html, second_html);
    let second_context = embedded_context(&second_html);
    assert_eq!(
        second_context["content_hash"],
        first_context["content_hash"]
    );
    assert_eq!(
        second_context["recipe_context_hash"],
        after["recipe_context_hash"]
    );
    assert_ne!(
        second_context["recipe_context_hash"],
        first_context["recipe_context_hash"]
    );
    let rebuilt = successful_json(cli(&["check", out.join("recipe.json").to_str().unwrap()]));
    assert_eq!(rebuilt["outputs"]["part"]["volume_mm3"], 60.0);
    let default_args = [
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ];
    let default = successful_json(cli(&default_args));
    let mut explicit_args = default_args.to_vec();
    explicit_args.extend(["--param", "width=2"]);
    let explicit = successful_json(cli(&explicit_args));
    assert_eq!(
        default["outputs"]["part"]["content_hash"],
        explicit["outputs"]["part"]["content_hash"]
    );
    assert_eq!(
        default["recipe_source_hash"],
        explicit["recipe_source_hash"]
    );
    assert_ne!(
        default["recipe_context_hash"],
        explicit["recipe_context_hash"]
    );
}

#[test]
fn json_source_is_retained_separately_from_resolved_graph() {
    let dir = tempfile::tempdir().unwrap();
    let original = fs::read_to_string(example("plate.json")).unwrap();
    let source = format!("\n\n{original}\n\n");
    let path = dir.path().join("input.json");
    let out = dir.path().join("out");
    fs::write(&path, &source).unwrap();
    let report = successful_json(cli(&[
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]));
    assert_eq!(report["recipe_source"]["language"], "json");
    assert_eq!(
        fs::read_to_string(out.join("recipe.source.json")).unwrap(),
        source
    );
    assert_ne!(
        fs::read(out.join("recipe.json")).unwrap(),
        source.as_bytes()
    );
    assert!(fs::read_to_string(out.join("plate.preview.html"))
        .unwrap()
        .contains("input.json"));
}

#[test]
fn disjoint_union_exports_two_step_solids_and_persistent_cache_hits() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("pair.star");
    fs::write(&recipe, "p = rectangle(id = 'p', width = 2, depth = 3)\na = extrude(id = 'a', profile = p, height = 4)\nb = translate(id = 'b', body = a, offset = [5,0,0])\npair = union(id = 'pair', left = a, right = b)\noutput(name = 'pair', feature = pair)\n").unwrap();
    let cache = dir.path().join("cache");
    let out = dir.path().join("out");
    let report = successful_json(cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    assert_eq!(report["outputs"]["pair"]["topology"]["solids"], 2);
    assert_eq!(report["outputs"]["pair"]["volume_mm3"], 48.0);
    let step = fs::read_to_string(out.join("pair.step")).unwrap();
    assert_eq!(step.matches("=FACETED_BREP(").count(), 2);
    let cached = successful_json(cli(&[
        "check",
        recipe.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    assert!(cached["stats"]["disk_cache_hits"].as_u64().unwrap() > 0);
}

#[test]
fn assemblies_publish_shared_assets_and_placement_edits_reuse_geometry() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("assembly.star");
    let out = dir.path().join("out");
    let cache = dir.path().join("cache");
    fs::write(&source,r#"gap = param(name = "gap", default = 20)
p = rectangle(id = "p", width = 2, depth = 3)
b = extrude(id = "b", profile = p, height = 4)
d = part(id = "block", body = b)
a = instance(id = "first", part = d)
c = instance(id = "second", part = d, offset = [gap, 0, 0], rotation = [0, 0, 0.3826834323650898, 0.9238795325112867])
s = assembly(id = "pair", instances = [a, c])
assembly_output(name = "pair", assembly = s)
"#).unwrap();
    let built = cli(&[
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&built.stdout).unwrap();
    assert_eq!(first["schema_version"], 2);
    assert_eq!(first["stats"]["computed_solids"], 1);
    assert_eq!(first["outputs"]["pair"]["instances"], 2);
    assert_eq!(first["outputs"]["pair"]["definitions"], 1);
    assert_eq!(first["outputs"]["pair"]["material_volume_mm3"], 48.0);
    let assembly: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("pair.assembly.json")).unwrap()).unwrap();
    assert!(assembly["definitions"]["block"].get("model").is_none());
    let asset = assembly["definitions"]["block"]["geometry_asset"]
        .as_str()
        .unwrap();
    assert!(out.join(asset).is_file());
    let validated = cli(&["validate", out.join("pair.assembly.json").to_str().unwrap()]);
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    assert_eq!(
        fs::read_dir(&out)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("definition-"))
            .count(),
        1
    );
    let display: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("pair.display.json")).unwrap()).unwrap();
    assert_eq!(display["meshes"].as_object().unwrap().len(), 1);
    assert_eq!(display["instances"].as_array().unwrap().len(), 2);
    let step = fs::read_to_string(out.join("pair.step")).unwrap();
    assert_eq!(step.matches("FACETED_BREP('").count(), 1);
    assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 2);
    let check = cli(&[
        "check",
        out.join("recipe.json").to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]);
    assert!(check.status.success());
    let compiled: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(
        compiled["outputs"]["pair"]["content_hash"],
        first["outputs"]["pair"]["content_hash"]
    );
    let moved = cli(&[
        "inspect",
        source.to_str().unwrap(),
        "--param",
        "gap=30",
        "--cache-dir",
        cache.to_str().unwrap(),
    ]);
    assert!(moved.status.success());
    let moved: serde_json::Value = serde_json::from_slice(&moved.stdout).unwrap();
    assert_eq!(moved["stats"]["computed_solids"], 0);
    assert_eq!(moved["stats"]["disk_cache_hits"], 1);
    assert_ne!(
        moved["outputs"]["pair"]["content_hash"],
        first["outputs"]["pair"]["content_hash"]
    );
    assert_eq!(moved["outputs"]["pair"]["material_volume_mm3"], 48.0);
    let bad = cli(&[
        "build",
        source.to_str().unwrap(),
        "--param",
        "gap=1e300",
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert!(!bad.status.success());
    assert_eq!(fs::read_to_string(out.join("pair.step")).unwrap(), step);
}

#[test]
fn assembly_validation_rejects_metadata_tampering_and_preflights_resources() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("parts.star");
    let out = dir.path().join("out");
    fs::write(&source, "p=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\nd=part(id='block',body=b)\ne=part(id='alias',body=b)\ni=instance(id='first',part=d)\nj=instance(id='second',part=e,offset=[10,0,0])\na=assembly(id='pair',instances=[i,j])\nassembly_output(name='pair',assembly=a)\n").unwrap();
    let built = cli(&[
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let path = out.join("pair.assembly.json");
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        original["definitions"]["block"]["geometry_asset"],
        original["definitions"]["alias"]["geometry_asset"]
    );
    // Display data is optional and not certified by exact validation.
    fs::remove_file(out.join("pair.display.json")).unwrap();
    assert!(cli(&["validate", path.to_str().unwrap()]).status.success());
    for (pointer, replacement) in [
        ("/material_volume_mm3", serde_json::json!(-99)),
        ("/bounds_mm/max/0", serde_json::json!(999)),
        (
            "/definitions/block/material_volume_mm3",
            serde_json::json!(-99),
        ),
        (
            "/definitions/alias/local_bounds_mm/min/1",
            serde_json::json!(1),
        ),
        ("/bounds_kind", serde_json::json!("tight")),
        ("/volume_kind", serde_json::json!("union")),
        ("/display_asset", serde_json::json!("../outside.json")),
        ("/instances/0/part", serde_json::json!("missing")),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        let validation = cli(&["validate", path.to_str().unwrap()]);
        assert!(!validation.status.success(), "accepted changed {pointer}");
        let error: serde_json::Value = serde_json::from_slice(&validation.stderr).unwrap();
        assert!(error["error"].is_string());
    }
    // Numeric formatting is not semantic: an integer volume equals its float.
    let mut numeric = original.clone();
    numeric["material_volume_mm3"] = serde_json::json!(48);
    numeric["definitions"]["block"]["material_volume_mm3"] = serde_json::json!(24);
    fs::write(&path, serde_json::to_vec(&numeric).unwrap()).unwrap();
    assert!(cli(&["validate", path.to_str().unwrap()]).status.success());
    // Count failures happen before the deliberately missing referenced file is opened.
    let mut oversized = original.clone();
    oversized["definitions"]["block"]["geometry_asset"] = serde_json::json!("missing.brep.json");
    oversized["instances"] =
        serde_json::Value::Array(vec![original["instances"][0].clone(); 100_001]);
    fs::write(&path, serde_json::to_vec(&oversized).unwrap()).unwrap();
    let result = cli(&["validate", path.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("count limit"));
}

const FACE_ASSERTION_RECIPE: &str = r#"
x = param(name = 'cut_x', default = 8)
w = param(name = 'cut_width', default = 2)
p = rectangle(id = 'profile', width = 10, depth = 8)
base = extrude(id = 'base', profile = p, height = 3)
q = rectangle(id = 'tool_profile', width = w, depth = 10)
tool = extrude(id = 'tool', profile = q, height = 5)
tool = translate(id = 'placed_tool', body = tool, offset = [x,-1,-1])
result = cut(id = 'cut', left = base, right = tool)
assert_faces(id = 'top_survives', from_feature = base, role = 'end_cap', in_feature = result)
output(name = 'part', feature = result)
"#;

fn artifact_bytes(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = std::collections::BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn face_assertions_publish_evidence_and_failed_parameter_edits_preserve_all_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let recipe = directory.path().join("assertions.star");
    let out = directory.path().join("out");
    let cache = directory.path().join("cache");
    fs::write(&recipe, FACE_ASSERTION_RECIPE).unwrap();
    let args = [
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ];
    let first = successful_json(cli(&args));
    let resolution = &first["face_assertions"]["top_survives"];
    assert_eq!(resolution["status"], "resolved");
    assert_eq!(
        resolution["selector"],
        serde_json::json!({
            "anchor_feature":"base", "role":"end_cap", "target_feature":"cut",
            "cardinality":{"kind":"exactly", "count":1}
        })
    );
    assert_eq!(resolution["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(resolution["witnesses"][0]["target_feature"], "cut");
    assert_eq!(
        resolution["witnesses"][0]["anchor_faces"],
        serde_json::json!([1])
    );
    assert!(resolution["witnesses"][0]["target_face"].is_u64());
    assert!(!resolution["lineage"].as_array().unwrap().is_empty());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["face_assertions"], first["face_assertions"]);
    let compiled_check = successful_json(cli(&[
        "check",
        out.join("recipe.json").to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    assert_eq!(compiled_check["face_assertions"], first["face_assertions"]);
    assert!(compiled_check["stats"]["disk_cache_hits"].as_u64().unwrap() > 0);
    let original = artifact_bytes(&out);
    assert!(original.contains_key(Path::new("manifest.json")));
    assert!(original.contains_key(Path::new("part.step")));
    assert!(original.contains_key(Path::new("part.preview.html")));
    for (parameters, status, witnesses) in [
        (vec!["--param", "cut_x=4"], "ambiguous", 2),
        (
            vec!["--param", "cut_x=-1", "--param", "cut_width=12"],
            "deleted",
            0,
        ),
    ] {
        // Repeating also checks that a cached failed geometry result never
        // bypasses re-evaluation of the declarative face assertion.
        for _ in 0..2 {
            let mut changed_args = args.to_vec();
            changed_args.extend(&parameters);
            let changed = cli(&changed_args);
            assert!(!changed.status.success());
            assert!(changed.stdout.is_empty());
            let error: serde_json::Value = serde_json::from_slice(&changed.stderr).unwrap();
            assert_eq!(error["code"], "face_assertion_failed", "{error}");
            assert_eq!(error["assertion_id"], "top_survives");
            assert_eq!(
                error["snapshot"]["kernel_build_id"],
                first["kernel_build_id"]
            );
            assert_eq!(
                error["snapshot"]["executor_build_id"],
                first["executor_build_id"]
            );
            assert_eq!(
                error["snapshot"]["anchor_content_hash"]
                    .as_str()
                    .unwrap()
                    .len(),
                64
            );
            assert_eq!(
                error["snapshot"]["target_content_hash"]
                    .as_str()
                    .unwrap()
                    .len(),
                64
            );
            assert_eq!(error["selection"]["status"], status);
            assert_eq!(error["selection"]["selector"], resolution["selector"]);
            assert_eq!(
                error["selection"]["witnesses"].as_array().unwrap().len(),
                witnesses
            );
            assert_eq!(
                artifact_bytes(&out),
                original,
                "failed {status} assertion modified artifacts"
            );
        }
    }
    assert_eq!(fs::read_to_string(recipe).unwrap(), FACE_ASSERTION_RECIPE);
}

#[test]
fn face_assertions_accept_explicit_split_counts_and_all_survivors() {
    let directory = tempfile::tempdir().unwrap();
    for (index, cardinality) in ["count = 2", "all = True"].into_iter().enumerate() {
        let recipe = directory.path().join(format!("split{index}.star"));
        let out = directory.path().join(format!("out{index}"));
        let source = FACE_ASSERTION_RECIPE.replace(
            "in_feature = result)",
            &format!("in_feature = result, {cardinality})"),
        );
        fs::write(&recipe, source).unwrap();
        let built = successful_json(cli(&[
            "build",
            recipe.to_str().unwrap(),
            "--param",
            "cut_x=4",
            "--out-dir",
            out.to_str().unwrap(),
        ]));
        let resolution = &built["face_assertions"]["top_survives"];
        assert_eq!(resolution["status"], "resolved");
        assert_eq!(resolution["witnesses"].as_array().unwrap().len(), 2);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["face_assertions"], built["face_assertions"]);
    }
}

#[test]
fn face_assertion_only_edits_are_visible_in_diff_without_geometry_changes() {
    let directory = tempfile::tempdir().unwrap();
    let before = directory.path().join("before.star");
    let after = directory.path().join("after.star");
    fs::write(&before, FACE_ASSERTION_RECIPE).unwrap();
    for (source, expected) in [
        (
            FACE_ASSERTION_RECIPE
                .replace("in_feature = result)", "in_feature = result, all = True)"),
            "changed",
        ),
        (
            FACE_ASSERTION_RECIPE.replace("top_survives", "top_surface"),
            "removed",
        ),
    ] {
        fs::write(&after, source).unwrap();
        let report = successful_json(cli(&[
            "diff",
            before.to_str().unwrap(),
            after.to_str().unwrap(),
        ]));
        assert_eq!(report["outputs"]["part"]["status"], "unchanged");
        let assertion = &report["face_assertions"]["top_survives"];
        assert_eq!(assertion["status"], expected, "{report}");
        assert_eq!(assertion["before"]["status"], "resolved");
        if expected == "changed" {
            assert_eq!(
                assertion["before"]["selector"]["cardinality"]["kind"],
                "exactly"
            );
            assert_eq!(assertion["after"]["selector"]["cardinality"]["kind"], "all");
            assert_eq!(
                assertion["before"]["witnesses"],
                assertion["after"]["witnesses"]
            );
        } else {
            assert!(assertion["after"].is_null());
            let renamed = &report["face_assertions"]["top_surface"];
            assert_eq!(renamed["status"], "added");
            assert!(renamed["before"].is_null());
            assert_eq!(renamed["after"], assertion["before"]);
        }
    }
}

#[test]
fn nested_rack_bom_reports_quantities_mass_and_published_metadata_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let source = example("modular_rack.star");
    let built = successful_json(cli(&["build", &source, "--out-dir", out.to_str().unwrap()]));
    let bom = successful_json(cli(&["bom", &source, "--assembly", "modular_rack"]));
    assert_eq!(bom["schema_version"], 1);
    assert_eq!(bom["volume_units"], "mm3");
    assert_eq!(bom["mass_units"], "kg");
    assert_eq!(bom["total_occurrences"], 17);
    assert!((bom["total_mass_kg"].as_f64().unwrap() - 0.2058336).abs() < 1e-12);
    assert_eq!(bom["missing_material_occurrences"], 0);
    let items = bom["items"].as_array().unwrap();
    let quantities: std::collections::BTreeMap<_, _> = items
        .iter()
        .map(|item| {
            (
                item["part_id"].as_str().unwrap(),
                item["quantity"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        quantities,
        std::collections::BTreeMap::from([
            ("guide", 8),
            ("plate", 4),
            ("rack_base", 1),
            ("slider", 4)
        ])
    );
    assert_eq!(
        bom["items"],
        built["outputs"]["modular_rack"]["bom"]["items"]
    );
    let published: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("modular_rack.bom.json")).unwrap()).unwrap();
    assert_eq!(published["items"], bom["items"]);
    assert_eq!(published["content_hash"], bom["content_hash"]);
    let csv = fs::read_to_string(out.join("modular_rack.bom.csv")).unwrap();
    assert_eq!(csv.lines().count(), 5);
    assert!(csv.contains("guide,RACK-GUIDE,A,Reversible guide,Demo steel,7800,8,"));
    let path = out.join("modular_rack.assembly.json");
    let mut native: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(native["hierarchy"].as_object().unwrap().len(), 3);
    assert_eq!(native["instances"].as_array().unwrap().len(), 17);
    assert_eq!(
        native["definitions"]["guide"]["metadata"]["material"]["density_kg_per_m3"],
        7800.0
    );
    assert!(cli(&["validate", path.to_str().unwrap()]).status.success());
    native["definitions"]["guide"]["metadata"]["revision"] = "tampered".into();
    fs::write(&path, serde_json::to_vec(&native).unwrap()).unwrap();
    let result = cli(&["validate", path.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(
        serde_json::from_slice::<serde_json::Value>(&result.stderr).unwrap()["error"].is_string()
    );
    let unknown = cli(&["bom", &source, "--assembly", "rack"]); // Root ID is not an output alias.
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown assembly output"));
}

#[test]
fn bom_metadata_only_edits_reuse_geometry_and_diff_exposes_mass_change() {
    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before.star");
    let after = dir.path().join("after.star");
    let cache = dir.path().join("cache");
    let source = fs::read_to_string(example("modular_rack.star")).unwrap();
    fs::write(&before, &source).unwrap();
    fs::write(
        &after,
        source.replace("density_kg_per_m3 = 7800", "density_kg_per_m3 = 3900"),
    )
    .unwrap();
    let first = successful_json(cli(&[
        "inspect",
        before.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    let changed = successful_json(cli(&[
        "inspect",
        after.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    assert_eq!(changed["stats"]["computed_solids"], 0);
    assert_eq!(first["feature_hashes"], changed["feature_hashes"]);
    assert_eq!(
        first["outputs"]["modular_rack"]["volume_mm3"],
        changed["outputs"]["modular_rack"]["volume_mm3"]
    );
    assert_ne!(
        first["outputs"]["modular_rack"]["content_hash"],
        changed["outputs"]["modular_rack"]["content_hash"]
    );
    assert!(
        (changed["outputs"]["modular_rack"]["bom"]["total_mass_kg"]
            .as_f64()
            .unwrap()
            - 0.1758816)
            .abs()
            < 1e-12
    );
    let diff = successful_json(cli(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    let change = &diff["outputs"]["modular_rack"];
    assert_eq!(change["status"], "changed");
    assert_eq!(change["volume_delta_mm3"], 0.0);
    assert_eq!(
        change["before"]["bom"]["items"],
        first["outputs"]["modular_rack"]["bom"]["items"]
    );
    assert_eq!(
        change["after"]["bom"]["items"],
        changed["outputs"]["modular_rack"]["bom"]["items"]
    );
}

#[test]
fn bom_missing_material_reports_unknown_total_and_known_partial_mass() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("partial.star");
    let source = fs::read_to_string(example("modular_rack.star")).unwrap();
    fs::write(
        &recipe,
        source.replace(
            ", material = \"Demo polymer\", density_kg_per_m3 = 1200",
            "",
        ),
    )
    .unwrap();
    let report = successful_json(cli(&[
        "bom",
        recipe.to_str().unwrap(),
        "--assembly",
        "modular_rack",
    ]));
    assert!(report["total_mass_kg"].is_null());
    assert_eq!(report["missing_material_occurrences"], 4);
    assert!((report["known_mass_kg"].as_f64().unwrap() - 0.2016864).abs() < 1e-12);
    let slider = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["part_id"] == "slider")
        .unwrap();
    assert_eq!(slider["quantity"], 4);
    assert!(slider["unit_mass_kg"].is_null());
    assert!(slider["total_mass_kg"].is_null());
}

#[test]
fn datum_attachments_round_trip_and_failed_reference_preserves_generation() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("pair.star");
    let out = dir.path().join("out");
    let recipe = fs::read_to_string(example("datum_pair.star")).unwrap();
    fs::write(&source, &recipe).unwrap();
    let build = || {
        cli(&[
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ])
    };
    let result = build();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let native_path = out.join("datum_pair.assembly.json");
    let bytes = fs::read(&native_path).unwrap();
    let native: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        native["definitions"]["base"]["datums"]["seat"]["translation"],
        serde_json::json!([5.0, 5.0, 2.0])
    );
    let upper = native["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "upper")
        .unwrap();
    for (actual, expected) in upper["transform"]["translation"]
        .as_array()
        .unwrap()
        .iter()
        .zip([6.0, 3.0, 2.0])
    {
        assert!((actual.as_f64().unwrap() - expected).abs() < 1e-10);
    }
    let intent = &native["hierarchy"]["pair"]["instances"][0];
    assert_eq!(intent["attachment"]["target_datum"], "seat");
    assert_eq!(
        intent["transform"]["translation"],
        serde_json::json!([0.0, 0.0, 0.0])
    );
    let valid = cli(&["validate", native_path.to_str().unwrap()]);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    // A changed datum cannot be smuggled into an otherwise valid published assembly.
    let mut damaged = native.clone();
    damaged["definitions"]["base"]["datums"]["seat"]["translation"][0] = 7.0.into();
    fs::write(&native_path, serde_json::to_vec(&damaged).unwrap()).unwrap();
    assert!(!cli(&["validate", native_path.to_str().unwrap()])
        .status
        .success());
    fs::write(&native_path, &bytes).unwrap();
    let generation = fs::read(out.join("manifest.json")).unwrap();
    fs::write(
        &source,
        recipe.replace("target_datum = \"seat\"", "target_datum = \"missing\""),
    )
    .unwrap();
    assert!(!build().status.success());
    assert_eq!(fs::read(out.join("manifest.json")).unwrap(), generation);
    assert_eq!(fs::read(&native_path).unwrap(), bytes);
}

#[test]
fn gantry_height_edit_moves_attachment_chain_and_rebuilds_one_definition() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache");
    let out = dir.path().join("out");
    let source = example("datum_gantry.star");
    successful_json(cli(&[
        "inspect",
        &source,
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    let changed = successful_json(cli(&[
        "build",
        &source,
        "--param",
        "post_height=55",
        "--cache-dir",
        cache.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]));
    assert_eq!(changed["stats"]["computed_solids"], 1);
    assert_eq!(changed["outputs"]["gantry"]["volume_mm3"], 83240.0);
    assert_eq!(
        changed["outputs"]["inspection_line"]["volume_mm3"],
        166480.0
    );
    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("inspection_line.assembly.json")).unwrap())
            .unwrap();
    for (leaf, z) in [
        ("bridge", 61.0),
        ("tool", 65.0),
        ("left_clamp", 69.0),
        ("right_clamp", 69.0),
    ] {
        for station in ["station_a", "station_b"] {
            let id = serde_json::to_string(&[station, leaf]).unwrap();
            let occurrence = native["instances"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["id"] == id)
                .unwrap();
            assert!((occurrence["transform"]["translation"][2].as_f64().unwrap() - z).abs() < 1e-9);
        }
    }
}

#[test]
fn display_only_publication_keeps_shared_meshes_and_removes_stale_exports() {
    let directory = tempfile::tempdir().unwrap();
    let recipe = directory.path().join("many.star");
    let out = directory.path().join("out");
    let source = "n=param(name='n',default=2)\np=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\nd=part(id='block',body=b)\nitems=[instance(id='i'+str(i),part=d,offset=[i*4,0,0]) for i in range(int(n))]\na=assembly(id='many',instances=items)\nassembly_output(name='many',assembly=a)\n";
    fs::write(&recipe, source).unwrap();
    let build = |extra: &[&str]| {
        let mut args = vec![
            "build",
            recipe.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ];
        args.extend(extra);
        cli(&args)
    };
    let full = build(&[]);
    assert!(
        full.status.success(),
        "{}",
        String::from_utf8_lossy(&full.stderr)
    );
    let full_manifest: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    assert_eq!(full_manifest["generation_mode"], "full");
    assert!(out.join("many.step").exists());
    assert!(fs::read_to_string(out.join("many.gpu.html"))
        .unwrap()
        .contains("\"portable_preview_available\":true"));
    let shown = build(&["--display-only", "--param", "n=10001"]);
    assert!(
        shown.status.success(),
        "{}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(summary["generation_mode"], "display_only");
    assert_eq!(summary["outputs"]["many"]["instances"], 10001);
    let available = summary["outputs"]["many"]["available_files"]
        .as_array()
        .unwrap();
    for name in [
        "many.display.json",
        "many.gpu.html",
        "many.stream.json",
        "many.stream.html",
    ] {
        assert!(available.contains(&serde_json::json!(name)));
    }
    assert_eq!(available.len(), 5); // four entrypoints plus one immutable shared mesh

    let actual: std::collections::BTreeSet<_> = fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    let wanted: std::collections::BTreeSet<_> = summary["artifact_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, wanted);
    assert!(!out.join("many.step").exists());
    assert!(!out.join("many.preview.html").exists());
    assert!(fs::read_to_string(out.join("many.gpu.html"))
        .unwrap()
        .contains("\"portable_preview_available\":false"));
    assert!(!out.join("many.preview.svg").exists());
    assert!(!actual.iter().any(|name| name.ends_with(".brep.json")));
    let scene: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("many.display.json")).unwrap()).unwrap();
    assert_eq!(scene["instances"].as_array().unwrap().len(), 10001);
    assert_eq!(scene["meshes"].as_object().unwrap().len(), 1);
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    fs::write(&recipe, "this is invalid !").unwrap();
    assert!(!build(&["--display-only"]).status.success());
    assert_eq!(fs::read(out.join("manifest.json")).unwrap(), manifest);
    assert_eq!(fs::read_to_string(out.join("recipe.star")).unwrap(), source);
}

#[test]
fn display_only_plain_body_publishes_cpu_html_without_engineering_exports() {
    let directory = tempfile::tempdir().unwrap();
    let out = directory.path().join("out");
    let built = cli(&[
        "build",
        &example("plate.star"),
        "--display-only",
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&built.stdout).unwrap();
    for (name, output) in summary["outputs"].as_object().unwrap() {
        assert_eq!(
            output["available_files"],
            serde_json::json!([format!("{name}.preview.html")])
        );
        assert!(!out.join(format!("{name}.step")).exists());
    }
}

#[test]
fn streamed_outputs_deduplicate_payloads_but_each_lists_shared_dependencies() {
    let directory = tempfile::tempdir().unwrap();
    let recipe = directory.path().join("shared.star");
    let output = directory.path().join("out");
    fs::write(&recipe, "p=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\nd=part(id='part',body=b)\ni=instance(id='leaf',part=d)\na=assembly(id='a',instances=[i])\nassembly_output(name='first',assembly=a)\nassembly_output(name='second',assembly=a)\n").unwrap();
    let built = cli(&[
        "build",
        recipe.to_str().unwrap(),
        "--display-only",
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let manifest: serde_json::Value = serde_json::from_slice(&built.stdout).unwrap();
    let meshes: Vec<_> = fs::read_dir(&output)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("mesh-"))
        .collect();
    assert_eq!(meshes.len(), 1);
    for name in ["first", "second"] {
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join(format!("{name}.stream.json"))).unwrap())
                .unwrap();
        assert_eq!(metadata["schema_version"], 2);
        assert_eq!(metadata["meshes"], serde_json::json!({}));
        let descriptors = metadata["mesh_assets"].as_object().unwrap();
        assert_eq!(descriptors.len(), 1);
        let descriptor = descriptors.values().next().unwrap();
        assert_eq!(descriptor["url"], meshes[0]);
        assert_eq!(
            descriptor["byte_length"],
            fs::metadata(output.join(&meshes[0])).unwrap().len()
        );
        assert!(manifest["outputs"][name]["available_files"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(meshes[0])));
        let html = fs::read_to_string(output.join(format!("{name}.stream.html"))).unwrap();
        let scene = html
            .split("<script id=\"cad-scene-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(scene).unwrap(),
            metadata
        );
    }
}
