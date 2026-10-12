//! Bounded focused views over the public modeling catalog; logical references
//! are not inline object dependencies and remain IDs in the selected contract.
use cad_build::capabilities::{ArgumentSpec, ArgumentType};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn dependencies(value: &ArgumentType, pending: &mut Vec<&'static str>) {
    match value {
        ArgumentType::Object { type_id } => pending.push(type_id),
        ArgumentType::Array { items, .. }
        | ArgumentType::Map { values: items }
        | ArgumentType::Nullable { value: items } => dependencies(items, pending),
        ArgumentType::String
        | ArgumentType::Number
        | ArgumentType::Integer
        | ArgumentType::Boolean
        | ArgumentType::Reference { .. }
        | ArgumentType::Enum { .. } => {}
    }
}
fn arguments(args: &[ArgumentSpec], pending: &mut Vec<&'static str>) {
    for arg in args {
        dependencies(&arg.value_type, pending);
    }
}

pub fn report(operation: Option<&str>, object_type: Option<&str>) -> Result<Value, String> {
    let mut catalog = cad_build::capability_catalog();
    let mut pending = vec![];
    let mut domain_ids = BTreeSet::new();
    let scope = match (operation, object_type) {
        (None, None) => {
            let mut document = serde_json::to_value(catalog).map_err(|e| e.to_string())?;
            document["scope"] = json!({"kind":"full"});
            return Ok(document);
        }
        (Some(id), None) => {
            let op = catalog
                .operations
                .iter()
                .find(|o| o.id == id)
                .ok_or_else(|| {
                    format!(
                        "unknown capability operation '{id}'; use capabilities for available IDs"
                    )
                })?;
            arguments(&op.arguments, &mut pending);
            domain_ids.extend(op.domain_ids.iter().copied());
            json!({"kind":"operation","id":id})
        }
        (None, Some(id)) => {
            let spec = catalog
                .object_types
                .iter()
                .find(|o| o.id == id)
                .ok_or_else(|| {
                    format!(
                        "unknown capability object type '{id}'; use capabilities for available IDs"
                    )
                })?;
            pending.push(spec.id);
            json!({"kind":"object_type","id":id})
        }
        (Some(_), Some(_)) => return Err("choose either an operation or an object type".into()),
    };
    let objects: BTreeMap<_, _> = catalog.object_types.iter().map(|o| (o.id, o)).collect();
    let mut needed = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !needed.insert(id) {
            continue;
        }
        let spec = objects
            .get(id)
            .ok_or_else(|| format!("incomplete capability catalog: missing type '{id}'"))?;
        arguments(&spec.arguments, &mut pending);
        for variant in &spec.variants {
            arguments(&variant.arguments, &mut pending);
        }
    }
    catalog.operations.retain(|o| Some(o.id) == operation);
    catalog.domains.retain(|d| domain_ids.contains(d.id));
    catalog.object_types.retain(|o| needed.contains(o.id));
    catalog.object_types.sort_by_key(|o| o.id);
    let mut document = serde_json::to_value(catalog).map_err(|e| e.to_string())?;
    document["scope"] = scope;
    Ok(document)
}
