//! Associative datum intent resolves to a rigid part-local frame only after
//! exact geometry and logical face lineage have been evaluated successfully.
use super::*;

pub const MAX_FACE_DATUMS: usize = 1000;
pub const MAX_FACE_DATUM_GEOMETRY_WORK: usize = 20_000_000;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceDatumDefinition {
    pub part: String,
    pub name: String,
    pub anchor_feature: String,
    pub role: String,
    pub x_direction: [f64; 3],
    #[serde(default)]
    pub offset: [f64; 3],
}
#[derive(Debug, Clone, Serialize)]
pub struct FaceDatumResolution {
    pub definition: FaceDatumDefinition,
    pub selection: FaceResolution,
    pub frame: RigidTransform,
    pub target_content_hash: String,
    pub anchor_content_hash: Option<String>,
}
pub type ResolvedFaceDatums = BTreeMap<String, BTreeMap<String, FaceDatumResolution>>;

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version != 2 && !recipe.face_datums.is_empty() {
        return Err(BuildError::FaceDatumSchema);
    }
    if recipe.face_datums.len() > MAX_FACE_DATUMS {
        return Err(BuildError::ResourceLimit {
            resource: "face datums",
            limit: MAX_FACE_DATUMS,
        });
    }
    let parts: BTreeMap<_, _> = recipe.parts.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut names = BTreeSet::new();
    let mut counts: BTreeMap<_, _> = parts.iter().map(|(id, p)| (*id, p.datums.len())).collect();
    for datum in &recipe.face_datums {
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
        ] {
            identifier(kind, value)?;
        }
        let part = parts
            .get(datum.part.as_str())
            .ok_or_else(|| invalid("unknown owning part"))?;
        if part.datums.contains_key(&datum.name) || !names.insert((&datum.part, &datum.name)) {
            return Err(invalid("duplicate fixed or associative datum name"));
        }
        if datum.x_direction.iter().any(|v| !v.is_finite())
            || datum.x_direction.iter().all(|v| *v == 0.0)
            || datum.offset.iter().any(|v| !v.is_finite())
        {
            return Err(invalid(
                "x_direction must be finite and nonzero; offset must be finite",
            ));
        }
        let count = counts.get_mut(datum.part.as_str()).unwrap();
        *count += 1;
        if *count > 1024 {
            return Err(invalid("part exceeds 1024 total datum frames"));
        }
    }
    Ok(())
}
// Placeholder frames are used only for pre-geometry attachment reference/cycle
// validation. They are never passed to output construction or published.
pub(super) fn add_placeholders(recipe: &Recipe, parts: &mut BTreeMap<String, PartDefinition>) {
    for datum in &recipe.face_datums {
        parts
            .get_mut(&datum.part)
            .expect("datum preflight")
            .datums
            .insert(datum.name.clone(), RigidTransform::default());
    }
}
pub(super) fn rigid_frame(frame: cad_kernel::FaceFrame) -> Result<RigidTransform, String> {
    let axes = [frame.x_axis, frame.y_axis, frame.z_axis];
    let m: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| axes[j][i]));
    let trace = m[0][0] + m[1][1] + m[2][2];
    let mut q = [0.0; 4];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        q = [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            s / 4.0,
        ];
    } else {
        let i = if m[0][0] >= m[1][1] && m[0][0] >= m[2][2] {
            0
        } else if m[1][1] >= m[2][2] {
            1
        } else {
            2
        };
        let j = (i + 1) % 3;
        let k = (i + 2) % 3;
        let s = (1.0 + m[i][i] - m[j][j] - m[k][k]).sqrt() * 2.0;
        q[i] = s / 4.0;
        q[j] = (m[j][i] + m[i][j]) / s;
        q[k] = (m[k][i] + m[i][k]) / s;
        q[3] = (m[k][j] - m[j][k]) / s;
    }
    let norm = q.iter().fold(0.0_f64, |n, v| n.hypot(*v));
    if !norm.is_finite() || norm == 0.0 {
        return Err("face basis cannot form a finite rigid quaternion".into());
    }
    for value in &mut q {
        *value /= norm;
    }
    let sign = if q[3] != 0.0 {
        q[3]
    } else {
        *q.iter().find(|v| **v != 0.0).unwrap()
    };
    for value in &mut q {
        if sign < 0.0 {
            *value = -*value;
        }
        if *value == 0.0 {
            *value = 0.0;
        }
    }
    let result = RigidTransform {
        translation: frame.origin,
        rotation: q,
    };
    result.validate().map_err(|e| e.to_string())?;
    for (axis, expected) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .zip(axes)
    {
        if result
            .vector(axis)
            .into_iter()
            .zip(expected)
            .any(|(a, b)| (a - b).abs() > 128.0 * f64::EPSILON)
        {
            return Err("quaternion does not reproduce the resolved face basis".into());
        }
    }
    Ok(result)
}
pub(super) fn resolve(
    recipe: &Recipe,
    build: &BuildResult,
    evaluated: &HashMap<String, Arc<OperationResult>>,
    tolerance: Tolerance,
    remaining: &mut usize,
) -> Result<ResolvedFaceDatums, BuildError> {
    let parts: BTreeMap<_, _> = recipe.parts.iter().map(|p| (&p.id, p)).collect();
    let mut result = ResolvedFaceDatums::new();
    let mut geometry_work = 0usize;
    for datum in &recipe.face_datums {
        let target = &parts[&datum.part].feature;
        let selector = FaceSelector {
            anchor_feature: datum.anchor_feature.clone(),
            role: datum.role.clone(),
            target_feature: target.clone(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        };
        let selection = build.resolve_faces_with_remaining(&selector, remaining);
        if selection.status != FaceResolutionStatus::Resolved {
            return Err(BuildError::FaceDatumFailed {
                part: datum.part.clone(),
                name: datum.name.clone(),
                resolution: Box::new(selection),
                target_content_hash: build.feature_hashes.get(target).cloned(),
                anchor_content_hash: build.feature_hashes.get(&datum.anchor_feature).cloned(),
            });
        }
        let model = &evaluated[target].model;
        let size = model
            .faces
            .len()
            .saturating_add(model.vertices.len())
            .saturating_add(
                model
                    .edges
                    .len()
                    .saturating_add(model.loops.iter().map(|l| l.edges.len()).sum::<usize>()),
            );
        geometry_work = geometry_work.saturating_add(size.saturating_mul(size));
        if geometry_work > MAX_FACE_DATUM_GEOMETRY_WORK {
            return Err(BuildError::ResourceLimit {
                resource: "face datum geometry work",
                limit: MAX_FACE_DATUM_GEOMETRY_WORK,
            });
        }
        let frame = model
            .face_frame(
                selection.witnesses[0].target_face,
                datum.x_direction,
                datum.offset,
                tolerance,
            )
            .map_err(|e| BuildError::FaceDatumFrame {
                part: datum.part.clone(),
                name: datum.name.clone(),
                message: e.to_string(),
            })?;
        let frame = rigid_frame(frame).map_err(|message| BuildError::FaceDatumFrame {
            part: datum.part.clone(),
            name: datum.name.clone(),
            message,
        })?;
        result.entry(datum.part.clone()).or_default().insert(
            datum.name.clone(),
            FaceDatumResolution {
                definition: datum.clone(),
                selection,
                frame,
                target_content_hash: build.feature_hashes[target].clone(),
                anchor_content_hash: build.feature_hashes.get(&datum.anchor_feature).cloned(),
            },
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_basis_conversion_handles_half_turns_without_an_arbitrary_axis() {
        for (axes, expected) in [
            (
                [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
                [1.0, 0.0, 0.0, 0.0],
            ),
            (
                [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
                [0.0, 1.0, 0.0, 0.0],
            ),
            (
                [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
                [0.0, 0.0, 1.0, 0.0],
            ),
        ] {
            let frame = rigid_frame(cad_kernel::FaceFrame {
                origin: [2.0, 3.0, 4.0],
                x_axis: axes[0],
                y_axis: axes[1],
                z_axis: axes[2],
            })
            .unwrap();
            assert_eq!(frame.rotation, expected);
            assert_eq!(frame.translation, [2.0, 3.0, 4.0]);
        }
    }
}
