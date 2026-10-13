use cad_export::{
    drawing_sheet_to_svg, DrawingSheet, DrawingSheetView, SectionDimensionAnnotation,
};
use cad_kernel::{extrude_annulus, extrude_rectangle, SectionPlane, Tolerance};
fn sheet() -> DrawingSheet {
    let t = Tolerance::default();
    let plane = SectionPlane {
        origin: [0., 0., 5.],
        normal: [0., 0., 1.],
        x_direction: [1., 0., 0.],
    };
    let rectangle = extrude_rectangle(20., 10., 10., t)
        .unwrap()
        .model
        .section(plane, t)
        .unwrap();
    let circle = extrude_annulus(5., 2., 10., t)
        .unwrap()
        .model
        .section(plane, t)
        .unwrap();
    DrawingSheet {
        title: "Plate & <cover>".into(),
        revision: "A\"1".into(),
        source_context_hash: "source<&>".into(),
        width: 210.,
        height: 297.,
        views: vec![
            DrawingSheetView {
                id: "plate".into(),
                label: "Plate".into(),
                x: 10.,
                y: 10.,
                width: 90.,
                height: 80.,
                scale: 2.,
                paper_annotation_font_mm: None,
                annotation_labels: Default::default(),
                paper_caption: None,
                sections: vec![rectangle],
                dimensions: vec![],
                circles: vec![],
                slots: vec![],
            },
            DrawingSheetView {
                id: "ring".into(),
                label: "Ring".into(),
                x: 110.,
                y: 10.,
                width: 90.,
                height: 80.,
                scale: 1.,
                paper_annotation_font_mm: None,
                annotation_labels: Default::default(),
                paper_caption: None,
                sections: vec![circle],
                dimensions: vec![],
                circles: vec![],
                slots: vec![],
            },
        ],
        dimension_tables: vec![],
    }
}

#[test]
fn native_dimension_tables_render_rounded_values_and_reject_bad_layout() {
    use cad_export::{DrawingDimensionRow, DrawingDimensionTable};
    let mut s = sheet();
    s.dimension_tables.push(DrawingDimensionTable {
        id: "machine".into(),
        title: "Machining & setup".into(),
        show_size_limits: false,
        x: 10.,
        y: 100.,
        width: 190.,
        height: 48.,
        rows: vec![DrawingDimensionRow {
            dimension: "pilot_depth".into(),
            label: "Pilot <floor>".into(),
            value_mm: 5.00000001,
            lower_bound_mm: 5.0,
            upper_bound_mm: 5.00000002,
            authored_limits_mm: None,
        }],
    });
    let svg = drawing_sheet_to_svg(&s).unwrap();
    assert!(svg.contains("id=\"drawing-table-machine-pilot_depth\""));
    assert!(svg.contains("Machining &amp; setup"));
    assert!(svg.contains("Pilot &lt;floor&gt;"));
    assert!(svg.contains("≈ 5.000 mm"));
    assert!(svg.contains("Nominal native geometry"));
    let mut bad = s.clone();
    bad.dimension_tables[0].y = 40.;
    assert!(drawing_sheet_to_svg(&bad).is_err(), "table/view overlap");
    let mut bad = s.clone();
    bad.dimension_tables[0].height = 15.;
    assert!(drawing_sheet_to_svg(&bad).is_err(), "row overflow");
    let mut bad = s.clone();
    bad.dimension_tables[0].rows[0].label = "W".repeat(100);
    assert!(drawing_sheet_to_svg(&bad).is_err(), "label overflow");
    let mut bad = s.clone();
    bad.dimension_tables[0].rows[0].lower_bound_mm = 6.;
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "invalid numerical bound"
    );
    let mut bad = s.clone();
    bad.dimension_tables[0].rows[0].value_mm = f64::INFINITY;
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "invalid numerical value"
    );
    let mut bad = s.clone();
    bad.dimension_tables[0].rows[0].label = "bad\0".into();
    assert!(drawing_sheet_to_svg(&bad).is_err(), "invalid XML text");
    let mut bad = s.clone();
    bad.dimension_tables[0].id = "plate".into();
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "duplicate view/table ID"
    );
    let mut bad = s.clone();
    bad.dimension_tables[0].id = "a".into();
    bad.dimension_tables[0].width = 80.;
    bad.dimension_tables[0].rows[0].dimension = "b".into();
    let mut second = bad.dimension_tables[0].clone();
    second.id = "a-b".into();
    second.x = 110.;
    second.rows[0].dimension = "c".into();
    bad.dimension_tables.push(second);
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "emitted table DOM ID collision"
    );
    let mut bad = s.clone();
    bad.views[0].id = "drawing-table-a".into();
    bad.dimension_tables[0].id = "a".into();
    bad.dimension_tables[0].rows[0].dimension = "section-contours-0".into();
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "table row must not alias a generated section fragment"
    );
}

#[test]
fn styled_slot_tags_keep_native_id_and_refuse_overlapping_labels() {
    use cad_export::{SectionSlotDimensionAnnotation as Slot, SectionSlotDimensionKind as Kind};
    let mut drawing = sheet();
    let slot = Slot {
        tolerance: None,
        id: "source_slot".into(),
        kind: Kind::Width,
        center: [10., 5.],
        centerline_endpoints: [[7., 5.], [13., 5.]],
        axis: [1., 0.],
        radius_mm: 3.,
        arithmetic_uncertainty_mm: 0.,
        label_offset: [8., 0.],
        value_mm: 6.,
    };
    drawing.views[0].slots = vec![slot.clone()];
    drawing.views[0].paper_annotation_font_mm = Some(2.8);
    drawing.views[0]
        .annotation_labels
        .insert("source_slot".into(), "W1".into());
    let svg = drawing_sheet_to_svg(&drawing).unwrap();
    assert!(svg.contains("data-dimension-id=\"source_slot\" data-kind=\"width\""));
    assert!(svg.contains("W1:W6mm"));
    let mut second = slot;
    second.id = "same_place".into();
    drawing.views[0].slots.push(second);
    drawing.views[0]
        .annotation_labels
        .insert("same_place".into(), "W2".into());
    assert!(drawing_sheet_to_svg(&drawing).is_err());
}

#[test]
fn paper_slot_extensions_refuse_crossing_another_annotation_label() {
    use cad_export::{SectionSlotDimensionAnnotation as Slot, SectionSlotDimensionKind as Kind};
    let mut drawing = sheet();
    drawing.views[0].paper_annotation_font_mm = Some(2.8);
    let first = Slot {
        tolerance: None,
        id: "first".into(),
        kind: Kind::OverallLength,
        center: [10., 5.],
        centerline_endpoints: [[7., 5.], [13., 5.]],
        axis: [1., 0.],
        radius_mm: 3.,
        arithmetic_uncertainty_mm: 0.,
        label_offset: [0., 12.],
        value_mm: 12.,
    };
    drawing.views[0].slots.push(first.clone());
    drawing.views[0]
        .annotation_labels
        .insert("first".into(), "A12345678901".into());
    assert!(
        drawing_sheet_to_svg(&drawing).is_ok(),
        "first label clears its own lines"
    );
    let mut second = first;
    second.id = "second".into();
    second.label_offset = [0., 24.];
    drawing.views[0].slots.push(second);
    drawing.views[0]
        .annotation_labels
        .insert("second".into(), "B12345678901".into());
    let error = drawing_sheet_to_svg(&drawing).unwrap_err().to_string();
    assert!(
        error.contains("slot line crosses an annotation label"),
        "{error}"
    );
}

#[test]
fn authored_size_bands_are_separate_exact_paper_cells() {
    let mut s = sheet();
    s.width = 420.;
    s.dimension_tables.push(cad_export::DrawingDimensionTable {
        id: "machine".into(),
        title: "Machining size bands".into(),
        show_size_limits: true,
        x: 10.,
        y: 100.,
        width: 195.,
        height: 48.,
        rows: vec![
            cad_export::DrawingDimensionRow {
                dimension: "pilot_depth".into(),
                label: "Pilot depth".into(),
                value_mm: 5.00000001,
                lower_bound_mm: 5.0,
                upper_bound_mm: 5.00000002,
                authored_limits_mm: Some([4.9, 5.1]),
            },
            cad_export::DrawingDimensionRow {
                dimension: "floor_stock".into(),
                label: "Floor stock".into(),
                value_mm: 11.,
                lower_bound_mm: 10.999999999,
                upper_bound_mm: 11.000000001,
                authored_limits_mm: None,
            },
        ],
    });
    let svg = drawing_sheet_to_svg(&s).unwrap();
    assert!(svg.contains("DESIGN LIMITS"));
    assert!(svg.contains("≈ 5.000 mm"));
    assert!(svg.contains("4.9..5.1 mm"));
    assert!(svg.contains(">not specified</text>"));
    assert!(
        !svg.contains("5.00000002.."),
        "native numerical bound is not a size limit"
    );
    let mut bad = s.clone();
    bad.dimension_tables[0].width = 190.;
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "three paper columns need width"
    );
    let mut legacy = s.clone();
    legacy.dimension_tables[0].show_size_limits = false;
    legacy.dimension_tables[0].width = 190.;
    let nominal_only = drawing_sheet_to_svg(&legacy).unwrap();
    assert!(!nominal_only.contains("DESIGN LIMITS"));
    assert!(!nominal_only.contains("4.9..5.1 mm"));
    let mut bad = s.clone();
    bad.dimension_tables[0].rows[0].authored_limits_mm = Some([5., 5.]);
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "size limits need a real band"
    );
    let mut bad = s.clone();
    bad.dimension_tables[0].rows[0].authored_limits_mm = Some([1e-300, 1e300]);
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "exact limit string must fit"
    );
    let mut short = s.clone();
    short.dimension_tables[0].rows.truncate(1);
    short.dimension_tables[0].height = 22.;
    assert!(
        drawing_sheet_to_svg(&short).is_err(),
        "header, row and footer need paper separation"
    );
    short.dimension_tables[0].height = 23.;
    assert!(
        drawing_sheet_to_svg(&short).is_ok(),
        "minimum separated one-row band table"
    );
}

#[test]
fn paper_view_annotations_use_declared_print_size_and_short_native_tags() {
    let mut s = sheet();
    let view = &mut s.views[0];
    view.scale = 0.8;
    view.paper_annotation_font_mm = Some(2.8);
    view.paper_caption = Some("Plate section".into());
    view.annotation_labels
        .insert("plate_width".into(), "P1".into());
    view.dimensions.push(SectionDimensionAnnotation {
        tolerance: None,
        id: "plate_width".into(),
        from: [0., 0.],
        to: [20., 0.],
        axis: 0,
        offset: -5.,
        value_mm: 20.,
    });
    let svg = drawing_sheet_to_svg(&s).unwrap();
    assert!(svg.contains("id=\"drawing-caption-plate\""));
    assert!(svg.contains("Plate section"));
    assert!(svg.contains("data-paper-annotation-font-mm=\"2.8\""));
    assert!(svg.contains("data-id=\"plate_width\""));
    assert!(svg.contains("P1: 20 mm"));
    let annotation = svg.split("data-id=\"plate_width\"").nth(1).unwrap();
    let native_font: f64 = annotation
        .split("font-size=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((native_font * 0.8 - 2.8).abs() < 1e-12);
    let styled_view = svg
        .split("<svg id=\"drawing-view-plate\"")
        .nth(1)
        .unwrap()
        .split("</svg>")
        .next()
        .unwrap();
    assert!(
        !styled_view.contains(">Origin:"),
        "styled paper view omits tiny metadata rows"
    );
    let mut bad = s.clone();
    bad.views[0].annotation_labels.clear();
    assert!(drawing_sheet_to_svg(&bad).is_err(), "missing native tag");
    let mut bad = s.clone();
    bad.views[0].paper_annotation_font_mm = Some(2.);
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "font below print floor"
    );
    let mut bad = s.clone();
    bad.views[0].paper_caption = Some("Caption exceeds available paper width".repeat(8));
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "caption cannot shrink to fit"
    );
    let mut bad = s.clone();
    let mut duplicate_placement = bad.views[0].dimensions[0].clone();
    duplicate_placement.id = "second_width".into();
    bad.views[0].dimensions.push(duplicate_placement);
    bad.views[0]
        .annotation_labels
        .insert("second_width".into(), "P2".into());
    assert!(
        drawing_sheet_to_svg(&bad).is_err(),
        "paper labels cannot overlap"
    );
}
#[test]
fn true_scale_native_paths_and_escaped_title_block() {
    let s = sheet();
    let svg = drawing_sheet_to_svg(&s).unwrap();
    assert_eq!(svg, drawing_sheet_to_svg(&s).unwrap());
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"210mm\" height=\"297mm\" viewBox=\"0 0 210 297\">"));
    // 20x10 section: margin1.2 each side and three text rows totaling3mm.
    // Declared2:1 scale gives44.8x30.8 paper mm, centered in90x80 allocation.
    assert!(
        svg.contains(
            "id=\"drawing-view-plate\" x=\"32.6\" y=\"34.6\" width=\"44.8\" height=\"30.8\""
        ),
        "{svg}"
    );
    assert!(svg.contains("data-scale=\"2\""));
    assert!(svg.contains("A 5 5") && svg.contains("A 2 2"));
    assert!(svg.contains("Plate &amp; &lt;cover&gt;"));
    assert!(svg.contains("Revision: A&quot;1"));
    assert!(svg.contains("Source context: source&lt;&amp;&gt;"));
    assert!(svg.contains("id=\"plate-section-contours-0\""));
    assert!(!svg.contains("clipPath"));
}
#[test]
fn rejects_overflow_annotations_overlap_and_invalid_scale() {
    let mut s = sheet();
    s.views[0].scale = 10.;
    assert!(drawing_sheet_to_svg(&s).is_err());
    let mut s = sheet();
    s.views[0].dimensions.push(SectionDimensionAnnotation {
        tolerance: None,
        id: "far".into(),
        from: [0., 0.],
        to: [20., 0.],
        axis: 0,
        offset: 100.,
        value_mm: 20.,
    });
    assert!(drawing_sheet_to_svg(&s).is_err());
    let mut s = sheet();
    s.views[1].x = 50.;
    assert!(drawing_sheet_to_svg(&s).is_err());
    let mut s = sheet();
    s.views[0].scale = f64::NAN;
    assert!(drawing_sheet_to_svg(&s).is_err());
    let mut s = sheet();
    s.revision = "bad\0".into();
    assert!(drawing_sheet_to_svg(&s).is_err());
}

#[test]
fn signed_manufacturing_labels_preserve_native_paths_and_fit_bounds() {
    use cad_export::{
        DimensionToleranceAnnotation as Limits, SectionCircleDimensionAnnotation,
        SectionCircleDimensionKind,
    };
    let mut s = sheet();
    s.views[0].dimensions.push(SectionDimensionAnnotation {
        id: "width".into(),
        from: [0., 0.],
        to: [20., 0.],
        axis: 0,
        offset: -5.,
        value_mm: 20.,
        tolerance: None,
    });
    s.views[1].circles.push(SectionCircleDimensionAnnotation {
        id: "bore".into(),
        center: [0., 0.],
        radius: 2.,
        kind: SectionCircleDimensionKind::Diameter,
        label_offset: [8., 8.],
        value_mm: 4.,
        tolerance: None,
        arc: None,
    });
    let baseline = drawing_sheet_to_svg(&s).unwrap();
    s.views[0].dimensions[0].tolerance = Some(Limits {
        nominal_mm: 20.,
        lower_deviation_mm: -0.01,
        upper_deviation_mm: 0.02,
    });
    s.views[1].circles[0].tolerance = Some(Limits {
        nominal_mm: 4.,
        lower_deviation_mm: 0.01,
        upper_deviation_mm: 0.03,
    });
    let with = drawing_sheet_to_svg(&s).unwrap();
    assert!(with.contains("width: 20 +0.02/-0.01 mm"));
    assert!(with.contains("bore: 4 +0.03/+0.01 mm diameter"));
    assert!(with.contains("independent of modeling tolerance"));
    let paths = |svg: &str| {
        svg.lines()
            .filter(|s| s.starts_with("<path id="))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&baseline), paths(&with));
    s.views[0].dimensions[0].tolerance = Some(Limits {
        nominal_mm: 20.,
        lower_deviation_mm: -1e-100,
        upper_deviation_mm: 0.123456789012345,
    });
    s.views[0].width = 50.;
    assert!(
        drawing_sheet_to_svg(&s).is_err(),
        "expanded tolerance text must participate in scale-fit"
    );
    for (lower, upper) in [
        (0.01, 0.01),
        (0.02, 0.01),
        (-21., 0.01),
        (0., f64::NAN),
        (0., f64::EPSILON),
    ] {
        let mut s = sheet();
        s.views[0].dimensions.push(SectionDimensionAnnotation {
            id: "bad".into(),
            from: [0., 0.],
            to: [20., 0.],
            axis: 0,
            offset: -5.,
            value_mm: 20.,
            tolerance: Some(Limits {
                nominal_mm: 20.,
                lower_deviation_mm: lower,
                upper_deviation_mm: upper,
            }),
        });
        assert!(drawing_sheet_to_svg(&s).is_err());
    }
}
