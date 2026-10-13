use std::collections::BTreeMap;

use cad_recipe::{
    from_starlark_with_modules, from_starlark_with_modules_and_provenance, SourceSpan,
};

fn slice<'a>(source: &'a str, span: &SourceSpan) -> &'a str {
    source.get(span.start_byte..span.end_byte).unwrap()
}

#[test]
fn direct_multiline_calls_are_exact_and_outside_the_recipe() {
    let source = "# café λ\nprofile = rectangle(\n    id='outline',\n    width=20, depth=10,\n)\nbody = extrude(id='blank', profile=profile, height=5)\np = part(\n    id='plate',\n    body=body,\n)\nassembly_output(name='scene',assembly=assembly(id='root',instances=[instance(id='one',part=p)]))\n";
    let result = from_starlark_with_modules_and_provenance(
        "main.star",
        source,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        slice(source, &result.provenance.features["outline"].declaration),
        "rectangle(\n    id='outline',\n    width=20, depth=10,\n)"
    );
    let part = &result.provenance.parts["plate"].declaration;
    assert_eq!(
        slice(source, part),
        "part(\n    id='plate',\n    body=body,\n)"
    );
    assert_eq!(part.file, "main.star");
    assert_eq!(part.start_line, 7);
    assert_eq!(part.start_column, 5);
    assert_eq!(
        serde_json::to_value(&result.recipe).unwrap(),
        serde_json::to_value(
            from_starlark_with_modules("main.star", source, &BTreeMap::new(), &BTreeMap::new())
                .unwrap()
        )
        .unwrap()
    );
    assert!(serde_json::to_string(&result.recipe)
        .unwrap()
        .find("main.star")
        .is_none());
}

#[test]
fn imported_helper_and_loop_record_executed_definition_and_caller() {
    let helper = "def make():\n    made = []\n    for i in range(2):\n        profile = rectangle(id='profile-' + str(i), width=5, depth=6)\n        body = extrude(id='body-' + str(i), profile=profile, height=2)\n        made.append(part(id='part-' + str(i), body=body))\n    return made\n";
    let root = "load('lib/parts.star','make')\nps=make()\nassembly_output(name='scene',assembly=assembly(id='root',instances=[instance(id='a',part=ps[0]),instance(id='b',part=ps[1],offset=[10,0,0])]))\n";
    let modules = BTreeMap::from([("lib/parts.star".to_owned(), helper.to_owned())]);
    let result =
        from_starlark_with_modules_and_provenance("main.star", root, &BTreeMap::new(), &modules)
            .unwrap();
    assert_eq!(result.provenance.features.len(), 4);
    assert_eq!(result.provenance.parts.len(), 2);
    for id in ["part-0", "part-1"] {
        let source = &result.provenance.parts[id];
        assert_eq!(source.declaration.file, "lib/parts.star");
        assert_eq!(
            slice(helper, &source.declaration),
            "part(id='part-' + str(i), body=body)"
        );
        assert!(source
            .call_stack
            .iter()
            .any(|frame| { frame.file == "main.star" && slice(root, frame) == "make()" }));
    }
    assert_eq!(
        result.provenance.parts["part-0"].declaration,
        result.provenance.parts["part-1"].declaration
    );
}

#[test]
fn comment_shift_only_moves_provenance_and_replay_is_deterministic() {
    let source = "p=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\npart(id='part',body=b)\n";
    let shifted = format!("# é comment\n{source}");
    let run = |source: &str| {
        from_starlark_with_modules_and_provenance(
            "entry.star",
            source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap()
    };
    let a = run(source);
    let b = run(source);
    let c = run(&shifted);
    assert_eq!(a.provenance, b.provenance);
    assert_eq!(
        serde_json::to_value(&a.recipe).unwrap(),
        serde_json::to_value(&c.recipe).unwrap()
    );
    assert_eq!(
        c.provenance.features["p"].declaration.start_byte,
        a.provenance.features["p"].declaration.start_byte + "# é comment\n".len()
    );
    assert_eq!(
        c.provenance.features["p"].declaration.start_line,
        a.provenance.features["p"].declaration.start_line + 1
    );
    let round_trip = serde_json::from_str(&serde_json::to_string(&a.provenance).unwrap()).unwrap();
    assert_eq!(a.provenance, round_trip);
}

#[test]
fn byte_offsets_and_character_columns_are_distinct_after_unicode() {
    let source = "p=(\"é\", rectangle(id='p',width=2,depth=3))[1]\n";
    let result = from_starlark_with_modules_and_provenance(
        "unicode.star",
        source,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let span = &result.provenance.features["p"].declaration;
    assert_eq!(slice(source, span), "rectangle(id='p',width=2,depth=3)");
    assert_eq!(span.start_byte, 9);
    assert_eq!(span.start_line, 1);
    assert_eq!(span.start_column, 9);
}
