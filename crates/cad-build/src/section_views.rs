//! Source-authored analytic sections and shared body/assembly query semantics.
use super::*;
use cad_kernel::{PlanarSection, SectionLoop, SectionPlane};
pub const MAX_SECTION_VIEWS: usize = 128;
pub const MAX_SECTION_VIEW_WORK: usize = 10_000_000;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionView {
    pub id: String,
    pub output: String,
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
        if !ids.insert(v.id.to_ascii_lowercase()) || !outputs.contains(&v.output) {
            return Err(BuildError::InvalidSectionView {
                id: v.id.clone(),
                message: "duplicate view ID or unknown target output".into(),
            });
        }
    }
    Ok(())
}
pub(super) fn evaluate(
    recipe: &Recipe,
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
        let query = query_section(source, v.plane(), tolerance).map_err(|e| {
            BuildError::SectionViewFailed {
                id: v.id.clone(),
                output: v.output.clone(),
                message: e.to_string(),
                actual_area: None,
            }
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
