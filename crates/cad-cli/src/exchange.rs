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
        "source_bundle_hash": summary["source_bundle_hash"],
        "recipe_context_hash": summary["recipe_context_hash"],
        "artifacts": exports,
    }))?)
}

#[derive(Debug)]
pub(crate) struct RequirementFailure {
    pub requirement: cad_build::ExchangeRequirement,
    pub reason: String,
    pub observed_status: Option<String>,
    pub source_count: Option<u64>,
}
impl std::fmt::Display for RequirementFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "exchange requirement '{}' failed: {}",
            self.requirement.id, self.reason
        )
    }
}
impl Error for RequirementFailure {}

pub(crate) fn pending(requirements: &[cad_build::ExchangeRequirement]) -> Value {
    Value::Object(
        requirements
            .iter()
            .map(|r| {
                (
                    r.id.clone(),
                    json!({
                        "requirement": r, "status": "pending_publication"
                    }),
                )
            })
            .collect(),
    )
}

/// Publication obligations are global, even when a caller selects fewer outputs.
/// Inspect/check cannot evaluate export fidelity and leave these explicitly pending.
pub(crate) fn enforce(
    requirements: &[cad_build::ExchangeRequirement],
    artifacts: &[(String, String)],
    display_only: bool,
) -> Result<Value, Box<dyn Error>> {
    let mut results = serde_json::Map::new();
    for requirement in requirements {
        let fail = |reason: &str, observed_status: Option<String>, source_count: Option<u64>| {
            Box::new(RequirementFailure {
                requirement: requirement.clone(),
                reason: reason.into(),
                observed_status,
                source_count,
            }) as Box<dyn Error>
        };
        if display_only {
            return Err(fail(
                "display-only publication cannot satisfy exchange requirements",
                None,
                None,
            ));
        }
        let filename = format!("{}.exchange.json", requirement.output);
        let content = &artifacts
            .iter()
            .find(|(name, _)| name == &filename)
            .ok_or_else(|| {
                fail(
                    "required output was not selected for full publication",
                    None,
                    None,
                )
            })?
            .1;
        let report: Value = serde_json::from_str(content)?;
        let entries = report["artifacts"][&requirement.format]["report"]["entries"]
            .as_array()
            .ok_or_else(|| fail("required export report is unavailable", None, None))?;
        let entry = entries
            .iter()
            .find(|e| e["aspect"] == requirement.aspect)
            .ok_or_else(|| fail("required aspect has no fidelity declaration", None, None))?;
        let status = entry["status"].as_str();
        let count = entry["source_count"].as_u64();
        if status != Some("supported") || !count.is_some_and(|n| n > 0) {
            return Err(fail(
                "required aspect must be present in source and supported by the exporter",
                status.map(str::to_owned),
                count,
            ));
        }
        results.insert(
            requirement.id.clone(),
            json!({
                "requirement": requirement,
                "status": "passed",
                "report_file": filename,
                "report_digest": blake3::hash(content.as_bytes()).to_hex().to_string(),
                "source_content_hash": report["source_content_hash"],
                "observed_status": status,
                "source_count": count,
            }),
        );
    }
    Ok(Value::Object(results))
}
