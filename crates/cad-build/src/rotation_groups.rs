//! Source-owned common rigid rotation; only explicitly named pairs are certified.
use super::*;
pub const MAX_ROTATION_GROUPS: usize = 16;
pub const MAX_ROTATION_GROUP_MEMBERS: usize = 64;
pub const MAX_ROTATION_GROUP_MEMBERS_TOTAL: usize = 256;
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RigidRotationGroup {
    pub id: String,
    pub output: String,
    pub instance_paths: Vec<Vec<String>>,
    pub reference_instance_path: Vec<String>,
    pub pivot: [f64; 3],
    pub axis: [f64; 3],
    pub angle_radians: f64,
}
impl<'de> Deserialize<'de> for RigidRotationGroup {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            id: String,
            output: String,
            instance_paths: Vec<Vec<String>>,
            reference_instance_path: Vec<String>,
            pivot: [f64; 3],
            axis: [f64; 3],
            angle_radians: f64,
        }
        let v = serde_json::Value::deserialize(d)?;
        if !v.is_object() {
            return Err(serde::de::Error::custom(
                "rigid rotation group must be an object",
            ));
        }
        let r: Raw = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: r.id,
            output: r.output,
            instance_paths: r.instance_paths,
            reference_instance_path: r.reference_instance_path,
            pivot: r.pivot,
            axis: r.axis,
            angle_radians: r.angle_radians,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct RigidRotationGroupReport {
    pub declaration: RigidRotationGroup,
    pub status: String,
    pub coverage: String,
    pub assembly_content_hash: String,
    pub reference: ClearanceOccurrenceWitness,
    pub members: Vec<ClearanceOccurrenceWitness>,
    pub rotation_world: RotationPath,
    pub pivot_uncertainty_mm: f64,
    pub axis_uncertainty: f64,
    pub clearance_check_ids: Vec<String>,
    pub maximum_member_radius_mm: f64,
    /// Point-trajectory mapping bound; not an unqueried native-support or collision certificate.
    pub maximum_path_uncertainty_mm: f64,
}
impl RigidRotationGroup {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |m: &str| BuildError::InvalidRotationGroup {
            id: self.id.clone(),
            output: self.output.clone(),
            message: m.into(),
        };
        let path = |p: &Vec<String>| {
            !p.is_empty()
                && p.len() <= 64
                && p.iter()
                    .all(|s| !s.is_empty() && s.len() <= 1024 && s.trim() == s)
        };
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || self.output.is_empty()
            || self.output.len() > 1024
        {
            return Err(fail("bounded portable ID and output required"));
        }
        if self.instance_paths.is_empty()
            || self.instance_paths.len() > MAX_ROTATION_GROUP_MEMBERS
            || self.instance_paths.iter().any(|p| !path(p))
            || !path(&self.reference_instance_path)
        {
            return Err(fail(
                "require 1..64 exact bounded leaf paths and an exact reference path",
            ));
        }
        let unique: BTreeSet<_> = self.instance_paths.iter().collect();
        if unique.len() != self.instance_paths.len()
            || !unique.contains(&self.reference_instance_path)
        {
            return Err(fail(
                "members must be unique and include the reference occurrence",
            ));
        }
        let length = |a: [f64; 3]| a.into_iter().fold(0_f64, f64::hypot);
        if self
            .pivot
            .into_iter()
            .chain(self.axis)
            .any(|v| !v.is_finite())
            || length(self.pivot) > 1e6
            || (length(self.axis) - 1.).abs() > 32. * f64::EPSILON
            || !self.angle_radians.is_finite()
            || self.angle_radians == 0.
            || self.angle_radians.abs() > std::f64::consts::TAU
        {
            return Err(fail("rotation requires finite local pivot within1000000mm, unit axis, nonzero signed angle at most2pi"));
        }
        Ok(())
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.rigid_rotation_groups.len() > MAX_ROTATION_GROUPS {
        return Err(BuildError::ResourceLimit {
            resource: "rigid rotation groups",
            limit: MAX_ROTATION_GROUPS,
        });
    }
    let mut ids = BTreeSet::new();
    let mut members = BTreeSet::new();
    let mut count = 0;
    for g in &recipe.rigid_rotation_groups {
        g.validate()?;
        let fail = |m: &str| BuildError::InvalidRotationGroup {
            id: g.id.clone(),
            output: g.output.clone(),
            message: m.into(),
        };
        if recipe.schema_version != 2
            || !ids.insert(g.id.to_ascii_lowercase())
            || !recipe.assembly_outputs.iter().any(|o| o.name == g.output)
        {
            return Err(fail(
                "schema2, unique ID and declared assembly output required",
            ));
        }
        count += g.instance_paths.len();
        if count > MAX_ROTATION_GROUP_MEMBERS_TOTAL {
            return Err(BuildError::ResourceLimit {
                resource: "rigid rotation group members",
                limit: MAX_ROTATION_GROUP_MEMBERS_TOTAL,
            });
        }
        for p in &g.instance_paths {
            if !members.insert((&g.output, p)) {
                return Err(fail(
                    "overlapping motion groups on the same output are unsupported",
                ));
            }
        }
        let mut checks = 0;
        for c in recipe
            .clearance_checks
            .iter()
            .filter(|c| c.rotation_group.as_ref() == Some(&g.id))
        {
            checks += 1;
            if c.output != g.output
                || !g.instance_paths.contains(&c.left_instance_path)
                    && !g.instance_paths.contains(&c.right_instance_path)
            {
                return Err(fail(
                    "referenced pair must share group output and contain a moving member",
                ));
            }
        }
        if checks == 0 {
            return Err(fail(
                "group requires at least one explicitly referenced clearance pair",
            ));
        }
    }
    for c in &recipe.clearance_checks {
        if let Some(id) = &c.rotation_group {
            if !recipe.rigid_rotation_groups.iter().any(|g| g.id == *id) {
                return Err(BuildError::InvalidClearanceCheck {
                    id: c.id.clone(),
                    output: c.output.clone(),
                    message: "rotation group reference does not exist".into(),
                });
            }
        }
    }
    Ok(())
}
pub(super) fn evaluate(
    recipe: &Recipe,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    t: cad_kernel::Tolerance,
) -> Result<BTreeMap<String, RigidRotationGroupReport>, BuildError> {
    let mut reports = BTreeMap::new();
    let mut work = 0usize;
    for g in &recipe.rigid_rotation_groups {
        let fail = |m: String| BuildError::RotationGroupFailed {
            id: g.id.clone(),
            output: g.output.clone(),
            message: m,
        };
        let a = assemblies
            .get(&g.output)
            .ok_or_else(|| fail("assembly output was not evaluated".into()))?;
        let mut members = vec![];
        for path in &g.instance_paths {
            let cost = path
                .iter()
                .fold(1usize, |n, s| n.saturating_add(s.len() + 1));
            work = work.saturating_add(a.instances.len().saturating_mul(cost));
            if work > MAX_CLEARANCE_RESOLVE_WORK {
                return Err(BuildError::ResourceLimit {
                    resource: "rotation group path resolution",
                    limit: MAX_CLEARANCE_RESOLVE_WORK,
                });
            }
            let mut found = a
                .instances
                .iter()
                .filter(|i| a.instance_paths.get(&i.id) == Some(path));
            let i = found
                .next()
                .ok_or_else(|| fail(format!("missing exact moving leaf path {path:?}")))?;
            if found.next().is_some() {
                return Err(fail("ambiguous moving leaf path".into()));
            }
            let p = &a.parts[&i.part];
            members.push(ClearanceOccurrenceWitness {
                instance_id: i.id.clone(),
                instance_path: path.clone(),
                part_id: i.part.clone(),
                feature_id: p.feature_id.clone(),
                part_content_hash: p.content_hash.clone(),
                frame: i.transform,
            });
        }
        let reference = members
            .iter()
            .find(|m| m.instance_path == g.reference_instance_path)
            .unwrap()
            .clone();
        let rotation_world = RotationPath {
            pivot: reference.frame.point(g.pivot),
            axis: reference.frame.vector(g.axis),
            angle_radians: g.angle_radians,
        };
        let pivot_uncertainty_mm = (g
            .pivot
            .into_iter()
            .chain(reference.frame.translation)
            .map(f64::abs)
            .sum::<f64>()
            * 1024.
            * f64::EPSILON)
            .next_up();
        let axis_uncertainty =
            (g.axis.into_iter().map(f64::abs).sum::<f64>() * 1024. * f64::EPSILON).next_up();
        if rotation_world
            .pivot
            .into_iter()
            .chain(rotation_world.axis)
            .any(|v| !v.is_finite())
            || !pivot_uncertainty_mm.is_finite()
            || pivot_uncertainty_mm > t.linear / 32.
        {
            return Err(fail(
                "common rotation frame exceeds numeric precision".into(),
            ));
        }
        let mut maximum_member_radius_mm = 0_f64;
        let mut maximum_member_placement_mm = 0_f64;
        for member in &members {
            let model = &a.parts[&member.part_id].result.model;
            work = work
                .saturating_add(model.vertices.len())
                .saturating_add(model.edges.len())
                .saturating_add(model.faces.len());
            if work > MAX_CLEARANCE_RESOLVE_WORK {
                return Err(BuildError::ResourceLimit {
                    resource: "rotation group geometry work",
                    limit: MAX_CLEARANCE_RESOLVE_WORK,
                });
            }
            let bounds = model.bounds();
            let operand_scale = bounds
                .min
                .into_iter()
                .chain(bounds.max)
                .chain(member.frame.translation)
                .map(f64::abs)
                .sum::<f64>();
            let placement_error = (operand_scale * 1024. * f64::EPSILON).next_up();
            maximum_member_placement_mm = maximum_member_placement_mm.max(placement_error);
            for mask in 0..8 {
                let corner = std::array::from_fn(|i| {
                    if mask & (1 << i) == 0 {
                        bounds.min[i]
                    } else {
                        bounds.max[i]
                    }
                });
                let point = member.frame.point(corner);
                let radius = (0..3)
                    .map(|i| point[i] - rotation_world.pivot[i])
                    .fold(0_f64, f64::hypot)
                    .next_up();
                if !radius.is_finite() {
                    return Err(fail("nonfinite common member trajectory".into()));
                }
                let radius = (radius
                    + placement_error
                    + pivot_uncertainty_mm
                    + (radius + rotation_world.pivot.into_iter().map(f64::abs).sum::<f64>())
                        * 32.
                        * f64::EPSILON)
                    .next_up();
                maximum_member_radius_mm = maximum_member_radius_mm.max(radius);
            }
        }
        let maximum_path_uncertainty_mm = (maximum_member_placement_mm
            + 2. * pivot_uncertainty_mm
            + 4. * g.angle_radians.abs() * maximum_member_radius_mm * axis_uncertainty)
            .next_up();
        if !maximum_path_uncertainty_mm.is_finite() || maximum_path_uncertainty_mm > t.linear / 8. {
            return Err(fail("common member path exceeds numeric precision".into()));
        }
        let clearance_check_ids = recipe
            .clearance_checks
            .iter()
            .filter(|c| c.rotation_group.as_ref() == Some(&g.id))
            .map(|c| c.id.clone())
            .collect();
        reports.insert(
            g.id.clone(),
            RigidRotationGroupReport {
                declaration: g.clone(),
                status: "named_pairs_passed".into(),
                coverage: "named_pairs_only".into(),
                assembly_content_hash: a.content_hash.clone(),
                reference,
                members,
                rotation_world,
                pivot_uncertainty_mm,
                axis_uncertainty,
                clearance_check_ids,
                maximum_member_radius_mm,
                maximum_path_uncertainty_mm,
            },
        );
    }
    Ok(reports)
}
