//! Self-contained previews derived from B-rep face loops, with no graphics stack.
//! Previews are visualization artifacts; they are not geometry validity checks.

use cad_kernel::{ModelSnapshot, Tolerance};
use cad_mesh::GenerationMeshCache;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;
use thiserror::Error;

pub mod metadata_pages;
mod motion;
mod provenance;
pub use motion::to_motion_study_html;

mod assembly;
pub use assembly::{
    to_assembly_gpu_html, to_assembly_gpu_html_with_fallback,
    to_assembly_gpu_html_with_fallback_and_cache, to_assembly_gpu_html_with_fallback_and_tolerance,
    to_assembly_gpu_scene_json, to_assembly_gpu_scene_json_with_cache,
    to_assembly_gpu_scene_json_with_tolerance, to_assembly_html, to_assembly_html_with_cache,
    to_assembly_html_with_tolerance, to_assembly_scene_json, to_assembly_scene_json_with_cache,
    to_assembly_scene_json_with_tolerance, to_assembly_stream_bundle,
    to_assembly_stream_bundle_with_cache, to_assembly_stream_bundle_with_tolerance,
    to_assembly_svg, to_assembly_svg_with_cache, to_assembly_svg_with_tolerance, DisplayMeshAsset,
    StreamBundle,
};

#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("cannot preview malformed geometry: {0}")]
    Geometry(&'static str),
    #[error("invalid recipe context: {0}")]
    Recipe(&'static str),
    #[error("cannot preview assembly: {0}")]
    Assembly(String),
    #[error("cannot serialize preview: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// The exact shared recipe captured for the build, plus this output's identity.
/// Parameters contain the overrides applied to that source, not rewritten code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeContext {
    pub language: String,
    pub source: String,
    pub source_name: String,
    /// Captured locked module sources, never fetched from the working tree.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_bundle_hash: Option<String>,
    /// Source-bound exact declaration spans supplied by the recipe evaluator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_provenance: Option<serde_json::Value>,
    pub output_name: String,
    pub feature_id: String,
    pub parameters: BTreeMap<String, f64>,
    /// Identity of this output's geometry in the corresponding build manifest.
    pub content_hash: String,
    /// Identity of source bytes and metadata in that same build manifest.
    pub recipe_context_hash: String,
    /// Captured native build obligations, including targets outside this output.
    /// The publisher binds these reports into `recipe_context_hash`.
    #[serde(default)]
    pub property_assertions: BTreeMap<String, EngineeringCheck>,
    /// Current-output native check summaries, bound to the captured report bytes
    /// by the publisher. These are display evidence, never viewer-side solvers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub native_engineering_checks: BTreeMap<String, NativeEngineeringCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEngineeringCheck {
    pub kind: String,
    pub id: String,
    pub output: String,
    pub status: String,
    pub scope: String,
    pub target_content_hash: String,
    /// Named native report values with units encoded in the names, such as
    /// `lower_bound_mm`. No display mesh measurement enters this map.
    pub quantities: BTreeMap<String, f64>,
    pub instance_paths: Vec<Vec<String>>,
    pub report_file: String,
    /// BLAKE3 digest of the canonical serialized native report entry.
    pub report_digest: String,
}

/// Read-only evidence supplied by the headless builder, never measured from a mesh.
/// This display DTO keeps the preview independent of the build executor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineeringCheck {
    pub output: String,
    pub property: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub actual: Option<f64>,
    pub status: String,
    pub target_content_hash: String,
    pub message: Option<String>,
}

#[derive(Clone, Serialize)]
struct Face {
    id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    semantic_face_id: Option<usize>,
    points: Vec<[f64; 3]>,
    edges: Vec<usize>,
    normal: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    triangles: Option<Vec<[usize; 3]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loop_starts: Option<Vec<usize>>,
}
impl Face {
    fn triangle_indices(&self) -> Vec<[usize; 3]> {
        self.triangles
            .clone()
            .unwrap_or_else(|| (1..self.points.len() - 1).map(|i| [0, i, i + 1]).collect())
    }
    fn next_point(&self, index: usize) -> usize {
        let starts = self.loop_starts.as_deref().unwrap_or(&[0]);
        let start = *starts.iter().rev().find(|&&start| start <= index).unwrap();
        let end = starts
            .iter()
            .copied()
            .find(|&start| start > index)
            .unwrap_or(self.points.len());
        if index + 1 == end {
            start
        } else {
            index + 1
        }
    }
}

#[derive(Clone, Serialize)]
struct Scene {
    #[serde(skip_serializing_if = "Option::is_none")]
    tessellation_error_mm: Option<f64>,
    faces: Vec<Face>,
    native_faces: usize,
    crease_edges: Vec<usize>,
    vertices: usize,
    edges: usize,
    solids: usize,
    bounds: Option<[[f64; 3]; 2]>,
    volume: Option<f64>,
}

fn scene(model: &ModelSnapshot) -> Result<Scene, PreviewError> {
    scene_with_tolerance(model, Tolerance::default())
}

fn scene_with_tolerance(
    model: &ModelSnapshot,
    tolerance: Tolerance,
) -> Result<Scene, PreviewError> {
    scene_with_cache(model, tolerance, &mut GenerationMeshCache::default())
}

fn scene_with_cache(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    cache: &mut GenerationMeshCache,
) -> Result<Scene, PreviewError> {
    scene_with_policy_and_cache(
        model,
        tolerance,
        cad_kernel::TessellationPolicy::default(),
        cache,
    )
}

#[cfg(test)]
fn scene_with_policy(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    policy: cad_kernel::TessellationPolicy,
) -> Result<Scene, PreviewError> {
    scene_with_policy_and_cache(
        model,
        tolerance,
        policy,
        &mut GenerationMeshCache::default(),
    )
}

fn scene_with_policy_and_cache(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    policy: cad_kernel::TessellationPolicy,
    cache: &mut GenerationMeshCache,
) -> Result<Scene, PreviewError> {
    let invalid = PreviewError::Geometry;
    if model.has_curved_geometry() {
        let mesh = cache
            .analytic(model, tolerance, policy)
            .map_err(|_| PreviewError::Geometry("analytic display tessellation failed"))?;
        let mut crease_edges = std::collections::BTreeSet::new();
        let faces = mesh
            .patches
            .iter()
            .enumerate()
            .map(|(id, patch)| {
                let edges = patch
                    .boundary_edges
                    .iter()
                    .map(|edge| match edge {
                        Some(edge) => {
                            crease_edges.insert(edge.0);
                            edge.0
                        }
                        // A derived triangulation edge must never alias a real
                        // boundary, including the last native edge.
                        None => model.edges.len(),
                    })
                    .collect();
                Face {
                    id,
                    semantic_face_id: Some(patch.face.0),
                    points: patch.points.clone(),
                    edges,
                    normal: patch.normal,
                    triangles: None,
                    loop_starts: None,
                }
            })
            .collect();
        let bounds = model.bounds();
        return Ok(Scene {
            tessellation_error_mm: Some(mesh.chordal_error_bound),
            faces,
            native_faces: model.faces.len(),
            crease_edges: crease_edges.into_iter().collect(),
            vertices: model.vertices.len(),
            edges: model.edges.len(),
            solids: model.solids.len(),
            bounds: Some([bounds.min, bounds.max]),
            volume: Some(model.volume()),
        });
    }
    if model.faces.len() > 50_000 {
        return Err(invalid("software preview exceeds 50,000 faces"));
    }
    if model
        .vertices
        .iter()
        .any(|v| v.position.iter().any(|v| !v.is_finite()))
    {
        return Err(invalid("nonfinite vertex"));
    }
    let mut faces = Vec::new();
    let mut face_points = 0usize;
    for (id, face) in model.faces.iter().enumerate() {
        if face.loops.is_empty() {
            return Err(invalid("missing face loop"));
        }
        if face
            .try_plane()
            .ok_or(invalid("curved face requires display tessellation"))?
            .normal
            .iter()
            .any(|n| !n.is_finite())
        {
            return Err(invalid("nonfinite normal"));
        }
        let mut points = Vec::new();
        let mut edges = Vec::new();
        let mut starts = Vec::new();
        let mut vertex_indices = BTreeMap::new();
        for loop_id in &face.loops {
            let boundary = model
                .loops
                .get(loop_id.0)
                .ok_or(invalid("missing face loop"))?;
            if boundary.edges.len() < 3 {
                return Err(invalid("face loop has fewer than three vertices"));
            }
            starts.push(points.len());
            for usage in &boundary.edges {
                let edge = model
                    .edges
                    .get(usage.edge.0)
                    .ok_or(invalid("missing edge"))?;
                let vertex = edge.vertices[usize::from(usage.reversed)];
                vertex_indices.insert(vertex.0, points.len());
                points.push(
                    model
                        .vertices
                        .get(vertex.0)
                        .ok_or(invalid("missing vertex"))?
                        .position,
                );
                edges.push(usage.edge.0);
            }
        }
        face_points += points.len();
        if face_points > 200_000 {
            return Err(invalid("software preview exceeds 200,000 face points"));
        }
        let triangles = if !cad_kernel::face_is_convex(model, cad_kernel::FaceId(id)) {
            Some(
                cache
                    .planar_face(model, tolerance, cad_kernel::FaceId(id))
                    .map_err(|_| invalid("face triangulation failed"))?
                    .iter()
                    .map(|ids| ids.map(|id| vertex_indices[&id.0]))
                    .collect(),
            )
        } else {
            None
        };
        faces.push(Face {
            id,
            semantic_face_id: None,
            points,
            edges,
            normal: face
                .try_plane()
                .ok_or(invalid("curved face requires display tessellation"))?
                .normal,
            triangles,
            loop_starts: (starts.len() > 1).then_some(starts),
        });
    }
    let bounds = (!model.vertices.is_empty()).then(|| {
        let bounds = model.bounds();
        [bounds.min, bounds.max]
    });
    let volume = model.volume();
    let mut normals = std::collections::BTreeMap::<usize, Vec<[f64; 3]>>::new();
    for face in &faces {
        for edge in &face.edges {
            normals.entry(*edge).or_default().push(face.normal);
        }
    }
    let crease_edges = normals
        .into_iter()
        .filter_map(|(id, ns)| (ns.len() != 2 || dot(ns[0], ns[1]) < 1.0 - 1e-10).then_some(id))
        .collect();
    Ok(Scene {
        tessellation_error_mm: None,
        faces,
        native_faces: model.faces.len(),
        crease_edges,
        vertices: model.vertices.len(),
        edges: model.edges.len(),
        solids: model.solids.len(),
        bounds,
        volume: volume.is_finite().then_some(volume),
    })
}

/// A standalone HTML canvas viewer. Drag to orbit, scroll to zoom, click a face.
pub fn to_html(model: &ModelSnapshot) -> Result<String, PreviewError> {
    html(model, None)
}

/// A standalone viewer with the actual, read-only source used for this output.
pub fn to_html_with_recipe(
    model: &ModelSnapshot,
    recipe: &RecipeContext,
) -> Result<String, PreviewError> {
    html(model, Some(recipe))
}

/// Render exact captured source using the model's build tolerance.
pub fn to_html_with_recipe_and_tolerance(
    model: &ModelSnapshot,
    recipe: &RecipeContext,
    tolerance: Tolerance,
) -> Result<String, PreviewError> {
    to_html_with_recipe_and_cache(
        model,
        recipe,
        tolerance,
        &mut GenerationMeshCache::default(),
    )
}

pub fn to_html_with_recipe_and_cache(
    model: &ModelSnapshot,
    recipe: &RecipeContext,
    tolerance: Tolerance,
    cache: &mut GenerationMeshCache,
) -> Result<String, PreviewError> {
    render_html(&scene_with_cache(model, tolerance, cache)?, Some(recipe))
}
/// Render geometry using the model's build tolerance.
pub fn to_html_with_tolerance(
    model: &ModelSnapshot,
    tolerance: Tolerance,
) -> Result<String, PreviewError> {
    to_html_with_cache(model, tolerance, &mut GenerationMeshCache::default())
}

pub fn to_html_with_cache(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    cache: &mut GenerationMeshCache,
) -> Result<String, PreviewError> {
    render_html(&scene_with_cache(model, tolerance, cache)?, None)
}
pub fn to_svg_with_tolerance(
    model: &ModelSnapshot,
    tolerance: Tolerance,
) -> Result<String, PreviewError> {
    to_svg_with_cache(model, tolerance, &mut GenerationMeshCache::default())
}

pub fn to_svg_with_cache(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    cache: &mut GenerationMeshCache,
) -> Result<String, PreviewError> {
    render_svg(&scene_with_cache(model, tolerance, cache)?)
}

fn script_json(value: &impl Serialize) -> Result<String, serde_json::Error> {
    // JSON escaping alone does not protect an HTML script element: the HTML
    // parser recognizes </script> even inside a JavaScript string. Escape all
    // HTML delimiters and JS line separators without changing the decoded text.
    Ok(serde_json::to_string(value)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

fn html(model: &ModelSnapshot, recipe: Option<&RecipeContext>) -> Result<String, PreviewError> {
    render_html(&scene(model)?, recipe)
}

fn render_html(
    data: &impl Serialize,
    recipe: Option<&RecipeContext>,
) -> Result<String, PreviewError> {
    render_html_template(data, recipe, include_str!("viewer.html"))
}

fn render_html_template(
    data: &impl Serialize,
    recipe: Option<&RecipeContext>,
    template: &str,
) -> Result<String, PreviewError> {
    if let Some(recipe) = recipe {
        provenance::validate(recipe)?;
        if !matches!(recipe.language.as_str(), "starlark" | "json") {
            return Err(PreviewError::Recipe("language must be starlark or json"));
        }
        if recipe.parameters.values().any(|value| !value.is_finite()) {
            return Err(PreviewError::Recipe("parameter overrides must be finite"));
        }
        for check in recipe.property_assertions.values() {
            if check
                .min
                .into_iter()
                .chain(check.max)
                .chain(check.actual)
                .any(|v| !v.is_finite())
                || (check.min.is_none() && check.max.is_none())
                || matches!((check.min, check.max), (Some(min), Some(max)) if min > max)
                || !matches!(
                    check.status.as_str(),
                    "passed" | "out_of_range" | "unavailable"
                )
                || check.output.is_empty()
                || check.property.is_empty()
                || check.target_content_hash.is_empty()
                || (check.output == recipe.output_name
                    && check.target_content_hash != recipe.content_hash)
                || (check.status == "passed"
                    && !check.actual.is_some_and(|value| {
                        check.min.is_none_or(|min| value >= min)
                            && check.max.is_none_or(|max| value <= max)
                    }))
            {
                return Err(PreviewError::Recipe(
                    "invalid or mismatched engineering check evidence",
                ));
            }
        }
        if recipe.native_engineering_checks.len() > 256 {
            return Err(PreviewError::Recipe("too many native engineering checks"));
        }
        for (key, check) in &recipe.native_engineering_checks {
            let report = match check.kind.as_str() {
                "datum_dimension" => "datum-dimensions.json",
                "nominal_seating" => "nominal-seating-checks.json",
                "pocket_fit" => "pocket-fit-checks.json",
                "clearance" => "clearance-checks.json",
                _ => {
                    return Err(PreviewError::Recipe(
                        "unknown native engineering check kind",
                    ))
                }
            };
            let identifier = |value: &str| {
                !value.is_empty()
                    && value.len() <= 128
                    && value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    })
            };
            if key != &format!("{}.{}", check.kind, check.id)
                || !identifier(&check.id)
                || check.output != recipe.output_name
                || check.target_content_hash != recipe.content_hash
                || check.status != "passed"
                || !matches!(
                    (check.kind.as_str(), check.scope.as_str()),
                    ("datum_dimension", "Directed native datum distance")
                        | ("nominal_seating", "Local nominal planar seating")
                        | ("pocket_fit", "Complete capsule footprint and axial overlap")
                        | ("clearance", "Initial pose")
                        | ("clearance", "Entire declared motion path")
                )
                || check.report_file != report
                || check.report_digest.len() != 64
                || !check
                    .report_digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || check.quantities.len() > 8
                || check.quantities.is_empty()
                || check
                    .quantities
                    .iter()
                    .any(|(name, value)| !identifier(name) || !value.is_finite())
                || check.instance_paths.len() != 2
                || check.instance_paths.iter().any(|path| {
                    path.is_empty()
                        || path.len() > 64
                        || path
                            .iter()
                            .any(|segment| segment.trim().is_empty() || segment.len() > 256)
                })
            {
                return Err(PreviewError::Recipe(
                    "invalid or mismatched native engineering check evidence",
                ));
            }
        }
    }
    let value = serde_json::to_value(data)?;
    let has_holes = |body: &serde_json::Value| {
        body["faces"]
            .as_array()
            .is_some_and(|faces| faces.iter().any(|face| face.get("triangles").is_some()))
    };
    let requires_loops = has_holes(&value)
        || value["meshes"]
            .as_object()
            .is_some_and(|meshes| meshes.values().any(has_holes));
    // Old live viewers require the exact legacy marker. A capability-specific
    // element makes them reject an unsupported replacement and retain the old scene.
    let has_semantic_patches = |body: &serde_json::Value| {
        body["faces"].as_array().is_some_and(|faces| {
            faces
                .iter()
                .any(|face| face.get("semantic_face_id").is_some())
        })
    };
    let requires_patches = has_semantic_patches(&value)
        || value["meshes"]
            .as_object()
            .is_some_and(|meshes| meshes.values().any(has_semantic_patches));
    let scene_id = if requires_patches {
        "cad-scene-data-v3"
    } else if requires_loops {
        "cad-scene-data-v2"
    } else {
        "cad-scene-data"
    };
    let data = script_json(data)?;
    let context = script_json(&recipe)?;
    // Match complete template elements. Embedded JSON escapes '<', so an
    // arbitrary label/source containing marker text cannot trigger substitution.
    Ok(template
        .replace(
            "<script>/*SOURCE_NAVIGATION*/</script>",
            &format!("<script>{}</script>", include_str!("source-navigation.js")),
        )
        .replace(
            "<script id=\"cad-scene-data\" type=\"application/json\">/*SCENE_DATA*/null</script>",
            &format!("<script id=\"{scene_id}\" type=\"application/json\">{data}</script>"),
        )
        .replace(
            "<script id=\"cad-recipe-data\" type=\"application/json\">/*RECIPE_DATA*/null</script>",
            &format!("<script id=\"cad-recipe-data\" type=\"application/json\">{context}</script>"),
        ))
}

/// A deterministic isometric SVG useful in Git pull requests and build artifacts.
pub fn to_svg(model: &ModelSnapshot) -> Result<String, PreviewError> {
    let scene = scene(model)?;
    render_svg(&scene)
}

fn render_svg(scene: &Scene) -> Result<String, PreviewError> {
    let mut output = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 960 720" role="img" aria-label="CAD model isometric preview"><rect width="960" height="720" fill="#f5f6f4"/><text x="32" y="42" fill="#24443e" font-family="monospace" font-size="16">HEADLESS CAD / ISOMETRIC</text>"##,
    );
    let Some([min, max]) = scene.bounds else {
        output.push_str(r##"<text x="480" y="360" text-anchor="middle" fill="#63716c" font-family="sans-serif">Empty result</text></svg>"##);
        return Ok(output);
    };
    // Rotate around Z then elevate: depth points towards the camera.
    let yaw: f64 = -0.75;
    let pitch: f64 = 0.6;
    let right = [-yaw.sin(), yaw.cos(), 0.0];
    let up = [
        -yaw.cos() * pitch.sin(),
        -yaw.sin() * pitch.sin(),
        pitch.cos(),
    ];
    let eye = [
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    ];
    let center = std::array::from_fn::<_, 3, _>(|i| min[i] * 0.5 + max[i] * 0.5);
    let project = |point: [f64; 3]| {
        let p = std::array::from_fn::<_, 3, _>(|i| point[i] - center[i]);
        [dot(p, right), -dot(p, up), dot(p, eye)]
    };
    let projected: Vec<_> = scene
        .faces
        .iter()
        .flat_map(|f| f.points.iter().map(|p| project(*p)))
        .collect();
    let mut extent = [0.0_f64; 2];
    for p in &projected {
        extent[0] = extent[0].max(p[0].abs());
        extent[1] = extent[1].max(p[1].abs());
    }
    let scale =
        (390.0 / extent[0].max(f64::MIN_POSITIVE)).min(270.0 / extent[1].max(f64::MIN_POSITIVE));
    if !scale.is_finite() || projected.iter().flatten().any(|v| !v.is_finite()) {
        return Err(PreviewError::Geometry("coordinates exceed preview range"));
    }
    let mut faces: Vec<_> = scene
        .faces
        .iter()
        .filter(|f| dot(f.normal, eye) > 0.0)
        .collect();
    faces.sort_by(|a, b| {
        let depth = |f: &Face| {
            f.points
                .iter()
                .map(|p| project(*p)[2] / f.points.len() as f64)
                .sum::<f64>()
        };
        depth(a).total_cmp(&depth(b))
    });
    for face in faces {
        let starts = face.loop_starts.as_deref().unwrap_or(&[0]);
        let mut path = String::new();
        for (loop_index, &start) in starts.iter().enumerate() {
            let end = starts
                .get(loop_index + 1)
                .copied()
                .unwrap_or(face.points.len());
            for (index, point) in face.points[start..end].iter().enumerate() {
                let p = project(*point);
                write!(
                    path,
                    "{} {:.4} {:.4} ",
                    if index == 0 { "M" } else { "L" },
                    480.0 + scale * p[0],
                    375.0 + scale * p[1]
                )
                .unwrap();
            }
            path.push_str("Z ");
        }
        let light = dot(face.normal, [0.3, -0.5, 0.8]).clamp(-1.0, 1.0);
        let shade = (175.0 + 55.0 * light) as u8;
        if face.loop_starts.is_some() {
            write!(
                output,
                "<path d=\"{path}\" fill-rule=\"evenodd\" fill=\"rgb({},{},{})\"/>",
                shade.saturating_sub(18),
                shade,
                shade.saturating_sub(5)
            )
            .unwrap();
        } else {
            let points = face
                .points
                .iter()
                .map(|p| {
                    let p = project(*p);
                    format!("{:.4},{:.4}", 480. + scale * p[0], 375. + scale * p[1])
                })
                .collect::<Vec<_>>()
                .join(" ");
            write!(
                output,
                "<polygon points=\"{points}\" fill=\"rgb({},{},{})\"/>",
                shade.saturating_sub(18),
                shade,
                shade.saturating_sub(5)
            )
            .unwrap();
        }
        for (i, edge) in face.edges.iter().enumerate() {
            if scene.crease_edges.binary_search(edge).is_ok() {
                let a = project(face.points[i]);
                let b = project(face.points[face.next_point(i)]);
                write!(output, "<path d=\"M {:.4} {:.4} L {:.4} {:.4}\" fill=\"none\" stroke=\"#46605a\" stroke-width=\"1\"/>", 480.0+scale*a[0], 375.0+scale*a[1], 480.0+scale*b[0], 375.0+scale*b[1]).unwrap();
            }
        }
    }
    write!(output, r##"<text x="32" y="688" fill="#63716c" font-family="monospace" font-size="13">{} solid(s) · {} native faces · millimetres</text></svg>"##, scene.solids, scene.native_faces).unwrap();
    Ok(output)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    #[test]
    fn derived_keyseat_edges_do_not_alias_native_boundary_edges() {
        use cad_kernel::{boolean, extrude_circle, extrude_rectangle, translate, BooleanOp};
        let t = Tolerance::default();
        let shaft = extrude_circle(6., 40., t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(4., 4., 14., t).unwrap().model,
            [4., -2., 10.],
            t,
        )
        .unwrap()
        .model;
        let model = boolean(&shaft, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let mesh =
            cad_kernel::tessellate_analytic(&model, t, cad_kernel::TessellationPolicy::default())
                .unwrap();
        let scene = super::scene(&model).unwrap();
        assert!(scene.crease_edges.contains(&(model.edges.len() - 1)));
        let mut derived = 0;
        for (patch, face) in mesh.patches.iter().zip(&scene.faces) {
            for (native, edge) in patch.boundary_edges.iter().zip(&face.edges) {
                if let Some(native) = native {
                    assert_eq!(*edge, native.0);
                    assert!(scene.crease_edges.contains(edge));
                } else {
                    derived += 1;
                    assert!(!scene.crease_edges.contains(edge));
                }
            }
        }
        assert!(derived > 0);
    }
    use super::*;
    use cad_kernel::{extrude_rectangle, EdgeId, Tolerance};

    #[test]
    fn analytic_preview_counts_native_faces_separately_from_display_patches() {
        let t = Tolerance::default();
        for profile in [
            vec![[0., 0.], [8., 0.], [0., 12.]],
            vec![[0., 0.], [8., 0.], [8., 20.], [6., 20.], [0., 32.]],
        ] {
            let model = cad_kernel::revolve_polygon(&profile, t).unwrap().model;
            let data = scene(&model).unwrap();
            assert_eq!(data.native_faces, model.faces.len());
            assert!(data.faces.len() > data.native_faces);
            let semantic: std::collections::BTreeSet<_> = data
                .faces
                .iter()
                .map(|face| face.semantic_face_id.unwrap())
                .collect();
            assert_eq!(semantic.len(), data.native_faces);
            let serialized = serde_json::to_value(&data).unwrap();
            assert_eq!(serialized["native_faces"], model.faces.len());
            assert_eq!(
                serialized["faces"].as_array().unwrap().len(),
                data.faces.len()
            );
            assert!(to_svg(&model)
                .unwrap()
                .contains(&format!("{} native faces", model.faces.len())));
        }
    }

    #[test]
    fn preview_embeds_actual_model_and_no_remote_dependencies() {
        let model = extrude_rectangle(80.0, 40.0, 6.0, Tolerance::default())
            .unwrap()
            .model;
        let html = to_html(&model).unwrap();
        assert!(html.contains("\"volume\":19200.0"));
        assert!(html.contains("\"solids\":1"));
        assert!(!html.contains("/*SCENE_DATA*/"));
        assert!(!html.contains("src=\"http"));
        let svg = to_svg(&model).unwrap();
        assert_eq!(svg.matches("<polygon ").count(), 3);
        assert_eq!(svg, to_svg(&model).unwrap());
    }

    #[test]
    fn embedded_json_dictionary_keys_and_template_markers_remain_data() {
        let data = serde_json::json!({"__proto__": {"/*RECIPE_DATA*/null": "</script><script>bad()</script>"}});
        let html = render_html(&data, None).unwrap();
        let raw = html
            .lines()
            .find_map(|line| {
                line.strip_prefix("<script id=\"cad-scene-data\" type=\"application/json\">")
            })
            .unwrap()
            .strip_suffix("</script>")
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(raw).unwrap(),
            data
        );
        assert!(html.contains("JSON.parse((document.getElementById('cad-scene-data-v3')||document.getElementById('cad-scene-data-v2')||document.getElementById('cad-scene-data')).textContent)"));
        assert!(!html.contains("<script>bad()"));
    }

    #[test]
    fn malformed_topology_returns_error() {
        let mut model = extrude_rectangle(1.0, 2.0, 3.0, Tolerance::default())
            .unwrap()
            .model;
        model.loops[0].edges[0].edge = EdgeId(usize::MAX);
        assert!(to_svg(&model).is_err());
        assert!(to_html(&model).is_err());
    }

    #[test]
    fn recipe_source_is_exact_and_cannot_escape_script_element() {
        let model = extrude_rectangle(1.0, 2.0, 3.0, Tolerance::default())
            .unwrap()
            .model;
        let recipe = RecipeContext {
            language: "starlark".into(),
            source: "# </script><script>globalThis.pwned=true</script>\n# <img src=x onerror=alert(1)> & <!--\n# separators:\u{2028}\u{2029}\n# /*SCENE_DATA*/null /*RECIPE_DATA*/null\noutput(\"part\", body)\n".into(),
            source_name: "<recipe>.star".into(),
            modules: BTreeMap::from([("lib/unsafe.star".into(), "# </script><script>globalThis.moduleInjected=true</script>\ndef make():\n    return 1\n".into())]),
            source_bundle_hash: Some("bundle-test".into()),
            source_provenance: None,
            output_name: "part".into(),
            feature_id: "body".into(),
            parameters: BTreeMap::from([("width".into(), 12.5)]),
            content_hash: "geometry-test".into(),
            recipe_context_hash: "recipe-test".into(),
            property_assertions: Default::default(),
            native_engineering_checks: Default::default(),
        };
        let html = to_html_with_recipe(&model, &recipe).unwrap();
        assert_eq!(html.matches("<script>").count(), 2);
        assert_eq!(html.matches("</script>").count(), 4);
        assert!(!html.contains("<img src=x"));
        let embedded = html
            .lines()
            .find_map(|line| {
                line.strip_prefix("<script id=\"cad-recipe-data\" type=\"application/json\">")
            })
            .unwrap()
            .strip_suffix("</script>")
            .unwrap();
        let restored: RecipeContext = serde_json::from_str(embedded).unwrap();
        assert_eq!(restored, recipe);
        assert!(html.contains("\\u003c/script\\u003e"));
        assert!(html.contains("\\u2028\\u2029"));
        assert!(to_html(&model)
            .unwrap()
            .contains("<script id=\"cad-recipe-data\" type=\"application/json\">null</script>"));
    }

    #[test]
    fn recipe_context_rejects_unrepresentable_parameter_values() {
        let model = extrude_rectangle(1.0, 2.0, 3.0, Tolerance::default())
            .unwrap()
            .model;
        let recipe = RecipeContext {
            language: "starlark".into(),
            source: String::new(),
            source_name: "part.star".into(),
            modules: Default::default(),
            source_bundle_hash: None,
            source_provenance: None,
            output_name: "part".into(),
            feature_id: "body".into(),
            parameters: BTreeMap::from([("width".into(), f64::NAN)]),
            content_hash: "geometry-test".into(),
            recipe_context_hash: "recipe-test".into(),
            property_assertions: Default::default(),
            native_engineering_checks: Default::default(),
        };
        assert!(matches!(
            to_html_with_recipe(&model, &recipe),
            Err(PreviewError::Recipe(_))
        ));
    }

    #[test]
    fn engineering_evidence_is_captured_safely_and_rejects_invalid_success() {
        let model = extrude_rectangle(1.0, 2.0, 3.0, Tolerance::default())
            .unwrap()
            .model;
        let mut context = RecipeContext {
            language: "starlark".into(),
            source: String::new(),
            source_name: "part.star".into(),
            modules: Default::default(),
            source_bundle_hash: None,
            source_provenance: None,
            output_name: "part".into(),
            feature_id: "body".into(),
            parameters: BTreeMap::new(),
            content_hash: "body-hash".into(),
            recipe_context_hash: "context-hash".into(),
            property_assertions: BTreeMap::from([(
                "</script><img src=x>".into(),
                EngineeringCheck {
                    output: "part".into(),
                    property: "volume_mm3".into(),
                    min: Some(5.),
                    max: Some(7.),
                    actual: Some(6.),
                    status: "passed".into(),
                    target_content_hash: "body-hash".into(),
                    message: Some("</script><script>globalThis.pwned=true</script>".into()),
                },
            )]),
            native_engineering_checks: Default::default(),
        };
        let html = to_html_with_recipe(&model, &context).unwrap();
        assert!(!html.contains("<img src=x>"));
        assert!(!html.contains("<script>globalThis.pwned"));
        let json = serde_json::to_value(&context).unwrap();
        let mut old = json.clone();
        old.as_object_mut().unwrap().remove("property_assertions");
        assert!(serde_json::from_value::<RecipeContext>(old)
            .unwrap()
            .property_assertions
            .is_empty());
        for invalid in [f64::NAN, f64::INFINITY, 8.] {
            context
                .property_assertions
                .values_mut()
                .next()
                .unwrap()
                .actual = Some(invalid);
            assert!(matches!(
                to_html_with_recipe(&model, &context),
                Err(PreviewError::Recipe(_))
            ));
        }
        context = serde_json::from_value(json).unwrap();
        context
            .property_assertions
            .values_mut()
            .next()
            .unwrap()
            .target_content_hash = "stale-body".into();
        assert!(matches!(
            to_html_with_recipe(&model, &context),
            Err(PreviewError::Recipe(_))
        ));
    }

    #[test]
    fn native_engineering_evidence_is_current_bounded_and_read_only() {
        let model = extrude_rectangle(1.0, 2.0, 3.0, Tolerance::default())
            .unwrap()
            .model;
        let mut context = RecipeContext {
            language: "starlark".into(),
            source: "# captured source\n".into(),
            source_name: "part.star".into(),
            modules: Default::default(),
            source_bundle_hash: None,
            source_provenance: None,
            output_name: "part".into(),
            feature_id: "body".into(),
            parameters: BTreeMap::new(),
            content_hash: "body-hash".into(),
            recipe_context_hash: "context-hash".into(),
            property_assertions: Default::default(),
            native_engineering_checks: BTreeMap::from([(
                "datum_dimension.depth".into(),
                NativeEngineeringCheck {
                    kind: "datum_dimension".into(),
                    id: "depth".into(),
                    output: "part".into(),
                    status: "passed".into(),
                    scope: "Directed native datum distance".into(),
                    target_content_hash: "body-hash".into(),
                    quantities: BTreeMap::from([("measured_mm".into(), 3.0)]),
                    instance_paths: vec![vec!["part".into()], vec!["part".into()]],
                    report_file: "datum-dimensions.json".into(),
                    report_digest: "a".repeat(64),
                },
            )]),
        };
        let html = to_html_with_recipe(&model, &context).unwrap();
        assert!(html.contains("Native report JSON"));
        assert!(html.contains("Captured recipe source"));
        assert!(html.contains("datum-dimensions.json"));
        assert!(html.contains("\"native_engineering_checks\""));
        let mut stale = context.clone();
        stale
            .native_engineering_checks
            .get_mut("datum_dimension.depth")
            .unwrap()
            .target_content_hash = "old-body".into();
        assert!(matches!(
            to_html_with_recipe(&model, &stale),
            Err(PreviewError::Recipe(_))
        ));
        let check = context
            .native_engineering_checks
            .get_mut("datum_dimension.depth")
            .unwrap();
        check.report_file = "../outside.json".into();
        assert!(matches!(
            to_html_with_recipe(&model, &context),
            Err(PreviewError::Recipe(_))
        ));
    }
}
