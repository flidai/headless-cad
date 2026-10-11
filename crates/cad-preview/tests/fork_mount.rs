//! Independent signed-box volume/inertia oracle for a concave profile with holes.
use std::{collections::BTreeMap, path::Path};
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-7_f64.max(expected.abs() * 1e-10),
        "actual {actual}, expected {expected}"
    );
}
#[test]
fn concave_mount_and_rotated_native_body_match_independent_box_moments() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let source = std::fs::read_to_string(examples.join("fork_mount.star")).unwrap();
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(examples.join("fork_mount.properties.expected.json")).unwrap(),
    )
    .unwrap();
    for case in expected["cases"].as_array().unwrap() {
        let w = case["notch_width"].as_f64().unwrap();
        let d = case["notch_depth"].as_f64().unwrap();
        let t = case["thickness"].as_f64().unwrap();
        let recipe = cad_recipe::from_starlark_with_params(
            "fork_mount.star",
            &source,
            &BTreeMap::from([
                ("notch_width".into(), w),
                ("notch_depth".into(), d),
                ("thickness".into(), t),
            ]),
        )
        .unwrap();
        let result = cad_build::build(&recipe).unwrap();
        let blank = &result.outputs["blank"].result.model;
        assert_eq!(blank.faces.len(), 10);
        assert!(blank.faces.iter().all(|f| f.loops.len() == 1));
        close(blank.volume(), (4800. - w * d) * t);
        let model = &result.outputs["fork"].result.model;
        assert_eq!(model.faces.len(), 18);
        assert_eq!(model.vertices.len(), 32);
        assert_eq!(model.edges.len(), 48);
        assert_eq!(model.faces.iter().filter(|f| f.loops.len() == 3).count(), 2);
        let tol = cad_kernel::Tolerance::default();
        let properties = model.mass_properties(tol).unwrap();
        close(properties.volume, case["volume"].as_f64().unwrap());
        let center = properties.centroid.unwrap();
        let inertia = properties.central_inertia.unwrap();
        for i in 0..3 {
            close(center[i], case["centroid"][i].as_f64().unwrap());
            for j in 0..3 {
                close(
                    inertia[i][j],
                    case["central_inertia"][i][j].as_f64().unwrap(),
                );
            }
        }
        let mount = &result.assemblies["fork_mount"];
        let locator = mount.instances.iter().find(|i| i.id == "locator").unwrap();
        let foot = locator.transform.point([2., 2., 0.]);
        close(foot[0], 40.);
        close(foot[1], center[1] - 12.);
        close(foot[2], 15. + t);
        assert!(
            foot[1] + 2. < 60. - d,
            "registration block must remain on front web"
        );
        close(mount.material_volume(), properties.volume + 20228.);
        // A native Y-axis rotation creates off-diagonal inertia. Assembly-only
        // placement cannot make this check pass: the kernel coordinates change.
        let c = 3f64.sqrt() / 2.;
        let s = 0.5;
        let rotated = cad_kernel::transform(
            model,
            [12., -7., 4.],
            [
                0.,
                (15f64).to_radians().sin(),
                0.,
                (15f64).to_radians().cos(),
            ],
            tol,
        )
        .unwrap();
        let moved = rotated.model.mass_properties(tol).unwrap();
        let expected_center = [
            12. + c * center[0] + s * center[2],
            -7. + center[1],
            4. - s * center[0] + c * center[2],
        ];
        for (a, b) in moved.centroid.unwrap().into_iter().zip(expected_center) {
            close(a, b);
        }
        let moved_i = moved.central_inertia.unwrap();
        close(moved_i[0][0], c * c * inertia[0][0] + s * s * inertia[2][2]);
        close(moved_i[1][1], inertia[1][1]);
        close(moved_i[2][2], s * s * inertia[0][0] + c * c * inertia[2][2]);
        close(moved_i[0][2], s * c * (inertia[2][2] - inertia[0][0]));
        close(moved_i[2][0], moved_i[0][2]);
    }
}
