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
    {
        return Err(invalid("invalid drawing paper or view count"));
    }
    let mut ids = BTreeSet::new();
    let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{}</title>\n<desc>Native analytic section sheet. Explicit scales; no projected hidden-line removal or GD&amp;T.</desc>\n<rect width=\"{}\" height=\"{}\" fill=\"white\"/>\n<rect x=\"10\" y=\"10\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.25\"/>\n",sheet.width,sheet.height,sheet.width,sheet.height,escape(&sheet.title)?,sheet.width,sheet.height,sheet.width-20.,sheet.height-20.);
    for (i, view) in sheet.views.iter().enumerate() {
        if view.id.is_empty()
            || !view
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
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
            view.x < other.x + other.width
                && other.x < view.x + view.width
                && view.y < other.y + other.height
                && other.y < view.y + view.height
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
    Ok(out)
}
