//! Solver profiles are evaluated before solid cache lookup, never from meshes.
use super::*;
const MAX_SOLVED_SKETCHES: usize = 128;
fn invalid(id: &str, message: impl Into<String>) -> BuildError {
    BuildError::Sketch {
        feature: id.into(),
        message: message.into(),
    }
}
pub(super) fn validate(id: &str, sketch: &Sketch, loops: &[Vec<String>]) -> Result<(), BuildError> {
    cad_sketch::validate(sketch).map_err(|e| invalid(id, e.to_string()))?;
    if loops.is_empty()
        || loops.len() > MAX_POLYGON_HOLES + 1
        || loops.iter().map(Vec::len).sum::<usize>() > MAX_POLYGON_POINTS
    {
        return Err(invalid(
            id,
            "sketch profile requires 1..33 rings and at most 256 vertices",
        ));
    }
    let ids: BTreeSet<_> = sketch.points.iter().map(|p| p.id.as_str()).collect();
    let mut used = BTreeSet::new();
    for ring in loops {
        if ring.len() < 3 {
            return Err(invalid(
                id,
                "sketch profile ring requires at least three points",
            ));
        }
        for point in ring {
            if !ids.contains(point.as_str()) {
                return Err(invalid(id, format!("unknown loop point '{point}'")));
            }
            if !used.insert(point) {
                return Err(invalid(id, format!("repeated loop point '{point}'")));
            }
        }
    }
    Ok(())
}
pub(super) fn budget(recipe: &Recipe, wanted: &BTreeSet<&str>) -> Result<(), BuildError> {
    if recipe
        .features
        .iter()
        .filter(|f| wanted.contains(f.id()) && matches!(f, Feature::Sketch { .. }))
        .count()
        > MAX_SOLVED_SKETCHES
    {
        return Err(BuildError::ResourceLimit {
            resource: "demanded sketches",
            limit: MAX_SOLVED_SKETCHES,
        });
    }
    Ok(())
}
type Rings = Vec<Vec<[f64; 2]>>;
pub(super) fn solve(
    id: &str,
    sketch: &Sketch,
    loops: &[Vec<String>],
    tolerance: Tolerance,
    content_hash: &str,
) -> Result<(SolveReport, Rings), BuildError> {
    let report =
        cad_sketch::solve(sketch, tolerance.linear).map_err(|e| invalid(id, e.to_string()))?;
    if !report.is_fully_constrained() {
        return Err(BuildError::SketchSolveFailed {
            feature: id.into(),
            report: Box::new(report),
            content_hash: content_hash.into(),
        });
    }
    let rings: Rings = loops
        .iter()
        .map(|ring| ring.iter().map(|point| report.points[point]).collect())
        .collect();
    cad_kernel::validate_polygon_with_holes(&rings[0], &rings[1..], tolerance).map_err(|e| {
        BuildError::PolygonProfile {
            feature: id.into(),
            message: e.to_string(),
        }
    })?;
    Ok((report, rings))
}
