use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::{json, Value};
fn value(z: f64) -> Value {
    json!({"schema_version":2,"units":"mm","features":[
        {"op":"circle","id":"shaft_profile","radius":6},
        {"op":"extrude","id":"stock","profile":"shaft_profile","height":40},
        {"op":"rectangle","id":"tool_profile","width":4,"depth":4.04},
        {"op":"extrude","id":"tool_blank","profile":"tool_profile","height":13.5},
        {"op":"translate","id":"tool","body":"tool_blank","offset":[4,-2.02,10]},
        {"op":"cut","id":"shaft","left":"stock","right":"tool"},
        {"op":"circle","id":"sleeve_profile","radius":9,"inner_radius":6.15},
        {"op":"extrude","id":"sleeve","profile":"sleeve_profile","height":4}
    ],"outputs":[{"name":"stock","feature":"stock"}],
    "parts":[{"id":"shaft_part","feature":"shaft"},{"id":"sleeve_part","feature":"sleeve"}],
    "assemblies":[{"id":"pair","instances":[{"id":"shaft","part":"shaft_part"},{"id":"sleeve","part":"sleeve_part","transform":{"translation":[0,0,z]}}]}],
    "assembly_outputs":[{"name":"pair","assembly":"pair"}],
    "clearance_checks":[{"id":"gap","output":"pair","left_instance_path":["shaft"],"right_instance_path":["sleeve"],"min_clearance":0.1}]})
}
#[test]
fn journal_seat_and_boundary_queries_have_real_material_witnesses() {
    for z in [2., 8., 12., 22.] {
        let r: Recipe = serde_json::from_value(value(z)).unwrap();
        let built = cad_build::build(&r).unwrap();
        let q = &built.clearance_checks["gap"].query;
        assert_eq!(
            serde_json::to_value(q.method).unwrap(),
            json!("native_trim_witness")
        );
        assert!(q.lower_bound_mm < 0.15 && q.upper_bound_mm > 0.15);
        assert!(q.upper_bound_mm - q.lower_bound_mm < 1e-7);
        let p = q.left_point;
        assert!((p[0].hypot(p[1]) - 6.).abs() < 1e-8);
        assert!(p[2] >= z - 1e-8 && p[2] <= z + 4. + 1e-8);
        // Independent material membership: shaft keyseat removes x>4,
        // |y|<2.02, 10<z<23.5. No witness may lie inside that removed region.
        assert!(
            !(p[0] > 4. + 1e-8
                && p[1].abs() < 2.02 - 1e-8
                && p[2] > 10. + 1e-8
                && p[2] < 23.5 - 1e-8)
        );
        let p = q.right_point;
        assert!((p[0].hypot(p[1]) - 6.15).abs() < 1e-8);
        assert!(p[2] >= z - 1e-8 && p[2] <= z + 4. + 1e-8);
    }
}
#[test]
fn selected_output_keeps_clearance_obligations_and_threshold_edits_reuse_solids() {
    let mut engine = BuildEngine::default();
    let mut r: Recipe = serde_json::from_value(value(12.)).unwrap();
    let first = engine.build_outputs(&r, &["stock".into()]).unwrap();
    assert!(first.assemblies.is_empty());
    assert_eq!(first.clearance_checks.len(), 1);
    r.clearance_checks[0].min_clearance = 0.16;
    assert!(matches!(
        engine.build_outputs(&r, &["stock".into()]),
        Err(BuildError::ClearanceCheckFailed { .. })
    ));
    r.clearance_checks[0].min_clearance = 0.12;
    let cached = engine.build_outputs(&r, &["stock".into()]).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.feature_hashes, first.feature_hashes);
    for offset in [[0.01, 0., 12.], [0., 0., 12.]] {
        let mut v = value(12.);
        v["assemblies"][0]["instances"][1]["transform"]["translation"] = json!(offset);
        if offset[0] == 0. {
            v["assemblies"][0]["instances"][1]["transform"]["rotation"] =
                json!([0.01, 0., 0., (1_f64 - 0.0001).sqrt()]);
        }
        assert!(cad_build::build(&serde_json::from_value::<Recipe>(v).unwrap()).is_err());
    }
}
