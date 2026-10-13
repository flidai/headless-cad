//! Read-only publication of source-bound local planar seating witnesses.
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
    if build.nominal_seating_checks.is_empty() {
        return Ok(());
    }
    let report = json!({"schema_version":1,"kind":"native_nominal_planar_seating","units":"mm",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),
        "checks":build.nominal_seating_checks,
        "semantics":"Local opposed native planar supports coincide within the reported arithmetic uncertainty. A protected disk is certified inside both projected face trims; their planes coincide only within the reported uncertainty. Only named faces at the nominal pose are checked. This does not prove whole-body noninterference, exact physical contact, contact forces, preload, threads, or manufactured-part conformance."});
    let data = serde_json::to_string_pretty(&report)?;
    let source = if artifacts.contains("recipe.bundle.json") {
        "recipe.bundle.json"
    } else {
        summary["recipe_source"]["artifact"]
            .as_str()
            .ok_or("missing captured seating source")?
    };
    let mut html = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Nominal seating · Headless CAD</title><style>body{font:16px system-ui;margin:0;padding:32px;background:#101a28;color:#e6edf5}main{max-width:1200px;margin:auto}a{color:#91d1ff}table{border-collapse:collapse;width:100%;margin:24px 0}td,th{text-align:left;padding:12px;border-bottom:1px solid #40546a}section{overflow-x:auto}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style></head><body><main><h1>Native nominal seating</h1><p>Source-selected planar faces with opposed normals, arithmetic-level coincidence, and a protected disk shared by their projected material trims. All lengths are millimetres.</p><p>These local nominal witnesses do not certify whole-body noninterference, forces, preload, threads, or manufactured-part conformance.</p><nav><a href=\"nominal-seating-checks.json\">Report JSON</a> · ");
    html.push_str(&format!("<a href=\"{}\">Captured recipe</a></nav><section><table><thead><tr><th>Check</th><th>Faces</th><th>Status</th><th>Plane gap interval</th><th>Protected disk radius ≥</th><th>Required radius ≥</th></tr></thead><tbody>", escape(source)));
    for (id, check) in &build.nominal_seating_checks {
        html.push_str(&format!("<tr><td>{}</td><td>{} / {}<br>{} / {}</td><td>{}</td><td>[{}, {}]</td><td>{}</td><td>{}</td></tr>",
            escape(id), escape(&check.left.instance_path.join(" / ")), escape(&check.left.datum),
            escape(&check.right.instance_path.join(" / ")), escape(&check.right.datum), escape(&check.status),
            check.signed_gap_lower_mm, check.signed_gap_upper_mm,
            check.disk_radius_lower_mm, check.declaration.min_shared_disk_radius_mm));
    }
    html.push_str("</tbody></table></section><nav>");
    for output in build
        .nominal_seating_checks
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
    artifacts.push(("nominal-seating-checks.json".into(), data))?;
    artifacts.push(("nominal-seating-checks.html".into(), html))?;
    Ok(())
}
