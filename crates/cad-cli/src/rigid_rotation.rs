//! Read-only inspection of prescribed rigid groups and named-pair certificates.
use super::*;

// Each inspector embeds one staged preview; bound that required re-read.
const MAX_PREVIEW_READ_BYTES: u64 = 256 * 1024 * 1024;

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
    artifacts: &mut artifact_sink::ArtifactSink,
) -> Result<(), Box<dyn Error>> {
    if build.rigid_rotation_groups.is_empty() {
        return Ok(());
    }
    let clearance = artifacts
        .get("clearance-checks.json")
        .ok_or("missing rigid group clearance evidence")?;
    let report = json!({"schema_version":1,"kind":"prescribed_rigid_rotation_groups","units":"mm","angle_units":"rad",
        "recipe_context_hash":summary["recipe_context_hash"],"recipe_source":summary["recipe_source"],
        "kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),
        "clearance_report":{"file":clearance.name,"algorithm":"blake3","digest":clearance.digest},
        "groups":build.rigid_rotation_groups,
        "semantics":"One prescribed world rotation is applied to all listed occurrences. Native continuous certificates cover only explicitly named clearance pairs. Display phase inspection is illustrative and does not compute geometry, solve constraints or certify unlisted contacts, loads or installation."});
    let mut html=String::from("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Rigid rotation · Headless CAD</title><style>body{font:16px system-ui;background:#101a28;color:#e6edf5;max-width:1100px;margin:40px auto;padding:0 24px;line-height:1.6}a{color:#91d1ff}section{border-top:1px solid #40546a;margin-top:24px}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style><h1>Prescribed rigid rotation</h1><p>Each group follows one source-authored rotation. Native certificates cover named clearance pairs over the declared path. Phase inspection only moves the display; it makes no modeling calls.</p><nav><a href=\"rigid-rotation-groups.json\">Source-bound group evidence</a> · <a href=\"clearance-checks.html\">Continuous clearance certificates</a></nav>");
    for (id, group) in &build.rigid_rotation_groups {
        let output = &group.declaration.output;
        let inspector = format!("rotation-{id}.html");
        filename(&format!("rotation-{id}"))?;
        html.push_str(&format!("<section><h2>{}</h2><p>{} moving occurrences · {} named checks · {:.1}°</p><p>Status: {}. Coverage: named pairs only.</p>",escape(id),group.members.len(),group.clearance_check_ids.len(),group.rotation_world.angle_radians.to_degrees(),escape(&group.status)));
        let preview_name = format!("{output}.preview.html");
        if artifacts.contains(&preview_name) {
            let preview = artifacts.read_text(&preview_name, MAX_PREVIEW_READ_BYTES)?;
            let embedded = serde_json::to_string(
                &json!({"group":group,"recipe_context_hash":summary["recipe_context_hash"]}),
            )?
            .replace('<', "\\u003c");
            let marker=format!("<script id=\"cad-rigid-rotation-data\" type=\"application/json\">{embedded}</script>");
            let mut document = preview.replacen(
                "<script id=\"cad-recipe-data\"",
                &format!("{marker}\n<script id=\"cad-recipe-data\""),
                1,
            );
            document=document.replacen("<section id=\"recipe-panel\"",concat!(
                "<section id=\"rotation-inspection\" class=\"section\"><h2>Source rotation</h2>",
                "<p id=\"rotation-summary\" class=\"muted\"></p><label for=\"rotation-phase\">Display phase <output id=\"rotation-angle\">0°</output></label>",
                "<input id=\"rotation-phase\" type=\"range\" min=\"0\" max=\"1\" step=\"0.0025\" value=\"0\" style=\"width:100%\" aria-label=\"Read-only rotation phase\">",
                "<p><button id=\"rotation-reset\">Initial pose</button></p><p class=\"muted\">Illustrative display phase. Native proof covers named pairs over the entire source path. Source, dimensions and exports retain their authored pose.</p>",
                "<p><a href=\"rigid-rotation-groups.html\">Group evidence</a> · <a href=\"clearance-checks.html\">Clearance proofs</a></p></section><section id=\"recipe-panel\""),1);
            document = document.replace(
                "</html>",
                &format!(
                    "<script>{}</script>\n</html>",
                    include_str!("rotation_inspector.js")
                ),
            );
            artifacts.push((inspector.clone(), document))?;
            html.push_str(&format!(
                "<p><a href=\"{}\">Inspect the rotating group and captured recipe</a></p>",
                escape(&inspector)
            ));
        }
        html.push_str("</section>");
    }
    let data = serde_json::to_string_pretty(&report)?;
    html.push_str("<details><summary>Source identities, members and path</summary><pre>");
    html.push_str(&escape(&data));
    html.push_str("</pre></details></html>");
    artifacts.push(("rigid-rotation-groups.json".into(), data))?;
    artifacts.push(("rigid-rotation-groups.html".into(), html))?;
    Ok(())
}
