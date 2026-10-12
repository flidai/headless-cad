//! Descriptive exchange coverage, not validation, certification, or hash binding.
use crate::ExportError;
use cad_assembly::BuiltAssembly;
use cad_kernel::{ModelSnapshot, Tolerance};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeFormat {
    Step,
    Stl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityStatus {
    Supported,
    NotCarried,
    NotApplicable,
    Approximate,
}

#[derive(Clone, Copy)]
pub enum ExchangeSource<'a> {
    Body(&'a ModelSnapshot),
    Assembly(&'a BuiltAssembly),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FidelityEntry {
    pub aspect: &'static str,
    pub status: FidelityStatus,
    /// Count in the supplied source, in units described by `detail`. Null means
    /// unknown, never zero. Definitions are counted once, not per occurrence.
    pub source_count: Option<usize>,
    pub detail: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExchangeFidelityReport {
    pub schema_version: u32,
    pub format: ExchangeFormat,
    pub source_kind: &'static str,
    pub source_units: &'static str,
    pub modeling_tolerance_mm: f64,
    pub scope: &'static str,
    pub entries: Vec<FidelityEntry>,
}

/// Describe the current exporter mapping for an inspected source. `Supported`
/// means the listed aspect is carried by this exporter, conditional on successful
/// export. It does not mean complete STEP conformance or downstream preservation.
/// This report neither exports bytes nor certifies those bytes; callers bind the
/// report to the actual successful export and its source revision separately.
pub fn exchange_fidelity(
    source: ExchangeSource<'_>,
    format: ExchangeFormat,
    tolerance: Tolerance,
) -> Result<ExchangeFidelityReport, ExportError> {
    if !tolerance.is_valid() {
        return Err(ExportError::InvalidModel(
            "invalid modeling tolerance".into(),
        ));
    }
    let assembly = match source {
        ExchangeSource::Body(model) => {
            let validation = model.validate(tolerance);
            if !validation.valid {
                return Err(ExportError::InvalidModel(format!(
                    "{:?}",
                    validation.issues
                )));
            }
            None
        }
        ExchangeSource::Assembly(assembly) => {
            assembly
                .validate(tolerance)
                .map_err(|e| ExportError::InvalidModel(e.to_string()))?;
            Some(assembly)
        }
    };
    let step = format == ExchangeFormat::Step;
    use FidelityStatus::*;
    let mut entries = Vec::new();
    let mut add = |aspect, status, source_count, detail| {
        entries.push(FidelityEntry {
            aspect,
            status,
            source_count,
            detail,
        })
    };
    let solids = match source {
        ExchangeSource::Body(model) => model.solids.len(),
        ExchangeSource::Assembly(a) => a.parts.values().map(|p| p.result.model.solids.len()).sum(),
    };
    add(
        "geometry",
        if step { Supported } else { Approximate },
        Some(solids),
        if step {
            "Native supported planar/analytic boundary geometry is exported as STEP B-reps; count is solids across distinct part definitions, not placed occurrences. This does not claim arbitrary surface support."
        } else {
            "STL carries tessellated surfaces with placed triangle coordinates, not analytic curves/surfaces or native topology; count is source solids across distinct part definitions. Assembly overlaps are not fused."
        },
    );
    add(
        "units",
        if step { Supported } else { NotCarried },
        Some(1),
        if step {
            "Explicit STEP millimetre length units; source coordinates and linear modeling tolerance are millimetres."
        } else {
            "STL has no unit declaration. Numeric coordinates are written in millimetres; consumers must choose that convention explicitly."
        },
    );
    let defs = assembly.map_or(0, |a| a.parts.len());
    let hierarchy = assembly.map_or(0, |a| a.hierarchy.len());
    let occurrences = assembly.map_or(0, |a| a.instances.len());
    let mapped = |count| {
        if count == 0 {
            NotApplicable
        } else if step {
            Supported
        } else {
            NotCarried
        }
    };
    add("assembly_definitions", mapped(defs), Some(defs), "Distinct part definitions: STEP product definitions retain sharing; STL repeats placed triangles without reusable definition identity.");
    add("assembly_hierarchy", mapped(hierarchy), Some(hierarchy), "Reachable assembly definitions: STEP product usage relationships retain parent/child structure; STL flattens leaf occurrences.");
    add("occurrence_paths", mapped(occurrences), Some(occurrences), "Leaf occurrence paths: STEP local occurrence names and parent/child relationships permit path reconstruction, not a literal serialized path array. STL labels are sanitized and do not preserve path identity.");
    add("occurrence_placements", mapped(occurrences), Some(occurrences), "Resolved leaf placements: STEP carries local rigid transformations through the hierarchy. STL bakes placements into vertices and does not carry editable transformation records.");
    let count_metadata = |field: fn(&cad_assembly::PartMetadata) -> bool| {
        assembly.map_or(0, |a| {
            a.parts.values().filter(|p| field(&p.metadata)).count()
        })
    };
    let omitted = |count| {
        if count == 0 {
            NotApplicable
        } else {
            NotCarried
        }
    };
    for (aspect, count, detail) in [
        ("part_numbers", count_metadata(|m| m.part_number.is_some()), "Part definitions with a part-number value. STEP uses internal part IDs as product names; neither exporter maps engineering part numbers. BOM exports carry these fields separately."),
        ("part_descriptions", count_metadata(|m| m.description.is_some()), "Part definitions with an engineering description; not mapped to STEP or STL. BOM exports carry these fields separately."),
        ("part_revisions", count_metadata(|m| m.revision.is_some()), "Part definitions with a metadata revision value; not mapped to STEP or STL. This is not a Git or recipe revision."),
        ("material_names", count_metadata(|m| m.material.is_some()), "Part definitions with a material name; not mapped to STEP or STL. BOM exports carry material metadata separately."),
        ("material_density", count_metadata(|m| m.material.is_some()), "Part definitions with density in kg/m3; not mapped to STEP or STL. Neither format receives the source material mass/inertia report."),
    ] { add(aspect, omitted(count), Some(count), detail); }
    let datums = assembly.map_or(0, |a| a.parts.values().map(|p| p.datums.len()).sum());
    add("datums", omitted(datums), Some(datums), "Named resolved datum frames summed once per part definition; these names and frames are not exported. A body snapshot does not contain datum fields.");
    let attachments = assembly.map_or(0, |a| {
        a.hierarchy
            .values()
            .map(|d| {
                d.instances
                    .iter()
                    .filter(|i| i.attachment.is_some())
                    .count()
                    + d.assemblies
                        .iter()
                        .filter(|i| i.attachment.is_some())
                        .count()
            })
            .sum()
    });
    add("attachment_intent", omitted(attachments), Some(attachments), "Datum attachment records counted once per reachable assembly definition, for both part and subassembly children; neither exporter carries constraints or attachment intent, even when resolved placements are carried.");
    let history = assembly.map(|a| {
        a.parts
            .values()
            .map(|p| {
                p.result.history.generated_faces.len()
                    + p.result
                        .history
                        .input_faces
                        .values()
                        .map(|m| m.len())
                        .sum::<usize>()
            })
            .sum()
    });
    add("operation_face_history", history.map_or(NotCarried, omitted), history, "Available operation-result generated-role entries plus operand face-mapping entries, summed once per part definition; no mapping is exported. A body snapshot does not expose operation history, so its count is unknown. This is not a full feature history.");
    add("semantic_face_references", NotCarried, None, "Recipe-level semantic selections and provenance references are not exposed by this API's source types and are not encoded by these exporters; presence/count is unknown.");
    add("modeling_tolerance", if step { Supported } else { NotCarried }, Some(1), "The supplied linear modeling tolerance is STEP geometric uncertainty, not a manufacturing tolerance. STL has no corresponding uncertainty field; tessellation policy is a separate approximation contract.");
    add("manufacturing_pmi", NotApplicable, Some(0), "Supplied source schemas contain no manufacturing dimensions, GD&T, surface-finish annotations, or PMI objects. Geometric uncertainty must not be interpreted as manufacturing PMI.");
    add("recipe_history", NotCarried, None, "Executable recipe, parameters, feature graph and revision history are outside these source types and are not encoded by either exporter; presence/count is unknown.");
    add("source_revision", NotCarried, None, "Git revision and recipe revision identity are unknown to this API and absent from STEP/STL. Content/revision binding belongs in an external publication envelope.");
    Ok(ExchangeFidelityReport { schema_version: 1, format, source_kind: if assembly.is_some() { "assembly" } else { "body" }, source_units: "mm", modeling_tolerance_mm: tolerance.linear, scope: "Describes current exporter mappings conditional on successful export; not a validation proof, downstream fidelity guarantee, manufacturing release, or complete format conformance claim.", entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_assembly::{
        BuiltPart, DatumAttachment, Instance, Material, PartMetadata, RigidTransform,
    };
    use std::{collections::BTreeMap, sync::Arc};

    fn entry<'a>(report: &'a ExchangeFidelityReport, aspect: &str) -> &'a FidelityEntry {
        report.entries.iter().find(|e| e.aspect == aspect).unwrap()
    }

    fn assembly(metadata: PartMetadata) -> BuiltAssembly {
        let t = Tolerance::default();
        let result = Arc::new(cad_kernel::extrude_circle(3., 4., t).unwrap());
        let part = BuiltPart {
            datums: BTreeMap::from([("mount".into(), RigidTransform::default())]),
            metadata,
            feature_id: "cylinder".into(),
            content_hash: "fixture".into(),
            result,
        };
        BuiltAssembly::new(
            "fixture".into(),
            BTreeMap::from([("part".into(), part)]),
            vec![
                Instance {
                    id: "fixed".into(),
                    part: "part".into(),
                    transform: RigidTransform::default(),
                    attachment: None,
                },
                Instance {
                    id: "attached".into(),
                    part: "part".into(),
                    transform: RigidTransform::default(),
                    attachment: Some(DatumAttachment {
                        instance: "fixed".into(),
                        target_path: vec![],
                        datum: "mount".into(),
                        target_datum: "mount".into(),
                    }),
                },
            ],
            t,
        )
        .unwrap()
    }

    #[test]
    fn reports_actual_step_mapping_and_stl_losses_for_shared_engineering_parts() {
        let a = assembly(PartMetadata {
            part_number: Some("ENGINEERING-PN".into()),
            revision: Some("REV-17".into()),
            description: Some("engineering description".into()),
            material: Some(Material {
                name: "Steel".into(),
                density_kg_per_m3: 7800.,
            }),
        });
        let t = Tolerance::default();
        let step_report =
            exchange_fidelity(ExchangeSource::Assembly(&a), ExchangeFormat::Step, t).unwrap();
        let stl_report =
            exchange_fidelity(ExchangeSource::Assembly(&a), ExchangeFormat::Stl, t).unwrap();
        assert_eq!(
            entry(&step_report, "assembly_definitions").source_count,
            Some(1)
        );
        assert_eq!(
            entry(&step_report, "occurrence_paths").source_count,
            Some(2)
        );
        assert_eq!(entry(&step_report, "datums").source_count, Some(1));
        assert_eq!(
            entry(&step_report, "attachment_intent").source_count,
            Some(1)
        );
        for aspect in [
            "assembly_definitions",
            "assembly_hierarchy",
            "occurrence_paths",
            "occurrence_placements",
            "units",
            "modeling_tolerance",
        ] {
            assert_eq!(
                entry(&step_report, aspect).status,
                FidelityStatus::Supported,
                "{aspect}"
            );
            assert_eq!(
                entry(&stl_report, aspect).status,
                FidelityStatus::NotCarried,
                "{aspect}"
            );
        }
        for aspect in [
            "part_numbers",
            "part_descriptions",
            "part_revisions",
            "material_names",
            "material_density",
            "datums",
            "attachment_intent",
        ] {
            assert_eq!(entry(&step_report, aspect).source_count, Some(1));
            assert_eq!(
                entry(&step_report, aspect).status,
                FidelityStatus::NotCarried
            );
        }
        // Check actual format output, not merely enum construction.
        let step = crate::assembly_to_step(&a, t).unwrap();
        assert!(step.contains("NEXT_ASSEMBLY_USAGE_OCCURRENCE"));
        assert!(step.contains("GLOBAL_UNIT_ASSIGNED_CONTEXT"));
        assert!(step.contains("UNCERTAINTY_MEASURE_WITH_UNIT"));
        assert!(!step.contains("ENGINEERING-PN"));
        assert!(!step.contains("REV-17"));
        assert!(!step.contains("Steel"));
        assert!(!step.contains("engineering description"));
        let stl = crate::assembly_to_stl(&a, t).unwrap();
        assert_eq!(stl.lines().filter(|l| l.starts_with("solid ")).count(), 2);
        assert!(!stl.contains("NEXT_ASSEMBLY_USAGE_OCCURRENCE"));
        assert_eq!(
            entry(&stl_report, "geometry").status,
            FidelityStatus::Approximate
        );
        assert_eq!(
            step_report,
            exchange_fidelity(ExchangeSource::Assembly(&a), ExchangeFormat::Step, t).unwrap()
        );
    }

    #[test]
    fn absent_metadata_is_distinguished_from_unknown_recipe_provenance() {
        let a = assembly(PartMetadata::default());
        let report = exchange_fidelity(
            ExchangeSource::Assembly(&a),
            ExchangeFormat::Step,
            Tolerance::default(),
        )
        .unwrap();
        for aspect in [
            "part_numbers",
            "material_density",
            "part_revisions",
            "manufacturing_pmi",
        ] {
            assert_eq!(entry(&report, aspect).source_count, Some(0));
            assert_eq!(entry(&report, aspect).status, FidelityStatus::NotApplicable);
        }
        for aspect in [
            "recipe_history",
            "source_revision",
            "semantic_face_references",
        ] {
            assert_eq!(entry(&report, aspect).source_count, None);
            assert_eq!(entry(&report, aspect).status, FidelityStatus::NotCarried);
        }
        assert!(
            entry(&report, "operation_face_history")
                .source_count
                .unwrap()
                > 0
        );
    }

    #[test]
    fn body_snapshot_does_not_invent_assembly_or_history_fields() {
        let t = Tolerance::default();
        let body = cad_kernel::extrude_circle(5., 3., t).unwrap().model;
        let report =
            exchange_fidelity(ExchangeSource::Body(&body), ExchangeFormat::Step, t).unwrap();
        assert_eq!(report.source_kind, "body");
        assert_eq!(entry(&report, "assembly_hierarchy").source_count, Some(0));
        assert_eq!(entry(&report, "operation_face_history").source_count, None);
        assert_eq!(entry(&report, "geometry").source_count, Some(1));
        let step = crate::to_step(&body, t).unwrap();
        assert!(step.contains("CYLINDRICAL_SURFACE"));
        assert!(step.contains(".MILLI.,.METRE."));
    }

    #[test]
    fn invalid_sources_and_tolerance_do_not_receive_coverage_reports() {
        let t = Tolerance::default();
        let mut body = cad_kernel::extrude_rectangle(2., 3., 4., t).unwrap().model;
        assert!(exchange_fidelity(
            ExchangeSource::Body(&body),
            ExchangeFormat::Step,
            Tolerance { linear: f64::NAN }
        )
        .is_err());
        body.vertices[0].position[0] = f64::NAN;
        assert!(exchange_fidelity(ExchangeSource::Body(&body), ExchangeFormat::Stl, t).is_err());
        let mut a = assembly(PartMetadata::default());
        a.instance_paths.clear();
        assert!(exchange_fidelity(ExchangeSource::Assembly(&a), ExchangeFormat::Step, t).is_err());
    }
}
