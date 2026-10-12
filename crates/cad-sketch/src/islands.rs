//! Deterministic graph partitioning precedes every numerical solve.
use super::*;
fn references(kind: &ConstraintKind) -> Vec<&str> {
    match kind {
        ConstraintKind::Fixed { point, .. } => vec![point],
        ConstraintKind::Coincident { a, b }
        | ConstraintKind::Horizontal { a, b }
        | ConstraintKind::Vertical { a, b }
        | ConstraintKind::Distance { a, b, .. } => vec![a, b],
        ConstraintKind::EqualLength { a, b }
        | ConstraintKind::Parallel { a, b }
        | ConstraintKind::Perpendicular { a, b }
        | ConstraintKind::Angle { a, b, .. } => vec![&a[0], &a[1], &b[0], &b[1]],
        ConstraintKind::CircleLineTangent { circle, line, .. } => {
            vec![&circle[0], &circle[1], &line[0], &line[1]]
        }
    }
}
/// Caller validates identifiers/references first. Roots are the least point ID.
pub(super) fn partition(sketch: &Sketch) -> Result<Vec<Sketch>, SketchError> {
    let points: BTreeMap<_, _> = sketch.points.iter().map(|p| (p.id.as_str(), p)).collect();
    let indices: BTreeMap<_, _> = points.keys().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut parents: Vec<_> = (0..points.len()).collect();
    fn root(parents: &[usize], mut i: usize) -> usize {
        while parents[i] != i {
            i = parents[i];
        }
        i
    }
    for constraint in &sketch.constraints {
        let refs = references(&constraint.kind);
        let first = indices[refs[0]];
        for id in &refs[1..] {
            let a = root(&parents, first);
            let b = root(&parents, indices[id]);
            parents[a.max(b)] = a.min(b);
        }
    }
    let mut groups: BTreeMap<usize, Sketch> = BTreeMap::new();
    for (id, p) in points {
        groups
            .entry(root(&parents, indices[id]))
            .or_insert_with(|| Sketch {
                points: vec![],
                constraints: vec![],
            })
            .points
            .push(p.clone());
    }
    for c in &sketch.constraints {
        groups
            .get_mut(&root(&parents, indices[references(&c.kind)[0]]))
            .unwrap()
            .constraints
            .push(c.clone());
    }
    for group in groups.values_mut() {
        group.constraints.sort_by(|a, b| a.id.cmp(&b.id));
        if group.points.len() > MAX_ISLAND_POINTS
            || group.constraints.len() > MAX_ISLAND_CONSTRAINTS
        {
            return Err(SketchError::IslandResourceLimit {
                island: group.points[0].id.clone(),
                points: group.points.len(),
                constraints: group.constraints.len(),
            });
        }
    }
    Ok(groups.into_values().collect())
}
fn severity(status: SolveStatus) -> usize {
    match status {
        SolveStatus::FullyConstrained => 0,
        SolveStatus::Underconstrained => 1,
        SolveStatus::NotConverged => 2,
        SolveStatus::NumericalFailure => 3,
    }
}
pub(super) fn solve(
    sketch: &Sketch,
    tolerance: f64,
    refine: bool,
) -> Result<SolveReport, SketchError> {
    validate(sketch)?;
    if !tolerance.is_finite() || tolerance <= 0. {
        return Err(SketchError::InvalidTolerance);
    }
    let groups = partition(sketch)?;
    let mut combined = SolveReport {
        schema_version: 3,
        islands: vec![],
        solver_build_id: SOLVER_BUILD_ID.into(),
        status: SolveStatus::FullyConstrained,
        tolerance_mm: tolerance,
        angular_tolerance_radians: ANGULAR_TOLERANCE_RADIANS,
        points: BTreeMap::new(),
        iterations: 0,
        max_residual_mm: 0.,
        max_angular_residual_radians: 0.,
        rank: 0,
        condition_bound: Some(0.),
        degrees_of_freedom: 0,
        redundant_equations: 0,
        constraint_residuals: vec![],
        message: None,
    };
    for group in groups {
        let mut best = None;
        let local = solve_policy(&group, tolerance, MAX_ITERATIONS, refine, &mut best)?;
        let local = best.unwrap_or(local);
        combined.islands.push(IslandReport {
            id: group.points[0].id.clone(),
            point_ids: group.points.iter().map(|p| p.id.clone()).collect(),
            constraint_ids: group.constraints.iter().map(|c| c.id.clone()).collect(),
            status: local.status,
            iterations: local.iterations,
            rank: local.rank,
            degrees_of_freedom: local.degrees_of_freedom,
            redundant_equations: local.redundant_equations,
            condition_bound: local.condition_bound,
            max_residual_mm: local.max_residual_mm,
            max_angular_residual_radians: local.max_angular_residual_radians,
            message: local.message.clone(),
        });
        if severity(local.status) > severity(combined.status) {
            combined.status = local.status;
        }
        combined.points.extend(local.points);
        combined
            .constraint_residuals
            .extend(local.constraint_residuals);
        combined.iterations += local.iterations;
        combined.rank += local.rank;
        combined.degrees_of_freedom += local.degrees_of_freedom;
        combined.redundant_equations += local.redundant_equations;
        combined.max_residual_mm = combined.max_residual_mm.max(local.max_residual_mm);
        combined.max_angular_residual_radians = combined
            .max_angular_residual_radians
            .max(local.max_angular_residual_radians);
        combined.condition_bound = combined
            .condition_bound
            .zip(local.condition_bound)
            .map(|(a, b)| a.max(b));
    }
    combined
        .constraint_residuals
        .sort_by(|a, b| a.id.cmp(&b.id));
    combined.message = if combined.islands.len() == 1 {
        combined.islands[0].message.clone()
    } else if combined.status != SolveStatus::FullyConstrained {
        Some("one or more independent constraint islands are unresolved; inspect per-island evidence".into())
    } else {
        None
    };
    Ok(combined)
}
