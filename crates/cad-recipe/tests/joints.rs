use cad_build::BuildEngine;
const SOURCE: &str = r#"
travel=param(name="travel",default=3)
body=extrude(id="body",profile=rectangle(id="p",width=4,depth=3),height=2)
p=part(id="part",body=body,datums={"base":datum(),"top":face_datum(from_feature=body,role="end_cap",x_direction=[1,0,0])})
module=assembly(id="module",instances=[instance(id="base",part=p),instance(id="slide",part=p,attach_to="base",datum="base",target_datum="top",joint=prismatic(position=travel,min=0,max=10))])
base=instance(id="root_base",part=p)
moving=assembly_instance(id="moving",assembly=module,attach_to="root_base",source_path=["base"],datum="base",target_datum="top",joint=revolute(angle=0.5,min=-1,max=1))
scene=assembly(id="root",instances=[base,moving])
assembly_output(name="scene",assembly=scene)
output(name="body",feature=body)
"#;
#[test]
fn starlark_joint_constructors_support_semantic_datums_and_rigid_module_motion() {
    let r = cad_recipe::from_starlark("joints.star", SOURCE).unwrap();
    let encoded = serde_json::to_string(&r).unwrap();
    let decoded: cad_build::Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    let mut engine = BuildEngine::default();
    let first = engine.build(&r).unwrap();
    assert_eq!(first.assemblies["scene"].instances.len(), 3);
    assert!(!first.face_datums.is_empty());
    let next = cad_recipe::from_starlark_with_params(
        "joints.star",
        SOURCE,
        &std::collections::BTreeMap::from([("travel".into(), 5.)]),
    )
    .unwrap();
    let changed = engine.build(&next).unwrap();
    assert_eq!(changed.stats.computed_solids, 0);
    assert_ne!(
        first.assemblies["scene"].content_hash,
        changed.assemblies["scene"].content_hash
    );
}
#[test]
fn joints_require_complete_attachment_finite_values_and_inclusive_limits() {
    for source in [
        "instance(id='x',part='p',joint=prismatic(position=1,min=0,max=2))",
        "assembly_instance(id='x',assembly='a',joint=revolute(angle=0,min=-1,max=1))",
        "revolute(angle=2,min=-1,max=1)",
        "prismatic(position=1,min=2,max=0)",
        "revolute(angle='NaN',min=-1,max=1)",
    ] {
        assert!(
            cad_recipe::from_starlark("bad.star", source).is_err(),
            "{source}"
        );
    }
    for joint in [
        "{'kind':'free','position':1,'min':0,'max':2}",
        "{'kind':'prismatic','position':1,'min':0,'max':2,'axis':[1,0,0]}",
    ] {
        assert!(cad_recipe::from_starlark(
            "bad.star",
            &SOURCE.replace("prismatic(position=travel,min=0,max=10)", joint)
        )
        .is_err());
    }
    assert!(cad_recipe::from_starlark(
        "bounds.star",
        "a=revolute(angle=-1,min=-1,max=1)\nb=prismatic(position=2,min=0,max=2)"
    )
    .is_ok());
    assert!(cad_recipe::from_starlark(
        "legacy.star",
        "instance(id='x',part='p',joint=None)\na=assembly(id='a',instances=['x'])"
    )
    .is_ok());
}
