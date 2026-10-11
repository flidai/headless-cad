//! Independent rational Green-theorem moments, distinct from kernel tetrahedra.
use std::{collections::BTreeMap, path::Path};
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-7_f64.max(expected.abs() * 1e-10),
        "actual {actual}, expected {expected}"
    );
}
#[test]
fn holed_plate_and_associative_locator_follow_material_moments_across_edits() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let source = std::fs::read_to_string(examples.join("perforated_mount.star")).unwrap();
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(examples.join("perforated_mount.properties.expected.json"))
            .unwrap(),
    )
    .unwrap();
    for case in expected["cases"].as_array().unwrap() {
        let width = case["slot_width"].as_f64().unwrap();
        let thickness = case["thickness"].as_f64().unwrap();
        let recipe = cad_recipe::from_starlark_with_params(
            "perforated_mount.star",
            &source,
            &BTreeMap::from([
                ("slot_width".into(), width),
                ("thickness".into(), thickness),
            ]),
        )
        .unwrap();
        let result = cad_build::build(&recipe).unwrap();
        let model = &result.outputs["plate"].result.model;
        assert_eq!(model.faces.len(), 17);
        assert_eq!(model.vertices.len(), 30);
        assert_eq!(model.edges.len(), 45);
        assert_eq!(model.faces.iter().filter(|f| f.loops.len() == 3).count(), 2);
        let properties = model
            .mass_properties(cad_kernel::Tolerance::default())
            .unwrap();
        close(properties.volume, case["volume"].as_f64().unwrap());
        let center = properties.centroid.unwrap();
        let inertia = properties.central_inertia.unwrap();
        for axis in 0..3 {
            close(center[axis], case["centroid"][axis].as_f64().unwrap());
            for column in 0..3 {
                close(
                    inertia[axis][column],
                    case["central_inertia"][axis][column].as_f64().unwrap(),
                );
            }
        }
        let mount = &result.assemblies["perforated_mount"];
        let locator = mount.instances.iter().find(|i| i.id == "locator").unwrap();
        let foot = locator.transform.point([2., 2., 0.]);
        close(foot[0], center[0]);
        close(foot[1], 25.);
        close(foot[2], 15. + thickness);
        close(mount.material_volume(), properties.volume + 13328.);
        assert_eq!(mount.instances.len(), 7);
    }
}
