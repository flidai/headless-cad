//! Bounded, sampled continuation of planar mechanisms. No continuous-path,
//! collision, dynamics, or global branch-continuity certification is implied.
use super::*;
pub const MAX_MOTION_TARGETS: usize = 128;
pub const MAX_MOTION_ATTEMPTS: usize = 512;
pub const MAX_MOTION_SUBDIVISION_DEPTH: usize = 16;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionStudy {
    pub targets: Vec<BTreeMap<String, f64>>,
    pub max_drive_step_radians: f64,
    pub max_pin_step_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MotionFrame {
    pub index: usize,
    pub requested_index: Option<usize>,
    pub target_index: usize,
    pub subdivision_depth: usize,
    pub coordinates: BTreeMap<String, f64>,
    pub pins: BTreeMap<String, [f64; 2]>,
    pub link_frames: BTreeMap<String, RigidTransform>,
    pub link_residuals_mm: BTreeMap<String, f64>,
    pub branches: Vec<MechanismBranchWitness>,
    pub status: cad_sketch::SolveStatus,
    pub rank: usize,
    pub degrees_of_freedom: usize,
    pub condition_bound: Option<f64>,
    pub max_residual_mm: f64,
    pub max_angular_residual_radians: f64,
    pub max_drive_step_radians: f64,
    pub max_pin_step_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MotionReport {
    pub schema_version: u32,
    pub motion_build_id: String,
    pub requested_frames: usize,
    pub completed_requested_frames: usize,
    pub attempted_solves: usize,
    pub accepted_steps: usize,
    pub subdivisions: usize,
    pub frames: Vec<MotionFrame>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MotionFailure {
    pub accepted: MotionReport,
    pub failed_coordinates: BTreeMap<String, f64>,
    pub requested_index: usize,
    pub message: String,
    pub solver_report: Option<Box<cad_sketch::SolveReport>>,
}
#[derive(Debug, Error)]
pub enum MotionError {
    #[error("invalid motion study: {0}")]
    Invalid(String),
    #[error("motion study failed: {}", .0.message)]
    Failed(Box<MotionFailure>),
}
impl MotionError {
    pub fn failure_report(&self) -> Option<&MotionFailure> {
        match self {
            Self::Failed(report) => Some(report),
            _ => None,
        }
    }
}
pub fn motion_build_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        hash.update(include_bytes!("motion.rs"));
        hash.update(mechanism_build_id().as_bytes());
        hash.finalize().to_hex().to_string()
    })
}
pub fn validate_motion_study(
    input: &PlanarMechanism,
    study: &MotionStudy,
) -> Result<(), MotionError> {
    validate_planar_mechanism(input).map_err(|e| MotionError::Invalid(e.to_string()))?;
    if !(2..=MAX_MOTION_TARGETS).contains(&study.targets.len()) || input.drives.is_empty() {
        return Err(MotionError::Invalid(
            "require 2..128 target frames and at least one prescribed drive".into(),
        ));
    }
    if !study.max_drive_step_radians.is_finite()
        || study.max_drive_step_radians <= 0.
        || !study.max_pin_step_mm.is_finite()
        || study.max_pin_step_mm <= 0.
    {
        return Err(MotionError::Invalid(
            "step limits must be finite and positive".into(),
        ));
    }
    let initial: BTreeMap<_, _> = input
        .drives
        .iter()
        .map(|d| (d.id.clone(), d.angle_radians))
        .collect();
    for target in &study.targets {
        if !target.keys().eq(initial.keys())
            || target
                .values()
                .any(|v| !v.is_finite() || *v <= -std::f64::consts::PI || *v > std::f64::consts::PI)
        {
            return Err(MotionError::Invalid(
                "each target must prescribe every source drive exactly once in (-pi,pi] radians"
                    .into(),
            ));
        }
    }
    if study.targets[0] != initial {
        return Err(MotionError::Invalid(
            "first target must equal the source mechanism drive coordinates".into(),
        ));
    }
    Ok(())
}
struct TrialFailure {
    message: String,
    solver_report: Option<Box<cad_sketch::SolveReport>>,
    exhausted: bool,
}
struct Continuation<'a> {
    input: &'a PlanarMechanism,
    study: &'a MotionStudy,
    tolerance: Tolerance,
    remaining: &'a mut usize,
    report: MotionReport,
}
impl Continuation<'_> {
    fn attempt(
        &mut self,
        coordinates: &BTreeMap<String, f64>,
    ) -> Result<MechanismReport, TrialFailure> {
        if self.report.attempted_solves >= MAX_MOTION_ATTEMPTS || *self.remaining == 0 {
            return Err(TrialFailure {
                message: "motion solve attempt budget exhausted".into(),
                solver_report: None,
                exhausted: true,
            });
        }
        self.report.attempted_solves += 1;
        *self.remaining -= 1;
        let mut input = self.input.clone();
        for drive in &mut input.drives {
            drive.angle_radians = coordinates[&drive.id];
        }
        if let Some(previous) = self.report.frames.last() {
            for pin in &mut input.pins {
                pin.initial = previous.pins[&pin.id];
            }
        }
        solve_planar_mechanism(&input, self.tolerance).map_err(|error| TrialFailure {
            message: error.to_string(),
            solver_report: error.solver_report().cloned().map(Box::new),
            exhausted: false,
        })
    }
    fn failure(
        &self,
        coordinates: BTreeMap<String, f64>,
        requested_index: usize,
        failed: TrialFailure,
    ) -> MotionError {
        MotionError::Failed(Box::new(MotionFailure {
            accepted: self.report.clone(),
            failed_coordinates: coordinates,
            requested_index,
            message: failed.message,
            solver_report: failed.solver_report,
        }))
    }
    fn accept(
        &mut self,
        coordinates: BTreeMap<String, f64>,
        requested_index: Option<usize>,
        target_index: usize,
        depth: usize,
        solution: MechanismReport,
        steps: [f64; 2],
    ) {
        let solver = &solution.solver;
        let frame = MotionFrame {
            index: self.report.frames.len(),
            requested_index,
            target_index,
            subdivision_depth: depth,
            coordinates,
            pins: solution.pins,
            link_frames: solution.link_frames,
            link_residuals_mm: solution.link_residuals_mm,
            branches: solution.branches,
            status: solver.status,
            rank: solver.rank,
            degrees_of_freedom: solver.degrees_of_freedom,
            condition_bound: solver.condition_bound,
            max_residual_mm: solver.max_residual_mm,
            max_angular_residual_radians: solver.max_angular_residual_radians,
            max_drive_step_radians: steps[0],
            max_pin_step_mm: steps[1],
        };
        if requested_index.is_some() {
            self.report.completed_requested_frames += 1;
        }
        self.report.frames.push(frame);
        self.report.accepted_steps = self.report.frames.len().saturating_sub(1);
    }
    fn advance(
        &mut self,
        target: BTreeMap<String, f64>,
        requested_index: Option<usize>,
        target_index: usize,
        depth: usize,
    ) -> Result<(), MotionError> {
        let previous = self.report.frames.last().expect("initial solve accepted");
        let start = previous.coordinates.clone();
        let drive_step = target
            .iter()
            .map(|(id, v)| (v - start[id]).abs())
            .fold(0., f64::max);
        let failed = if drive_step > self.study.max_drive_step_radians {
            TrialFailure {
                message: "drive increment requires subdivision".into(),
                solver_report: None,
                exhausted: false,
            }
        } else {
            match self.attempt(&target) {
                Ok(solution) => {
                    let previous = self.report.frames.last().unwrap();
                    let pin_step = solution
                        .pins
                        .iter()
                        .map(|(id, p)| {
                            let before = previous.pins[id];
                            (p[0] - before[0]).hypot(p[1] - before[1])
                        })
                        .fold(0., f64::max);
                    if pin_step.is_finite() && pin_step <= self.study.max_pin_step_mm {
                        self.accept(
                            target,
                            requested_index,
                            target_index,
                            depth,
                            solution,
                            [drive_step, pin_step],
                        );
                        return Ok(());
                    }
                    TrialFailure {
                        message: "solved pin displacement requires subdivision".into(),
                        solver_report: Some(Box::new(solution.solver)),
                        exhausted: false,
                    }
                }
                Err(error) => error,
            }
        };
        if failed.exhausted || depth >= MAX_MOTION_SUBDIVISION_DEPTH || start == target {
            let mut failed = failed;
            if depth >= MAX_MOTION_SUBDIVISION_DEPTH {
                failed.message = format!("subdivision depth limit reached: {}", failed.message);
            }
            return Err(self.failure(target, target_index, failed));
        }
        let midpoint: BTreeMap<_, _> = target
            .iter()
            .map(|(id, v)| (id.clone(), start[id] + (v - start[id]) / 2.))
            .collect();
        if midpoint == start || midpoint == target {
            return Err(self.failure(
                target,
                target_index,
                TrialFailure {
                    message: "midpoint is numerically unrepresentable".into(),
                    ..failed
                },
            ));
        }
        self.report.subdivisions += 1;
        self.advance(midpoint, None, target_index, depth + 1)?;
        self.advance(target, requested_index, target_index, depth + 1)
    }
}
/// Warm-start each actual solve from the last accepted pin coordinates. Only
/// accepted sampled steps meet the supplied bounds; intervals are not certified.
/// Every attempted solve, including rejected trials, consumes the shared budget.
pub fn continue_planar_mechanism(
    input: &PlanarMechanism,
    study: &MotionStudy,
    tolerance: Tolerance,
    remaining_attempts: &mut usize,
) -> Result<MotionReport, MotionError> {
    validate_motion_study(input, study)?;
    if !tolerance.is_valid() {
        return Err(MotionError::Invalid(
            "linear tolerance must be finite and positive".into(),
        ));
    }
    let mut state = Continuation {
        input,
        study,
        tolerance,
        remaining: remaining_attempts,
        report: MotionReport {
            schema_version: 1,
            motion_build_id: motion_build_id().into(),
            requested_frames: study.targets.len(),
            completed_requested_frames: 0,
            attempted_solves: 0,
            accepted_steps: 0,
            subdivisions: 0,
            frames: Vec::new(),
        },
    };
    let first = study.targets[0].clone();
    match state.attempt(&first) {
        Ok(solution) => state.accept(first, Some(0), 0, 0, solution, [0., 0.]),
        Err(failed) => return Err(state.failure(first, 0, failed)),
    }
    for (index, target) in study.targets.iter().enumerate().skip(1) {
        state.advance(target.clone(), Some(index), index, 0)?;
    }
    Ok(state.report)
}
