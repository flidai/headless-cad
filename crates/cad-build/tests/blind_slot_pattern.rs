use cad_build::{
    BuildEngine, BuildError, FaceCardinality, FaceResolutionStatus, FaceSelector, Recipe,
};
use serde_json::{json, Value};

fn recipe_value() -> Value {
    json!({
        "schema_version":2,"units":"mm",
        "features":[
            {"op":"rectangle","id":"outline","width":100,"depth":80},
            {"op":"extrude","id":"stock","profile":"outline","height":12},
            {"op":"blind_slot_pattern","id":"pockets","body":"stock",
                "face":{"from_feature":"stock","role":"end_cap"},
                "depth":4,"frame_origin":[0,0,12],
                "pattern_slots":[
                    {"id":"left","center":[30,40],"direction":[0,1],"centerline_length":24,"radius":6},
                    {"id":"right","center":[70,40],"direction":[0,1],"centerline_length":24,"radius":6}
                ]}
        ],
        "outputs":[{"name":"pockets","feature":"pockets"},{"name":"stock","feature":"stock"}]
    })
}

fn parsed(value: Value) -> Recipe {
    serde_json::from_value(value).unwrap()
}

#[test]
fn native_blind_slots_keep_named_walls_floors_and_cap_lineage() {
    let recipe = parsed(recipe_value());
    let build = BuildEngine::default()
        .build_outputs(&recipe, &["pockets".into()])
        .unwrap();
    let body = &build.outputs["pockets"].result.model;
    let expected = 100. * 80. * 12. - 2. * (24. * 12. + std::f64::consts::PI * 36.) * 4.;
    assert!((body.volume() - expected).abs() < 1e-6);
    for name in ["left", "right"] {
        let walls = build.resolve_faces(&FaceSelector {
            anchor_feature: "pockets".into(),
            role: format!("slot_{name}_walls"),
            target_feature: "pockets".into(),
            cardinality: FaceCardinality::All,
        });
        assert_eq!(walls.status, FaceResolutionStatus::Resolved);
        assert_eq!(walls.witnesses.len(), 4);
        let floor = build.resolve_faces(&FaceSelector {
            anchor_feature: "pockets".into(),
            role: format!("slot_{name}_floor"),
            target_feature: "pockets".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        });
        assert_eq!(floor.status, FaceResolutionStatus::Resolved);
        let plane = body.faces[floor.witnesses[0].target_face.0]
            .try_plane()
            .unwrap();
        assert!((plane.origin[2] - 8.).abs() < 1e-8);
    }
    let cap = build.resolve_faces(&FaceSelector {
        anchor_feature: "stock".into(),
        role: "start_cap".into(),
        target_feature: "pockets".into(),
        cardinality: FaceCardinality::Exactly { count: 1 },
    });
    assert_eq!(cap.status, FaceResolutionStatus::Resolved);
}

#[test]
fn blind_slot_depth_and_cap_role_bind_cache_identity() {
    let mut engine = BuildEngine::default();
    let mut recipe = parsed(recipe_value());
    let first = engine.build_outputs(&recipe, &["pockets".into()]).unwrap();
    let old_hash = first.feature_hashes["pockets"].clone();
    if let cad_build::Feature::BlindSlotPattern { depth, .. } = &mut recipe.features[2] {
        *depth = 5.;
    }
    let changed = engine.build_outputs(&recipe, &["pockets".into()]).unwrap();
    assert_ne!(changed.feature_hashes["pockets"], old_hash);
    if let cad_build::Feature::BlindSlotPattern { slots, .. } = &mut recipe.features[2] {
        slots.reverse();
    }
    let reordered = engine.build_outputs(&recipe, &["pockets".into()]).unwrap();
    assert_ne!(
        reordered.feature_hashes["pockets"],
        changed.feature_hashes["pockets"]
    );
    for name in ["left", "right"] {
        let floor = reordered.resolve_faces(&FaceSelector {
            anchor_feature: "pockets".into(),
            role: format!("slot_{name}_floor"),
            target_feature: "pockets".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        });
        assert_eq!(floor.status, FaceResolutionStatus::Resolved);
    }
    if let cad_build::Feature::BlindSlotPattern { face, .. } = &mut recipe.features[2] {
        face.role = "missing_cap".into();
    }
    assert!(matches!(
        engine.build_outputs(&recipe, &["pockets".into()]),
        Err(BuildError::Kernel { .. })
    ));
    let mut invalid = recipe_value();
    invalid["features"][2]["depth"] = json!(0);
    assert!(engine.build(&parsed(invalid)).is_err());
    let mut schema = recipe_value();
    schema["schema_version"] = json!(1);
    assert!(matches!(
        engine.build(&parsed(schema)),
        Err(BuildError::BlindSlotPatternSchema)
    ));
}
