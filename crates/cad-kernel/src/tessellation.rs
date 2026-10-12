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
    let (n, error) = circle_resolution(c.radius, c.height, c.origin, policy)?;
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
    let edge = |face: FaceId, loop_index: usize| {
        model.loops[model.faces[face.0].loops[loop_index].0].edges[0].edge
    };
    let lower = edge(c.start_cap, 0);
    let upper = edge(c.end_cap, 0);
    let mut patches = Vec::with_capacity(4 * n);
    let top_center = std::array::from_fn(|a| c.origin[a] + c.height * c.axis[a]);
    let mut inner_bottom = Vec::new();
    let mut inner_top = Vec::new();
    if c.inner_wall.is_some() {
        for i in 0..n {
            let (s, co) = (std::f64::consts::TAU * i as f64 / n as f64).sin_cos();
            let p = std::array::from_fn(|a| {
                c.origin[a] + c.inner_radius * (co * c.x_direction[a] + s * y[a])
            });
            inner_bottom.push(p);
            inner_top.push(std::array::from_fn(|a| p[a] + c.height * c.axis[a]));
        }
    }
    for i in 0..n {
        let j = (i + 1) % n;
        if let Some(inner_wall) = c.inner_wall {
            let inner_lower = edge(c.start_cap, 1);
            let inner_upper = edge(c.end_cap, 1);
            patches.push(MeshPatch {
                face: c.start_cap,
                points: vec![bottom[i], inner_bottom[i], inner_bottom[j], bottom[j]],
                normal: c.axis.map(|v| -v),
                boundary_edges: vec![None, Some(inner_lower), None, Some(lower)],
            });
            patches.push(MeshPatch {
                face: c.end_cap,
                points: vec![top[i], top[j], inner_top[j], inner_top[i]],
                normal: c.axis,
                boundary_edges: vec![Some(upper), None, Some(inner_upper), None],
            });
            let (s, co) = (std::f64::consts::TAU * (i as f64 + 0.5) / n as f64).sin_cos();
            patches.push(MeshPatch {
                face: inner_wall,
                points: vec![inner_bottom[j], inner_bottom[i], inner_top[i], inner_top[j]],
                normal: std::array::from_fn(|a| -(co * c.x_direction[a] + s * y[a])),
                boundary_edges: vec![Some(inner_lower), None, Some(inner_upper), None],
            });
        } else {
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
        let (s, co) = (std::f64::consts::TAU * (i as f64 + 0.5) / n as f64).sin_cos();
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

pub(crate) fn circle_resolution(
    radius: f64,
    height: f64,
    origin: [f64; 3],
    policy: TessellationPolicy,
) -> Result<(usize, f64), KernelError> {
    if !policy.chordal_error.is_finite()
        || policy.chordal_error <= 0.
        || !(12..=65536).contains(&policy.max_segments)
    {
        return Err(KernelError::Unsupported(
            "tessellation requires positive finite chordal error and 12..65536 maximum segments"
                .into(),
        ));
    }
    let scale = origin
        .iter()
        .fold(radius.max(height), |s, v| s.max(v.abs()));
    let roundoff = scale * 256. * f64::EPSILON;
    let available = policy.chordal_error - roundoff;
    if !available.is_normal() || available <= 0. {
        return Err(KernelError::NumericRange);
    }
    let half_angle = (available / radius / 2.).min(1.).sqrt().asin();
    let count = (std::f64::consts::PI / (2. * half_angle)).ceil().max(12.);
    if !count.is_finite() || count > policy.max_segments as f64 {
        return Err(KernelError::ResourceLimit(
            "chordal error requires more cylinder segments than permitted".into(),
        ));
    }
    let mut n = count as usize;
    let error = loop {
        let bound = 2. * radius * (std::f64::consts::PI / (2. * n as f64)).sin().powi(2) + roundoff;
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
    Ok((n, error))
}

/// Tessellate supported analytic bodies without changing native topology.
pub fn tessellate_analytic(
    model: &ModelSnapshot,
    tolerance: Tolerance,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    if let Ok(g) = crate::stepped_bore::recognize(model, tolerance) {
        return crate::stepped_bore::tessellate(&g, tolerance, policy);
    }
    if let Ok(g) = crate::bored_prism::recognize(model, tolerance) {
        return crate::bored_prism::tessellate(&g, tolerance, policy);
    }
    if let Some(g) = model.arc_prism_geometry(tolerance)? {
        return crate::arc_prism::tessellate(model, &g, policy);
    }
    if let Some(geometry) = model.revolve_geometry(tolerance)? {
        return tessellate_revolve(model, &geometry, policy);
    }
    let Some(g) = model.profile_prism_geometry(tolerance)? else {
        return tessellate_cylinder(model, tolerance, policy);
    };
    use crate::math::{dot, sub};
    use std::collections::BTreeMap;
    let mut sampled = BTreeMap::<EdgeId, Vec<[f64; 3]>>::new();
    let mut patches = Vec::new();
    let mut segments = 0;
    let mut bound = 0f64;
    let cap_base_points: usize = g.base.faces[g.start_cap.0]
        .loops
        .iter()
        .map(|id| g.base.loops[id.0].edges.len())
        .sum();
    for (i, bore) in g.bores.iter().enumerate() {
        let lower_loop =
            model.faces[g.start_cap.0].loops[g.base.faces[g.start_cap.0].loops.len() + i];
        let upper_loop = model.faces[g.end_cap.0].loops[g.base.faces[g.end_cap.0].loops.len() + i];
        let lower = model.loops[lower_loop.0].edges[0].edge;
        let upper = model.loops[upper_loop.0].edges[0].edge;
        let (n, error) = circle_resolution(bore.radius, bore.height, bore.origin, policy)?;
        segments += n;
        bound = bound.max(error);
        if segments + cap_base_points > 2048 {
            return Err(KernelError::ResourceLimit(
                "derived cap exceeds 2048 sampled boundary points".into(),
            ));
        }
        let y = cross(bore.axis, bore.x_direction);
        let bottom: Vec<[f64; 3]> = (0..n)
            .map(|k| {
                let (s, c) = (std::f64::consts::TAU * k as f64 / n as f64).sin_cos();
                std::array::from_fn(|a| {
                    bore.origin[a] + bore.radius * (c * bore.x_direction[a] + s * y[a])
                })
            })
            .collect();
        let top: Vec<_> = bottom
            .iter()
            .map(|p| std::array::from_fn(|a| p[a] + bore.height * bore.axis[a]))
            .collect();
        for k in 0..n {
            let j = (k + 1) % n;
            let (s, c) = (std::f64::consts::TAU * (k as f64 + 0.5) / n as f64).sin_cos();
            patches.push(MeshPatch {
                face: bore.wall,
                points: vec![bottom[j], bottom[k], top[k], top[j]],
                normal: std::array::from_fn(|a| -(c * bore.x_direction[a] + s * y[a])),
                boundary_edges: vec![Some(lower), None, Some(upper), None],
            });
        }
        sampled.insert(lower, bottom);
        sampled.insert(upper, top);
    }
    for (id, face) in g.base.faces.iter().enumerate() {
        let face_id = FaceId(id);
        let normal = face.try_plane().unwrap().normal;
        if face_id != g.start_cap && face_id != g.end_cap {
            let mut points = Vec::new();
            let mut edges = Vec::new();
            for usage in &g.base.loops[face.loops[0].0].edges {
                points.push(
                    g.base.vertices
                        [g.base.edges[usage.edge.0].vertices[usize::from(usage.reversed)].0]
                        .position,
                );
                edges.push(Some(usage.edge));
            }
            patches.push(MeshPatch {
                face: face_id,
                points,
                normal,
                boundary_edges: edges,
            });
            continue;
        }
        let mut rings = Vec::new();
        let mut world = Vec::new();
        let mut boundary = BTreeMap::new();
        let x = g.x_direction;
        let y = cross(normal, x);
        for loop_id in &model.faces[id].loops {
            let offset = world.len();
            let mut ring = Vec::new();
            let mut edge_ids = Vec::new();
            for usage in &model.loops[loop_id.0].edges {
                if let Some(points) = sampled.get(&usage.edge) {
                    for k in 0..points.len() {
                        let k = if usage.reversed {
                            (points.len() - k) % points.len()
                        } else {
                            k
                        };
                        world.push(points[k]);
                        edge_ids.push(usage.edge);
                    }
                } else {
                    world.push(
                        model.vertices
                            [model.edges[usage.edge.0].vertices[usize::from(usage.reversed)].0]
                            .position,
                    );
                    edge_ids.push(usage.edge);
                }
            }
            for (k, &edge) in edge_ids.iter().enumerate() {
                boundary.insert((offset + k, offset + (k + 1) % edge_ids.len()), edge);
                let d = sub(world[offset + k], g.origin);
                ring.push([dot(d, x), dot(d, y)]);
            }
            rings.push(ring);
        }
        for t in crate::triangulation::triangulate_rings_2d(&rings)? {
            patches.push(MeshPatch {
                face: face_id,
                points: t.map(|i| world[i]).to_vec(),
                normal,
                boundary_edges: (0..3)
                    .map(|k| boundary.get(&(t[k], t[(k + 1) % 3])).copied())
                    .collect(),
            });
        }
    }
    Ok(CylinderMesh {
        patches,
        segments,
        chordal_error_bound: bound,
    })
}

fn tessellate_revolve(
    model: &ModelSnapshot,
    geometry: &crate::RevolveGeometry,
    policy: TessellationPolicy,
) -> Result<CylinderMesh, KernelError> {
    use std::collections::BTreeMap;
    let circles: Vec<_> = model
        .edges
        .iter()
        .enumerate()
        .filter_map(|(id, edge)| {
            if let crate::Curve::Circle(circle) = &edge.curve {
                Some((EdgeId(id), circle))
            } else {
                None
            }
        })
        .collect();
    let radius = circles.iter().map(|(_, c)| c.radius).fold(0., f64::max);
    let scale = circles
        .iter()
        .flat_map(|(_, c)| c.origin)
        .map(f64::abs)
        .fold(radius, f64::max);
    let (n, error) = circle_resolution(radius, 4. * scale, [0.; 3], policy)?;
    if n.checked_mul(geometry.segments.len())
        .is_none_or(|count| count > 200_000)
    {
        return Err(KernelError::ResourceLimit(
            "revolved display exceeds 200000 patches".into(),
        ));
    }
    // Every shared circle is sampled once. Adjacent planar and curved patches
    // reuse bit-identical points, including across changes in shoulder radius.
    let rings: BTreeMap<_, Vec<[f64; 3]>> = circles
        .iter()
        .map(|(id, c)| {
            let y = cross(c.axis, c.x_direction);
            let points = (0..n)
                .map(|i| {
                    let (s, co) = (std::f64::consts::TAU * i as f64 / n as f64).sin_cos();
                    std::array::from_fn(|a| {
                        c.origin[a] + c.radius * (co * c.x_direction[a] + s * y[a])
                    })
                })
                .collect();
            (*id, points)
        })
        .collect();
    let mut patches = Vec::with_capacity(n * geometry.segments.len());
    for segment in &geometry.segments {
        if segment.start[1] != segment.end[1] {
            let pole = |z: f64| std::array::from_fn(|a| geometry.origin[a] + geometry.axis[a] * z);
            let start = segment.start_circle.map(|id| &rings[&id]);
            let end = segment.end_circle.map(|id| &rings[&id]);
            let sign = (segment.end[1] - segment.start[1]).signum();
            let slope = (segment.end[0] - segment.start[0]) / (segment.end[1] - segment.start[1]);
            // Use the normal of the planar chordal trapezoid exported to STL.
            let facet_slope = slope * (std::f64::consts::PI / n as f64).cos();
            let normal_length = facet_slope.hypot(1.);
            let y = cross(geometry.axis, geometry.x_direction);
            for i in 0..n {
                let j = (i + 1) % n;
                let (s, c) = (std::f64::consts::TAU * (i as f64 + 0.5) / n as f64).sin_cos();
                let (points, boundary_edges) = match (start, end) {
                    (Some(start), Some(end)) => (
                        vec![start[i], start[j], end[j], end[i]],
                        vec![segment.start_circle, None, segment.end_circle, None],
                    ),
                    (Some(start), None) => (
                        vec![start[i], start[j], pole(segment.end[1])],
                        vec![segment.start_circle, None, None],
                    ),
                    (None, Some(end)) => (
                        vec![pole(segment.start[1]), end[j], end[i]],
                        vec![None, segment.end_circle, None],
                    ),
                    (None, None) => unreachable!("axis segments have no surface"),
                };
                patches.push(MeshPatch {
                    face: segment.face,
                    points,
                    normal: std::array::from_fn(|a| {
                        sign * (c * geometry.x_direction[a] + s * y[a]
                            - facet_slope * geometry.axis[a])
                            / normal_length
                    }),
                    boundary_edges,
                });
            }
        } else {
            let (outer_id, inner_id) = if segment.start[0] > segment.end[0] {
                (segment.start_circle.unwrap(), segment.end_circle)
            } else {
                (segment.end_circle.unwrap(), segment.start_circle)
            };
            let outer = &rings[&outer_id];
            let inner = inner_id.map(|id| &rings[&id]);
            let crate::Curve::Circle(circle) = &model.edges[outer_id.0].curve else {
                unreachable!()
            };
            let normal = model.faces[segment.face.0].try_plane().unwrap().normal;
            let reverse = segment.start[0] < segment.end[0];
            for i in 0..n {
                let j = (i + 1) % n;
                let (points, edges) = match inner {
                    Some(inner) if reverse => (
                        vec![inner[i], inner[j], outer[j], outer[i]],
                        vec![inner_id, None, Some(outer_id), None],
                    ),
                    Some(inner) => (
                        vec![inner[i], outer[i], outer[j], inner[j]],
                        vec![None, Some(outer_id), None, inner_id],
                    ),
                    None if reverse => (
                        vec![circle.origin, outer[j], outer[i]],
                        vec![None, Some(outer_id), None],
                    ),
                    None => (
                        vec![circle.origin, outer[i], outer[j]],
                        vec![None, Some(outer_id), None],
                    ),
                };
                patches.push(MeshPatch {
                    face: segment.face,
                    points,
                    normal,
                    boundary_edges: edges,
                });
            }
        }
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
    fn revolve_mesh_resource_limit_preserves_native_model() {
        let t = Tolerance::default();
        let model = crate::revolve_polygon(
            &[
                [0., 0.],
                [5., 0.],
                [5., 10.],
                [3., 10.],
                [3., 20.],
                [0., 20.],
            ],
            t,
        )
        .unwrap()
        .model;
        let before = serde_json::to_vec(&model).unwrap();
        let error = tessellate_analytic(
            &model,
            t,
            TessellationPolicy {
                chordal_error: 1e-8,
                max_segments: 65536,
            },
        )
        .unwrap_err();
        assert!(
            matches!(error, KernelError::ResourceLimit(message) if message.contains("200000 patches"))
        );
        assert_eq!(serde_json::to_vec(&model).unwrap(), before);
        assert!(model.validate(t).valid);
        assert!(tessellate_analytic(&model, t, TessellationPolicy::default()).is_ok());
    }

    #[test]
    fn mixed_cap_budget_fails_without_changing_authoritative_geometry() {
        let tolerance = Tolerance::default();
        let model = crate::extrude_polygon_with_circular_holes(
            &[[0., 0.], [40., 0.], [40., 40.], [0., 40.]],
            &[],
            &[crate::CircleHole {
                center: [20., 20.],
                radius: 10.,
            }],
            5.,
            tolerance,
        )
        .unwrap()
        .model;
        let before = serde_json::to_vec(&model).unwrap();
        let error = tessellate_analytic(
            &model,
            tolerance,
            TessellationPolicy {
                chordal_error: 1e-6,
                max_segments: 65536,
            },
        )
        .unwrap_err();
        assert!(matches!(error, KernelError::ResourceLimit(_)));
        assert_eq!(serde_json::to_vec(&model).unwrap(), before);
        assert!(model.validate(tolerance).valid);
        assert!(tessellate_analytic(&model, tolerance, TessellationPolicy::default()).is_ok());
    }

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
