use cad_build::{BuildEngine, Feature, Recipe};
const SOURCE: &str = r#"
d = param(name="setback",default=1.25)
p = rectangle(id="p",width=20,depth=12)
b = extrude(id="stock",profile=p,height=10)
c = chamfer_faces(id="bevel",body=b,faces=[{"from_feature":b,"role":"wall_x_max"},{"from_feature":b,"role":"end_cap"}],distance=d)
output(name="bevel",feature=c)
"#;
#[test]
fn semantic_chamfer_starlark_roundtrip_and_parameter_edit() {
    let r = cad_recipe::from_starlark("semantic.star", SOURCE).unwrap();
    assert!(matches!(r.features[2], Feature::ChamferFaces { .. }));
    let encoded = serde_json::to_string(&r).unwrap();
    let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    let mut engine = BuildEngine::default();
    let first = engine.build(&decoded).unwrap();
    let next = cad_recipe::from_starlark_with_params(
        "semantic.star",
        SOURCE,
        &std::collections::BTreeMap::from([("setback".into(), 2.)]),
    )
    .unwrap();
    let changed = engine.build(&next).unwrap();
    assert_eq!(changed.stats.computed_solids, 1);
    assert_ne!(
        first.outputs["bevel"].content_hash,
        changed.outputs["bevel"].content_hash
    );
    assert!((changed.outputs["bevel"].result.model.volume() - 2376.).abs() < 1e-8);
}
#[test]
fn semantic_chamfer_starlark_rejects_malformed_face_objects() {
    for replacement in ["[]","[{\"from_feature\":b,\"role\":\"end_cap\"}]","[{\"from_feature\":b,\"role\":\"end_cap\",\"index\":1},{\"from_feature\":b,\"role\":\"wall_x_max\"}]","[[1,0,0],[0,0,1]]"] {
  let source=SOURCE.replace("[{\"from_feature\":b,\"role\":\"wall_x_max\"},{\"from_feature\":b,\"role\":\"end_cap\"}]",replacement);
  assert!(cad_recipe::from_starlark("bad.star",&source).is_err());
 }
}
