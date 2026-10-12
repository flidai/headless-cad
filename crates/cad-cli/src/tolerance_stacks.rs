//! Read-only, source-bound worst-case dimensional stack reports.
use super::*;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
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
    if build.tolerance_stacks.is_empty() {
        return Ok(());
    }
    let mut sections = BTreeMap::new();
    for stack in build.tolerance_stacks.values() {
        for term in &stack.declaration.terms {
            let filename = format!("section-{}.json", term.view);
            let bytes = &artifacts
                .iter()
                .find(|(name, _)| name == &filename)
                .ok_or("missing stack section report")?
                .1;
            sections.insert(term.view.clone(), json!({"file":filename,"algorithm":"blake3","digest":blake3::hash(bytes.as_bytes()).to_hex().to_string()}));
        }
    }
    let report = json!({"schema_version":1,"kind":"worst_case_linear_dimensional_stacks","units":"mm",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "executor_build_id":cad_build::executor_build_id(),"section_reports":sections,"stacks":build.tolerance_stacks,
        "semantics":"Independent bounded size intervals combined with signed coefficients and outward-rounded binary64 arithmetic. Whole interval must satisfy source requirements. Not alignment, collision clearance, fit classification, RSS, GD&T or manufactured-part conformance."});
    let json = serde_json::to_string_pretty(&report)?;
    let source_artifact = if artifacts
        .iter()
        .any(|(name, _)| name == "recipe.bundle.json")
    {
        "recipe.bundle.json"
    } else {
        summary["recipe_source"]["artifact"]
            .as_str()
            .ok_or("missing captured stack source")?
    };
    let mut html = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Dimensional stacks · Headless CAD</title><style>body{font:16px system-ui;margin:0;padding:32px;background:#101a28;color:#e6edf5}main{max-width:1100px;margin:auto}a{color:#91d1ff}table{width:100%;border-collapse:collapse;margin:24px 0}th,td{text-align:left;padding:12px;border-bottom:1px solid #40546a}code,pre{font:13px monospace}pre{white-space:pre-wrap;overflow-wrap:anywhere}section{overflow-x:auto}small{overflow-wrap:anywhere}</style></head><body><main><h1>Worst-case dimensional stacks</h1><p>Source-authored size requirements evaluated against native dimensions. All values are millimetres.</p><p>Independent size intervals; this report does not establish alignment, collision clearance, fit class or manufactured-part conformance.</p><nav><a href=\"tolerance-stacks.json\">Report JSON</a> · <a href=\"recipe.bundle.json\">Captured recipe bundle</a></nav><section><table><thead><tr><th>Stack</th><th>Status</th><th>Nominal</th><th>Worst-case interval</th><th>Required interval</th></tr></thead><tbody>");
    html = html.replace(
        "href=\"recipe.bundle.json\">Captured recipe bundle",
        &format!("href=\"{}\">Captured recipe", escape(source_artifact)),
    );
    for (id, stack) in &build.tolerance_stacks {
        let lower = stack
            .declaration
            .min
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unbounded".into());
        let upper = stack
            .declaration
            .max
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unbounded".into());
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>[{}, {}]</td><td>[{}, {}]</td></tr>",
            escape(id),
            escape(&stack.status),
            stack.nominal_mm,
            stack.lower_limit_mm,
            stack.upper_limit_mm,
            lower,
            upper
        ));
    }
    html.push_str("</tbody></table></section><p><small>Source context: ");
    html.push_str(&escape(
        summary["recipe_context_hash"]
            .as_str()
            .ok_or("missing stack source context")?,
    ));
    html.push_str("</small></p><details><summary>Terms, source bindings and numerical evidence</summary><pre>");
    html.push_str(&escape(&json));
    html.push_str("</pre></details></main></body></html>");
    artifacts.push(("tolerance-stacks.json".into(), json));
    artifacts.push(("tolerance-stacks.html".into(), html));
    Ok(())
}
