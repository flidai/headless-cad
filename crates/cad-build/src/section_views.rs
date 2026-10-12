//! Source-authored analytic sections and shared body/assembly query semantics.
use super::*;
use cad_kernel::{PlanarSection, SectionLoop, SectionPlane};
pub const MAX_SECTION_VIEWS: usize = 128;
pub const MAX_SECTION_VIEW_WORK: usize = 10_000_000;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionAnchor {
    pub instance_path: Vec<String>,
    pub datum: String,
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
    let outputs: BTreeSet<_> = recipe
        .outputs
        .iter()
        .map(|o| &o.name)
        .chain(recipe.assembly_outputs.iter().map(|o| &o.name))
        .collect();
    let mut ids = BTreeSet::new();
    for v in &recipe.section_views {
        v.validate()?;
        if v.anchor.is_some() && !recipe.assembly_outputs.iter().any(|o| o.name == v.output) {
            return Err(BuildError::InvalidSectionView {
                id: v.id.clone(),
                message: "anchored section target must be an assembly output".into(),
            });
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

pub(super) fn evaluate(
    recipe: &Recipe,
    face_datums: &ResolvedFaceDatums,
    hashes: &BTreeMap<String, String>,
    models: &HashMap<String, Arc<OperationResult>>,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, SectionViewReport>, BuildError> {
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
        reports.insert(
            v.id.clone(),
            SectionViewReport {
                declaration: v.clone(),
                status: "passed".into(),
                query,
                anchor_witness,
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
}
