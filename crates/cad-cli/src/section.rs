//! Source-bound read-only analytic section reporting.
use super::*;
use cad_kernel::{PlanarSection, SectionPlane};

pub(super) fn implementation_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        for source in [
            include_str!("section.rs"),
            include_str!("main.rs"),
            include_str!("../../cad-export/src/section.rs"),
            include_str!("../../../Cargo.lock"),
        ] {
            hash.update(&(source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.finalize().to_hex().to_string()
    })
}

pub(super) fn vector(value: &str) -> Result<[f64; 3], Box<dyn Error>> {
    let values = value
        .split(',')
        .map(|v| v.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()?;
    let result: [f64; 3] = values
        .try_into()
        .map_err(|_| "section vectors require exactly three comma-separated numbers")?;
    if result.iter().any(|v| !v.is_finite()) {
        return Err("section vectors must be finite".into());
    }
    Ok(result)
}
pub(super) fn report(
    recipe: &Recipe,
    build: &BuildResult,
    output: &str,
    plane: SectionPlane,
    source: &[u8],
    parameters: &BTreeMap<String, f64>,
) -> Result<Value, Box<dyn Error>> {
    let tolerance = recipe.tolerance.unwrap_or_default();
    cad_kernel::ModelSnapshot {
        vertices: vec![],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    }
    .section(plane, tolerance)?;
    let mut occurrences = vec![];
    let (section, kind, feature_id, content_hash) = if let Some(body) = build.outputs.get(output) {
        (
            body.result.model.section(plane, tolerance)?,
            "body",
            body.feature_id.as_str(),
            body.content_hash.as_str(),
        )
    } else if let Some(assembly) = build.assemblies.get(output) {
        let mut combined = PlanarSection {
            plane,
            loops: vec![],
            area: 0.,
            centroid: None,
        };
        let mut moment = [0.; 2];
        for instance in &assembly.instances {
            let part = &assembly.parts[&instance.part];
            let inverse = instance.transform.inverse(tolerance)?;
            let local = SectionPlane {
                origin: inverse.point(plane.origin),
                normal: inverse.vector(plane.normal),
                x_direction: inverse.vector(plane.x_direction),
            };
            let mut section = part.result.model.section(local, tolerance)?;
            // Inverse-transforming all three frame vectors preserves the common
            // drawing coordinates; only the frame label returns to world space.
            section.plane = plane;
            if let Some(center) = section.centroid {
                for i in 0..2 {
                    moment[i] += section.area * center[i];
                }
            }
            combined.area += section.area;
            occurrences.push(json!({"instance_id":instance.id,"instance_path":assembly.instance_paths[&instance.id],"part_id":instance.part,"content_hash":part.content_hash,"section":section}));
        }
        if !combined.area.is_finite() || moment.iter().any(|v| !v.is_finite()) {
            return Err(cad_kernel::KernelError::NumericRange.into());
        }
        if combined.area > 0. {
            combined.centroid = Some(moment.map(|v| v / combined.area));
        }
        (
            combined,
            "assembly",
            assembly.assembly_id.as_str(),
            assembly.content_hash.as_str(),
        )
    } else {
        return Err(format!("unknown recipe output '{output}'").into());
    };
    let source_hash = blake3::hash(source).to_hex().to_string();
    let query_context = json!({"recipe_source_hash":source_hash,"section_build_id":implementation_id(),"parameter_overrides":parameters,"output_name":output,"content_hash":content_hash,"assembly_build_id":if kind=="assembly"{Some(cad_assembly::assembly_build_id())}else{None},"plane":plane,"tolerance":tolerance,"kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id()});
    let mut section = serde_json::to_value(section)?;
    if kind == "assembly" {
        section.as_object_mut().unwrap().remove("loops");
    }
    Ok(json!({
        "schema_version":1,"kind":kind,"output_name":output,
        "coordinate_frame":if kind=="assembly" {"root_assembly_frame"} else {"output_model_frame"},
        "units":{"length":"mm","area":"mm2","angles":"radians"},
        "area_kind":if kind=="assembly" {"summed_occurrence_material_not_union"}else{"integrated_solid_material"},
        "section":section,"occurrences":occurrences,
        "snapshot":{"recipe_source_hash":source_hash,"section_build_id":implementation_id(),"query_context_hash":blake3::hash(&serde_json::to_vec(&query_context)?).to_hex().to_string(),"parameter_overrides":parameters,"tolerance":tolerance,"feature_id":feature_id,"content_hash":content_hash,"kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),"assembly_build_id":if kind=="assembly"{Some(cad_assembly::assembly_build_id())}else{None}},
        "stats":build.stats,"scope":build.scope,"face_assertions":build.face_assertions,"property_assertions":build.property_assertions,"sketch_reports":build.sketch_reports,
        "exchange_requirements":exchange::pending(&recipe.exchange_requirements),"exchange_policy_status":if recipe.exchange_requirements.is_empty(){"not_requested"}else{"pending_publication"}
    }))
}
