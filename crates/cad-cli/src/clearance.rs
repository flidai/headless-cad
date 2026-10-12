//! Static inspection of source-bound native convex clearance certificates.
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
    artifacts: &mut Vec<(String, String)>,
) -> Result<(), Box<dyn Error>> {
    if build.clearance_checks.is_empty() {
        return Ok(());
    }
    let report = json!({"schema_version":1,"kind":"native_convex_geometric_clearance","units":"mm",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),
        "checks":build.clearance_checks,
        "semantics":"Minimum Euclidean separation of two declared convex planar solid occurrences. Native supporting-plane lower bound and feasible-witness upper bound, widened for placement arithmetic. Not a whole-assembly or continuous-motion collision check; no manufacturing tolerance inflation."});
    let data = serde_json::to_string_pretty(&report)?;
    let source_artifact = if artifacts
        .iter()
        .any(|(name, _)| name == "recipe.bundle.json")
    {
        "recipe.bundle.json"
    } else {
        summary["recipe_source"]["artifact"]
            .as_str()
            .ok_or("missing captured clearance source")?
    };
    let mut html=String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Geometric clearance · Headless CAD</title><style>body{font:16px system-ui;margin:0;padding:32px;background:#101a28;color:#e6edf5}main{max-width:1200px;margin:auto}a{color:#91d1ff}table{border-collapse:collapse;width:100%;margin:24px 0}td,th{text-align:left;padding:12px;border-bottom:1px solid #40546a}section{overflow-x:auto}pre{font:13px monospace;white-space:pre-wrap;overflow-wrap:anywhere}small{overflow-wrap:anywhere}</style></head><body><main><h1>Native geometric clearance</h1><p>Minimum separation of source-selected convex planar solid occurrences. All distances are millimetres.</p><p>Only the declared pairs are checked. These results do not certify an entire assembly, continuous motion or manufactured tolerance envelopes.</p><nav><a href=\"clearance-checks.json\">Report JSON</a> · <a href=\"recipe.bundle.json\">Captured recipe bundle</a></nav><section><table><thead><tr><th>Check</th><th>Occurrences</th><th>Status</th><th>Distance interval</th><th>Required minimum</th></tr></thead><tbody>");
    html = html.replace(
        "href=\"recipe.bundle.json\">Captured recipe bundle",
        &format!("href=\"{}\">Captured recipe", escape(source_artifact)),
    );
    for (id, check) in &build.clearance_checks {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}<br>{}</td><td>{}</td><td>[{}, {}]</td><td>{}</td></tr>",
            escape(id),
            escape(&check.left.instance_path.join(" / ")),
            escape(&check.right.instance_path.join(" / ")),
            escape(&check.status),
            check.lower_bound_mm,
            check.upper_bound_mm,
            check.declaration.min_clearance
        ));
    }
    html.push_str("</tbody></table></section><nav>");
    for output in build
        .clearance_checks
        .values()
        .map(|check| &check.declaration.output)
        .collect::<BTreeSet<_>>()
    {
        if artifacts
            .iter()
            .any(|(name, _)| name == &format!("{output}.preview.html"))
        {
            html.push_str(&format!(
                "<a href=\"{}.preview.html\">Inspect {}</a> ",
                escape(output),
                escape(output)
            ));
        }
    }
    html.push_str("</nav><p><small>Source context: ");
    html.push_str(&escape(
        summary["recipe_context_hash"]
            .as_str()
            .ok_or("missing clearance source context")?,
    ));
    html.push_str("</small></p><details><summary>Native witnesses, occurrence frames and bounds</summary><pre>");
    html.push_str(&escape(&data));
    html.push_str("</pre></details></main></body></html>");
    artifacts.push(("clearance-checks.json".into(), data));
    artifacts.push(("clearance-checks.html".into(), html));
    Ok(())
}
