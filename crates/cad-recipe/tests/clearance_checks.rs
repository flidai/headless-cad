const SOURCE:&str="b=extrude(id='body',profile=rectangle(id='p',width=2,depth=3),height=4)\npartdef=part(id='block',body=b)\nassembly_output(name='pair',assembly=assembly(id='root',instances=[instance(id='left',part=partdef),instance(id='right',part=partdef,offset=[5,7,0])]))\n";
const CHECK:&str="clearance_check(id='gap',output='pair',left_instance_path=['left'],right_instance_path=['right'],min_clearance=4.99)";
#[test]
fn source_clearance_roundtrips_and_uses_native_euclidean_distance() {
    let r = cad_recipe::from_starlark("clearance.star", &format!("{SOURCE}{CHECK}")).unwrap();
    let decoded: cad_build::Recipe =
        serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(r.clearance_checks, decoded.clearance_checks);
    let b = cad_build::build(&r).unwrap();
    let q = &b.clearance_checks["gap"];
    assert!(q.lower_bound_mm < 5. && q.upper_bound_mm > 5.);
}
#[test]
fn malformed_source_clearance_rejects_before_build() {
    for c in [
        CHECK.replace("min_clearance=4.99", "min_clearance=0"),
        CHECK.replace("['left']", "[]"),
        CHECK.replace("['right']", "['left']"),
        CHECK.replace("id='gap'", "id='../bad'"),
    ] {
        assert!(cad_recipe::from_starlark("bad.star", &format!("{SOURCE}{c}")).is_err());
    }
    assert!(cad_recipe::from_starlark(
        "bad.star",
        &format!("{SOURCE}{CHECK}\n{}", CHECK.replace("id='gap'", "id='GAP'"))
    )
    .is_err());
}

#[test]
fn source_solid_cylinder_clearance_uses_analytic_geometry() {
    let source = SOURCE
        .replace(
            "rectangle(id='p',width=2,depth=3)",
            "circle(id='p',radius=1)",
        )
        .replace("offset=[5,7,0]", "offset=[5,0,0]");
    let check = CHECK.replace("min_clearance=4.99", "min_clearance=2.99");
    let r = cad_recipe::from_starlark("cylinder.star", &format!("{source}{check}")).unwrap();
    let b = cad_build::build(&r).unwrap();
    let q = &b.clearance_checks["gap"];
    assert!(q.lower_bound_mm <= 3. && q.upper_bound_mm >= 3.);
}
