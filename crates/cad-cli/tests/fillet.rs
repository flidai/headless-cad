use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn success(args: &[&str]) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn rounded_cover_publishes_exact_native_geometry_source_and_failure_atomicity() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rounded_bearing_cover.star");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("generation");
    let cache = dir.path().join("cache");
    let built = success(&[
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]);
    assert_eq!(built["outputs"]["bearing_cover"]["topology"]["solids"], 1);
    assert_eq!(built["face_assertions"].as_object().unwrap().len(), 4);
    assert!(built["stats"]["computed_solids"].as_u64().unwrap() > 0);
    for face in built["face_assertions"].as_object().unwrap().values() {
        assert_eq!(face["status"], "resolved");
        assert_eq!(face["witnesses"].as_array().unwrap().len(), 1);
    }
    let warm = success(&[
        "inspect",
        source.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ]);
    assert_eq!(warm["stats"]["computed_solids"], 0);
    assert!(warm["stats"]["disk_cache_hits"].as_u64().unwrap() > 0);
    assert_eq!(warm["face_assertions"], built["face_assertions"]);
    let area = 2400. - (4. - std::f64::consts::PI) * 16. - 125. * std::f64::consts::PI;
    assert!(
        (built["outputs"]["bearing_cover"]["volume_mm3"]
            .as_f64()
            .unwrap()
            - area * 8.)
            .abs()
            < 1e-7
    );
    let step = fs::read_to_string(out.join("bearing_cover.step")).unwrap();
    assert!(step.contains("CYLINDRICAL_SURFACE"));
    success(&["verify", out.to_str().unwrap()]);
    let replay = success(&["inspect", out.join("recipe.star").to_str().unwrap()]);
    assert_eq!(
        built["outputs"]["bearing_cover"]["content_hash"],
        replay["outputs"]["bearing_cover"]["content_hash"]
    );
    let old = fs::read(out.join("manifest.json")).unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--param",
            "corner_radius=50",
            "--out-dir",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert_eq!(fs::read(out.join("manifest.json")).unwrap(), old);
    for (radius, bore, height) in [(2., 8., 4.), (6., 12., 12.)] {
        let r = format!("corner_radius={radius}");
        let b = format!("bore_radius={bore}");
        let h = format!("height={height}");
        let variant = success(&[
            "inspect",
            source.to_str().unwrap(),
            "--param",
            &r,
            "--param",
            &b,
            "--param",
            &h,
            "--cache-dir",
            cache.to_str().unwrap(),
        ]);
        let expected = (2400.
            - (4. - std::f64::consts::PI) * radius * radius
            - std::f64::consts::PI * (bore * bore + 25.))
            * height;
        assert!(
            (variant["outputs"]["bearing_cover"]["volume_mm3"]
                .as_f64()
                .unwrap()
                - expected)
                .abs()
                < 1e-7
        );
        assert_eq!(variant["face_assertions"].as_object().unwrap().len(), 4);
    }
    let caps = success(&["capabilities", "--operation", "feature.fillet"]);
    assert!(caps.to_string().contains("solid.longitudinal_fillet"));
}

#[test]
fn rounded_cover_radius_annotations_bind_native_arcs_and_remain_global_obligations() {
    let source_text = include_str!("../../../examples/rounded_bearing_cover.star");
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.star");
    let out = dir.path().join("generation");
    let cache = dir.path().join("cache");
    fs::write(&source, source_text).unwrap();
    let args = [
        "build",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--cache-dir",
        cache.to_str().unwrap(),
    ];
    let built = success(&args);
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("section-cover_midsection.json")).unwrap())
            .unwrap();
    let dimensions = report["section_view"]["circle_dimensions"]
        .as_array()
        .unwrap();
    assert_eq!(dimensions.len(), 5);
    for dimension in dimensions
        .iter()
        .filter(|d| d["declaration"]["coverage"] == "single_arc")
    {
        assert_eq!(dimension["value_mm"], 4.);
        assert_eq!(dimension["selection"]["status"], "resolved");
        assert_eq!(
            dimension["selection"]["witnesses"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            dimension["arc"]["target_face"],
            dimension["selection"]["witnesses"][0]["target_face"]
        );
        let loop_index = dimension["arc"]["loop_index"].as_u64().unwrap() as usize;
        let segment_index = dimension["arc"]["segment_index"].as_u64().unwrap() as usize;
        let segment = &report["section"]["loops"][loop_index]["segments"][segment_index];
        assert_eq!(segment["face"], dimension["arc"]["target_face"]);
        assert_eq!(segment["curve"]["kind"], "arc");
        assert_eq!(segment["curve"]["radius"], dimension["radius"]);
        assert_eq!(
            dimension["target_content_hash"],
            built["outputs"]["bearing_cover"]["content_hash"]
        );
    }
    let svg = fs::read_to_string(out.join("section-cover_midsection.svg")).unwrap();
    assert_eq!(svg.matches("class=\"section-circle-dimension\"").count(), 5);
    for dimension in dimensions
        .iter()
        .filter(|d| d["declaration"]["coverage"] == "single_arc")
    {
        let tip_x = dimension["arc"]["tip"][0].as_f64().unwrap();
        let tip_y = dimension["arc"]["tip"][1].as_f64().unwrap();
        assert!(svg.contains(&format!("x2=\"{}\" y2=\"{}\"", tip_x, -tip_y)));
    }
    let warm = success(&args);
    assert_eq!(warm["stats"]["computed_solids"], 0);
    let prior_manifest = fs::read(out.join("manifest.json")).unwrap();
    fs::write(
        &source,
        source_text.replace(
            "max = corner_radius+0.000001",
            "max = corner_radius-0.000001",
        ),
    )
    .unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--output",
            "rounded_blank",
            "--display-only",
        ])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "section_dimension_failed");
    assert_eq!(prior_manifest, fs::read(out.join("manifest.json")).unwrap());
}
