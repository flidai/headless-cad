//! Composite engineering oracle from independent 50-digit rectangular moments.
//! Boxes (a,b,h), beam-axis t, center height z, and density kg/m³:
//! base (40,40,6),0,3,2700; post (8,8,30),0,21,7800;
//! beam (120,12,6),0,39,2700; weight (20,16,12),-d,48,7800;
//! payload (20,20,20),50,52,2700. All share a 30-degree longitudinal frame;
//! square base/post footprints make their inertia independent of that turn.
//! With weighted T,Z and c=√3/2,s=1/2, each contribution is:
//! Ixx=w[((b²+h²)c²+(a²+h²)s²)/12+s²(t-T)²+(z-Z)²],
//! Iyy swaps c²/s²; Izz=w[(a²+b²)/12+(t-T)²];
//! Ixy=w cs[(b²-a²)/12-(t-T)²]; Ixz=-wc(t-T)(z-Z); Iyz=-ws(t-T)(z-Z).
use std::{collections::BTreeMap, path::Path};

fn close(actual: f64, expected: &serde_json::Value) {
    let expected = expected.as_f64().unwrap();
    assert!(
        (actual - expected).abs() <= 1e-10_f64.max(expected.abs() * 1e-11),
        "actual {actual}, independently expected {expected}"
    );
}
fn vector(actual: [f64; 3], expected: &serde_json::Value) {
    for (axis, value) in actual.into_iter().enumerate() {
        close(value, &expected[axis]);
    }
}
fn tensor(actual: [[f64; 3]; 3], expected: &serde_json::Value) {
    for (row, values) in actual.into_iter().enumerate() {
        vector(values, &expected[row]);
    }
}

#[test]
fn mixed_material_balance_arm_matches_independent_moments_across_placement_edits() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let source = std::fs::read_to_string(examples.join("balance_arm.star")).unwrap();
    let expectations: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(examples.join("balance_arm.properties.expected.json")).unwrap(),
    )
    .unwrap();
    for case in expectations["cases"].as_array().unwrap() {
        let distance = case["counterweight_distance"].as_f64().unwrap();
        let parameters = BTreeMap::from([("counterweight_distance".into(), distance)]);
        let recipe =
            cad_recipe::from_starlark_with_params("balance_arm.star", &source, &parameters)
                .unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let assembly = &built.assemblies["balance_arm"];
        let properties = assembly
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        assert_eq!(properties.missing_density_occurrences, 0);
        close(properties.volume, &case["geometry"]["weight"]);
        vector(
            properties.volume_centroid.unwrap(),
            &case["geometry"]["centroid"],
        );
        tensor(
            properties.central_volume_inertia.unwrap(),
            &case["geometry"]["central_inertia"],
        );
        close(properties.mass_kg.unwrap(), &case["mass"]["weight"]);
        vector(
            properties.center_of_mass.unwrap(),
            &case["mass"]["centroid"],
        );
        tensor(
            properties.central_mass_inertia.unwrap(),
            &case["mass"]["central_inertia"],
        );
        let bom = assembly
            .bill_of_materials(recipe.tolerance.unwrap_or_default())
            .unwrap();
        assert!((bom.total_mass_kg.unwrap() - properties.mass_kg.unwrap()).abs() < 1e-12);
        // Material weighting is observable; returning the volume centroid as
        // the center of mass would pass a uniform-density-only test.
        assert!(
            (properties.volume_centroid.unwrap()[0] - properties.center_of_mass.unwrap()[0]).abs()
                > 1.0
        );
    }
}
