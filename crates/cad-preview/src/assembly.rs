//! Compact display assets retain shared local geometry and rigid occurrences.
//! Software rasterization expands bounded transient draw arrays; this is the
//! baseline renderer, not a GPU instance renderer or exact B-rep viewer.
use super::{render_html, render_svg, scene, Face, PreviewError, RecipeContext, Scene};
use cad_assembly::{BuiltAssembly, Instance};
use cad_kernel::Tolerance;
use serde::Serialize;
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
}

fn assembly_scene(assembly: &BuiltAssembly) -> Result<AssemblyScene, PreviewError> {
    assembly
        .validate(Tolerance::default())
        .map_err(|error| PreviewError::Assembly(error.to_string()))?;
    if assembly.instances.len() > MAX_INSTANCES {
        return Err(PreviewError::Assembly(
            "display exceeds 10,000 occurrences".into(),
        ));
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
        if faces > MAX_FACES || points > MAX_POINTS {
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
    })
}

/// Versioned display JSON: one local face mesh per content hash plus instances.
/// Material volume sums occurrences; overlaps are counted, not Boolean-unioned.
pub fn to_assembly_scene_json(assembly: &BuiltAssembly) -> Result<String, PreviewError> {
    Ok(serde_json::to_string_pretty(&assembly_scene(assembly)?)?)
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
    render_html(&assembly_scene(assembly)?, Some(recipe))
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
    render_svg(&expand(&assembly_scene(assembly)?)?)
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
        let bounds = assembly_scene(&assembly).unwrap().bounds.unwrap();
        for (actual, expected) in bounds
            .into_iter()
            .flatten()
            .zip([0.0, 0.0, 0.0, 10.0, 3.0, 4.0])
        {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn rotated_draw_points_and_normals_match_analytic_quarter_turn() {
        let compact = assembly_scene(&fixture()).unwrap();
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
        let compact = assembly_scene(&overlapping).unwrap();
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
