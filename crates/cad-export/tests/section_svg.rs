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
