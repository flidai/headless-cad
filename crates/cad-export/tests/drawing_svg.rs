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
                sections: vec![rectangle],
                dimensions: vec![],
                circles: vec![],
            },
            DrawingSheetView {
                id: "ring".into(),
                label: "Ring".into(),
                x: 110.,
                y: 10.,
                width: 90.,
                height: 80.,
                scale: 1.,
                sections: vec![circle],
                dimensions: vec![],
                circles: vec![],
            },
        ],
    }
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
