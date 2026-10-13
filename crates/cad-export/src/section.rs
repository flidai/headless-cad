//! Deterministic, true-scale millimetre SVG for exact native section curves.
use crate::ExportError;
use cad_kernel::{FaceId, PlanarSection, SectionCurve};
use std::{collections::BTreeMap, fmt::Write};

fn number(v: f64) -> String {
    if v == 0. {
        "0".into()
    } else {
        v.to_string()
    }
}
fn point(p: [f64; 2]) -> String {
    format!("{} {}", number(p[0]), number(-p[1]))
}
fn escape(s: &str) -> Result<String, ExportError> {
    if s.chars().any(|c| {
        ((c as u32) < 32 && !matches!(c, '\t' | '\n' | '\r'))
            || matches!(c, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(ExportError::InvalidModel(
            "section label contains an invalid XML character".into(),
        ));
    }
    Ok(s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}
fn invalid() -> ExportError {
    ExportError::InvalidModel(
        "section SVG requires finite native line/arc geometry and properties".into(),
    )
}

/// Render native section coordinates at one SVG user unit per millimetre.
///
/// This inspection artifact carries exact circular arcs; it is not a drawing,
/// tolerance specification, manufacturing approval, or replacement for source.
pub fn section_to_svg(section: &PlanarSection, label: &str) -> Result<String, ExportError> {
    annotated_section_to_svg(section, label, &[])
}

/// Source-authored manufacturing size limits, distinct from kernel tolerance.
#[derive(Debug, Clone)]
pub struct DimensionToleranceAnnotation {
    pub nominal_mm: f64,
    pub lower_deviation_mm: f64,
    pub upper_deviation_mm: f64,
}
fn tolerance_label(t: &DimensionToleranceAnnotation) -> Result<String, ExportError> {
    let lower = t.nominal_mm + t.lower_deviation_mm;
    let upper = t.nominal_mm + t.upper_deviation_mm;
    let width = upper - lower;
    let arithmetic = 128. * f64::EPSILON * lower.abs().max(upper.abs()).max(t.nominal_mm);
    if [
        t.nominal_mm,
        t.lower_deviation_mm,
        t.upper_deviation_mm,
        lower,
        upper,
        width,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || t.nominal_mm <= 0.
        || lower <= 0.
        || upper <= 0.
        || t.lower_deviation_mm >= t.upper_deviation_mm
        || width <= arithmetic
    {
        return Err(ExportError::InvalidModel(
            "invalid or numerically unresolved manufacturing size tolerance".into(),
        ));
    }
    let signed = |v: f64| {
        if v >= 0. {
            format!("+{}", number(v))
        } else {
            number(v)
        }
    };
    Ok(format!(
        "{} {}/{}",
        number(t.nominal_mm),
        signed(t.upper_deviation_mm),
        signed(t.lower_deviation_mm)
    ))
}

/// Projected linear inspection annotation in section-plane millimetres.
/// `axis` is 0 (X) or 1 (Y); `offset` is the absolute perpendicular coordinate
/// of the dimension line. Optional size deviations carry authored manufacturing
/// limits; no fit class or GD&T interpretation is performed.
#[derive(Debug, Clone)]
pub struct SectionDimensionAnnotation {
    pub tolerance: Option<DimensionToleranceAnnotation>,
    pub id: String,
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub axis: usize,
    pub offset: f64,
    pub value_mm: f64,
}

/// Render section geometry and projected dimensions in a common bounded canvas.
pub fn annotated_section_to_svg(
    section: &PlanarSection,
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
) -> Result<String, ExportError> {
    dimensioned_section_to_svg(section, label, dimensions, &[])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionCircleDimensionKind {
    Radius,
    Diameter,
}

/// A native circular section measurement, with a label offset from its center.
#[derive(Debug, Clone)]
pub struct SectionCircleDimensionAnnotation {
    pub tolerance: Option<DimensionToleranceAnnotation>,
    pub id: String,
    pub center: [f64; 2],
    pub radius: f64,
    pub kind: SectionCircleDimensionKind,
    pub label_offset: [f64; 2],
    pub value_mm: f64,
    /// Exact selected partial arc. Absent for the existing complete-circle mode.
    pub arc: Option<SectionArcWitnessAnnotation>,
}

/// A source-bound section arc used to place a radial arrow on actual geometry.
#[derive(Debug, Clone)]
pub struct SectionArcWitnessAnnotation {
    /// Index into the source occurrence sections (zero for a body section).
    pub occurrence_index: usize,
    pub target_face: FaceId,
    pub loop_index: usize,
    pub segment_index: usize,
    pub start_angle: f64,
    pub sweep_angle: f64,
    pub tip: [f64; 2],
    pub arithmetic_uncertainty_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionSlotDimensionKind {
    Width,
    CenterlineLength,
    OverallLength,
}

/// A native capsule section measurement. All coordinates are in the section
/// plane; the source report retains the selected native curve witnesses.
#[derive(Debug, Clone)]
pub struct SectionSlotDimensionAnnotation {
    pub tolerance: Option<DimensionToleranceAnnotation>,
    pub id: String,
    pub kind: SectionSlotDimensionKind,
    pub center: [f64; 2],
    pub centerline_endpoints: [[f64; 2]; 2],
    pub axis: [f64; 2],
    pub radius_mm: f64,
    /// Native projection plus placement arithmetic guard, not design tolerance.
    pub arithmetic_uncertainty_mm: f64,
    pub label_offset: [f64; 2],
    pub value_mm: f64,
}

fn validate_arc_annotations(
    sections: &[PlanarSection],
    circles: &[SectionCircleDimensionAnnotation],
) -> Result<(), ExportError> {
    for annotation in circles {
        let Some(arc) = &annotation.arc else { continue };
        if annotation.kind != SectionCircleDimensionKind::Radius {
            return Err(ExportError::InvalidModel(
                "partial arc requires a radius dimension".into(),
            ));
        }
        let section = sections.get(arc.occurrence_index).ok_or_else(invalid)?;
        let segment = section
            .loops
            .get(arc.loop_index)
            .and_then(|l| l.segments.get(arc.segment_index))
            .ok_or_else(invalid)?;
        let SectionCurve::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } = segment.curve
        else {
            return Err(ExportError::InvalidModel(
                "selected radius witness is not a native arc".into(),
            ));
        };
        let scale = annotation
            .center
            .into_iter()
            .chain(center)
            .chain([annotation.radius, radius, arc.tip[0], arc.tip[1]])
            .map(f64::abs)
            .fold(1., f64::max);
        let angular = 256. * f64::EPSILON * (1. + start_angle.abs() + sweep_angle.abs());
        let error = scale * 512. * f64::EPSILON + radius.abs() * angular;
        let expected_tip = [
            center[0] + radius * (start_angle + sweep_angle / 2.).cos(),
            center[1] + radius * (start_angle + sweep_angle / 2.).sin(),
        ];
        let count = section
            .loops
            .iter()
            .flat_map(|l| &l.segments)
            .filter(|s| s.face == arc.target_face)
            .count();
        if [
            arc.start_angle,
            arc.sweep_angle,
            arc.tip[0],
            arc.tip[1],
            start_angle,
            sweep_angle,
            center[0],
            center[1],
            radius,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || !scale.is_finite()
            || !error.is_finite()
            || !angular.is_finite()
            || segment.face != arc.target_face
            || count != 1
            || !radius.is_normal()
            || radius <= 0.
            || !sweep_angle.is_finite()
            || sweep_angle.abs() <= angular
            || sweep_angle.abs() >= std::f64::consts::TAU - angular
            || radius * sweep_angle.abs() <= error
            || !arc.arithmetic_uncertainty_mm.is_finite()
            || arc.arithmetic_uncertainty_mm < 0.
            || (annotation.center[0] - center[0]).hypot(annotation.center[1] - center[1]) > error
            || (annotation.radius - radius).abs() > error
            || (arc.start_angle - start_angle).abs() > 512. * f64::EPSILON
            || (arc.sweep_angle - sweep_angle).abs() > 512. * f64::EPSILON
            || (arc.tip[0] - expected_tip[0]).hypot(arc.tip[1] - expected_tip[1]) > error
        {
            return Err(ExportError::InvalidModel(
                "partial radius annotation does not match its native section arc".into(),
            ));
        }
    }
    Ok(())
}

/// Render native section geometry with projected and circular inspection dimensions.
pub fn dimensioned_section_to_svg(
    section: &PlanarSection,
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
) -> Result<String, ExportError> {
    validate_arc_annotations(std::slice::from_ref(section), circles)?;
    Ok(section_layout(section, label, dimensions, circles)?.svg)
}
pub(crate) struct SectionLayout {
    pub svg: String,
    pub view_box: [f64; 4],
}
/// Paper drawing presentation only. Native dimension IDs and values are kept
/// in the generated groups; tags replace visible long source identifiers.
pub(crate) struct PaperAnnotationStyle<'a> {
    pub font_model_mm: f64,
    pub tags: &'a BTreeMap<String, String>,
}
fn section_layout(
    section: &PlanarSection,
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
) -> Result<SectionLayout, ExportError> {
    section_layout_with_style(section, label, dimensions, circles, &[], None)
}
fn section_layout_with_style(
    section: &PlanarSection,
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
    slots: &[SectionSlotDimensionAnnotation],
    style: Option<&PaperAnnotationStyle<'_>>,
) -> Result<SectionLayout, ExportError> {
    let mut ids = std::collections::BTreeSet::new();
    if dimensions
        .iter()
        .map(|d| &d.id)
        .chain(circles.iter().map(|d| &d.id))
        .chain(slots.iter().map(|d| &d.id))
        .any(|id| !ids.insert(id))
    {
        return Err(ExportError::InvalidModel(
            "section dimension IDs must be unique".into(),
        ));
    }
    let label = escape(label)?;
    if !section.area.is_finite()
        || section.area < 0.
        || section
            .centroid
            .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
        || section
            .plane
            .origin
            .iter()
            .chain(&section.plane.normal)
            .chain(&section.plane.x_direction)
            .any(|v| !v.is_finite())
    {
        return Err(invalid());
    }
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut path = String::new();
    let mut include = |p: [f64; 2]| {
        bounds[0] = bounds[0].min(p[0]);
        bounds[1] = bounds[1].min(p[1]);
        bounds[2] = bounds[2].max(p[0]);
        bounds[3] = bounds[3].max(p[1]);
    };
    for boundary in &section.loops {
        if boundary.segments.is_empty() {
            return Err(invalid());
        }
        for (index, segment) in boundary.segments.iter().enumerate() {
            match segment.curve {
                SectionCurve::Line { start, end } => {
                    if start.iter().chain(&end).any(|v| !v.is_finite()) {
                        return Err(invalid());
                    }
                    include(start);
                    include(end);
                    if index == 0 {
                        write!(path, "M {} ", point(start)).unwrap();
                    }
                    write!(path, "L {} ", point(end)).unwrap();
                }
                SectionCurve::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                } => {
                    if center
                        .iter()
                        .chain([&radius, &start_angle, &sweep_angle])
                        .any(|v| !v.is_finite())
                        || radius <= 0.
                        || sweep_angle == 0.
                        || sweep_angle.abs() > std::f64::consts::TAU + 1e-12
                    {
                        return Err(invalid());
                    }
                    let at = |angle: f64| {
                        [
                            center[0] + radius * angle.cos(),
                            center[1] + radius * angle.sin(),
                        ]
                    };
                    // A conservative support-circle bound cannot clip a major arc.
                    include([center[0] - radius, center[1] - radius]);
                    include([center[0] + radius, center[1] + radius]);
                    if index == 0 {
                        write!(path, "M {} ", point(at(start_angle))).unwrap();
                    }
                    // Two arcs also represent a full circle without coincident-endpoint ambiguity.
                    for step in [0.5, 1.] {
                        write!(
                            path,
                            "A {} {} 0 0 {} {} ",
                            number(radius),
                            number(radius),
                            u8::from(sweep_angle < 0.),
                            point(at(start_angle + sweep_angle * step))
                        )
                        .unwrap();
                    }
                }
            }
        }
        path.push_str("Z ");
    }
    if section.loops.is_empty() {
        bounds = [-1., -1., 1., 1.];
    }
    let span = (bounds[2] - bounds[0]).max(bounds[3] - bounds[1]).max(1.);
    let font = style.map_or(span * 0.025, |s| s.font_model_mm);
    if !font.is_finite() || font <= 0. {
        return Err(invalid());
    }
    if let Some(style) = style {
        let actual: std::collections::BTreeSet<_> = dimensions
            .iter()
            .map(|d| d.id.as_str())
            .chain(circles.iter().map(|d| d.id.as_str()))
            .chain(slots.iter().map(|d| d.id.as_str()))
            .collect();
        let authored: std::collections::BTreeSet<_> =
            style.tags.keys().map(String::as_str).collect();
        let unique_tags: std::collections::BTreeSet<_> =
            style.tags.values().map(String::as_str).collect();
        if actual != authored
            || unique_tags.len() != style.tags.len()
            || style.tags.values().any(|tag| {
                tag.is_empty()
                    || tag.len() > 12
                    || !tag.as_bytes()[0].is_ascii_alphabetic()
                    || !tag.as_bytes()[1..]
                        .iter()
                        .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
            })
        {
            return Err(ExportError::InvalidModel(
                "paper drawing tags must cover every native section annotation".into(),
            ));
        }
    }
    let mut label_boxes = Vec::new();
    let mut slot_segments = Vec::new();
    let mut annotations = dimension_svg(
        dimensions,
        font,
        &mut bounds,
        style.map(|s| s.tags),
        &mut label_boxes,
    )?;
    annotations.push_str(&circle_dimension_svg(
        circles,
        font,
        &mut bounds,
        style.map(|s| s.tags),
        &mut label_boxes,
    )?);
    annotations.push_str(&slot_dimension_svg(
        slots,
        font,
        &mut bounds,
        style.map(|s| s.tags),
        &mut label_boxes,
        &mut slot_segments,
    )?);
    if style.is_some() {
        for (i, a) in label_boxes.iter().enumerate() {
            if label_boxes[..i]
                .iter()
                .any(|b| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3])
            {
                return Err(ExportError::InvalidModel(
                    "paper drawing annotation labels overlap".into(),
                ));
            }
        }
        for (from, to, half_stroke) in &slot_segments {
            for label in &label_boxes {
                let expanded = [
                    label[0] - half_stroke,
                    label[1] - half_stroke,
                    label[2] + half_stroke,
                    label[3] + half_stroke,
                ];
                if segment_intersects_box(*from, *to, expanded) {
                    return Err(ExportError::InvalidModel(
                        "paper drawing slot line crosses an annotation label".into(),
                    ));
                }
            }
        }
    }
    let margin = span * 0.06;
    let width = bounds[2] - bounds[0] + 2. * margin;
    let height = bounds[3] - bounds[1] + 2. * margin + if style.is_some() { 0. } else { font * 6. };
    let left = bounds[0] - margin;
    let top = -bounds[3] - margin;
    if [span, margin, font, width, height, left, top]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err(invalid());
    }
    let vector = |v: [f64; 3]| v.map(number).join(", ");
    let frame = format!(
        "origin ({}) mm; normal ({}); X ({})",
        vector(section.plane.origin),
        vector(section.plane.normal),
        vector(section.plane.x_direction)
    );
    let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\" role=\"img\">\n<title>{label}</title>\n<desc>Exact native section; coordinates in millimetres, X right, Y up. {}. Inspection only; no manufacturing tolerances or GD&amp;T.</desc>\n",number(width),number(height),number(left),number(top),number(width),number(height),escape(&frame)?);
    writeln!(out,"<path id=\"section-contours\" d=\"{}\" fill=\"#cbd5e1\" fill-rule=\"evenodd\" stroke=\"#0f172a\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",path.trim(),number(span*0.0015)).unwrap();
    out.push_str(&annotations);
    if style.is_none() {
        let y = -bounds[1] + margin + font;
        for (row, text) in [
            format!("{label} — section, mm; area {} mm²", number(section.area)),
            format!("Origin: ({}) mm", vector(section.plane.origin)),
            format!(
                "Normal: ({}); X axis: ({})",
                vector(section.plane.normal),
                vector(section.plane.x_direction)
            ),
        ]
        .into_iter()
        .enumerate()
        {
            // The first row already contains the escaped caller label; numeric frame rows are trusted.
            writeln!(out,"<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"#0f172a\">{text}</text>",number(bounds[0]),number(y+row as f64*font*1.5),number(font.min(width / (text.chars().count().max(1) as f64 * 0.65)))).unwrap();
        }
    }
    out.push_str("</svg>\n");
    if !slots.is_empty() {
        let detail = if dimensions.iter().any(|d| d.tolerance.is_some())
            || circles.iter().any(|d| d.tolerance.is_some())
            || slots.iter().any(|d| d.tolerance.is_some())
        {
            "Native nominal geometry with source-authored manufacturing size deviations in mm; independent of modeling tolerance. No GD&amp;T or manufactured-part conformance."
        } else {
            "Native nominal geometry; inspection only. No GD&amp;T or manufactured-part conformance."
        };
        out = out.replace(
            "Inspection only; no manufacturing tolerances or GD&amp;T.",
            detail,
        );
    } else if dimensions.iter().any(|d| d.tolerance.is_some())
        || circles.iter().any(|d| d.tolerance.is_some())
    {
        out = out.replace("Inspection only; no manufacturing tolerances or GD&amp;T.", "Source-authored manufacturing size deviations in mm; independent of modeling tolerance. No fit classes or GD&amp;T.");
    }
    Ok(SectionLayout {
        svg: out,
        view_box: [left, top, width, height],
    })
}

/// Render separate occurrences in one common section frame. Each occurrence has
/// its own even-odd path: overlapping solids stay filled rather than cancelling.
pub fn sections_to_svg(sections: &[PlanarSection], label: &str) -> Result<String, ExportError> {
    annotated_sections_to_svg(sections, label, &[])
}

/// Render each occurrence separately, with shared projected dimension annotations.
pub fn annotated_sections_to_svg(
    sections: &[PlanarSection],
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
) -> Result<String, ExportError> {
    dimensioned_sections_to_svg(sections, label, dimensions, &[])
}

/// Keep occurrence fills separate while sharing a common dimension canvas.
pub fn dimensioned_sections_to_svg(
    sections: &[PlanarSection],
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
) -> Result<String, ExportError> {
    slot_dimensioned_sections_to_svg(sections, label, dimensions, circles, &[])
}

pub fn slot_dimensioned_sections_to_svg(
    sections: &[PlanarSection],
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
    slots: &[SectionSlotDimensionAnnotation],
) -> Result<String, ExportError> {
    Ok(sections_layout_with_slots(sections, label, dimensions, circles, slots)?.svg)
}
pub(crate) fn sections_layout_with_slots(
    sections: &[PlanarSection],
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
    slots: &[SectionSlotDimensionAnnotation],
) -> Result<SectionLayout, ExportError> {
    sections_layout_with_style(sections, label, dimensions, circles, slots, None)
}
pub(crate) fn sections_layout_with_style(
    sections: &[PlanarSection],
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
    slots: &[SectionSlotDimensionAnnotation],
    style: Option<&PaperAnnotationStyle<'_>>,
) -> Result<SectionLayout, ExportError> {
    let first = sections.first().ok_or_else(invalid)?;
    validate_arc_annotations(sections, circles)?;
    if sections.iter().any(|section| section.plane != first.plane) {
        return Err(ExportError::InvalidModel(
            "section SVG occurrences require a common plane frame".into(),
        ));
    }
    let area: f64 = sections.iter().map(|section| section.area).sum();
    let aggregate = PlanarSection {
        plane: first.plane,
        loops: sections
            .iter()
            .flat_map(|section| section.loops.clone())
            .collect(),
        area,
        centroid: if area == 0. {
            None
        } else {
            Some(std::array::from_fn(|axis| {
                sections
                    .iter()
                    .map(|section| section.area * section.centroid.map_or(0., |p| p[axis]))
                    .sum::<f64>()
                    / area
            }))
        },
    };
    let layout = section_layout_with_style(&aggregate, label, dimensions, circles, slots, style)?;
    let base = layout.svg.replace(
        "Exact native section;",
        "Native occurrence sections; overlapping occurrence areas are summed;",
    );
    let mut paths = String::new();
    for (index, section) in sections.iter().enumerate() {
        let svg = section_to_svg(section, label)?;
        let path = svg
            .lines()
            .find(|line| line.starts_with("<path "))
            .ok_or_else(invalid)?;
        writeln!(
            paths,
            "{}",
            path.replace(
                "id=\"section-contours\"",
                &format!("id=\"section-contours-{index}\" data-occurrence-index=\"{index}\"")
            )
        )
        .unwrap();
    }
    let mut out = String::new();
    for line in base.lines() {
        if line.starts_with("<path ") {
            out.push_str(&paths);
        } else {
            writeln!(out, "{line}").unwrap();
        }
    }
    Ok(SectionLayout {
        svg: out,
        view_box: layout.view_box,
    })
}

fn dimension_svg(
    dimensions: &[SectionDimensionAnnotation],
    font: f64,
    bounds: &mut [f64; 4],
    tags: Option<&BTreeMap<String, String>>,
    label_boxes: &mut Vec<[f64; 4]>,
) -> Result<String, ExportError> {
    let mut out = String::new();
    let mut ids = std::collections::BTreeSet::new();
    for dimension in dimensions {
        let SectionDimensionAnnotation {
            id,
            from,
            to,
            axis,
            offset,
            value_mm,
            tolerance: manufacturing_tolerance,
        } = dimension;
        if *axis > 1
            || id.is_empty()
            || !ids.insert(id)
            || from
                .iter()
                .chain(to)
                .chain([offset, value_mm])
                .any(|v| !v.is_finite())
            || *value_mm <= 0.
        {
            return Err(invalid());
        }
        let actual = (to[*axis] - from[*axis]).abs();
        // Serialization and rigid-frame arithmetic may introduce roundoff;
        // this is consistency validation, not an engineering tolerance claim.
        let coordinate_scale = from[*axis].abs().max(to[*axis].abs());
        let tolerance = 128. * f64::EPSILON * actual.max(*value_mm).max(coordinate_scale);
        if !actual.is_finite()
            || actual <= 0.
            // A rounding budget comparable to the measurement cannot establish
            // endpoint/value consistency. Reject instead of drawing false precision.
            || tolerance / actual.min(*value_mm) >= 0.01
            || (actual - value_mm).abs() > tolerance
        {
            return Err(ExportError::InvalidModel(
                "section dimension does not match its projected endpoints".into(),
            ));
        }
        let escaped_id = escape(id)?
            .replace('\n', "&#10;")
            .replace('\r', "&#13;")
            .replace('\t', "&#9;");
        let mut value = format!("{value_mm:.6}");
        while value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
        // Do not label a nonzero sub-micrometre projection as zero.
        if value == "0" {
            value = format!("{value_mm:.6e}");
        }
        if let Some(t) = manufacturing_tolerance {
            value = tolerance_label(t)?;
        }
        let visible = tags
            .and_then(|map| map.get(id))
            .map_or(id.as_str(), String::as_str);
        let label = format!("{visible}: {value} mm");
        let escaped_label = escape(&label)?;
        let text_width = if tags.is_some() {
            label.len() as f64 * font * 0.7
        } else {
            label.chars().count() as f64 * font * 0.7
        };
        let mut a = *from;
        let mut b = *to;
        a[1 - *axis] = *offset;
        b[1 - *axis] = *offset;
        let direction = (b[*axis] - a[*axis]).signum();
        let arrow = (font * 0.6).min(actual * 0.2);
        let mut arrows = [[a; 3], [b; 3]];
        for (index, points) in arrows.iter_mut().enumerate() {
            let inward = if index == 0 { direction } else { -direction };
            points[1][*axis] += inward * arrow;
            points[2][*axis] += inward * arrow;
            points[1][1 - *axis] += arrow * 0.4;
            points[2][1 - *axis] -= arrow * 0.4;
        }
        let midpoint = [a[0] * 0.5 + b[0] * 0.5, a[1] * 0.5 + b[1] * 0.5];
        let text_center = if *axis == 0 {
            midpoint[0]
        } else {
            midpoint[0] + font + text_width * 0.5
        };
        let text_baseline = midpoint[1] + font;
        // Explicit textLength makes the visible label's width match its bound,
        // including escaped IDs and fonts supplied by different SVG consumers.
        let label_min = [text_center - text_width * 0.5, text_baseline - font * 0.4];
        let label_max = [text_center + text_width * 0.5, text_baseline + font * 1.2];
        label_boxes.push([label_min[0], label_min[1], label_max[0], label_max[1]]);
        for p in [*from, *to, a, b, label_min, label_max]
            .into_iter()
            .chain(arrows.into_iter().flatten())
        {
            if p.iter().any(|v| !v.is_finite()) {
                return Err(invalid());
            }
            bounds[0] = bounds[0].min(p[0]);
            bounds[1] = bounds[1].min(p[1]);
            bounds[2] = bounds[2].max(p[0]);
            bounds[3] = bounds[3].max(p[1]);
        }
        writeln!(out, "<g class=\"section-dimension\" data-id=\"{escaped_id}\" data-value-mm=\"{}\" fill=\"#334155\" stroke=\"#334155\" stroke-width=\"{}\">", number(*value_mm),number(font*0.05)).unwrap();
        for (start, end) in [(*from, a), (*to, b), (a, b)] {
            writeln!(
                out,
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>",
                number(start[0]),
                number(-start[1]),
                number(end[0]),
                number(-end[1])
            )
            .unwrap();
        }
        for points in arrows {
            writeln!(
                out,
                "  <polygon points=\"{}\"/>",
                points.map(point).join(" ")
            )
            .unwrap();
        }
        if tags.is_some() {
            writeln!(out,"  <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"monospace\" font-size=\"{}\" stroke=\"none\">{escaped_label}</text>\n</g>",number(text_center),number(-text_baseline),number(font)).unwrap();
        } else {
            writeln!(out,"  <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" stroke=\"none\">{escaped_label}</text>\n</g>",number(text_center),number(-text_baseline),number(font),number(text_width)).unwrap();
        }
    }
    Ok(out)
}

fn segment_intersects_box(from: [f64; 2], to: [f64; 2], bounds: [f64; 4]) -> bool {
    let mut enter: f64 = 0.;
    let mut leave: f64 = 1.;
    for axis in 0..2 {
        let delta = to[axis] - from[axis];
        if delta == 0. {
            if from[axis] < bounds[axis] || from[axis] > bounds[axis + 2] {
                return false;
            }
            continue;
        }
        let a = (bounds[axis] - from[axis]) / delta;
        let b = (bounds[axis + 2] - from[axis]) / delta;
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
        if enter > leave {
            return false;
        }
    }
    true
}

fn slot_dimension_svg(
    slots: &[SectionSlotDimensionAnnotation],
    font: f64,
    bounds: &mut [f64; 4],
    tags: Option<&BTreeMap<String, String>>,
    label_boxes: &mut Vec<[f64; 4]>,
    slot_segments: &mut Vec<([f64; 2], [f64; 2], f64)>,
) -> Result<String, ExportError> {
    let mut out = String::new();
    for slot in slots {
        let SectionSlotDimensionAnnotation {
            tolerance,
            id,
            kind,
            center,
            centerline_endpoints,
            axis,
            radius_mm,
            arithmetic_uncertainty_mm,
            label_offset,
            value_mm,
        } = slot;
        let coordinates = center
            .iter()
            .chain(centerline_endpoints.iter().flatten())
            .chain(axis)
            .chain(label_offset)
            .chain([radius_mm, value_mm, arithmetic_uncertainty_mm]);
        if id.is_empty()
            || coordinates.into_iter().any(|v| !v.is_finite())
            || *radius_mm <= 0.
            || *arithmetic_uncertainty_mm < 0.
            || *value_mm <= 0.
        {
            return Err(invalid());
        }
        let length = (centerline_endpoints[1][0] - centerline_endpoints[0][0])
            .hypot(centerline_endpoints[1][1] - centerline_endpoints[0][1]);
        let unit = axis[0].hypot(axis[1]);
        let scale = center
            .iter()
            .chain(centerline_endpoints.iter().flatten())
            .chain([radius_mm, value_mm, &length])
            .fold(1_f64, |a, b| a.max(b.abs()));
        let error = 512. * f64::EPSILON * scale;
        let middle = [
            (centerline_endpoints[0][0] + centerline_endpoints[1][0]) * 0.5,
            (centerline_endpoints[0][1] + centerline_endpoints[1][1]) * 0.5,
        ];
        if !length.is_finite()
            || length <= error
            || !unit.is_finite()
            || (unit - 1.).abs() > 512. * f64::EPSILON
            || (center[0] - middle[0]).hypot(center[1] - middle[1]) > error
            || (centerline_endpoints[1][0] - centerline_endpoints[0][0] - axis[0] * length).abs()
                > error
            || (centerline_endpoints[1][1] - centerline_endpoints[0][1] - axis[1] * length).abs()
                > error
            || error / length.min(*radius_mm) >= 0.01
        {
            return Err(ExportError::InvalidModel(
                "slot dimension does not match native capsule frame".into(),
            ));
        }
        let normal = [-axis[1], axis[0]];
        let (kind_name, expected, from, to, direction) = match kind {
            SectionSlotDimensionKind::Width => (
                "width",
                2. * radius_mm,
                [
                    center[0] - normal[0] * radius_mm,
                    center[1] - normal[1] * radius_mm,
                ],
                [
                    center[0] + normal[0] * radius_mm,
                    center[1] + normal[1] * radius_mm,
                ],
                normal,
            ),
            SectionSlotDimensionKind::CenterlineLength => (
                "centerline_length",
                length,
                centerline_endpoints[0],
                centerline_endpoints[1],
                *axis,
            ),
            SectionSlotDimensionKind::OverallLength => (
                "overall_length",
                length + 2. * radius_mm,
                [
                    centerline_endpoints[0][0] - axis[0] * radius_mm,
                    centerline_endpoints[0][1] - axis[1] * radius_mm,
                ],
                [
                    centerline_endpoints[1][0] + axis[0] * radius_mm,
                    centerline_endpoints[1][1] + axis[1] * radius_mm,
                ],
                *axis,
            ),
        };
        if !expected.is_finite()
            || (expected - value_mm).abs() > error
            || label_offset[0].hypot(label_offset[1]) <= error
        {
            return Err(ExportError::InvalidModel(
                "slot dimension value or label offset does not match native capsule".into(),
            ));
        }
        let a = [from[0] + label_offset[0], from[1] + label_offset[1]];
        let b = [to[0] + label_offset[0], to[1] + label_offset[1]];
        let arrow = (font * 0.6).min(expected * 0.2);
        let mut heads = [[a; 3], [b; 3]];
        let across = [-direction[1], direction[0]];
        for (index, points) in heads.iter_mut().enumerate() {
            let inward = if index == 0 {
                direction
            } else {
                [-direction[0], -direction[1]]
            };
            for (side, vertex) in [1, 2].into_iter().zip([-1., 1.]) {
                points[side] = [
                    points[0][0] + inward[0] * arrow + across[0] * arrow * 0.4 * vertex,
                    points[0][1] + inward[1] * arrow + across[1] * arrow * 0.4 * vertex,
                ];
            }
        }
        let mut value = format!("{value_mm:.6}");
        while value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
        if value == "0" {
            value = format!("{value_mm:.6e}");
        }
        if let Some(t) = tolerance {
            value = tolerance_label(t)?;
        }
        let visible = tags
            .and_then(|map| map.get(id))
            .map_or(id.as_str(), String::as_str);
        let prefix = match kind {
            SectionSlotDimensionKind::Width => "W",
            SectionSlotDimensionKind::CenterlineLength => "C",
            SectionSlotDimensionKind::OverallLength => "L",
        };
        let label = if tags.is_some() {
            format!("{visible}:{prefix}{value}mm")
        } else {
            format!("{visible}: {value} mm {kind_name}")
        };
        let text_width = label.len() as f64 * font * 0.7;
        let center_line = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let text_center = center_line[0];
        // Keep the full label beyond the dimension line, on the side away
        // from the slot. Lower length annotations otherwise intersect their
        // own extension lines between the slot and the offset line.
        let text_baseline = if label_offset[1] < 0. {
            a[1].min(b[1]) - 2. * font
        } else {
            a[1].max(b[1]) + font
        };
        let label_box = [
            text_center - text_width * 0.5,
            text_baseline - font * 0.4,
            text_center + text_width * 0.5,
            text_baseline + font * 1.2,
        ];
        label_boxes.push(label_box);
        let cross = (font * 0.45).min(*radius_mm * 0.45);
        let mark_centers = [*center, centerline_endpoints[0], centerline_endpoints[1]];
        for p in [
            from,
            to,
            a,
            b,
            [label_box[0], label_box[1]],
            [label_box[2], label_box[3]],
        ]
        .into_iter()
        .chain(heads.into_iter().flatten())
        .chain(mark_centers)
        {
            if p.iter().any(|v| !v.is_finite()) {
                return Err(invalid());
            }
            bounds[0] = bounds[0].min(p[0] - cross);
            bounds[1] = bounds[1].min(p[1] - cross);
            bounds[2] = bounds[2].max(p[0] + cross);
            bounds[3] = bounds[3].max(p[1] + cross);
        }
        let escaped_id = escape(id)?
            .replace('\n', "&#10;")
            .replace('\r', "&#13;")
            .replace('\t', "&#9;");
        writeln!(out, "<g class=\"slot-dimension\" data-dimension-id=\"{escaped_id}\" data-kind=\"{kind_name}\" data-value-mm=\"{}\" data-center-x=\"{}\" data-center-y=\"{}\" data-native-from-x=\"{}\" data-native-from-y=\"{}\" data-native-to-x=\"{}\" data-native-to-y=\"{}\" data-radius-mm=\"{}\" data-arithmetic-uncertainty-mm=\"{}\" fill=\"#334155\" stroke=\"#334155\" stroke-width=\"{}\">", number(*value_mm),number(center[0]),number(center[1]),number(from[0]),number(from[1]),number(to[0]),number(to[1]),number(*radius_mm),number(*arithmetic_uncertainty_mm),number(font*0.05)).unwrap();
        for (start, end) in [(from, a), (to, b), (a, b)] {
            slot_segments.push((start, end, font * 0.025));
            writeln!(
                out,
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>",
                number(start[0]),
                number(-start[1]),
                number(end[0]),
                number(-end[1])
            )
            .unwrap();
        }
        for head in heads {
            writeln!(out, "  <polygon points=\"{}\"/>", head.map(point).join(" ")).unwrap();
        }
        for p in mark_centers {
            writeln!(out,"  <path class=\"slot-center-mark\" d=\"M {} {} L {} {} M {} {} L {} {}\" fill=\"none\"/>",number(p[0]-cross),number(-p[1]),number(p[0]+cross),number(-p[1]),number(p[0]),number(-(p[1]-cross)),number(p[0]),number(-(p[1]+cross))).unwrap();
        }
        writeln!(out,"  <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"{}\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" stroke=\"none\">{}</text>\n</g>",number(text_center),number(-text_baseline),if tags.is_some(){"monospace"}else{"sans-serif"},number(font),number(text_width),escape(&label)?).unwrap();
    }
    Ok(out)
}

fn circle_dimension_svg(
    circles: &[SectionCircleDimensionAnnotation],
    font: f64,
    bounds: &mut [f64; 4],
    tags: Option<&BTreeMap<String, String>>,
    label_boxes: &mut Vec<[f64; 4]>,
) -> Result<String, ExportError> {
    let mut out = String::new();
    for circle in circles {
        let SectionCircleDimensionAnnotation {
            id,
            center,
            radius,
            kind,
            label_offset,
            value_mm,
            tolerance: manufacturing_tolerance,
            arc,
        } = circle;
        if id.is_empty()
            || center
                .iter()
                .chain(label_offset)
                .chain([radius, value_mm])
                .any(|v| !v.is_finite())
            || *radius <= 0.
            || *value_mm <= 0.
        {
            return Err(invalid());
        }
        let (kind_name, expected) = match kind {
            SectionCircleDimensionKind::Radius => ("radius", *radius),
            SectionCircleDimensionKind::Diameter => ("diameter", radius * 2.),
        };
        let distance = label_offset[0].hypot(label_offset[1]);
        let scale = center[0]
            .abs()
            .max(center[1].abs())
            .max(*radius)
            .max(distance);
        let rounding = 128. * f64::EPSILON * scale;
        if !expected.is_finite()
            || !distance.is_finite()
            || distance <= *radius
            || rounding / radius >= 0.01
            || distance - radius <= rounding
            || (expected - value_mm).abs() > 128. * f64::EPSILON * expected.max(*value_mm)
        {
            return Err(ExportError::InvalidModel(
                "invalid or unresolved circular section dimension".into(),
            ));
        }
        let label_direction = [label_offset[0] / distance, label_offset[1] / distance];
        let near = arc.as_ref().map_or_else(
            || {
                [
                    center[0] + label_direction[0] * radius,
                    center[1] + label_direction[1] * radius,
                ]
            },
            |arc| arc.tip,
        );
        let direction = if arc.is_some() {
            [
                (near[0] - center[0]) / radius,
                (near[1] - center[1]) / radius,
            ]
        } else {
            label_direction
        };
        let far = [
            center[0] - direction[0] * radius,
            center[1] - direction[1] * radius,
        ];
        let leader = [center[0] + label_offset[0], center[1] + label_offset[1]];
        let elbow = arc.as_ref().map(|_| {
            [
                near[0] + direction[0] * font * 1.5,
                near[1] + direction[1] * font * 1.5,
            ]
        });
        let start = if *kind == SectionCircleDimensionKind::Diameter {
            far
        } else {
            *center
        };
        let arrow = (font * 0.6).min(radius * 0.2);
        let mut arrows = Vec::new();
        for (tip, sign) in if *kind == SectionCircleDimensionKind::Diameter {
            vec![(near, -1.), (far, 1.)]
        } else {
            vec![(near, -1.)]
        } {
            arrows.push([
                tip,
                [
                    tip[0] + sign * direction[0] * arrow - direction[1] * arrow * 0.4,
                    tip[1] + sign * direction[1] * arrow + direction[0] * arrow * 0.4,
                ],
                [
                    tip[0] + sign * direction[0] * arrow + direction[1] * arrow * 0.4,
                    tip[1] + sign * direction[1] * arrow - direction[0] * arrow * 0.4,
                ],
            ]);
        }
        let mut value = format!("{value_mm:.6}");
        while value.ends_with('0') {
            value.pop();
        }
        if value.ends_with('.') {
            value.pop();
        }
        if value == "0" {
            value = format!("{value_mm:.6e}");
        }
        if let Some(t) = manufacturing_tolerance {
            value = tolerance_label(t)?;
        }
        let visible = tags
            .and_then(|map| map.get(id))
            .map_or(id.as_str(), String::as_str);
        let label = if tags.is_some() {
            let symbol = match kind {
                SectionCircleDimensionKind::Radius => "R",
                SectionCircleDimensionKind::Diameter => "⌀",
            };
            format!("{visible}:{symbol}{value}mm")
        } else {
            format!("{visible}: {value} mm {kind_name}")
        };
        let escaped_label = escape(&label)?;
        let escaped_id = escape(id)?
            .replace('\n', "&#10;")
            .replace('\r', "&#13;")
            .replace('\t', "&#9;");
        let width = font
            * 0.7
            * if tags.is_some() {
                label.len()
            } else {
                label.chars().count()
            } as f64;
        let right = label_direction[0] >= 0.;
        let text_x = leader[0] + if right { font } else { -font - width };
        let baseline = leader[1] + font;
        label_boxes.push([
            text_x,
            baseline - font * 0.4,
            text_x + width,
            baseline + font * 1.2,
        ]);
        for p in [
            start,
            near,
            leader,
            [text_x, baseline - font * 0.4],
            [text_x + width, baseline + font * 1.2],
        ]
        .into_iter()
        .chain(elbow)
        .chain(arrows.iter().flatten().copied())
        {
            if p.iter().any(|v| !v.is_finite()) {
                return Err(invalid());
            }
            bounds[0] = bounds[0].min(p[0]);
            bounds[1] = bounds[1].min(p[1]);
            bounds[2] = bounds[2].max(p[0]);
            bounds[3] = bounds[3].max(p[1]);
        }
        writeln!(out,"<g class=\"section-circle-dimension\" data-id=\"{escaped_id}\" data-value-mm=\"{}\" data-kind=\"{kind_name}\" fill=\"#334155\" stroke=\"#334155\" stroke-width=\"{}\">",number(*value_mm),number(font*0.05)).unwrap();
        let segments = if let Some(elbow) = elbow {
            vec![(start, near), (near, elbow), (elbow, leader)]
        } else {
            vec![(start, near), (near, leader)]
        };
        for (a, b) in segments {
            writeln!(
                out,
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>",
                number(a[0]),
                number(-a[1]),
                number(b[0]),
                number(-b[1])
            )
            .unwrap();
        }
        for points in arrows {
            writeln!(
                out,
                "  <polygon points=\"{}\"/>",
                points.map(point).join(" ")
            )
            .unwrap();
        }
        if tags.is_some() {
            writeln!(out,"  <text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"{}\" stroke=\"none\">{escaped_label}</text>\n</g>",number(text_x),number(-baseline),number(font)).unwrap();
        } else {
            writeln!(out,"  <text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" stroke=\"none\">{escaped_label}</text>\n</g>",number(text_x),number(-baseline),number(font),number(width)).unwrap();
        }
    }
    Ok(out)
}
