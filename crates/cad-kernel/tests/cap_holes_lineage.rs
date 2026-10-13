use cad_kernel::{drill_cap_pattern, extrude_rectangle, CapBlindHole, FaceId, Tolerance};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn asymmetric_named_floors_survive_both_caps_and_reversed_order() {
    let tolerance = Tolerance::default();
    let body = extrude_rectangle(40., 30., 12., tolerance).unwrap().model;
    let holes = vec![
        CapBlindHole {
            id: "alpha".into(),
            center: [-10., 3.],
            radius: 1.5,
            depth: 3.,
        },
        CapBlindHole {
            id: "beta".into(),
            center: [8., -4.],
            radius: 2.5,
            depth: 5.,
        },
    ];
    let removed: Vec<f64> = holes
        .iter()
        .map(|hole| std::f64::consts::PI * hole.radius.powi(2) * hole.depth)
        .collect();
    let expected_volume = 40. * 30. * 12. - removed.iter().sum::<f64>();
    for (cap, opening_z) in [(FaceId(1), 12.), (FaceId(0), 0.)] {
        let frame_y = if cap == FaceId(1) { 1. } else { -1. };
        let world_center =
            |hole: &CapBlindHole| [21. + hole.center[0], 15. + frame_y * (2. + hole.center[1])];
        for ordered in [holes.clone(), holes.iter().cloned().rev().collect()] {
            let result =
                drill_cap_pattern(&body, cap, [1., 0., 0.], [1., 2.], &ordered, tolerance).unwrap();
            assert!(result.model.validate(tolerance).valid);
            let mass = result.model.mass_properties(tolerance).unwrap();
            close(mass.volume, expected_volume);
            let hole_z = |depth: f64| {
                if cap == FaceId(1) {
                    opening_z - depth / 2.
                } else {
                    opening_z + depth / 2.
                }
            };
            let expected_centroid = [
                (40. * 30. * 12. * 20.
                    - removed[0] * world_center(&holes[0])[0]
                    - removed[1] * world_center(&holes[1])[0])
                    / expected_volume,
                (40. * 30. * 12. * 15.
                    - removed[0] * world_center(&holes[0])[1]
                    - removed[1] * world_center(&holes[1])[1])
                    / expected_volume,
                (40. * 30. * 12. * 6.
                    - removed[0] * hole_z(holes[0].depth)
                    - removed[1] * hole_z(holes[1].depth))
                    / expected_volume,
            ];
            for (actual, expected) in mass.centroid.unwrap().into_iter().zip(expected_centroid) {
                close(actual, expected);
            }
            for hole in &holes {
                let floor = &result.history.generated_faces[&format!("hole_{}_floor", hole.id)];
                let wall = &result.history.generated_faces[&format!("hole_{}_wall", hole.id)];
                assert_eq!((floor.len(), wall.len()), (1, 1));
                let plane = result.model.faces[floor[0].0].try_plane().unwrap();
                close(plane.origin[0], world_center(hole)[0]);
                close(plane.origin[1], world_center(hole)[1]);
                close(
                    plane.origin[2],
                    if cap == FaceId(1) {
                        opening_z - hole.depth
                    } else {
                        opening_z + hole.depth
                    },
                );
            }
            let lineage = &result.history.input_faces["body"];
            assert_eq!(lineage.len(), body.faces.len());
            for index in 0..body.faces.len() {
                assert_eq!(lineage[&FaceId(index)], vec![FaceId(index)]);
            }
        }
    }
}
