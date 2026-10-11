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
