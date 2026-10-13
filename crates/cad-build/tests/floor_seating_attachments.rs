use cad_build::{BuildEngine, Recipe};
use serde_json::{json, Value};
fn fixture(bottom: bool) -> Value {
    let role = if bottom { "start_cap" } else { "end_cap" };
    json!({"schema_version":2,"units":"mm","features":[
        {"op":"rectangle","id":"outline","width":100,"depth":80},
        {"op":"extrude","id":"stock","profile":"outline","height":12},
        {"op":"blind_slot_pattern","id":"pockets","body":"stock","face":{"from_feature":"stock","role":role},"depth":4,"frame_origin":[0,0,if bottom {0} else {12}],"pattern_slots":[{"id":"seat","center":[30,40],"direction":[0,1],"centerline_length":24,"radius":6}]},
        {"op":"circle","id":"pad_profile","radius":3}, {"op":"extrude","id":"pad_body","profile":"pad_profile","height":6}],
        "outputs":[{"name":"stock","feature":"stock"}],
        "parts":[{"id":"plate","feature":"pockets"},{"id":"pad","feature":"pad_body","datums":{"mount":{}}}],
        "face_datums":[{"part":"plate","name":"floor","anchor_feature":"pockets","role":"slot_seat_floor","x_direction":[1,0,0]},{"part":"pad","name":"seat","anchor_feature":"pad_body","role":"start_cap","x_direction":[1,0,0]}],
        "assemblies":[{"id":"cell","instances":[{"id":"plate","part":"plate"},{"id":"pad","part":"pad","attachment":{"instance":"plate","datum":"mount","target_datum":"floor"}}]}, {"id":"root","assemblies":[{"id":"placed","assembly":"cell"}]}],
        "assembly_outputs":[{"name":"root","assembly":"root"}],
        "nominal_seating_checks":[{"id":"seat","output":"root","left":{"instance_path":["placed","pad"],"datum":"seat"},"right":{"instance_path":["placed","plate"],"datum":"floor"},"min_shared_disk_radius_mm":2}]})
}
fn run(v: Value) -> Result<cad_build::BuildResult, cad_build::BuildError> {
    let r: Recipe = serde_json::from_value(v).unwrap();
    BuildEngine::default().build_outputs(&r, &["stock".into()])
}
#[test]
fn floor_attachment_is_native_global_and_pose_bound() {
    for bottom in [false, true] {
        for posed in [false, true] {
            let mut v = fixture(bottom);
            if posed {
                v["assemblies"][1]["assemblies"][0]["transform"] = json!({"translation":[20,-7,4],"rotation":[0.5,0.5,0.,std::f64::consts::FRAC_1_SQRT_2]});
            }
            let b = run(v).unwrap();
            assert!(b.nominal_seating_checks["seat"].disk_radius_lower_mm > 2.);
            assert!(b.assemblies.is_empty());
        }
    }
}
#[test]
fn unsupported_attachment_operands_and_cancellation_refuse() {
    for case in 0..6 {
        let mut v = fixture(false);
        match case {
            0 => v["parts"][1]["datums"]["mount"] = json!({"translation":[1,0,0]}),
            1 => v["face_datums"][0]["x_direction"] = json!([1e-7, 0, 1]),
            2 => {
                v["assemblies"][0]["instances"][0]["transform"] = json!({"translation":[1e7,0,0]});
                v["assemblies"][1]["assemblies"][0]["transform"] =
                    json!({"translation":[-1e7,0,0]});
            }
            3 => {
                v["assemblies"][0]["instances"][1]["attachment"]["target_path"] = json!(["nested"])
            }
            4 => {
                v["assemblies"][0]["instances"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"anchor","part":"plate"}));
                v["assemblies"][0]["instances"][0]["attachment"] =
                    json!({"instance":"anchor","datum":"floor","target_datum":"floor"});
            }
            _ => {
                v["assemblies"][0]["instances"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"unrelated","part":"plate"}));
                v["assemblies"][0]["instances"][1]["attachment"]["instance"] = json!("unrelated");
            }
        }
        assert!(run(v).is_err(), "case {case}");
    }
}

#[test]
fn capsule_fit_is_independent_of_seating_global_and_cache_safe() {
    for bottom in [false, true] {
        let mut v = fixture(bottom);
        v["features"][3] = json!({"op":"path","id":"pad_profile","start":[3,-8],"segments":[{"kind":"line_to","end":[3,8]},{"kind":"arc_to","center":[0,8],"end":[-3,8]},{"kind":"line_to","end":[-3,-8]},{"kind":"arc_to","center":[0,-8],"end":[3,-8]}]});
        v["nominal_seating_checks"] = json!([]);
        v["pocket_fit_checks"] = json!([{"id":"fit","output":"root","pocket":{"instance_path":["placed","plate"],"datum":"floor"},"insert":{"instance_path":["placed","pad"],"datum":"seat"},"min_side_clearance_mm":2.9}]);
        let source: Recipe = serde_json::from_value(v.clone()).unwrap();
        let mut engine = BuildEngine::default();
        let result = engine.build_outputs(&source, &["stock".into()]).unwrap();
        assert!(result.assemblies.is_empty());
        let report = &result.pocket_fit_checks["fit"];
        assert!(report.side_clearance_lower_mm < 3. && report.side_clearance_upper_mm > 3.);
        assert_eq!(
            report.query.insert_floor_face,
            report.insert.semantic_datum.selection.witnesses[0].target_face
        );
        assert_eq!(
            engine
                .build_outputs(&source, &["stock".into()])
                .unwrap()
                .stats
                .computed_solids,
            0
        );
        let mut failed = source.clone();
        failed.pocket_fit_checks[0].min_side_clearance_mm = 3.1;
        assert!(matches!(
            engine.build_outputs(&failed, &["stock".into()]),
            Err(cad_build::BuildError::PocketFitCheckFailed { .. })
        ));
        let mut array = v.clone();
        array["pocket_fit_checks"][0] = json!(["fit","root",{"instance_path":["placed","plate"],"datum":"floor"},{"instance_path":["placed","pad"],"datum":"seat"},2.9]);
        assert!(serde_json::from_value::<Recipe>(array).is_err());
        let mut wrong_cap = v.clone();
        wrong_cap["face_datums"][1]["role"] = json!("end_cap");
        assert!(matches!(
            run(wrong_cap),
            Err(cad_build::BuildError::PocketFitCheckFailed { .. })
        ));
        let mut wrong_floor = v.clone();
        wrong_floor["face_datums"][0]["role"] = json!("end_cap");
        wrong_floor["face_datums"][0]["anchor_feature"] = json!("stock");
        assert!(run(wrong_floor).is_err());
        v["pocket_fit_checks"][0]["unknown"] = json!(1);
        assert!(serde_json::from_value::<Recipe>(v).is_err());
        failed.pocket_fit_checks[0].min_side_clearance_mm = -1.;
        assert!(matches!(
            engine.build(&failed),
            Err(cad_build::BuildError::InvalidPocketFitCheck { .. })
        ));
    }
}
