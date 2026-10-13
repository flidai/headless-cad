//! Reject mismatched source navigation before embedding it in an inspector.
use crate::{PreviewError, RecipeContext};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn validate(context: &RecipeContext) -> Result<(), PreviewError> {
    let Some(report) = &context.source_provenance else {
        return Ok(());
    };
    let bad = || PreviewError::Recipe("invalid or mismatched source provenance");
    if context.language != "starlark"
        || report["schema_version"] != 1
        || report["kind"] != "source_provenance"
        || report["recipe_context_hash"] != context.recipe_context_hash
        || report["recipe_source_hash"] != blake3::hash(context.source.as_bytes()).to_hex().as_str()
        || report["source_bundle_hash"] != serde_json::to_value(&context.source_bundle_hash)?
    {
        return Err(bad());
    }
    let mut sources: BTreeMap<&str, &str> = context
        .modules
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    if sources
        .insert(&context.source_name, &context.source)
        .is_some()
    {
        return Err(bad());
    }
    let inventory = report["source_inventory"].as_object().ok_or_else(bad)?;
    if inventory.len() != sources.len() {
        return Err(bad());
    }
    for (name, source) in &sources {
        let entry = inventory.get(*name).ok_or_else(bad)?;
        if entry["bytes"].as_u64() != Some(source.len() as u64)
            || entry["blake3"] != blake3::hash(source.as_bytes()).to_hex().as_str()
            || entry["sha256"]
                != Sha256::digest(source.as_bytes())
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
        {
            return Err(bad());
        }
    }
    let mut positions: BTreeMap<String, BTreeMap<usize, (u64, u64)>> = BTreeMap::new();
    let mut span = |s: &Value| -> Result<(), PreviewError> {
        let source = sources
            .get(s["file"].as_str().ok_or_else(bad)?)
            .ok_or_else(bad)?;
        let index = |field: &str| {
            s[field]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(bad)
        };
        let start = index("start_byte")?;
        let end = index("end_byte")?;
        if start >= end
            || source.get(start..end).is_none()
            || ["start_line", "end_line", "start_column", "end_column"]
                .iter()
                .any(|key| s[*key].as_u64().is_none_or(|n| n == 0))
        {
            return Err(bad());
        }
        let file = s["file"].as_str().ok_or_else(bad)?.to_owned();
        let wanted = positions.entry(file).or_default();
        for (byte, line, column) in [
            (start, "start_line", "start_column"),
            (end, "end_line", "end_column"),
        ] {
            let location = (
                s[line].as_u64().ok_or_else(bad)?,
                s[column].as_u64().ok_or_else(bad)?,
            );
            if wanted
                .insert(byte, location)
                .is_some_and(|old| old != location)
            {
                return Err(bad());
            }
        }
        Ok(())
    };
    for key in ["features", "parts"] {
        let entries = report[key].as_object().ok_or_else(bad)?;
        if entries.len() > 10_000 {
            return Err(bad());
        }
        for entry in entries.values() {
            span(&entry["declaration"])?;
            let calls = entry["call_stack"].as_array().ok_or_else(bad)?;
            if calls.len() > 64 {
                return Err(bad());
            }
            for call in calls {
                span(call)?;
            }
        }
    }
    // Walk each captured source once across sorted requested offsets. Repeated
    // runtime IDs at the same call site do not repeatedly rescan source text.
    for (file, wanted) in positions {
        let source = sources[file.as_str()];
        let (mut offset, mut line, mut column) = (0, 1_u64, 1_u64);
        for (byte, expected) in wanted {
            for c in source[offset..byte].chars() {
                if c == '\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
            }
            if (line, column) != expected {
                return Err(bad());
            }
            offset = byte;
        }
    }
    let bodies = report["part_body_features"].as_object().ok_or_else(bad)?;
    for (part, feature) in bodies {
        if report["parts"].get(part).is_none()
            || feature
                .as_str()
                .is_none_or(|id| report["features"].get(id).is_none())
        {
            return Err(bad());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn context() -> RecipeContext {
        let source = "# α\np = part(id = \"p\", body = b)\n";
        let start = source.find("part(").unwrap();
        let end = source.trim_end().len();
        let span = json!({"file":"part.star","start_byte":start,"end_byte":end,
            "start_line":2,"start_column":5,"end_line":2,"end_column":source.lines().last().unwrap().chars().count()+1});
        let entry = json!({"declaration":span,"call_stack":[]});
        RecipeContext {
            language: "starlark".into(),
            source: source.into(),
            source_name: "part.star".into(),
            modules: BTreeMap::new(),
            source_bundle_hash: None,
            output_name: "part".into(),
            feature_id: "b".into(),
            parameters: BTreeMap::new(),
            content_hash: "body-hash".into(),
            recipe_context_hash: "context-hash".into(),
            property_assertions: BTreeMap::new(),
            native_engineering_checks: Default::default(),
            source_provenance: Some(json!({"schema_version":1,"kind":"source_provenance",
                "recipe_context_hash":"context-hash","recipe_source_hash":blake3::hash(source.as_bytes()).to_hex().to_string(),
                "source_bundle_hash":null,
                "source_inventory":{"part.star":{"bytes":source.len(),"blake3":blake3::hash(source.as_bytes()).to_hex().to_string(),"sha256":Sha256::digest(source.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>()}},
                "features":{"b":entry},"parts":{"p":entry},"part_body_features":{"p":"b"}})),
        }
    }
    #[test]
    fn refuses_stale_source_and_cross_generation_navigation() {
        let valid = context();
        validate(&valid).unwrap();
        let mut changed = valid.clone();
        changed.source.push(' ');
        assert!(validate(&changed).is_err());
        let mut changed = valid.clone();
        changed.recipe_context_hash = "another-generation".into();
        assert!(validate(&changed).is_err());
        let mut changed = valid.clone();
        changed.source_bundle_hash = Some("other-bundle".into());
        assert!(validate(&changed).is_err());
        let mut changed = valid.clone();
        changed.source_provenance.as_mut().unwrap()["part_body_features"]["p"] = json!("missing");
        assert!(validate(&changed).is_err());
        let mut changed = valid;
        changed.source_provenance.as_mut().unwrap()["source_inventory"]["part.star"]["blake3"] =
            json!("wrong");
        assert!(validate(&changed).is_err());
    }
    #[test]
    fn refuses_spans_outside_captured_utf8_sources() {
        let valid = context();
        for (field, value) in [
            ("file", json!("uncaptured.star")),
            ("start_byte", json!(3)),
            ("end_byte", json!(1000)),
            ("end_byte", json!(0)),
            ("start_line", json!(0)),
            ("start_column", json!(6)),
        ] {
            let mut bad = valid.clone();
            bad.source_provenance.as_mut().unwrap()["parts"]["p"]["declaration"][field] = value;
            assert!(validate(&bad).is_err(), "{field}");
        }
    }
}
