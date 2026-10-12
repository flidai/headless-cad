//! Bounded planar pin/bar closure using the numerical sketch engine.
//! Initial guesses select a local branch; no dynamics or global uniqueness claim.
use super::*;
use cad_sketch::{Constraint, ConstraintKind, Sketch, SketchPoint, SolveReport};
pub const MAX_MECHANISM_PINS: usize = 30;
pub const MAX_MECHANISM_LINKS: usize = 64;
pub const MAX_MECHANISM_DRIVES: usize = 16;
pub const MAX_MECHANISM_BRANCHES: usize = 32;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanarMechanism {
    pub pins: Vec<MechanismPin>,
    pub links: Vec<MechanismLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drives: Vec<MechanismAngleDrive>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<MechanismBranch>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismPin {
    pub id: String,
    pub initial: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<[f64; 2]>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismLink {
    pub id: String,
    pub from: String,
    pub to: String,
    pub length: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismAngleDrive {
    pub id: String,
    pub link: String,
    pub angle_radians: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanismBranchSide {
    Left,
    Right,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismBranch {
    pub id: String,
    pub pins: [String; 3],
    pub side: MechanismBranchSide,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MechanismBranchWitness {
    pub id: String,
    pub pins: [String; 3],
    pub side: MechanismBranchSide,
    pub signed_double_area_mm2: f64,
    pub uncertainty_margin_mm2: f64,
    pub position_margin_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MechanismReport {
    pub schema_version: u32,
    pub mechanism_build_id: String,
    /// Includes generated axis-reference pins/constraints; coordinates are local XY.
    pub solver: SolveReport,
    pub pins: BTreeMap<String, [f64; 2]>,
    /// Rigid local XY frames. The nominal bar endpoint differs from the solved
    /// endpoint by the reported length residual; no deformation is performed.
    pub link_frames: BTreeMap<String, RigidTransform>,
    pub link_residuals_mm: BTreeMap<String, f64>,
    pub branches: Vec<MechanismBranchWitness>,
}
#[derive(Debug, Error)]
pub enum MechanismError {
    #[error("invalid planar mechanism: {0}")]
    Invalid(String),
    #[error(transparent)]
    Sketch(#[from] cad_sketch::SketchError),
    #[error("planar mechanism placement rejected: {message}")]
    Rejected {
        message: String,
        report: Box<SolveReport>,
    },
}
impl MechanismError {
    pub fn solver_report(&self) -> Option<&SolveReport> {
        match self {
            Self::Rejected { report, .. } => Some(report),
            _ => None,
        }
    }
}
pub fn mechanism_build_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        hash.update(include_bytes!("mechanism.rs"));
        hash.update(cad_sketch::SOLVER_BUILD_ID.as_bytes());
        hash.update(assembly_build_id().as_bytes());
        hash.finalize().to_hex().to_string()
    })
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.trim() == id && !id.chars().any(char::is_control)
}
fn invalid(message: impl Into<String>) -> MechanismError {
    MechanismError::Invalid(message.into())
}
fn point_id(id: &str) -> String {
    format!("pin:{id}")
}
fn equations(input: &PlanarMechanism) -> Result<Sketch, MechanismError> {
    if input.pins.is_empty()
        || input.pins.len() > MAX_MECHANISM_PINS
        || input.links.is_empty()
        || input.links.len() > MAX_MECHANISM_LINKS
        || input.drives.len() > MAX_MECHANISM_DRIVES
        || input.branches.len() > MAX_MECHANISM_BRANCHES
    {
        return Err(invalid(
            "resource limit: 1..30 pins, 1..64 links, 16 drives, 32 branch witnesses",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut points = Vec::new();
    let mut constraints = Vec::new();
    for pin in &input.pins {
        if !valid_id(&pin.id) || !ids.insert(pin.id.as_str()) {
            return Err(invalid("invalid or duplicate pin ID"));
        }
        points.push(SketchPoint {
            id: point_id(&pin.id),
            position: pin.initial,
        });
        if let Some(position) = pin.ground {
            constraints.push(Constraint {
                id: format!("ground:{}", pin.id),
                kind: ConstraintKind::Fixed {
                    point: point_id(&pin.id),
                    position,
                },
            });
        }
    }
    let mut links = BTreeMap::new();
    let mut scale = 1_f64;
    for link in &input.links {
        if !valid_id(&link.id) || links.insert(link.id.as_str(), link).is_some() {
            return Err(invalid("invalid or duplicate link ID"));
        }
        if link.from == link.to
            || !ids.contains(link.from.as_str())
            || !ids.contains(link.to.as_str())
            || !link.length.is_finite()
            || link.length <= 0.
        {
            return Err(invalid(format!(
                "link '{}' requires distinct existing pins and a positive finite length",
                link.id
            )));
        }
        scale = scale.max(link.length);
        constraints.push(Constraint {
            id: format!("link:{}", link.id),
            kind: ConstraintKind::Distance {
                a: point_id(&link.from),
                b: point_id(&link.to),
                value: link.length,
            },
        });
    }
    if !input.drives.is_empty() {
        for (id, position) in [("reference:origin", [0., 0.]), ("reference:x", [scale, 0.])] {
            points.push(SketchPoint {
                id: id.into(),
                position,
            });
            constraints.push(Constraint {
                id: id.into(),
                kind: ConstraintKind::Fixed {
                    point: id.into(),
                    position,
                },
            });
        }
    }
    let mut drives = BTreeSet::new();
    for drive in &input.drives {
        if !valid_id(&drive.id) || !drives.insert(drive.id.as_str()) {
            return Err(invalid("invalid or duplicate drive ID"));
        }
        let link = links
            .get(drive.link.as_str())
            .ok_or_else(|| invalid(format!("drive '{}' references missing link", drive.id)))?;
        if !drive.angle_radians.is_finite()
            || drive.angle_radians <= -std::f64::consts::PI
            || drive.angle_radians > std::f64::consts::PI
        {
            return Err(invalid(
                "directed drive angles must lie in (-pi, pi] radians",
            ));
        }
        constraints.push(Constraint {
            id: format!("drive:{}", drive.id),
            kind: ConstraintKind::Angle {
                a: ["reference:origin".into(), "reference:x".into()],
                b: [point_id(&link.from), point_id(&link.to)],
                degrees: drive.angle_radians.to_degrees(),
            },
        });
    }
    let mut branches = BTreeSet::new();
    for branch in &input.branches {
        if !valid_id(&branch.id)
            || !branches.insert(branch.id.as_str())
            || branch.pins.iter().collect::<BTreeSet<_>>().len() != 3
            || branch.pins.iter().any(|p| !ids.contains(p.as_str()))
        {
            return Err(invalid(
                "branch requires unique ID and three distinct existing pins",
            ));
        }
    }
    if constraints.len() > cad_sketch::MAX_ISLAND_CONSTRAINTS {
        return Err(invalid("mechanism generated constraints exceed 128"));
    }
    points.sort_by(|a, b| a.id.cmp(&b.id));
    constraints.sort_by(|a, b| a.id.cmp(&b.id));
    let sketch = Sketch {
        points,
        constraints,
    };
    cad_sketch::validate(&sketch)?;
    Ok(sketch)
}
/// Structural preflight, before any iterative solve.
pub fn validate_planar_mechanism(input: &PlanarMechanism) -> Result<(), MechanismError> {
    equations(input).map(|_| ())
}
/// Accept only a converged, fully constrained local solution with explicitly
/// certified requested branch sides. Initial guesses are never silently flipped.
pub fn solve_planar_mechanism(
    input: &PlanarMechanism,
    tolerance: Tolerance,
) -> Result<MechanismReport, MechanismError> {
    let sketch = equations(input)?;
    if !tolerance.is_valid() {
        return Err(invalid("linear tolerance must be finite and positive"));
    }
    let solver = cad_sketch::solve_refined(&sketch, tolerance.linear)?;
    let reject = |message: String| MechanismError::Rejected {
        message,
        report: Box::new(solver.clone()),
    };
    if !solver.is_fully_constrained() {
        return Err(reject(format!(
            "solver status {:?}; placement requires fully constrained convergence",
            solver.status
        )));
    }
    let condition = solver
        .condition_bound
        .ok_or_else(|| reject("missing local condition bound".into()))?;
    if !condition.is_finite() || condition > cad_sketch::MAX_CONDITION_BOUND {
        return Err(reject("unresolved local conditioning".into()));
    }
    let pins: BTreeMap<_, _> = input
        .pins
        .iter()
        .map(|p| (p.id.clone(), solver.points[&point_id(&p.id)]))
        .collect();
    let scale = pins
        .values()
        .flatten()
        .copied()
        .map(f64::abs)
        .chain(input.links.iter().map(|l| l.length))
        .fold(1_f64, f64::max);
    // Conservative branch admission policy using both solver tolerances/residuals
    // and local conditioning. This is not a global coordinate-error or uniqueness proof.
    let margin = (solver.max_residual_mm.max(tolerance.linear)
        + scale
            * solver
                .max_angular_residual_radians
                .max(solver.angular_tolerance_radians)
        + scale * 256. * f64::EPSILON)
        * condition.max(1.);
    if !margin.is_finite() {
        return Err(reject("branch numeric envelope overflow".into()));
    }
    let mut branches = Vec::new();
    for branch in &input.branches {
        let [a, b, c] = branch.pins.each_ref().map(|id| pins[id]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ac = [c[0] - a[0], c[1] - a[1]];
        let cross = ab[0] * ac[1] - ab[1] * ac[0];
        let bound = 2. * margin * (ab[0].hypot(ab[1]) + ac[0].hypot(ac[1]))
            + 4. * margin * margin
            + 256. * f64::EPSILON * ((ab[0] * ac[1]).abs() + (ab[1] * ac[0]).abs());
        let signed = match branch.side {
            MechanismBranchSide::Left => cross,
            MechanismBranchSide::Right => -cross,
        };
        if !signed.is_finite() || !bound.is_finite() || signed <= bound {
            return Err(reject(format!("branch '{}' is wrong-sided or numerically unresolved (signed area {cross}, margin {bound})",branch.id)));
        }
        branches.push(MechanismBranchWitness {
            id: branch.id.clone(),
            pins: branch.pins.clone(),
            side: branch.side,
            signed_double_area_mm2: cross,
            uncertainty_margin_mm2: bound,
            position_margin_mm: margin,
        });
    }
    branches.sort_by(|a, b| a.id.cmp(&b.id));
    let mut link_frames = BTreeMap::new();
    let mut link_residuals_mm = BTreeMap::new();
    for link in &input.links {
        let a = pins[&link.from];
        let b = pins[&link.to];
        let delta = [b[0] - a[0], b[1] - a[1]];
        let length = delta[0].hypot(delta[1]);
        let residual = length - link.length;
        if !length.is_finite()
            || length <= tolerance.linear
            || !residual.is_finite()
            || residual.abs() > tolerance.linear
        {
            return Err(reject(format!(
                "link '{}' endpoint separation is unresolved",
                link.id
            )));
        }
        let angle = delta[1].atan2(delta[0]);
        let frame = RigidTransform {
            translation: [a[0], a[1], 0.],
            rotation: [0., 0., (angle / 2.).sin(), (angle / 2.).cos()],
        };
        frame
            .validate()
            .map_err(|error| reject(error.to_string()))?;
        // Certify the actual rigid placement of the nominal second endpoint,
        // including quaternion conversion and large-coordinate arithmetic. A
        // distance equation alone does not certify a representable rigid frame.
        let endpoint = frame
            .compose(
                &RigidTransform {
                    translation: [link.length, 0., 0.],
                    ..Default::default()
                },
                tolerance,
            )
            .map_err(|error| reject(format!("link '{}' frame: {error}", link.id)))?
            .translation;
        let endpoint_error = (endpoint[0] - b[0]).hypot(endpoint[1] - b[1]);
        if !endpoint_error.is_finite() || endpoint_error > tolerance.linear {
            return Err(reject(format!(
                "link '{}' rigid endpoint exceeds modeling tolerance",
                link.id
            )));
        }
        link_frames.insert(link.id.clone(), frame);
        link_residuals_mm.insert(link.id.clone(), residual);
    }
    Ok(MechanismReport {
        schema_version: 1,
        mechanism_build_id: mechanism_build_id().into(),
        solver,
        pins,
        link_frames,
        link_residuals_mm,
        branches,
    })
}
