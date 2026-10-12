//! Source-authored analytic sections and shared body/assembly query semantics.
use super::*;
use cad_kernel::{PlanarSection, SectionLoop, SectionPlane};
pub const MAX_SECTION_VIEWS: usize = 128;
pub const MAX_SECTION_DIMENSIONS_PER_VIEW: usize = 32;
pub const MAX_SECTION_DIMENSIONS: usize = 1024;
pub const MAX_SECTION_VIEW_WORK: usize = 10_000_000;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionAnchor {
    pub instance_path: Vec<String>,
    pub datum: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionDimensionAxis {
    X,
    Y,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDimension {
    pub id: String,
    pub from: SectionAnchor,
    pub to: SectionAnchor,
    pub axis: SectionDimensionAxis,
    pub offset: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionTolerance>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SectionDimensionReport {
    pub declaration: SectionDimension,
    pub status: String,
    pub value_mm: f64,
    pub signed_delta_mm: f64,
    pub from_point: [f64; 2],
    pub to_point: [f64; 2],
    pub from_witness: SectionAnchorWitness,
    pub to_witness: SectionAnchorWitness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionToleranceReport>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionCircleDimensionKind {
    Radius,
    Diameter,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionCircleDimension {
    pub id: String,
    pub anchor_feature: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_path: Option<Vec<String>>,
    pub kind: SectionCircleDimensionKind,
    pub label_offset: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionTolerance>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SectionCircleDimensionReport {
    pub declaration: SectionCircleDimension,
    pub status: String,
    pub value_mm: f64,
    pub center: [f64; 2],
    pub radius: f64,
    pub selection: FaceResolution,
    pub anchor_content_hash: Option<String>,
    pub target_content_hash: String,
    pub instance_id: Option<String>,
    pub instance_path: Option<Vec<String>>,
    pub part_id: Option<String>,
    pub occurrence_frame: Option<cad_assembly::RigidTransform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionToleranceReport>,
}
impl SectionCircleDimension {
    pub fn validate(&self) -> Result<(), String> {
        if !safe_id(&self.id)
            || self.anchor_feature.is_empty()
            || self.anchor_feature.trim() != self.anchor_feature
            || self.role.is_empty()
            || self.role.trim() != self.role
        {
            return Err("invalid circle dimension ID, anchor feature or role".into());
        }
        if self.instance_path.as_ref().is_some_and(|p| {
            !valid_anchor(&SectionAnchor {
                instance_path: p.clone(),
                datum: "unused".into(),
            })
        }) {
            return Err("invalid literal circle dimension occurrence path".into());
        }
        if self.label_offset.iter().any(|v| !v.is_finite())
            || self.label_offset == [0., 0.]
            || [self.min, self.max]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v < 0.)
            || matches!((self.min,self.max),(Some(a),Some(b)) if a>b)
        {
            return Err("circle dimension label offset must be finite nonzero; bounds finite nonnegative and ordered".into());
        }
        if let Some(tolerance) = &self.tolerance {
            tolerance.validate()?;
        }
        Ok(())
    }
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn valid_anchor(anchor: &SectionAnchor) -> bool {
    let valid = |id: &str| !id.is_empty() && id.len() <= 1024 && id.trim() == id;
    !anchor.instance_path.is_empty()
        && anchor.instance_path.len() <= 64
        && anchor.instance_path.iter().all(|id| valid(id))
        && valid(&anchor.datum)
}
impl SectionDimension {
    pub fn validate(&self) -> Result<(), String> {
        if !safe_id(&self.id) {
            return Err(
                "dimension id must contain 1..128 ASCII letters, digits, underscores or hyphens"
                    .into(),
            );
        }
        if !valid_anchor(&self.from) || !valid_anchor(&self.to) {
            return Err(
                "dimension endpoints require bounded literal leaf paths and datum names".into(),
            );
        }
        if !self.offset.is_finite()
            || [self.min, self.max]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v < 0.)
            || matches!((self.min,self.max),(Some(a),Some(b)) if a>b)
        {
            return Err(
                "dimension offset must be finite; bounds finite nonnegative and ordered".into(),
            );
        }
        if let Some(tolerance) = &self.tolerance {
            tolerance.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct SectionAnchorWitness {
    pub instance_id: String,
    pub instance_path: Vec<String>,
    pub part_id: String,
    pub part_content_hash: String,
    pub datum: String,
    pub datum_frame: cad_assembly::RigidTransform,
    pub occurrence_frame: cad_assembly::RigidTransform,
    pub world_frame: cad_assembly::RigidTransform,
    pub resolved_plane: SectionPlane,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_datum: Option<FaceDatumResolution>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionView {
    pub id: String,
    pub output: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<SectionDimension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub circle_dimensions: Vec<SectionCircleDimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<SectionAnchor>,
    pub origin: [f64; 3],
    #[serde(default = "normal")]
    pub normal: [f64; 3],
    #[serde(default = "x_direction")]
    pub x_direction: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_area: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_area: Option<f64>,
}
fn normal() -> [f64; 3] {
    [0., 0., 1.]
}
fn x_direction() -> [f64; 3] {
    [1., 0., 0.]
}
impl SectionView {
    pub fn plane(&self) -> SectionPlane {
        SectionPlane {
            origin: self.origin,
            normal: self.normal,
            x_direction: self.x_direction,
        }
    }
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidSectionView {
            id: self.id.clone(),
            message: message.into(),
        };
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(fail(
                "id must contain 1..128 ASCII letters, digits, underscores or hyphens",
            ));
        }
        if self.output.is_empty() || self.output.trim() != self.output {
            return Err(fail("output must be a nonempty identifier"));
        }
        if let Some(anchor) = &self.anchor {
            let valid = |id: &str| !id.is_empty() && id.len() <= 1024 && id.trim() == id;
            if anchor.instance_path.is_empty()
                || anchor.instance_path.len() > 64
                || !anchor.instance_path.iter().all(|id| valid(id))
                || !valid(&anchor.datum)
            {
                return Err(fail("anchor needs a literal leaf path of 1..64 IDs and datum; IDs must be nonempty, at most 1024 bytes, without surrounding whitespace"));
            }
        }
        if self.dimensions.len() + self.circle_dimensions.len() > MAX_SECTION_DIMENSIONS_PER_VIEW {
            return Err(fail("at most 32 dimensions per section view"));
        }
        let mut ids = BTreeSet::new();
        for d in &self.dimensions {
            d.validate()
                .map_err(|e| fail(&format!("dimension '{}': {e}", d.id)))?;
            if !ids.insert(d.id.to_ascii_lowercase()) {
                return Err(fail("duplicate dimension ID within view"));
            }
        }
        for d in &self.circle_dimensions {
            d.validate()
                .map_err(|e| fail(&format!("circle dimension '{}': {e}", d.id)))?;
            if !ids.insert(d.id.to_ascii_lowercase()) {
                return Err(fail("duplicate dimension ID within view"));
            }
        }
        self.plane().validate().map_err(|e| fail(&e.to_string()))?;
        if [self.min_area, self.max_area]
            .into_iter()
            .flatten()
            .any(|a| !a.is_finite() || a < 0.)
            || matches!((self.min_area,self.max_area),(Some(a),Some(b)) if a>b)
        {
            return Err(fail(
                "area bounds must be finite nonnegative numbers with min_area <= max_area",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionSummary {
    pub plane: SectionPlane,
    pub area: f64,
    pub centroid: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loops: Option<Vec<SectionLoop>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionOccurrence {
    pub instance_id: String,
    pub instance_path: Vec<String>,
    pub part_id: String,
    pub content_hash: String,
    pub section: PlanarSection,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionQuery {
    pub kind: String,
    pub feature_id: String,
    pub content_hash: String,
    pub section: SectionSummary,
    pub occurrences: Vec<SectionOccurrence>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SectionViewReport {
    pub declaration: SectionView,
    pub status: String,
    pub query: SectionQuery,
    pub dimensions: Vec<SectionDimensionReport>,
    pub circle_dimensions: Vec<SectionCircleDimensionReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_witness: Option<SectionAnchorWitness>,
}
pub enum SectionSource<'a> {
    Body {
        model: &'a cad_kernel::ModelSnapshot,
        feature_id: &'a str,
        content_hash: &'a str,
    },
    Assembly(&'a cad_assembly::BuiltAssembly),
}
pub fn query_section(
    source: SectionSource<'_>,
    plane: SectionPlane,
    tolerance: Tolerance,
) -> Result<SectionQuery, BuildError> {
    let fail = |message: String| BuildError::SectionQuery { message };
    plane.validate().map_err(|e| fail(e.to_string()))?;
    if !tolerance.is_valid() {
        return Err(fail("invalid section tolerance".into()));
    }
    match source {
        SectionSource::Body {
            model,
            feature_id,
            content_hash,
        } => {
            let s = model
                .section(plane, tolerance)
                .map_err(|e| fail(e.to_string()))?;
            Ok(SectionQuery {
                kind: "body".into(),
                feature_id: feature_id.into(),
                content_hash: content_hash.into(),
                section: SectionSummary {
                    plane,
                    area: s.area,
                    centroid: s.centroid,
                    loops: Some(s.loops),
                },
                occurrences: vec![],
            })
        }
        SectionSource::Assembly(a) => {
            a.validate(tolerance).map_err(|e| fail(e.to_string()))?;
            let mut occurrences = vec![];
            let mut area = 0.;
            let mut moment = [0.; 2];
            for instance in &a.instances {
                let part = &a.parts[&instance.part];
                let inverse = instance
                    .transform
                    .inverse(tolerance)
                    .map_err(|e| fail(e.to_string()))?;
                let local = SectionPlane {
                    origin: inverse.point(plane.origin),
                    normal: inverse.vector(plane.normal),
                    x_direction: inverse.vector(plane.x_direction),
                };
                let mut section = part.result.model.section(local, tolerance).map_err(|e| {
                    fail(format!(
                        "occurrence {:?}: {e}",
                        a.instance_paths[&instance.id]
                    ))
                })?;
                section.plane = plane;
                if let Some(c) = section.centroid {
                    for i in 0..2 {
                        moment[i] += section.area * c[i];
                    }
                }
                area += section.area;
                occurrences.push(SectionOccurrence {
                    instance_id: instance.id.clone(),
                    instance_path: a.instance_paths[&instance.id].clone(),
                    part_id: instance.part.clone(),
                    content_hash: part.content_hash.clone(),
                    section,
                });
            }
            if !area.is_finite() || moment.iter().any(|v| !v.is_finite()) {
                return Err(fail("section aggregation exceeds numeric range".into()));
            }
            Ok(SectionQuery {
                kind: "assembly".into(),
                feature_id: a.assembly_id.clone(),
                content_hash: a.content_hash.clone(),
                section: SectionSummary {
                    plane,
                    area,
                    centroid: if area > 0. {
                        Some(moment.map(|v| v / area))
                    } else {
                        None
                    },
                    loops: None,
                },
                occurrences,
            })
        }
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version == 1 && !recipe.section_views.is_empty() {
        return Err(BuildError::SectionViewSchema);
    }
    if recipe.section_views.len() > MAX_SECTION_VIEWS {
        return Err(BuildError::ResourceLimit {
            resource: "section views",
            limit: MAX_SECTION_VIEWS,
        });
    }
    if recipe
        .section_views
        .iter()
        .map(|v| v.dimensions.len() + v.circle_dimensions.len())
        .sum::<usize>()
        > MAX_SECTION_DIMENSIONS
    {
        return Err(BuildError::ResourceLimit {
            resource: "section dimensions",
            limit: MAX_SECTION_DIMENSIONS,
        });
    }
    let outputs: BTreeSet<_> = recipe
        .outputs
        .iter()
        .map(|o| &o.name)
        .chain(recipe.assembly_outputs.iter().map(|o| &o.name))
        .collect();
    let mut ids = BTreeSet::new();
    for v in &recipe.section_views {
        v.validate()?;
        if (v.anchor.is_some() || !v.dimensions.is_empty())
            && !recipe.assembly_outputs.iter().any(|o| o.name == v.output)
        {
            return Err(BuildError::InvalidSectionView {
                id: v.id.clone(),
                message: "anchored or dimensioned section target must be an assembly output".into(),
            });
        }
        let assembly = recipe.assembly_outputs.iter().any(|o| o.name == v.output);
        if v.circle_dimensions
            .iter()
            .any(|d| d.instance_path.is_some() != assembly)
        {
            return Err(BuildError::InvalidSectionView{id:v.id.clone(),message:"circle dimensions require an exact leaf path for assembly outputs and no path for bodies".into()});
        }
        if !ids.insert(v.id.to_ascii_lowercase()) || !outputs.contains(&v.output) {
            return Err(BuildError::InvalidSectionView {
                id: v.id.clone(),
                message: "duplicate view ID or unknown target output".into(),
            });
        }
    }
    Ok(())
}
fn resolve_anchor(
    view: &SectionView,
    face_datums: &ResolvedFaceDatums,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    tolerance: Tolerance,
) -> Result<Option<SectionAnchorWitness>, BuildError> {
    let Some(anchor) = &view.anchor else {
        return Ok(None);
    };
    let fail = |message: String| BuildError::SectionViewFailed {
        id: view.id.clone(),
        output: view.output.clone(),
        message,
        actual_area: None,
    };
    let assembly = assemblies
        .get(&view.output)
        .ok_or_else(|| fail("anchor requires resolved assembly output".into()))?;
    let mut matches = assembly
        .instances
        .iter()
        .filter(|i| assembly.instance_paths.get(&i.id) == Some(&anchor.instance_path));
    let instance = matches.next().ok_or_else(|| {
        fail(format!(
            "anchor leaf path {:?} does not exist",
            anchor.instance_path
        ))
    })?;
    if matches.next().is_some() {
        return Err(fail("anchor path is ambiguous".into()));
    }
    let part = &assembly.parts[&instance.part];
    let datum_frame = *part.datums.get(&anchor.datum).ok_or_else(|| {
        fail(format!(
            "datum '{}' does not exist on anchor part '{}'",
            anchor.datum, instance.part
        ))
    })?;
    let occurrence_frame = instance.transform;
    let world_frame = occurrence_frame
        .compose(&datum_frame, tolerance)
        .map_err(|e| fail(e.to_string()))?;
    let resolved_plane = SectionPlane {
        origin: world_frame.point(view.origin),
        normal: world_frame.vector(view.normal),
        x_direction: world_frame.vector(view.x_direction),
    };
    resolved_plane.validate().map_err(|e| fail(e.to_string()))?;
    Ok(Some(SectionAnchorWitness {
        instance_id: instance.id.clone(),
        instance_path: anchor.instance_path.clone(),
        part_id: instance.part.clone(),
        part_content_hash: part.content_hash.clone(),
        datum: anchor.datum.clone(),
        datum_frame,
        occurrence_frame,
        world_frame,
        resolved_plane,
        semantic_datum: face_datums
            .get(&instance.part)
            .and_then(|d| d.get(&anchor.datum))
            .cloned(),
    }))
}

fn dimensions(
    view: &SectionView,
    plane: SectionPlane,
    face_datums: &ResolvedFaceDatums,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    tolerance: Tolerance,
) -> Result<Vec<SectionDimensionReport>, BuildError> {
    let mut reports = vec![];
    let (n, x) = (plane.normal, plane.x_direction);
    let y = [
        n[1] * x[2] - n[2] * x[1],
        n[2] * x[0] - n[0] * x[2],
        n[0] * x[1] - n[1] * x[0],
    ];
    for d in &view.dimensions {
        let fail = |message: String, actual_value| BuildError::SectionDimensionFailed {
            view: view.id.clone(),
            id: d.id.clone(),
            output: view.output.clone(),
            message,
            actual_value,
        };
        let resolve = |anchor: &SectionAnchor| {
            let mut endpoint = view.clone();
            endpoint.anchor = Some(anchor.clone());
            endpoint.origin = [0.; 3];
            endpoint.normal = [0., 0., 1.];
            endpoint.x_direction = [1., 0., 0.];
            endpoint.dimensions.clear();
            resolve_anchor(&endpoint, face_datums, assemblies, tolerance)
                .map_err(|e| fail(e.to_string(), None))?
                .ok_or_else(|| fail("missing dimension endpoint witness".into(), None))
        };
        let from_witness = resolve(&d.from)?;
        let to_witness = resolve(&d.to)?;
        let project = |p: [f64; 3]| {
            let delta = std::array::from_fn::<_, 3, _>(|i| p[i] - plane.origin[i]);
            [0, 1].map(|i| {
                delta
                    .iter()
                    .zip(if i == 0 { x } else { y })
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
            })
        };
        let from_point = project(from_witness.world_frame.translation);
        let to_point = project(to_witness.world_frame.translation);
        let axis = match d.axis {
            SectionDimensionAxis::X => 0,
            SectionDimensionAxis::Y => 1,
        };
        let direction = if axis == 0 { x } else { y };
        let signed_delta_mm = (0..3)
            .map(|i| {
                (to_witness.world_frame.translation[i] - from_witness.world_frame.translation[i])
                    * direction[i]
            })
            .sum::<f64>();
        let value_mm = signed_delta_mm.abs();
        if !d.offset.is_finite()
            || from_point
                .into_iter()
                .chain(to_point)
                .any(|v| !v.is_finite())
            || !value_mm.is_finite()
        {
            return Err(fail(
                "dimension projection exceeds numeric range".into(),
                None,
            ));
        }
        let scale = from_witness
            .world_frame
            .translation
            .into_iter()
            .chain(to_witness.world_frame.translation)
            .chain(plane.origin)
            .map(f64::abs)
            .fold(value_mm, f64::max);
        if scale * 128. * f64::EPSILON > tolerance.linear / 16.
            || ((to_point[axis] - from_point[axis]) - signed_delta_mm).abs()
                > scale.max(1.) * 128. * f64::EPSILON
        {
            return Err(fail(
                "dimension projection exceeds modeling precision".into(),
                None,
            ));
        }
        if value_mm <= tolerance.linear {
            return Err(fail(
                "projected dimension is degenerate at modeling tolerance".into(),
                Some(value_mm),
            ));
        }
        if d.min.is_some_and(|a| value_mm < a) || d.max.is_some_and(|a| value_mm > a) {
            return Err(fail(
                "projected dimension lies outside declared bounds".into(),
                Some(value_mm),
            ));
        }
        reports.push(SectionDimensionReport {
            tolerance: d
                .tolerance
                .as_ref()
                .map(|spec| spec.report(value_mm, tolerance))
                .transpose()
                .map_err(|message| fail(message, Some(value_mm)))?,
            declaration: d.clone(),
            status: "passed".into(),
            value_mm,
            signed_delta_mm,
            from_point,
            to_point,
            from_witness,
            to_witness,
        });
    }
    Ok(reports)
}

fn complete_circle(
    curves: &[&cad_kernel::SectionCurve],
    tolerance: Tolerance,
) -> Result<([f64; 2], f64), String> {
    use cad_kernel::SectionCurve;
    let Some(SectionCurve::Arc {
        center,
        radius,
        sweep_angle,
        ..
    }) = curves.first().copied()
    else {
        return Err("selected face does not section as a circle".into());
    };
    let (center, radius, sign) = (*center, *radius, sweep_angle.signum());
    let scale = center
        .into_iter()
        .map(f64::abs)
        .fold(radius, f64::max)
        .max(1.);
    let error = scale * 128. * f64::EPSILON;
    if error > tolerance.linear / 16. || !radius.is_normal() || radius <= tolerance.linear {
        return Err("circle support exceeds modeling precision".into());
    }
    let mut sweep = 0.;
    let mut first = None;
    let mut last = None;
    for curve in curves {
        let SectionCurve::Arc {
            center: c,
            radius: r,
            start_angle: a,
            sweep_angle: d,
        } = **curve
        else {
            return Err("selected section contains a noncircular segment".into());
        };
        if c.into_iter().chain([r, a, d]).any(|v| !v.is_finite())
            || (c[0] - center[0]).hypot(c[1] - center[1]) > error
            || (r - radius).abs() > error
            || d.signum() != sign
            || d == 0.
            || d.abs() > std::f64::consts::TAU + 128. * f64::EPSILON
        {
            return Err("selected arcs do not share one oriented circle support".into());
        }
        let point = |angle: f64| [radius * angle.cos(), radius * angle.sin()];
        let start = point(a);
        let end = point(a + d);
        if let Some(p) = last {
            let p: [f64; 2] = p;
            if (p[0] - start[0]).hypot(p[1] - start[1]) > error {
                return Err("selected arcs are not a continuous circle".into());
            }
        }
        first.get_or_insert(start);
        last = Some(end);
        sweep += d;
    }
    let a = first.unwrap();
    let b = last.unwrap();
    if (sweep.abs() - std::f64::consts::TAU).abs() > 128. * f64::EPSILON * curves.len() as f64
        || (a[0] - b[0]).hypot(a[1] - b[1]) > error
    {
        return Err("selected section is partial or has duplicate winding".into());
    }
    Ok((center, radius))
}
// A semantic cylindrical wall may split into several descendants. Measure a
// complete native section loop, never concatenate unrelated loops or silently
// drop noncircular portions of the selected boundary.
fn selected_complete_circle(
    loops: &[SectionLoop],
    faces: &std::collections::BTreeSet<cad_kernel::FaceId>,
    tolerance: Tolerance,
) -> Result<([f64; 2], f64), String> {
    let mut selected = loops.iter().filter(|l| {
        l.segments
            .iter()
            .any(|segment| faces.contains(&segment.face))
    });
    let boundary = selected
        .next()
        .ok_or_else(|| "selected faces have no native section loop".to_string())?;
    if selected.next().is_some() {
        return Err("selected faces intersect more than one native section loop".into());
    }
    if boundary
        .segments
        .iter()
        .any(|segment| !faces.contains(&segment.face))
    {
        return Err("selected faces do not cover one complete native section loop".into());
    }
    let curves: Vec<_> = boundary
        .segments
        .iter()
        .map(|segment| &segment.curve)
        .collect();
    complete_circle(&curves, tolerance)
}

fn circle_dimensions(
    view: &SectionView,
    query: &SectionQuery,
    build: &BuildResult,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    tolerance: Tolerance,
    selection_work: &mut usize,
    work: &mut usize,
) -> Result<Vec<SectionCircleDimensionReport>, BuildError> {
    let mut reports = vec![];
    for d in &view.circle_dimensions {
        let fail = |message: String, actual_value| BuildError::SectionDimensionFailed {
            view: view.id.clone(),
            id: d.id.clone(),
            output: view.output.clone(),
            message,
            actual_value,
        };
        let (target, hash, loops, instance_id, instance_path, part_id, occurrence_frame) =
            if let Some(path) = &d.instance_path {
                let a = &assemblies[&view.output];
                *work =
                    work.saturating_add(a.instances.len().saturating_add(query.occurrences.len()));
                if *work > MAX_SECTION_VIEW_WORK {
                    return Err(BuildError::ResourceLimit {
                        resource: "section view geometry work",
                        limit: MAX_SECTION_VIEW_WORK,
                    });
                }
                let mut matches = a
                    .instances
                    .iter()
                    .filter(|i| a.instance_paths.get(&i.id) == Some(path));
                let instance = matches.next().ok_or_else(|| {
                    fail(
                        "circle dimension occurrence path does not exist".into(),
                        None,
                    )
                })?;
                if matches.next().is_some() {
                    return Err(fail("ambiguous circle dimension path".into(), None));
                }
                let part = &a.parts[&instance.part];
                let section = query
                    .occurrences
                    .iter()
                    .find(|s| s.instance_id == instance.id)
                    .ok_or_else(|| fail("missing occurrence section".into(), None))?;
                (
                    &part.feature_id,
                    &part.content_hash,
                    section.section.loops.as_slice(),
                    Some(instance.id.clone()),
                    Some(path.clone()),
                    Some(instance.part.clone()),
                    Some(instance.transform),
                )
            } else {
                (
                    &query.feature_id,
                    &query.content_hash,
                    query
                        .section
                        .loops
                        .as_deref()
                        .ok_or_else(|| fail("body section missing native loops".into(), None))?,
                    None,
                    None,
                    None,
                    None,
                )
            };
        *work = work.saturating_add(loops.iter().map(|l| l.segments.len()).sum::<usize>());
        if *work > MAX_SECTION_VIEW_WORK {
            return Err(BuildError::ResourceLimit {
                resource: "section view geometry work",
                limit: MAX_SECTION_VIEW_WORK,
            });
        }
        let selection = build.resolve_faces_with_remaining(
            &FaceSelector {
                anchor_feature: d.anchor_feature.clone(),
                role: d.role.clone(),
                target_feature: target.clone(),
                cardinality: FaceCardinality::All,
            },
            selection_work,
        );
        if selection.status != FaceResolutionStatus::Resolved || selection.witnesses.is_empty() {
            return Err(fail(
                format!(
                    "circle face selection {:?}: {}",
                    selection.status, selection.diagnostic
                ),
                None,
            ));
        }
        let faces = selection.witnesses.iter().map(|w| w.target_face).collect();
        let (center, radius) =
            selected_complete_circle(loops, &faces, tolerance).map_err(|e| fail(e, None))?;
        let label_distance = d.label_offset[0].hypot(d.label_offset[1]);
        if !label_distance.is_finite()
            || label_distance <= radius + tolerance.linear
            || (0..2).any(|i| !(center[i] + d.label_offset[i]).is_finite())
        {
            return Err(fail(
                "circle label offset must place a finite label outside the circle".into(),
                None,
            ));
        }
        let value_mm = radius
            * match d.kind {
                SectionCircleDimensionKind::Radius => 1.,
                SectionCircleDimensionKind::Diameter => 2.,
            };
        if !value_mm.is_finite() {
            return Err(fail(
                "circle measurement exceeds numeric range".into(),
                None,
            ));
        }
        if d.min.is_some_and(|v| value_mm < v) || d.max.is_some_and(|v| value_mm > v) {
            return Err(fail(
                "circle dimension lies outside declared bounds".into(),
                Some(value_mm),
            ));
        }
        reports.push(SectionCircleDimensionReport {
            tolerance: d
                .tolerance
                .as_ref()
                .map(|spec| spec.report(value_mm, tolerance))
                .transpose()
                .map_err(|message| fail(message, Some(value_mm)))?,
            declaration: d.clone(),
            status: "passed".into(),
            value_mm,
            center,
            radius,
            selection,
            anchor_content_hash: build.feature_hashes.get(&d.anchor_feature).cloned(),
            target_content_hash: hash.clone(),
            instance_id,
            instance_path,
            part_id,
            occurrence_frame,
        });
    }
    Ok(reports)
}

pub(super) fn evaluate(
    recipe: &Recipe,
    build: &BuildResult,
    models: &HashMap<String, Arc<OperationResult>>,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    tolerance: Tolerance,
    selection_work: &mut usize,
) -> Result<BTreeMap<String, SectionViewReport>, BuildError> {
    let face_datums = &build.face_datums;
    let hashes = &build.feature_hashes;
    let mut reports = BTreeMap::new();
    let mut work = 0usize;
    for v in &recipe.section_views {
        let source = if let Some(o) = recipe.outputs.iter().find(|o| o.name == v.output) {
            SectionSource::Body {
                model: &models[&o.feature].model,
                feature_id: &o.feature,
                content_hash: &hashes[&o.feature],
            }
        } else {
            SectionSource::Assembly(&assemblies[&v.output])
        };
        let charge = |m: &cad_kernel::ModelSnapshot| {
            m.vertices
                .len()
                .saturating_add(m.edges.len())
                .saturating_add(m.faces.len())
                .saturating_add(m.loops.len())
                .saturating_add(m.shells.len())
                .saturating_add(m.solids.len())
                .saturating_add(
                    m.loops
                        .iter()
                        .fold(0usize, |n, l| n.saturating_add(l.edges.len())),
                )
                .saturating_add(
                    m.shells
                        .iter()
                        .fold(0usize, |n, s| n.saturating_add(s.faces.len())),
                )
        };
        work = work.saturating_add(match &source {
            SectionSource::Body { model, .. } => charge(model),
            SectionSource::Assembly(a) => a.instances.iter().fold(a.instances.len(), |n, i| {
                n.saturating_add(charge(&a.parts[&i.part].result.model))
            }),
        });
        if let SectionSource::Assembly(a) = &source {
            work = work.saturating_add(
                a.instances
                    .len()
                    .saturating_mul(v.dimensions.len().saturating_mul(2)),
            );
        }
        if work > MAX_SECTION_VIEW_WORK {
            return Err(BuildError::ResourceLimit {
                resource: "section view geometry work",
                limit: MAX_SECTION_VIEW_WORK,
            });
        }
        let anchor_witness = resolve_anchor(v, face_datums, assemblies, tolerance)?;
        let plane = anchor_witness
            .as_ref()
            .map_or(v.plane(), |a| a.resolved_plane);
        let query =
            query_section(source, plane, tolerance).map_err(|e| BuildError::SectionViewFailed {
                id: v.id.clone(),
                output: v.output.clone(),
                message: e.to_string(),
                actual_area: None,
            })?;
        let area = query.section.area;
        if v.min_area.is_some_and(|a| area < a) || v.max_area.is_some_and(|a| area > a) {
            return Err(BuildError::SectionViewFailed {
                id: v.id.clone(),
                output: v.output.clone(),
                message: "section area lies outside declared bounds".into(),
                actual_area: Some(area),
            });
        }
        let dimensions = dimensions(v, plane, face_datums, assemblies, tolerance)?;
        let circle_dimensions = circle_dimensions(
            v,
            &query,
            build,
            assemblies,
            tolerance,
            selection_work,
            &mut work,
        )?;
        reports.insert(
            v.id.clone(),
            SectionViewReport {
                declaration: v.clone(),
                status: "passed".into(),
                query,
                anchor_witness,
                dimensions,
                circle_dimensions,
            },
        );
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe() -> Recipe {
        serde_json::from_value(serde_json::json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":10,"depth":12},{"op":"extrude","id":"b","profile":"p","height":8},{"op":"translate","id":"other","body":"b","offset":[20,0,0]}],"outputs":[{"name":"body","feature":"b"},{"name":"other","feature":"other"}],"section_views":[{"id":"drawing","output":"other","origin":[0,0,4],"min_area":119,"max_area":121}]})).unwrap()
    }
    #[test]
    fn global_sections_demand_unselected_targets_and_recheck_warm_cache_bounds() {
        let mut r = recipe();
        let mut engine = BuildEngine::default();
        let a = engine.build_outputs(&r, &["body".into()]).unwrap();
        assert!(!a.outputs.contains_key("other"));
        assert!(a.scope.validation_roots.contains(&"other".into()));
        assert_eq!(a.section_views["drawing"].query.section.area, 120.);
        assert_eq!(
            a.section_views["drawing"].query.section.centroid,
            Some([25., 6.])
        );
        assert_eq!(
            a.section_views["drawing"].query.content_hash,
            a.feature_hashes["other"]
        );
        r.section_views[0].max_area = Some(119.5);
        assert!(matches!(
            engine.build_outputs(&r, &["body".into()]),
            Err(BuildError::SectionViewFailed {
                actual_area: Some(120.),
                ..
            })
        ));
        r.section_views[0].max_area = None;
        r.section_views[0].origin[2] = 0.;
        assert!(matches!(
            engine.build(&r),
            Err(BuildError::SectionViewFailed {
                actual_area: None,
                ..
            })
        ));
    }
    fn anchored_recipe() -> Recipe {
        let mut value = serde_json::to_value(recipe()).unwrap();
        value["parts"] = serde_json::json!([{"id":"part","feature":"b","datums":{"center":{"translation":[5,6,4],"rotation":[1,0,0,0]}}}]);
        value["assemblies"] = serde_json::json!([
            {"id":"child","instances":[{"id":"leaf","part":"part","transform":{"translation":[2,3,0],"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]}}]},
            {"id":"root","assemblies":[{"id":"left","assembly":"child","transform":{"translation":[-40,0,0]}},{"id":"right","assembly":"child","transform":{"translation":[40,0,0],"rotation":[0,0,std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]}}]}
        ]);
        value["assembly_outputs"] = serde_json::json!([{"name":"machine","assembly":"root"}]);
        value["section_views"] = serde_json::json!([{"id":"local","output":"machine","origin":[0,0,0],"anchor":{"instance_path":["right","leaf"],"datum":"center"},"min_area":239.99999,"max_area":240.00001}]);
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn nested_exact_path_anchor_composes_datum_and_tracks_cached_placement_edits() {
        let mut r = anchored_recipe();
        let mut engine = BuildEngine::default();
        let first = engine.build_outputs(&r, &["body".into()]).unwrap();
        let report = &first.section_views["local"];
        let witness = report.anchor_witness.as_ref().unwrap();
        assert_eq!(witness.instance_path, vec!["right", "leaf"]);
        assert_eq!(witness.part_id, "part");
        for (actual, expected) in witness.resolved_plane.normal.into_iter().zip([0., 0., -1.]) {
            assert!((actual - expected).abs() <= 128. * f64::EPSILON);
        }
        assert_eq!(report.declaration.origin, [0., 0., 0.]);
        assert_eq!(report.query.section.plane, witness.resolved_plane);
        assert!((report.query.section.area - 240.).abs() < 1e-8);
        let old = witness.resolved_plane.origin;
        r.assemblies[1].assemblies[1].transform.translation[0] += 7.;
        let second = engine.build_outputs(&r, &["body".into()]).unwrap();
        assert_eq!(first.feature_hashes, second.feature_hashes);
        assert_eq!(
            second.section_views["local"]
                .anchor_witness
                .as_ref()
                .unwrap()
                .part_content_hash,
            witness.part_content_hash
        );
        assert!(
            (second.section_views["local"].query.section.plane.origin[0] - old[0] - 7.).abs()
                < 1e-8
        );
        assert_ne!(
            report.query.content_hash,
            second.section_views["local"].query.content_hash
        );
        r.parts[0].datums.clear();
        assert!(matches!(
            engine.build_outputs(&r, &["body".into()]),
            Err(BuildError::SectionViewFailed { .. })
        ));
    }
    #[test]
    fn associative_anchor_retains_native_face_selection_evidence() {
        let mut r = anchored_recipe();
        r.parts[0].datums.clear();
        r.face_datums.push(FaceDatumDefinition {
            part: "part".into(),
            name: "center".into(),
            anchor_feature: "b".into(),
            role: "end_cap".into(),
            x_direction: [1., 0., 0.],
            offset: [0.; 3],
        });
        r.section_views[0].origin = [0., 0., -4.];
        let result = build(&r).unwrap();
        let witness = result.section_views["local"]
            .anchor_witness
            .as_ref()
            .unwrap();
        let semantic = witness.semantic_datum.as_ref().unwrap();
        assert_eq!(semantic.target_content_hash, witness.part_content_hash);
        assert_eq!(semantic.frame, witness.datum_frame);
        assert_eq!(semantic.selection.status, FaceResolutionStatus::Resolved);
        assert_eq!(semantic.definition.role, "end_cap");
        assert!((result.section_views["local"].query.section.area - 240.).abs() < 1e-8);
        r.face_datums[0].role = "missing_role".into();
        assert!(build(&r).is_err());
    }
    #[test]
    fn anchor_missing_path_datum_and_malformed_options_fail() {
        for mutation in 0..7 {
            let mut r = anchored_recipe();
            let v = &mut r.section_views[0];
            match mutation {
                0 => v.anchor.as_mut().unwrap().instance_path = vec!["leaf".into()],
                1 => v.anchor.as_mut().unwrap().datum = "deleted".into(),
                2 => v.anchor.as_mut().unwrap().instance_path = vec!["..".into(), "leaf".into()],
                3 => v.anchor.as_mut().unwrap().instance_path = vec![],
                4 => v.anchor.as_mut().unwrap().instance_path = vec!["right".into(); 65],
                5 => v.anchor.as_mut().unwrap().datum = "bad ".into(),
                _ => v.output = "body".into(),
            }
            assert!(build(&r).is_err(), "mutation {mutation}");
        }
        let mut value = serde_json::to_value(anchored_recipe()).unwrap();
        value["section_views"][0]["anchor"]["unknown"] = true.into();
        assert!(serde_json::from_value::<Recipe>(value).is_err());
    }
    fn dimension() -> SectionDimension {
        SectionDimension {
            tolerance: None,
            id: "spacing".into(),
            from: SectionAnchor {
                instance_path: vec!["left".into(), "leaf".into()],
                datum: "center".into(),
            },
            to: SectionAnchor {
                instance_path: vec!["right".into(), "leaf".into()],
                datum: "center".into(),
            },
            axis: SectionDimensionAxis::X,
            offset: -30.,
            min: Some(75.99999),
            max: Some(76.00001),
        }
    }
    #[test]
    fn projected_dimensions_follow_exact_paths_rotated_frames_and_cache_edits() {
        let mut r = anchored_recipe();
        r.section_views[0].dimensions = vec![dimension()];
        let mut engine = BuildEngine::default();
        let a = engine.build_outputs(&r, &["body".into()]).unwrap();
        let d = &a.section_views["local"].dimensions[0];
        assert!((d.value_mm - 76.).abs() < 1e-10);
        assert!((d.signed_delta_mm + 76.).abs() < 1e-10);
        assert_eq!(d.from_witness.instance_path, vec!["left", "leaf"]);
        assert_eq!(d.to_witness.instance_path, vec!["right", "leaf"]);
        assert!(d.from_point[0] > 75.99);
        assert!(d.to_point[0].abs() < 1e-10);
        let declaration = &mut r.section_views[0].dimensions[0];
        std::mem::swap(&mut declaration.from, &mut declaration.to);
        declaration.offset = 40.;
        let b = engine.build_outputs(&r, &["body".into()]).unwrap();
        assert_eq!(b.stats.computed_solids, 0);
        assert_eq!(a.feature_hashes, b.feature_hashes);
        assert!((b.section_views["local"].dimensions[0].signed_delta_mm - 76.).abs() < 1e-10);
        r.section_views[0].x_direction = [0., 1., 0.];
        r.section_views[0].dimensions[0].min = Some(11.99999);
        r.section_views[0].dimensions[0].max = Some(12.00001);
        let c = engine.build(&r).unwrap();
        assert!((c.section_views["local"].dimensions[0].value_mm - 12.).abs() < 1e-10);
        r.section_views[0].dimensions[0].max = Some(11.);
        r.section_views[0].dimensions[0].min = None;
        assert!(matches!(
            engine.build_outputs(&r, &["body".into()]),
            Err(BuildError::SectionDimensionFailed {
                actual_value: Some(_),
                ..
            })
        ));
    }
    #[test]
    fn dimensions_carry_semantic_witness_and_reject_missing_or_degenerate_endpoints() {
        let mut r = anchored_recipe();
        r.parts[0].datums.clear();
        r.face_datums.push(FaceDatumDefinition {
            part: "part".into(),
            name: "center".into(),
            anchor_feature: "b".into(),
            role: "end_cap".into(),
            x_direction: [1., 0., 0.],
            offset: [0.; 3],
        });
        r.section_views[0].origin = [0., 0., -4.];
        r.section_views[0].dimensions = vec![dimension()];
        let b = build(&r).unwrap();
        let d = &b.section_views["local"].dimensions[0];
        assert!(d.from_witness.semantic_datum.is_some());
        assert!(d.to_witness.semantic_datum.is_some());
        // Datum origins sit 4mm above the section, but projected distance remains76mm.
        assert!((d.value_mm - 76.).abs() < 1e-10);
        let endpoint = r.section_views[0].dimensions[0].from.clone();
        r.section_views[0].dimensions[0].to = endpoint;
        assert!(
            matches!(build(&r),Err(BuildError::SectionDimensionFailed{actual_value:Some(v),..}) if v<=1e-7)
        );
        r.section_views[0].dimensions[0].to.instance_path = vec!["missing".into()];
        assert!(matches!(
            build(&r),
            Err(BuildError::SectionDimensionFailed {
                actual_value: None,
                ..
            })
        ));
    }
    #[test]
    fn dimension_schema_limits_and_body_targets_fail_preflight() {
        for mutation in 0..7 {
            let mut r = anchored_recipe();
            r.section_views[0].dimensions = vec![dimension()];
            match mutation {
                0 => r.section_views[0].dimensions[0].offset = f64::NAN,
                1 => r.section_views[0].dimensions[0].min = Some(-1.),
                2 => r.section_views[0].dimensions[0].id = "../bad".into(),
                3 => {
                    let mut d = dimension();
                    d.id = "SPACING".into();
                    r.section_views[0].dimensions.push(d);
                }
                4 => r.section_views[0].dimensions = vec![dimension(); 33],
                5 => {
                    r.section_views[0].anchor = None;
                    r.section_views[0].output = "body".into();
                }
                _ => r.section_views[0].dimensions[0].from.instance_path.clear(),
            }
            assert!(build(&r).is_err(), "mutation {mutation}");
        }
        let mut over = anchored_recipe();
        let template = over.section_views[0].clone();
        over.section_views = (0..33)
            .map(|i| {
                let mut view = template.clone();
                view.id = format!("view_{i}");
                view.dimensions = (0..32)
                    .map(|j| {
                        let mut d = dimension();
                        d.id = format!("dimension_{j}");
                        d
                    })
                    .collect();
                view
            })
            .collect();
        assert!(matches!(
            preflight(&over),
            Err(BuildError::ResourceLimit {
                resource: "section dimensions",
                limit: 1024
            })
        ));
        let mut r = anchored_recipe();
        r.section_views[0].dimensions = vec![dimension()];
        let mut v = serde_json::to_value(r).unwrap();
        v["section_views"][0]["dimensions"][0]["axis"] = "z".into();
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    fn circle_dimension() -> SectionCircleDimension {
        SectionCircleDimension {
            tolerance: None,
            id: "diameter".into(),
            anchor_feature: "b".into(),
            role: "wall".into(),
            instance_path: None,
            kind: SectionCircleDimensionKind::Diameter,
            label_offset: [10., 10.],
            min: None,
            max: None,
        }
    }
    fn circle_recipe() -> Recipe {
        let mut v = serde_json::to_value(recipe()).unwrap();
        v["features"][0] = serde_json::json!({"op":"circle","id":"p","radius":3});
        v["section_views"] = serde_json::json!([{"id":"circle","output":"body","origin":[0,0,4],"circle_dimensions":[circle_dimension()]}]);
        serde_json::from_value(v).unwrap()
    }
    #[test]
    fn semantic_circle_dimensions_track_native_radius_and_global_cache_obligations() {
        let mut r = circle_recipe();
        let mut engine = BuildEngine::default();
        let a = engine.build_outputs(&r, &["other".into()]).unwrap();
        let d = &a.section_views["circle"].circle_dimensions[0];
        assert_eq!(d.value_mm, 6.);
        assert_eq!(d.radius, 3.);
        assert_eq!(d.center, [0., 0.]);
        assert_eq!(d.selection.status, FaceResolutionStatus::Resolved);
        assert!(d.instance_id.is_none());
        assert_eq!(d.target_content_hash, a.feature_hashes["b"]);
        r.section_views[0].circle_dimensions[0].max = Some(5.);
        assert!(matches!(
            engine.build_outputs(&r, &["other".into()]),
            Err(BuildError::SectionDimensionFailed {
                actual_value: Some(6.),
                ..
            })
        ));
        r.section_views[0].circle_dimensions[0].max = None;
        if let Feature::Circle { radius, .. } = &mut r.features[0] {
            *radius = 4.;
        } else {
            panic!()
        };
        let b = engine.build(&r).unwrap();
        assert_eq!(b.section_views["circle"].circle_dimensions[0].value_mm, 8.);
        assert_ne!(a.feature_hashes, b.feature_hashes);
        for role in ["missing", "boundary", "end_cap"] {
            r.section_views[0].circle_dimensions[0].role = role.into();
            assert!(matches!(
                engine.build(&r),
                Err(BuildError::SectionDimensionFailed { .. })
            ));
        }
    }
    #[test]
    fn circle_dimensions_resolve_exact_repeated_rotated_occurrence() {
        let mut value = serde_json::to_value(anchored_recipe()).unwrap();
        value["features"][0] = serde_json::json!({"op":"circle","id":"p","radius":3});
        let mut r: Recipe = serde_json::from_value(value).unwrap();
        r.section_views[0].min_area = None;
        r.section_views[0].max_area = None;
        let mut d = circle_dimension();
        d.instance_path = Some(vec!["right".into(), "leaf".into()]);
        d.kind = SectionCircleDimensionKind::Radius;
        r.section_views[0].circle_dimensions = vec![d];
        let b = build(&r).unwrap();
        let report = &b.section_views["local"].circle_dimensions[0];
        assert_eq!(report.value_mm, 3.);
        assert_eq!(
            report.instance_path,
            Some(vec!["right".into(), "leaf".into()])
        );
        assert_eq!(report.part_id.as_deref(), Some("part"));
        assert!(report.occurrence_frame.is_some());
        r.section_views[0].circle_dimensions[0].instance_path = Some(vec!["leaf".into()]);
        assert!(build(&r).is_err());
    }
    #[test]
    fn complete_circle_certificate_rejects_partial_disconnected_lines_and_duplicate_winding() {
        use cad_kernel::SectionCurve as C;
        let arc = |a, d| C::Arc {
            center: [0., 0.],
            radius: 3.,
            start_angle: a,
            sweep_angle: d,
        };
        let t = Tolerance::default();
        let pi = std::f64::consts::PI;
        assert!(complete_circle(&[&arc(0., pi), &arc(pi, pi)], t).is_ok());
        for curves in [
            vec![arc(0., pi)],
            vec![arc(0., pi), arc(0., pi)],
            vec![arc(0., 2. * pi), arc(0., 2. * pi)],
            vec![arc(0., pi), arc(pi, -pi)],
            vec![C::Line {
                start: [0., 0.],
                end: [1., 0.],
            }],
        ] {
            assert!(complete_circle(&curves.iter().collect::<Vec<_>>(), t).is_err());
        }
        let mut r = circle_recipe();
        r.section_views[0].circle_dimensions[0].label_offset = [1., 0.];
        assert!(build(&r).is_err());
        r.section_views[0].circle_dimensions[0].label_offset = [10., 0.];
        r.section_views[0].dimensions = vec![dimension()];
        r.section_views[0].dimensions[0].id = "DIAMETER".into();
        assert!(preflight(&r).is_err());
    }
    #[test]
    fn declarations_are_strict_bounded_and_safe() {
        let base = recipe();
        for mutation in 0..8 {
            let mut r = base.clone();
            match mutation {
                0 => r.section_views[0].id = "../escape".into(),
                1 => r.section_views[0].normal = [0., 0., 2.],
                2 => r.section_views[0].x_direction = [0., 0., 1.],
                3 => r.section_views[0].max_area = Some(-1.),
                4 => r.section_views[0].min_area = Some(122.),
                5 => r.section_views[0].output = "missing".into(),
                6 => {
                    let mut v = r.section_views[0].clone();
                    v.id = "DRAWING".into();
                    r.section_views.push(v);
                }
                _ => r.section_views = vec![r.section_views[0].clone(); 129],
            }
            assert!(build(&r).is_err(), "mutation {mutation}");
        }
        let mut v = serde_json::to_value(base).unwrap();
        v["section_views"][0]["unexpected"] = true.into();
        assert!(serde_json::from_value::<Recipe>(v.clone()).is_err());
        v["section_views"] = serde_json::json!([]);
        v["schema_version"] = 1.into();
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    #[test]
    fn authored_linear_tolerances_are_global_nominal_checks_and_cache_safe() {
        let mut r = anchored_recipe();
        let mut d = dimension();
        d.tolerance = Some(DimensionTolerance {
            nominal: 76.,
            lower: 0.1,
            upper: 0.2,
        });
        r.section_views[0].dimensions = vec![d];
        let mut engine = BuildEngine::default();
        let first = engine.build_outputs(&r, &["body".into()]).unwrap();
        let report = first.section_views["local"].dimensions[0]
            .tolerance
            .as_ref()
            .unwrap();
        assert_eq!(report.nominal_mm, 76.);
        assert_eq!(report.lower_limit_mm, 76.1);
        assert_eq!(report.upper_limit_mm, 76.2);
        // Both positive deviations intentionally exclude the ideal design size.
        r.section_views[0].dimensions[0]
            .tolerance
            .as_mut()
            .unwrap()
            .lower = -0.3;
        let edited = engine.build_outputs(&r, &["body".into()]).unwrap();
        assert_eq!(edited.stats.computed_solids, 0);
        assert_eq!(first.feature_hashes, edited.feature_hashes);
        assert_eq!(
            edited.section_views["local"].dimensions[0]
                .tolerance
                .as_ref()
                .unwrap()
                .lower_limit_mm,
            75.7
        );
        r.section_views[0].dimensions[0]
            .tolerance
            .as_mut()
            .unwrap()
            .nominal = 75.9;
        let failure = engine.build_outputs(&r, &["body".into()]).unwrap_err();
        assert!(failure.to_string().contains("nominal"));
        // Missing tolerance remains backward-compatible and omitted from reports.
        r.section_views[0].dimensions[0].tolerance = None;
        let plain = engine.build(&r).unwrap();
        assert!(
            serde_json::to_value(&plain.section_views["local"].dimensions[0])
                .unwrap()
                .get("tolerance")
                .is_none()
        );
    }
    #[test]
    fn authored_circle_tolerances_do_not_relax_nominal_or_design_bounds() {
        let mut r = circle_recipe();
        r.section_views[0].circle_dimensions[0].tolerance = Some(DimensionTolerance {
            nominal: 6.,
            lower: -0.2,
            upper: -0.1,
        });
        let mut engine = BuildEngine::default();
        let first = engine.build_outputs(&r, &["other".into()]).unwrap();
        assert_eq!(
            first.section_views["circle"].circle_dimensions[0]
                .tolerance
                .as_ref()
                .unwrap()
                .upper_limit_mm,
            5.9
        );
        r.section_views[0].circle_dimensions[0]
            .tolerance
            .as_mut()
            .unwrap()
            .upper = 0.1;
        let edited = engine.build_outputs(&r, &["other".into()]).unwrap();
        assert_eq!(edited.stats.computed_solids, 0);
        r.section_views[0].circle_dimensions[0].max = Some(5.99);
        assert!(engine.build(&r).is_err());
        r.section_views[0].circle_dimensions[0].max = None;
        r.section_views[0].circle_dimensions[0]
            .tolerance
            .as_mut()
            .unwrap()
            .nominal = 6.05;
        assert!(engine
            .build_outputs(&r, &["other".into()])
            .unwrap_err()
            .to_string()
            .contains("nominal"));
        r.section_views[0].circle_dimensions[0]
            .tolerance
            .as_mut()
            .unwrap()
            .nominal = 6.;
        r.section_views[0].circle_dimensions[0].kind = SectionCircleDimensionKind::Radius;
        assert!(engine
            .build(&r)
            .unwrap_err()
            .to_string()
            .contains("nominal"));
        r.section_views[0].circle_dimensions[0]
            .tolerance
            .as_mut()
            .unwrap()
            .nominal = 3.;
        assert_eq!(
            engine.build(&r).unwrap().section_views["circle"].circle_dimensions[0]
                .tolerance
                .as_ref()
                .unwrap()
                .nominal_mm,
            3.
        );
    }
}

#[cfg(test)]
mod split_circle_selection_tests {
    use super::*;
    use cad_kernel::{FaceId, SectionCurve, SectionSegment};
    use std::collections::BTreeSet;
    fn ring(face: usize, center: [f64; 2]) -> SectionLoop {
        SectionLoop {
            segments: vec![SectionSegment {
                face: FaceId(face),
                curve: SectionCurve::Arc {
                    center,
                    radius: 3.,
                    start_angle: 0.,
                    sweep_angle: std::f64::consts::TAU,
                },
            }],
        }
    }
    #[test]
    fn multiple_native_loops_remain_ambiguous_even_on_identical_supports() {
        let faces = BTreeSet::from([FaceId(0), FaceId(1)]);
        for center in [[0., 0.], [10., 0.]] {
            assert!(selected_complete_circle(
                &[ring(0, [0., 0.]), ring(1, center)],
                &faces,
                Tolerance::default()
            )
            .is_err());
        }
    }
    #[test]
    fn split_loop_requires_every_segment_and_exactly_one_winding() {
        let mut boundary = ring(0, [0., 0.]);
        boundary.segments[0].curve = SectionCurve::Arc {
            center: [0., 0.],
            radius: 3.,
            start_angle: 0.,
            sweep_angle: std::f64::consts::PI,
        };
        boundary.segments.push(SectionSegment {
            face: FaceId(1),
            curve: SectionCurve::Arc {
                center: [0., 0.],
                radius: 3.,
                start_angle: std::f64::consts::PI,
                sweep_angle: std::f64::consts::PI,
            },
        });
        let faces = BTreeSet::from([FaceId(0), FaceId(1)]);
        assert!(
            selected_complete_circle(&[boundary.clone()], &faces, Tolerance::default()).is_ok()
        );
        assert!(selected_complete_circle(
            &[boundary.clone()],
            &BTreeSet::from([FaceId(0)]),
            Tolerance::default()
        )
        .is_err());
        boundary.segments.extend(boundary.segments.clone());
        assert!(selected_complete_circle(&[boundary], &faces, Tolerance::default()).is_err());
    }
}
