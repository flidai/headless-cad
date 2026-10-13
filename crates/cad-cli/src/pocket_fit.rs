//! Read-only source-bound capsule pocket fit reports.
use super::*;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn publish(
    build: &BuildResult,
    summary: &Value,
    artifacts: &mut ArtifactSink,
) -> Result<(), Box<dyn Error>> {
    if build.pocket_fit_checks.is_empty() {
        return Ok(());
    }
    let semantics = "The complete capsule insert footprint has a certified positive clearance from the selected native pocket's four physical sidewalls, with finite axial wall overlap and a guarded floor gap. Floor seating is checked separately. Arithmetic-level floor coincidence does not establish exact mathematical nonpenetration, retention, loads or manufactured-part conformance.";
    let report = json!({"schema_version":1,"kind":"native_capsule_pocket_fit","units":"mm",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),
        "checks":build.pocket_fit_checks,"semantics":semantics});
    let data = serde_json::to_string_pretty(&report)?;
    let source = if artifacts.contains("recipe.bundle.json") {
        "recipe.bundle.json"
    } else {
        summary["recipe_source"]["artifact"]
            .as_str()
            .ok_or("missing captured pocket fit source")?
    };
    let mut html = format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Pocket fit · Headless CAD</title><style>body{{font:16px system-ui;margin:0;padding:32px;background:#101a28;color:#e6edf5}}main{{max-width:1200px;margin:auto}}a{{color:#91d1ff}}table{{border-collapse:collapse;width:100%;margin:24px 0}}td,th{{text-align:left;padding:12px;border-bottom:1px solid #40546a}}section{{overflow-x:auto}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}</style></head><body><main><h1>Native pocket fit</h1><p>{}</p><p>All lengths are millimetres.</p><nav><a href=\"pocket-fit-checks.json\">Report JSON</a> · <a href=\"{}\">Captured recipe</a></nav><section><table><thead><tr><th>Check</th><th>Status</th><th>Side clearance interval</th><th>Required clearance ≥</th></tr></thead><tbody>", escape(semantics), escape(source));
    for (id, check) in &build.pocket_fit_checks {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>[{}, {}]</td><td>{}</td></tr>",
            escape(id),
            escape(&check.status),
            check.side_clearance_lower_mm,
            check.side_clearance_upper_mm,
            check.declaration.min_side_clearance_mm
        ));
    }
    html.push_str("</tbody></table></section><nav>");
    for output in build
        .pocket_fit_checks
        .values()
        .map(|c| &c.declaration.output)
        .collect::<BTreeSet<_>>()
    {
        if artifacts.contains(&format!("{output}.preview.html")) {
            html.push_str(&format!(
                "<a href=\"{}.preview.html\">Inspect {}</a> ",
                escape(output),
                escape(output)
            ));
        }
    }
    html.push_str(
        "</nav><details><summary>Native faces, bounds and captured source evidence</summary><pre>",
    );
    html.push_str(&escape(&data));
    html.push_str("</pre></details></main></body></html>");
    artifacts.push(("pocket-fit-checks.json".into(), data))?;
    artifacts.push(("pocket-fit-checks.html".into(), html))?;
    Ok(())
}
