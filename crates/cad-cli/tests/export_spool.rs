use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap()
}

fn success(args: &[&str]) -> Value {
    let result = run(args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn source(count: usize) -> String {
    let mut source = String::new();
    for index in 0..count {
        source.push_str(&format!(
            "p{index} = circle(id='p{index}', radius={:.3})\n\
             b{index} = extrude(id='b{index}', profile=p{index}, height={:.3})\n\
             output(name='part_{index:03}', feature=b{index})\n",
            2. + index as f64 / 1000.,
            3. + index as f64 / 1000.
        ));
    }
    source
}

fn tree(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .unwrap()
        .map(|item| item.unwrap())
        .map(|item| {
            (
                item.file_name().to_string_lossy().into_owned(),
                fs::read(item.path()).unwrap(),
            )
        })
        .collect()
}

fn no_stages(parent: &Path) {
    for entry in fs::read_dir(parent).unwrap() {
        let name = entry.unwrap().file_name();
        assert!(
            !name.to_string_lossy().starts_with(".cad-stage-"),
            "abandoned publication stage: {}",
            name.to_string_lossy()
        );
    }
}

#[test]
fn late_exchange_rejection_preserves_every_previous_byte_and_cleans_stage() {
    let root = tempfile::tempdir().unwrap();
    let recipe = root.path().join("parts.star");
    let out = root.path().join("published");
    let valid = source(24);
    fs::write(&recipe, &valid).unwrap();
    let built = success(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(built["outputs"].as_object().unwrap().len(), 24);
    let previous = tree(&out);
    assert!(previous.len() > 100);
    success(&["verify", out.to_str().unwrap()]);

    // This requirement is syntactically valid and reaches the exchange policy
    // gate only after every selected geometry export has been generated.
    fs::write(
        &recipe,
        format!(
            "{valid}\nrequire_exchange(id='late_reject', output='part_023', format='stl', aspect='geometry')\n"
        ),
    )
    .unwrap();
    for _ in 0..2 {
        let failed = run(&[
            "build",
            recipe.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ]);
        assert!(!failed.status.success());
        let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
        assert_eq!(error["code"], "exchange_requirement_failed", "{error}");
        assert_eq!(tree(&out), previous);
        no_stages(root.path());
        success(&["verify", out.to_str().unwrap()]);
    }

    let empty_out = root.path().join("never_published");
    let failed = run(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        empty_out.to_str().unwrap(),
    ]);
    assert!(!failed.status.success());
    assert!(!empty_out.exists());
    no_stages(root.path());
}

#[test]
fn shared_assembly_assets_are_unique_and_validate_after_publication() {
    let root = tempfile::tempdir().unwrap();
    let recipe = root.path().join("shared.star");
    let out = root.path().join("published");
    fs::write(
        &recipe,
        "body = extrude(id='body', profile=circle(id='profile',radius=2), height=6)\n\
         piece = part(id='piece', body=body)\n\
         pair = assembly(id='pair',instances=[instance(id='left',part=piece),instance(id='right',part=piece,offset=[12,0,0])])\n\
         single = assembly(id='single',instances=[instance(id='one',part=piece,offset=[0,8,0])])\n\
         assembly_output(name='pair',assembly=pair)\n\
         assembly_output(name='single',assembly=single)\n",
    )
    .unwrap();
    let built = success(&[
        "build",
        recipe.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(built["outputs"].as_object().unwrap().len(), 2);
    let pair: Value =
        serde_json::from_slice(&fs::read(out.join("pair.assembly.json")).unwrap()).unwrap();
    let single: Value =
        serde_json::from_slice(&fs::read(out.join("single.assembly.json")).unwrap()).unwrap();
    let pair_asset = pair["definitions"]["piece"]["geometry_asset"]
        .as_str()
        .unwrap();
    assert_eq!(single["definitions"]["piece"]["geometry_asset"], pair_asset);
    let publication = tree(&out);
    assert_eq!(
        publication
            .keys()
            .filter(|name| name.starts_with("definition-") && name.ends_with(".brep.json"))
            .count(),
        1
    );
    assert!(publication.contains_key(pair_asset));
    for assembly in ["pair", "single"] {
        success(&[
            "validate",
            out.join(format!("{assembly}.assembly.json"))
                .to_str()
                .unwrap(),
        ]);
    }
    success(&["verify", out.to_str().unwrap()]);
    no_stages(root.path());
}
