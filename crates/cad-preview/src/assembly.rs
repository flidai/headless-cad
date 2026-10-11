//! Compact display assets retain shared local geometry and rigid occurrences.
//! Software previews expand bounded transient arrays. The separate opt-in GPU
//! viewer retains shared meshes with a larger occurrence budget. Neither browser
//! viewer loads or evaluates the exact B-rep.
use super::{
    render_html, render_html_template, render_svg, scene, Face, PreviewError, RecipeContext, Scene,
};
use cad_assembly::{BuiltAssembly, Instance};
use cad_kernel::Tolerance;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_INSTANCES: usize = 10_000;
const MAX_FACES: usize = 50_000;
const MAX_POINTS: usize = 200_000;

#[derive(Serialize)]
struct Definition {
    mesh: String,
    feature_id: String,
    metadata: cad_assembly::PartMetadata,
    datums: BTreeMap<String, cad_assembly::RigidTransform>,
}

#[derive(Serialize)]
struct AssemblyScene {
    schema_version: u32,
    kind: &'static str,
    assembly_id: String,
    content_hash: String,
    meshes: BTreeMap<String, Scene>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    mesh_assets: BTreeMap<String, MeshAssetDescriptor>,
    definitions: BTreeMap<String, Definition>,
    instances: Vec<Instance>,
    instance_paths: BTreeMap<String, Vec<String>>,
    assembly_definitions: usize,
    total_mass_kg: Option<f64>,
    missing_material_occurrences: usize,
    bounds: Option<[[f64; 3]; 2]>,
    material_volume: f64,
    solids: usize,
    display_faces: usize,
    display_points: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    portable_preview_available: Option<bool>,
}

/// Immutable UTF-8 display payload. The SHA-256 in its filename addresses
/// exactly `content.as_bytes()`; this is display geometry, never an exact B-rep.
#[derive(Debug, Clone)]
pub struct DisplayMeshAsset {
    pub name: String,
    pub content: String,
}

/// Metadata-only streamed scene, viewer HTML, and shared immutable mesh assets.
#[derive(Debug, Clone)]
pub struct StreamBundle {
    pub scene_json: String,
    pub html: String,
    pub assets: Vec<DisplayMeshAsset>,
}

#[derive(Serialize)]
struct MeshAssetDescriptor {
    url: String,
    sha256: String,
    byte_length: usize,
    bounds: Option<[[f64; 3]; 2]>,
    faces: usize,
    points: usize,
    vertices: usize,
    gpu_bytes: usize,
}

fn assembly_scene(assembly: &BuiltAssembly, gpu: bool) -> Result<AssemblyScene, PreviewError> {
    assembly
        .validate(Tolerance::default())
        .map_err(|error| PreviewError::Assembly(error.to_string()))?;
    let instance_limit = if gpu { 100_000 } else { MAX_INSTANCES };
    if assembly.instances.len() > instance_limit {
        return Err(PreviewError::Assembly(format!(
            "display exceeds {instance_limit} occurrences"
        )));
    }
    let mut meshes = BTreeMap::new();
    let mut definitions = BTreeMap::new();
    let mut source_models = BTreeMap::new();
    let mut unique_faces = 0usize;
    let mut unique_points = 0usize;
    for (id, part) in &assembly.parts {
        if let Some(previous) = source_models.insert(&part.content_hash, &part.result.model) {
            if previous != &part.result.model {
                return Err(PreviewError::Assembly(
                    "one content hash refers to different local models".into(),
                ));
            }
        }
        if !meshes.contains_key(&part.content_hash) {
            let local = scene(&part.result.model)?;
            unique_faces += local.faces.len();
            unique_points += local
                .faces
                .iter()
                .map(|face| face.points.len())
                .sum::<usize>();
            if unique_faces > MAX_FACES || unique_points > MAX_POINTS {
                return Err(PreviewError::Assembly(
                    "local display meshes exceed 50,000 faces or 200,000 face points".into(),
                ));
            }
            meshes.insert(part.content_hash.clone(), local);
        }
        definitions.insert(
            id.clone(),
            Definition {
                mesh: part.content_hash.clone(),
                feature_id: part.feature_id.clone(),
                metadata: part.metadata.clone(),
                datums: part.datums.clone(),
            },
        );
    }
    let mut faces = 0usize;
    let mut points = 0usize;
    for instance in &assembly.instances {
        let mesh = &meshes[&definitions[&instance.part].mesh];
        faces = faces
            .checked_add(mesh.faces.len())
            .ok_or_else(|| PreviewError::Assembly("display count overflow".into()))?;
        points = points
            .checked_add(mesh.faces.iter().map(|face| face.points.len()).sum())
            .ok_or_else(|| PreviewError::Assembly("display count overflow".into()))?;
        if !gpu && (faces > MAX_FACES || points > MAX_POINTS) {
            return Err(PreviewError::Assembly(
                "software display exceeds 50,000 faces or 200,000 face points".into(),
            ));
        }
    }
    let material_volume = assembly.material_volume();
    let bounds = assembly.bounds().map(|bounds| [bounds.min, bounds.max]);
    if !material_volume.is_finite()
        || bounds
            .iter()
            .flatten()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return Err(PreviewError::Assembly(
            "display measurements exceed numerical range".into(),
        ));
    }
    let bom = assembly
        .bill_of_materials(Tolerance::default())
        .map_err(|error| PreviewError::Assembly(error.to_string()))?;
    Ok(AssemblyScene {
        schema_version: 1,
        kind: "assembly",
        assembly_id: assembly.assembly_id.clone(),
        content_hash: assembly.content_hash.clone(),
        meshes,
        mesh_assets: BTreeMap::new(),
        definitions,
        instances: assembly.instances.clone(),
        instance_paths: assembly.instance_paths.clone(),
        assembly_definitions: assembly.hierarchy.len(),
        total_mass_kg: bom.total_mass_kg,
        missing_material_occurrences: bom.missing_material_occurrences,
        bounds,
        material_volume,
        solids: assembly.solid_count(),
        display_faces: faces,
        display_points: points,
        portable_preview_available: None,
    })
}

/// Versioned display JSON: one local face mesh per content hash plus instances.
/// Material volume sums occurrences; overlaps are counted, not Boolean-unioned.
pub fn to_assembly_scene_json(assembly: &BuiltAssembly) -> Result<String, PreviewError> {
    Ok(serde_json::to_string_pretty(&assembly_scene(
        assembly, false,
    )?)?)
}

/// A standalone occurrence-aware viewer with exact source/provenance context.
pub fn to_assembly_html(
    assembly: &BuiltAssembly,
    recipe: &RecipeContext,
) -> Result<String, PreviewError> {
    if recipe.content_hash != assembly.content_hash {
        return Err(PreviewError::Recipe(
            "recipe output identity does not match assembly",
        ));
    }
    render_html(&assembly_scene(assembly, false)?, Some(recipe))
}

/// Compact GPU display JSON with up to 100,000 occurrences. Unique local meshes
/// remain bounded; instance geometry is not expanded into software face arrays.
pub fn to_assembly_gpu_scene_json(assembly: &BuiltAssembly) -> Result<String, PreviewError> {
    Ok(serde_json::to_string_pretty(&assembly_scene(
        assembly, true,
    )?)?)
}

/// Opt-in WebGL2 occurrence viewer, preserving shared meshes on the GPU.
pub fn to_assembly_gpu_html(
    assembly: &BuiltAssembly,
    recipe: &RecipeContext,
) -> Result<String, PreviewError> {
    to_assembly_gpu_html_with_fallback(assembly, recipe, false)
}

/// Declare whether the published artifact set includes the portable CPU viewer.
/// The explicit flag avoids network probes and enables local-file fallback links.
pub fn to_assembly_gpu_html_with_fallback(
    assembly: &BuiltAssembly,
    recipe: &RecipeContext,
    portable_preview_available: bool,
) -> Result<String, PreviewError> {
    if recipe.content_hash != assembly.content_hash {
        return Err(PreviewError::Recipe(
            "recipe output identity does not match assembly",
        ));
    }
    let mut scene = assembly_scene(assembly, true)?;
    scene.portable_preview_available = Some(portable_preview_available);
    render_html_template(&scene, Some(recipe), include_str!("gpu-viewer.html"))
}

/// Produce schema-2 metadata and immutable shared mesh payloads. Unique mesh
/// budgets remain the same as embedded GPU previews; no face arrays appear in
/// the HTML or scene metadata. Assets are fetched only by the streamed viewer.
pub fn to_assembly_stream_bundle(
    assembly: &BuiltAssembly,
    recipe: &RecipeContext,
    portable_preview_available: bool,
) -> Result<StreamBundle, PreviewError> {
    if recipe.content_hash != assembly.content_hash {
        return Err(PreviewError::Recipe(
            "recipe output identity does not match assembly",
        ));
    }
    let mut scene = assembly_scene(assembly, true)?;
    scene.schema_version = 2;
    scene.portable_preview_available = Some(portable_preview_available);
    let mut assets = BTreeMap::<String, String>::new();
    for (hash, mesh) in std::mem::take(&mut scene.meshes) {
        let content = serde_json::to_string(&mesh)?;
        let sha256: String = Sha256::digest(content.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let name = format!("mesh-{sha256}.display.json");
        let descriptor = MeshAssetDescriptor {
            url: name.clone(),
            sha256,
            byte_length: content.len(),
            bounds: mesh.bounds,
            faces: mesh.faces.len(),
            points: mesh.faces.iter().map(|face| face.points.len()).sum(),
            vertices: mesh.vertices,
            gpu_bytes: mesh
                .faces
                .iter()
                .map(|face| (face.points.len().saturating_sub(2)) * 3 * 7 * 4)
                .sum(),
        };
        if let Some(previous) = assets.insert(name, content.clone()) {
            if previous != content {
                return Err(PreviewError::Assembly(
                    "display payload SHA-256 collision".into(),
                ));
            }
        }
        scene.mesh_assets.insert(hash, descriptor);
    }
    Ok(StreamBundle {
        scene_json: serde_json::to_string_pretty(&scene)?,
        html: render_html_template(&scene, Some(recipe), include_str!("gpu-viewer.html"))?,
        assets: assets
            .into_iter()
            .map(|(name, content)| DisplayMeshAsset { name, content })
            .collect(),
    })
}

fn expand(scene: &AssemblyScene) -> Result<Scene, PreviewError> {
    let mut result = Scene {
        faces: vec![],
        crease_edges: vec![],
        vertices: 0,
        edges: 0,
        solids: scene.solids,
        bounds: scene.bounds,
        volume: Some(scene.material_volume),
    };
    for instance in &scene.instances {
        let mesh = &scene.meshes[&scene.definitions[&instance.part].mesh];
        let edge_offset = result.edges;
        for face in &mesh.faces {
            let points: Vec<_> = face
                .points
                .iter()
                .map(|point| instance.transform.point(*point))
                .collect();
            let normal = instance.transform.vector(face.normal);
            if points
                .iter()
                .flatten()
                .chain(normal.iter())
                .any(|value| !value.is_finite())
            {
                return Err(PreviewError::Assembly(
                    "transformed display coordinates exceed numerical range".into(),
                ));
            }
            result.faces.push(Face {
                id: result.faces.len(),
                points,
                edges: face.edges.iter().map(|id| id + edge_offset).collect(),
                normal,
            });
        }
        result
            .crease_edges
            .extend(mesh.crease_edges.iter().map(|id| id + edge_offset));
        result.vertices += mesh.vertices;
        result.edges += mesh.edges;
    }
    Ok(result)
}

/// Deterministic isometric thumbnail; meshes expand only while drawing the SVG.
pub fn to_assembly_svg(assembly: &BuiltAssembly) -> Result<String, PreviewError> {
    render_svg(&expand(&assembly_scene(assembly, false)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_assembly::{BuiltPart, RigidTransform};
    use cad_kernel::extrude_rectangle;
    use std::sync::Arc;

    fn fixture() -> BuiltAssembly {
        let part = BuiltPart {
            datums: Default::default(),
            metadata: Default::default(),
            feature_id: "blank".into(),
            content_hash: "shared-geometry".into(),
            result: Arc::new(extrude_rectangle(2.0, 3.0, 4.0, Tolerance::default()).unwrap()),
        };
        BuiltAssembly::new(
            "pair".into(),
            BTreeMap::from([
                ("block".into(), part.clone()),
                ("rotated_definition".into(), part),
            ]),
            vec![
                Instance {
                    attachment: None,
                    id: "original".into(),
                    part: "block".into(),
                    transform: RigidTransform::default(),
                },
                Instance {
                    attachment: None,
                    id: "turned".into(),
                    part: "rotated_definition".into(),
                    transform: RigidTransform {
                        translation: [10.0, 0.0, 0.0],
                        rotation: [
                            0.0,
                            0.0,
                            std::f64::consts::FRAC_1_SQRT_2,
                            std::f64::consts::FRAC_1_SQRT_2,
                        ],
                    },
                },
            ],
            Tolerance::default(),
        )
        .unwrap()
    }

    #[test]
    fn display_shares_meshes_and_preserves_definition_occurrence_identity() {
        let assembly = fixture();
        let value: serde_json::Value =
            serde_json::from_str(&to_assembly_scene_json(&assembly).unwrap()).unwrap();
        assert_eq!(value["kind"], "assembly");
        assert_eq!(value["meshes"].as_object().unwrap().len(), 1);
        assert_eq!(value["definitions"].as_object().unwrap().len(), 2);
        assert_eq!(value["instances"].as_array().unwrap().len(), 2);
        assert!(value["instances"][0].get("faces").is_none());
        assert_eq!(value["material_volume"], 48.0);
        assert_eq!(value["instances"][1]["id"], "turned");
        assert_eq!(value["instances"][1]["part"], "rotated_definition");
        assert_eq!(value["display_faces"], 12);
        let bounds = assembly_scene(&assembly, false).unwrap().bounds.unwrap();
        for (actual, expected) in bounds
            .into_iter()
            .flatten()
            .zip([0.0, 0.0, 0.0, 10.0, 3.0, 4.0])
        {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn stream_bundle_has_only_metadata_and_sha_addressed_shared_payloads() {
        let assembly = fixture();
        let recipe = RecipeContext {
            language: "starlark".into(),
            source: "# </script><img src=x>".into(),
            source_name: "parts.star".into(),
            output_name: "parts".into(),
            feature_id: "pair".into(),
            parameters: BTreeMap::new(),
            content_hash: assembly.content_hash.clone(),
            recipe_context_hash: "source".into(),
        };
        let bundle = to_assembly_stream_bundle(&assembly, &recipe, true).unwrap();
        assert_eq!(bundle.assets.len(), 1);
        let metadata: serde_json::Value = serde_json::from_str(&bundle.scene_json).unwrap();
        assert_eq!(metadata["schema_version"], 2);
        assert_eq!(metadata["meshes"], serde_json::json!({}));
        assert_eq!(metadata["instances"].as_array().unwrap().len(), 2);
        let descriptor = &metadata["mesh_assets"]["shared-geometry"];
        let asset = &bundle.assets[0];
        assert_eq!(descriptor["byte_length"], asset.content.len());
        assert_eq!(descriptor["faces"], 6);
        assert_eq!(descriptor["points"], 24);
        assert_eq!(descriptor["vertices"], 8);
        assert_eq!(descriptor["gpu_bytes"], 1008);
        assert_eq!(
            descriptor["bounds"],
            serde_json::json!([[0., 0., 0.], [2., 3., 4.]])
        );
        assert_eq!(
            asset.name,
            format!(
                "mesh-{}.display.json",
                descriptor["sha256"].as_str().unwrap()
            )
        );
        assert!(descriptor["sha256"]
            .as_str()
            .unwrap()
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        let expected: String = Sha256::digest(asset.content.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(descriptor["sha256"], expected);
        let local: serde_json::Value = serde_json::from_str(&asset.content).unwrap();
        assert_eq!(local["faces"].as_array().unwrap().len(), 6);
        assert!(!bundle.html.contains("</script><img"));
        let embedded = bundle
            .html
            .split("<script id=\"cad-scene-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(embedded).unwrap(),
            metadata
        );
        let repeated = to_assembly_stream_bundle(&assembly, &recipe, true).unwrap();
        assert_eq!(bundle.scene_json, repeated.scene_json);
        assert_eq!(bundle.html, repeated.html);
        assert_eq!(bundle.assets[0].content, repeated.assets[0].content);
    }

    #[test]
    fn gpu_scene_keeps_one_mesh_beyond_software_occurrence_and_face_limits() {
        let original = fixture();
        let instances = (0..10_001)
            .map(|index| Instance {
                id: format!("occurrence_{index}"),
                part: "block".into(),
                attachment: None,
                transform: RigidTransform::default(),
            })
            .collect();
        let large = BuiltAssembly::new(
            "large".into(),
            original.parts,
            instances,
            Tolerance::default(),
        )
        .unwrap();
        assert!(to_assembly_scene_json(&large).is_err());
        let compact: serde_json::Value =
            serde_json::from_str(&to_assembly_gpu_scene_json(&large).unwrap()).unwrap();
        assert_eq!(compact["instances"].as_array().unwrap().len(), 10_001);
        assert_eq!(compact["meshes"].as_object().unwrap().len(), 1);
        assert_eq!(compact["display_faces"], 60_006);
        assert_eq!(
            compact["meshes"]["shared-geometry"]["faces"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }

    #[test]
    fn rotated_draw_points_and_normals_match_analytic_quarter_turn() {
        let compact = assembly_scene(&fixture(), false).unwrap();
        let expanded = expand(&compact).unwrap();
        assert_eq!(expanded.faces.len(), 12);
        let face = &expanded.faces[6];
        for (actual, expected) in face
            .points
            .iter()
            .flatten()
            .zip([10.0, 0.0, 0.0, 7.0, 0.0, 0.0, 7.0, 2.0, 0.0, 10.0, 2.0, 0.0])
        {
            assert!((actual - expected).abs() < 1e-12);
        }
        for (actual, expected) in expanded.faces[9].normal.into_iter().zip([0.0, 1.0, 0.0]) {
            assert!((actual - expected).abs() < 1e-12);
        }
        assert_eq!(expanded.faces[6].edges.iter().min().copied(), Some(12));
        let svg = to_assembly_svg(&fixture()).unwrap();
        assert!(svg.contains("2 solid(s)"));
        assert_eq!(svg, to_assembly_svg(&fixture()).unwrap());
    }

    #[test]
    fn assembly_html_embeds_compact_scene_and_full_shared_source() {
        let assembly = fixture();
        let context = RecipeContext {
            language: "starlark".into(),
            source: "# assembly recipe\n".into(),
            source_name: "pair.star".into(),
            output_name: "pair".into(),
            feature_id: "pair".into(),
            parameters: BTreeMap::new(),
            content_hash: assembly.content_hash.clone(),
            recipe_context_hash: "source-hash".into(),
        };
        let html = to_assembly_html(&assembly, &context).unwrap();
        let mut hostile = context.clone();
        hostile.source = "# </script><img src=x onerror=alert(1)>".into();
        let gpu = to_assembly_gpu_html(&assembly, &hostile).unwrap();
        assert!(gpu.contains("\"portable_preview_available\":false"));
        let with_fallback = to_assembly_gpu_html_with_fallback(&assembly, &hostile, true).unwrap();
        assert!(with_fallback.contains("\"portable_preview_available\":true"));
        assert!(!with_fallback.contains("</script><img"));
        assert!(!to_assembly_gpu_scene_json(&assembly)
            .unwrap()
            .contains("portable_preview_available"));
        assert!(!gpu.contains("</script><img"));
        assert!(gpu.contains("\\u003c/script\\u003e"));
        assert!(!gpu.contains("/*SCENE_DATA*/null"));
        assert!(!gpu.contains("/*RECIPE_DATA*/null"));
        let raw = html
            .lines()
            .find_map(|line| {
                line.strip_prefix("<script id=\"cad-scene-data\" type=\"application/json\">")
            })
            .unwrap()
            .trim_end_matches("</script>");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(value.get("faces").is_none());
        assert_eq!(value["meshes"].as_object().unwrap().len(), 1);
        assert!(html.contains("assembly recipe"));
        assert!(html.contains("local_face_id"));
        assert!(html.contains("Material volume (sum)"));
        let mut mismatch = context.clone();
        mismatch.content_hash = "another-assembly".into();
        assert!(matches!(
            to_assembly_html(&assembly, &mismatch),
            Err(PreviewError::Recipe(_))
        ));
    }

    #[test]
    fn coincident_occurrences_retain_material_accounting_and_display_limits_are_explicit() {
        let original = fixture();
        let overlapping = BuiltAssembly::new(
            "overlap".into(),
            original.parts.clone(),
            original
                .instances
                .iter()
                .cloned()
                .map(|mut instance| {
                    instance.transform = RigidTransform::default();
                    instance
                })
                .collect(),
            Tolerance::default(),
        )
        .unwrap();
        let compact = assembly_scene(&overlapping, false).unwrap();
        assert_eq!(compact.material_volume, 48.0);
        assert_eq!(compact.bounds, Some([[0.0, 0.0, 0.0], [2.0, 3.0, 4.0]]));
        assert_eq!(compact.instances.len(), 2);
        let instances = (0..8_334)
            .map(|index| Instance {
                attachment: None,
                id: format!("occurrence_{index}"),
                part: "block".into(),
                transform: RigidTransform::default(),
            })
            .collect();
        let oversized = BuiltAssembly::new(
            "oversized".into(),
            original.parts,
            instances,
            Tolerance::default(),
        )
        .unwrap();
        assert!(
            matches!(to_assembly_scene_json(&oversized), Err(PreviewError::Assembly(message)) if message.contains("50,000 faces"))
        );
    }
}
