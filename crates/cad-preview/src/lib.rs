//! Self-contained previews derived from B-rep face loops, with no graphics stack.
//! Previews are visualization artifacts; they are not geometry validity checks.

use cad_kernel::ModelSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;
use thiserror::Error;

mod assembly;
pub use assembly::{
    to_assembly_gpu_html, to_assembly_gpu_html_with_fallback, to_assembly_gpu_scene_json,
    to_assembly_html, to_assembly_scene_json, to_assembly_stream_bundle, to_assembly_svg,
    DisplayMeshAsset, StreamBundle,
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
    pub output_name: String,
    pub feature_id: String,
    pub parameters: BTreeMap<String, f64>,
    /// Identity of this output's geometry in the corresponding build manifest.
    pub content_hash: String,
    /// Identity of source bytes and metadata in that same build manifest.
    pub recipe_context_hash: String,
}

#[derive(Clone, Serialize)]
struct Face {
    id: usize,
    points: Vec<[f64; 3]>,
    edges: Vec<usize>,
    normal: [f64; 3],
}

#[derive(Clone, Serialize)]
struct Scene {
    faces: Vec<Face>,
    crease_edges: Vec<usize>,
    vertices: usize,
    edges: usize,
    solids: usize,
    bounds: Option<[[f64; 3]; 2]>,
    volume: Option<f64>,
}

fn scene(model: &ModelSnapshot) -> Result<Scene, PreviewError> {
    let invalid = PreviewError::Geometry;
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
        if face.loops.len() != 1 {
            return Err(invalid("preview supports one outer loop per face"));
        }
        if face.plane.normal.iter().any(|n| !n.is_finite()) {
            return Err(invalid("nonfinite normal"));
        }
        let boundary = model
            .loops
            .get(face.loops[0].0)
            .ok_or(invalid("missing face loop"))?;
        face_points += boundary.edges.len();
        if face_points > 200_000 {
            return Err(invalid("software preview exceeds 200,000 face points"));
        }
        let mut points = Vec::new();
        for usage in &boundary.edges {
            let edge = model
                .edges
                .get(usage.edge.0)
                .ok_or(invalid("missing edge"))?;
            let vertex = edge.vertices[usize::from(usage.reversed)];
            points.push(
                model
                    .vertices
                    .get(vertex.0)
                    .ok_or(invalid("missing vertex"))?
                    .position,
            );
        }
        if points.len() < 3 {
            return Err(invalid("face has fewer than three vertices"));
        }
        faces.push(Face {
            id,
            points,
            edges: boundary.edges.iter().map(|e| e.edge.0).collect(),
            normal: face.plane.normal,
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
        faces,
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
        if !matches!(recipe.language.as_str(), "starlark" | "json") {
            return Err(PreviewError::Recipe("language must be starlark or json"));
        }
        if recipe.parameters.values().any(|value| !value.is_finite()) {
            return Err(PreviewError::Recipe("parameter overrides must be finite"));
        }
    }
    let data = script_json(data)?;
    let context = script_json(&recipe)?;
    // Match complete template elements. Embedded JSON escapes '<', so an
    // arbitrary label/source containing marker text cannot trigger substitution.
    Ok(template
        .replace(
            "<script id=\"cad-scene-data\" type=\"application/json\">/*SCENE_DATA*/null</script>",
            &format!("<script id=\"cad-scene-data\" type=\"application/json\">{data}</script>"),
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
        let points = face
            .points
            .iter()
            .map(|p| {
                let p = project(*p);
                format!("{:.4},{:.4}", 480.0 + scale * p[0], 375.0 + scale * p[1])
            })
            .collect::<Vec<_>>()
            .join(" ");
        let light = dot(face.normal, [0.3, -0.5, 0.8]).clamp(-1.0, 1.0);
        let shade = (175.0 + 55.0 * light) as u8;
        write!(
            output,
            "<polygon points=\"{points}\" fill=\"rgb({},{},{})\"/>",
            shade.saturating_sub(18),
            shade,
            shade.saturating_sub(5)
        )
        .unwrap();
        for (i, edge) in face.edges.iter().enumerate() {
            if scene.crease_edges.binary_search(edge).is_ok() {
                let a = project(face.points[i]);
                let b = project(face.points[(i + 1) % face.points.len()]);
                write!(output, "<path d=\"M {:.4} {:.4} L {:.4} {:.4}\" fill=\"none\" stroke=\"#46605a\" stroke-width=\"1\"/>", 480.0+scale*a[0], 375.0+scale*a[1], 480.0+scale*b[0], 375.0+scale*b[1]).unwrap();
            }
        }
    }
    write!(output, r##"<text x="32" y="688" fill="#63716c" font-family="monospace" font-size="13">{} solid(s) · {} faces · millimetres</text></svg>"##, scene.solids, scene.faces.len()).unwrap();
    Ok(output)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_kernel::{extrude_rectangle, EdgeId, Tolerance};

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
        assert!(html.contains("JSON.parse(document.getElementById('cad-scene-data').textContent)"));
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
            output_name: "part".into(),
            feature_id: "body".into(),
            parameters: BTreeMap::from([("width".into(), 12.5)]),
            content_hash: "geometry-test".into(),
            recipe_context_hash: "recipe-test".into(),
        };
        let html = to_html_with_recipe(&model, &recipe).unwrap();
        assert_eq!(html.matches("<script>").count(), 1);
        assert_eq!(html.matches("</script>").count(), 3);
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
            output_name: "part".into(),
            feature_id: "body".into(),
            parameters: BTreeMap::from([("width".into(), f64::NAN)]),
            content_hash: "geometry-test".into(),
            recipe_context_hash: "recipe-test".into(),
        };
        assert!(matches!(
            to_html_with_recipe(&model, &recipe),
            Err(PreviewError::Recipe(_))
        ));
    }
}
