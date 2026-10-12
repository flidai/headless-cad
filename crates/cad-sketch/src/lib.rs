//! Bounded deterministic 2D sketch equations, independent of any geometry kernel.
//! Results describe convergence and local Jacobian rank, not global consistency,
//! unique solutions or branch continuity. Initial positions select a local branch.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
mod deserialize;

/// Independent convergence tolerance for angular equations, in radians.
pub const ANGULAR_TOLERANCE_RADIANS: f64 = 1e-9;
pub const MAX_POINTS: usize = 32;
pub const MAX_CONSTRAINTS: usize = 128;
pub const MAX_ITERATIONS: usize = 256;
/// Conservative local Jacobian conditioning bound required for full acceptance.
pub const MAX_CONDITION_BOUND: f64 = 1e8;
pub const SOLVER_BUILD_ID: &str = env!("CAD_SKETCH_BUILD_ID");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sketch {
    pub points: Vec<SketchPoint>,
    pub constraints: Vec<Constraint>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchPoint {
    pub id: String,
    pub position: [f64; 2],
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Constraint {
    pub id: String,
    #[serde(flatten)]
    pub kind: ConstraintKind,
}
/// Side of a directed supporting line, viewed in the sketch XY plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineSide {
    Left,
    Right,
}
impl LineSide {
    fn sign(self) -> f64 {
        match self {
            Self::Left => 1.,
            Self::Right => -1.,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstraintKind {
    Fixed {
        point: String,
        position: [f64; 2],
    },
    Coincident {
        a: String,
        b: String,
    },
    Horizontal {
        a: String,
        b: String,
    },
    Vertical {
        a: String,
        b: String,
    },
    Distance {
        a: String,
        b: String,
        value: f64,
    },
    /// Circle center/radius handle tangent to an infinite supporting line.
    /// The signed side is explicit; contact need not lie inside the segment.
    CircleLineTangent {
        circle: [String; 2],
        line: [String; 2],
        side: LineSide,
    },
    /// Equality of the two endpoint-pair segment lengths.
    EqualLength {
        a: [String; 2],
        b: [String; 2],
    },
    /// Unoriented parallelism (angles modulo pi).
    Parallel {
        a: [String; 2],
        b: [String; 2],
    },
    /// Unoriented perpendicularity (angles modulo pi).
    Perpendicular {
        a: [String; 2],
        b: [String; 2],
    },
    /// Directed angle from a to b, degrees in (-180, 180].
    Angle {
        a: [String; 2],
        b: [String; 2],
        degrees: f64,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveStatus {
    FullyConstrained,
    Underconstrained,
    NotConverged,
    NumericalFailure,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstraintResidual {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_radians: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolveReport {
    pub schema_version: u32,
    pub angular_tolerance_radians: f64,
    pub max_angular_residual_radians: f64,
    pub solver_build_id: String,
    pub status: SolveStatus,
    pub tolerance_mm: f64,
    pub points: BTreeMap<String, [f64; 2]>,
    pub iterations: usize,
    pub max_residual_mm: f64,
    pub rank: usize,
    /// Conditioning of normalized linear/radian Jacobian rows, not a
    /// world-coordinate error bound or a proof of global uniqueness.
    pub condition_bound: Option<f64>,
    pub degrees_of_freedom: usize,
    pub redundant_equations: usize,
    pub constraint_residuals: Vec<ConstraintResidual>,
    pub message: Option<String>,
}
impl SolveReport {
    pub fn is_fully_constrained(&self) -> bool {
        self.status == SolveStatus::FullyConstrained
    }
}
#[derive(Debug, Error)]
pub enum SketchError {
    #[error("sketch supports 1..=32 points and at most 128 constraints")]
    ResourceLimit,
    #[error("invalid or duplicate {kind} identifier '{id}'")]
    Identifier { kind: &'static str, id: String },
    #[error("constraint '{constraint}' references missing point '{point}'")]
    MissingPoint { constraint: String, point: String },
    #[error("invalid constraint '{id}': {message}")]
    Constraint { id: String, message: &'static str },
    #[error("coordinates and distances must be finite, within ±1e12 mm; nonzero coordinates below 1e-100 mm are unsupported")]
    NumericInput,
    #[error("solver tolerance must be finite and positive")]
    InvalidTolerance,
}
fn finite_coordinate(v: f64) -> bool {
    v.is_finite() && v.abs() <= 1e12 && (v == 0. || v.abs() >= 1e-100)
}
fn identifier(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.trim() == id && !id.chars().any(char::is_control)
}
pub fn validate(sketch: &Sketch) -> Result<(), SketchError> {
    if sketch.points.is_empty()
        || sketch.points.len() > MAX_POINTS
        || sketch.constraints.len() > MAX_CONSTRAINTS
    {
        return Err(SketchError::ResourceLimit);
    }
    let mut ids = BTreeSet::new();
    for p in &sketch.points {
        if !identifier(&p.id) || !ids.insert(p.id.as_str()) {
            return Err(SketchError::Identifier {
                kind: "point",
                id: p.id.clone(),
            });
        }
        if !p.position.into_iter().all(finite_coordinate) {
            return Err(SketchError::NumericInput);
        }
    }
    let mut constraints = BTreeSet::new();
    for c in &sketch.constraints {
        if !identifier(&c.id) || !constraints.insert(c.id.as_str()) {
            return Err(SketchError::Identifier {
                kind: "constraint",
                id: c.id.clone(),
            });
        }
        let points = match &c.kind {
            ConstraintKind::Fixed { point, position } => {
                if !position.iter().copied().all(finite_coordinate) {
                    return Err(SketchError::NumericInput);
                }
                vec![point]
            }
            ConstraintKind::CircleLineTangent {
                circle: a, line: b, ..
            }
            | ConstraintKind::EqualLength { a, b }
            | ConstraintKind::Parallel { a, b }
            | ConstraintKind::Perpendicular { a, b }
            | ConstraintKind::Angle { a, b, .. } => {
                if a[0] == a[1] || b[0] == b[1] {
                    return Err(SketchError::Constraint {
                        id: c.id.clone(),
                        message: "segments require distinct endpoint IDs",
                    });
                }
                if let ConstraintKind::Angle { degrees, .. } = &c.kind {
                    if !degrees.is_finite() || *degrees <= -180. || *degrees > 180. {
                        return Err(SketchError::Constraint {
                            id: c.id.clone(),
                            message: "directed angle degrees must be finite in (-180, 180]",
                        });
                    }
                }
                vec![&a[0], &a[1], &b[0], &b[1]]
            }
            ConstraintKind::Coincident { a, b }
            | ConstraintKind::Horizontal { a, b }
            | ConstraintKind::Vertical { a, b }
            | ConstraintKind::Distance { a, b, .. } => {
                if a == b {
                    return Err(SketchError::Constraint {
                        id: c.id.clone(),
                        message: "pair constraints require two distinct point IDs",
                    });
                }
                vec![a, b]
            }
        };
        for point in points {
            if !ids.contains(point.as_str()) {
                return Err(SketchError::MissingPoint {
                    constraint: c.id.clone(),
                    point: point.clone(),
                });
            }
        }
        if let ConstraintKind::Distance { value, .. } = c.kind {
            if !finite_coordinate(value) || value <= 0. {
                return Err(SketchError::Constraint {
                    id: c.id.clone(),
                    message: "distance must be finite and positive within the numeric range",
                });
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
enum Equation {
    Fixed(usize, f64),
    Delta(usize, usize),
    Distance(usize, usize, f64),
    EqualLength([usize; 2], [usize; 2]),
    CircleLineTangent([usize; 2], [usize; 2], LineSide),
    Angle([usize; 2], [usize; 2], f64, f64),
}
struct System {
    ids: Vec<String>,
    equations: Vec<Equation>,
    groups: Vec<(String, std::ops::Range<usize>)>,
    origin: [f64; 2],
    scale: f64,
    world_scale: f64,
    initial: Vec<f64>,
    tolerance_mm: f64,
}
impl System {
    fn new(sketch: &Sketch, tolerance_mm: f64) -> Self {
        let points: BTreeMap<_, _> = sketch
            .points
            .iter()
            .map(|p| (p.id.clone(), p.position))
            .collect();
        let origin = *points.values().next().unwrap();
        let mut scale = 0f64;
        let mut world_scale = 0f64;
        let mut include = |p: [f64; 2]| {
            for i in 0..2 {
                scale = scale.max((p[i] - origin[i]).abs());
                world_scale = world_scale.max(p[i].abs());
            }
        };
        for &p in points.values() {
            include(p)
        }
        for c in &sketch.constraints {
            if let ConstraintKind::Fixed { position, .. } = c.kind {
                include(position)
            }
        }
        for c in &sketch.constraints {
            if let ConstraintKind::Distance { value, .. } = c.kind {
                scale = scale.max(value);
                world_scale = world_scale.max(value)
            }
        }
        if scale == 0. {
            scale = 1.;
        }
        let initial = points
            .values()
            .flat_map(|p| [(p[0] - origin[0]) / scale, (p[1] - origin[1]) / scale])
            .collect();
        let indices: BTreeMap<_, _> = points
            .keys()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i * 2))
            .collect();
        let constraints: BTreeMap<_, _> = sketch
            .constraints
            .iter()
            .map(|c| (&c.id, &c.kind))
            .collect();
        let mut equations = vec![];
        let mut groups = vec![];
        for (id, c) in constraints {
            let start = equations.len();
            match c {
                ConstraintKind::Fixed { point, position } => {
                    let index = indices[point.as_str()];
                    for i in 0..2 {
                        equations.push(Equation::Fixed(
                            index + i,
                            (position[i] - origin[i]) / scale,
                        ));
                    }
                }
                ConstraintKind::Coincident { a, b } => {
                    for i in 0..2 {
                        equations.push(Equation::Delta(
                            indices[a.as_str()] + i,
                            indices[b.as_str()] + i,
                        ));
                    }
                }
                ConstraintKind::Horizontal { a, b } => equations.push(Equation::Delta(
                    indices[a.as_str()] + 1,
                    indices[b.as_str()] + 1,
                )),
                ConstraintKind::Vertical { a, b } => {
                    equations.push(Equation::Delta(indices[a.as_str()], indices[b.as_str()]))
                }
                ConstraintKind::CircleLineTangent { circle, line, side } => {
                    equations.push(Equation::CircleLineTangent(
                        circle.each_ref().map(|v| indices[v.as_str()]),
                        line.each_ref().map(|v| indices[v.as_str()]),
                        *side,
                    ))
                }
                ConstraintKind::EqualLength { a, b } => equations.push(Equation::EqualLength(
                    a.each_ref().map(|v| indices[v.as_str()]),
                    b.each_ref().map(|v| indices[v.as_str()]),
                )),
                ConstraintKind::Parallel { a, b }
                | ConstraintKind::Perpendicular { a, b }
                | ConstraintKind::Angle { a, b, .. } => {
                    let (target, period) = match c {
                        ConstraintKind::Parallel { .. } => (0., std::f64::consts::PI),
                        ConstraintKind::Perpendicular { .. } => {
                            (std::f64::consts::FRAC_PI_2, std::f64::consts::PI)
                        }
                        ConstraintKind::Angle { degrees, .. } => {
                            (degrees.to_radians(), std::f64::consts::TAU)
                        }
                        _ => unreachable!(),
                    };
                    equations.push(Equation::Angle(
                        a.each_ref().map(|v| indices[v.as_str()]),
                        b.each_ref().map(|v| indices[v.as_str()]),
                        target,
                        period,
                    ));
                }
                ConstraintKind::Distance { a, b, value } => equations.push(Equation::Distance(
                    indices[a.as_str()],
                    indices[b.as_str()],
                    *value / scale,
                )),
            }
            groups.push((id.clone(), start..equations.len()));
        }
        Self {
            ids: points.into_keys().collect(),
            equations,
            groups,
            origin,
            scale,
            world_scale,
            initial,
            tolerance_mm,
        }
    }
    fn evaluate(&self, x: &[f64]) -> Evaluation {
        let n = x.len();
        let mut residuals = Vec::with_capacity(self.equations.len());
        let mut jacobian = Vec::with_capacity(self.equations.len());
        let mut singular = false;
        let mut linear_roundoff = 0f64;
        for eq in &self.equations {
            let mut row = vec![0.; n];
            let residual = match *eq {
                Equation::Fixed(i, target) => {
                    row[i] = 1.;
                    x[i] - target
                }
                Equation::Delta(a, b) => {
                    row[a] = 1.;
                    row[b] = -1.;
                    x[a] - x[b]
                }
                Equation::CircleLineTangent(circle, line, side) => {
                    let [c, r] = circle;
                    let [a, b] = line;
                    let u = [x[b] - x[a], x[b + 1] - x[a + 1]];
                    let w = [x[c] - x[a], x[c + 1] - x[a + 1]];
                    let q = [x[r] - x[c], x[r + 1] - x[c + 1]];
                    let length = u[0].hypot(u[1]);
                    let radius = q[0].hypot(q[1]);
                    let world = x
                        .iter()
                        .map(|v| v.abs() * self.scale)
                        .fold(self.world_scale, f64::max);
                    let coordinate_error = 256. * f64::EPSILON * world / self.scale;
                    let valid = length > (64. * f64::EPSILON).max(16. * coordinate_error)
                        && radius > (64. * f64::EPSILON).max(16. * coordinate_error);
                    let distance = if length > 0. {
                        (u[0] / length) * w[1] - (u[1] / length) * w[0]
                    } else {
                        0.
                    };
                    if valid {
                        let du = [
                            w[1] / length - distance * (u[0] / length) / length,
                            -w[0] / length - distance * (u[1] / length) / length,
                        ];
                        let dw = [-u[1] / length, u[0] / length];
                        for axis in 0..2 {
                            row[a + axis] -= du[axis] + dw[axis];
                            row[b + axis] += du[axis];
                            row[c + axis] += dw[axis] + side.sign() * q[axis] / radius;
                            row[r + axis] -= side.sign() * q[axis] / radius;
                        }
                        // Far-away contact can amplify endpoint direction error.
                        // Reserve uncertainty using the uncombined derivative
                        // magnitudes, including arithmetic in the signed distance.
                        let amplification = 4. + 2. * w[0].hypot(w[1]) / length;
                        let error = coordinate_error * amplification;
                        linear_roundoff = linear_roundoff.max(error);
                        if error * self.scale > self.tolerance_mm / 16. {
                            singular = true;
                        }
                    } else {
                        singular = true;
                    }
                    distance - side.sign() * radius
                }
                Equation::EqualLength(a, b) | Equation::Angle(a, b, _, _) => {
                    let u = [x[a[1]] - x[a[0]], x[a[1] + 1] - x[a[0] + 1]];
                    let v = [x[b[1]] - x[b[0]], x[b[1] + 1] - x[b[0] + 1]];
                    let lengths = [u[0].hypot(u[1]), v[0].hypot(v[1])];
                    let angular = matches!(eq, Equation::Angle(..));
                    let world = x
                        .iter()
                        .map(|v| v.abs() * self.scale)
                        .fold(self.world_scale, f64::max);
                    let angular_roundoff =
                        256. * f64::EPSILON * world / self.scale / lengths[0].min(lengths[1]);
                    let valid = lengths.iter().all(|d| *d > 64. * f64::EPSILON)
                        && (!angular || angular_roundoff <= ANGULAR_TOLERANCE_RADIANS / 16.);
                    if !valid {
                        singular = true;
                    }
                    if let Equation::Angle(_, _, target, period) = *eq {
                        let raw = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
                        let residual = wrap_angle(raw - target, period);
                        // At the residual branch cut there is no unique local derivative.
                        if period / 2. - residual.abs() <= 1e-8 {
                            singular = true;
                        }
                        if valid {
                            for (ends, vector, length, sign) in
                                [(a, u, lengths[0], -1.), (b, v, lengths[1], 1.)]
                            {
                                let derivative = [
                                    -vector[1] / length / length * sign,
                                    vector[0] / length / length * sign,
                                ];
                                for axis in 0..2 {
                                    row[ends[1] + axis] += derivative[axis];
                                    row[ends[0] + axis] -= derivative[axis];
                                }
                            }
                        }
                        residual
                    } else {
                        if valid {
                            for (ends, vector, length, sign) in
                                [(a, u, lengths[0], 1.), (b, v, lengths[1], -1.)]
                            {
                                for axis in 0..2 {
                                    let d = sign * vector[axis] / length;
                                    row[ends[1] + axis] += d;
                                    row[ends[0] + axis] -= d;
                                }
                            }
                        }
                        lengths[0] - lengths[1]
                    }
                }
                Equation::Distance(a, b, target) => {
                    let dx = x[a] - x[b];
                    let dy = x[a + 1] - x[b + 1];
                    let d = dx.hypot(dy);
                    if d <= 64. * f64::EPSILON {
                        singular = true;
                    } else {
                        row[a] = dx / d;
                        row[a + 1] = dy / d;
                        row[b] = -row[a];
                        row[b + 1] = -row[a + 1];
                    }
                    d - target
                }
            };
            residuals.push(residual);
            jacobian.push(row);
        }
        Evaluation {
            residuals,
            jacobian,
            singular,
            linear_roundoff,
        }
    }
    fn max_residual(&self, e: &Evaluation) -> f64 {
        self.max_residual_kind(e, false)
    }
    fn max_angular_residual(&self, e: &Evaluation) -> f64 {
        self.max_residual_kind(e, true)
    }
    fn max_residual_kind(&self, e: &Evaluation, angular: bool) -> f64 {
        self.groups
            .iter()
            .filter(|(_, range)| {
                matches!(self.equations[range.start], Equation::Angle(..)) == angular
            })
            .map(|(_, range)| {
                range
                    .clone()
                    .fold(0f64, |norm, i| norm.hypot(e.residuals[i]))
            })
            .fold(0., f64::max)
    }
    fn report(
        &self,
        x: &[f64],
        _e: &Evaluation,
        mut status: SolveStatus,
        iterations: usize,
        mut message: Option<String>,
    ) -> SolveReport {
        // Evidence refers to the returned rounded world coordinates, not hidden
        // normalized iterates that cannot be reproduced by a downstream consumer.
        let points: BTreeMap<_, _> = self
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                (
                    id.clone(),
                    [
                        self.origin[0] + x[2 * i] * self.scale,
                        self.origin[1] + x[2 * i + 1] * self.scale,
                    ],
                )
            })
            .collect();
        let rounded: Vec<_> = points
            .values()
            .flat_map(|p| {
                [
                    (p[0] - self.origin[0]) / self.scale,
                    (p[1] - self.origin[1]) / self.scale,
                ]
            })
            .collect();
        let e = self.evaluate(&rounded);
        let (rank, condition_bound) = rank_condition(&e.jacobian, x.len());
        if matches!(
            status,
            SolveStatus::FullyConstrained | SolveStatus::Underconstrained
        ) {
            if e.singular
                || (self.max_residual(&e) + e.linear_roundoff) * self.scale > self.tolerance_mm
                || self.max_angular_residual(&e) > ANGULAR_TOLERANCE_RADIANS
            {
                status = SolveStatus::NumericalFailure;
                message=Some("returned world coordinates cannot certify convergence at the requested tolerance".into());
            } else {
                status = if rank == x.len() {
                    SolveStatus::FullyConstrained
                } else {
                    SolveStatus::Underconstrained
                };
            }
        }
        if status == SolveStatus::FullyConstrained
            && condition_bound.is_none_or(|c| c > MAX_CONDITION_BOUND)
        {
            status = SolveStatus::NumericalFailure;
            message=Some("local Jacobian conditioning exceeds the supported bound; rank alone does not certify a stable solution".into());
        }
        SolveReport {
            schema_version: 2,
            angular_tolerance_radians: ANGULAR_TOLERANCE_RADIANS,
            max_angular_residual_radians: self.max_angular_residual(&e),
            solver_build_id: SOLVER_BUILD_ID.into(),
            status,
            points,
            tolerance_mm: self.tolerance_mm,
            iterations,
            max_residual_mm: self.max_residual(&e) * self.scale,
            rank,
            condition_bound,
            degrees_of_freedom: x.len() - rank,
            redundant_equations: e.residuals.len() - rank,
            constraint_residuals: self
                .groups
                .iter()
                .map(|(id, range)| {
                    let angular = matches!(self.equations[range.start], Equation::Angle(..));
                    let norm = range
                        .clone()
                        .fold(0f64, |norm, i| norm.hypot(e.residuals[i]));
                    ConstraintResidual {
                        id: id.clone(),
                        residual_mm: (!angular).then_some(norm * self.scale),
                        residual_radians: angular.then_some(norm),
                    }
                })
                .collect(),
            message,
        }
    }
}
fn wrap_angle(angle: f64, period: f64) -> f64 {
    (angle + period / 2.).rem_euclid(period) - period / 2.
}
struct Evaluation {
    residuals: Vec<f64>,
    jacobian: Vec<Vec<f64>>,
    singular: bool,
    linear_roundoff: f64,
}
impl Evaluation {
    fn objective(&self) -> f64 {
        self.residuals.iter().map(|v| v * v).sum()
    }
}

/// Column-pivoted Householder QR exposes local rank without squaring the
/// condition number. Rows mix normalized linear coordinates and radians; this
/// condition bound is not a world-coordinate error bound.
#[allow(clippy::needless_range_loop)] // Explicit row/column indices express Householder updates.
fn rank_condition(matrix: &[Vec<f64>], n: usize) -> (usize, Option<f64>) {
    let m = matrix.len();
    if m == 0 {
        return (0, None);
    }
    let mut a = matrix.to_vec();
    let normcol = |a: &Vec<Vec<f64>>, j: usize, k: usize| {
        a[k..].iter().fold(0f64, |sum, row| sum.hypot(row[j]))
    };
    let scale = (0..n).map(|j| normcol(&a, j, 0)).fold(0., f64::max);
    let threshold = scale * (m.max(n) as f64) * 128. * f64::EPSILON;
    let mut rank = 0;
    for k in 0..m.min(n) {
        let mut pivot = k;
        let mut magnitude = normcol(&a, k, k);
        for j in k + 1..n {
            let candidate = normcol(&a, j, k);
            if candidate > magnitude {
                magnitude = candidate;
                pivot = j;
            }
        }
        if magnitude <= threshold || !magnitude.is_finite() {
            break;
        }
        for row in &mut a {
            row.swap(k, pivot)
        }
        let alpha = if a[k][k] >= 0. { -magnitude } else { magnitude };
        let mut v: Vec<_> = a[k..].iter().map(|row| row[k]).collect();
        v[0] -= alpha;
        let vn = v.iter().fold(0f64, |n, v| n.hypot(*v));
        for value in &mut v {
            *value /= vn;
        }
        for j in k..n {
            let projection = 2.
                * v.iter()
                    .enumerate()
                    .map(|(i, v)| v * a[k + i][j])
                    .sum::<f64>();
            for (i, value) in v.iter().enumerate() {
                a[k + i][j] -= projection * value;
            }
        }
        rank += 1;
    }
    let condition = if rank == n {
        let mut inverse_norm = 0f64;
        for column in 0..n {
            let mut x = vec![0.; n];
            for i in (0..n).rev() {
                x[i] = ((if i == column { 1. } else { 0. })
                    - (i + 1..n).map(|j| a[i][j] * x[j]).sum::<f64>())
                    / a[i][i];
            }
            for value in x {
                inverse_norm = inverse_norm.hypot(value);
            }
        }
        let norm = matrix.iter().flatten().fold(0f64, |n, v| n.hypot(*v));
        let bound = norm * inverse_norm;
        if bound.is_finite() {
            Some(bound)
        } else {
            None
        }
    } else {
        None
    };
    (rank, condition)
}
fn rank(matrix: &[Vec<f64>], n: usize) -> usize {
    rank_condition(matrix, n).0
}
/// Pivoted solve for the damped normal system. Damping bounds conditioning;
/// rank decisions use the undamped Jacobian QR above, never this matrix.
#[allow(clippy::needless_range_loop)] // Elimination couples pivot and trailing matrix columns.
fn linear_solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for k in 0..n {
        let mut pivot = k;
        for i in k + 1..n {
            if a[i][k].abs() > a[pivot][k].abs() {
                pivot = i
            }
        }
        a.swap(k, pivot);
        b.swap(k, pivot);
        let d = a[k][k];
        if !d.is_normal() {
            return None;
        }
        for i in k + 1..n {
            let factor = a[i][k] / d;
            a[i][k] = 0.;
            for j in k + 1..n {
                a[i][j] -= factor * a[k][j];
            }
            b[i] -= factor * b[k];
        }
    }
    let mut x = vec![0.; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|j| a[i][j] * x[j]).sum::<f64>()) / a[i][i];
        if !x[i].is_finite() {
            return None;
        }
    }
    Some(x)
}
pub fn solve(sketch: &Sketch, tolerance: f64) -> Result<SolveReport, SketchError> {
    solve_bounded(sketch, tolerance, MAX_ITERATIONS)
}
fn solve_bounded(
    sketch: &Sketch,
    tolerance: f64,
    budget: usize,
) -> Result<SolveReport, SketchError> {
    validate(sketch)?;
    if !tolerance.is_finite() || tolerance <= 0. {
        return Err(SketchError::InvalidTolerance);
    }
    let system = System::new(sketch, tolerance);
    let mut x = system.initial.clone();
    let mut eval = system.evaluate(&x);
    let normalized_tolerance =
        (tolerance - system.world_scale * 256. * f64::EPSILON) / system.scale;
    let fail = |x: &[f64], e: &Evaluation, i, msg: &str| {
        system.report(x, e, SolveStatus::NumericalFailure, i, Some(msg.into()))
    };
    if !normalized_tolerance.is_normal()
        || system.world_scale * 256. * f64::EPSILON > tolerance / 16.
    {
        return Ok(fail(
            &x,
            &eval,
            0,
            "world-coordinate arithmetic cannot resolve the requested tolerance",
        ));
    }
    let mut damping = 1e-3_f64;
    for iteration in 0..=budget {
        if eval.singular {
            return Ok(fail(&x,&eval,iteration,"a segment/radius direction or supporting-line precision is unresolved, or an angular residual lies on its wrap cut"));
        }
        if system.max_residual(&eval) + eval.linear_roundoff <= normalized_tolerance
            && system.max_angular_residual(&eval) <= ANGULAR_TOLERANCE_RADIANS * 0.9
        {
            let rank = rank(&eval.jacobian, x.len());
            let status = if rank == x.len() {
                SolveStatus::FullyConstrained
            } else {
                SolveStatus::Underconstrained
            };
            return Ok(system.report(&x,&eval,status,iteration,if status==SolveStatus::Underconstrained{Some("converged equations have a rank-deficient local Jacobian; degrees of freedom are a local linearization".into())}else{None}));
        }
        if iteration == budget {
            return Ok(system.report(
                &x,
                &eval,
                SolveStatus::NotConverged,
                iteration,
                Some(
                    "iteration budget exhausted; this does not establish global inconsistency"
                        .into(),
                ),
            ));
        }
        let n = x.len();
        let mut normal = vec![vec![0.; n]; n];
        let mut rhs = vec![0.; n];
        for (row, residual) in eval.jacobian.iter().zip(&eval.residuals) {
            for i in 0..n {
                rhs[i] -= row[i] * residual;
                for j in 0..n {
                    normal[i][j] += row[i] * row[j];
                }
            }
        }
        for (i, row) in normal.iter_mut().enumerate() {
            row[i] += damping;
        }
        let Some(step) = linear_solve(normal, rhs) else {
            return Ok(fail(
                &x,
                &eval,
                iteration,
                "damped linear solve exceeded numerical range",
            ));
        };
        let next: Vec<_> = x.iter().zip(&step).map(|(x, d)| x + d).collect();
        let coordinate_scale = next.iter().map(|v| v.abs()).fold(0., f64::max);
        if !coordinate_scale.is_finite() || coordinate_scale > 1e6 {
            return Ok(fail(
                &x,
                &eval,
                iteration,
                "candidate coordinates exceeded the bounded normalized solve domain",
            ));
        }
        let world_scale = next
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| {
                [
                    system.origin[0] + p[0] * system.scale,
                    system.origin[1] + p[1] * system.scale,
                ]
            })
            .map(f64::abs)
            .fold(0., f64::max);
        if !world_scale.is_finite()
            || world_scale > 1e12
            || world_scale * 256. * f64::EPSILON > tolerance / 16.
        {
            return Ok(fail(
                &x,
                &eval,
                iteration,
                "candidate world-coordinate arithmetic cannot resolve the requested tolerance",
            ));
        }
        let candidate = system.evaluate(&next);
        // Bound each vector's continuous rotation along the straight trial step.
        // The sum bounds any angular excursion, including a cut crossing and
        // return that endpoint-only residual comparisons would miss. Large
        // rotations and zero-crossing segments are damped, never branch-flipped.
        let crosses_cut = system.equations.iter().enumerate().any(|(i, eq)| {
            if let Equation::Angle(a, b, _, period) = eq {
                let mut excursion = 0.;
                for ends in [a, b] {
                    let u = [x[ends[1]] - x[ends[0]], x[ends[1] + 1] - x[ends[0] + 1]];
                    let v = [
                        next[ends[1]] - next[ends[0]],
                        next[ends[1] + 1] - next[ends[0] + 1],
                    ];
                    let turn = (u[0] * v[1] - u[1] * v[0])
                        .atan2(u[0] * v[0] + u[1] * v[1])
                        .abs();
                    if turn >= std::f64::consts::FRAC_PI_2 {
                        return true;
                    }
                    excursion += turn;
                }
                eval.residuals[i].abs() + excursion >= period / 2. - 1e-8
            } else {
                false
            }
        });
        if !candidate.singular && !crosses_cut && candidate.objective() < eval.objective() {
            x = next;
            eval = candidate;
            damping = (damping * 0.3).max(1e-12);
        } else {
            damping *= 10.;
        }
        if damping > 1e16 {
            return Ok(system.report(
                &x,
                &eval,
                SolveStatus::NotConverged,
                iteration + 1,
                Some(
                    "local iteration stagnated; this does not establish global inconsistency"
                        .into(),
                ),
            ));
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(id: &str, x: f64, y: f64) -> SketchPoint {
        SketchPoint {
            id: id.into(),
            position: [x, y],
        }
    }
    fn c(id: &str, kind: ConstraintKind) -> Constraint {
        Constraint {
            id: id.into(),
            kind,
        }
    }
    fn fixed(id: &str, point: &str, x: f64, y: f64) -> Constraint {
        c(
            id,
            ConstraintKind::Fixed {
                point: point.into(),
                position: [x, y],
            },
        )
    }
    fn distance(id: &str, a: &str, b: &str, value: f64) -> Constraint {
        c(
            id,
            ConstraintKind::Distance {
                a: a.into(),
                b: b.into(),
                value,
            },
        )
    }
    fn triangle(scale: f64, offset: f64, sign: f64) -> Sketch {
        Sketch {
            points: vec![
                p("a", offset, offset),
                p("b", offset + 4. * scale, offset),
                p("c", offset + 2. * scale, offset + sign * 2. * scale),
            ],
            constraints: vec![
                fixed("a", "a", offset, offset),
                fixed("b", "b", offset + 4. * scale, offset),
                distance("ac", "a", "c", 5. * scale),
                distance("bc", "b", "c", 3. * scale),
            ],
        }
    }
    #[test]
    fn independent_trilateration_branches_scales_and_permutation() {
        for (scale, offset, tol) in [(1., 0., 1e-8), (1e-6, 0., 1e-13), (1e3, 1e5, 1e-5)] {
            for sign in [-1., 1.] {
                let s = triangle(scale, offset, sign);
                let r = solve(&s, tol).unwrap();
                assert!(r.is_fully_constrained(), "{r:?}");
                assert_eq!(
                    (r.rank, r.degrees_of_freedom, r.redundant_equations),
                    (6, 0, 0)
                );
                let actual = r.points["c"];
                assert!((actual[0] - offset - 4. * scale).abs() < tol * 4.);
                assert!((actual[1] - offset - sign * 3. * scale).abs() < tol * 4.);
                let mut reordered = s.clone();
                reordered.points.reverse();
                reordered.constraints.reverse();
                assert_eq!(r, solve(&reordered, tol).unwrap());
                assert_eq!(r, solve(&s, tol).unwrap());
            }
        }
    }
    #[test]
    fn three_side_fixture_matches_closed_form_across_27_designs() {
        for span in [70_f64, 80., 100.] {
            for left in [55_f64, 60., 80.] {
                for right in [60_f64, 70., 85.] {
                    let s = Sketch {
                        points: vec![
                            p("a", 0., 0.),
                            p("b", 0.9 * span, 2.),
                            p("c", 0.35 * span, 30.),
                        ],
                        constraints: vec![
                            fixed("anchor", "a", 0., 0.),
                            c(
                                "horizontal",
                                ConstraintKind::Horizontal {
                                    a: "a".into(),
                                    b: "b".into(),
                                },
                            ),
                            distance("span", "a", "b", span),
                            distance("left", "a", "c", left),
                            distance("right", "b", "c", right),
                        ],
                    };
                    let r = solve(&s, 1e-7).unwrap();
                    assert!(r.is_fully_constrained(), "{span}/{left}/{right}: {r:?}");
                    let x = (span * span + left * left - right * right) / (2. * span);
                    let y = (left * left - x * x).sqrt();
                    assert!((r.points["c"][0] - x).abs() < 1e-6);
                    assert!((r.points["c"][1] - y).abs() < 1e-6);
                    assert_eq!(r.tolerance_mm, 1e-7);
                    assert!(r.max_residual_mm <= r.tolerance_mm);
                    let a = r.points["a"];
                    let b = r.points["b"];
                    let actual = (a[0] - b[0]).hypot(a[1] - b[1]) - span;
                    let reported = r
                        .constraint_residuals
                        .iter()
                        .find(|c| c.id == "span")
                        .unwrap()
                        .residual_mm
                        .unwrap();
                    assert!((reported - actual.abs()).abs() < 1e-12);
                }
            }
        }
    }
    #[test]
    fn analytic_jacobian_and_qr_rank_have_independent_checks() {
        let s = triangle(1., 0., 1.);
        let system = System::new(&s, 1e-8);
        let e = system.evaluate(&system.initial);
        let h = 1e-6;
        for column in 0..system.initial.len() {
            let mut a = system.initial.clone();
            let mut b = a.clone();
            a[column] += h;
            b[column] -= h;
            let ea = system.evaluate(&a);
            let eb = system.evaluate(&b);
            for row in 0..e.residuals.len() {
                let difference = (ea.residuals[row] - eb.residuals[row]) / (2. * h);
                assert!((difference - e.jacobian[row][column]).abs() < 1e-9);
            }
        }
        assert_eq!(
            rank(&[vec![1., 2., 3.], vec![2., 4., 6.], vec![0., 1., 1.]], 3),
            2
        );
        assert_eq!(rank(&[vec![1., 0.], vec![1., 1e-15]], 2), 1);
        assert_eq!(rank(&[vec![1., 0.], vec![1., 1e-6]], 2), 2);
    }
    #[test]
    fn maximum_problem_budget_is_bounded_and_reports_redundancy() {
        let mut s = Sketch {
            points: (0..MAX_POINTS)
                .map(|i| p(&format!("p{i}"), i as f64 + 0.2, 3.))
                .collect(),
            constraints: vec![],
        };
        for i in 0..MAX_CONSTRAINTS {
            let index = i % MAX_POINTS;
            s.constraints.push(fixed(
                &format!("fixed{i}"),
                &format!("p{index}"),
                index as f64,
                0.,
            ));
        }
        let r = solve(&s, 1e-7).unwrap();
        assert!(r.is_fully_constrained());
        assert_eq!(r.rank, 64);
        assert_eq!(r.redundant_equations, 192);
        assert!(r.iterations <= MAX_ITERATIONS);
        s.constraints.push(fixed("overflow", "p0", 0., 0.));
        assert!(matches!(validate(&s), Err(SketchError::ResourceLimit)));
    }

    #[test]
    fn independent_rectangle_and_redundant_constraints() {
        let mut s = Sketch {
            points: vec![
                p("a", 0., 0.),
                p("b", 9., 1.),
                p("c", 10., 7.),
                p("d", 1., 6.),
            ],
            constraints: vec![
                fixed("anchor", "a", 0., 0.),
                c(
                    "bottom",
                    ConstraintKind::Horizontal {
                        a: "a".into(),
                        b: "b".into(),
                    },
                ),
                c(
                    "right",
                    ConstraintKind::Vertical {
                        a: "b".into(),
                        b: "c".into(),
                    },
                ),
                c(
                    "top",
                    ConstraintKind::Horizontal {
                        a: "c".into(),
                        b: "d".into(),
                    },
                ),
                c(
                    "left",
                    ConstraintKind::Vertical {
                        a: "d".into(),
                        b: "a".into(),
                    },
                ),
                distance("width", "a", "b", 12.),
                distance("height", "b", "c", 8.),
            ],
        };
        let r = solve(&s, 1e-8).unwrap();
        assert!(r.is_fully_constrained(), "{r:?}");
        for (id, expected) in [
            ("a", [0., 0.]),
            ("b", [12., 0.]),
            ("c", [12., 8.]),
            ("d", [0., 8.]),
        ] {
            for (a, b) in r.points[id].into_iter().zip(expected) {
                assert!((a - b).abs() < 1e-7)
            }
        }
        s.constraints
            .push(distance("duplicate_width", "a", "b", 12.));
        let r = solve(&s, 1e-8).unwrap();
        assert!(r.is_fully_constrained());
        assert_eq!(r.redundant_equations, 1);
    }
    #[test]
    fn gauges_conflicts_and_singular_starts_report_without_geometry() {
        let free = Sketch {
            points: vec![p("a", 0., 0.), p("b", 3., 4.)],
            constraints: vec![distance("length", "a", "b", 5.)],
        };
        let r = solve(&free, 1e-8).unwrap();
        assert_eq!(r.status, SolveStatus::Underconstrained);
        assert_eq!(r.degrees_of_freedom, 3);
        let conflict = Sketch {
            points: vec![p("a", 0., 0.)],
            constraints: vec![fixed("first", "a", 0., 0.), fixed("second", "a", 2., 0.)],
        };
        let r = solve(&conflict, 1e-8).unwrap();
        assert_eq!(r.status, SolveStatus::NotConverged);
        assert!((r.max_residual_mm - 1.).abs() < 1e-7);
        assert!(r
            .constraint_residuals
            .iter()
            .all(|r| r.residual_mm.unwrap().is_finite()));
        let mut singular = free;
        singular.points[1].position = [0., 0.];
        assert_eq!(
            solve(&singular, 1e-8).unwrap().status,
            SolveStatus::NumericalFailure
        );
        let mut near_singular = triangle(1., 0., 1.);
        near_singular.points[2].position = [2., 0.];
        let r = solve(&near_singular, 1e-8).unwrap();
        assert_eq!(r.status, SolveStatus::NotConverged);
        assert!(r.rank < 6);
    }
    #[test]
    fn bounded_iterations_and_unresolved_world_coordinates() {
        let s = triangle(1., 0., 1.);
        let r = solve_bounded(&s, 1e-8, 0).unwrap();
        assert_eq!(r.status, SolveStatus::NotConverged);
        assert_eq!(r.iterations, 0);
        let s = triangle(1., 1e12, 1.);
        let r = solve(&s, 1e-8);
        assert!(r.is_err());
        let s = triangle(1., 1e9, 1.);
        assert_eq!(
            solve(&s, 1e-8).unwrap().status,
            SolveStatus::NumericalFailure
        );
    }
    #[test]
    fn malformed_input_and_fractional_json_are_strict() {
        let c: Constraint =
            serde_json::from_str(r#"{"id":"d","kind":"distance","a":"p","b":"q","value":1.125}"#)
                .unwrap();
        assert!(matches!(
            c.kind,
            ConstraintKind::Distance { value: 1.125, .. }
        ));
        assert_eq!(
            c,
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap()
        );
        for source in [
            r#"{"id":"d","kind":"distance","a":"p","b":"q","value":1,"value":2}"#,
            r#"{"id":"d","kind":"horizontal","a":"p","b":"q","value":1}"#,
            r#"{"id":"d","kind":"fixed","point":"p","position":[0,0],"a":"q"}"#,
            r#"{"id":"d","kind":"distance","a":"p","b":"q","value":1,"unexpected":0}"#,
        ] {
            assert!(serde_json::from_str::<Constraint>(source).is_err())
        }
        let mut s = triangle(1., 0., 1.);
        s.points[0].id = "b".into();
        assert!(validate(&s).is_err());
        s = triangle(1., 0., 1.);
        s.constraints.push(distance("missing", "x", "c", 1.));
        assert!(validate(&s).is_err());
        s = triangle(1., 0., 1.);
        s.points[0].position[0] = f64::NAN;
        assert!(validate(&s).is_err());
        assert!(solve(&triangle(1., 0., 1.), 0.).is_err());
    }
    #[test]
    fn rank_full_but_unstable_solution_is_not_certified() {
        let mut s = Sketch {
            points: vec![p("a", 0., 0.), p("b", 2., 0.), p("c", 1., 1e-9)],
            constraints: vec![
                fixed("a", "a", 0., 0.),
                fixed("b", "b", 2., 0.),
                distance("ac", "a", "c", 1.),
                distance("bc", "b", "c", 1.),
            ],
        };
        let r = solve(&s, 1e-8).unwrap();
        assert_eq!(r.rank, 6);
        assert_eq!(r.status, SolveStatus::NumericalFailure);
        assert!(r.condition_bound.unwrap() > MAX_CONDITION_BOUND);
        assert!(serde_json::to_string(&r).is_ok());
        s.points[2].position[1] = 0.;
        let r = solve(&s, 1e-8).unwrap();
        assert_eq!(r.status, SolveStatus::Underconstrained);
        assert_eq!(r.degrees_of_freedom, 1);
        assert!(r.message.unwrap().contains("local"));
    }
    #[test]
    fn positional_constraint_error_is_euclidean_and_matches_returned_points() {
        let s = Sketch {
            points: vec![p("a", 0.75e-6, 0.75e-6)],
            constraints: vec![fixed("fixed", "a", 0., 0.)],
        };
        let initial = solve_bounded(&s, 1e-6, 0).unwrap();
        assert_eq!(initial.status, SolveStatus::NotConverged);
        assert!(initial.max_residual_mm > 1e-6);
        let r = solve(&s, 1e-6).unwrap();
        assert!(r.is_fully_constrained());
        let p = r.points["a"];
        assert!((r.constraint_residuals[0].residual_mm.unwrap() - p[0].hypot(p[1])).abs() < 1e-20);
    }

    #[test]
    fn coincident_and_fixed_constraints_preserve_finite_zero_iteration_report() {
        let s = Sketch {
            points: vec![p("a", 1., 2.), p("b", 1., 2.)],
            constraints: vec![
                fixed("anchor", "a", 1., 2.),
                c(
                    "coincident",
                    ConstraintKind::Coincident {
                        a: "a".into(),
                        b: "b".into(),
                    },
                ),
            ],
        };
        let r = solve(&s, 1e-8).unwrap();
        assert!(r.is_fully_constrained());
        assert_eq!(r.iterations, 0);
        assert_eq!(r.max_residual_mm, 0.);
        assert!(!r.solver_build_id.is_empty());
    }
}

#[cfg(test)]
mod angular_tests;

#[cfg(test)]
mod tangent_tests;
