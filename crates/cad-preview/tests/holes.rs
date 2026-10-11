use cad_kernel::{extrude_polygon_with_holes, Tolerance};
use serde_json::Value;

#[test]
fn display_triangles_preserve_hole_area_and_native_boundary_edges() {
    let model = extrude_polygon_with_holes(
        &[[0., 0.], [20., 0.], [20., 10.], [0., 10.]],
        &[vec![[2., 2.], [2., 6.], [6., 6.], [6., 2.]]],
        3.,
        Tolerance::default(),
    )
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
        assert_eq!(face["loop_starts"], serde_json::json!([0, 4]));
        assert_eq!(face["edges"].as_array().unwrap().len(), 8);
        let points: Vec<[f64; 3]> = serde_json::from_value(face["points"].clone()).unwrap();
        let triangles: Vec<[usize; 3]> = serde_json::from_value(face["triangles"].clone()).unwrap();
        let area: f64 = triangles
            .iter()
            .map(|&[a, b, c]| {
                let p = points[a];
                let q = points[b];
                let r = points[c];
                ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])).abs() / 2.
            })
            .sum();
        assert!((area - 184.).abs() < 1e-10);
        assert_eq!(triangles.len(), 8);
        assert!(triangles.iter().all(|t| {
            let cx = t.iter().map(|&i| points[i][0] / 3.).sum::<f64>();
            let cy = t.iter().map(|&i| points[i][1] / 3.).sum::<f64>();
            !(cx > 2. && cx < 6. && cy > 2. && cy < 6.)
        }));
    }
    assert!(cad_preview::to_svg(&model)
        .unwrap()
        .contains("fill-rule=\"evenodd\""));
}

#[test]
fn streamed_hole_triangle_counts_force_legacy_fan_clients_to_fail_closed() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/perforated_mount.star"),
    )
    .unwrap();
    let recipe = cad_recipe::from_starlark("perforated_mount.star", &source).unwrap();
    let built = cad_build::build(&recipe).unwrap();
    let assembly = &built.assemblies["perforated_mount"];
    let context = cad_preview::RecipeContext {
        language: "starlark".into(),
        source,
        source_name: "perforated_mount.star".into(),
        output_name: "perforated_mount".into(),
        feature_id: assembly.assembly_id.clone(),
        parameters: Default::default(),
        content_hash: assembly.content_hash.clone(),
        recipe_context_hash: "context".into(),
    };
    let bundle = cad_preview::to_assembly_stream_bundle(assembly, &context, true).unwrap();
    let metadata: Value = serde_json::from_str(&bundle.scene_json).unwrap();
    let descriptor = &metadata["mesh_assets"][&assembly.parts["plate"].content_hash];
    let asset = bundle
        .assets
        .iter()
        .find(|a| a.name == descriptor["url"].as_str().unwrap())
        .unwrap();
    let mesh: Value = serde_json::from_str(&asset.content).unwrap();
    assert_eq!(mesh["schema_version"], 2);
    assert!(mesh.get("faces").is_none()); // Legacy clients must reject before upload.
    let faces = mesh["mesh"]["faces"].as_array().unwrap();
    let old_fan_bytes: usize = faces
        .iter()
        .map(|f| (f["points"].as_array().unwrap().len() - 2) * 3 * 28)
        .sum();
    let actual_bytes: usize = faces
        .iter()
        .map(|f| {
            f.get("triangles")
                .map(|t| t.as_array().unwrap().len())
                .unwrap_or_else(|| f["points"].as_array().unwrap().len() - 2)
                * 3
                * 28
        })
        .sum();
    assert_eq!(descriptor["gpu_bytes"], actual_bytes);
    assert_eq!(actual_bytes - old_fan_bytes, 8 * 3 * 28); // 2 caps × 2 holes × 2 extra triangles
                                                          // Legacy schema2 loaders compare their fan count to gpu_bytes before upload;
                                                          // this mandatory mismatch blocks silent filling even without a new schema.
    assert_ne!(descriptor["gpu_bytes"], old_fan_bytes);
}

#[test]
fn custom_tolerance_flows_through_body_and_all_assembly_renderers() {
    let tolerance = Tolerance { linear: 1e-12 };
    let result = extrude_polygon_with_holes(
        &[[0., 0.], [2e-6, 0.], [2e-6, 1e-6], [0., 1e-6]],
        &[vec![[2e-8, 2e-8], [2e-8, 6e-7], [6e-7, 6e-7], [6e-7, 2e-8]]],
        1e-6,
        tolerance,
    )
    .unwrap();
    assert!(cad_preview::to_html(&result.model).is_err());
    assert!(cad_preview::to_html_with_tolerance(&result.model, tolerance).is_ok());
    assert!(cad_preview::to_svg_with_tolerance(&result.model, tolerance).is_ok());
    let assembly = cad_assembly::BuiltAssembly::new(
        "small".into(),
        std::collections::BTreeMap::from([(
            "part".into(),
            cad_assembly::BuiltPart {
                feature_id: "small".into(),
                content_hash: "small".into(),
                result: std::sync::Arc::new(result),
                metadata: Default::default(),
                datums: Default::default(),
            },
        )]),
        vec![cad_assembly::Instance {
            id: "small".into(),
            part: "part".into(),
            transform: Default::default(),
            attachment: None,
        }],
        tolerance,
    )
    .unwrap();
    let context = cad_preview::RecipeContext {
        language: "starlark".into(),
        source: "# tiny solid".into(),
        source_name: "tiny.star".into(),
        output_name: "small".into(),
        feature_id: "small".into(),
        parameters: Default::default(),
        content_hash: assembly.content_hash.clone(),
        recipe_context_hash: "small-context".into(),
    };
    assert!(cad_preview::to_assembly_html_with_tolerance(&assembly, &context, tolerance).is_ok());
    assert!(cad_preview::to_assembly_svg_with_tolerance(&assembly, tolerance).is_ok());
    assert!(cad_preview::to_assembly_scene_json_with_tolerance(&assembly, tolerance).is_ok());
    assert!(cad_preview::to_assembly_gpu_scene_json_with_tolerance(&assembly, tolerance).is_ok());
    assert!(
        cad_preview::to_assembly_gpu_html_with_fallback_and_tolerance(
            &assembly, &context, true, tolerance
        )
        .is_ok()
    );
    assert!(cad_preview::to_assembly_stream_bundle_with_tolerance(
        &assembly, &context, true, tolerance
    )
    .is_ok());
}
