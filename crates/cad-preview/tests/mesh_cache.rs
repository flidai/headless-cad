use cad_mesh::GenerationMeshCache;
use cad_preview::*;
const SOURCE: &str = r#"
body=extrude(id='body',profile=circle(id='profile',radius=10,inner_radius=3),height=8)
p=part(id='part',body=body)
output(name='body',feature=body)
assembly_output(name='pair',assembly=assembly(id='pair',instances=[instance(id='first',part=p),instance(id='second',part=p,offset=[30,0,0])]))
"#;
fn context(hash: &str) -> RecipeContext {
    RecipeContext {
        language: "starlark".into(),
        source: SOURCE.into(),
        source_name: "pair.star".into(),
        modules: Default::default(),
        source_bundle_hash: None,
        source_provenance: None,
        output_name: "pair".into(),
        feature_id: "pair".into(),
        parameters: Default::default(),
        content_hash: hash.into(),
        recipe_context_hash: "captured-source".into(),
        property_assertions: Default::default(),
        native_engineering_checks: Default::default(),
    }
}
#[test]
fn one_generation_reuses_body_and_assembly_meshes_without_changing_artifacts() {
    let recipe = cad_recipe::from_starlark("pair.star", SOURCE).unwrap();
    let built = cad_build::build(&recipe).unwrap();
    let model = &built.outputs["body"].result.model;
    let assembly = &built.assemblies["pair"];
    let recipe = context(&assembly.content_hash);
    let tol = Default::default();
    let mut cache = GenerationMeshCache::default();
    assert_eq!(
        to_svg_with_tolerance(model, tol).unwrap(),
        to_svg_with_cache(model, tol, &mut cache).unwrap()
    );
    assert_eq!(
        to_html_with_recipe_and_tolerance(model, &recipe, tol).unwrap(),
        to_html_with_recipe_and_cache(model, &recipe, tol, &mut cache).unwrap()
    );
    assert_eq!(
        to_html_with_tolerance(model, tol).unwrap(),
        to_html_with_cache(model, tol, &mut cache).unwrap()
    );
    assert_eq!(
        to_assembly_scene_json_with_tolerance(assembly, tol).unwrap(),
        to_assembly_scene_json_with_cache(assembly, tol, &mut cache).unwrap()
    );
    assert_eq!(
        to_assembly_gpu_scene_json_with_tolerance(assembly, tol).unwrap(),
        to_assembly_gpu_scene_json_with_cache(assembly, tol, &mut cache).unwrap()
    );
    assert_eq!(
        to_assembly_html_with_tolerance(assembly, &recipe, tol).unwrap(),
        to_assembly_html_with_cache(assembly, &recipe, tol, &mut cache).unwrap()
    );
    assert_eq!(
        to_assembly_gpu_html_with_fallback_and_tolerance(assembly, &recipe, true, tol).unwrap(),
        to_assembly_gpu_html_with_fallback_and_cache(assembly, &recipe, true, tol, &mut cache)
            .unwrap()
    );
    assert_eq!(
        to_assembly_svg_with_tolerance(assembly, tol).unwrap(),
        to_assembly_svg_with_cache(assembly, tol, &mut cache).unwrap()
    );
    assert_eq!(cache.stats().misses, 1);
    assert!(cache.stats().hits >= 7);
    let uncached = to_assembly_stream_bundle_with_tolerance(assembly, &recipe, true, tol).unwrap();
    let cached =
        to_assembly_stream_bundle_with_cache(assembly, &recipe, true, tol, &mut cache).unwrap();
    assert_eq!(uncached.html, cached.html);
    assert_eq!(uncached.scene_json, cached.scene_json);
    assert_eq!(
        uncached
            .assets
            .iter()
            .map(|a| (&a.name, &a.content))
            .collect::<Vec<_>>(),
        cached
            .assets
            .iter()
            .map(|a| (&a.name, &a.content))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        cache.stats().misses,
        3,
        "default and two LOD policies must have distinct meshes"
    );
    let misses = cache.stats().misses;
    to_assembly_stream_bundle_with_cache(assembly, &recipe, true, tol, &mut cache).unwrap();
    assert_eq!(cache.stats().misses, misses);
}
#[test]
fn polygon_cap_triangulations_share_the_same_generation_cache() {
    let source="body=extrude(id='body',profile=polygon(id='p',points=[[0,0],[20,0],[20,20],[0,20]],holes=[[[5,5],[5,10],[10,10],[10,5]]]),height=3)\noutput(name='body',feature=body)";
    let recipe = cad_recipe::from_starlark("polygon.star", source).unwrap();
    let built = cad_build::build(&recipe).unwrap();
    let model = &built.outputs["body"].result.model;
    let mut cache = GenerationMeshCache::default();
    let svg = to_svg_with_cache(model, Default::default(), &mut cache).unwrap();
    assert_eq!(svg, to_svg(model).unwrap());
    let misses = cache.stats().misses;
    assert_eq!(misses, 2);
    let html = to_html_with_cache(model, Default::default(), &mut cache).unwrap();
    assert_eq!(html, to_html(model).unwrap());
    assert_eq!(cache.stats().misses, misses);
    assert_eq!(cache.stats().hits, 2);
}
