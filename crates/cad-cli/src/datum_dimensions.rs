//! Source-bound native directed plane-distance measurements.
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
    if build.datum_dimensions.is_empty() {
        return Ok(());
    }
    let report = json!({"schema_version":1,"kind":"native_directed_datum_dimensions","units":"mm",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),
        "dimensions":build.datum_dimensions,
        "semantics":"Directed separation of parallel native planar face supports, resolved through semantic datums and exact occurrence paths. Numerical bounds describe measurement arithmetic; authored size tolerances are separate design requirements. Not general closest-surface clearance, contact, GD&T or manufactured-part conformance."});
    let data = serde_json::to_string_pretty(&report)?;
    let source = if artifacts
        .iter()
        .any(|(name, _)| name == "recipe.bundle.json")
    {
        "recipe.bundle.json"
    } else {
        summary["recipe_source"]["artifact"]
            .as_str()
            .ok_or("missing captured dimension source")?
    };
    let mut html = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Axial dimensions · Headless CAD</title><style>body{font:16px system-ui;margin:0;padding:32px;background:#101a28;color:#e6edf5}main{max-width:1200px;margin:auto}a{color:#91d1ff}table{border-collapse:collapse;width:100%;margin:24px 0}td,th{text-align:left;padding:12px;border-bottom:1px solid #40546a}section{overflow-x:auto}pre{font:13px monospace;white-space:pre-wrap;overflow-wrap:anywhere}</style></head><body><main><h1>Native axial dimensions</h1><p>Directed distances between source-selected parallel planar face datums. All values are millimetres. Display values are rounded; JSON retains full precision.</p><p>Numerical bounds describe measurement arithmetic. Authored size tolerances describe design requirements; these are not measurements of manufactured parts.</p><nav><a href=\"datum-dimensions.json\">Report JSON</a> · ");
    html.push_str(&format!(
        "<a href=\"{}\">Captured recipe</a>",
        escape(source)
    ));
    if !build.tolerance_stacks.is_empty() {
        html.push_str(" · <a href=\"tolerance-stacks.html\">Dimensional stacks</a>");
    }
    html.push_str("</nav><section><table><thead><tr><th>Dimension</th><th>From datum</th><th>To datum</th><th>Status</th><th>Measured</th><th>Numerical interval</th></tr></thead><tbody>");
    for (id, dimension) in &build.datum_dimensions {
        html.push_str(&format!("<tr><td>{}</td><td>{} / {}</td><td>{} / {}</td><td>{}</td><td>≈ {}</td><td>{}</td></tr>",
            escape(id),escape(&dimension.from.instance_path.join(" / ")),escape(&dimension.from.datum),
            escape(&dimension.to.instance_path.join(" / ")),escape(&dimension.to.datum),escape(&dimension.status),
            report_numbers::number(dimension.value_mm),report_numbers::measured(dimension.lower_bound_mm,dimension.upper_bound_mm)));
    }
    html.push_str("</tbody></table></section><nav>");
    for output in build
        .datum_dimensions
        .values()
        .map(|d| &d.declaration.output)
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
    html.push_str("</nav><details><summary>Native faces, occurrence frames and source evidence</summary><pre>");
    html.push_str(&escape(&data));
    html.push_str("</pre></details></main></body></html>");
    artifacts.push(("datum-dimensions.json".into(), data));
    artifacts.push(("datum-dimensions.html".into(), html));
    Ok(())
}
