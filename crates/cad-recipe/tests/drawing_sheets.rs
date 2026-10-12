const SOURCE: &str = "b=extrude(id='body',profile=rectangle(id='profile',width=20,depth=10),height=8)\noutput(name='part',feature=b)\nsection_view(id='plan',output='part',origin=[0,0,4])\n";
const SHEET: &str = "drawing_sheet(id='layout',title='Plate drawing',revision='A',width=210,height=297,views=[{'view':'plan','x':10,'y':10,'width':190,'height':252,'scale':2}])";
#[test]
fn source_sheet_retains_explicit_paper_layout_and_schema_two() {
    let recipe = cad_recipe::from_starlark("drawing.star", &format!("{SOURCE}{SHEET}")).unwrap();
    assert_eq!(recipe.schema_version, 2);
    assert_eq!(recipe.drawing_sheets[0].views[0].scale, 2.);
    assert_eq!(
        cad_build::build(&recipe).unwrap().drawing_sheets["layout"].section_view_ids,
        ["plan"]
    );
    let json = serde_json::to_string(&recipe).unwrap();
    let decoded: cad_build::Recipe = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.drawing_sheets, recipe.drawing_sheets);
}
#[test]
fn source_rejects_missing_refs_bad_viewport_shapes_and_duplicate_ids() {
    for sheet in [
        SHEET.replace("'plan'", "'missing'"),
        SHEET.replace("'scale':2", "'scale':0"),
        SHEET.replace("'scale':2", "'scale':2,'extra':1"),
        SHEET.replace("width=210", "width=9"),
        SHEET.replace("revision='A'", "revision=''"),
        SHEET.replace("id='layout'", "id='../bad'"),
    ] {
        assert!(cad_recipe::from_starlark("bad.star", &format!("{SOURCE}{sheet}")).is_err());
    }
    assert!(cad_recipe::from_starlark(
        "bad.star",
        &format!(
            "{SOURCE}{SHEET}\n{}",
            SHEET.replace("id='layout'", "id='LAYOUT'")
        )
    )
    .is_err());
}
