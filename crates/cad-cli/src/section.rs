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
            include_str!("source.rs"),
            include_str!("../../cad-export/src/section.rs"),
            include_str!("../../cad-build/src/section_views.rs"),
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
    let source_geometry = if let Some(body) = build.outputs.get(output) {
        cad_build::SectionSource::Body {
            model: &body.result.model,
            feature_id: &body.feature_id,
            content_hash: &body.content_hash,
        }
    } else if let Some(assembly) = build.assemblies.get(output) {
        cad_build::SectionSource::Assembly(assembly)
    } else {
        return Err(format!("unknown recipe output '{output}'").into());
    };
    let query =
        cad_build::query_section(source_geometry, plane, recipe.tolerance.unwrap_or_default())?;
    bind_report(recipe, build, output, &query, source, parameters)
}

pub(super) fn bind_view_report(
    recipe: &Recipe,
    build: &BuildResult,
    view: &cad_build::SectionViewReport,
    source: &[u8],
    parameters: &BTreeMap<String, f64>,
) -> Result<Value, Box<dyn Error>> {
    let mut report = bind_report(
        recipe,
        build,
        &view.declaration.output,
        &view.query,
        source,
        parameters,
    )?;
    report["section_view"] = json!({"declaration":view.declaration,"status":view.status});
    if let Some(witness) = &view.anchor_witness {
        report["section_view"]["anchor_witness"] = serde_json::to_value(witness)?;
        // Equal world planes with different semantic anchors remain distinct
        // reviewable queries. The base context binds native source/query inputs.
        let context = json!({
            "base_query_context_hash":report["snapshot"]["query_context_hash"],
            "anchor":view.declaration.anchor,
            "anchor_witness":witness,
            "resolved_plane":view.query.section.plane,
        });
        report["snapshot"]["query_context_hash"] =
            json!(blake3::hash(&serde_json::to_vec(&context)?)
                .to_hex()
                .to_string());
    }
    if !view.dimensions.is_empty() {
        report["section_view"]["dimensions"] = serde_json::to_value(&view.dimensions)?;
        let context = json!({
            "base_query_context_hash":report["snapshot"]["query_context_hash"],
            "projected_datum_dimensions":view.dimensions,
        });
        report["snapshot"]["query_context_hash"] =
            json!(blake3::hash(&serde_json::to_vec(&context)?)
                .to_hex()
                .to_string());
    }
    if !view.circle_dimensions.is_empty() {
        report["section_view"]["circle_dimensions"] =
            serde_json::to_value(&view.circle_dimensions)?;
        let context = json!({
            "base_query_context_hash": report["snapshot"]["query_context_hash"],
            "native_circle_dimensions": view.circle_dimensions,
        });
        report["snapshot"]["query_context_hash"] =
            json!(blake3::hash(&serde_json::to_vec(&context)?)
                .to_hex()
                .to_string());
    }
    Ok(report)
}

pub(super) fn bind_report(
    recipe: &Recipe,
    build: &BuildResult,
    output: &str,
    query: &cad_build::SectionQuery,
    source: &[u8],
    parameters: &BTreeMap<String, f64>,
) -> Result<Value, Box<dyn Error>> {
    let tolerance = recipe.tolerance.unwrap_or_default();
    let plane = query.section.plane;
    let kind = query.kind.as_str();
    let feature_id = &query.feature_id;
    let content_hash = &query.content_hash;
    let occurrences = &query.occurrences;
    // Keep global obligation evidence, without embedding every other view's
    // curves in every report/SVG (quadratic geometry duplication).
    let section_checks: BTreeMap<_, _> = build.section_views.iter().map(|(id, view)| {
        let mut check = json!({"declaration":view.declaration,"status":view.status,
                   "actual_area":view.query.section.area,"target_content_hash":view.query.content_hash});
        if let Some(witness) = &view.anchor_witness {
            check["anchor_witness"] = json!(witness);
        }
        if !view.dimensions.is_empty() {
            check["dimensions"] = json!(view.dimensions.iter().map(|d| json!({
                "id":d.declaration.id,"status":d.status,"value_mm":d.value_mm,
                "signed_delta_mm":d.signed_delta_mm
            })).collect::<Vec<_>>());
        }
        if !view.circle_dimensions.is_empty() {
            check["circle_dimensions"] = json!(view.circle_dimensions.iter().map(|d| json!({
                "id":d.declaration.id,"status":d.status,"value_mm":d.value_mm,
                "radius":d.radius,"center":d.center,"target_content_hash":d.target_content_hash
            })).collect::<Vec<_>>());
        }
        (id, check)
    }).collect();
    let source_hash = blake3::hash(source).to_hex().to_string();
    let query_context = json!({"recipe_source_hash":source_hash,"mechanism_context_hash":build.mechanism_context_hash,"section_build_id":implementation_id(),"parameter_overrides":parameters,"output_name":output,"content_hash":content_hash,"assembly_build_id":if kind=="assembly"{Some(cad_assembly::assembly_build_id())}else{None},"plane":plane,"tolerance":tolerance,"kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id()});
    let section = &query.section;
    Ok(json!({
        "schema_version":1,"kind":kind,"output_name":output,
        "coordinate_frame":if kind=="assembly" {"root_assembly_frame"} else {"output_model_frame"},
        "units":{"length":"mm","area":"mm2","angles":"radians"},
        "area_kind":if kind=="assembly" {"summed_occurrence_material_not_union"}else{"integrated_solid_material"},
        "section":section,"occurrences":occurrences,
        "snapshot":{"recipe_source_hash":source_hash,"mechanism_context_hash":build.mechanism_context_hash,"section_build_id":implementation_id(),"query_context_hash":blake3::hash(&serde_json::to_vec(&query_context)?).to_hex().to_string(),"parameter_overrides":parameters,"tolerance":tolerance,"feature_id":feature_id,"content_hash":content_hash,"kernel_build_id":cad_kernel::KERNEL_BUILD_ID,"executor_build_id":cad_build::executor_build_id(),"assembly_build_id":if kind=="assembly"{Some(cad_assembly::assembly_build_id())}else{None}},
        "stats":build.stats,"scope":build.scope,"section_views":section_checks,"face_assertions":build.face_assertions,"property_assertions":build.property_assertions,"sketch_reports":build.sketch_reports,"mechanism_reports":build.mechanism_reports,"motion_studies":motion_summary(build),"mechanism_bindings":build.mechanism_bindings,"mechanism_context_hash":build.mechanism_context_hash,
        "exchange_requirements":exchange::pending(&recipe.exchange_requirements),"exchange_policy_status":if recipe.exchange_requirements.is_empty(){"not_requested"}else{"pending_publication"}
    }))
}

pub(super) struct DrawingGeometry {
    pub label: String,
    pub sections: Vec<PlanarSection>,
    pub dimensions: Vec<cad_export::SectionDimensionAnnotation>,
    pub circles: Vec<cad_export::SectionCircleDimensionAnnotation>,
}

fn dimension_tolerance(
    report: &Value,
) -> Result<Option<cad_export::DimensionToleranceAnnotation>, Box<dyn Error>> {
    let Some(value) = report.get("tolerance").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    Ok(Some(cad_export::DimensionToleranceAnnotation {
        nominal_mm: value["nominal_mm"]
            .as_f64()
            .ok_or("missing tolerance nominal")?,
        lower_deviation_mm: value["lower_deviation_mm"]
            .as_f64()
            .ok_or("missing tolerance lower deviation")?,
        upper_deviation_mm: value["upper_deviation_mm"]
            .as_f64()
            .ok_or("missing tolerance upper deviation")?,
    }))
}

pub(super) fn drawing_geometry(report: &Value) -> Result<DrawingGeometry, Box<dyn Error>> {
    let mut label = report["output_name"]
        .as_str()
        .unwrap_or("section")
        .to_owned();
    if let Some(id) = report["section_view"]["declaration"]["id"].as_str() {
        label.push_str(&format!(" — {id}"));
    }
    if report["kind"] == "assembly" {
        label.push_str(" — summed part areas (overlaps counted)");
    }
    let mut sections: Vec<PlanarSection> = if report["kind"] == "assembly" {
        report["occurrences"]
            .as_array()
            .ok_or("missing section occurrences")?
            .iter()
            .map(|o| serde_json::from_value(o["section"].clone()))
            .collect::<Result<_, _>>()?
    } else {
        vec![serde_json::from_value(report["section"].clone())?]
    };
    if sections.is_empty() {
        sections.push(PlanarSection {
            plane: serde_json::from_value(report["section"]["plane"].clone())?,
            loops: vec![],
            area: 0.,
            centroid: None,
        });
    }
    let dimensions: Vec<cad_export::SectionDimensionAnnotation> = report["section_view"]
        ["dimensions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            Ok(cad_export::SectionDimensionAnnotation {
                id: d["declaration"]["id"]
                    .as_str()
                    .ok_or("missing dimension id")?
                    .into(),
                from: serde_json::from_value(d["from_point"].clone())?,
                to: serde_json::from_value(d["to_point"].clone())?,
                axis: match d["declaration"]["axis"].as_str() {
                    Some("x") => 0,
                    Some("y") => 1,
                    _ => return Err("invalid dimension axis".into()),
                },
                offset: d["declaration"]["offset"]
                    .as_f64()
                    .ok_or("missing dimension offset")?,
                value_mm: d["value_mm"].as_f64().ok_or("missing dimension value")?,
                tolerance: dimension_tolerance(d)?,
            })
        })
        .collect::<Result<_, Box<dyn Error>>>()?;
    let circles: Vec<cad_export::SectionCircleDimensionAnnotation> = report["section_view"]
        ["circle_dimensions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            Ok(cad_export::SectionCircleDimensionAnnotation {
                id: d["declaration"]["id"]
                    .as_str()
                    .ok_or("missing circle dimension id")?
                    .into(),
                center: serde_json::from_value(d["center"].clone())?,
                radius: d["radius"].as_f64().ok_or("missing circle radius")?,
                kind: match d["declaration"]["kind"].as_str() {
                    Some("radius") => cad_export::SectionCircleDimensionKind::Radius,
                    Some("diameter") => cad_export::SectionCircleDimensionKind::Diameter,
                    _ => return Err("invalid circle dimension kind".into()),
                },
                label_offset: serde_json::from_value(d["declaration"]["label_offset"].clone())?,
                value_mm: d["value_mm"]
                    .as_f64()
                    .ok_or("missing circle dimension value")?,
                tolerance: dimension_tolerance(d)?,
            })
        })
        .collect::<Result<_, Box<dyn Error>>>()?;
    Ok(DrawingGeometry {
        label,
        sections,
        dimensions,
        circles,
    })
}

pub(super) fn svg(report: &Value) -> Result<String, Box<dyn Error>> {
    let geometry = drawing_geometry(report)?;
    let mut svg = cad_export::dimensioned_sections_to_svg(
        &geometry.sections,
        &geometry.label,
        &geometry.dimensions,
        &geometry.circles,
    )?;
    let metadata = serde_json::to_string(report)?
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        // JSON permits these scalar values, but XML 1.0 text does not.
        .replace('\u{fffe}', "\\ufffe")
        .replace('\u{ffff}', "\\uffff");
    let at = svg.find('>').ok_or("invalid SVG root")? + 1;
    svg.insert_str(at, &format!("\n<metadata>{metadata}</metadata>"));
    Ok(svg)
}
