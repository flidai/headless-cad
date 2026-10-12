use cad_build::{BuildEngine, Feature, Recipe};
const SOURCE: &str = r#"
d = param(name="setback",default=1.25)
p = rectangle(id="p",width=20,depth=12)
b = extrude(id="stock",profile=p,height=10)
c = chamfer(id="bevel",body=b,face_normals=[[1,0,0],[0,0,1]],distance=d)
output(name="bevel",feature=c)
"#;
#[test]
fn chamfer_starlark_parameters_roundtrip_and_regenerate() {
    let r = cad_recipe::from_starlark("chamfer.star", SOURCE).unwrap();
    let encoded = serde_json::to_string(&r).unwrap();
    let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    assert!(matches!(
        decoded.features[2],
        Feature::Chamfer { distance: 1.25, .. }
    ));
    let mut engine = BuildEngine::default();
    let first = engine.build(&decoded).unwrap();
    assert!(
        (first.outputs["bevel"].result.model.volume() - (2400. - 6. * 1.25 * 1.25)).abs() < 1e-8
    );
    let edited = cad_recipe::from_starlark_with_params(
        "chamfer.star",
        SOURCE,
        &std::collections::BTreeMap::from([("setback".into(), 2.)]),
    )
    .unwrap();
    let changed = engine.build(&edited).unwrap();
    assert_eq!(changed.stats.computed_solids, 1);
    assert_ne!(
        first.outputs["bevel"].content_hash,
        changed.outputs["bevel"].content_hash
    );
}
#[test]
fn chamfer_frontend_rejects_malformed_normal_criteria() {
    for replacement in [
        "[[2,0,0],[0,0,1]]",
        "[[1,0],[0,0,1]]",
        "[[1,0,0]]",
        "[1,0,0]",
        "[[1,0,0],[0,0,1],[0,1,0]]",
    ] {
        assert!(cad_recipe::from_starlark(
            "bad.star",
            &SOURCE.replace("[[1,0,0],[0,0,1]]", replacement)
        )
        .is_err());
    }
}
