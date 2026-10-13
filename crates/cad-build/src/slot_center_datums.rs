//! Associative machining datums at certified native obround-slot centers.
use super::*;

pub const MAX_SLOT_CENTER_DATUMS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotCenterDatumDefinition {
    pub part: String,
    pub name: String,
    pub anchor_feature: String,
    pub role: String,
    pub cap_role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotCenterDatumResolution {
    pub definition: SlotCenterDatumDefinition,
    pub walls: FaceResolution,
    pub cap: FaceResolution,
    pub native: cad_kernel::SlotCenterFrame,
    pub frame: RigidTransform,
    pub target_content_hash: String,
    pub anchor_content_hash: Option<String>,
}

pub type ResolvedSlotCenterDatums = BTreeMap<String, BTreeMap<String, SlotCenterDatumResolution>>;

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version != 2 && !recipe.slot_center_datums.is_empty() {
        return Err(BuildError::FaceDatumSchema);
    }
    if recipe.slot_center_datums.len() + recipe.face_datums.len() > MAX_SLOT_CENTER_DATUMS {
        return Err(BuildError::ResourceLimit {
            resource: "associative datums",
            limit: MAX_SLOT_CENTER_DATUMS,
        });
    }
    let parts: BTreeMap<_, _> = recipe.parts.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut names = BTreeSet::new();
    let mut counts: BTreeMap<_, _> = parts.iter().map(|(id, p)| (*id, p.datums.len())).collect();
    for datum in &recipe.slot_center_datums {
        let invalid = |message: &str| BuildError::InvalidFaceDatum {
            part: datum.part.clone(),
            name: datum.name.clone(),
            message: message.into(),
        };
        for (kind, value) in [
            ("part", &datum.part),
            ("datum", &datum.name),
            ("feature", &datum.anchor_feature),
            ("role", &datum.role),
            ("cap role", &datum.cap_role),
        ] {
            identifier(kind, value)?;
        }
        let part = parts
            .get(datum.part.as_str())
            .ok_or_else(|| invalid("unknown owning part"))?;
        if part.datums.contains_key(&datum.name)
            || recipe
                .face_datums
                .iter()
                .any(|face| face.part == datum.part && face.name == datum.name)
            || !names.insert((&datum.part, &datum.name))
        {
            return Err(invalid("duplicate fixed or associative datum name"));
        }
        let count = counts.get_mut(datum.part.as_str()).unwrap();
        *count += 1;
        if *count
            + recipe
                .face_datums
                .iter()
                .filter(|face| face.part == datum.part)
                .count()
            > 1024
        {
            return Err(invalid("part exceeds 1024 total datum frames"));
        }
    }
    Ok(())
}

pub(super) fn add_placeholders(recipe: &Recipe, parts: &mut BTreeMap<String, PartDefinition>) {
    for datum in &recipe.slot_center_datums {
        parts
            .get_mut(&datum.part)
            .expect("datum preflight")
            .datums
            .insert(datum.name.clone(), RigidTransform::default());
    }
}

pub(super) fn resolve(
    recipe: &Recipe,
    build: &BuildResult,
    evaluated: &HashMap<String, Arc<OperationResult>>,
    tolerance: Tolerance,
    remaining: &mut usize,
) -> Result<ResolvedSlotCenterDatums, BuildError> {
    let parts: BTreeMap<_, _> = recipe.parts.iter().map(|p| (&p.id, p)).collect();
    let mut result = ResolvedSlotCenterDatums::new();
    let mut geometry_work = 0usize;
    for datum in &recipe.slot_center_datums {
        let target = &parts[&datum.part].feature;
        let fail = |message: String| BuildError::SlotCenterDatumFailed {
            part: datum.part.clone(),
            name: datum.name.clone(),
            message,
        };
        let walls = build.resolve_faces_with_remaining(
            &FaceSelector {
                anchor_feature: datum.anchor_feature.clone(),
                role: datum.role.clone(),
                target_feature: target.clone(),
                cardinality: FaceCardinality::All,
            },
            remaining,
        );
        if walls.status != FaceResolutionStatus::Resolved || walls.witnesses.len() != 4 {
            return Err(fail(format!(
                "slot wall role must resolve to exactly four native faces: {:?}: {}",
                walls.status, walls.diagnostic
            )));
        }
        let cap = build.resolve_faces_with_remaining(
            &FaceSelector {
                anchor_feature: datum.anchor_feature.clone(),
                role: datum.cap_role.clone(),
                target_feature: target.clone(),
                cardinality: FaceCardinality::Exactly { count: 1 },
            },
            remaining,
        );
        if cap.status != FaceResolutionStatus::Resolved {
            return Err(fail(format!(
                "slot cap role must resolve to one native face: {:?}: {}",
                cap.status, cap.diagnostic
            )));
        }
        let model = &evaluated[target].model;
        let size = model
            .faces
            .len()
            .saturating_add(model.vertices.len())
            .saturating_add(model.edges.len())
            .saturating_add(model.loops.iter().map(|l| l.edges.len()).sum::<usize>());
        geometry_work = geometry_work.saturating_add(size.saturating_mul(size));
        if geometry_work > face_datums::MAX_FACE_DATUM_GEOMETRY_WORK {
            return Err(BuildError::ResourceLimit {
                resource: "slot center datum geometry work",
                limit: face_datums::MAX_FACE_DATUM_GEOMETRY_WORK,
            });
        }
        let faces: [cad_kernel::FaceId; 4] = walls
            .witnesses
            .iter()
            .map(|w| w.target_face)
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| fail("slot requires exactly four native walls".into()))?;
        let native =
            cad_kernel::slot_center_frame(model, faces, cap.witnesses[0].target_face, tolerance)
                .map_err(|e| fail(format!("native slot center: {e}")))?;
        let frame = super::face_datums::rigid_frame(native.frame.clone())
            .map_err(|e| fail(format!("slot frame: {e}")))?;
        result.entry(datum.part.clone()).or_default().insert(
            datum.name.clone(),
            SlotCenterDatumResolution {
                definition: datum.clone(),
                walls,
                cap,
                native,
                frame,
                target_content_hash: build.feature_hashes[target].clone(),
                anchor_content_hash: build.feature_hashes.get(&datum.anchor_feature).cloned(),
            },
        );
    }
    Ok(result)
}
