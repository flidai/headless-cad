use super::*;
use cad_assembly::{BuiltAssembly, BuiltPart, Instance, RigidTransform};
use std::{collections::BTreeMap, sync::Arc};

fn check_mesh(output: &str, expected_volume: f64, expected_triangles: usize) {
    let points: Vec<[f64; 3]> = output
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("vertex"))
                .then(|| std::array::from_fn(|_| words.next().unwrap().parse().unwrap()))
        })
        .collect();
    let normals: Vec<[f64; 3]> = output
        .lines()
        .filter_map(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            (words.first() == Some(&"facet"))
                .then(|| std::array::from_fn(|i| words[i + 2].parse().unwrap()))
        })
        .collect();
    assert_eq!(normals.len(), expected_triangles);
    assert_eq!(points.len(), expected_triangles * 3);
    let mut incidence = BTreeMap::new();
    let mut volume = 0.;
    for (triangle, normal) in points.as_chunks::<3>().0.iter().zip(normals) {
        let actual = normalized(cross(
            sub(triangle[1], triangle[0]),
            sub(triangle[2], triangle[0]),
        ))
        .unwrap();
        assert!((dot(actual, normal) - 1.).abs() < 1e-12);
        volume += dot(triangle[0], cross(triangle[1], triangle[2])) / 6.;
        for i in 0..3 {
            let key = |p: Point| p.map(|x| if x == 0. { 0 } else { x.to_bits() });
            let a = key(triangle[i]);
            let b = key(triangle[(i + 1) % 3]);
            let (edge, direction) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let entry = incidence.entry(edge).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += direction;
        }
    }
    assert!((volume - expected_volume).abs() < 1e-10);
    assert!(incidence
        .values()
        .all(|&(count, direction)| count == 2 && direction == 0));
}

#[test]
fn convex_prism_step_caps_remain_polygons_and_stl_is_closed() {
    let cases: &[(&[[f64; 2]], f64, f64)] = &[
        (&[[0., 0.], [8., 0.], [0., 6.]], 5., 120.),
        (&[[0., 0.], [10., 0.], [8., 4.], [2., 4.]], 3., 96.),
        (&[[0., 0.], [6., 0.], [6., 4.], [3., 7.], [0., 4.]], 2., 66.),
    ];
    for &(points, height, volume) in cases {
        let model = cad_kernel::extrude_polygon(points, height, Tolerance::default())
            .unwrap()
            .model;
        let step = to_step(&model, Tolerance::default()).unwrap();
        assert_eq!(step, to_step(&model, Tolerance::default()).unwrap());
        assert_eq!(step.matches("FACE_SURFACE(").count(), points.len() + 2);
        assert_eq!(step.matches("FACETED_BREP('").count(), 1);
        let caps = step
            .lines()
            .filter(|line| {
                line.contains("POLY_LOOP(") && line.matches('#').count() == points.len() + 1
            })
            .count();
        assert!(
            caps >= 2,
            "STEP must preserve both untriangulated cap polygons"
        );
        let stl = to_stl(&model, Tolerance::default()).unwrap();
        check_mesh(&stl, volume, 4 * points.len() - 4);
    }
}

#[test]
fn rotated_triangular_prism_assembly_has_correct_world_mesh() {
    let result = Arc::new(
        cad_kernel::extrude_polygon(&[[0., 0.], [8., 0.], [0., 6.]], 5., Tolerance::default())
            .unwrap(),
    );
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let assembly = BuiltAssembly::new(
        "prism".into(),
        BTreeMap::from([(
            "wedge".into(),
            BuiltPart {
                datums: Default::default(),
                feature_id: "extrude".into(),
                content_hash: "triangle".into(),
                metadata: Default::default(),
                result,
            },
        )]),
        vec![Instance {
            attachment: None,
            id: "turned".into(),
            part: "wedge".into(),
            transform: RigidTransform {
                translation: [20., 10., 4.],
                rotation: [q, 0., 0., q],
            },
        }],
        Tolerance::default(),
    )
    .unwrap();
    check_mesh(
        &assembly_to_stl(&assembly, Tolerance::default()).unwrap(),
        120.,
        8,
    );
    let step = assembly_to_step(&assembly, Tolerance::default()).unwrap();
    assert_eq!(step.matches("FACE_SURFACE(").count(), 5);
    assert_eq!(step.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 1);
}

#[test]
fn rigid_solid_transform_exports_exact_planes_and_closed_oriented_mesh() {
    let tolerance = Tolerance::default();
    let original =
        cad_kernel::extrude_polygon(&[[0., 0.], [8., 0.], [0., 6.]], 5., tolerance).unwrap();
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let first =
        cad_kernel::transform(&original.model, [10., 0., 0.], [q, 0., 0., q], tolerance).unwrap();
    let result =
        cad_kernel::transform(&first.model, [0., 20., 0.], [0., q, 0., q], tolerance).unwrap();
    let stl = to_stl(&result.model, tolerance).unwrap();
    check_mesh(&stl, 120., 8);
    for line in stl
        .lines()
        .filter(|line| line.trim_start().starts_with("vertex "))
    {
        let p: Vec<f64> = line
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse().unwrap())
            .collect();
        // Independent Rx90 then Ry90 composition, including both translations.
        assert!(original.model.vertices.iter().any(|vertex| {
            let [x, y, z] = vertex.position;
            p.iter()
                .zip([y, 20. - z, -10. - x])
                .all(|(a, b)| (a - b).abs() < 1e-12)
        }));
    }
    let step = to_step(&result.model, tolerance).unwrap();
    assert_eq!(step.matches("FACE_SURFACE(").count(), 5);
    assert_eq!(step.matches("PLANE(").count(), 5);
    assert_eq!(step.matches("FACETED_BREP('").count(), 1);
    assert_eq!(step, to_step(&result.model, tolerance).unwrap());
}

#[test]
fn convex_intersection_exports_new_cut_plane_and_closed_mesh() {
    let tolerance = Tolerance::default();
    let triangle =
        cad_kernel::extrude_polygon(&[[0., 0.], [8., 0.], [0., 6.]], 5., tolerance).unwrap();
    let clip = cad_kernel::extrude_rectangle(6., 6., 5., tolerance).unwrap();
    let clip = cad_kernel::translate(&clip.model, [2., 0., 0.], tolerance).unwrap();
    let result = cad_kernel::boolean(
        &triangle.model,
        &clip.model,
        cad_kernel::BooleanOp::Intersection,
        tolerance,
    )
    .unwrap();
    check_mesh(&to_stl(&result.model, tolerance).unwrap(), 67.5, 8);
    let step = to_step(&result.model, tolerance).unwrap();
    assert_eq!(step.matches("FACE_SURFACE(").count(), 5);
    assert_eq!(step.matches("FACETED_BREP('").count(), 1);
    assert!(result
        .model
        .vertices
        .iter()
        .all(|v| v.position[0] >= 2. - 1e-12));

    let cube = cad_kernel::extrude_rectangle(4., 4., 4., tolerance).unwrap();
    let tool = cad_kernel::extrude_rectangle(4., 8., 8., tolerance).unwrap();
    let tool = cad_kernel::transform(
        &tool.model,
        [2., -4., -2.],
        [0., 0., 0.3826834323650898, 0.9238795325112867],
        tolerance,
    )
    .unwrap();
    let cut = cad_kernel::boolean(
        &cube.model,
        &tool.model,
        cad_kernel::BooleanOp::Intersection,
        tolerance,
    )
    .unwrap();
    // Tool's local x<=4 plane becomes x+y <= 4*sqrt(2)-2.
    let leg = 4. * 2f64.sqrt() - 2.;
    check_mesh(&to_stl(&cut.model, tolerance).unwrap(), 2. * leg * leg, 8);
    let step = to_step(&cut.model, tolerance).unwrap();
    assert_eq!(step.matches("FACE_SURFACE(").count(), 5);
    assert!(cut
        .model
        .vertices
        .iter()
        .all(|v| v.position[0] + v.position[1] <= leg + 1e-12));
    assert_eq!(step, to_step(&cut.model, tolerance).unwrap());
}

#[test]
fn through_holes_preserve_native_bounds_and_closed_triangle_material() {
    let outer = [[0., 0.], [20., 0.], [20., 10.], [0., 10.]];
    let holes = vec![
        vec![[2., 2.], [2., 6.], [6., 6.], [6., 2.]],
        vec![[12., 2.], [12., 6.], [16., 2.]],
    ];
    let model = cad_kernel::extrude_polygon_with_holes(&outer, &holes, 3., Tolerance::default())
        .unwrap()
        .model;
    let step = to_step(&model, Tolerance::default()).unwrap();
    assert_eq!(step.matches("FACE_OUTER_BOUND(").count(), 13);
    assert_eq!(step.matches("FACE_BOUND(").count(), 4);
    assert_eq!(step.matches("POLY_LOOP(").count(), 17);
    assert_eq!(step.matches("FACETED_BREP(").count(), 1);
    check_mesh(&to_stl(&model, Tolerance::default()).unwrap(), 528., 48);
    let angle = 0.4_f64;
    let rotated = cad_kernel::transform(
        &model,
        [3., 4., 5.],
        [angle.sin(), 0., 0., angle.cos()],
        Tolerance::default(),
    )
    .unwrap();
    check_mesh(
        &to_stl(&rotated.model, Tolerance::default()).unwrap(),
        528.,
        48,
    );
    assert_eq!(
        to_step(&rotated.model, Tolerance::default())
            .unwrap()
            .matches("FACE_BOUND(")
            .count(),
        4
    );
}

#[test]
fn nonconvex_caps_preserve_unsplit_step_faces_and_closed_stl_material() {
    // A 20 by 10 plate with a 6 by 6 bay opening onto its upper edge.
    let outline = [
        [0., 0.],
        [20., 0.],
        [20., 10.],
        [13., 10.],
        [13., 4.],
        [7., 4.],
        [7., 10.],
        [0., 10.],
    ];
    let model = cad_kernel::extrude_polygon_with_holes(&outline, &[], 3., Tolerance::default())
        .unwrap()
        .model;
    let step = to_step(&model, Tolerance::default()).unwrap();
    assert_eq!(step.matches("FACE_OUTER_BOUND(").count(), 10);
    assert_eq!(step.matches("POLY_LOOP(").count(), 10);
    assert_eq!(step.matches("FACE_BOUND(").count(), 0);
    check_mesh(&to_stl(&model, Tolerance::default()).unwrap(), 492., 28);
    let angle = 0.37_f64;
    let rotated = cad_kernel::transform(
        &model,
        [3., 4., 5.],
        [angle.sin(), 0., 0., angle.cos()],
        Tolerance::default(),
    )
    .unwrap();
    check_mesh(
        &to_stl(&rotated.model, Tolerance::default()).unwrap(),
        492.,
        28,
    );
}

#[test]
fn concave_holes_preserve_material_in_their_bays() {
    let t = Tolerance::default();
    let outer = [[0., 0.], [40., 0.], [40., 30.], [0., 30.]];
    let hole = vec![
        [5., 5.],
        [5., 23.],
        [9., 23.],
        [9., 9.],
        [25., 9.],
        [25., 23.],
        [29., 23.],
        [29., 5.],
    ];
    let model = cad_kernel::extrude_polygon_with_holes(
        &outer,
        &[hole, vec![[13., 13.], [13., 21.], [21., 21.], [21., 13.]]],
        6.,
        t,
    )
    .unwrap()
    .model;
    for model in [
        model.clone(),
        cad_kernel::transform(&model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
            .unwrap()
            .model,
    ] {
        let step = to_step(&model, t).unwrap();
        assert_eq!(step.matches("FACE_BOUND(").count(), 4);
        assert_eq!(step.matches("FACE_OUTER_BOUND(").count(), 18);
        // 16 boundary vertices, two holes: 18 triangles per cap and 32 walls.
        check_mesh(&to_stl(&model, t).unwrap(), 5568., 68);
    }
}
