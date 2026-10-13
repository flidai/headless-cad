//! Source-owned common rigid translation; only explicitly named pairs are certified.
use super::*;

pub const MAX_TRANSLATION_GROUPS: usize = 16;
pub const MAX_TRANSLATION_GROUP_MEMBERS: usize = 64;
pub const MAX_TRANSLATION_GROUP_MEMBERS_TOTAL: usize = 256;

/// The transform visits analytic support origins as well as topology vertices.
/// Some raw origins can be outside the occupied solid's tight bounds.
pub(super) fn raw_support_scale(model: &cad_kernel::ModelSnapshot) -> f64 {
    let mut scale = 0_f64;
    for vertex in &model.vertices {
        scale = vertex
            .position
            .into_iter()
            .map(f64::abs)
            .fold(scale, f64::max);
    }
    for edge in &model.edges {
        let (origin, radius) = match &edge.curve {
            cad_kernel::Curve::Line(g) => (g.origin, 0.),
            cad_kernel::Curve::Circle(g) => (g.origin, g.radius),
            cad_kernel::Curve::CircularArc(g) => (g.circle.origin, g.circle.radius),
        };
        scale = origin
            .into_iter()
            .map(f64::abs)
            .fold(scale, f64::max)
            .max(radius.abs());
    }
    for face in &model.faces {
        let (origin, radius) = match &face.surface {
            cad_kernel::Surface::Plane(g) => (g.origin, 0.),
            cad_kernel::Surface::Cylinder(g) => (g.origin, g.radius),
            cad_kernel::Surface::Cone(g) => (g.origin, g.radius),
        };
        scale = origin
            .into_iter()
            .map(f64::abs)
            .fold(scale, f64::max)
            .max(radius.abs());
    }
    scale
}

/// Bound arithmetic from every authored occurrence transform, even when large
/// parent and child offsets cancel in the final flattened frame. Attachments
/// have solved placement provenance outside this simple path model, so group
/// motion refuses them until their complete derivation is charged explicitly.
pub(super) fn ancestry_translation_scale(
    assembly: &cad_assembly::BuiltAssembly,
    path: &[String],
) -> Result<f64, &'static str> {
    let mut definition = assembly
        .hierarchy
        .get(&assembly.assembly_id)
        .ok_or("missing root assembly definition")?;
    let mut scale = 0_f64;
    for (index, name) in path.iter().enumerate() {
        let last = index + 1 == path.len();
        if last {
            let leaf = definition
                .instances
                .iter()
                .find(|i| &i.id == name)
                .ok_or("missing authoritative leaf path")?;
            if leaf.attachment.is_some() {
                return Err("attached leaf frame is not supported for rigid group motion");
            }
            scale += leaf
                .transform
                .translation
                .into_iter()
                .map(f64::abs)
                .sum::<f64>();
        } else {
            let child = definition
                .assemblies
                .iter()
                .find(|i| &i.id == name)
                .ok_or("missing authoritative ancestor path")?;
            if child.attachment.is_some() {
                return Err("attached ancestor frame is not supported for rigid group motion");
            }
            scale += child
                .transform
                .translation
                .into_iter()
                .map(f64::abs)
                .sum::<f64>();
            definition = assembly
                .hierarchy
                .get(&child.assembly)
                .ok_or("missing authoritative child assembly")?;
        }
        if !scale.is_finite() {
            return Err("nonfinite ancestor translation scale");
        }
    }
    Ok(scale)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidTranslationGroup {
    pub id: String,
    pub output: String,
    pub instance_paths: Vec<Vec<String>>,
    pub reference_instance_path: Vec<String>,
    /// Signed displacement in the initial reference occurrence's body frame.
    pub translation: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct RigidTranslationGroupReport {
    pub declaration: RigidTranslationGroup,
    pub status: String,
    pub coverage: String,
    pub assembly_content_hash: String,
    pub reference: ClearanceOccurrenceWitness,
    pub members: Vec<ClearanceOccurrenceWitness>,
    pub translation_world: [f64; 3],
    pub translation_uncertainty_mm: f64,
    pub clearance_check_ids: Vec<String>,
    pub maximum_path_uncertainty_mm: f64,
}

impl RigidTranslationGroup {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidTranslationGroup {
            id: self.id.clone(),
            output: self.output.clone(),
            message: message.into(),
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
            || self.instance_paths.len() > MAX_TRANSLATION_GROUP_MEMBERS
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
        let length = self.translation.into_iter().fold(0_f64, f64::hypot);
        if !self.translation.into_iter().all(f64::is_finite)
            || length == 0.
            || length > MAX_CLEARANCE_TRANSLATION_MM
        {
            return Err(fail(
                "translation requires a nonzero finite local vector of length at most 1000000 mm",
            ));
        }
        Ok(())
    }
}

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.rigid_translation_groups.len() > MAX_TRANSLATION_GROUPS {
        return Err(BuildError::ResourceLimit {
            resource: "rigid translation groups",
            limit: MAX_TRANSLATION_GROUPS,
        });
    }
    let mut ids = BTreeSet::new();
    let mut members = BTreeSet::new();
    let mut count = 0;
    for g in &recipe.rigid_translation_groups {
        g.validate()?;
        let fail = |message: &str| BuildError::InvalidTranslationGroup {
            id: g.id.clone(),
            output: g.output.clone(),
            message: message.into(),
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
        if count > MAX_TRANSLATION_GROUP_MEMBERS_TOTAL {
            return Err(BuildError::ResourceLimit {
                resource: "rigid translation group members",
                limit: MAX_TRANSLATION_GROUP_MEMBERS_TOTAL,
            });
        }
        for p in &g.instance_paths {
            if recipe
                .rigid_rotation_groups
                .iter()
                .any(|r| r.output == g.output && r.instance_paths.contains(p))
                || !members.insert((&g.output, p))
            {
                return Err(fail(
                    "overlapping motion groups on the same output are unsupported",
                ));
            }
        }
        let checks: Vec<_> = recipe
            .clearance_checks
            .iter()
            .filter(|c| c.translation_group.as_ref() == Some(&g.id))
            .collect();
        if checks.is_empty() {
            return Err(fail(
                "group requires at least one explicitly referenced clearance pair",
            ));
        }
        if checks.iter().any(|c| {
            c.output != g.output
                || !g.instance_paths.contains(&c.left_instance_path)
                    && !g.instance_paths.contains(&c.right_instance_path)
        }) {
            return Err(fail(
                "referenced pair must share group output and contain a moving member",
            ));
        }
    }
    for c in &recipe.clearance_checks {
        if c.translation_group
            .as_ref()
            .is_some_and(|id| !recipe.rigid_translation_groups.iter().any(|g| g.id == *id))
        {
            return Err(BuildError::InvalidClearanceCheck {
                id: c.id.clone(),
                output: c.output.clone(),
                message: "translation group reference does not exist".into(),
            });
        }
    }
    Ok(())
}

pub(super) fn evaluate(
    recipe: &Recipe,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    t: cad_kernel::Tolerance,
) -> Result<BTreeMap<String, RigidTranslationGroupReport>, BuildError> {
    let mut reports = BTreeMap::new();
    let mut work = 0usize;
    for g in &recipe.rigid_translation_groups {
        let fail = |message: String| BuildError::TranslationGroupFailed {
            id: g.id.clone(),
            output: g.output.clone(),
            message,
        };
        let assembly = assemblies
            .get(&g.output)
            .ok_or_else(|| fail("assembly output was not evaluated".into()))?;
        let mut members = Vec::new();
        let mut maximum_placement = 0_f64;
        for path in &g.instance_paths {
            let cost = path
                .iter()
                .fold(1usize, |n, s| n.saturating_add(s.len() + 1));
            work = work.saturating_add(assembly.instances.len().saturating_mul(cost));
            if work > MAX_CLEARANCE_RESOLVE_WORK {
                return Err(BuildError::ResourceLimit {
                    resource: "translation group path resolution",
                    limit: MAX_CLEARANCE_RESOLVE_WORK,
                });
            }
            let mut found = assembly
                .instances
                .iter()
                .filter(|i| assembly.instance_paths.get(&i.id) == Some(path));
            let instance = found
                .next()
                .ok_or_else(|| fail(format!("missing exact moving leaf path {path:?}")))?;
            if found.next().is_some() {
                return Err(fail("ambiguous moving leaf path".into()));
            }
            let part = &assembly.parts[&instance.part];
            let model = &part.result.model;
            work = work
                .saturating_add(model.vertices.len())
                .saturating_add(model.edges.len())
                .saturating_add(model.faces.len());
            if work > MAX_CLEARANCE_RESOLVE_WORK {
                return Err(BuildError::ResourceLimit {
                    resource: "translation group geometry work",
                    limit: MAX_CLEARANCE_RESOLVE_WORK,
                });
            }
            let bounds = model.bounds();
            let ancestry = ancestry_translation_scale(assembly, path)
                .map_err(|message| fail(message.into()))?;
            let scale = bounds
                .min
                .into_iter()
                .chain(bounds.max)
                .chain(instance.transform.translation)
                .map(f64::abs)
                .sum::<f64>()
                + raw_support_scale(model);
            let error =
                ((scale + ancestry) * (1024. + 128. * path.len() as f64) * f64::EPSILON).next_up();
            if !error.is_finite() || error > t.linear / 32. {
                return Err(fail("member placement exceeds numeric precision".into()));
            }
            maximum_placement = maximum_placement.max(error);
            members.push(ClearanceOccurrenceWitness {
                instance_id: instance.id.clone(),
                instance_path: path.clone(),
                part_id: instance.part.clone(),
                feature_id: part.feature_id.clone(),
                part_content_hash: part.content_hash.clone(),
                frame: instance.transform,
            });
        }
        let reference = members
            .iter()
            .find(|m| m.instance_path == g.reference_instance_path)
            .unwrap()
            .clone();
        let translation_world = reference.frame.vector(g.translation);
        let translation_uncertainty_mm = (g.translation.into_iter().map(f64::abs).sum::<f64>()
            * (1024. + 128. * reference.instance_path.len() as f64)
            * f64::EPSILON)
            .next_up();
        let maximum_path_uncertainty_mm =
            (maximum_placement + translation_uncertainty_mm).next_up();
        if !translation_world.into_iter().all(f64::is_finite)
            || !maximum_path_uncertainty_mm.is_finite()
            || maximum_path_uncertainty_mm > t.linear / 8.
        {
            return Err(fail(
                "common translation frame exceeds numeric precision".into(),
            ));
        }
        let clearance_check_ids = recipe
            .clearance_checks
            .iter()
            .filter(|c| c.translation_group.as_ref() == Some(&g.id))
            .map(|c| c.id.clone())
            .collect();
        reports.insert(
            g.id.clone(),
            RigidTranslationGroupReport {
                declaration: g.clone(),
                status: "named_pairs_passed".into(),
                coverage: "named_pairs_only".into(),
                assembly_content_hash: assembly.content_hash.clone(),
                reference,
                members,
                translation_world,
                translation_uncertainty_mm,
                clearance_check_ids,
                maximum_path_uncertainty_mm,
            },
        );
    }
    Ok(reports)
}
