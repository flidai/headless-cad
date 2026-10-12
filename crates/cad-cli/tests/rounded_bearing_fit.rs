use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn run(source: &Path, out: &Path, cache: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
        ])
        .args(args)
        .output()
        .unwrap()
}
fn success(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn verify(report: &Value, out: &Path, r: f64, gap: f64, posed: bool) {
    assert_eq!(report["outputs"]["rounded_bearing_fit"]["instances"], 2);
    let q = &report["clearance_checks"]["running_gap"];
    assert_eq!(q["status"], "passed");
    assert_eq!(q["query"]["method"], "bore_trim_witness");
    assert!(q["lower_bound_mm"].as_f64().unwrap() >= 0.1);
    assert!(
        q["lower_bound_mm"].as_f64().unwrap() <= gap
            && q["upper_bound_mm"].as_f64().unwrap() >= gap
    );
    assert!(q["upper_bound_mm"].as_f64().unwrap() - q["lower_bound_mm"].as_f64().unwrap() < 1e-6);
    assert!(q["rotation_sweep"].is_null());
    let point = |key: &str| {
        let a = q["query"][key].as_array().unwrap();
        let p = [
            a[0].as_f64().unwrap(),
            a[1].as_f64().unwrap(),
            a[2].as_f64().unwrap(),
        ];
        if posed {
            [p[1] + 7., p[2] - 15., p[0] - 13.]
        } else {
            p
        }
    };
    let housing = point("left_point");
    let shaft = point("right_point");
    assert!((housing[0].hypot(housing[1]) - (r + gap)).abs() < 1e-6);
    assert!(housing[2] >= -1e-6 && housing[2] <= 12. + 1e-6);
    assert!((shaft[0].hypot(shaft[1]) - r).abs() < 1e-6);
    assert!(shaft[2] >= -10. - 1e-6 && shaft[2] <= 30. + 1e-6);
    // The true external seat removes x>=r-2, |y|<=2, world z6..14.
    assert!(
        shaft[0] <= r - 2. + 1e-6
            || shaft[1].abs() >= 2. - 1e-6
            || shaft[2] <= 6. + 1e-6
            || shaft[2] >= 14. - 1e-6
    );
    let actual = (housing[0] - shaft[0])
        .hypot(housing[1] - shaft[1])
        .hypot(housing[2] - shaft[2]);
    assert!((actual - gap).abs() < 1e-6);
    let area =
        1200. - (4. - std::f64::consts::PI) * 16. - std::f64::consts::PI * ((r + gap).powi(2) + 8.);
    assert!(
        (report["outputs"]["housing"]["volume_mm3"].as_f64().unwrap() - area * 12.).abs() < 1e-6
    );
    for (file, count, diameter) in [
        ("housing_midsection", 5, 2. * (r + gap)),
        ("shaft_journal", 1, 2. * r),
    ] {
        let section: Value =
            serde_json::from_slice(&fs::read(out.join(format!("section-{file}.json"))).unwrap())
                .unwrap();
        let dims = section["section_view"]["circle_dimensions"]
            .as_array()
            .unwrap();
        assert_eq!(dims.len(), count);
        assert!((dims[0]["value_mm"].as_f64().unwrap() - diameter).abs() < 1e-9);
    }
}
#[test]
fn rounded_housing_keyed_shaft_fit_is_native_global_and_replayable() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("fit.star");
    fs::write(
        &source,
        include_str!("../../../examples/rounded_bearing_fit.star"),
    )
    .unwrap();
    let out = dir.path().join("out");
    let cache = dir.path().join("cache");
    let first = success(run(&source, &out, &cache, &[]));
    verify(&first, &out, 6., 0.15, false);
    let warm = success(run(&source, &out, &cache, &[]));
    assert_eq!(warm["stats"]["computed_solids"], 0);
    assert_eq!(warm["clearance_checks"], first["clearance_checks"]);
    let replay = success(run(
        &out.join("recipe.star"),
        &dir.path().join("replay"),
        &dir.path().join("fresh"),
        &[],
    ));
    assert_eq!(replay["clearance_checks"], first["clearance_checks"]);
    let manifest = fs::read(out.join("manifest.json")).unwrap();
    let bad = run(
        &source,
        &out,
        &cache,
        &[
            "--param",
            "bore_gap=0.05",
            "--output",
            "shaft",
            "--display-only",
        ],
    );
    assert!(!bad.status.success());
    let diagnostic: Value = serde_json::from_slice(&bad.stderr).unwrap();
    assert_eq!(diagnostic["code"], "clearance_check_failed");
    assert_eq!(manifest, fs::read(out.join("manifest.json")).unwrap());
    for (radius, gap, pose) in [(5., 0.25, 0), (7., 0.2, 1)] {
        let r = format!("shaft_radius={radius}");
        let g = format!("bore_gap={gap}");
        let p = format!("pose={pose}");
        let target = dir.path().join(format!("r{radius}"));
        let variant = success(run(
            &source,
            &target,
            &cache,
            &["--param", &r, "--param", &g, "--param", &p],
        ));
        verify(&variant, &target, radius, gap, pose == 1);
    }
}
