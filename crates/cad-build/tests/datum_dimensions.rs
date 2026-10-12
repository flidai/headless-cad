use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::{json, Value};
fn value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":2,"depth":2},{"op":"extrude","id":"body","profile":"p","height":2}],
    "outputs":[{"name":"body","feature":"body"}],"parts":[{"id":"part","feature":"body"}],
    "face_datums":[{"part":"part","name":"top","anchor_feature":"body","role":"end_cap","x_direction":[1,0,0]},{"part":"part","name":"bottom","anchor_feature":"body","role":"start_cap","x_direction":[1,0,0]}],
    "assemblies":[{"id":"pair","instances":[{"id":"a","part":"part"},{"id":"b","part":"part","transform":{"translation":[50,0,5]}}]},{"id":"root","assemblies":[{"id":"cell","assembly":"pair"}]}],
    "assembly_outputs":[{"name":"root","assembly":"root"}],
    "datum_dimensions":[{"id":"gap","output":"root","from":{"instance_path":["cell","a"],"datum":"top"},"to":{"instance_path":["cell","b"],"datum":"bottom"},"direction":1,"tolerance":{"nominal":3,"lower":-0.01,"upper":0.01}}],
    "tolerance_stacks":[{"id":"stack","terms":[{"datum_dimension":"gap","coefficient":1}],"min":2.9,"max":3.1}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn support_plane_distance_is_directed_pose_invariant_global_and_cache_safe() {
    let mut engine = BuildEngine::default();
    let r = recipe(value());
    let first = engine.build_outputs(&r, &["body".into()]).unwrap();
    assert!(first.assemblies.is_empty());
    let d = &first.datum_dimensions["gap"];
    assert_eq!(d.value_mm, 3.);
    assert!(d.lower_bound_mm < 3. && d.upper_bound_mm > 3.);
    assert_eq!(d.from.semantic_datum.selection.witnesses.len(), 1);
    assert_eq!(d.from.part_content_hash, first.feature_hashes["body"]);
    let s = &first.tolerance_stacks["stack"];
    assert!(s.measurement_interval_certified && s.terms[0].measurement_interval_certified);
    assert!(s.measured_lower_bound_mm < 3. && s.measured_upper_bound_mm > 3.);
    assert!(s.lower_limit_mm < 2.99 && s.upper_limit_mm > 3.01);
    assert_eq!(
        engine
            .build_outputs(&r, &["body".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let mut posed = value();
    posed["assemblies"][1]["assemblies"][0]["transform"] =
        json!({"translation":[10,-20,30],"rotation":[0.5,0.5,0.,std::f64::consts::FRAC_1_SQRT_2]});
    let b = engine.build(&recipe(posed)).unwrap();
    let d = &b.datum_dimensions["gap"];
    assert!(d.lower_bound_mm < 3. && d.upper_bound_mm > 3.);
    let mut moved = value();
    moved["assemblies"][0]["instances"][1]["transform"]["translation"] = json!([50, 0, 5.02]);
    assert!(matches!(
        engine.build_outputs(&recipe(moved), &["body".into()]),
        Err(BuildError::DatumDimensionFailed { .. })
    ));
}
#[test]
fn reversed_crossed_tilted_offset_and_authored_coordinate_datums_fail() {
    for case in 0..6 {
        let mut v = value();
        match case {
            0 => v["datum_dimensions"][0]["direction"] = json!(-1),
            1 => v["assemblies"][0]["instances"][1]["transform"]["translation"] = json!([0, 0, 1]),
            2 => {
                v["assemblies"][0]["instances"][1]["transform"]["rotation"] =
                    json!([0.01, 0., 0., (1_f64 - 0.0001).sqrt()])
            }
            3 => v["face_datums"][0]["offset"] = json!([0.1, 0, 0]),
            4 => {
                v["datum_dimensions"][0]["from"]["datum"] = json!("coordinate");
                v["parts"][0]["datums"] = json!({"coordinate":{}});
            }
            _ => v["datum_dimensions"][0]["from"]["instance_path"] = json!(["cell/a"]),
        }
        assert!(cad_build::build(&recipe(v)).is_err(), "case {case}");
    }
}
#[test]
fn manufacturing_intervals_cannot_hide_measurement_arithmetic_uncertainty() {
    let mut v = value();
    v["datum_dimensions"][0]["tolerance"] = json!({"nominal":3,"lower":-1e-12,"upper":1e-12});
    v["tolerance_stacks"][0]["min"] = json!(3. - 2e-12);
    v["tolerance_stacks"][0]["max"] = json!(3. + 2e-12);
    let err = cad_build::build(&recipe(v)).unwrap_err();
    assert!(
        matches!(err,BuildError::ToleranceStackFailed{message,..} if message.contains("native measured"))
    );
    let mut v = value();
    v["datum_dimensions"][0]["min"] = json!(3.);
    assert!(matches!(
        cad_build::build(&recipe(v)),
        Err(BuildError::DatumDimensionFailed { .. })
    ));
}
#[test]
fn declarations_are_strict_bounded_and_do_not_mix_uncertified_sources() {
    for field in ["extra", "axis"] {
        let mut v = value();
        v["datum_dimensions"][0][field] = json!(true);
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut v = value();
    v["datum_dimensions"][0]["from"] = json!([["cell", "a"], "top"]);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    let mut v = value();
    v["datum_dimensions"] = json!([["gap", "root"]]);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    let mut null = value();
    null["datum_dimensions"] = Value::Null;
    assert!(serde_json::from_value::<Recipe>(null).is_err());
    let mut v = value();
    v["schema_version"] = json!(1);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    for direction in [0, 2] {
        let mut v = value();
        v["datum_dimensions"][0]["direction"] = json!(direction);
        assert!(matches!(
            cad_build::build(&recipe(v)),
            Err(BuildError::InvalidDatumDimension { .. })
        ));
    }
    let mut v = value();
    let d = v["datum_dimensions"][0].clone();
    v["datum_dimensions"] = Value::Array(
        (0..129)
            .map(|i| {
                let mut d = d.clone();
                d["id"] = json!(format!("d{i}"));
                d
            })
            .collect(),
    );
    assert!(matches!(
        cad_build::build(&recipe(v)),
        Err(BuildError::ResourceLimit {
            resource: "datum dimensions",
            ..
        })
    ));
    let mut v = value();
    v["tolerance_stacks"][0]["terms"]
        .as_array_mut()
        .unwrap()
        .push(json!({"view":"view","dimension":"size","coefficient":1}));
    assert!(
        matches!(cad_build::build(&recipe(v)),Err(BuildError::InvalidToleranceStack{message,..}) if message.contains("mixing"))
    );
    let mut v = value();
    v["tolerance_stacks"][0]["terms"][0]["view"] = json!("view");
    assert!(matches!(
        cad_build::build(&recipe(v)),
        Err(BuildError::InvalidToleranceStack { .. })
    ));
}

#[test]
fn huge_native_face_extent_cannot_hide_angular_departure_behind_small_support_origins() {
    let mut v = value();
    v["features"][0]["width"] = json!(10000);
    v["features"][0]["depth"] = json!(10000);
    v["assemblies"][0]["instances"][1]["transform"]["rotation"] = json!([2e-14, 0., 0., 1.]);
    let error = cad_build::build(&recipe(v)).unwrap_err();
    assert!(
        matches!(error, BuildError::DatumDimensionFailed{message,..} if message.contains("precision"))
    );
}
