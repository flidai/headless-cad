//! Deterministic, true-scale millimetre SVG for exact native section curves.
use crate::ExportError;
use cad_kernel::{PlanarSection, SectionCurve};
use std::fmt::Write;

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

/// Projected linear inspection annotation in section-plane millimetres.
/// `axis` is 0 (X) or 1 (Y); `offset` is the absolute perpendicular coordinate
/// of the dimension line. This carries no fit, tolerance, or GD&T meaning.
#[derive(Debug, Clone)]
pub struct SectionDimensionAnnotation {
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
    pub id: String,
    pub center: [f64; 2],
    pub radius: f64,
    pub kind: SectionCircleDimensionKind,
    pub label_offset: [f64; 2],
    pub value_mm: f64,
}

/// Render native section geometry with projected and circular inspection dimensions.
pub fn dimensioned_section_to_svg(
    section: &PlanarSection,
    label: &str,
    dimensions: &[SectionDimensionAnnotation],
    circles: &[SectionCircleDimensionAnnotation],
) -> Result<String, ExportError> {
    let mut ids = std::collections::BTreeSet::new();
    if dimensions
        .iter()
        .map(|d| &d.id)
        .chain(circles.iter().map(|d| &d.id))
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
    let font = span * 0.025;
    let mut annotations = dimension_svg(dimensions, font, &mut bounds)?;
    annotations.push_str(&circle_dimension_svg(circles, font, &mut bounds)?);
    let margin = span * 0.06;
    let width = bounds[2] - bounds[0] + 2. * margin;
    let height = bounds[3] - bounds[1] + 2. * margin + font * 6.;
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
    out.push_str("</svg>\n");
    Ok(out)
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
    let first = sections.first().ok_or_else(invalid)?;
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
    let base = dimensioned_section_to_svg(&aggregate, label, dimensions, circles)?.replace(
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
    Ok(out)
}

fn dimension_svg(
    dimensions: &[SectionDimensionAnnotation],
    font: f64,
    bounds: &mut [f64; 4],
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
        let label = format!("{id}: {value} mm");
        let escaped_label = escape(&label)?;
        let text_width = label.chars().count() as f64 * font * 0.7;
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
        writeln!(out,"  <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" stroke=\"none\">{escaped_label}</text>\n</g>",number(text_center),number(-text_baseline),number(font),number(text_width)).unwrap();
    }
    Ok(out)
}

fn circle_dimension_svg(
    circles: &[SectionCircleDimensionAnnotation],
    font: f64,
    bounds: &mut [f64; 4],
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
        let direction = [label_offset[0] / distance, label_offset[1] / distance];
        let near = [
            center[0] + direction[0] * radius,
            center[1] + direction[1] * radius,
        ];
        let far = [
            center[0] - direction[0] * radius,
            center[1] - direction[1] * radius,
        ];
        let leader = [center[0] + label_offset[0], center[1] + label_offset[1]];
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
        let label = format!("{id}: {value} mm {kind_name}");
        let escaped_label = escape(&label)?;
        let escaped_id = escape(id)?
            .replace('\n', "&#10;")
            .replace('\r', "&#13;")
            .replace('\t', "&#9;");
        let width = font * 0.7 * label.chars().count() as f64;
        let right = direction[0] >= 0.;
        let text_x = leader[0] + if right { font } else { -font - width };
        let baseline = leader[1] + font;
        for p in [
            start,
            near,
            leader,
            [text_x, baseline - font * 0.4],
            [text_x + width, baseline + font * 1.2],
        ]
        .into_iter()
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
        for (a, b) in [(start, near), (near, leader)] {
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
        writeln!(out,"  <text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" stroke=\"none\">{escaped_label}</text>\n</g>",number(text_x),number(-baseline),number(font),number(width)).unwrap();
    }
    Ok(out)
}
