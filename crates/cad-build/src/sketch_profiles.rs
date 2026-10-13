//! Solver profiles are evaluated before solid cache lookup, never from meshes.
use super::*;
const MAX_SOLVED_SKETCHES: usize = 128;
fn invalid(id: &str, message: impl Into<String>) -> BuildError {
    BuildError::Sketch {
        feature: id.into(),
        message: message.into(),
    }
}
pub(super) fn validate(
    id: &str,
    sketch: &Sketch,
    loops: &[Vec<String>],
    curve_outline: Option<&SketchPath>,
    circular_holes: &[SketchCircleHole],
) -> Result<(), BuildError> {
    cad_sketch::validate(sketch).map_err(|e| invalid(id, e.to_string()))?;
    if curve_outline.is_none()
        && (loops.is_empty()
            || loops.len().saturating_add(circular_holes.len()) > MAX_POLYGON_HOLES + 1
            || loops.iter().map(Vec::len).sum::<usize>() > MAX_POLYGON_POINTS)
    {
        return Err(invalid(
            id,
            "sketch profile requires 1..33 rings and at most 256 vertices",
        ));
    }
    let ids: BTreeSet<_> = sketch.points.iter().map(|p| p.id.as_str()).collect();
    if let Some(path) = curve_outline {
        if !loops.is_empty() {
            return Err(invalid(
                id,
                "curve_outline cannot be combined with polygon loops or holes",
            ));
        }
        if path.segments.is_empty()
            || path.segments.len() > MAX_POLYGON_POINTS
            || circular_holes.len() > MAX_POLYGON_HOLES
        {
            return Err(invalid(
                id,
                "curved sketch requires 1..256 segments and at most 32 circular holes",
            ));
        }
        let mut referenced = vec![&path.start];
        let mut last = &path.start;
        for segment in &path.segments {
            match segment {
                SketchPathSegment::LineTo { end } => {
                    referenced.push(end);
                    last = end;
                }
                SketchPathSegment::ArcTo { center, end, .. } => {
                    referenced.extend([center, end]);
                    last = end;
                }
            }
        }
        if last != &path.start {
            return Err(invalid(
                id,
                "curve_outline must close with the start point ID",
            ));
        }
        for point in referenced {
            if !ids.contains(point.as_str()) {
                return Err(invalid(id, format!("unknown curve point '{point}'")));
            }
        }
    }
    for hole in circular_holes {
        if hole.center == hole.rim {
            return Err(invalid(
                id,
                "circle center and rim require distinct point IDs",
            ));
        }
        for point in [&hole.center, &hole.rim] {
            if !ids.contains(point.as_str()) {
                return Err(invalid(
                    id,
                    format!("unknown circular-hole point '{point}'"),
                ));
            }
        }
    }
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
        .filter(|f| {
            wanted.contains(f.id())
                && matches!(f, Feature::Sketch { .. } | Feature::HolePattern { .. })
        })
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
pub(super) enum SolvedProfile {
    Polygon {
        rings: Rings,
        circles: Vec<CircleHole>,
    },
    Path {
        path: PlanarPath,
        circles: Vec<CircleHole>,
    },
}
pub(super) fn solve(
    id: &str,
    sketch: &Sketch,
    loops: &[Vec<String>],
    curve_outline: Option<&SketchPath>,
    circular_holes: &[SketchCircleHole],
    tolerance: Tolerance,
    content_hash: &str,
) -> Result<(SolveReport, SolvedProfile), BuildError> {
    let solve = if curve_outline.is_some() {
        cad_sketch::solve_refined
    } else {
        cad_sketch::solve
    };
    let report = solve(sketch, tolerance.linear).map_err(|e| invalid(id, e.to_string()))?;
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
    let circles: Vec<_> = circular_holes
        .iter()
        .map(|hole| {
            let center = report.points[&hole.center];
            let rim = report.points[&hole.rim];
            CircleHole {
                center,
                radius: (rim[0] - center[0]).hypot(rim[1] - center[1]),
            }
        })
        .collect();
    if let Some(outline) = curve_outline {
        let path = PlanarPath {
            start: report.points[&outline.start],
            segments: outline
                .segments
                .iter()
                .map(|segment| match segment {
                    SketchPathSegment::LineTo { end } => PathSegment::LineTo {
                        end: report.points[end],
                    },
                    SketchPathSegment::ArcTo {
                        center,
                        end,
                        clockwise,
                    } => PathSegment::ArcTo {
                        center: report.points[center],
                        end: report.points[end],
                        clockwise: *clockwise,
                    },
                })
                .collect(),
        };
        cad_kernel::validate_path(&path, &circles, tolerance).map_err(|e| {
            BuildError::PolygonProfile {
                feature: id.into(),
                message: e.to_string(),
            }
        })?;
        return Ok((report, SolvedProfile::Path { path, circles }));
    }
    cad_kernel::validate_polygon_with_circular_holes(&rings[0], &rings[1..], &circles, tolerance)
        .map_err(|e| BuildError::PolygonProfile {
        feature: id.into(),
        message: e.to_string(),
    })?;
    Ok((report, SolvedProfile::Polygon { rings, circles }))
}

// The feature discriminator may follow circular_holes in JSON. Decode typed
// fields directly: serde Content buffering loses f64 with arbitrary_precision.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputHole {
    center: HoleCenter,
    #[serde(default)]
    radius: OptionalField<f64>,
    #[serde(default)]
    rim: OptionalField<String>,
}
struct OptionalField<T>(Option<T>);
impl<T> Default for OptionalField<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for OptionalField<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(|v| Self(Some(v)))
    }
}
enum HoleCenter {
    Point(String),
    Coordinates([f64; 2]),
}
impl<'de> Deserialize<'de> for HoleCenter {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = HoleCenter;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a point ID or a two-coordinate center")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(HoleCenter::Point(v.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                use serde::de::Error;
                let x = a
                    .next_element::<f64>()?
                    .ok_or_else(|| A::Error::custom("missing center x"))?;
                let y = a
                    .next_element::<f64>()?
                    .ok_or_else(|| A::Error::custom("missing center y"))?;
                if a.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(A::Error::custom("center requires two coordinates"));
                }
                Ok(HoleCenter::Coordinates([x, y]))
            }
        }
        d.deserialize_any(V)
    }
}
impl InputHole {
    pub(super) fn into_sketch<E: serde::de::Error>(self) -> Result<SketchCircleHole, E> {
        match (self.center, self.rim.0, self.radius.0) {
            (HoleCenter::Point(center), Some(rim), None) => Ok(SketchCircleHole { center, rim }),
            _ => Err(E::custom(
                "sketch circular hole requires center and rim point IDs",
            )),
        }
    }
    pub(super) fn into_circle<E: serde::de::Error>(self) -> Result<CircleHole, E> {
        match (self.center, self.rim.0, self.radius.0) {
            (HoleCenter::Coordinates(center), None, Some(radius)) => {
                Ok(CircleHole { center, radius })
            }
            _ => Err(E::custom(
                "native circular hole requires center coordinates and radius",
            )),
        }
    }
}
