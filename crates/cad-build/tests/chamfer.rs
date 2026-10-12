use cad_build::*;
use serde_json::json;

fn recipe(distance: f64) -> Recipe {
    serde_json::from_value(json!({"schema_version":2,"units":"mm","features":[
        {"op":"rectangle","id":"profile","width":20,"depth":12},
        {"op":"extrude","id":"stock","profile":"profile","height":10},
        {"op":"chamfer","id":"bevel","body":"stock","face_normals":[[1,0,0],[0,0,1]],"distance":distance}
    ],"outputs":[{"name":"stock","feature":"stock"},{"name":"finished","feature":"bevel"}]})).unwrap()
}
fn select(anchor: &str, role: &str, target: &str) -> FaceSelector {
    FaceSelector {
        anchor_feature: anchor.into(),
        role: role.into(),
        target_feature: target.into(),
        cardinality: FaceCardinality::Exactly { count: 1 },
    }
}
#[test]
fn chamfer_regeneration_cache_and_complete_original_face_lineage() {
    let mut engine = BuildEngine::default();
    let r = recipe(2.);
    let first = engine.build(&r).unwrap();
    assert!((first.outputs["finished"].result.model.volume() - (2400. - 24.)).abs() < 1e-8);
    assert_eq!(first.outputs["finished"].result.model.faces.len(), 7);
    let history = &first.outputs["finished"].result.history;
    assert_eq!(history.input_faces["body"].len(), 6);
    assert!(history.input_faces["body"]
        .values()
        .all(|faces| faces.len() == 1));
    for (anchor, role) in [
        ("stock", "end_cap"),
        ("stock", "start_cap"),
        ("bevel", "chamfer"),
    ] {
        assert_eq!(
            first.resolve_faces(&select(anchor, role, "bevel")).status,
            FaceResolutionStatus::Resolved
        );
    }
    let cached = engine.build(&r).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(
        first.outputs["finished"].content_hash,
        cached.outputs["finished"].content_hash
    );
    let edited = engine.build(&recipe(1.)).unwrap();
    assert_eq!(edited.stats.computed_solids, 1);
    assert_eq!(
        first.outputs["stock"].content_hash,
        edited.outputs["stock"].content_hash
    );
    assert_ne!(
        first.outputs["finished"].content_hash,
        edited.outputs["finished"].content_hash
    );
    assert!((edited.outputs["finished"].result.model.volume() - 2394.).abs() < 1e-8);
}
#[test]
fn chained_chamfer_preserves_generated_face_selection_and_selected_assertions() {
    let mut r = recipe(2.);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    r.features.push(Feature::Chamfer {
        id: "second".into(),
        body: "bevel".into(),
        face_normals: [[s, 0., s], [0., 1., 0.]],
        distance: 0.25,
        op_version: 1,
    });
    r.outputs.push(Output {
        name: "second".into(),
        feature: "second".into(),
    });
    let mut engine = BuildEngine::default();
    let b = engine.build(&r).unwrap();
    assert_eq!(
        b.resolve_faces(&select("bevel", "chamfer", "second"))
            .status,
        FaceResolutionStatus::Resolved
    );
    assert_eq!(
        b.resolve_faces(&select("second", "chamfer", "second"))
            .status,
        FaceResolutionStatus::Resolved
    );
    assert_eq!(
        b.resolve_faces(&select("stock", "end_cap", "second"))
            .status,
        FaceResolutionStatus::Resolved
    );
    let mut value = serde_json::to_value(&r).unwrap();
    value["face_assertions"] = json!([{"id":"keep_bevel","selector":{"anchor_feature":"bevel","role":"chamfer","target_feature":"second","cardinality":{"kind":"exactly","count":1}}}]);
    let with_assertion: Recipe = serde_json::from_value(value.clone()).unwrap();
    assert!(engine
        .build_outputs(&with_assertion, &["stock".into()])
        .is_ok());
    value["face_assertions"][0]["selector"]["role"] = json!("missing_role");
    let invalid: Recipe = serde_json::from_value(value).unwrap();
    assert!(engine.build_outputs(&invalid, &["stock".into()]).is_err());
}
#[test]
fn malformed_chamfer_normals_distance_and_ir_fields_reject() {
    for normals in [
        [[2., 0., 0.], [0., 0., 1.]],
        [[0., 0., 0.], [0., 0., 1.]],
        [[1., 0., 0.], [-1., 0., 0.]],
        [[1., 0., 0.], [1., 0., 0.]],
    ] {
        let mut r = recipe(1.);
        let Feature::Chamfer { face_normals, .. } = &mut r.features[2] else {
            unreachable!()
        };
        *face_normals = normals;
        assert!(build(&r).is_err());
    }
    for distance in [0., -1., 10., f64::NAN] {
        assert!(build(&recipe_finite_or_nan(distance)).is_err());
    }
    let mut value = serde_json::to_value(recipe(1.)).unwrap();
    value["features"][2]["nearest_edge"] = json!(true);
    assert!(serde_json::from_value::<Recipe>(value).is_err());
}
fn recipe_finite_or_nan(distance: f64) -> Recipe {
    let mut r = recipe(1.);
    if let Feature::Chamfer { distance: d, .. } = &mut r.features[2] {
        *d = distance;
    }
    r
}
