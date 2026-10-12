use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::{json, Value};

fn value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[
        {"op":"rectangle","id":"profile","width":20,"depth":10},
        {"op":"extrude","id":"body","profile":"profile","height":8},
        {"op":"rectangle","id":"other_profile","width":3,"depth":4},
        {"op":"extrude","id":"other_body","profile":"other_profile","height":5}
    ],"outputs":[{"name":"part","feature":"body"},{"name":"other","feature":"other_body"}],
    "section_views":[{"id":"plan","output":"part","origin":[0,0,4],"normal":[0,0,1],"x_direction":[1,0,0]}],
    "drawing_sheets":[{"id":"drawing","title":"Mounting plate","revision":"A","width":210,"height":297,
        "views":[{"view":"plan","x":10,"y":10,"width":190,"height":252,"scale":2}]}]})
}
fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}

#[test]
fn selected_build_keeps_global_sheet_sections_and_title_edit_reuses_geometry() {
    let mut recipe = parse(value());
    let mut engine = BuildEngine::default();
    let first = engine.build_outputs(&recipe, &["other".into()]).unwrap();
    assert!(!first.outputs.contains_key("part"));
    assert_eq!(first.section_views["plan"].query.section.area, 200.);
    let sheet = &first.drawing_sheets["drawing"];
    assert_eq!(sheet.status, "layout_validated");
    assert_eq!(sheet.section_view_ids, ["plan"]);
    assert_eq!(sheet.declaration.views[0].scale, 2.);
    assert_eq!(first.stats.computed_solids, 2);
    recipe.drawing_sheets[0].title = "Revised mounting plate".into();
    recipe.drawing_sheets[0].revision = "B".into();
    let second = engine.build_outputs(&recipe, &["other".into()]).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(second.drawing_sheets["drawing"].declaration.revision, "B");
    assert_eq!(first.feature_hashes, second.feature_hashes);
    recipe.section_views[0].min_area = Some(201.);
    assert!(
        engine.build_outputs(&recipe, &["other".into()]).is_err(),
        "unselected sheet section remains an engineering obligation on cache hits"
    );
}

#[test]
fn invalid_layout_or_reference_fails_before_geometry_even_for_unselected_output() {
    for (field, invalid) in [
        ("id", json!("../escape")),
        ("id", json!("CON")),
        ("title", json!("")),
        ("revision", json!("A\nB")),
        ("width", json!(2001)),
        ("height", json!(0)),
    ] {
        let mut v = value();
        v["drawing_sheets"][0][field] = invalid;
        v["features"][0]["width"] = json!(-1);
        assert!(matches!(
            BuildEngine::default().build_outputs(&parse(v), &["other".into()]),
            Err(BuildError::InvalidDrawingSheet { .. })
        ));
    }
    for (field, invalid) in [
        ("view", json!("missing")),
        ("x", json!(9.)),
        ("y", json!(9.)),
        ("width", json!(191.)),
        ("height", json!(253.)),
        ("scale", json!(0.)),
    ] {
        let mut v = value();
        v["drawing_sheets"][0]["views"][0][field] = invalid;
        v["features"][0]["width"] = json!(-1);
        assert!(
            matches!(
                BuildEngine::default().build_outputs(&parse(v), &["other".into()]),
                Err(BuildError::InvalidDrawingSheet { .. })
            ),
            "field {field}"
        );
    }
    let mut recipe = parse(value());
    recipe.drawing_sheets[0].views[0].scale = f64::NAN;
    assert!(matches!(
        cad_build::build(&recipe),
        Err(BuildError::InvalidDrawingSheet { .. })
    ));
}

#[test]
fn strict_schema_and_unique_sheet_and_view_identifiers() {
    for version in [1, 2] {
        let mut v = value();
        v["schema_version"] = json!(version);
        if version == 1 {
            v.as_object_mut().unwrap().remove("section_views");
            v["drawing_sheets"] = json!([]);
        } else {
            v["drawing_sheets"] = Value::Null;
        }
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut v = value();
    v["drawing_sheets"][0]["unknown"] = json!(true);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    let mut v = value();
    v["drawing_sheets"][0]["views"][0]["unknown"] = json!(true);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    let mut recipe = parse(value());
    let mut duplicate = recipe.drawing_sheets[0].clone();
    duplicate.id = "DRAWING".into();
    recipe.drawing_sheets.push(duplicate);
    assert!(cad_build::build(&recipe).is_err());
    let mut recipe = parse(value());
    let viewport = recipe.drawing_sheets[0].views[0].clone();
    recipe.drawing_sheets[0].views.push(viewport);
    assert!(cad_build::build(&recipe).is_err());
}

#[test]
fn touching_viewports_are_legal_but_overlap_is_rejected() {
    let mut v = value();
    let mut section = v["section_views"][0].clone();
    section["id"] = json!("side");
    v["section_views"].as_array_mut().unwrap().push(section);
    v["drawing_sheets"][0]["views"] = json!([
        {"view":"plan","x":10,"y":10,"width":90,"height":90,"scale":1},
        {"view":"side","x":100,"y":10,"width":100,"height":90,"scale":0.5}]);
    let mut recipe = parse(v);
    assert!(cad_build::build(&recipe).is_ok());
    recipe.drawing_sheets[0].views[1].x = 99.999;
    assert!(cad_build::build(&recipe).is_err());
}

#[test]
fn sheet_and_total_viewport_limits_are_preflight_obligations() {
    let mut recipe = parse(value());
    let template = recipe.drawing_sheets[0].clone();
    recipe.drawing_sheets = (0..17)
        .map(|i| {
            let mut s = template.clone();
            s.id = format!("s{i}");
            s
        })
        .collect();
    assert!(matches!(
        cad_build::build(&recipe),
        Err(BuildError::ResourceLimit {
            resource: "drawing sheets",
            ..
        })
    ));
    let mut recipe = parse(value());
    let section = recipe.section_views[0].clone();
    recipe.section_views = (0..32)
        .map(|i| {
            let mut s = section.clone();
            s.id = format!("v{i}");
            s
        })
        .collect();
    let mut sheet = template;
    sheet.width = 2000.;
    sheet.height = 2000.;
    sheet.views = (0..32)
        .map(|i| cad_build::DrawingViewport {
            view: format!("v{i}"),
            x: 10. + i as f64 * 50.,
            y: 10.,
            width: 40.,
            height: 40.,
            scale: 1.,
        })
        .collect();
    recipe.drawing_sheets = (0..9)
        .map(|i| {
            let mut s = sheet.clone();
            s.id = format!("s{i}");
            s
        })
        .collect();
    assert!(matches!(
        cad_build::build(&recipe),
        Err(BuildError::ResourceLimit {
            resource: "drawing viewports",
            ..
        })
    ));
}
