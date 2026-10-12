use cad_kernel::{extrude_annulus, extrude_rectangle, SectionPlane, Tolerance};
fn plane() -> SectionPlane {
    SectionPlane {
        origin: [0., 0., 5.],
        normal: [0., 0., 1.],
        x_direction: [1., 0., 0.],
    }
}
#[test]
fn native_section_svg_preserves_holes_scale_frame_and_escaped_labels() {
    let t = Tolerance::default();
    let model = extrude_annulus(20., 8., 10., t).unwrap().model;
    let before = format!("{model:?}");
    let section = model.section(plane(), t).unwrap();
    let svg = cad_export::section_to_svg(&section, "cover <script> & \"inspection\"").unwrap();
    assert_eq!(
        svg,
        cad_export::section_to_svg(&section, "cover <script> & \"inspection\"").unwrap()
    );
    assert_eq!(before, format!("{model:?}"));
    assert_eq!(section.loops.len(), 2);
    assert!(svg.contains("fill-rule=\"evenodd\""));
    assert!(svg.contains("A 20 20") && svg.contains("A 8 8"));
    assert!(!svg.contains("<script>"));
    assert!(svg.contains("&lt;script&gt; &amp; &quot;inspection&quot;"));
    assert!(svg.contains("mm\" height=") && svg.contains("origin (0, 0, 5) mm"));
    assert!(svg.contains("normal (0, 0, 1); X (1, 0, 0)"));
    assert!(cad_export::section_to_svg(&section, "bad\0label").is_err());
    assert!(cad_export::section_to_svg(&section, "bad\u{fffe}label").is_err());
    let mut invalid = section;
    invalid.area = f64::NAN;
    assert!(cad_export::section_to_svg(&invalid, "bad").is_err());
}
#[test]
fn planar_and_empty_sections_render_without_meshes() {
    let t = Tolerance::default();
    let model = extrude_rectangle(20., 10., 10., t).unwrap().model;
    let section = model.section(plane(), t).unwrap();
    let svg = cad_export::section_to_svg(&section, "plate").unwrap();
    assert!(svg.contains("L ") && !svg.contains("A "));
    let empty = model
        .section(
            SectionPlane {
                origin: [0., 0., 15.],
                ..plane()
            },
            t,
        )
        .unwrap();
    assert!(empty.loops.is_empty());
    let svg = cad_export::section_to_svg(&empty, "empty").unwrap();
    assert!(svg.contains("d=\"\""));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn occurrence_paths_do_not_cancel_overlapping_material() {
    let t = Tolerance::default();
    let section = extrude_annulus(20., 8., 10., t)
        .unwrap()
        .model
        .section(plane(), t)
        .unwrap();
    let svg = cad_export::sections_to_svg(&[section.clone(), section.clone()], "two occurrences")
        .unwrap();
    assert_eq!(svg.matches("<path ").count(), 2);
    assert_eq!(svg.matches("fill-rule=\"evenodd\"").count(), 2);
    assert!(
        svg.contains("data-occurrence-index=\"0\"") && svg.contains("data-occurrence-index=\"1\"")
    );
    assert!(svg.contains("overlapping occurrence areas are summed"));
    let mut other = section.clone();
    other.plane.origin[2] += 1.;
    assert!(cad_export::sections_to_svg(&[section, other], "mixed frames").is_err());
}

fn attribute<'a>(tag: &'a str, name: &str) -> &'a str {
    tag.split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

#[test]
fn projected_dimensions_are_bounded_escaped_and_keep_occurrences_separate() {
    use cad_export::SectionDimensionAnnotation as Dimension;
    let section = extrude_rectangle(20., 10., 10., Tolerance::default())
        .unwrap()
        .model
        .section(plane(), Tolerance::default())
        .unwrap();
    let dimensions = vec![
        Dimension {
            id: "spacing <&\"'>".into(),
            from: [20., 10.],
            to: [0., 0.],
            axis: 0,
            offset: -100.,
            value_mm: 20.,
        },
        Dimension {
            id: "height".into(),
            from: [0., 10.],
            to: [20., 0.],
            axis: 1,
            offset: 140.,
            value_mm: 10.,
        },
    ];
    let svg = cad_export::annotated_sections_to_svg(
        &[section.clone(), section.clone()],
        "pair",
        &dimensions,
    )
    .unwrap();
    assert_eq!(
        svg,
        cad_export::annotated_sections_to_svg(
            &[section.clone(), section.clone()],
            "pair",
            &dimensions
        )
        .unwrap()
    );
    assert_eq!(svg.matches("data-occurrence-index=").count(), 2);
    assert_eq!(svg.matches("class=\"section-dimension\"").count(), 2);
    assert_eq!(svg.matches("<line ").count(), 6);
    assert_eq!(svg.matches("<polygon ").count(), 4);
    assert!(svg.contains("data-id=\"spacing &lt;&amp;&quot;&apos;&gt;\""));
    assert!(svg.contains("height: 10 mm"));
    assert!(svg.contains("data-value-mm=\"20\""));
    // The canvas must contain every line endpoint, arrow vertex and full label,
    // even when annotations sit far outside the original geometry bounds.
    let view: Vec<f64> = attribute(&svg, "viewBox")
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let inside = |x: f64, y: f64| {
        assert!(x > view[0] && x < view[0] + view[2], "clipped x={x}");
        assert!(y > view[1] && y < view[1] + view[3], "clipped y={y}");
    };
    let mut lowest_annotation = f64::NEG_INFINITY;
    for line in svg.lines() {
        let line = line.trim();
        if line.starts_with("<line ") {
            for (x, y) in [("x1", "y1"), ("x2", "y2")] {
                let x = attribute(line, x).parse().unwrap();
                let y = attribute(line, y).parse().unwrap();
                inside(x, y);
                lowest_annotation = lowest_annotation.max(y);
            }
        } else if line.starts_with("<polygon ") {
            let coords: Vec<f64> = attribute(line, "points")
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            for p in coords.as_chunks::<2>().0 {
                inside(p[0], p[1]);
                lowest_annotation = lowest_annotation.max(p[1]);
            }
        } else if line.starts_with("<text ") && line.contains("textLength=") {
            let x: f64 = attribute(line, "x").parse().unwrap();
            let y: f64 = attribute(line, "y").parse().unwrap();
            let font: f64 = attribute(line, "font-size").parse().unwrap();
            let width: f64 = attribute(line, "textLength").parse().unwrap();
            inside(x - width / 2., y - font * 1.2);
            inside(x + width / 2., y + font * 0.4);
            lowest_annotation = lowest_annotation.max(y + font * 0.4);
        } else if line.starts_with("<text ") {
            let y: f64 = attribute(line, "y").parse().unwrap();
            let font: f64 = attribute(line, "font-size").parse().unwrap();
            assert!(y - font > lowest_annotation, "footer overlaps annotations");
        }
    }
    assert_eq!(
        cad_export::annotated_section_to_svg(&section, "plate", &[]).unwrap(),
        cad_export::section_to_svg(&section, "plate").unwrap()
    );
    assert_eq!(
        cad_export::annotated_sections_to_svg(std::slice::from_ref(&section), "plate", &[])
            .unwrap(),
        cad_export::sections_to_svg(&[section], "plate").unwrap()
    );
}

#[test]
fn dimension_validation_rejects_invalid_geometry_and_labels() {
    use cad_export::SectionDimensionAnnotation as Dimension;
    let section = extrude_rectangle(20., 10., 10., Tolerance::default())
        .unwrap()
        .model
        .section(plane(), Tolerance::default())
        .unwrap();
    let base = Dimension {
        id: "spacing".into(),
        from: [0., 0.],
        to: [20., 0.],
        axis: 0,
        offset: 15.,
        value_mm: 20.,
    };
    for invalid in [
        Dimension {
            axis: 2,
            ..base.clone()
        },
        Dimension {
            offset: f64::INFINITY,
            ..base.clone()
        },
        Dimension {
            from: [f64::NAN, 0.],
            ..base.clone()
        },
        Dimension {
            value_mm: f64::NAN,
            ..base.clone()
        },
        Dimension {
            value_mm: 21.,
            ..base.clone()
        },
        Dimension {
            value_mm: 0.,
            ..base.clone()
        },
        Dimension {
            value_mm: -20.,
            ..base.clone()
        },
        Dimension {
            to: [0., 20.],
            ..base.clone()
        },
        Dimension {
            id: "bad\0".into(),
            ..base.clone()
        },
        Dimension {
            id: "bad\u{ffff}".into(),
            ..base.clone()
        },
        Dimension {
            id: String::new(),
            ..base.clone()
        },
    ] {
        assert!(cad_export::annotated_section_to_svg(&section, "plate", &[invalid]).is_err());
    }
    let tiny = Dimension {
        to: [5e-8, 0.],
        value_mm: 5e-8,
        ..base.clone()
    };
    let svg = cad_export::annotated_section_to_svg(&section, "plate", &[tiny]).unwrap();
    assert!(svg.contains("spacing: 5.000000e-8 mm"));
    assert!(!svg.contains("spacing: 0 mm"));
    let smaller = Dimension {
        to: [5e-10, 0.],
        value_mm: 5e-10,
        ..base.clone()
    };
    let svg =
        cad_export::annotated_section_to_svg(&section, "plate", std::slice::from_ref(&smaller))
            .unwrap();
    assert!(svg.contains("spacing: 5.000000e-10 mm"));
    let translated = Dimension {
        from: [1e4, 0.],
        to: [1e4 + 0.1, 0.],
        value_mm: 0.1,
        ..base.clone()
    };
    assert!(cad_export::annotated_section_to_svg(&section, "plate", &[translated]).is_ok());
    let unresolved = Dimension {
        from: [1e14, 0.],
        to: [1e14 + 0.1, 0.],
        value_mm: 0.1,
        ..base.clone()
    };
    assert!(cad_export::annotated_section_to_svg(&section, "plate", &[unresolved]).is_err());
    let mismatch = Dimension {
        value_mm: 1e-10,
        ..smaller
    };
    assert!(cad_export::annotated_section_to_svg(&section, "plate", &[mismatch]).is_err());

    assert!(
        cad_export::annotated_section_to_svg(&section, "plate", &[base.clone(), base]).is_err()
    );
}
