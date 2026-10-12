//! Static inspection of source-bound native clearance certificates.
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
    let report = json!({"schema_version":3,"kind":"native_geometric_clearance","units":"mm",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),
        "checks":build.clearance_checks,
        "semantics":"Each check covers either its initial pose or the entire declared right-local translation or fixed-axis rotation over time 0..1. Top-level bounds enclose the minimum distance over that scope; sweep intervals provide continuous Lipschitz certificates, not sample-only claims. Query retains initial-pose witnesses; sweep samples have their own times, native methods and motion uncertainty. Bounds include native, placement and path arithmetic. Witness axes need not be global separating planes. Only declared pairs and motions are checked; no manufacturing tolerance inflation."});
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
    let mut html=String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Geometric clearance · Headless CAD</title><style>body{font:16px system-ui;margin:0;padding:32px;background:#101a28;color:#e6edf5}main{max-width:1200px;margin:auto}a{color:#91d1ff}table{border-collapse:collapse;width:100%;margin:24px 0}td,th{text-align:left;padding:12px;border-bottom:1px solid #40546a}section{overflow-x:auto}pre{font:13px monospace;white-space:pre-wrap;overflow-wrap:anywhere}small{overflow-wrap:anywhere}</style></head><body><main><h1>Native geometric clearance</h1><p>Minimum separation of source-selected native solid occurrences. Each row identifies an initial pose or a complete declared translation or rotation. All distances are millimetres.</p><p>Continuous-motion results bound the entire declared path. Only the named pairs and prescribed motions are checked; other moving parts and manufactured tolerance envelopes need separate checks.</p><nav><a href=\"clearance-checks.json\">Report JSON</a> · <a href=\"recipe.bundle.json\">Captured recipe bundle</a></nav><section><table><thead><tr><th>Check</th><th>Occurrences</th><th>Status</th><th>Distance interval</th><th>Required minimum</th><th>Geometry</th><th>Scope</th></tr></thead><tbody>");
    html = html.replace(
        "href=\"recipe.bundle.json\">Captured recipe bundle",
        &format!("href=\"{}\">Captured recipe", escape(source_artifact)),
    );
    for (id, check) in &build.clearance_checks {
        let method = match check.query.method {
            cad_kernel::ClearanceMethod::ConvexSupport => "Convex",
            cad_kernel::ClearanceMethod::CoaxialMeridian => "Coaxial",
            cad_kernel::ClearanceMethod::ParallelAnnular => "Parallel annuli",
            cad_kernel::ClearanceMethod::NativeTrimWitness => "Native trimmed surfaces",
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}<br>{}</td><td>{}</td><td>[{}, {}]</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(id),
            escape(&check.left.instance_path.join(" / ")),
            escape(&check.right.instance_path.join(" / ")),
            escape(&check.status),
            check.lower_bound_mm,
            check.upper_bound_mm,
            check.declaration.min_clearance,
            method,
            if check.rotation_sweep.is_some() { "Continuous rotation" } else if check.sweep.is_some() { "Continuous translation" } else { "Initial pose" }
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
