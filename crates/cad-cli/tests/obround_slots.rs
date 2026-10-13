use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn inventory(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

#[test]
fn slotted_plate_exports_native_surfaces_and_replays_after_failed_edits() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("obround_slot_plate.star");
    let original = include_str!("../../../examples/obround_slot_plate.star");
    fs::write(&source, original).unwrap();
    let out = temp.path().join("out");
    let run = |input: &Path, directory: &Path| {
        Command::new(env!("CARGO_BIN_EXE_cad"))
            .arg("build")
            .arg(input)
            .arg("--out-dir")
            .arg(directory)
            .arg("--cache-dir")
            .arg(temp.path().join("cache"))
            .output()
            .unwrap()
    };
    let built = run(&source, &out);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let report: Value = serde_json::from_slice(&built.stdout).unwrap();
    let area =
        80. * 50. - 25. * std::f64::consts::PI - 2. * 9. * std::f64::consts::PI - 6. * (8. + 6.);
    assert!(
        (report["outputs"]["slotted_plate"]["volume_mm3"]
            .as_f64()
            .unwrap()
            - area * 6.)
            .abs()
            < 1e-7
    );
    for id in ["left_slot_wall", "right_slot_wall", "stock_cap_survives"] {
        assert_eq!(report["face_assertions"][id]["status"], "resolved");
    }
    let step = fs::read_to_string(out.join("slotted_plate.step")).unwrap();
    assert_eq!(step.matches("CYLINDRICAL_SURFACE(").count(), 5);
    let baseline = inventory(&out);
    for (from, to) in [
        ("frame_origin=[0,0,0]", "frame_origin=[0,0,0.01]"),
        ("\"center\":[15,12]", "\"center\":[2,12]"),
        ("\"direction\":[1,0]", "\"direction\":[2,0]"),
    ] {
        assert!(original.contains(from));
        fs::write(&source, original.replace(from, to)).unwrap();
        let rejected = run(&source, &out);
        assert!(!rejected.status.success(), "accepted {to}");
        assert_eq!(inventory(&out), baseline);
    }
    let offline = temp.path().join("offline");
    fs::create_dir(&offline).unwrap();
    let captured = offline.join("obround_slot_plate.star");
    fs::copy(out.join("recipe.star"), &captured).unwrap();
    fs::remove_file(&source).unwrap();
    let rebuilt = offline.join("rebuilt");
    let replay = run(&captured, &rebuilt);
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(
        fs::read(rebuilt.join("slotted_plate.step")).unwrap(),
        step.as_bytes()
    );
    let verify = Command::new(env!("CARGO_BIN_EXE_cad"))
        .arg("verify")
        .arg(rebuilt)
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
}
