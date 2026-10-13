//! Revision-bound native section sheets staged with their engineering generation.
use super::*;

// A drawing reads one previously staged section report at a time.
const MAX_SECTION_REPORT_READ_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DATUM_REPORT_READ_BYTES: u64 = 32 * 1024 * 1024;

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
    artifacts: &mut artifact_sink::ArtifactSink,
) -> Result<(), Box<dyn Error>> {
    let has_dimension_tables = build
        .drawing_sheets
        .values()
        .any(|sheet| !sheet.declaration.dimension_tables.is_empty());
    let datum_report = if has_dimension_tables {
        let name = "datum-dimensions.json";
        let digest = artifacts
            .get(name)
            .ok_or("drawing references unpublished datum dimension report")?
            .digest
            .clone();
        let content = artifacts.read_text(name, MAX_DATUM_REPORT_READ_BYTES)?;
        let report: Value = serde_json::from_str(&content)?;
        if report["recipe_context_hash"] != summary["recipe_context_hash"]
            || report["dimensions"] != json!(build.datum_dimensions)
        {
            return Err("drawing datum report does not match this native build".into());
        }
        Some((digest, report))
    } else {
        None
    };
    for (id, report) in &build.drawing_sheets {
        let declaration = &report.declaration;
        let mut views = Vec::new();
        let mut evidence = Vec::new();
        for viewport in &declaration.views {
            let name = format!("section-{}.json", viewport.view);
            let digest = artifacts
                .get(&name)
                .ok_or("drawing references unpublished section report")?;
            let digest = digest.digest.clone();
            let section_json = artifacts.read_text(&name, MAX_SECTION_REPORT_READ_BYTES)?;
            let section_report: Value = serde_json::from_str(&section_json)?;
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
                "algorithm":"blake3","digest":digest,
                "snapshot":section_report["snapshot"],"viewport":viewport}));
        }
        let mut dimension_tables = Vec::new();
        let mut table_evidence = Vec::new();
        for table in &declaration.dimension_tables {
            let mut rows = Vec::new();
            let mut row_evidence = Vec::new();
            for row in &table.rows {
                let native = build
                    .datum_dimensions
                    .get(&row.dimension)
                    .ok_or("drawing references unknown native datum dimension")?;
                if native.declaration.output != table.output || native.status != "passed" {
                    return Err(
                        "drawing dimension output or status does not match native report".into(),
                    );
                }
                let witness = serde_json::to_value(native)?;
                if datum_report
                    .as_ref()
                    .ok_or("missing drawing datum report")?
                    .1["dimensions"][&row.dimension]
                    != witness
                {
                    return Err("drawing dimension does not match staged native report".into());
                }
                rows.push(cad_export::DrawingDimensionRow {
                    dimension: row.dimension.clone(),
                    label: row.label.clone(),
                    value_mm: native.value_mm,
                    lower_bound_mm: native.lower_bound_mm,
                    upper_bound_mm: native.upper_bound_mm,
                });
                row_evidence.push(json!({"dimension":row.dimension,"label":row.label,
                    "native_dimension":witness}));
            }
            dimension_tables.push(cad_export::DrawingDimensionTable {
                id: table.id.clone(),
                title: table.title.clone(),
                x: table.x,
                y: table.y,
                width: table.width,
                height: table.height,
                rows,
            });
            table_evidence.push(
                json!({"id":table.id,"title":table.title,"output":table.output,
                "paper_box_mm":{"x":table.x,"y":table.y,"width":table.width,"height":table.height},
                "rows":row_evidence}),
            );
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
            dimension_tables,
        };
        let svg = cad_export::drawing_sheet_to_svg(&sheet)?;
        let svg_name = format!("drawing-{id}.svg");
        let json_name = format!("drawing-{id}.json");
        let mut witness = json!({"schema_version":if declaration.dimension_tables.is_empty(){1}else{2},"kind":"native_section_drawing_sheet",
            "renderer_build_id":implementation_id(),
            "declaration":declaration,"status":"published","paper_units":"mm",
            "scale_kind":"paper_mm_per_model_mm","recipe_context_hash":context,
            "recipe_source":summary["recipe_source"],"views":evidence,
            "svg":{"file":svg_name,"algorithm":"blake3","digest":blake3::hash(svg.as_bytes()).to_hex().to_string()},
            "fidelity":"Native analytic section curves and supported dimensions at explicit scale; optional authored manufacturing size limits; no projected hidden lines, fit classes, GD&T or manufactured-part conformance."});
        if let Some((digest, _)) = &datum_report {
            if !declaration.dimension_tables.is_empty() {
                witness["datum_dimension_report"] = json!({"file":"datum-dimensions.json",
                    "algorithm":"blake3","digest":digest});
                witness["dimension_tables"] = json!(table_evidence);
                witness["fidelity"] = json!("Native analytic sections and source-bound nominal datum distances. Three-decimal table values are rounded display only; drawing JSON and the linked native report retain full precision and numerical bounds. No projected hidden lines, fit classes, GD&T or manufactured-part conformance.");
            }
        }
        summary["drawing_sheets"][id]["artifacts"] = json!({"svg":svg_name,"json":json_name});
        summary["drawing_sheets"][id]["status"] = json!("published");
        artifacts.push((svg_name, svg))?;
        artifacts.push((json_name, serde_json::to_string_pretty(&witness)?))?;
    }
    Ok(())
}
