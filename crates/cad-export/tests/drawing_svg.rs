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
