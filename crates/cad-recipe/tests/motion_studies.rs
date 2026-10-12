#[test]
fn motion_study_source_compiles_and_remains_a_global_obligation() {
    let source = r#"
m=mechanism(id="m",pins=[{"id":"a","initial":[0,0],"ground":[0,0]},{"id":"b","initial":[8,6]}],links=[{"id":"bar","from":"a","to":"b","length":10}],drives=[{"id":"drive","link":"bar","angle_radians":0.6}])
motion_study(id="sweep",mechanism=m,targets=[{"drive":0.6},{"drive":0.9}],max_drive_step_radians=0.1,max_pin_step_mm=2)
output(name="body",feature=extrude(id="body",profile=rectangle(id="p",width=10,depth=2),height=1))
"#;
    let recipe = cad_recipe::from_starlark("motion.star", source).unwrap();
    assert_eq!(recipe.schema_version, 2);
    assert_eq!(recipe.motion_studies.len(), 1);
    let build = cad_build::build(&recipe).unwrap();
    assert_eq!(
        build.motion_studies["sweep"]
            .solution
            .completed_requested_frames,
        2
    );
    assert_eq!(build.stats.computed_solids, 1);
    let invalid = source.replace("max_pin_step_mm=2", "max_pin_step_mm=0");
    assert!(cad_build::build(&cad_recipe::from_starlark("bad.star", &invalid).unwrap()).is_err());
    let duplicate=source.replace("output(name=", "motion_study(id=\"sweep\",mechanism=m,targets=[],max_drive_step_radians=1,max_pin_step_mm=1)\noutput(name=");
    assert!(cad_recipe::from_starlark("bad.star", &duplicate).is_err());
}
