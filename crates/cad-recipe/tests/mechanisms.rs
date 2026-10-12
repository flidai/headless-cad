use cad_recipe::from_starlark;
#[test]
fn starlark_mechanism_and_binding_compile_and_solve() {
    let source = r#"
m=mechanism(id="m",pins=[{"id":"a","initial":[0,0],"ground":[0,0]},{"id":"b","initial":[8,6]}],links=[{"id":"link","from":"a","to":"b","length":10}],drives=[{"id":"drive","link":"link","angle_radians":0.6}],offset=[1,2,3])
p=part(id="p",body=extrude(id="body",profile=rectangle(id="profile",width=10,depth=2),height=1))
a=assembly(id="a",instances=[instance(id="leaf",part=p)])
mechanism_bind(assembly=a,instance="leaf",mechanism=m,link="link")
assembly_output(name="scene",assembly=a)
"#;
    let recipe = from_starlark("motion.star", source).unwrap();
    assert_eq!(recipe.schema_version, 2);
    assert_eq!(recipe.mechanism_bindings.len(), 1);
    let built = cad_build::build(&recipe).unwrap();
    assert_eq!(
        built.assemblies["scene"].instances[0].transform.translation,
        [1., 2., 3.]
    );
    assert_eq!(built.mechanism_reports.len(), 1);
    let roundtrip: cad_build::Recipe =
        serde_json::from_slice(&serde_json::to_vec(&recipe).unwrap()).unwrap();
    assert_eq!(
        built.mechanism_context_hash,
        cad_build::build(&roundtrip).unwrap().mechanism_context_hash
    );
    for broken in [
        source.replace("angle_radians", "typo"),
        source.replace("mechanism=m", "mechanism=\"missing\""),
        source.replace("instance=\"leaf\"", "instance=\"LEAF\""),
    ] {
        assert!(from_starlark("bad.star", &broken)
            .map(|r| cad_build::build(&r).is_err())
            .unwrap_or(true));
    }
}
