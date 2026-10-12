use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/modular_linear_stage.star");
fn modules() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "recipe_modules/stock.star".into(),
            include_str!("../../../examples/recipe_modules/stock.star").into(),
        ),
        (
            "recipe_modules/drive_parts.star".into(),
            include_str!("../../../examples/recipe_modules/drive_parts.star").into(),
        ),
    ])
}
#[test]
fn modular_stage_reuses_definitions_and_matches_independent_mass_and_sections() {
    let mut engine = cad_build::BuildEngine::default();
    for length in [140., 160., 180.] {
        for setback in [1., 2., 3.] {
            for bore in [2., 3., 4.] {
                let params = BTreeMap::from([
                    ("length".into(), length),
                    ("setback".into(), setback),
                    ("bore".into(), bore),
                ]);
                let recipe = cad_recipe::from_starlark_with_modules(
                    "modular_linear_stage.star",
                    SOURCE,
                    &params,
                    &modules(),
                )
                .unwrap();
                let build = engine.build(&recipe).unwrap();
                let assembly = &build.assemblies["modular_linear_stage"];
                assert_eq!(assembly.parts.len(), 2);
                assert_eq!(assembly.instances.len(), 4);
                let area = 3600. - 4. * std::f64::consts::PI * bore * bore;
                let mass = 2. * length * (400. - setback * setback) * 7800. / 1e9
                    + 2. * area * 8. * 2700. / 1e9;
                assert!(
                    (build.property_assertions["material_mass"].actual.unwrap() - mass).abs()
                        < 1e-8
                );
                assert!(
                    (build.section_views["carriages"].query.section.area - 2. * area).abs() < 1e-7
                );
                assert!(
                    (build.section_views["rails"].query.section.area - 40. * length).abs() < 1e-7
                );
                assert_eq!(engine.build(&recipe).unwrap().stats.computed_solids, 0);
            }
        }
    }
}
