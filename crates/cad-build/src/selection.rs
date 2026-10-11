//! Semantic intent resolves through operation history, never nearest geometry.
//! This is scoped orthogonal face lineage, not general persistent CAD naming.
use super::*;
use cad_kernel::FaceId;
use std::collections::BTreeSet;

/// Upper bound on traversal, history validation, and retained/copied lineage
/// handles plus copied identifier bytes per query. Exhaustion returns Unsupported
/// with no certified witnesses.
pub const MAX_FACE_SELECTION_WORK: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FaceCardinality {
    Exactly { count: usize },
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceSelector {
    pub anchor_feature: String,
    pub role: String,
    pub target_feature: String,
    pub cardinality: FaceCardinality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceResolutionStatus {
    Resolved,
    Deleted,
    Ambiguous,
    Unsupported,
}

/// Snapshot-local handles are evidence, never stored authoring intent.
#[derive(Debug, Clone, Serialize)]
pub struct FaceWitness {
    pub target_feature: String,
    pub target_face: FaceId,
    pub anchor_faces: Vec<FaceId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FaceLineageStep {
    pub feature: String,
    pub input_feature: String,
    pub input_slot: String,
    pub input_face: FaceId,
    pub output_faces: Vec<FaceId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FaceResolution {
    pub status: FaceResolutionStatus,
    pub selector: FaceSelector,
    pub witnesses: Vec<FaceWitness>,
    pub lineage: Vec<FaceLineageStep>,
    pub diagnostic: String,
}

#[derive(Debug)]
pub(super) struct SelectionGraph {
    nodes: BTreeMap<String, Feature>,
    order: Vec<String>,
    results: HashMap<String, Arc<OperationResult>>,
}
impl SelectionGraph {
    pub(super) fn new(
        recipe: &Recipe,
        order: Vec<usize>,
        results: HashMap<String, Arc<OperationResult>>,
    ) -> Self {
        Self {
            nodes: recipe
                .features
                .iter()
                .map(|f| (f.id().to_owned(), f.clone()))
                .collect(),
            order: order
                .into_iter()
                .map(|i| recipe.features[i].id().to_owned())
                .collect(),
            results,
        }
    }
}

#[derive(Default, Clone)]
struct SelectionState {
    faces: BTreeMap<FaceId, BTreeSet<FaceId>>,
    unsupported: bool,
}

impl BuildResult {
    /// Resolve generated role intent along this build's logical feature DAG.
    /// Absent history is unsupported; an explicit empty mapping means deletion.
    /// `Exactly` enforces positive cardinality; `All` accepts nonempty splits.
    pub fn resolve_faces(&self, selector: &FaceSelector) -> FaceResolution {
        self.resolve_faces_with_budget(selector, MAX_FACE_SELECTION_WORK)
    }

    fn resolve_faces_with_budget(
        &self,
        selector: &FaceSelector,
        mut budget: usize,
    ) -> FaceResolution {
        self.resolve_faces_with_remaining(selector, &mut budget)
    }

    pub(super) fn resolve_faces_with_remaining(
        &self,
        selector: &FaceSelector,
        remaining: &mut usize,
    ) -> FaceResolution {
        use FaceResolutionStatus::*;
        let budget = *remaining;
        let mut resolution = FaceResolution {
            status: Unsupported,
            selector: selector.clone(),
            witnesses: Vec::new(),
            lineage: Vec::new(),
            diagnostic: String::new(),
        };
        macro_rules! charge {
            ($cost:expr) => {
                let cost = $cost;
                if cost > *remaining {
                    resolution.lineage.clear();
                    resolution.witnesses.clear();
                    resolution.status = Unsupported;
                    resolution.diagnostic = format!("face selection resource budget exceeded ({budget} work/lineage handles); no witnesses certified");
                    *remaining = 0;
                    return resolution;
                }
                *remaining -= cost;
            };
        }
        charge!(selector
            .anchor_feature
            .len()
            .saturating_add(selector.role.len())
            .saturating_add(selector.target_feature.len()));
        let graph = &self.selection_graph;
        let Some(anchor) = graph.results.get(&selector.anchor_feature) else {
            resolution.diagnostic = "anchor must identify an existing solid feature".into();
            return resolution;
        };
        if !graph.results.contains_key(&selector.target_feature) {
            resolution.diagnostic = "target must identify an existing solid feature".into();
            return resolution;
        }
        if matches!(selector.cardinality, FaceCardinality::Exactly { count: 0 }) {
            resolution.diagnostic = "exact cardinality must be positive".into();
            return resolution;
        }
        let Some(anchor_faces) = anchor.history.generated_faces.get(&selector.role) else {
            resolution.diagnostic = format!(
                "anchor '{}' has no generated role '{}'",
                selector.anchor_feature, selector.role
            );
            return resolution;
        };
        charge!(anchor_faces.len().saturating_mul(3));
        if anchor_faces
            .iter()
            .any(|face| face.0 >= anchor.model.faces.len())
        {
            resolution.diagnostic = "anchor role contains invalid snapshot handles".into();
            return resolution;
        }
        // Restrict traversal to ancestors of the requested target, so unrelated
        // downstream failures or logical aliases cannot alter this resolution.
        let mut ancestors = BTreeSet::new();
        let mut pending = vec![selector.target_feature.as_str()];
        while let Some(id) = pending.pop() {
            charge!(1);
            if ancestors.insert(id) {
                pending.extend(graph.nodes[id].dependencies());
            }
        }
        if !ancestors.contains(selector.anchor_feature.as_str()) {
            resolution.diagnostic =
                "anchor is not an ancestor of target; geometry equality is not logical ancestry"
                    .into();
            return resolution;
        }
        let mut states = BTreeMap::new();
        states.insert(
            selector.anchor_feature.as_str(),
            SelectionState {
                faces: anchor_faces
                    .iter()
                    .map(|face| (*face, BTreeSet::from([*face])))
                    .collect(),
                unsupported: false,
            },
        );
        for id in &graph.order {
            charge!(1);
            if id == &selector.anchor_feature || !ancestors.contains(id.as_str()) {
                continue;
            }
            let slots: Vec<_> = match &graph.nodes[id] {
                Feature::Translate { body, .. } => vec![("input", body)],
                Feature::Union { left, right, .. }
                | Feature::Cut { left, right, .. }
                | Feature::Intersect { left, right, .. } => vec![("left", left), ("right", right)],
                _ => continue,
            };
            let output = &graph.results[id];
            let mut next = SelectionState::default();
            let mut reachable = false;
            for (slot, source) in slots {
                let Some(previous) = states.get(source.as_str()) else {
                    continue;
                };
                reachable = true;
                next.unsupported |= previous.unsupported;
                let input_count = graph.results[source].model.faces.len();
                match output.history.input_faces.get(slot) {
                    None => next.unsupported = true,
                    Some(mapping) => {
                        charge!(mapping.len());
                        next.unsupported |= mapping.len() != input_count;
                        for (input, targets) in mapping {
                            charge!(targets.len());
                            next.unsupported |= input.0 >= input_count
                                || targets
                                    .iter()
                                    .any(|face| face.0 >= output.model.faces.len());
                        }
                    }
                }
                for (face, origins) in &previous.faces {
                    charge!(1);
                    let Some(mapped) = output
                        .history
                        .input_faces
                        .get(slot)
                        .and_then(|faces| faces.get(face))
                    else {
                        next.unsupported = true;
                        continue;
                    };
                    charge!(mapped
                        .len()
                        .saturating_mul(origins.len().saturating_add(3))
                        .saturating_add(1));
                    let mut mapped = mapped.clone();
                    mapped.sort();
                    mapped.dedup();
                    charge!(id
                        .len()
                        .saturating_add(source.len())
                        .saturating_add(slot.len()));
                    resolution.lineage.push(FaceLineageStep {
                        feature: id.clone(),
                        input_feature: source.clone(),
                        input_slot: slot.into(),
                        input_face: *face,
                        output_faces: mapped.clone(),
                    });
                    for target in mapped {
                        if target.0 >= output.model.faces.len() {
                            next.unsupported = true;
                            continue;
                        }
                        next.faces.entry(target).or_default().extend(origins);
                    }
                }
            }
            if reachable {
                states.insert(id.as_str(), next);
            }
        }
        let Some(state) = states.get(selector.target_feature.as_str()) else {
            resolution.diagnostic = "no supported lineage path reaches the target".into();
            return resolution;
        };
        for origins in state.faces.values() {
            charge!(origins
                .len()
                .saturating_add(1)
                .saturating_add(selector.target_feature.len()));
        }
        resolution.witnesses = state
            .faces
            .iter()
            .map(|(face, origins)| FaceWitness {
                target_feature: selector.target_feature.clone(),
                target_face: *face,
                anchor_faces: origins.iter().copied().collect(),
            })
            .collect();
        // Canonical ordering is independent of recipe declaration order.
        resolution.lineage.sort_by(|a, b| {
            (&a.feature, &a.input_slot, &a.input_feature, a.input_face).cmp(&(
                &b.feature,
                &b.input_slot,
                &b.input_feature,
                b.input_face,
            ))
        });
        if state.unsupported {
            resolution.diagnostic = "required face lineage is absent or invalid; any witnesses are partial and uncertified".into();
        } else if state.faces.is_empty() {
            resolution.status = Deleted;
            resolution.diagnostic =
                "all descendants of the generated role were explicitly deleted".into();
        } else if matches!(selector.cardinality, FaceCardinality::Exactly { count } if count != state.faces.len())
        {
            resolution.status = Ambiguous;
            resolution.diagnostic = format!(
                "cardinality mismatch: found {} candidate faces; no fallback chosen",
                state.faces.len()
            );
        } else {
            resolution.status = Resolved;
            resolution.diagnostic =
                "generated role resolved through explicit operation lineage".into();
        }
        resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe(width: f64, depth: f64, height: f64, x: f64, cut_width: f64) -> Recipe {
        serde_json::from_value(serde_json::json!({
            "schema_version":1,"units":"mm",
            "features":[
                {"op":"rectangle","id":"p","width":width,"depth":depth},
                {"op":"extrude","id":"body","profile":"p","height":height},
                {"op":"rectangle","id":"cp","width":cut_width,"depth":depth+2.0},
                {"op":"extrude","id":"c","profile":"cp","height":height+2.0},
                {"op":"translate","id":"tool","body":"c","offset":[x,-1,-1]},
                {"op":"cut","id":"cut","left":"body","right":"tool"},
                {"op":"translate","id":"moved","body":"cut","offset":[0,0,5]}
            ],"outputs":[{"name":"cut","feature":"cut"},{"name":"moved","feature":"moved"}]
        }))
        .unwrap()
    }
    fn selector(target: &str, cardinality: FaceCardinality) -> FaceSelector {
        FaceSelector {
            anchor_feature: "body".into(),
            role: "end_cap".into(),
            target_feature: target.into(),
            cardinality,
        }
    }
    fn selected_area(
        build: &BuildResult,
        target: &str,
        resolution: &FaceResolution,
        z: f64,
    ) -> f64 {
        let model = &build.outputs[target].result.model;
        resolution
            .witnesses
            .iter()
            .map(|w| {
                let face = &model.faces[w.target_face.0];
                assert_eq!(face.plane.normal, [0.0, 0.0, 1.0]);
                let mut min = [f64::INFINITY; 2];
                let mut max = [f64::NEG_INFINITY; 2];
                for edge in &model.loops[face.loops[0].0].edges {
                    for vertex in model.edges[edge.edge.0].vertices {
                        let p = model.vertices[vertex.0].position;
                        assert!((p[2] - z).abs() < 1e-9);
                        for axis in 0..2 {
                            min[axis] = min[axis].min(p[axis]);
                            max[axis] = max[axis].max(p[axis]);
                        }
                    }
                }
                (max[0] - min[0]) * (max[1] - min[1])
            })
            .sum()
    }

    #[test]
    fn generated_edit_sequences_preserve_semantic_cap_or_report_deletion_and_splits() {
        for edit in 0..512 {
            let width = 12.0 + (edit % 7) as f64;
            let depth = 8.0 + ((edit / 7) % 5) as f64;
            let height = 3.0 + ((edit / 35) % 3) as f64;
            let (x, cut_width, count, removed) = match edit % 4 {
                0 => (width / 3.0, width / 4.0, 2, width / 4.0),
                1 => (-1.0, width / 2.0 + 1.0, 1, width / 2.0),
                2 => (-1.0, width + 2.0, 0, width),
                _ => (width + 1.0, 2.0, 1, 0.0),
            };
            let build = build(&recipe(width, depth, height, x, cut_width)).unwrap();
            for target in ["cut", "moved"] {
                let all = build.resolve_faces(&selector(target, FaceCardinality::All));
                assert_eq!(all.witnesses.len(), count, "edit {edit} {all:?}");
                assert_eq!(
                    all.status,
                    if count == 0 {
                        FaceResolutionStatus::Deleted
                    } else {
                        FaceResolutionStatus::Resolved
                    }
                );
                let one =
                    build.resolve_faces(&selector(target, FaceCardinality::Exactly { count: 1 }));
                assert_eq!(
                    one.status,
                    match count {
                        0 => FaceResolutionStatus::Deleted,
                        1 => FaceResolutionStatus::Resolved,
                        _ => FaceResolutionStatus::Ambiguous,
                    }
                );
                let area = selected_area(
                    &build,
                    target,
                    &all,
                    height + if target == "moved" { 5.0 } else { 0.0 },
                );
                assert!(
                    (area - (width - removed) * depth).abs() < 1e-8,
                    "edit {edit}: {area}"
                );
            }
        }
    }

    #[test]
    fn semantic_intent_uses_logical_ancestry_not_shared_geometry_or_face_numbers() {
        let mut recipe = recipe(12.0, 8.0, 3.0, 4.0, 3.0);
        recipe.features.push(Feature::Extrude {
            id: "alias".into(),
            profile: "p".into(),
            height: 3.0,
            op_version: 1,
        });
        let mut engine = BuildEngine::default();
        let first = engine.build(&recipe).unwrap();
        assert!(Arc::ptr_eq(
            &first.selection_graph.results["body"],
            &first.selection_graph.results["alias"]
        ));
        let mut intent = selector("cut", FaceCardinality::All);
        assert_eq!(
            first.resolve_faces(&intent).status,
            FaceResolutionStatus::Resolved
        );
        intent.anchor_feature = "alias".into();
        assert_eq!(
            first.resolve_faces(&intent).status,
            FaceResolutionStatus::Unsupported
        );
        intent.anchor_feature = "body".into();
        intent.role = "missing_role".into();
        assert_eq!(
            first.resolve_faces(&intent).status,
            FaceResolutionStatus::Unsupported
        );
        recipe.features.reverse();
        let reordered = engine.build(&recipe).unwrap();
        assert_eq!(reordered.stats.computed_solids, 0);
        let intent = selector("moved", FaceCardinality::All);
        assert_eq!(
            serde_json::to_value(first.resolve_faces(&intent)).unwrap(),
            serde_json::to_value(reordered.resolve_faces(&intent)).unwrap()
        );
    }

    #[test]
    fn branching_lineage_deduplicates_witnesses_and_retains_operand_provenance() {
        let mut recipe = recipe(12.0, 8.0, 3.0, 4.0, 3.0);
        recipe.features.push(Feature::Union {
            id: "diamond".into(),
            left: "body".into(),
            right: "body".into(),
            op_version: 1,
        });
        let built = build(&recipe).unwrap();
        let resolved =
            built.resolve_faces(&selector("diamond", FaceCardinality::Exactly { count: 1 }));
        assert_eq!(resolved.status, FaceResolutionStatus::Resolved);
        assert_eq!(resolved.witnesses.len(), 1);
        assert_eq!(
            resolved
                .lineage
                .iter()
                .map(|step| step.input_slot.as_str())
                .collect::<Vec<_>>(),
            ["left", "right"]
        );
        let tool_wall = FaceSelector {
            anchor_feature: "c".into(),
            role: "wall_x_min".into(),
            target_feature: "cut".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        };
        let wall = built.resolve_faces(&tool_wall);
        assert_eq!(wall.status, FaceResolutionStatus::Resolved);
        assert!(wall.lineage.iter().any(|step| step.input_slot == "right"));
        assert_eq!(
            built.outputs["cut"].result.model.faces[wall.witnesses[0].target_face.0]
                .plane
                .normal,
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn query_budget_exhaustion_never_certifies_partial_witnesses_or_deletion() {
        let built = build(&recipe(12.0, 8.0, 3.0, 4.0, 3.0)).unwrap();
        for budget in [0, 10, 50] {
            let result =
                built.resolve_faces_with_budget(&selector("moved", FaceCardinality::All), budget);
            assert_eq!(result.status, FaceResolutionStatus::Unsupported);
            assert!(result.diagnostic.contains("resource budget"));
            assert!(result.witnesses.is_empty());
            assert!(result.lineage.is_empty());
        }
        assert_eq!(
            built
                .resolve_faces(&selector("moved", FaceCardinality::All))
                .status,
            FaceResolutionStatus::Resolved
        );
    }

    #[test]
    fn copied_identifier_bytes_count_toward_query_budget() {
        let mut recipe = recipe(12.0, 8.0, 3.0, 4.0, 3.0);
        let long_id = "m".repeat(1000);
        if let Feature::Translate { id, .. } = &mut recipe.features[6] {
            *id = long_id.clone();
        }
        recipe.outputs[1].feature = long_id.clone();
        let built = build(&recipe).unwrap();
        let result =
            built.resolve_faces_with_budget(&selector(&long_id, FaceCardinality::All), 500);
        assert_eq!(result.status, FaceResolutionStatus::Unsupported);
        assert!(result.diagnostic.contains("resource budget"));
        assert!(result.lineage.is_empty());
    }

    #[test]
    fn persisted_assertions_gate_builds_and_reuse_geometry_on_contract_edits() {
        let mut recipe = recipe(12.0, 8.0, 3.0, 4.0, 3.0);
        recipe.schema_version = 2;
        recipe.face_assertions.push(FaceAssertion {
            id: "top_faces".into(),
            selector: selector("cut", FaceCardinality::All),
        });
        let mut engine = BuildEngine::default();
        let first = engine.build(&recipe).unwrap();
        assert_eq!(first.face_assertions["top_faces"].witnesses.len(), 2);
        recipe.face_assertions[0].selector.cardinality = FaceCardinality::Exactly { count: 2 };
        let changed = engine.build(&recipe).unwrap();
        assert_eq!(changed.stats.computed_solids, 0);
        assert_eq!(first.feature_hashes, changed.feature_hashes);
        recipe.face_assertions[0].selector.cardinality = FaceCardinality::Exactly { count: 1 };
        let BuildError::FaceAssertionFailed {
            assertion_id,
            resolution,
            anchor_content_hash,
            target_content_hash,
        } = engine.build(&recipe).unwrap_err()
        else {
            panic!("expected structured assertion failure")
        };
        assert_eq!(assertion_id, "top_faces");
        assert_eq!(resolution.status, FaceResolutionStatus::Ambiguous);
        assert_eq!(resolution.witnesses.len(), 2);
        assert_eq!(
            anchor_content_hash.as_ref(),
            first.feature_hashes.get("body")
        );
        assert_eq!(
            target_content_hash.as_ref(),
            first.feature_hashes.get("cut")
        );
        recipe.face_assertions[0].selector.role = "unknown".into();
        let BuildError::FaceAssertionFailed { resolution, .. } = engine.build(&recipe).unwrap_err()
        else {
            panic!("expected unsupported selector")
        };
        assert_eq!(resolution.status, FaceResolutionStatus::Unsupported);
        recipe.face_assertions[0].selector.role = "end_cap".into();
        if let Feature::Cut { right, .. } = &mut recipe.features[5] {
            *right = "body".into();
        }
        let BuildError::FaceAssertionFailed { resolution, .. } = engine.build(&recipe).unwrap_err()
        else {
            panic!("expected deleted selector")
        };
        assert_eq!(resolution.status, FaceResolutionStatus::Deleted);
    }

    #[test]
    fn many_successful_assertions_share_one_aggregate_work_budget() {
        let mut recipe = recipe(12.0, 8.0, 3.0, 4.0, 3.0);
        recipe.schema_version = 2;
        let long_id = "m".repeat(5000);
        if let Feature::Translate { id, .. } = &mut recipe.features[6] {
            *id = long_id.clone();
        }
        recipe.outputs[1].feature = long_id.clone();
        recipe.face_assertions = (0..200)
            .map(|i| FaceAssertion {
                id: format!("assertion_{i}"),
                selector: selector(&long_id, FaceCardinality::All),
            })
            .collect();
        let error = build(&recipe).unwrap_err();
        let BuildError::FaceAssertionFailed { resolution, .. } = error else {
            panic!("expected aggregate budget failure")
        };
        assert_eq!(resolution.status, FaceResolutionStatus::Unsupported);
        assert!(resolution.diagnostic.contains("resource budget"));
        assert!(resolution.witnesses.is_empty());
    }

    #[test]
    fn assertion_schema_and_preflight_errors_do_not_evaluate_geometry() {
        let base = recipe(12.0, 8.0, 3.0, 4.0, 3.0);
        for value in [serde_json::json!([]), serde_json::Value::Null] {
            let mut raw = serde_json::to_value(&base).unwrap();
            raw["face_assertions"] = value;
            assert!(serde_json::from_value::<Recipe>(raw).is_err());
        }
        let mut recipe = base;
        recipe.schema_version = 2;
        recipe.face_assertions.push(FaceAssertion {
            id: "top".into(),
            selector: selector("cut", FaceCardinality::Exactly { count: 1 }),
        });
        let serialized = serde_json::to_string(&recipe).unwrap();
        assert_eq!(
            serde_json::from_str::<Recipe>(&serialized)
                .unwrap()
                .face_assertions[0]
                .id,
            "top"
        );
        let mut unknown = serde_json::to_value(&recipe).unwrap();
        unknown["face_assertions"][0]["typo"] = true.into();
        assert!(serde_json::from_value::<Recipe>(unknown).is_err());
        // Invalid geometry would fail later; assertion preflight must win first.
        if let Feature::Rectangle { width, .. } = &mut recipe.features[0] {
            *width = -1.0;
        }
        recipe.face_assertions[0].selector.cardinality = FaceCardinality::Exactly { count: 0 };
        assert!(matches!(
            build(&recipe),
            Err(BuildError::InvalidFaceAssertion { .. })
        ));
        recipe.face_assertions[0].selector.cardinality = FaceCardinality::All;
        recipe
            .face_assertions
            .push(recipe.face_assertions[0].clone());
        assert!(matches!(
            build(&recipe),
            Err(BuildError::InvalidFaceAssertion { .. })
        ));
        recipe.face_assertions.pop();
        recipe.face_assertions[0].id = " ".into();
        assert!(matches!(
            build(&recipe),
            Err(BuildError::Identifier {
                kind: "face assertion",
                ..
            })
        ));
        recipe.face_assertions = vec![recipe.face_assertions[0].clone(); MAX_FACE_ASSERTIONS + 1];
        assert!(matches!(
            build(&recipe),
            Err(BuildError::ResourceLimit {
                resource: "face assertions",
                ..
            })
        ));
    }

    #[test]
    fn incomplete_or_invalid_history_is_unsupported_never_deletion() {
        let mut built = build(&recipe(12.0, 8.0, 3.0, 4.0, 3.0)).unwrap();
        let original = (*built.selection_graph.results["cut"]).clone();
        for corrupt in 0..3 {
            let mut damaged = original.clone();
            match corrupt {
                0 => {
                    damaged.history.input_faces.remove("left");
                }
                1 => {
                    damaged
                        .history
                        .input_faces
                        .get_mut("left")
                        .unwrap()
                        .remove(&FaceId(0));
                }
                _ => {
                    damaged
                        .history
                        .input_faces
                        .get_mut("left")
                        .unwrap()
                        .insert(FaceId(1), vec![FaceId(usize::MAX)]);
                }
            }
            built
                .selection_graph
                .results
                .insert("cut".into(), Arc::new(damaged));
            assert_eq!(
                built
                    .resolve_faces(&selector("cut", FaceCardinality::All))
                    .status,
                FaceResolutionStatus::Unsupported
            );
        }
    }
}
