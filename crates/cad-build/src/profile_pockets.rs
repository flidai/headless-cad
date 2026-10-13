//! Source profile/frame admission and canonical conversion before cache lookup.
use super::*;

pub(super) fn validate(feature: &Feature, schema: u32) -> Result<(), BuildError> {
    let Feature::BlindProfile {
        id,
        face,
        depth,
        frame_origin,
        frame_x_direction: x,
        frame_y_direction: y,
        ..
    } = feature
    else {
        unreachable!()
    };
    let fail = |message: &str| BuildError::Kernel {
        feature: id.clone(),
        message: message.into(),
    };
    if schema != 2 {
        return Err(fail("blind_profile requires recipe schema version 2"));
    }
    dimension(id, "profile pocket depth", *depth)?;
    if face.from_feature.is_empty()
        || face.role.is_empty()
        || face.from_feature.trim() != face.from_feature
        || face.role.trim() != face.role
    {
        return Err(fail(
            "profile pocket requires an exact semantic cap source and role",
        ));
    }
    let dot = |a: &[f64; 3], b: &[f64; 3]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    if frame_origin
        .iter()
        .chain(x)
        .chain(y)
        .any(|v| !v.is_finite())
        || (dot(x, x).sqrt() - 1.).abs() > 128. * f64::EPSILON
        || (dot(y, y).sqrt() - 1.).abs() > 128. * f64::EPSILON
        || dot(x, y).abs() > 128. * f64::EPSILON
    {
        return Err(fail(
            "profile pocket requires a finite orthonormal source frame",
        ));
    }
    Ok(())
}

fn polygon(points: &[[f64; 2]]) -> Option<PlanarPath> {
    let start = *points.first()?;
    Some(PlanarPath {
        start,
        segments: points
            .iter()
            .skip(1)
            .copied()
            .chain([start])
            .map(|end| PathSegment::LineTo { end })
            .collect(),
    })
}

pub(super) fn resolve(
    feature: &Feature,
    input: &Feature,
    solved: &BTreeMap<String, sketch_profiles::SolvedProfile>,
    body: &cad_kernel::ModelSnapshot,
    cap: cad_kernel::FaceId,
    tolerance: cad_kernel::Tolerance,
) -> Result<PlanarPath, BuildError> {
    let Feature::BlindProfile {
        id,
        frame_origin: origin,
        frame_x_direction: x,
        frame_y_direction: y,
        ..
    } = feature
    else {
        unreachable!()
    };
    let fail = |message: String| BuildError::Kernel {
        feature: id.clone(),
        message,
    };
    let profile = match input {
        Feature::Path {
            path,
            circular_holes,
            ..
        } if circular_holes.is_empty() => Some(path.clone()),
        Feature::Rectangle { width, depth, .. } => {
            polygon(&[[0., 0.], [*width, 0.], [*width, *depth], [0., *depth]])
        }
        Feature::Polygon {
            points,
            holes,
            circular_holes,
            ..
        } if holes.is_empty() && circular_holes.is_empty() => polygon(points),
        Feature::Sketch { id, .. } => match solved.get(id) {
            Some(sketch_profiles::SolvedProfile::Path { path, circles }) if circles.is_empty() => {
                Some(path.clone())
            }
            Some(sketch_profiles::SolvedProfile::Polygon { rings, circles })
                if rings.len() == 1 && circles.is_empty() =>
            {
                polygon(&rings[0])
            }
            _ => None,
        },
        _ => None,
    }
    .ok_or_else(|| {
        fail("profile pocket requires one hole-free line/arc, polygon or rectangle profile".into())
    })?;
    if profile.segments.len() < 2 || profile.segments.len() > 32 {
        return Err(fail("profile pocket requires 2..32 segments".into()));
    }
    let stock =
        cad_kernel::blind_profile_frame(body, tolerance).map_err(|e| fail(e.to_string()))?;
    let dot = |a: [f64; 3], b: [f64; 3]| a.into_iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    let delta = std::array::from_fn(|i| origin[i] - stock.origin[i]);
    let mut scale = origin
        .iter()
        .chain(stock.origin.iter())
        .map(|v| v.abs())
        .fold(stock.height.max(1.), f64::max);
    for point in
        std::iter::once(&profile.start).chain(profile.segments.iter().flat_map(|s| match s {
            PathSegment::LineTo { end } => vec![end],
            PathSegment::ArcTo { center, end, .. } => vec![center, end],
        }))
    {
        scale += point.iter().map(|v| v.abs()).sum::<f64>();
    }
    let error = (scale * 4096. * f64::EPSILON).next_up();
    if cap.0 > 1
        || !error.is_finite()
        || error > tolerance.linear / 16.
        || (dot(delta, stock.axis) - if cap.0 == 0 { 0. } else { stock.height }).abs() > error
        || dot(*x, stock.axis).abs() > 128. * f64::EPSILON
        || dot(*y, stock.axis).abs() > 128. * f64::EPSILON
    {
        return Err(fail(
            "profile source frame must lie on the selected native cap with bounded precision"
                .into(),
        ));
    }
    // The authored contour keeps its segment identity and handedness; reflected
    // source frames are outside this first admission.
    let orientation = dot(*x, stock.x_direction) * dot(*y, stock.y_direction)
        - dot(*x, stock.y_direction) * dot(*y, stock.x_direction);
    if orientation <= 0. {
        return Err(fail(
            "profile pocket source axes must preserve stock-frame contour orientation".into(),
        ));
    }
    let convert = |p: [f64; 2]| {
        let world =
            std::array::from_fn(|i| origin[i] + x[i] * p[0] + y[i] * p[1] - stock.origin[i]);
        [dot(world, stock.x_direction), dot(world, stock.y_direction)]
    };
    Ok(PlanarPath {
        start: convert(profile.start),
        segments: profile
            .segments
            .iter()
            .map(|s| match s {
                PathSegment::LineTo { end } => PathSegment::LineTo { end: convert(*end) },
                PathSegment::ArcTo {
                    center,
                    end,
                    clockwise,
                } => PathSegment::ArcTo {
                    center: convert(*center),
                    end: convert(*end),
                    clockwise: *clockwise,
                },
            })
            .collect(),
    })
}
