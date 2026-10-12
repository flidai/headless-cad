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
    let margin = span * 0.06;
    let font = span * 0.025;
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
    let base = section_to_svg(&aggregate, label)?.replace(
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
