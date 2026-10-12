//! Revision-bound native section sheets staged with their engineering generation.
use super::*;

fn implementation_id() -> String {
    let mut hash = blake3::Hasher::new();
    for source in [
        include_str!("drawing.rs"),
        include_str!("section.rs"),
        include_str!("../../cad-export/src/drawing.rs"),
        include_str!("../../cad-export/src/section.rs"),
        include_str!("../../../Cargo.lock"),
    ] {
        hash.update(&(source.len() as u64).to_le_bytes());
        hash.update(source.as_bytes());
    }
    hash.finalize().to_hex().to_string()
}

pub(super) fn publish(
    build: &BuildResult,
    summary: &mut Value,
    artifacts: &mut Vec<(String, String)>,
) -> Result<(), Box<dyn Error>> {
    for (id, report) in &build.drawing_sheets {
        let declaration = &report.declaration;
        let mut views = Vec::new();
        let mut evidence = Vec::new();
        for viewport in &declaration.views {
            let name = format!("section-{}.json", viewport.view);
            let section_json = artifacts
                .iter()
                .find(|(file, _)| file == &name)
                .ok_or("drawing references unpublished section report")?;
            let section_report: Value = serde_json::from_str(&section_json.1)?;
            let geometry = section::drawing_geometry(&section_report)?;
            views.push(cad_export::DrawingSheetView {
                id: viewport.view.clone(),
                label: geometry.label,
                x: viewport.x,
                y: viewport.y,
                width: viewport.width,
                height: viewport.height,
                scale: viewport.scale,
                sections: geometry.sections,
                dimensions: geometry.dimensions,
                circles: geometry.circles,
            });
            evidence.push(json!({"view":viewport.view,"section_report":name,
                "algorithm":"blake3","digest":blake3::hash(section_json.1.as_bytes()).to_hex().to_string(),
                "snapshot":section_report["snapshot"],"viewport":viewport}));
        }
        let context = summary["recipe_context_hash"]
            .as_str()
            .ok_or("missing drawing source context")?;
        let sheet = cad_export::DrawingSheet {
            title: declaration.title.clone(),
            revision: declaration.revision.clone(),
            source_context_hash: context.into(),
            width: declaration.width,
            height: declaration.height,
            views,
        };
        let svg = cad_export::drawing_sheet_to_svg(&sheet)?;
        let svg_name = format!("drawing-{id}.svg");
        let json_name = format!("drawing-{id}.json");
        let witness = json!({"schema_version":1,"kind":"native_section_drawing_sheet",
            "renderer_build_id":implementation_id(),
            "declaration":declaration,"status":"published","paper_units":"mm",
            "scale_kind":"paper_mm_per_model_mm","recipe_context_hash":context,
            "recipe_source":summary["recipe_source"],"views":evidence,
            "svg":{"file":svg_name,"algorithm":"blake3","digest":blake3::hash(svg.as_bytes()).to_hex().to_string()},
            "fidelity":"Native analytic section curves and supported dimensions at explicit scale; no projected hidden lines, fits, tolerances, GD&T or manufacturing approval."});
        summary["drawing_sheets"][id]["artifacts"] = json!({"svg":svg_name,"json":json_name});
        summary["drawing_sheets"][id]["status"] = json!("published");
        artifacts.push((svg_name, svg));
        artifacts.push((json_name, serde_json::to_string_pretty(&witness)?));
    }
    Ok(())
}
