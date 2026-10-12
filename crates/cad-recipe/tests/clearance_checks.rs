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

#[test]
fn source_revolved_cone_clearance_is_native() {
    let source = SOURCE
        .replace(
            "extrude(id='body',profile=rectangle(id='p',width=2,depth=3),height=4)",
            "revolve(id='body',profile=polygon(id='p',points=[[0,0],[2,0],[0,4]]))",
        )
        .replace("offset=[5,7,0]", "offset=[0,0,7]");
    let check = CHECK.replace("min_clearance=4.99", "min_clearance=2.99");
    let r = cad_recipe::from_starlark("cone.star", &format!("{source}{check}")).unwrap();
    let b = cad_build::build(&r).unwrap();
    let q = &b.clearance_checks["gap"];
    assert!(q.lower_bound_mm <= 3. && q.upper_bound_mm >= 3.);
}

#[test]
fn source_coaxial_shaft_inside_ring_reports_meridian_method() {
    let source = "shaft=extrude(id='shaft',profile=circle(id='sp',radius=1),height=4)\nring=extrude(id='ring',profile=circle(id='rp',radius=3,inner_radius=2),height=4)\na=part(id='shaft_part',body=shaft)\nb=part(id='ring_part',body=ring)\nassembly_output(name='pair',assembly=assembly(id='root',instances=[instance(id='left',part=a),instance(id='right',part=b)]))\n";
    let check = CHECK.replace("min_clearance=4.99", "min_clearance=0.99");
    let r = cad_recipe::from_starlark("coaxial.star", &format!("{source}{check}")).unwrap();
    let b = cad_build::build(&r).unwrap();
    let q = &b.clearance_checks["gap"].query;
    assert_eq!(
        serde_json::to_value(q).unwrap()["method"],
        "coaxial_meridian"
    );
    assert!(q.lower_bound_mm <= 1. && q.upper_bound_mm >= 1.);
}

#[test]
fn source_eccentric_shaft_inside_ring_reports_parallel_annular_method() {
    let source = "shaft=extrude(id='shaft',profile=circle(id='sp',radius=1),height=4)\nring=extrude(id='ring',profile=circle(id='rp',radius=3,inner_radius=2),height=4)\na=part(id='shaft_part',body=shaft)\nb=part(id='ring_part',body=ring)\nassembly_output(name='pair',assembly=assembly(id='root',instances=[instance(id='left',part=a),instance(id='right',part=b,offset=[0.25,0,0])]))\n";
    let check = CHECK.replace("min_clearance=4.99", "min_clearance=0.74");
    let r = cad_recipe::from_starlark("eccentric.star", &format!("{source}{check}")).unwrap();
    let b = cad_build::build(&r).unwrap();
    let q = &b.clearance_checks["gap"].query;
    assert_eq!(
        serde_json::to_value(q).unwrap()["method"],
        "parallel_annular"
    );
    assert!(q.lower_bound_mm <= 0.75 && q.upper_bound_mm >= 0.75);
}

#[test]
fn source_translation_optional_vector_is_strict_and_preserves_static_defaults() {
    let source = SOURCE.replace("offset=[5,7,0]", "offset=[5,0,0]");
    let check = CHECK.replace(
        "min_clearance=4.99",
        "min_clearance=2,right_translation=[0,0,2]",
    );
    let r = cad_recipe::from_starlark("sweep.star", &format!("{source}{check}")).unwrap();
    assert_eq!(r.clearance_checks[0].right_translation, Some([0., 0., 2.]));
    assert!(cad_build::build(&r).unwrap().clearance_checks["gap"]
        .sweep
        .is_some());
    for vector in ["[0,0,0]", "[1,2]", "'bad'", "[1000001,0,0]"] {
        let invalid = check.replace("[0,0,2]", vector);
        assert!(cad_recipe::from_starlark("invalid.star", &format!("{source}{invalid}")).is_err());
    }
    let none = CHECK.replace(
        "min_clearance=4.99",
        "min_clearance=2,right_translation=None",
    );
    let r = cad_recipe::from_starlark("static.star", &format!("{source}{none}")).unwrap();
    assert!(r.clearance_checks[0].right_translation.is_none());
}

#[test]
fn source_rotation_path_is_strict_optional_and_mutually_exclusive() {
    let source = SOURCE.replace("offset=[5,7,0]", "offset=[8,0,0]");
    let declaration = "{'pivot':[1,1.5,2],'axis':[0,0,1],'angle_radians':1}";
    let check = CHECK.replace(
        "min_clearance=4.99",
        &format!("min_clearance=4,right_rotation={declaration}"),
    );
    let r = cad_recipe::from_starlark("rotation.star", &format!("{source}{check}")).unwrap();
    assert!(cad_build::build(&r).unwrap().clearance_checks["gap"]
        .rotation_sweep
        .is_some());
    for declaration in [
        "[]",
        "{'pivot':[0,0,0],'axis':[0,0,2],'angle_radians':1}",
        "{'pivot':[0,0,0],'axis':[0,0,1],'angle_radians':0}",
        "{'pivot':[0,0,0],'axis':[0,0,1],'angle_radians':1,'extra':1}",
    ] {
        let invalid = CHECK.replace(
            "min_clearance=4.99",
            &format!("min_clearance=4,right_rotation={declaration}"),
        );
        assert!(cad_recipe::from_starlark("invalid.star", &format!("{source}{invalid}")).is_err());
    }
    let both = check.replace(
        "min_clearance=4,",
        "min_clearance=4,right_translation=[1,0,0],",
    );
    assert!(cad_recipe::from_starlark("exclusive.star", &format!("{source}{both}")).is_err());
    let none = CHECK.replace("min_clearance=4.99", "min_clearance=4,right_rotation=None");
    let r = cad_recipe::from_starlark("static.star", &format!("{source}{none}")).unwrap();
    assert!(r.clearance_checks[0].right_rotation.is_none());
}
