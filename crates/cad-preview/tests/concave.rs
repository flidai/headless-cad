use cad_kernel::{extrude_polygon_with_holes, Tolerance};
use serde_json::Value;

#[test]
fn concave_single_loop_caps_use_explicit_triangles_without_diagonal_edges() {
    let outline = [
        [0., 0.],
        [20., 0.],
        [20., 10.],
        [13., 10.],
        [13., 4.],
        [7., 4.],
        [7., 10.],
        [0., 10.],
    ];
    let model = extrude_polygon_with_holes(&outline, &[], 3., Tolerance::default())
        .unwrap()
        .model;
    let html = cad_preview::to_html(&model).unwrap();
    let tag = "<script id=\"cad-scene-data-v2\" type=\"application/json\">";
    let data = html
        .split_once(tag)
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    let scene: Value = serde_json::from_str(data).unwrap();
    assert!(!html.contains("<script id=\"cad-scene-data\" type=\"application/json\">"));
    let caps: Vec<_> = scene["faces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f.get("triangles").is_some())
        .collect();
    assert_eq!(caps.len(), 2);
    for face in caps {
        assert!(face.get("loop_starts").is_none());
        assert_eq!(face["edges"].as_array().unwrap().len(), 8);
        let points: Vec<[f64; 3]> = serde_json::from_value(face["points"].clone()).unwrap();
        let triangles: Vec<[usize; 3]> = serde_json::from_value(face["triangles"].clone()).unwrap();
        assert_eq!(triangles.len(), 6);
        let area: f64 = triangles
            .iter()
            .map(|&[a, b, c]| {
                let p = points[a];
                let q = points[b];
                let r = points[c];
                ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])).abs() / 2.
            })
            .sum();
        assert!((area - 164.).abs() < 1e-10);
        assert!(triangles.iter().all(|t| {
            let x = t.iter().map(|&i| points[i][0] / 3.).sum::<f64>();
            let y = t.iter().map(|&i| points[i][1] / 3.).sum::<f64>();
            !(x > 7. && x < 13. && y > 4.)
        }));
    }
    // SVG's polygon fill preserves a simple concave boundary directly.
    assert!(cad_preview::to_svg(&model).unwrap().contains("<polygon"));
}

#[test]
fn concave_stream_envelope_rejects_legacy_fans_even_with_equal_triangle_counts() {
    let source = r#"
p = polygon(id="p", points=[[0,0],[20,0],[20,10],[13,10],[13,4],[7,4],[7,10],[0,10]])
b = extrude(id="body",profile=p,height=3)
a = part(id="plate",body=b)
s = assembly(id="assembly",instances=[instance(id="plate",part=a)])
assembly_output(name="assembly",assembly=s)
"#;
    let recipe = cad_recipe::from_starlark("concave.star", source).unwrap();
    let built = cad_build::build(&recipe).unwrap();
    let assembly = &built.assemblies["assembly"];
    let context = cad_preview::RecipeContext {
        language: "starlark".into(),
        source: source.into(),
        source_name: "concave.star".into(),
        modules: Default::default(),
        source_bundle_hash: None,
        source_provenance: None,
        output_name: "assembly".into(),
        feature_id: assembly.assembly_id.clone(),
        parameters: Default::default(),
        content_hash: assembly.content_hash.clone(),
        recipe_context_hash: "context".into(),
        property_assertions: Default::default(),
        native_engineering_checks: Default::default(),
    };
    let bundle = cad_preview::to_assembly_stream_bundle(assembly, &context, true).unwrap();
    let metadata: Value = serde_json::from_str(&bundle.scene_json).unwrap();
    let descriptor = &metadata["mesh_assets"][&assembly.parts["plate"].content_hash];
    let asset = bundle
        .assets
        .iter()
        .find(|a| a.name == descriptor["url"].as_str().unwrap())
        .unwrap();
    let payload: Value = serde_json::from_str(&asset.content).unwrap();
    assert_eq!(payload["schema_version"], 2);
    assert!(payload.get("faces").is_none());
    let fan_bytes: usize = payload["mesh"]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["points"].as_array().unwrap().len() - 2) * 3 * 28)
        .sum();
    assert_eq!(descriptor["gpu_bytes"], fan_bytes); // Counts alone cannot detect an old client.
}
