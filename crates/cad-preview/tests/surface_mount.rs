//! Associative mounting oracle from the ramp's independent slope equation.
use std::{collections::BTreeMap, path::Path};

fn close(actual: [f64; 3], expected: [f64; 3]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!(
            (a - e).abs() < 1e-7,
            "actual {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn changing_ramp_slope_moves_and_turns_attached_plate_from_face_intent() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/surface_mount.star"),
    )
    .unwrap();
    for rise in [12.0_f64, 20.0, 28.0] {
        let params = BTreeMap::from([("ramp_rise".into(), rise)]);
        let recipe =
            cad_recipe::from_starlark_with_params("surface_mount.star", &source, &params).unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let assembly = &built.assemblies["surface_mount"];
        let seat = &assembly.parts["ramp"].datums["seat"];
        let hypotenuse = 40.0_f64.hypot(rise);
        let x_axis = [40.0 / hypotenuse, 0.0, -rise / hypotenuse];
        let z_axis = [rise / hypotenuse, 0.0, 40.0 / hypotenuse];
        close(seat.translation, [20.0, 15.0, rise / 2.0]);
        close(seat.vector([1.0, 0.0, 0.0]), x_axis);
        close(seat.vector([0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]);
        close(seat.vector([0.0, 0.0, 1.0]), z_axis);
        let occurrence = assembly.instances.iter().find(|i| i.id == "plate").unwrap();
        let mount = [30.0, 25.0, 4.0 + rise / 2.0];
        close(occurrence.transform.point([6.0, 10.0, 0.0]), mount);
        let actual: Vec<_> = assembly.parts["plate"]
            .result
            .model
            .vertices
            .iter()
            .map(|v| occurrence.transform.point(v.position))
            .collect();
        for along in [-6.0, 6.0] {
            for across in [-10.0, 10.0] {
                for height in [0.0, 2.0] {
                    let expected = [
                        mount[0] + along * x_axis[0] + height * z_axis[0],
                        mount[1] + across,
                        mount[2] + along * x_axis[2] + height * z_axis[2],
                    ];
                    assert_eq!(
                        actual
                            .iter()
                            .filter(|point| (0..3)
                                .all(|axis| (point[axis] - expected[axis]).abs() < 1e-7))
                            .count(),
                        1
                    );
                    // Signed perpendicular distance to the source ramp plane.
                    let distance = (rise * (expected[0] - 10.0) + 40.0 * (expected[2] - 4.0)
                        - 40.0 * rise)
                        / hypotenuse;
                    assert!((distance - height).abs() < 1e-7);
                }
            }
        }
        assert!((assembly.material_volume() - (12480.0 + 600.0 * rise)).abs() < 1e-7);
    }
}
