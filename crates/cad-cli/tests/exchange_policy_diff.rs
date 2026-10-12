use serde_json::Value;
use std::{fs, process::Command};
const SOURCE: &str = "b=extrude(id='b',profile=rectangle(id='p',width=2,depth=3),height=4)\noutput(name='body',feature=b)\nassembly_output(name='machine',assembly=assembly(id='root',instances=[instance(id='piece',part=part(id='piece',body=b))]))\n";
fn cli(args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_cad"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn policy_only_add_remove_and_change_are_visible_without_geometry_changes() {
    let temp = tempfile::tempdir().unwrap();
    let before = temp.path().join("before.star");
    let after = temp.path().join("after.star");
    // Assembly keeps both revisions in schema 2 even when the policy is empty.
    let step = "require_exchange(id='policy',output='body',format='step',aspect='units')\n";
    let stl = "require_exchange(id='policy',output='body',format='stl',aspect='units')\n";
    for (old, new, status) in [
        ("", step, "added"),
        (step, "", "removed"),
        (step, stl, "changed"),
        (step, step, "unchanged"),
    ] {
        fs::write(&before, format!("{SOURCE}{old}")).unwrap();
        fs::write(&after, format!("{SOURCE}{new}")).unwrap();
        let result = cli(&["diff", before.to_str().unwrap(), after.to_str().unwrap()]);
        assert_eq!(result["outputs"]["body"]["status"], "unchanged");
        assert_eq!(result["outputs"]["machine"]["status"], "unchanged");
        let change = &result["exchange_requirements"]["policy"];
        assert_eq!(change["status"], status);
        for (side, declaration) in [("before", old), ("after", new)] {
            if declaration.is_empty() {
                assert!(change[side].is_null());
                assert_eq!(result["exchange_policy_status"][side], "not_requested");
            } else {
                assert_eq!(change[side]["status"], "pending_publication");
                assert_eq!(
                    change[side]["requirement"]["format"],
                    if declaration == stl { "stl" } else { "step" }
                );
                assert!(change[side].get("report_digest").is_none());
            }
        }
    }
}
#[test]
fn bom_and_selection_expose_pending_publication_obligations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.star");
    fs::write(
        &source,
        format!(
            "{SOURCE}require_exchange(id='policy',output='body',format='stl',aspect='units')\n"
        ),
    )
    .unwrap();
    let path = source.to_str().unwrap();
    for args in [
        vec!["bom", path, "--assembly", "machine"],
        vec![
            "select", path, "--from", "b", "--role", "end_cap", "--in", "b",
        ],
    ] {
        let report = cli(&args);
        assert_eq!(report["exchange_policy_status"], "pending_publication");
        assert_eq!(
            report["exchange_requirements"]["policy"]["status"],
            "pending_publication"
        );
        assert_eq!(
            report["exchange_requirements"]["policy"]["requirement"]["format"],
            "stl"
        );
        assert!(report["exchange_requirements"]["policy"]
            .get("report_digest")
            .is_none());
    }
}
