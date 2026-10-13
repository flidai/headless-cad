use cad_kernel::{
    extrude_annulus, extrude_rectangle, FaceId, SectionCurve, SectionPlane, Tolerance,
};
fn plane() -> SectionPlane {
    SectionPlane {
        origin: [0., 0., 5.],
        normal: [0., 0., 1.],
        x_direction: [1., 0., 0.],
    }
}

#[test]
fn slot_dimensions_render_native_extents_and_reject_inconsistent_capsules() {
    use cad_export::{SectionSlotDimensionAnnotation as Slot, SectionSlotDimensionKind as Kind};
    let section = extrude_rectangle(40., 20., 10., Tolerance::default())
        .unwrap()
        .model
        .section(plane(), Tolerance::default())
        .unwrap();
    let slot = Slot {
        tolerance: None,
        id: "slot-a".into(),
        kind: Kind::OverallLength,
        center: [20., 10.],
        centerline_endpoints: [[17., 10.], [23., 10.]],
        axis: [1., 0.],
        radius_mm: 3.,
        arithmetic_uncertainty_mm: 0.,
        label_offset: [0., 12.],
        value_mm: 12.,
    };
    let svg = cad_export::slot_dimensioned_sections_to_svg(
        std::slice::from_ref(&section),
        "native",
        &[],
        &[],
        std::slice::from_ref(&slot),
    )
    .unwrap();
    assert!(svg.contains("class=\"slot-dimension\" data-dimension-id=\"slot-a\" data-kind=\"overall_length\" data-value-mm=\"12\""));
    assert!(
        svg.contains("data-native-from-x=\"14\" data-native-from-y=\"10\" data-native-to-x=\"26\"")
    );
    assert!(svg.contains("x1=\"14\" y1=\"-10\" x2=\"14\" y2=\"-22\""));
    assert_eq!(svg.matches("class=\"slot-center-mark\"").count(), 3);
    let mut width = slot.clone();
    width.id = "slot-width".into();
    width.kind = Kind::Width;
    width.label_offset = [8., 0.];
    width.value_mm = 6.;
    let width_svg = cad_export::slot_dimensioned_sections_to_svg(
        std::slice::from_ref(&section),
        "width",
        &[],
        &[],
        std::slice::from_ref(&width),
    )
    .unwrap();
    let group = width_svg.split("class=\"slot-dimension\"").nth(1).unwrap();
    let lines: Vec<_> = group
        .lines()
        .filter(|line| line.contains("<line "))
        .collect();
    assert_eq!(lines.len(), 3);
    let dimension_line = lines[2];
    let top = attribute(dimension_line, "y1")
        .parse::<f64>()
        .unwrap()
        .min(attribute(dimension_line, "y2").parse::<f64>().unwrap());
    let text = group.lines().find(|line| line.contains("<text ")).unwrap();
    let baseline = attribute(text, "y").parse::<f64>().unwrap();
    let font = attribute(text, "font-size").parse::<f64>().unwrap();
    // SVG Y runs downward; the conservative label box extends 1.2 font
    // below its baseline in native coordinates, or upward in SVG coordinates.
    assert!(
        baseline + font * 0.4 < top,
        "width label must clear its dimension line"
    );
    let label_clears_lines = |svg: &str| {
        let group = svg.split("class=\"slot-dimension\"").nth(1).unwrap();
        let text = group.lines().find(|line| line.contains("<text ")).unwrap();
        let x = attribute(text, "x").parse::<f64>().unwrap();
        let y = attribute(text, "y").parse::<f64>().unwrap();
        let font = attribute(text, "font-size").parse::<f64>().unwrap();
        let width = attribute(text, "textLength").parse::<f64>().unwrap();
        let bounds = [
            x - width * 0.5,
            y - font * 1.2,
            x + width * 0.5,
            y + font * 0.4,
        ];
        let lines: Vec<_> = group
            .lines()
            .filter(|line| line.contains("<line "))
            .collect();
        assert_eq!(lines.len(), 3);
        for line in lines {
            let x1 = attribute(line, "x1").parse::<f64>().unwrap();
            let y1 = attribute(line, "y1").parse::<f64>().unwrap();
            let x2 = attribute(line, "x2").parse::<f64>().unwrap();
            let y2 = attribute(line, "y2").parse::<f64>().unwrap();
            assert!(
                x1.max(x2) < bounds[0]
                    || x1.min(x2) > bounds[2]
                    || y1.max(y2) < bounds[1]
                    || y1.min(y2) > bounds[3],
                "slot label box intersects an extension or dimension line: {line}"
            );
        }
    };
    label_clears_lines(&svg);
    label_clears_lines(&width_svg);
    let mut lower = slot.clone();
    lower.label_offset = [0., -12.];
    let lower_svg = cad_export::slot_dimensioned_sections_to_svg(
        std::slice::from_ref(&section),
        "lower",
        &[],
        &[],
        std::slice::from_ref(&lower),
    )
    .unwrap();
    label_clears_lines(&lower_svg);
    let mut bad = slot.clone();
    bad.value_mm = 11.;
    assert!(cad_export::slot_dimensioned_sections_to_svg(
        std::slice::from_ref(&section),
        "bad",
        &[],
        &[],
        &[bad]
    )
    .is_err());
    let mut bad = slot.clone();
    bad.axis = [0., 1.];
    assert!(cad_export::slot_dimensioned_sections_to_svg(
        std::slice::from_ref(&section),
        "bad",
        &[],
        &[],
        &[bad]
    )
    .is_err());
    let mut bad = slot;
    bad.centerline_endpoints[1] = [17., 10.];
    assert!(
        cad_export::slot_dimensioned_sections_to_svg(&[section], "bad", &[], &[], &[bad]).is_err()
    );
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
            tolerance: None,
            id: "spacing <&\"'>".into(),
            from: [20., 10.],
            to: [0., 0.],
            axis: 0,
            offset: -100.,
            value_mm: 20.,
        },
        Dimension {
            tolerance: None,
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
        tolerance: None,
        id: "spacing".into(),
        from: [0., 0.],
        to: [20., 0.],
        axis: 0,
        offset: 15.,
        value_mm: 20.,
    };
    for invalid in [
        Dimension {
            tolerance: None,
            axis: 2,
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            offset: f64::INFINITY,
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            from: [f64::NAN, 0.],
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            value_mm: f64::NAN,
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            value_mm: 21.,
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            value_mm: 0.,
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            value_mm: -20.,
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            to: [0., 20.],
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            id: "bad\0".into(),
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            id: "bad\u{ffff}".into(),
            ..base.clone()
        },
        Dimension {
            tolerance: None,
            id: String::new(),
            ..base.clone()
        },
    ] {
        assert!(cad_export::annotated_section_to_svg(&section, "plate", &[invalid]).is_err());
    }
    let tiny = Dimension {
        tolerance: None,
        to: [5e-8, 0.],
        value_mm: 5e-8,
        ..base.clone()
    };
    let svg = cad_export::annotated_section_to_svg(&section, "plate", &[tiny]).unwrap();
    assert!(svg.contains("spacing: 5.000000e-8 mm"));
    assert!(!svg.contains("spacing: 0 mm"));
    let smaller = Dimension {
        tolerance: None,
        to: [5e-10, 0.],
        value_mm: 5e-10,
        ..base.clone()
    };
    let svg =
        cad_export::annotated_section_to_svg(&section, "plate", std::slice::from_ref(&smaller))
            .unwrap();
    assert!(svg.contains("spacing: 5.000000e-10 mm"));
    let translated = Dimension {
        tolerance: None,
        from: [1e4, 0.],
        to: [1e4 + 0.1, 0.],
        value_mm: 0.1,
        ..base.clone()
    };
    assert!(cad_export::annotated_section_to_svg(&section, "plate", &[translated]).is_ok());
    let unresolved = Dimension {
        tolerance: None,
        from: [1e14, 0.],
        to: [1e14 + 0.1, 0.],
        value_mm: 0.1,
        ..base.clone()
    };
    assert!(cad_export::annotated_section_to_svg(&section, "plate", &[unresolved]).is_err());
    let mismatch = Dimension {
        tolerance: None,
        value_mm: 1e-10,
        ..smaller
    };
    assert!(cad_export::annotated_section_to_svg(&section, "plate", &[mismatch]).is_err());

    assert!(
        cad_export::annotated_section_to_svg(&section, "plate", &[base.clone(), base]).is_err()
    );
}

#[test]
fn circular_dimensions_preserve_native_paths_and_bound_leaders_and_labels() {
    use cad_export::{
        SectionCircleDimensionAnnotation as Circle, SectionCircleDimensionKind as Kind,
    };
    let section = extrude_annulus(20., 8., 10., Tolerance::default())
        .unwrap()
        .model
        .section(plane(), Tolerance::default())
        .unwrap();
    let circles = [
        Circle {
            tolerance: None,
            id: "bore<&\"".into(),
            center: [0., 0.],
            radius: 8.,
            kind: Kind::Diameter,
            label_offset: [-100., -60.],
            value_mm: 16.,
            arc: None,
        },
        Circle {
            tolerance: None,
            id: "outer".into(),
            center: [0., 0.],
            radius: 20.,
            kind: Kind::Radius,
            label_offset: [80., 110.],
            value_mm: 20.,
            arc: None,
        },
    ];
    let svg = cad_export::dimensioned_sections_to_svg(
        &[section.clone(), section.clone()],
        "rings",
        &[],
        &circles,
    )
    .unwrap();
    assert_eq!(
        svg,
        cad_export::dimensioned_sections_to_svg(
            &[section.clone(), section],
            "rings",
            &[],
            &circles
        )
        .unwrap()
    );
    assert_eq!(svg.matches("data-occurrence-index=").count(), 2);
    assert_eq!(svg.matches("class=\"section-circle-dimension\"").count(), 2);
    assert_eq!(svg.matches("<polygon ").count(), 3);
    assert!(svg.contains("bore&lt;&amp;&quot;: 16 mm diameter"));
    assert!(svg.contains("outer: 20 mm radius"));
    assert!(svg.contains("data-kind=\"diameter\"") && svg.contains("data-kind=\"radius\""));
    let view: Vec<f64> = attribute(&svg, "viewBox")
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    let inside = |x: f64, y: f64| {
        assert!(x > view[0] && x < view[0] + view[2]);
        assert!(y > view[1] && y < view[1] + view[3]);
    };
    let mut lowest = f64::NEG_INFINITY;
    for line in svg.lines().map(str::trim) {
        if line.starts_with("<line ") {
            for (x, y) in [("x1", "y1"), ("x2", "y2")] {
                let x = attribute(line, x).parse().unwrap();
                let y = attribute(line, y).parse().unwrap();
                inside(x, y);
                lowest = lowest.max(y);
            }
        } else if line.starts_with("<polygon ") {
            let points: Vec<f64> = attribute(line, "points")
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            for p in points.as_chunks::<2>().0 {
                inside(p[0], p[1]);
                lowest = lowest.max(p[1]);
            }
        } else if line.starts_with("<text ") && line.contains("textLength=") {
            let x: f64 = attribute(line, "x").parse().unwrap();
            let y: f64 = attribute(line, "y").parse().unwrap();
            let width: f64 = attribute(line, "textLength").parse().unwrap();
            let font: f64 = attribute(line, "font-size").parse().unwrap();
            inside(x, y - font * 1.2);
            inside(x + width, y + font * 0.4);
            lowest = lowest.max(y + font * 0.4);
        } else if line.starts_with("<text ") {
            let y: f64 = attribute(line, "y").parse().unwrap();
            let font: f64 = attribute(line, "font-size").parse().unwrap();
            assert!(y - font > lowest);
        }
    }
}

#[test]
fn circular_dimension_validation_covers_precision_and_shared_ids() {
    use cad_export::{
        SectionCircleDimensionAnnotation as Circle, SectionCircleDimensionKind as Kind,
        SectionDimensionAnnotation as Linear,
    };
    let section = extrude_annulus(20., 8., 10., Tolerance::default())
        .unwrap()
        .model
        .section(plane(), Tolerance::default())
        .unwrap();
    let base = Circle {
        tolerance: None,
        id: "bore".into(),
        center: [0., 0.],
        radius: 8.,
        kind: Kind::Radius,
        label_offset: [20., 0.],
        value_mm: 8.,
        arc: None,
    };
    for invalid in [
        Circle {
            tolerance: None,
            radius: 0.,
            ..base.clone()
        },
        Circle {
            tolerance: None,
            radius: f64::NAN,
            ..base.clone()
        },
        Circle {
            tolerance: None,
            center: [f64::INFINITY, 0.],
            ..base.clone()
        },
        Circle {
            tolerance: None,
            value_mm: 9.,
            ..base.clone()
        },
        Circle {
            tolerance: None,
            kind: Kind::Diameter,
            ..base.clone()
        },
        Circle {
            tolerance: None,
            label_offset: [0., 0.],
            ..base.clone()
        },
        Circle {
            tolerance: None,
            label_offset: [8., 0.],
            ..base.clone()
        },
        Circle {
            tolerance: None,
            label_offset: [1., 1.],
            ..base.clone()
        },
        Circle {
            tolerance: None,
            label_offset: [f64::NAN, 0.],
            ..base.clone()
        },
        Circle {
            tolerance: None,
            id: "bad\0".into(),
            ..base.clone()
        },
        Circle {
            tolerance: None,
            id: "bad\u{ffff}".into(),
            ..base.clone()
        },
        Circle {
            tolerance: None,
            center: [1e16, 0.],
            ..base.clone()
        },
    ] {
        assert!(cad_export::dimensioned_section_to_svg(&section, "test", &[], &[invalid]).is_err());
    }
    let tiny = Circle {
        tolerance: None,
        radius: 5e-10,
        value_mm: 5e-10,
        label_offset: [1e-9, 0.],
        ..base.clone()
    };
    let svg =
        cad_export::dimensioned_section_to_svg(&section, "test", &[], std::slice::from_ref(&tiny))
            .unwrap();
    assert!(svg.contains("5.000000e-10 mm radius"));
    assert!(cad_export::dimensioned_section_to_svg(
        &section,
        "test",
        &[],
        &[Circle {
            tolerance: None,
            value_mm: 1e-10,
            ..tiny
        }]
    )
    .is_err());
    let translated = Circle {
        tolerance: None,
        center: [1e4, 1e4],
        radius: 0.1,
        value_mm: 0.1,
        label_offset: [0.2, 0.3],
        ..base.clone()
    };
    assert!(cad_export::dimensioned_section_to_svg(&section, "test", &[], &[translated]).is_ok());
    let linear = Linear {
        tolerance: None,
        id: "bore".into(),
        from: [0., 0.],
        to: [8., 0.],
        axis: 0,
        offset: 20.,
        value_mm: 8.,
    };
    assert!(cad_export::dimensioned_section_to_svg(
        &section,
        "test",
        &[linear],
        std::slice::from_ref(&base)
    )
    .is_err());
    assert!(
        cad_export::dimensioned_section_to_svg(&section, "test", &[], &[base.clone(), base])
            .is_err()
    );
}

#[test]
fn partial_radius_arrow_uses_exact_arc_and_rejects_tampered_evidence() {
    use cad_export::{
        SectionArcWitnessAnnotation as ArcWitness, SectionCircleDimensionAnnotation as Circle,
        SectionCircleDimensionKind as Kind,
    };
    let t = Tolerance::default();
    let stock = extrude_rectangle(20., 12., 10., t).unwrap().model;
    let rounded = cad_kernel::fillet_longitudinal(&stock, [FaceId(2), FaceId(3)], 2., t)
        .unwrap()
        .model;
    let section = rounded.section(plane(), t).unwrap();
    let (loop_index, segment_index, segment) = section
        .loops
        .iter()
        .enumerate()
        .flat_map(|(i, l)| l.segments.iter().enumerate().map(move |(j, s)| (i, j, s)))
        .find(|(_, _, s)| matches!(s.curve, SectionCurve::Arc { .. }))
        .unwrap();
    let SectionCurve::Arc {
        center,
        radius,
        start_angle,
        sweep_angle,
    } = segment.curve
    else {
        unreachable!()
    };
    let tip = [
        center[0] + radius * (start_angle + sweep_angle / 2.).cos(),
        center[1] + radius * (start_angle + sweep_angle / 2.).sin(),
    ];
    let dimension = Circle {
        tolerance: None,
        id: "corner_radius".into(),
        center,
        radius,
        kind: Kind::Radius,
        label_offset: [-10., -9.],
        value_mm: radius,
        arc: Some(ArcWitness {
            occurrence_index: 0,
            target_face: segment.face,
            loop_index,
            segment_index,
            start_angle,
            sweep_angle,
            tip,
            arithmetic_uncertainty_mm: 0.,
        }),
    };
    let svg = cad_export::dimensioned_section_to_svg(
        &section,
        "rounded",
        &[],
        std::slice::from_ref(&dimension),
    )
    .unwrap();
    assert!(svg.contains(&format!("x2=\"{}\" y2=\"{}\"", tip[0], -tip[1])));
    assert_eq!(svg.matches("class=\"section-circle-dimension\"").count(), 1);
    assert!(svg.contains("corner_radius: 2 mm radius"));
    let mut bad = dimension.clone();
    bad.arc.as_mut().unwrap().tip[0] += 0.1;
    assert!(cad_export::dimensioned_section_to_svg(&section, "rounded", &[], &[bad]).is_err());
    let mut bad = dimension.clone();
    bad.arc.as_mut().unwrap().start_angle += 0.1;
    assert!(cad_export::dimensioned_section_to_svg(&section, "rounded", &[], &[bad]).is_err());
    let mut bad = dimension.clone();
    bad.arc.as_mut().unwrap().target_face = FaceId(0);
    assert!(cad_export::dimensioned_section_to_svg(&section, "rounded", &[], &[bad]).is_err());
    let mut bad = dimension.clone();
    bad.arc.as_mut().unwrap().occurrence_index = 1;
    assert!(cad_export::dimensioned_section_to_svg(&section, "rounded", &[], &[bad]).is_err());
    let mut bad = dimension.clone();
    bad.kind = Kind::Diameter;
    bad.value_mm *= 2.;
    assert!(cad_export::dimensioned_section_to_svg(&section, "rounded", &[], &[bad]).is_err());
    let plain = stock.section(plane(), t).unwrap();
    let mut second = dimension.clone();
    second.arc.as_mut().unwrap().occurrence_index = 1;
    assert!(cad_export::dimensioned_sections_to_svg(
        &[plain.clone(), section.clone()],
        "two",
        &[],
        &[dimension]
    )
    .is_err());
    let svg =
        cad_export::dimensioned_sections_to_svg(&[plain, section], "two", &[], &[second]).unwrap();
    assert!(svg.contains("corner_radius: 2 mm radius"));
}
