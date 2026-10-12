const SOURCE:&str="b=extrude(id='b',profile=circle(id='p',radius=3),height=8)\noutput(name='body',feature=b)\npartdef=part(id='part',body=b,datums={'center':datum(offset=[0,0,4])})\nassembly_output(name='machine',assembly=assembly(id='root',instances=[instance(id='left',part=partdef),instance(id='right',part=partdef,offset=[20,0,0])]))\nsection_view(id='body_view',output='body',origin=[0,0,4])\nsection_view(id='assembly_view',output='machine',origin=[0,0,4])\n";
const LINEAR:&str="section_dimension(view='assembly_view',id='pitch',from_instance_path=['left'],from_datum='center',to_instance_path=['right'],to_datum='center',offset=-10,tolerance={'nominal':20,'lower':0.1,'upper':0.2})\n";
const CIRCLE:&str="section_circle_dimension(view='body_view',id='diameter',from_feature=b,role='wall',label_offset=[10,10],tolerance={'nominal':6,'lower':-0.1,'upper':0.2})\n";
#[test]
fn authored_tolerance_dicts_roundtrip_and_report_native_limits() {
    let r =
        cad_recipe::from_starlark("tolerances.star", &format!("{SOURCE}{LINEAR}{CIRCLE}")).unwrap();
    let bytes = serde_json::to_vec(&r).unwrap();
    let restored: cad_build::Recipe = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.section_views, r.section_views);
    let b = cad_build::build(&r).unwrap();
    assert_eq!(
        b.section_views["assembly_view"].dimensions[0]
            .tolerance
            .as_ref()
            .unwrap()
            .lower_limit_mm,
        20.1
    );
    assert_eq!(
        b.section_views["body_view"].circle_dimensions[0]
            .tolerance
            .as_ref()
            .unwrap()
            .upper_limit_mm,
        6.2
    );
}
#[test]
fn malformed_tolerances_reject_and_none_preserves_legacy_semantics() {
    for directive in [LINEAR, CIRCLE] {
        let start = directive.find("tolerance=").unwrap();
        for invalid in [
            "{'nominal':6,'lower':0,'upper':0}",
            "{'nominal':6,'lower':-6,'upper':1}",
            "{'nominal':6,'lower':0,'upper':1,'extra':2}",
            "{'nominal':6,'lower':0}",
            "[6,0,1]",
        ] {
            let source = format!("{SOURCE}{}tolerance={invalid})", &directive[..start]);
            assert!(cad_recipe::from_starlark("bad.star", &source).is_err());
        }
        let r = cad_recipe::from_starlark(
            "none.star",
            &format!("{SOURCE}{}tolerance=None)", &directive[..start]),
        )
        .unwrap();
        assert!(cad_build::build(&r).is_ok());
    }
}
