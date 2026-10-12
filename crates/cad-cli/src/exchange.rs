//! Bind exporter fidelity declarations to the actual published source and bytes.
use cad_export::{exchange_fidelity, ExchangeFormat, ExchangeSource};
use cad_kernel::Tolerance;
use serde_json::{json, Value};
use std::error::Error;

pub(crate) fn report(
    name: &str,
    source_content_hash: &str,
    summary: &Value,
    artifacts: &[(String, String)],
    source: ExchangeSource<'_>,
    tolerance: Tolerance,
) -> Result<String, Box<dyn Error>> {
    let mut exports = serde_json::Map::new();
    for (extension, format) in [("step", ExchangeFormat::Step), ("stl", ExchangeFormat::Stl)] {
        let filename = format!("{name}.{extension}");
        let content = &artifacts
            .iter()
            .find(|(file, _)| file == &filename)
            .ok_or_else(|| format!("missing generated exchange artifact '{filename}'"))?
            .1;
        exports.insert(
            extension.into(),
            json!({
                "file": filename,
                "algorithm": "blake3",
                "digest": blake3::hash(content.as_bytes()).to_hex().to_string(),
                "bytes": content.len(),
                "report": exchange_fidelity(source, format, tolerance)?,
            }),
        );
    }
    Ok(serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "output_name": name,
        "source_content_hash": source_content_hash,
        "recipe_source_hash": summary["recipe_source_hash"],
        "recipe_context_hash": summary["recipe_context_hash"],
        "artifacts": exports,
    }))?)
}
