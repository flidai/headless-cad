use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

#[test]
fn continuous_rotation_publication_covers_paths_replays_and_preserves_failed_generations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("rotation_clearance.star");
    let text = include_str!("../../../examples/rotation_clearance.star");
    fs::write(&source, text).unwrap();
    let out = temp.path().join("out");
    let cache = temp.path().join("cache");
    let run = |input: &Path, target: &Path, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .args([
                "build",
                input.to_str().unwrap(),
                "--out-dir",
                target.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    let first = run(&source, &out, &[]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let manifest: Value = serde_json::from_slice(&first.stdout).unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("clearance-checks.json")).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 4);
    assert_eq!(report["checks"], manifest["clearance_checks"]);
    assert_eq!(
        report["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert!(report["semantics"]
        .as_str()
        .unwrap()
        .contains("entire declared"));
    assert_eq!(report["checks"].as_object().unwrap().len(), 2);
    for (id, actual_minimum) in [("tool_guard", 5.), ("tool_overhead", 6.)] {
        let check = &report["checks"][id];
        assert_eq!(check["status"], "passed");
        assert!(number(&check["lower_bound_mm"]) >= 0.5);
        assert!(number(&check["lower_bound_mm"]) <= actual_minimum);
        assert!(number(&check["upper_bound_mm"]) >= actual_minimum);
        let sweep = &check["rotation_sweep"];
        let samples = sweep["samples"].as_array().unwrap();
        assert_eq!(
            sweep["query_count"].as_u64().unwrap() as usize,
            samples.len()
        );
        assert!(samples.len() <= 1025);
        assert!(samples.iter().any(|sample| number(&sample["time"]) == 1.));
        let initial = samples
            .iter()
            .find(|sample| number(&sample["time"]) == 0.)
            .unwrap();
        assert_eq!(initial["query"], check["query"]);
        assert!(number(&sweep["required_minimum_mm"]) > 0.5);
        assert!(
            number(&check["path_uncertainty_mm"]) >= number(&check["placement_uncertainty_mm"])
        );
        let mut end = 0.;
        for interval in sweep["intervals"].as_array().unwrap() {
            let start = number(&interval["start"]);
            let next = number(&interval["end"]);
            assert_eq!(start, end, "missing or overlapping whole-path coverage");
            assert!(next > start);
            let midpoint = number(&interval["midpoint"]);
            assert_eq!(midpoint, (start + next) / 2.);
            let sample = &samples[interval["sample_index"].as_u64().unwrap() as usize];
            assert_eq!(sample["time"], interval["midpoint"]);
            assert!(number(&interval["lower_bound_mm"]) >= number(&sweep["required_minimum_mm"]));
            end = next;
        }
        assert_eq!(end, 1.);
    }
    let html = fs::read_to_string(out.join("clearance-checks.html")).unwrap();
    assert!(html.contains("Continuous rotation"));
    let stable = files(&out);

    // The endpoints and midpoint are clear, but quarter travel hits the guard.
    for x in [20., -20., 20.] {
        let sampled = text
            .replace("offset = [20,0,0]", &format!("offset = [{x},0,0]"))
            .replace(", right_rotation = path", "");
        let sample_source = temp.path().join("sample.star");
        fs::write(&sample_source, sampled).unwrap();
        let sample_out = temp.path().join("sample-out");
        let sample = run(
            &sample_source,
            &sample_out,
            &["--param", "guard_radius=20", "--param", "pose=0"],
        );
        assert!(
            sample.status.success(),
            "{}",
            String::from_utf8_lossy(&sample.stderr)
        );
        let sample: Value = serde_json::from_slice(&sample.stdout).unwrap();
        let c = &sample["clearance_checks"]["tool_guard"];
        assert!(c.get("rotation_sweep").is_none());
        assert!(number(&c["lower_bound_mm"]) > 23.);
    }
    for (args, failed_check) in [
        (
            vec![
                "--param",
                "guard_radius=20",
                "--param",
                "pose=0",
                "--output",
                "tool",
            ],
            "tool_guard",
        ),
        (
            vec!["--param", "required_clearance=100", "--output", "tool"],
            "tool_guard",
        ),
    ] {
        let failed = run(&source, &out, &args);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "clearance_check_failed");
        assert_eq!(error["check_id"], failed_check);
        assert_eq!(
            stable,
            files(&out),
            "failed global sweep replaced published files"
        );
    }
    let changed = run(&source, &out, &["--param", "angle=-1.5707963267948966"]);
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let changed: Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(changed["stats"]["computed_solids"], 0);
    assert_ne!(
        changed["recipe_context_hash"],
        manifest["recipe_context_hash"]
    );
    assert_eq!(
        changed["clearance_checks"]["tool_guard"]["query"],
        manifest["clearance_checks"]["tool_guard"]["query"]
    );
    assert_ne!(
        changed["clearance_checks"]["tool_guard"]["rotation_sweep"],
        manifest["clearance_checks"]["tool_guard"]["rotation_sweep"]
    );
    fs::remove_file(&source).unwrap();
    let replay = temp.path().join("replay");
    let replayed = run(
        &out.join("recipe.star"),
        &replay,
        &["--param", "angle=-1.5707963267948966"],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let replayed: Value = serde_json::from_slice(&replayed.stdout).unwrap();
    assert_eq!(changed["clearance_checks"], replayed["clearance_checks"]);
    assert!(Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(["verify", replay.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());
}
