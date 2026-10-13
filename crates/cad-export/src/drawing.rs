//! Source-authored paper layout of native sections; no projected HLR or GD&T.
use crate::{ExportError, SectionCircleDimensionAnnotation, SectionDimensionAnnotation};
use cad_kernel::PlanarSection;
use std::{collections::BTreeSet, fmt::Write};
#[derive(Debug, Clone)]
pub struct DrawingSheet {
    pub title: String,
    pub revision: String,
    pub source_context_hash: String,
    pub width: f64,
    pub height: f64,
    pub views: Vec<DrawingSheetView>,
    pub dimension_tables: Vec<DrawingDimensionTable>,
}
/// A paper-space table of measurements already resolved by the native build.
/// Values are supplied by the publisher, never by the authored recipe.
#[derive(Debug, Clone)]
pub struct DrawingDimensionTable {
    pub id: String,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rows: Vec<DrawingDimensionRow>,
}
#[derive(Debug, Clone)]
pub struct DrawingDimensionRow {
    pub dimension: String,
    pub label: String,
    pub value_mm: f64,
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
}
#[derive(Debug, Clone)]
pub struct DrawingSheetView {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub sections: Vec<PlanarSection>,
    pub dimensions: Vec<SectionDimensionAnnotation>,
    pub circles: Vec<SectionCircleDimensionAnnotation>,
}
fn invalid(message: &str) -> ExportError {
    ExportError::InvalidModel(message.into())
}
fn escape(text: &str) -> Result<String, ExportError> {
    if text.chars().any(|c| {
        (c < ' ' && !matches!(c, '\n' | '\r' | '\t')) || matches!(c, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(invalid("invalid XML drawing text"));
    }
    Ok(text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn printable_text(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text.trim() == text
        && !text.chars().any(char::is_control)
}
fn interiors_overlap(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}
fn text_width_mm(text: &str, font_mm: f64) -> f64 {
    // UTF-8 bytes deliberately overestimate wide/combining glyph advances.
    // This conservative monospace bound does not depend on a host font engine.
    text.len() as f64 * font_mm * 0.7
}
/// Paper coordinates are millimetres from the top left. Native section scale
/// is explicit; an annotated canvas that exceeds its allocation is an error.
pub fn drawing_sheet_to_svg(sheet: &DrawingSheet) -> Result<String, ExportError> {
    if !sheet.width.is_finite()
        || !sheet.height.is_finite()
        || sheet.width <= 20.
        || sheet.height <= 45.
        || sheet.width > 2000.
        || sheet.height > 2000.
        || sheet.views.is_empty()
        || sheet.views.len() > 32
        || sheet.dimension_tables.len() > 8
    {
        return Err(invalid("invalid drawing paper or view count"));
    }
    let mut ids = BTreeSet::new();
    let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{}</title>\n<desc>Native analytic section sheet. Explicit scales; no projected hidden-line removal or GD&amp;T.</desc>\n<rect width=\"{}\" height=\"{}\" fill=\"white\"/>\n<rect x=\"10\" y=\"10\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.25\"/>\n",sheet.width,sheet.height,sheet.width,sheet.height,escape(&sheet.title)?,sheet.width,sheet.height,sheet.width-20.,sheet.height-20.);
    for (i, view) in sheet.views.iter().enumerate() {
        if !safe_id(&view.id)
            || !ids.insert(view.id.to_ascii_lowercase())
            || [view.x, view.y, view.width, view.height, view.scale]
                .iter()
                .any(|v| !v.is_finite())
            || view.width <= 0.
            || view.height <= 0.
            || view.scale <= 0.
            || view.x < 10.
            || view.y < 10.
            || view.x + view.width > sheet.width - 10.
            || view.y + view.height > sheet.height - 35.
        {
            return Err(invalid("invalid drawing viewport or scale"));
        }
        if sheet.views[..i].iter().any(|other| {
            interiors_overlap(
                [view.x, view.y, view.width, view.height],
                [other.x, other.y, other.width, other.height],
            )
        }) {
            return Err(invalid("drawing viewports overlap"));
        }
        let layout = crate::section::sections_layout(
            &view.sections,
            &view.label,
            &view.dimensions,
            &view.circles,
        )?;
        let [left, top, width, height] = layout.view_box;
        let w = width * view.scale;
        let h = height * view.scale;
        if !w.is_finite()
            || !h.is_finite()
            || w <= 0.
            || h <= 0.
            || w > view.width
            || h > view.height
        {
            return Err(invalid(&format!("drawing view {} geometry and annotations exceed allocated viewport at declared scale",view.id)));
        }
        let x = view.x + (view.width - w) / 2.;
        let y = view.y + (view.height - h) / 2.;
        writeln!(out,"<svg id=\"drawing-view-{}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" viewBox=\"{left} {top} {width} {height}\" data-scale=\"{}\" overflow=\"visible\">",view.id,view.scale).unwrap();
        // Only internally generated SVG enters this extraction. Bounds travel as
        // typed numbers from the same renderer; never parse caller SVG geometry.
        for line in layout.svg.lines().skip(1) {
            if line != "</svg>" {
                writeln!(
                    out,
                    "{}",
                    line.replace("id=\"section-", &format!("id=\"{}-section-", view.id))
                )
                .unwrap();
            }
        }
        out.push_str("</svg>\n");
    }
    let mut emitted_table_ids = BTreeSet::new();
    for (i, table) in sheet.dimension_tables.iter().enumerate() {
        if !safe_id(&table.id)
            || !ids.insert(table.id.to_ascii_lowercase())
            || !emitted_table_ids.insert(format!("drawing-table-{}", table.id.to_ascii_lowercase()))
            || !printable_text(&table.title, 128)
            || [table.x, table.y, table.width, table.height]
                .iter()
                .any(|v| !v.is_finite())
            || table.width <= 0.
            || table.height <= 0.
            || table.x < 10.
            || table.y < 10.
            || table.x + table.width > sheet.width - 10.
            || table.y + table.height > sheet.height - 35.
            || table.rows.is_empty()
            || table.rows.len() > 16
            || table.height < 10. + table.rows.len() as f64 * 8.
            || table.width < 80.
            || text_width_mm(&table.title, 3.2) > table.width - 6.
        {
            return Err(invalid("invalid drawing dimension table bounds or title"));
        }
        let bounds = [table.x, table.y, table.width, table.height];
        if sheet
            .views
            .iter()
            .any(|view| interiors_overlap(bounds, [view.x, view.y, view.width, view.height]))
            || sheet.dimension_tables[..i].iter().any(|other| {
                interiors_overlap(bounds, [other.x, other.y, other.width, other.height])
            })
        {
            return Err(invalid(
                "drawing dimension table overlaps another layout element",
            ));
        }
        let mut dimensions = BTreeSet::new();
        for row in &table.rows {
            let value = format!("≈ {:.3} mm", row.value_mm);
            if !safe_id(&row.dimension)
                || !dimensions.insert(row.dimension.to_ascii_lowercase())
                || !emitted_table_ids.insert(format!(
                    "drawing-table-{}-{}",
                    table.id.to_ascii_lowercase(),
                    row.dimension.to_ascii_lowercase()
                ))
                || !printable_text(&row.label, 96)
                || [row.value_mm, row.lower_bound_mm, row.upper_bound_mm]
                    .iter()
                    .any(|v| !v.is_finite())
                || row.lower_bound_mm > row.value_mm
                || row.value_mm > row.upper_bound_mm
                || text_width_mm(&row.label, 2.8) > table.width - 42.
                || text_width_mm(&value, 2.8) > 36.
            {
                return Err(invalid("invalid drawing dimension table row or text fit"));
            }
        }
        writeln!(out, "<g id=\"drawing-table-{}\">", table.id).unwrap();
        writeln!(out, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.25\"/>", table.x, table.y, table.width, table.height).unwrap();
        writeln!(
            out,
            "<text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"3.2\">{}</text>",
            table.x + 3.,
            table.y + 5.,
            escape(&table.title)?
        )
        .unwrap();
        writeln!(
            out,
            "<path d=\"M {} {} H {}\" stroke=\"black\" stroke-width=\"0.2\"/>",
            table.x,
            table.y + 7.,
            table.x + table.width
        )
        .unwrap();
        for (row_index, row) in table.rows.iter().enumerate() {
            let baseline = table.y + 12. + row_index as f64 * 7.;
            writeln!(out, "<g id=\"drawing-table-{}-{}\" data-dimension=\"{}\"><text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"2.8\">{}</text><text x=\"{}\" y=\"{}\" text-anchor=\"end\" font-family=\"monospace\" font-size=\"2.8\">≈ {:.3} mm</text></g>", table.id, row.dimension, row.dimension, table.x + 3., baseline, escape(&row.label)?, table.x + table.width - 3., baseline, row.value_mm).unwrap();
        }
        writeln!(out, "<text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"2\">Nominal native geometry</text>", table.x + 3., table.y + table.height - 2.).unwrap();
        out.push_str("</g>\n");
    }
    let title_y = sheet.height - 35.;
    writeln!(
        out,
        "<path d=\"M 10 {title_y} H {}\" stroke=\"black\" stroke-width=\"0.25\"/>",
        sheet.width - 10.
    )
    .unwrap();
    for (i, text) in [
        sheet.title.clone(),
        format!("Revision: {}", sheet.revision),
        format!("Source context: {}", sheet.source_context_hash),
    ]
    .iter()
    .enumerate()
    {
        let font = 3_f64.min((sheet.width - 24.) / (text.chars().count().max(1) as f64));
        writeln!(
            out,
            "<text x=\"12\" y=\"{}\" font-family=\"monospace\" font-size=\"{font}\">{}</text>",
            title_y + 6. + i as f64 * 7.,
            escape(text)?
        )
        .unwrap();
    }
    out.push_str("</svg>\n");
    // Native section markup contributes its own generated IDs. Check the
    // complete rendered document as well as authored table references, so a
    // table row can never alias a section element or another row.
    let mut svg_ids = BTreeSet::new();
    const ID_ATTR: &str = " id=\"";
    for (offset, _) in out.match_indices(ID_ATTR) {
        let id = out[offset + ID_ATTR.len()..]
            .split_once('"')
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("invalid generated drawing SVG ID"))?;
        if !svg_ids.insert(id.to_ascii_lowercase()) {
            return Err(invalid("generated drawing SVG IDs overlap"));
        }
    }
    Ok(out)
}
