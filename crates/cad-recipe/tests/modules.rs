use cad_recipe::*;
use std::collections::BTreeMap;
fn bundle(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}
#[test]
fn imported_functions_share_graph_parameters_and_frozen_exports() {
    let modules = bundle(&[(
        "lib/plate.star",
        r#"
scale=param(name="width",default=10)
_hidden=42
def plate(id):
    p=rectangle(id=id+"_profile",width=scale,depth=4)
    return extrude(id=id,profile=p,height=2)
"#,
    )]);
    let root = "load('lib/plate.star', 'plate')\noutput(name='part',feature=plate('body'))\n";
    let recipe = from_starlark_with_modules(
        "root.star",
        root,
        &BTreeMap::from([("width".into(), 12.)]),
        &modules,
    )
    .unwrap();
    assert_eq!(
        cad_build::build(&recipe).unwrap().outputs["part"]
            .result
            .model
            .volume(),
        96.
    );
    assert!(from_starlark_with_modules(
        "root.star",
        "load('lib/plate.star','_hidden')",
        &BTreeMap::new(),
        &modules
    )
    .is_err());
    assert!(from_starlark_with_modules(
        "root.star",
        "load('lib/plate.star','missing')",
        &BTreeMap::new(),
        &modules
    )
    .is_err());
    assert!(from_starlark("root.star", root).is_err());
    assert!(from_starlark_with_modules(
        "root.star",
        root,
        &BTreeMap::from([("typo".into(), 1.)]),
        &modules
    )
    .is_err());
}
#[test]
fn diamond_evaluates_shared_module_once_and_does_not_evaluate_unused_modules() {
    let modules = bundle(&[
        (
            "base.star",
            "shared=rectangle(id='one',width=3,depth=4)\ndef base():\n    return shared\n",
        ),
        (
            "left.star",
            "load('base.star','base')\ndef left():\n    return base()\n",
        ),
        (
            "right.star",
            "load('base.star','base')\ndef right():\n    return base()\n",
        ),
        ("unused.star", "fail('unused module was evaluated')\n"),
    ]);
    let root="load('left.star','left')\nload('right.star','right')\na=extrude(id='a',profile=left(),height=2)\nb=extrude(id='b',profile=right(),height=3)\noutput(name='a',feature=a)\noutput(name='b',feature=b)\n";
    let first = from_starlark_with_modules("root.star", root, &BTreeMap::new(), &modules).unwrap();
    assert_eq!(first.features.len(), 3);
    let built = cad_build::build(&first).unwrap();
    assert_eq!(built.outputs["a"].result.model.volume(), 24.);
    assert_eq!(built.outputs["b"].result.model.volume(), 36.);
    let again = from_starlark_with_modules("root.star", root, &BTreeMap::new(), &modules).unwrap();
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
}
#[test]
fn imports_reject_missing_cycles_noncanonical_labels_and_entry_collisions() {
    let empty = BTreeMap::new();
    assert!(from_starlark_with_modules(
        "root.star",
        "load('missing.star','x')",
        &empty,
        &BTreeMap::new()
    )
    .is_err());
    let cycle = bundle(&[
        ("a.star", "load('b.star','x')\nx=1"),
        ("b.star", "load('a.star','x')\nx=1"),
    ]);
    assert!(
        from_starlark_with_modules("root.star", "load('a.star','x')", &empty, &cycle)
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    for label in [
        "/absolute.star",
        "../up.star",
        "a/../b.star",
        "a//b.star",
        "./a.star",
        "a\\b.star",
        "a/./b.star",
        "C:/a.star",
        "a.py",
        " a.star",
        "a.star\n",
    ] {
        assert!(validate_module_label(label).is_err(), "{label}");
    }
    assert!(
        from_starlark_with_modules("root.star", "", &empty, &bundle(&[("root.star", "")])).is_err()
    );
    assert_eq!(
        starlark_loads("root.star", "load('lib/a.star','a')\nload('b.star','b')").unwrap(),
        ["lib/a.star", "b.star"]
    );
}
#[test]
fn cross_module_parameters_detect_conflicts_only_after_complete_evaluation() {
    let modules = bundle(&[
        ("a.star", "x=param(name='width',default=10)"),
        ("b.star", "y=param(name='depth',default=4)"),
    ]);
    let root = "load('a.star','x')\nload('b.star','y')\np=rectangle(id='p',width=x,depth=y)\n";
    let overrides = BTreeMap::from([("width".into(), 12.), ("depth".into(), 6.)]);
    assert!(from_starlark_with_modules("root.star", root, &overrides, &modules).is_ok());
    let bad = bundle(&[
        ("a.star", "x=param(name='width',default=10)"),
        ("b.star", "y=param(name='width',default=11)"),
    ]);
    assert!(from_starlark_with_modules("root.star", root, &BTreeMap::new(), &bad).is_err());
}
#[test]
fn source_module_count_heap_and_tick_budgets_are_enforced() {
    let overrides = BTreeMap::new();
    let too_many: BTreeMap<_, _> = (0..=MAX_RECIPE_MODULES)
        .map(|i| (format!("{i}.star"), String::new()))
        .collect();
    assert!(
        from_starlark_with_modules("root.star", "", &overrides, &too_many)
            .unwrap_err()
            .to_string()
            .contains("supplied modules")
    );
    assert!(
        from_starlark("root.star", &" ".repeat(MAX_MODULE_SOURCE_BYTES + 1))
            .unwrap_err()
            .to_string()
            .contains("source exceeds")
    );
    let large: BTreeMap<_, _> = (0..9)
        .map(|i| (format!("{i}.star"), " ".repeat(MAX_MODULE_SOURCE_BYTES)))
        .collect();
    assert!(
        from_starlark_with_modules("root.star", "", &overrides, &large)
            .unwrap_err()
            .to_string()
            .contains("source bundle")
    );
    let excessive = "def work():\n    for i in range(2000000):\n        pass\nwork()\n";
    assert!(from_starlark_with_modules(
        "root.star",
        "load('a.star','work')",
        &overrides,
        &bundle(&[("a.star", excessive)])
    )
    .is_err());
    let heap = "data='x' * 20000000\n";
    assert!(from_starlark_with_modules(
        "root.star",
        "load('a.star','data')",
        &overrides,
        &bundle(&[("a.star", heap)])
    )
    .is_err());
    let busy = "def work():\n    for i in range(800000):\n        pass\nwork()\nx=1\n";
    assert!(from_starlark("single.star", busy).is_ok());
    let modules: BTreeMap<_, _> = (0..6).map(|i| (format!("{i}.star"), busy.into())).collect();
    let root = (0..6)
        .map(|i| format!("load('{i}.star',x{i}='x')\n"))
        .collect::<String>();
    assert!(from_starlark_with_modules("root.star", &root, &overrides, &modules).is_err());
    let moderate = "data='x' * 12000000\n";
    assert!(from_starlark("single.star", moderate).is_ok());
    let modules: BTreeMap<_, _> = (0..6)
        .map(|i| (format!("{i}.star"), moderate.into()))
        .collect();
    let root = (0..6)
        .map(|i| format!("load('{i}.star',data{i}='data')\n"))
        .collect::<String>();
    assert!(from_starlark_with_modules("root.star", &root, &overrides, &modules).is_err());
}
