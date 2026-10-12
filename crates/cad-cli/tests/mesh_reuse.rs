//! Cross-consumer acceptance: preview-first and export-first must retain the
//! same certified geometry, including native holes and outward closed triangles.
use cad_kernel::{ModelSnapshot, Tolerance};
use std::collections::BTreeMap;

fn check_stl(stl: &str, expected_volume: f64, volume_error: f64) {
    let vertices: Vec<[f64; 3]> = stl
        .lines()
        .filter_map(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            (words.first() == Some(&"vertex"))
                .then(|| std::array::from_fn(|i| words[i + 1].parse().unwrap()))
        })
        .collect();
    assert_eq!(vertices.len() % 3, 0);
    let mut edges = BTreeMap::<([u64; 3], [u64; 3]), (usize, i32)>::new();
    let mut volume = 0.;
    for tri in vertices.as_chunks::<3>().0 {
        let [a, b, c] = [tri[0], tri[1], tri[2]];
        volume += (a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.;
        for i in 0..3 {
            let key = |p: [f64; 3]| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
            let a = key(tri[i]);
            let b = key(tri[(i + 1) % 3]);
            assert_ne!(a, b);
            let (pair, direction) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let value = edges.entry(pair).or_default();
            value.0 += 1;
            value.1 += direction;
        }
    }
    assert!(
        edges.values().all(|v| *v == (2, 0)),
        "STL must be an oriented closed manifold"
    );
    assert!(
        (volume - expected_volume).abs() <= volume_error,
        "volume {volume} expected {expected_volume}"
    );
}

fn check_order(model: &ModelSnapshot, volume: f64, error: f64) {
    let tolerance = Tolerance::default();
    let reference_stl = cad_export::to_stl(model, tolerance).unwrap();
    let reference_svg = cad_preview::to_svg_with_tolerance(model, tolerance).unwrap();
    for preview_first in [false, true] {
        let mut cache = cad_mesh::GenerationMeshCache::default();
        if preview_first {
            assert_eq!(
                reference_svg,
                cad_preview::to_svg_with_cache(model, tolerance, &mut cache).unwrap()
            );
        }
        let stl = cad_export::to_stl_with_cache(model, tolerance, &mut cache).unwrap();
        assert_eq!(reference_stl, stl);
        let builds = cache.stats().builds;
        assert!(builds > 0);
        assert_eq!(
            reference_svg,
            cad_preview::to_svg_with_cache(model, tolerance, &mut cache).unwrap()
        );
        assert_eq!(
            stl,
            cad_export::to_stl_with_cache(model, tolerance, &mut cache).unwrap()
        );
        assert_eq!(
            cache.stats().builds,
            builds,
            "warm cross-consumer requests must reuse certified work"
        );
        assert!(cache.stats().hits > 0);
        check_stl(&stl, volume, error);
    }
}

#[test]
fn analytic_and_planar_holes_survive_cross_consumer_reuse() {
    let tolerance = Tolerance::default();
    let tube = cad_kernel::extrude_annulus(8., 3., 6., tolerance)
        .unwrap()
        .model;
    // Chord sag <= .01 mm: bound both circular area deviations by annular strips.
    check_order(
        &tube,
        std::f64::consts::PI * (64. - 9.) * 6.,
        2. * std::f64::consts::PI * 11. * 0.01 * 6.,
    );
    let outer = [
        [0., 0.],
        [20., 0.],
        [20., 16.],
        [12., 16.],
        [12., 8.],
        [8., 8.],
        [8., 16.],
        [0., 16.],
    ];
    let holes = vec![vec![[2., 2.], [2., 5.], [5., 5.], [5., 2.]]];
    let plate = cad_kernel::extrude_polygon_with_holes(&outer, &holes, 4., tolerance)
        .unwrap()
        .model;
    check_order(&plate, (320. - 32. - 9.) * 4., 1e-8);
}

#[test]
fn publication_shares_body_assembly_and_lod_meshes() {
    let directory = tempfile::tempdir().unwrap();
    let recipe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/constrained_drill_jig.star");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cad"))
        .args([
            "build",
            recipe.to_str().unwrap(),
            "--out-dir",
            directory.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let published: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("manifest.json")).unwrap())
            .unwrap();
    let stats = &summary["mesh_cache"];
    assert_eq!(stats, &published["mesh_cache"]);
    // Two analytic definitions (plate, guide), each requested at three policies.
    // The plate is also a body output, and the guide has repeated occurrences.
    assert_eq!(stats["builds"], 6);
    assert_eq!(stats["misses"], 6);
    assert_eq!(stats["failures"], 0);
    assert_eq!(stats["evictions"], 0);
    assert!(stats["hits"].as_u64().unwrap() >= 10);
    assert!(
        stats["retained_bytes"].as_u64().unwrap() <= stats["max_retained_bytes"].as_u64().unwrap()
    );
    assert_eq!(stats["kernel_build_id"], summary["kernel_build_id"]);
    assert_eq!(stats["mesher_build_id"].as_str().unwrap().len(), 64);
}
