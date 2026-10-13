//! Display summaries derived from native reports, bound to current occurrences.
use super::*;
use cad_preview::NativeEngineeringCheck;
type Checks = BTreeMap<String, NativeEngineeringCheck>;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Box<dyn Error>> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("native engineering report missing {key}").into())
}

fn witness(
    assembly: &cad_assembly::BuiltAssembly,
    value: &Value,
) -> Result<Vec<String>, Box<dyn Error>> {
    let id = text(value, "instance_id")?;
    let path: Vec<String> = serde_json::from_value(value["instance_path"].clone())?;
    let instance = assembly
        .instances
        .iter()
        .find(|i| i.id == id)
        .ok_or("engineering report occurrence is absent from current assembly")?;
    if assembly.instance_paths.get(id) != Some(&path)
        || instance.part != text(value, "part_id")?
        || assembly
            .parts
            .get(&instance.part)
            .map(|p| p.content_hash.as_str())
            != Some(text(value, "part_content_hash")?)
    {
        return Err("engineering report occurrence identity is stale".into());
    }
    let frame_key = if value.get("occurrence_frame").is_some() {
        "occurrence_frame"
    } else {
        "frame"
    };
    if value[frame_key] != serde_json::to_value(instance.transform)? {
        return Err("engineering report occurrence placement is stale".into());
    }
    Ok(path)
}

pub(super) fn collect(build: &BuildResult) -> Result<BTreeMap<String, Checks>, Box<dyn Error>> {
    let mut outputs: BTreeMap<String, Checks> = BTreeMap::new();
    let groups = [
        (
            "datum_dimension",
            "datum-dimensions.json",
            serde_json::to_value(&build.datum_dimensions)?,
            ["from", "to"],
            &["value_mm", "lower_bound_mm", "upper_bound_mm"][..],
        ),
        (
            "nominal_seating",
            "nominal-seating-checks.json",
            serde_json::to_value(&build.nominal_seating_checks)?,
            ["left", "right"],
            &[
                "signed_gap_lower_mm",
                "signed_gap_upper_mm",
                "disk_radius_lower_mm",
            ][..],
        ),
        (
            "pocket_fit",
            "pocket-fit-checks.json",
            serde_json::to_value(&build.pocket_fit_checks)?,
            ["pocket", "insert"],
            &[
                "side_clearance_lower_mm",
                "side_clearance_upper_mm",
                "signed_floor_gap_lower_mm",
                "signed_floor_gap_upper_mm",
                "axial_overlap_lower_mm",
                "axial_overlap_upper_mm",
            ][..],
        ),
        (
            "clearance",
            "clearance-checks.json",
            serde_json::to_value(&build.clearance_checks)?,
            ["left", "right"],
            &["lower_bound_mm", "upper_bound_mm"][..],
        ),
    ];
    for (kind, file, reports, endpoints, fields) in groups {
        for (id, report) in reports.as_object().ok_or("invalid native report map")? {
            let output = text(&report["declaration"], "output")?;
            // Globally checked obligations may target outputs outside a selected publication.
            let Some(assembly) = build.assemblies.get(output) else {
                continue;
            };
            if text(&report["declaration"], "id")? != id || text(report, "status")? != "passed" {
                return Err("native engineering report ID/status mismatch".into());
            }
            if kind != "datum_dimension"
                && text(report, "assembly_content_hash")? != assembly.content_hash
            {
                return Err("native engineering report assembly identity is stale".into());
            }
            let instance_paths = endpoints
                .iter()
                .map(|e| witness(assembly, &report[e]))
                .collect::<Result<Vec<_>, _>>()?;
            let mut quantities = BTreeMap::new();
            for field in fields {
                let value = report[field]
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or("native engineering report has invalid measurement")?;
                quantities.insert((*field).to_owned(), value);
            }
            let scope = match kind {
                "datum_dimension" => "Directed native datum distance",
                "nominal_seating" => "Local nominal planar seating",
                "pocket_fit" => "Complete capsule footprint and axial overlap",
                _ if [
                    "right_translation",
                    "right_rotation",
                    "rotation_group",
                    "translation_group",
                ]
                .iter()
                .any(|k| !report["declaration"][k].is_null()) =>
                {
                    "Entire declared motion path"
                }
                _ => "Initial pose",
            };
            let check = NativeEngineeringCheck {
                scope: scope.into(),
                kind: kind.into(),
                id: id.clone(),
                output: output.into(),
                status: "passed".into(),
                target_content_hash: assembly.content_hash.clone(),
                quantities,
                instance_paths,
                report_file: file.into(),
                report_digest: blake3::hash(&serde_json::to_vec(report)?)
                    .to_hex()
                    .to_string(),
            };
            outputs
                .entry(output.into())
                .or_default()
                .insert(format!("{kind}.{id}"), check);
        }
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn housing() -> BuildResult {
        let recipe = cad_recipe::from_starlark_with_params(
            "motor_mount_housing.star",
            include_str!("../../../examples/motor_mount_housing.star"),
            &BTreeMap::new(),
        )
        .unwrap();
        BuildEngine::default().build(&recipe).unwrap()
    }
    #[test]
    fn panel_binds_native_values_and_complete_report_payload() {
        let mut build = housing();
        let checks = collect(&build).unwrap();
        assert_eq!(checks.len(), 1);
        let dimensions = &checks["motor_mount_housing"];
        assert_eq!(dimensions.len(), 3);
        let original = &dimensions["datum_dimension.motor_depth"];
        assert!((original.quantities["value_mm"] - 12.).abs() < 1e-9);
        assert_eq!(original.instance_paths, vec![vec!["housing".to_owned()]; 2]);
        build
            .datum_dimensions
            .get_mut("motor_depth")
            .unwrap()
            .arithmetic_uncertainty_mm *= 2.;
        assert_ne!(
            collect(&build).unwrap()["motor_mount_housing"]["datum_dimension.motor_depth"]
                .report_digest,
            original.report_digest
        );
        build.assemblies.clear();
        assert!(collect(&build).unwrap().is_empty());
    }
    #[test]
    fn stale_occurrence_identity_and_placement_refuse_panel_publication() {
        let mut build = housing();
        let report = build.datum_dimensions.get_mut("motor_depth").unwrap();
        let original = report.from.part_content_hash.clone();
        report.from.part_content_hash = "stale".into();
        assert!(collect(&build).is_err());
        let report = build.datum_dimensions.get_mut("motor_depth").unwrap();
        report.from.part_content_hash = original;
        report.from.occurrence_frame.translation[0] += 1.;
        assert!(collect(&build).is_err());
    }
}
