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
fn property_assertions_publish_pinned_evidence_and_fail_before_any_artifact_change() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("balanced.star");
    let out = directory.path().join("out");
    let cache = directory.path().join("cache");
    let recipe = r#"
gap=param(name='gap',default=20)
density=param(name='density',default=3000)
b=extrude(id='block_body',profile=rectangle(id='profile',width=10,depth=10),height=10)
p=part(id='light',body=b,material='Light material',density_kg_per_m3=1000)
def counterweight():
    if density > 0:
        return part(id='heavy',body=b,material='Counterweight material',density_kg_per_m3=density)
    return part(id='heavy',body=b)
q=counterweight()
a=assembly(id='assembly',instances=[instance(id='left',part=p),instance(id='right',part=q,offset=[gap,0,0])])
output(name='block',feature=b)
assembly_output(name='balanced',assembly=a)
assert_property(id='volume',output='block',property='volume_mm3',min=999.999,max=1000.001)
assert_property(id='balance',output='balanced',property='mass_centroid_x_mm',min=0,max=21)
assert_property(id='mass',output='balanced',property='mass_kg',min=0.003999,max=0.004001)
assert_property(id='inertia',output='balanced',property='mass_inertia_zz_kg_mm2',min=0.36,max=1)
"#;
    fs::write(&source, recipe).unwrap();
    let args = [
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
        "--output",
        "block",
    ];
    let initial = successful_json(cli(&args));
    assert_eq!(
        initial["outputs"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["block"]
    );
    assert!(!out.join("balanced.step").exists());
    let balance = &initial["property_assertions"]["balance"];
    assert_eq!(balance["status"], "passed");
    assert!((balance["actual"].as_f64().unwrap() - 20.).abs() < 1e-10);
    assert_eq!(balance["property"], "mass_centroid_x_mm");
    assert_eq!(balance["max"], 21.);
    assert!(balance["target_content_hash"].as_str().unwrap().len() == 64);
    assert!(
        (initial["property_assertions"]["inertia"]["actual"]
            .as_f64()
            .unwrap()
            - 11. / 30.)
            .abs()
            < 1e-12
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["property_assertions"],
        initial["property_assertions"]
    );
    let embedded_context = |path: &std::path::Path| -> serde_json::Value {
        let html = fs::read_to_string(path).unwrap();
        let json = html
            .split("<script id=\"cad-recipe-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        serde_json::from_str(json).unwrap()
    };
    let context = embedded_context(&out.join("block.preview.html"));
    assert_eq!(
        context["property_assertions"],
        manifest["property_assertions"]
    );
    assert_eq!(
        context["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(context["output_name"], "block");
    assert_ne!(
        context["property_assertions"]["balance"]["target_content_hash"],
        context["content_hash"]
    );
    let checked = successful_json(cli(&[
        "check",
        out.join("recipe.json").to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    assert_eq!(
        checked["property_assertions"],
        initial["property_assertions"]
    );
    assert_eq!(checked["stats"]["computed_solids"], 0);
    let measured = successful_json(cli(&[
        "properties",
        source.to_str().unwrap(),
        "--output",
        "block",
    ]));
    assert_eq!(
        measured["property_assertions"],
        initial["property_assertions"]
    );
    let generation = artifact_bytes(&out);
    for (parameter, status, expected) in [
        ("gap=22", "out_of_range", Some(21.5)),
        ("density=-1", "unavailable", None),
    ] {
        for _ in 0..2 {
            let mut changed = args.to_vec();
            changed.extend(["--param", parameter]);
            let failed = cli(&changed);
            assert!(!failed.status.success());
            let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
            assert_eq!(error["code"], "property_assertion_failed", "{error}");
            assert_eq!(error["assertion_id"], "balance");
            assert_eq!(error["assertion"]["status"], status);
            if let Some(value) = expected {
                assert!((error["assertion"]["actual"].as_f64().unwrap() - value).abs() < 1e-10);
            } else {
                assert!(error["assertion"]["actual"].is_null());
            }
            assert_eq!(
                error["snapshot"]["target_content_hash"],
                error["assertion"]["target_content_hash"]
            );
            assert_eq!(
                error["snapshot"]["executor_build_id"],
                initial["executor_build_id"]
            );
            assert_ne!(
                error["assertion"]["target_content_hash"],
                balance["target_content_hash"]
            );
            assert_eq!(artifact_bytes(&out), generation);
        }
    }
    let changed = directory.path().join("changed.star");
    fs::write(&changed, recipe.replace("max=21)", "max=22)")).unwrap();
    let difference = successful_json(cli(&[
        "diff",
        source.to_str().unwrap(),
        changed.to_str().unwrap(),
    ]));
    assert_eq!(difference["outputs"]["balanced"]["status"], "unchanged");
    assert_eq!(
        difference["property_assertions"]["balance"]["status"],
        "changed"
    );
    assert_eq!(
        difference["property_assertions"]["balance"]["before"]["actual"],
        difference["property_assertions"]["balance"]["after"]["actual"]
    );
    fs::write(&source, recipe.replace("max=21)", "max=22)")).unwrap();
    let rebuilt = successful_json(cli(&args));
    assert_eq!(
        rebuilt["outputs"]["block"]["content_hash"],
        initial["outputs"]["block"]["content_hash"]
    );
    assert_ne!(
        rebuilt["recipe_context_hash"],
        initial["recipe_context_hash"]
    );
    let refreshed = embedded_context(&out.join("block.preview.html"));
    assert_eq!(refreshed["property_assertions"]["balance"]["max"], 22.);
    assert_eq!(
        refreshed["property_assertions"],
        rebuilt["property_assertions"]
    );

    let all = successful_json(cli(&args[..args.len() - 2]));
    for suffix in ["preview.html", "gpu.html", "stream.html"] {
        let context = embedded_context(&out.join(format!("balanced.{suffix}")));
        assert_eq!(context["property_assertions"], all["property_assertions"]);
        assert_eq!(context["recipe_context_hash"], all["recipe_context_hash"]);
        assert_eq!(
            context["content_hash"],
            context["property_assertions"]["balance"]["target_content_hash"]
        );
    }
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
fn descendant_attachment_publishes_resolved_frames_and_bad_paths_preserve_all_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("nested.star");
    let out = dir.path().join("out");
    let recipe = r#"
b = extrude(id='b',profile=rectangle(id='p',width=2,depth=3),height=4)
p = part(id='part',body=b,datums={'foot':datum(offset=[1,0,0]),'seat':datum(offset=[0,2,0])})
m = assembly(id='module',instances=[
    instance(id='fixed',part=p,offset=[3,0,0]),
    instance(id='target/leaf',part=p,attach_to='fixed',datum='foot',target_datum='seat',offset=[0,0,4]),
])
a = assembly(id='root',instances=[
    instance(id='probe',part=p,attach_to='holder',target_path=['target/leaf'],datum='foot',target_datum='seat',offset=[0,0,3]),
    assembly_instance(id='holder',assembly=m,offset=[10,20,30],rotation=[0,0,0.7071067811865476,0.7071067811865476]),
    assembly_instance(id='spare',assembly=m,offset=[50,0,0]),
])
assembly_output(name='nested',assembly=a)
"#;
    fs::write(&source, recipe).unwrap();
    let build = || {
        cli(&[
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ])
    };
    let built = build();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let manifest_path = out.join("nested.assembly.json");
    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let probe = native["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "[\"probe\"]")
        .unwrap();
    // Independent rigid-frame arithmetic: target resolves to [2,2,4] inside
    // the module; probe resolves to [1,4,7], then holder rotates +90 about Z.
    for (actual, expected) in probe["transform"]["translation"]
        .as_array()
        .unwrap()
        .iter()
        .zip([6., 21., 37.])
    {
        assert!((actual.as_f64().unwrap() - expected).abs() < 1e-10);
    }
    assert_eq!(
        native["hierarchy"]["root"]["instances"][0]["attachment"]["target_path"],
        serde_json::json!(["target/leaf"])
    );
    let valid = cli(&["validate", manifest_path.to_str().unwrap()]);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    // Record every artifact, including captured recipe, shared assets and STEP.
    fn snapshot(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        let mut result = std::collections::BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    result.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        result
    }
    let generation = snapshot(&out);
    for replacement in [
        "['missing']",
        "['target','leaf']",
        "['target/leaf','extra']",
        "[]",
    ] {
        fs::write(&source, recipe.replace("['target/leaf']", replacement)).unwrap();
        let failed = build();
        assert!(
            !failed.status.success(),
            "accepted invalid path {replacement}"
        );
        let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "invalid_graph", "{error}");
        assert_eq!(snapshot(&out), generation);
    }
}

#[test]
fn whole_module_attachment_roundtrips_and_invalid_sources_preserve_generation() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mounted.star");
    let out = dir.path().join("out");
    let recipe = r#"
q=0.7071067811865476
b=extrude(id='b',profile=rectangle(id='p',width=2,depth=3),height=4)
p=part(id='part',body=b,datums={'foot':datum(offset=[1,0,0]),'seat':datum(offset=[0,2,0])})
core=assembly(id='core',instances=[instance(id='leaf',part=p,offset=[3,4,5],rotation=[0,q,0,q])])
module=assembly(id='module',instances=[assembly_instance(id='nested',assembly=core)])
root=assembly(id='root',instances=[
    instance(id='base',part=p,offset=[10,20,30],rotation=[0,0,q,q]),
    assembly_instance(id='mounted',assembly=module,attach_to='base',source_path=['nested','leaf'],datum='foot',target_datum='seat',offset=[0,0,7]),
    assembly_instance(id='spare',assembly=module,attach_to='base',source_path=['nested','leaf'],datum='foot',target_datum='seat',offset=[30,0,7]),
    instance(id='sensor',part=p,attach_to='mounted',target_path=['nested','leaf'],datum='foot',target_datum='seat'),
])
assembly_output(name='mounted',assembly=root)
"#;
    fs::write(&source, recipe).unwrap();
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
    let native_path = out.join("mounted.assembly.json");
    let bytes = fs::read(&native_path).unwrap();
    let native: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(native["instances"].as_array().unwrap().len(), 4);
    assert_eq!(native["definitions"].as_object().unwrap().len(), 1);
    for (path, expected) in [
        (
            serde_json::json!(["mounted", "nested", "leaf"]),
            [8., 19., 37.],
        ),
        (
            serde_json::json!(["spare", "nested", "leaf"]),
            [8., 49., 37.],
        ),
        (serde_json::json!(["sensor"]), [6., 18., 37.]),
    ] {
        let id = serde_json::to_string(&path).unwrap();
        let actual = native["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .unwrap();
        for (a, b) in actual["transform"]["translation"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected)
        {
            assert!((a.as_f64().unwrap() - b).abs() < 1e-10);
        }
    }
    let intent = &native["hierarchy"]["root"]["assemblies"][0];
    assert_eq!(
        intent["attachment"]["source_path"],
        serde_json::json!(["nested", "leaf"])
    );
    assert_eq!(
        intent["transform"]["translation"],
        serde_json::json!([0., 0., 7.])
    );
    assert!(cli(&["validate", native_path.to_str().unwrap()])
        .status
        .success());
    // Source intent is required to reproduce the derived positions; removing
    // an attachment from an otherwise unchanged manifest must fail validation.
    let mut damaged = native.clone();
    damaged["hierarchy"]["root"]["assemblies"][0]
        .as_object_mut()
        .unwrap()
        .remove("attachment");
    fs::write(&native_path, serde_json::to_vec(&damaged).unwrap()).unwrap();
    assert!(!cli(&["validate", native_path.to_str().unwrap()])
        .status
        .success());
    fs::write(&native_path, &bytes).unwrap();
    let generation = fs::read(out.join("manifest.json")).unwrap();
    for replacement in [
        "[]",
        "['missing']",
        "['nested']",
        "['nested','leaf','extra']",
    ] {
        fs::write(&source, recipe.replace("['nested','leaf']", replacement)).unwrap();
        assert!(
            !build().status.success(),
            "accepted invalid source path {replacement}"
        );
        assert_eq!(fs::read(&native_path).unwrap(), bytes);
        assert_eq!(fs::read(out.join("manifest.json")).unwrap(), generation);
    }
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
        "many.metadata.json",
    ] {
        assert!(available.contains(&serde_json::json!(name)));
    }
    let page_count = available
        .iter()
        .filter(|name| name.as_str().unwrap().starts_with("metadata-"))
        .count();
    assert!(page_count > 1); // Large assemblies must split into independently loadable pages.
    assert_eq!(available.len(), 6 + page_count); // Five entrypoints, shared mesh, metadata pages.

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
    let pages: Vec<_> = fs::read_dir(&output)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("metadata-"))
        .collect();
    assert_eq!(pages.len(), 1);
    for name in ["first", "second"] {
        let index_name = format!("{name}.metadata.json");
        let index: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join(&index_name)).unwrap()).unwrap();
        assert_eq!(index["kind"], "assembly_metadata_index");
        assert_eq!(index["total_occurrences"], 1);
        assert_eq!(
            index["recipe_context_hash"],
            manifest["recipe_context_hash"]
        );
        assert_eq!(index["root"]["url"], pages[0]);
        assert_eq!(
            index["root"]["byte_length"],
            fs::metadata(output.join(&pages[0])).unwrap().len()
        );
        for dependency in [&index_name, &pages[0]] {
            assert!(manifest["outputs"][name]["available_files"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(dependency)));
        }
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
    let verified = cli(&["verify", output.to_str().unwrap()]);
    assert!(verified.status.success());
    fs::write(output.join(&pages[0]), "{}").unwrap();
    let corrupted = cli(&["verify", output.to_str().unwrap()]);
    assert!(
        !corrupted.status.success(),
        "corrupt metadata must invalidate generation"
    );
}

fn near(value: &serde_json::Value, expected: f64) {
    let actual = value.as_f64().unwrap();
    assert!(
        (actual - expected).abs() <= 1e-11 * expected.abs().max(1.),
        "{actual} != {expected}"
    );
}

#[test]
fn body_properties_are_analytic_parameterized_and_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("box.star");
    let source="w=param(name='width',default=2)\np=rectangle(id='p',width=w,depth=3)\nb=extrude(id='b',profile=p,height=4)\noutput(name='box',feature=b)\n";
    fs::write(&recipe, source).unwrap();
    let result = cli(&[
        "properties",
        recipe.to_str().unwrap(),
        "--output",
        "box",
        "--density-kg-per-m3",
        "8000",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["kind"], "body");
    assert_eq!(report["coordinate_frame"], "output_model_frame");
    assert_eq!(report["units"]["central_volume_inertia"], "mm5");
    assert_eq!(report["units"]["central_mass_inertia"], "kg*mm2");
    near(&report["volume"], 24.);
    assert_eq!(report["volume_centroid"], serde_json::json!([1., 1.5, 2.]));
    for (axis, expected) in [50., 40., 26.].into_iter().enumerate() {
        near(&report["central_volume_inertia"][axis][axis], expected);
        near(&report["central_mass_inertia"][axis][axis], expected * 8e-6);
    }
    near(&report["mass_kg"], 0.000192);
    let changed = cli(&[
        "properties",
        recipe.to_str().unwrap(),
        "--output",
        "box",
        "--param",
        "width=4",
    ]);
    assert!(changed.status.success());
    let changed: serde_json::Value = serde_json::from_slice(&changed.stdout).unwrap();
    near(&changed["volume"], 48.);
    assert_eq!(changed["volume_centroid"], serde_json::json!([2., 1.5, 2.]));
    assert!(
        changed["mass_kg"].is_null()
            && changed["center_of_mass"].is_null()
            && changed["central_mass_inertia"].is_null()
    );
    assert_ne!(
        changed["snapshot"]["content_hash"],
        report["snapshot"]["content_hash"]
    );
    assert_eq!(
        changed["snapshot"]["recipe_source_hash"],
        report["snapshot"]["recipe_source_hash"]
    );
    assert_eq!(changed["snapshot"]["parameter_overrides"]["width"], 4.);
    assert_eq!(fs::read_to_string(&recipe).unwrap(), source);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    for density in ["0", "-1", "NaN", "inf", "1e-320"] {
        let failed = cli(&[
            "properties",
            recipe.to_str().unwrap(),
            "--output",
            "box",
            &format!("--density-kg-per-m3={density}"),
        ]);
        assert!(
            !failed.status.success(),
            "density {density} unexpectedly accepted"
        );
        let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "invalid_request");
    }
    assert!(
        !cli(&["properties", recipe.to_str().unwrap(), "--output", "absent"])
            .status
            .success()
    );
}

#[test]
fn assembly_properties_distinguish_volume_and_density_weighted_centers() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("mixed.star");
    let source="p=rectangle(id='p',width=2,depth=2)\nb=extrude(id='b',profile=p,height=2)\na=part(id='a',body=b,material='A',density_kg_per_m3=1000)\nc=part(id='c',body=b,material='C',density_kg_per_m3=2000)\ni=instance(id='i',part=a)\nj=instance(id='j',part=c,offset=[10,0,0])\nr=assembly(id='r',instances=[i,j])\nassembly_output(name='mixed',assembly=r)\n";
    fs::write(&recipe, source).unwrap();
    let query = || cli(&["properties", recipe.to_str().unwrap(), "--output", "mixed"]);
    let result = query();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    near(&report["volume"], 16.);
    assert_eq!(report["volume_centroid"], serde_json::json!([6., 1., 1.]));
    near(&report["mass_kg"], 24e-6);
    near(&report["center_of_mass"][0], 23. / 3.);
    near(&report["central_mass_inertia"][0][0], 16e-6);
    near(&report["central_mass_inertia"][1][1], 16e-6 + 4800e-6 / 9.);
    assert_eq!(report["missing_density_occurrences"], 0);
    let rejected = cli(&[
        "properties",
        recipe.to_str().unwrap(),
        "--output",
        "mixed",
        "--density-kg-per-m3",
        "1000",
    ]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("assembly density override"));
    fs::write(
        &recipe,
        source.replace(",material='C',density_kg_per_m3=2000", ""),
    )
    .unwrap();
    let missing = query();
    assert!(missing.status.success());
    let missing: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(missing["missing_density_occurrences"], 1);
    assert!(
        missing["mass_kg"].is_null()
            && missing["center_of_mass"].is_null()
            && missing["central_mass_inertia"].is_null()
    );
    near(&missing["volume"], 16.);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn empty_body_properties_have_zero_mass_and_no_fabricated_centroid() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("empty.star");
    fs::write(&recipe,"p=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\ne=cut(id='e',left=b,right=b)\noutput(name='empty',feature=e)\n").unwrap();
    let result = cli(&[
        "properties",
        recipe.to_str().unwrap(),
        "--output",
        "empty",
        "--density-kg-per-m3",
        "8000",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    near(&report["volume"], 0.);
    near(&report["mass_kg"], 0.);
    for field in [
        "volume_centroid",
        "central_volume_inertia",
        "center_of_mass",
        "central_mass_inertia",
    ] {
        assert!(report[field].is_null(), "{field}");
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

const FACE_DATUM_RECIPE: &str = r#"
h = param(name="height", default=4)
x = param(name="cut_x", default=-1)
w = param(name="cut_width", default=3)
def box(id, width, depth, height):
    return extrude(id=id,profile=rectangle(id=id+"_p",width=width,depth=depth),height=height)
blank = box("blank",10,10,h)
tool = translate(id="tool_at",body=box("tool",w,12,h+2),offset=[x,-1,-1])
shaped = cut(id="shaped",left=blank,right=tool)
base = part(id="base",body=shaped,datums={"top":face_datum(from_feature=blank,role="end_cap",x_direction=[1,0,0])})
block = part(id="block",body=box("block_body",2,2,2),datums={"foot":datum(offset=[1,1,0])})
b = instance(id="base",part=base)
c = instance(id="mounted",part=block,attach_to=b,datum="foot",target_datum="top")
assembly_output(name="mounted",assembly=assembly(id="mounted",instances=[b,c]))
output(name="base_body",feature=shaped)
"#;

#[test]
fn associative_datums_publish_witnesses_reposition_and_preserve_failed_generations() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("face.star");
    let out = directory.path().join("out");
    let cache = directory.path().join("cache");
    fs::write(&path, FACE_DATUM_RECIPE).unwrap();
    let args = [
        "build",
        path.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ];
    let first = successful_json(cli(&args));
    let datum = &first["face_datums"]["base"]["top"];
    assert_eq!(datum["selection"]["status"], "resolved");
    assert_eq!(datum["selection"]["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(
        datum["frame"]["translation"],
        serde_json::json!([6., 5., 4.])
    );
    assert_eq!(datum["definition"]["anchor_feature"], "blank");
    assert_eq!(
        datum["target_content_hash"],
        first["feature_hashes"]["shaped"]
    );
    let assembled: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("mounted.assembly.json")).unwrap()).unwrap();
    assert_eq!(
        assembled["definitions"]["base"]["datums"]["top"],
        datum["frame"]
    );
    let attached = assembled["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "mounted")
        .unwrap();
    assert_eq!(
        attached["transform"]["translation"],
        serde_json::json!([5., 4., 4.])
    );
    let compiled: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("recipe.json")).unwrap()).unwrap();
    assert_eq!(compiled["face_datums"][0]["part"], "base");
    assert!(compiled["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "base")
        .unwrap()["datums"]["top"]
        .is_null());
    let checked = successful_json(cli(&[
        "inspect",
        out.join("recipe.json").to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]));
    assert_eq!(checked["face_datums"], first["face_datums"]);
    assert!(checked["stats"]["disk_cache_hits"].as_u64().unwrap() > 0);
    let mut raised = args.to_vec();
    raised.extend(["--param", "height=7"]);
    let changed = successful_json(cli(&raised));
    assert_eq!(
        changed["face_datums"]["base"]["top"]["frame"]["translation"],
        serde_json::json!([6., 5., 7.])
    );
    assert_ne!(
        changed["outputs"]["mounted"]["content_hash"],
        first["outputs"]["mounted"]["content_hash"]
    );
    assert_eq!(
        changed["feature_hashes"]["block_body"],
        first["feature_hashes"]["block_body"]
    );
    assert!(changed["stats"]["disk_cache_hits"].as_u64().unwrap() > 0);
    let original = artifact_bytes(&out);
    for (parameters, status) in [
        (
            vec!["--param", "cut_x=4", "--param", "cut_width=2"],
            "ambiguous",
        ),
        (vec!["--param", "cut_width=12"], "deleted"),
    ] {
        for _ in 0..2 {
            let mut failed = args.to_vec();
            failed.extend(&parameters);
            let failed = cli(&failed);
            assert!(!failed.status.success());
            let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
            assert_eq!(error["code"], "face_datum_failed", "{error}");
            assert_eq!(error["part"], "base");
            assert_eq!(error["datum"], "top");
            assert_eq!(error["selection"]["status"], status);
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
            assert_eq!(artifact_bytes(&out), original);
        }
    }
    fs::write(
        &path,
        FACE_DATUM_RECIPE.replace("x_direction=[1,0,0]", "x_direction=[0,0,1]"),
    )
    .unwrap();
    let parallel = cli(&args);
    assert!(!parallel.status.success());
    let error: serde_json::Value = serde_json::from_slice(&parallel.stderr).unwrap();
    assert_eq!(error["code"], "face_datum_frame_failed", "{error}");
    assert_eq!(artifact_bytes(&out), original);
}

#[test]
fn semantic_datum_diff_tracks_intent_even_when_resolved_frame_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let before = directory.path().join("before.star");
    let after = directory.path().join("after.star");
    fs::write(&before, FACE_DATUM_RECIPE).unwrap();
    fs::write(
        &after,
        FACE_DATUM_RECIPE.replace("x_direction=[1,0,0]", "x_direction=[2,0,0]"),
    )
    .unwrap();
    let diff = successful_json(cli(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]));
    assert_eq!(diff["outputs"]["base_body"]["status"], "unchanged");
    assert_eq!(diff["outputs"]["mounted"]["status"], "unchanged");
    let datum = &diff["face_datums"]["base"]["top"];
    assert_eq!(datum["status"], "changed");
    assert_eq!(datum["before"]["frame"], datum["after"]["frame"]);
    assert_ne!(datum["before"]["definition"], datum["after"]["definition"]);
}

const SELECTIVE_RECIPE: &str = r#"
h = param(name="unused_height", default=7)
def box(id, width, height):
    return extrude(id=id,profile=rectangle(id=id+"_profile",width=width,depth=3),height=height)
a = box("a",2,4)
b = box("b",5,h)
output(name="first",feature=a)
output(name="second",feature=b)
"#;

#[test]
fn selected_generations_skip_work_keep_source_and_replace_all_previous_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("scope.star");
    let out = dir.path().join("out");
    let cache = dir.path().join("cache");
    fs::write(&recipe, SELECTIVE_RECIPE).unwrap();
    let args = [
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ];
    let full = successful_json(cli(&args));
    assert_eq!(full["scope"]["mode"], "all");
    assert!(out.join("second.step").exists());
    let mut selected = args.to_vec();
    selected.extend(["--output", "first"]);
    let result = successful_json(cli(&selected));
    assert_eq!(result["scope"]["mode"], "selected");
    assert_eq!(
        result["scope"]["requested_outputs"],
        serde_json::json!(["first"])
    );
    assert_eq!(
        result["scope"]["evaluated_features"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        result["scope"]["skipped_features"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(result["outputs"].as_object().unwrap().len(), 1);
    assert!(!out.join("second.step").exists());
    assert_eq!(
        fs::read_to_string(out.join("recipe.star")).unwrap(),
        SELECTIVE_RECIPE
    );
    let captured: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("recipe.json")).unwrap()).unwrap();
    assert_eq!(captured["outputs"].as_array().unwrap().len(), 2);
    assert_ne!(full["recipe_context_hash"], result["recipe_context_hash"]);
    let mut unrelated = selected.clone();
    unrelated.extend(["--param", "unused_height=9"]);
    let changed = successful_json(cli(&unrelated));
    assert_eq!(
        changed["outputs"]["first"]["content_hash"],
        result["outputs"]["first"]["content_hash"]
    );
    assert_eq!(changed["stats"]["disk_cache_hits"], 1);
    assert_eq!(changed["stats"]["computed_solids"], 0);
    let original = artifact_bytes(&out);
    let mut unknown = args.to_vec();
    unknown.extend(["--output", "missing"]);
    let failed = cli(&unknown);
    assert!(!failed.status.success());
    let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "unknown_selected_output");
    assert_eq!(error["output"], "missing");
    assert_eq!(artifact_bytes(&out), original);
    let inspected = successful_json(cli(&[
        "inspect",
        recipe.to_str().unwrap(),
        "--output",
        "second",
        "--output",
        "first",
        "--output",
        "second",
    ]));
    assert_eq!(
        inspected["scope"]["requested_outputs"],
        serde_json::json!(["first", "second"])
    );
    assert_eq!(inspected["outputs"].as_object().unwrap().len(), 2);
    let properties = successful_json(cli(&[
        "properties",
        recipe.to_str().unwrap(),
        "--output",
        "first",
    ]));
    assert_eq!(properties["scope"]["mode"], "selected");
    assert_eq!(
        properties["scope"]["evaluated_features"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn selected_builds_enforce_global_assertions_and_check_remains_eager() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("scope.star");
    let out = dir.path().join("out");
    fs::write(&recipe, SELECTIVE_RECIPE).unwrap();
    let args = [
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--output",
        "first",
    ];
    successful_json(cli(&args));
    let original = artifact_bytes(&out);
    fs::write(&recipe, format!("{SELECTIVE_RECIPE}\nassert_faces(id=\"global\",from_feature=b,role=\"end_cap\",in_feature=b,count=2)\n")).unwrap();
    let failed = cli(&args);
    assert!(!failed.status.success());
    let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "face_assertion_failed");
    assert_eq!(artifact_bytes(&out), original);
    fs::write(
        &recipe,
        SELECTIVE_RECIPE.replace("b = box(\"b\",5,h)", "b = box(\"b\",5,-1)"),
    )
    .unwrap();
    assert!(!cli(&["check", recipe.to_str().unwrap()]).status.success());
    fs::write(&recipe, SELECTIVE_RECIPE).unwrap();
    let eager = successful_json(cli(&["check", recipe.to_str().unwrap()]));
    assert_eq!(eager["scope"]["mode"], "all");
    assert_eq!(
        eager["scope"]["evaluated_features"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(eager["outputs"].as_object().unwrap().len(), 2);
}

#[test]
fn watch_forwards_output_scope_and_bom_evaluates_selected_assembly() {
    struct Running(std::process::Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let recipe = dir.path().join("scope.star");
    let out = dir.path().join("out");
    fs::write(&recipe, SELECTIVE_RECIPE).unwrap();
    let _running = Running(
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "watch",
                recipe.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
                "--output",
                "first",
                "--interval-ms",
                "10",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let wait = |volume| {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(bytes) = fs::read(out.join("manifest.json")) {
                let manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                if manifest["outputs"]["first"]["volume_mm3"] == volume {
                    return manifest;
                }
            }
            assert!(
                std::time::Instant::now() < end,
                "watch did not publish expected volume"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    let first = wait(serde_json::json!(24.));
    assert_eq!(
        first["scope"]["requested_outputs"],
        serde_json::json!(["first"])
    );
    assert!(!out.join("second.step").exists());
    fs::write(
        &recipe,
        SELECTIVE_RECIPE.replace("a = box(\"a\",2,4)", "a = box(\"a\",2,5)"),
    )
    .unwrap();
    let next = wait(serde_json::json!(30.));
    assert_eq!(next["scope"]["mode"], "selected");
    assert!(!out.join("second.step").exists());
    drop(_running);
    fs::write(&recipe, format!("{SELECTIVE_RECIPE}\np=part(id=\"part\",body=a)\nassembly_output(name=\"group\",assembly=assembly(id=\"group\",instances=[instance(id=\"item\",part=p)]))\n")).unwrap();
    let bom = successful_json(cli(&[
        "bom",
        recipe.to_str().unwrap(),
        "--assembly",
        "group",
    ]));
    assert_eq!(bom["build_scope"]["mode"], "selected");
    assert_eq!(
        bom["build_scope"]["requested_outputs"],
        serde_json::json!(["group"])
    );
    assert_eq!(
        bom["build_scope"]["evaluated_features"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn custom_tolerance_flows_to_holed_body_and_assembly_previews() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("small.star");
    fs::write(&source, "p=polygon(id='p',points=[[0,0],[0.00001,0],[0.00001,0.00001],[0,0.00001]],holes=[[[0.00000005,0.000002],[0.00000005,0.000008],[0.000004,0.000008],[0.000004,0.000002]]])\nb=extrude(id='b',profile=p,height=0.000003)\ndefinition=part(id='part',body=b)\na=assembly(id='assembly',instances=[instance(id='only',part=definition)])\noutput(name='body',feature=b)\nassembly_output(name='assembly',assembly=a)\n").unwrap();
    let compiled = cli(&["compile", source.to_str().unwrap()]);
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let mut recipe: serde_json::Value = serde_json::from_slice(&compiled.stdout).unwrap();
    recipe["tolerance"] = serde_json::json!({"linear": 1e-10});
    let input = directory.path().join("small.json");
    fs::write(&input, serde_json::to_vec(&recipe).unwrap()).unwrap();
    let destination = directory.path().join("built");
    let built = cli(&[
        "build",
        input.to_str().unwrap(),
        "--out-dir",
        destination.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&built.stdout).unwrap();
    let expected = (1e-10 - (4e-6 - 5e-8) * 6e-6) * 3e-6;
    let volume = report["outputs"]["body"]["volume_mm3"].as_f64().unwrap();
    assert!((volume - expected).abs() < expected * 1e-10);
    for file in [
        "body.preview.html",
        "body.preview.svg",
        "body.step",
        "body.stl",
        "assembly.preview.html",
        "assembly.preview.svg",
        "assembly.gpu.html",
        "assembly.stream.html",
        "assembly.display.json",
    ] {
        assert!(destination.join(file).is_file(), "missing {file}");
    }
    // The 5e-8 mm clearance is below the default 1e-7 mm tolerance. This fixture
    // must fail under that policy rather than accidentally exercising defaults.
    recipe["tolerance"] = serde_json::json!({"linear": 1e-7});
    fs::write(&input, serde_json::to_vec(&recipe).unwrap()).unwrap();
    assert!(!cli(&["check", input.to_str().unwrap()]).status.success());
}

#[test]
fn sketch_diagnostics_and_failed_solves_preserve_last_valid_publication() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("fixture.star");
    let output = directory.path().join("out");
    let source = fs::read_to_string(example("constrained_slope_fixture.star")).unwrap();
    fs::write(&source_path, &source).unwrap();
    let run = || {
        cli(&[
            "build",
            source_path.to_str().unwrap(),
            "--out-dir",
            output.to_str().unwrap(),
        ])
    };
    let valid = run();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&valid.stdout).unwrap();
    let sketch = &report["sketch_reports"]["support_sketch"];
    assert_eq!(sketch["status"], "fully_constrained");
    assert_eq!(sketch["degrees_of_freedom"], 0);
    assert_eq!(sketch["rank"], 6);
    assert!(sketch["max_residual_mm"].as_f64().unwrap() <= 1e-7);
    assert_ne!(sketch["points"]["c"], serde_json::json!([28, 30]));
    assert_eq!(sketch["solver_build_id"], report["solver_build_id"]);
    let manifest = fs::read(output.join("manifest.json")).unwrap();
    let step = fs::read(output.join("constrained_slope_fixture.step")).unwrap();
    let captured = fs::read(output.join("recipe.star")).unwrap();
    let underconstrained=source.replace("    {\"id\":\"right_length\", \"kind\":\"distance\", \"a\":\"b\", \"b\":\"c\", \"value\":right_length},\n","");
    assert_ne!(underconstrained, source);
    let conflicting=source.replace("], outline =", "    {\"id\":\"conflict\",\"kind\":\"fixed\",\"point\":\"b\",\"position\":[span+10,0]},\n], outline =");
    for (text, status) in [
        (underconstrained, "underconstrained"),
        (conflicting, "not_converged"),
    ] {
        fs::write(&source_path, text).unwrap();
        let failed = run();
        assert!(!failed.status.success());
        let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "sketch_solve_failed", "{error}");
        assert_eq!(error["feature"], "support_sketch");
        assert_eq!(error["target_content_hash"].as_str().unwrap().len(), 64);
        assert_eq!(error["solver_build_id"], error["sketch"]["solver_build_id"]);
        assert_eq!(error["sketch"]["status"], status, "{error}");
        assert!(error["sketch"]["constraint_residuals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["residual_mm"].as_f64().is_some_and(f64::is_finite)));
        assert_eq!(fs::read(output.join("manifest.json")).unwrap(), manifest);
        assert_eq!(
            fs::read(output.join("constrained_slope_fixture.step")).unwrap(),
            step
        );
        assert_eq!(fs::read(output.join("recipe.star")).unwrap(), captured);
    }
    fs::write(&source_path, &source).unwrap();
    assert!(run().status.success());
    let revised = directory.path().join("revised.star");
    fs::write(&revised, source.replace("default = 60)", "default = 62)")).unwrap();
    let diff = cli(&[
        "diff",
        source_path.to_str().unwrap(),
        revised.to_str().unwrap(),
    ]);
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let diff: serde_json::Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_eq!(diff["sketches"]["support_sketch"]["status"], "changed");
    assert_ne!(
        diff["sketches"]["support_sketch"]["before_hash"],
        diff["sketches"]["support_sketch"]["after_hash"]
    );
}

#[test]
fn angular_conflicts_report_separate_units_and_preserve_published_generation() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("angle.star");
    let output = directory.path().join("out");
    let source = fs::read_to_string(example("constrained_angle_fixture.star")).unwrap();
    fs::write(&source_path, &source).unwrap();
    let run = || {
        cli(&[
            "build",
            source_path.to_str().unwrap(),
            "--out-dir",
            output.to_str().unwrap(),
        ])
    };
    let valid = successful_json(run());
    let report = &valid["sketch_reports"]["support_sketch"];
    assert_eq!(report["schema_version"], 3);
    assert_eq!(report["status"], "fully_constrained");
    assert_eq!(report["angular_tolerance_radians"], 1e-9);
    assert!(report["max_angular_residual_radians"].as_f64().unwrap() <= 1e-9);
    assert!(
        report["max_residual_mm"].as_f64().unwrap() <= report["tolerance_mm"].as_f64().unwrap()
    );
    let original = artifact_bytes(&output);
    assert!(!original.is_empty());
    // Two individually valid directed angles on the same endpoint pairs are
    // incompatible. Keep structural validation valid so this exercises solving.
    let conflicting = source.replacen(
        "], outline = [\"a\",\"b\",\"c\"])",
        "    {\"id\":\"angle_conflict\",\"kind\":\"angle\",\"a\":[\"a\",\"b\"],\"b\":[\"a\",\"c\"],\"degrees\":tilt_degrees+10},\n], outline = [\"a\",\"b\",\"c\"])",
        1,
    );
    assert_ne!(conflicting, source);
    fs::write(&source_path, &conflicting).unwrap();
    let failed = run();
    assert!(!failed.status.success());
    let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "sketch_solve_failed", "{error}");
    assert_eq!(error["feature"], "support_sketch");
    assert_eq!(error["target_content_hash"].as_str().unwrap().len(), 64);
    assert_ne!(
        error["target_content_hash"],
        valid["feature_hashes"]["support_sketch"]
    );
    let failed_report = &error["sketch"];
    assert_eq!(failed_report["schema_version"], 3);
    assert_eq!(failed_report["status"], "not_converged");
    assert_eq!(failed_report["solver_build_id"], error["solver_build_id"]);
    assert_eq!(failed_report["angular_tolerance_radians"], 1e-9);
    let point = |id: &str| {
        let p = &failed_report["points"][id];
        [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
    };
    let a = point("a");
    let b = point("b");
    let c = point("c");
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [c[0] - a[0], c[1] - a[1]];
    let actual_angle = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
    let residuals = failed_report["constraint_residuals"].as_array().unwrap();
    let mut max_mm = 0f64;
    let mut max_radians = 0f64;
    for residual in residuals {
        let angular = matches!(
            residual["id"].as_str().unwrap(),
            "incline" | "upright" | "angle_conflict"
        );
        let (field, absent) = if angular {
            ("residual_radians", "residual_mm")
        } else {
            ("residual_mm", "residual_radians")
        };
        let value = residual[field].as_f64().unwrap();
        assert!(value.is_finite() && value >= 0.);
        assert!(residual.get(absent).is_none());
        if angular {
            max_radians = max_radians.max(value);
        } else {
            max_mm = max_mm.max(value);
        }
        if let Some(degrees) = match residual["id"].as_str().unwrap() {
            "incline" => Some(30f64),
            "angle_conflict" => Some(40f64),
            _ => None,
        } {
            let independently_measured = ((actual_angle - degrees.to_radians()
                + std::f64::consts::PI)
                .rem_euclid(std::f64::consts::TAU)
                - std::f64::consts::PI)
                .abs();
            assert!((value - independently_measured).abs() < 1e-12);
        }
    }
    // Incompatible targets are ten degrees apart, so at least one independent
    // angular witness must miss its target by five degrees or more.
    assert!(max_radians >= 5f64.to_radians() - 1e-10);
    assert!(
        (failed_report["max_angular_residual_radians"]
            .as_f64()
            .unwrap()
            - max_radians)
            .abs()
            < 1e-14
    );
    assert!((failed_report["max_residual_mm"].as_f64().unwrap() - max_mm).abs() < 1e-12);
    assert_eq!(artifact_bytes(&output), original);
    let checked = cli(&["check", source_path.to_str().unwrap()]);
    assert!(!checked.status.success());
    let check_error: serde_json::Value = serde_json::from_slice(&checked.stderr).unwrap();
    assert_eq!(check_error["sketch"], error["sketch"]);
    assert_eq!(artifact_bytes(&output), original);
    fs::write(&source_path, &source).unwrap();
    let restored = successful_json(run());
    assert_eq!(restored["sketch_reports"], valid["sketch_reports"]);
    assert_eq!(
        fs::read(output.join("recipe.star")).unwrap(),
        source.as_bytes()
    );
}

#[test]
fn invalid_solved_circle_profile_preserves_last_valid_drill_jig() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("jig.star");
    let output = directory.path().join("out");
    let source = fs::read_to_string(example("constrained_drill_jig.star")).unwrap();
    fs::write(&source_path, &source).unwrap();
    let run = || {
        cli(&[
            "build",
            source_path.to_str().unwrap(),
            "--out-dir",
            output.to_str().unwrap(),
        ])
    };
    let valid = successful_json(run());
    assert_eq!(
        valid["sketch_reports"]["guide_layout"]["status"],
        "fully_constrained"
    );
    let before = artifact_bytes(&output);
    // Keep all equations valid but make native circle geometry extend outside
    // the outer boundary. This must fail after solving, before publication.
    let invalid = source.replace(
        "{\"center\":center,\"rim\":center+\"_rim\"}",
        "{\"center\":\"a\",\"rim\":center+\"_rim\"}",
    );
    assert_ne!(invalid, source);
    fs::write(&source_path, &invalid).unwrap();
    let failed = run();
    assert!(!failed.status.success());
    let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "invalid_polygon_profile", "{error}");
    assert_eq!(error["feature"], "guide_layout");
    assert!(error["diagnostic"].as_str().is_some());
    assert_eq!(artifact_bytes(&output), before);
    // Explicitly selected output still enforces the same native validity gate.
    let selected = cli(&[
        "build",
        source_path.to_str().unwrap(),
        "--output",
        "plate",
        "--out-dir",
        output.to_str().unwrap(),
    ]);
    assert!(!selected.status.success());
    assert_eq!(artifact_bytes(&output), before);
    fs::write(&source_path, &source).unwrap();
    let restored = successful_json(run());
    assert_eq!(restored["sketch_reports"], valid["sketch_reports"]);
}
