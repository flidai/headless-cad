//! Derived mesh patches for the analytic circular-extrusion domain.
use crate::math::cross;
use crate::{EdgeId, FaceId, KernelError, ModelSnapshot, Tolerance};

#[derive(Debug, Clone, Copy)]
pub struct TessellationPolicy {
    /// Maximum radial chord deviation in millimetres, including roundoff allowance.
    pub chordal_error: f64,
    pub max_segments: usize,
}
impl Default for TessellationPolicy {
    fn default() -> Self {
        Self {
            chordal_error: 0.01,
            max_segments: 4096,
        }
    }
}
#[derive(Debug, Clone)]
pub struct MeshPatch {
    pub face: FaceId,
    pub points: Vec<[f64; 3]>,
    pub normal: [f64; 3],
    pub boundary_edges: Vec<Option<EdgeId>>,
}
#[derive(Debug, Clone)]
pub struct CylinderMesh {
    pub patches: Vec<MeshPatch>,
    pub segments: usize,
    pub chordal_error_bound: f64,
}

pub fn tessellate_cylinder(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    let c = model.cylinder_geometry(tolerance)?.ok_or_else(|| {
        KernelError::Unsupported("circular extrusion required for cylinder tessellation".into())
    })?;
    if !policy.chordal_error.is_finite()
        || policy.chordal_error <= 0.
        || !(12..=65536).contains(&policy.max_segments)
    {
        return Err(KernelError::Unsupported(
            "tessellation requires positive finite chordal error and 12..65536 maximum segments"
                .into(),
        ));
    }
    let scale = c
        .origin
        .iter()
        .fold(c.radius.max(c.height), |s, v| s.max(v.abs()));
    let roundoff = scale * 256. * f64::EPSILON;
    let available = policy.chordal_error - roundoff;
    if !available.is_normal() || available <= 0. {
        return Err(KernelError::NumericRange);
    }
    let half_angle = (available / c.radius / 2.).min(1.).sqrt().asin();
    let count = (std::f64::consts::PI / (2. * half_angle)).ceil().max(12.);
    if !count.is_finite() || count > policy.max_segments as f64 {
        return Err(KernelError::ResourceLimit(
            "chordal error requires more cylinder segments than permitted".into(),
        ));
    }
    let mut n = count as usize;
    let error = loop {
        let bound =
            2. * c.radius * (std::f64::consts::PI / (2. * n as f64)).sin().powi(2) + roundoff;
        if bound <= policy.chordal_error {
            break bound;
        }
        n += 1;
        if n > policy.max_segments {
            return Err(KernelError::ResourceLimit(
                "cylinder tessellation error budget exceeded".into(),
            ));
        }
    };
    let y = cross(c.axis, c.x_direction);
    let mut bottom = Vec::with_capacity(n);
    let mut top = Vec::with_capacity(n);
    for i in 0..n {
        let angle = std::f64::consts::TAU * i as f64 / n as f64;
        let (s, co) = angle.sin_cos();
        let p =
            std::array::from_fn(|a| c.origin[a] + c.radius * (co * c.x_direction[a] + s * y[a]));
        bottom.push(p);
        top.push(std::array::from_fn(|a| p[a] + c.height * c.axis[a]));
    }
    let edge = |face: FaceId| model.loops[model.faces[face.0].loops[0].0].edges[0].edge;
    let lower = edge(c.start_cap);
    let upper = edge(c.end_cap);
    let mut patches = Vec::with_capacity(3 * n);
    let top_center = std::array::from_fn(|a| c.origin[a] + c.height * c.axis[a]);
    for i in 0..n {
        let j = (i + 1) % n;
        patches.push(MeshPatch {
            face: c.start_cap,
            points: vec![c.origin, bottom[j], bottom[i]],
            normal: c.axis.map(|v| -v),
            boundary_edges: vec![None, Some(lower), None],
        });
        patches.push(MeshPatch {
            face: c.end_cap,
            points: vec![top_center, top[i], top[j]],
            normal: c.axis,
            boundary_edges: vec![None, Some(upper), None],
        });
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let angle = std::f64::consts::TAU * (i as f64 + 0.5) / n as f64;
        let (s, co) = angle.sin_cos();
        patches.push(MeshPatch {
            face: c.wall,
            points: vec![bottom[i], bottom[j], top[j], top[i]],
            normal: std::array::from_fn(|a| co * c.x_direction[a] + s * y[a]),
            boundary_edges: vec![Some(lower), None, Some(upper), None],
        });
    }
    if patches
        .iter()
        .flat_map(|p| p.points.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(KernelError::NumericRange);
    }
    Ok(CylinderMesh {
        patches,
        segments: n,
        chordal_error_bound: error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mesh_error_orientation_and_mass_converge_independently() {
        let t = Tolerance::default();
        let m = crate::extrude_circle(8., 60., t).unwrap().model;
        let mut previous_error = f64::INFINITY;
        for error in [0.1, 0.01, 0.001] {
            let mesh = tessellate_cylinder(
                &m,
                t,
                TessellationPolicy {
                    chordal_error: error,
                    max_segments: 4096,
                },
            )
            .unwrap();
            assert!(mesh.chordal_error_bound <= error);
            let mut volume = 0.;
            for patch in &mesh.patches {
                for i in 1..patch.points.len() - 1 {
                    let [a, b, c] = [patch.points[0], patch.points[i], patch.points[i + 1]];
                    let normal = cross(crate::math::sub(b, a), crate::math::sub(c, a));
                    assert!(crate::math::dot(normal, patch.normal) > 0.);
                    volume += crate::math::dot(a, cross(b, c)) / 6.;
                }
            }
            let expected_polygon_volume = 0.5
                * mesh.segments as f64
                * 64.
                * (std::f64::consts::TAU / mesh.segments as f64).sin()
                * 60.;
            assert!((volume - expected_polygon_volume).abs() < 1e-8);
            let missing = std::f64::consts::PI * 64. * 60. - volume;
            assert!(missing > 0. && missing < previous_error);
            previous_error = missing;
            for patch in mesh.patches.iter().filter(|p| p.face == FaceId(2)) {
                let midpoint = std::array::from_fn::<_, 3, _>(|a| {
                    (patch.points[0][a] + patch.points[1][a]) / 2.
                });
                let deviation = 8. - midpoint[0].hypot(midpoint[1]);
                assert!(deviation <= mesh.chordal_error_bound);
                assert_eq!(patch.face, FaceId(2));
            }
        }
        assert!(tessellate_cylinder(
            &m,
            t,
            TessellationPolicy {
                chordal_error: 1e-12,
                max_segments: 12
            }
        )
        .is_err());
    }
}
