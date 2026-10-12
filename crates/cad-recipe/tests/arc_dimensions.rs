const SOURCE: &str = include_str!("../../../examples/rounded_bearing_cover.star");
#[test]
fn cover_recipe_exposes_true_arc_radii_and_legacy_complete_bore() {
    let recipe = cad_recipe::from_starlark("cover.star", SOURCE).unwrap();
    let raw = serde_json::to_value(&recipe).unwrap();
    assert!(raw["section_views"][0]["circle_dimensions"][0]
        .get("coverage")
        .is_none());
    let replay: cad_build::Recipe = serde_json::from_value(raw).unwrap();
    let built = cad_build::build(&replay).unwrap();
    let dims = &built.section_views["cover_midsection"].circle_dimensions;
    assert_eq!(dims.len(), 5);
    assert_eq!(dims[0].value_mm, 20.);
    assert!(dims[0].arc.is_none());
    for d in &dims[1..] {
        assert_eq!(d.value_mm, 4.);
        assert_eq!(
            d.declaration.coverage,
            cad_build::SectionCircleCoverage::SingleArc
        );
        assert!(d.arc.is_some());
    }
    let changed = SOURCE.replace("default = 4)", "default = 6)");
    let built =
        cad_build::build(&cad_recipe::from_starlark("changed.star", &changed).unwrap()).unwrap();
    assert_eq!(
        built.section_views["cover_midsection"].circle_dimensions[1].value_mm,
        6.
    );
}
#[test]
fn source_refuses_silent_partial_coverage_and_wrong_kind() {
    for source in [
        SOURCE.replace("coverage = \"single_arc\", ", ""),
        SOURCE.replace(
            "kind = \"radius\", coverage",
            "kind = \"diameter\", coverage",
        ),
        SOURCE.replace("coverage = \"single_arc\"", "coverage = \"partial_guess\""),
    ] {
        let recipe = cad_recipe::from_starlark("bad.star", &source);
        assert!(recipe.is_err() || cad_build::build(&recipe.unwrap()).is_err());
    }
}
