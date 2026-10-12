use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::json;

fn plate(width: f64) -> Recipe {
    serde_json::from_value(json!({
        "schema_version": 1,
        "units": "mm",
        "features": [
            {"op": "rectangle", "id": "outline", "width": width, "depth": 40.0},
            {"op": "extrude", "id": "plate", "profile": "outline", "height": 6.0}
        ],
        "outputs": [{"name": "plate", "feature": "plate"}]
    }))
    .unwrap()
}

fn logical_size(width: f64) -> u64 {
    let built = BuildEngine::default().build(&plate(width)).unwrap();
    serde_json::to_vec(&*built.outputs["plate"].result)
        .unwrap()
        .len() as u64
}

#[test]
fn logical_byte_limit_charges_exact_serialized_results_and_refuses_oversize() {
    let first_size = logical_size(80.0);
    let second_size = logical_size(90.0);
    assert!(first_size > 1 && second_size > 1);

    let mut too_small = BuildEngine::default().with_cache_limits(10, first_size - 1);
    assert_eq!(
        too_small.build(&plate(80.0)).unwrap().stats.computed_solids,
        1
    );
    let usage = too_small.cache_usage();
    assert_eq!((usage.entries, usage.logical_bytes), (0, 0));
    assert_eq!(usage.skipped_retention, 1);
    assert_eq!(
        too_small.build(&plate(80.0)).unwrap().stats.computed_solids,
        1
    );

    let mut bounded = BuildEngine::default().with_cache_limits(10, first_size + second_size - 1);
    let first = bounded.build(&plate(80.0)).unwrap();
    assert_eq!(bounded.cache_usage().logical_bytes, first_size);
    drop(first);
    let second = bounded.build(&plate(90.0)).unwrap();
    let usage = bounded.cache_usage();
    assert_eq!((usage.entries, usage.logical_bytes), (1, second_size));
    assert_eq!(usage.evictions, 1);
    assert_eq!(usage.skipped_retention, 0);
    assert_eq!(
        second.outputs["plate"].result.model.volume(),
        90.0 * 40.0 * 6.0
    );
}

#[test]
fn recent_cache_hit_changes_lru_victim() {
    let mut engine = BuildEngine::default().with_cache_limits(2, u64::MAX);
    for width in [80.0, 90.0] {
        assert_eq!(
            engine.build(&plate(width)).unwrap().stats.computed_solids,
            1
        );
    }
    assert_eq!(engine.build(&plate(80.0)).unwrap().stats.cache_hits, 1);
    assert_eq!(
        engine.build(&plate(100.0)).unwrap().stats.computed_solids,
        1
    );
    assert_eq!(engine.cache_usage().evictions, 1);
    assert_eq!(engine.build(&plate(80.0)).unwrap().stats.cache_hits, 1);
    assert_eq!(engine.build(&plate(90.0)).unwrap().stats.computed_solids, 1);
}

#[test]
fn late_failed_build_does_not_leave_a_hidden_pin() {
    let tolerance = cad_kernel::Tolerance::default().linear;
    let failing: Recipe = serde_json::from_value(json!({
        "schema_version": 1,
        "units": "mm",
        "features": [
            {"op": "rectangle", "id": "a_outline", "width": 80.0, "depth": 40.0},
            {"op": "extrude", "id": "a_good", "profile": "a_outline", "height": 6.0},
            {"op": "rectangle", "id": "z_outline", "width": 2.0, "depth": 2.0},
            {"op": "extrude", "id": "z_bad", "profile": "z_outline", "height": tolerance / 2.0}
        ],
        "outputs": [
            {"name": "good", "feature": "a_good"},
            {"name": "bad", "feature": "z_bad"}
        ]
    }))
    .unwrap();
    let mut engine = BuildEngine::default().with_cache_limits(1, u64::MAX);
    assert!(
        matches!(engine.build(&failing), Err(BuildError::Kernel { feature, .. }) if feature == "z_bad")
    );
    let usage = engine.cache_usage();
    assert_eq!(usage.entries, 1);
    assert_eq!(usage.pinned_entries, 0);
    let next = engine.build(&plate(90.0)).unwrap();
    assert_eq!(next.stats.computed_solids, 1);
    assert_eq!(engine.cache_usage().evictions, 1);
    assert_eq!(engine.cache_usage().entries, 1);
}
