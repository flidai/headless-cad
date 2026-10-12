const SOURCE:&str="a=extrude(id='a',profile=circle(id='pa',radius=3),height=8)\noutput(name='a',feature=a)\nsection_view(id='av',output='a',origin=[0,0,4])\nsection_circle_dimension(view='av',id='dia',from_feature=a,role='wall',label_offset=[10,10],tolerance={'nominal':6,'lower':-0.1,'upper':0.1})\n";
#[test]
fn source_stack_roundtrips_and_evaluates_native_dimension_witness() {
    let r=cad_recipe::from_starlark("stack.star",&format!("{SOURCE}tolerance_stack(id='negative',terms=[{{'view':'av','dimension':'dia','coefficient':-2}}],min=-12.3,max=-11.7)")).unwrap();
    assert_eq!(r.schema_version, 2);
    let decoded: cad_build::Recipe =
        serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(r.tolerance_stacks, decoded.tolerance_stacks);
    let b = cad_build::build(&r).unwrap();
    let s = &b.tolerance_stacks["negative"];
    assert_eq!(s.nominal_mm, -12.);
    assert_eq!(s.terms[0].native_value_mm, 6.);
}
#[test]
fn invalid_stack_authoring_is_rejected() {
    for args in [
        "id='s',terms=[]",
        "id='s',terms=[{'view':'av','dimension':'dia','coefficient':0}],min=0",
        "id='s',terms=[{'view':'av','dimension':'dia','coefficient':1,'extra':0}],min=0",
        "id='s',terms=[['av','dia',1]],min=0",
        "id='s',terms=[{'view':'av','dimension':'dia','coefficient':1}]",
        "id='s',terms=[{'view':'av','dimension':'dia','coefficient':1}],min=2,max=1",
    ] {
        assert!(
            cad_recipe::from_starlark("bad.star", &format!("{SOURCE}tolerance_stack({args})"))
                .is_err(),
            "{args}"
        );
    }
}
