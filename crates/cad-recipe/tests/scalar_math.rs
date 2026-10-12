use cad_recipe::from_starlark;

#[test]
fn finite_scalar_formulas_are_replayable_and_domain_checked() {
    let source = r#"
def check_math():
    if sqrt(9) != 3 or sqrt(0) != 0:
        fail("square root boundary")
    if asin(0) != 0 or abs(asin(1)-1.5707963267948966) > 1e-15:
        fail("inverse sine boundary")
    if abs(asin(-1)+asin(1)) > 1e-15:
        fail("inverse sine sign")
check_math()
p = rectangle(id="p", width=sqrt(25), depth=2+asin(0))
b = extrude(id="b", profile=p, height=3)
output(name="body", feature=b)
"#;
    let first = from_starlark("math.star", source).unwrap();
    let replay = from_starlark("math.star", source).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(
        cad_build::build(&first).unwrap().outputs["body"]
            .result
            .model
            .volume(),
        30.
    );
    for expression in [
        "sqrt(-1)",
        "asin(1.00001)",
        "asin(-1.00001)",
        "sqrt(float('inf'))",
        "asin(float('nan'))",
    ] {
        assert!(
            from_starlark("invalid.star", &format!("value = {expression}\n")).is_err(),
            "{expression}"
        );
    }
}
