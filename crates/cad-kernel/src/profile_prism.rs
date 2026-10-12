//! Exact polygon prisms with disjoint analytic circular through bores.
use crate::math::{finite, norm};
use crate::*;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircleHole {
    pub center: [f64; 2],
    pub radius: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ProfilePrismGeometry {
    pub base: ModelSnapshot,
    pub bores: Vec<CylinderGeometry>,
    pub start_cap: FaceId,
    pub end_cap: FaceId,
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub height: f64,
}
fn bad() -> KernelError {
    KernelError::Unsupported("invalid polygon prism with circular through holes".into())
}
pub fn validate_polygon_with_circular_holes(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    circles: &[CircleHole],
    tol: Tolerance,
) -> Result<(), KernelError> {
    validate_polygon_with_holes(outer, holes, tol)?;
    if circles.is_empty() {
        return Ok(());
    }
    if circles.len() + holes.len() > MAX_POLYGON_HOLES {
        return Err(KernelError::ResourceLimit(
            "at most 32 combined profile holes".into(),
        ));
    }
    let scale = outer
        .iter()
        .chain(holes.iter().flatten())
        .flatten()
        .copied()
        .chain(
            circles
                .iter()
                .flat_map(|c| [c.center[0], c.center[1], c.radius]),
        )
        .map(f64::abs)
        .fold(0., f64::max);
    let error = scale * 512. * f64::EPSILON;
    if !error.is_finite() || error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    for (i, c) in circles.iter().enumerate() {
        validate_circle(c.radius, tol)?;
        if !c.center.iter().all(|v| v.is_finite()) {
            return Err(KernelError::NumericRange);
        }
        if !crate::simple_polygon::inside(c.center, outer)
            || holes
                .iter()
                .any(|h| crate::simple_polygon::inside(c.center, h))
        {
            return Err(bad());
        }
        for ring in std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)) {
            for j in 0..ring.len() {
                let distance =
                    crate::simple_polygon::distance(c.center, ring[j], ring[(j + 1) % ring.len()]);
                if !distance.is_finite() || distance - c.radius <= tol.linear + error {
                    return Err(bad());
                }
            }
        }
        for d in &circles[..i] {
            if (c.center[0] - d.center[0]).hypot(c.center[1] - d.center[1]) - c.radius - d.radius
                <= tol.linear + error
            {
                return Err(bad());
            }
        }
    }
    Ok(())
}
pub fn extrude_polygon_with_circular_holes(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    circles: &[CircleHole],
    height: f64,
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_polygon_with_circular_holes(outer, holes, circles, tol)?;
    let mut result = extrude_polygon_with_holes(outer, holes, height, tol)?;
    for (i, c) in circles.iter().enumerate() {
        let support = extrude_circle(c.radius, height, tol)?.model;
        let support = translate(&support, [c.center[0], c.center[1], 0.], tol)?.model;
        let m = &mut result.model;
        let (v, e, l, f) = (
            m.vertices.len(),
            m.edges.len(),
            m.loops.len(),
            m.faces.len(),
        );
        m.vertices.extend(support.vertices);
        m.edges.extend(support.edges.into_iter().map(|mut edge| {
            edge.vertices = edge.vertices.map(|id| VertexId(id.0 + v));
            edge
        }));
        let eu = |n, r| EdgeUse {
            edge: EdgeId(e + n),
            reversed: r,
        };
        m.loops.extend([
            Loop {
                edges: vec![eu(0, false)],
            },
            Loop {
                edges: vec![eu(1, true)],
            },
            Loop {
                edges: vec![eu(0, true), eu(2, false), eu(1, false), eu(2, true)],
            },
        ]);
        m.faces[0].loops.push(LoopId(l));
        m.faces[1].loops.push(LoopId(l + 1));
        let mut wall = support.faces[2].clone();
        if let Surface::Cylinder(c) = &mut wall.surface {
            c.inward = true;
        }
        wall.loops = vec![LoopId(l + 2)];
        m.faces.push(wall);
        m.shells[0].faces.push(FaceId(f));
        result
            .history
            .generated_faces
            .insert(format!("circular_hole_{i}_wall"), vec![FaceId(f)]);
    }
    let report = result.model.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    Ok(result)
}
// The convenience volume query has no modeling tolerance argument. Recover
// using a conservative numeric margin for the stored coordinates;
// geometric errors remain bounded by roundoff, never by this tolerance.
pub(crate) fn volume(model: &ModelSnapshot) -> Result<f64, KernelError> {
    let scale = model
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .map(f64::abs)
        .fold(0., f64::max);
    let tolerance = Tolerance {
        // Recognition also projects relative to a bore origin. A difference
        // between points in the world bounds has norm <= 2*sqrt(3)*scale;
        // a unit-axis projection cannot increase it. Include that local range
        // instead of assuming all coordinates are positive or origin-centered.
        linear: scale * (4. * 8192. * f64::EPSILON),
    };
    recognize(model, tolerance)?
        .properties(tolerance)
        .map(|p| p.volume)
}
pub(crate) fn recognize(
    m: &ModelSnapshot,
    tol: Tolerance,
) -> Result<ProfilePrismGeometry, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    let count = m
        .faces
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cylinder(_)))
        .count();
    if count == 0
        || count > MAX_POLYGON_HOLES
        || m.vertices.len() < 2 * count + 6
        || m.edges.len() < 3 * count + 9
        || m.loops.len() < 3 * count + 5
        || m.faces.len() < count + 5
        || m.shells.len() != 1
        || m.solids.len() != 1
    {
        return Err(bad());
    }
    let (nv, ne, nl, nf) = (
        m.vertices.len() - 2 * count,
        m.edges.len() - 3 * count,
        m.loops.len() - 3 * count,
        m.faces.len() - count,
    );
    let mut base = m.clone();
    base.vertices.truncate(nv);
    base.edges.truncate(ne);
    base.loops.truncate(nl);
    base.faces.truncate(nf);
    for f in &mut base.faces[..2] {
        if f.loops.len() <= count {
            return Err(bad());
        }
        f.loops.truncate(f.loops.len() - count);
    }
    if m.shells[0].faces != (0..m.faces.len()).map(FaceId).collect::<Vec<_>>()
        || m.solids[0].shells != [ShellId(0)]
    {
        return Err(bad());
    }
    base.shells[0].faces.truncate(nf);
    if base.has_curved_geometry() || !base.validate(tol).valid {
        return Err(bad());
    }
    crate::prism::validate_prism_caps(&base, tol, true)?;
    let mut bores = vec![];
    for i in 0..count {
        let (v, e, l, f) = (nv + 2 * i, ne + 3 * i, nl + 3 * i, nf + i);
        let eu = |n, r| EdgeUse {
            edge: EdgeId(e + n),
            reversed: r,
        };
        if m.faces[0].loops[base.faces[0].loops.len() + i] != LoopId(l)
            || m.faces[1].loops[base.faces[1].loops.len() + i] != LoopId(l + 1)
            || m.faces[f].loops != [LoopId(l + 2)]
            || m.loops[l].edges != [eu(0, false)]
            || m.loops[l + 1].edges != [eu(1, true)]
            || m.loops[l + 2].edges != [eu(0, true), eu(2, false), eu(1, false), eu(2, true)]
        {
            return Err(bad());
        }
        let Surface::Cylinder(wall) = &m.faces[f].surface else {
            return Err(bad());
        };
        if !wall.inward {
            return Err(bad());
        }
        let Curve::Circle(top) = &m.edges[e + 1].curve else {
            return Err(bad());
        };
        let mut disk = extrude_circle(wall.radius, norm(sub(top.origin, wall.origin)), tol)?.model;
        disk.vertices = m.vertices[v..v + 2].to_vec();
        disk.edges = m.edges[e..e + 3]
            .iter()
            .cloned()
            .map(|mut edge| {
                for id in &mut edge.vertices {
                    id.0 = id.0.checked_sub(v).unwrap_or(usize::MAX);
                }
                edge
            })
            .collect();
        disk.faces[0].surface = Surface::Plane(Plane {
            origin: wall.origin,
            normal: wall.axis.map(|x| -x),
        });
        disk.faces[1].surface = Surface::Plane(Plane {
            origin: top.origin,
            normal: wall.axis,
        });
        let mut support = wall.clone();
        support.inward = false;
        disk.faces[2].surface = Surface::Cylinder(support);
        let mut g = crate::analytic::recognize(&disk, Some(tol))?;
        g.wall = FaceId(f);
        g.start_cap = FaceId(0);
        g.end_cap = FaceId(1);
        bores.push(g);
    }
    let first = &bores[0];
    let origin = first.origin;
    let axis = first.axis;
    let x_direction = first.x_direction;
    let height = first.height;
    let y = cross(axis, x_direction);
    let scale = m
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .map(f64::abs)
        .fold(height, f64::max);
    let error = scale * 512. * f64::EPSILON;
    if error > tol.linear / 16. || !error.is_normal() {
        return Err(KernelError::NumericRange);
    }
    for g in &bores {
        if norm(sub(g.axis, axis)) > 64. * f64::EPSILON
            || norm(sub(g.x_direction, x_direction)) > 64. * f64::EPSILON
            || (g.height - height).abs() > error
        {
            return Err(bad());
        }
        for (id, center, normal) in [
            (0, g.origin, axis.map(|v| -v)),
            (
                1,
                std::array::from_fn(|j| g.origin[j] + g.height * axis[j]),
                axis,
            ),
        ] {
            let plane = base.faces[id].try_plane().ok_or_else(bad)?;
            if norm(sub(plane.normal, normal)) > 64. * f64::EPSILON
                || dot(sub(center, plane.origin), axis).abs() > error
            {
                return Err(bad());
            }
        }
    }
    let project = |p| {
        let d = sub(p, origin);
        [dot(d, x_direction), dot(d, y)]
    };
    let rings: Vec<Vec<[f64; 2]>> = base.faces[1]
        .loops
        .iter()
        .map(|l| {
            base.loop_points(*l)
                .unwrap()
                .into_iter()
                .map(project)
                .collect()
        })
        .collect();
    let circles: Vec<_> = bores
        .iter()
        .map(|g| CircleHole {
            center: project(g.origin),
            radius: g.radius,
        })
        .collect();
    validate_polygon_with_circular_holes(&rings[0], &rings[1..], &circles, tol)?;
    let result = ProfilePrismGeometry {
        base,
        bores,
        start_cap: FaceId(0),
        end_cap: FaceId(1),
        origin,
        axis,
        x_direction,
        height,
    };
    result.properties(tol)?;
    Ok(result)
}
impl ProfilePrismGeometry {
    pub(crate) fn properties(&self, tol: Tolerance) -> Result<MassProperties, KernelError> {
        let base = self.base.mass_properties(tol)?;
        let reference = base.centroid.ok_or_else(bad)?;
        let mut volume = base.volume;
        let mut first = [0.; 3];
        let mut inertia = base.central_inertia.ok_or_else(bad)?;
        let mut absolute = inertia.map(|row| row.map(f64::abs));
        for g in &self.bores {
            let p = g.properties()?;
            let d = sub(p.centroid.unwrap(), reference);
            volume -= p.volume;
            for i in 0..3 {
                first[i] -= p.volume * d[i];
                for j in 0..3 {
                    let own = p.central_inertia.unwrap()[i][j];
                    let shift = p.volume * (if i == j { dot(d, d) } else { 0. } - d[i] * d[j]);
                    inertia[i][j] -= own + shift;
                    absolute[i][j] += own.abs() + shift.abs();
                }
            }
        }
        if !volume.is_normal() || volume <= base.volume * 4096. * f64::EPSILON {
            return Err(KernelError::NumericRange);
        }
        let d = first.map(|v| v / volume);
        let centroid = std::array::from_fn(|i| reference[i] + d[i]);
        for i in 0..3 {
            for j in 0..3 {
                let shift = volume * (if i == j { dot(d, d) } else { 0. } - d[i] * d[j]);
                inertia[i][j] -= shift;
                absolute[i][j] += shift.abs();
            }
        }
        if !finite(centroid)
            || inertia
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || (*v != 0. && !v.is_normal()))
            || absolute.iter().flatten().any(|v| !v.is_finite())
            || (0..3).any(|i| inertia[i][i] <= absolute[i][i] * 4096. * f64::EPSILON)
        {
            return Err(KernelError::NumericRange);
        }
        Ok(MassProperties {
            volume,
            centroid: Some(centroid),
            central_inertia: Some(inertia),
        })
    }
    pub(crate) fn cap_centroid(&self, id: FaceId, tol: Tolerance) -> Result<[f64; 3], KernelError> {
        let center = self.properties(tol)?.centroid.unwrap();
        let offset = if id == self.start_cap {
            -self.height / 2.
        } else {
            self.height / 2.
        };
        Ok(std::array::from_fn(|i| center[i] + offset * self.axis[i]))
    }
}
impl ModelSnapshot {
    pub fn profile_prism_geometry(
        &self,
        tol: Tolerance,
    ) -> Result<Option<ProfilePrismGeometry>, KernelError> {
        if !tol.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if self.has_curved_geometry() && crate::analytic::recognize(self, Some(tol)).is_err() {
            recognize(self, tol)
                .map(Some)
                .or_else(|_| crate::revolve::recognize(self, tol).map(|_| None))
                .or_else(|_| crate::arc_prism::recognize(self, tol).map(|_| None))
                .or_else(|_| crate::bored_prism::recognize(self, tol).map(|_| None))
        } else {
            let r = self.validate(tol);
            if r.valid {
                Ok(None)
            } else {
                Err(KernelError::InvalidInput(r))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9 * b.abs().max(1.), "{a} != {b}");
    }
    fn fixture() -> ModelSnapshot {
        extrude_polygon_with_circular_holes(
            &[[0., 0.], [40., 0.], [40., 30.], [0., 30.]],
            &[],
            &[
                CircleHole {
                    center: [10., 12.],
                    radius: 3.,
                },
                CircleHole {
                    center: [29., 20.],
                    radius: 4.,
                },
            ],
            8.,
            Tolerance::default(),
        )
        .unwrap()
        .model
    }
    #[test]
    fn exact_mixed_profile_moments_and_oblique_placement() {
        let t = Tolerance::default();
        let m = fixture();
        assert!(m.validate(t).valid);
        assert_eq!(m.faces.len(), 8);
        assert_eq!(m.vertices.len(), 12);
        assert_eq!(m.edges.len(), 18);
        let p = m.mass_properties(t).unwrap();
        let vb = 9600.;
        let v1 = 72. * std::f64::consts::PI;
        let v2 = 128. * std::f64::consts::PI;
        let v = vb - v1 - v2;
        let center = [
            (vb * 20. - v1 * 10. - v2 * 29.) / v,
            (vb * 15. - v1 * 12. - v2 * 20.) / v,
            4.,
        ];
        close(p.volume, v);
        close(m.volume(), v);
        for (i, c) in center.iter().enumerate() {
            close(p.centroid.unwrap()[i], *c);
        }
        let parts = [
            (
                vb,
                [20., 15., 4.],
                [
                    vb * (900. + 64.) / 12.,
                    vb * (1600. + 64.) / 12.,
                    vb * 2500. / 12.,
                ],
            ),
            (
                -v1,
                [10., 12., 4.],
                [
                    -v1 * (27. + 64.) / 12.,
                    -v1 * (27. + 64.) / 12.,
                    -v1 * 9. / 2.,
                ],
            ),
            (
                -v2,
                [29., 20., 4.],
                [
                    -v2 * (48. + 64.) / 12.,
                    -v2 * (48. + 64.) / 12.,
                    -v2 * 16. / 2.,
                ],
            ),
        ];
        for i in 0..3 {
            for j in 0..3 {
                let expected: f64 = parts
                    .iter()
                    .map(|(v, c, tensor)| {
                        let d = sub(*c, center);
                        (if i == j {
                            tensor[i] + v * dot(d, d)
                        } else {
                            0.
                        }) - v * d[i] * d[j]
                    })
                    .sum();
                close(p.central_inertia.unwrap()[i][j], expected);
            }
        }
        let top = m.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], t).unwrap();
        close(top.origin[0], center[0]);
        close(top.origin[1], center[1]);
        close(top.origin[2], 8.);
        let moved = transform(&m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
            .unwrap()
            .model;
        assert!(moved.profile_prism_geometry(t).unwrap().is_some());
        close(moved.mass_properties(t).unwrap().volume, v);
        assert!(moved.cylinder_geometry(t).unwrap().is_none());
    }
    #[test]
    fn mixed_profile_validation_rejects_corrupted_support_and_topology() {
        let t = Tolerance::default();
        let m = fixture();
        let changes: Vec<fn(&mut ModelSnapshot)> = vec![
            |m| m.vertices[8].position[0] += 1e-9,
            |m| m.loops[6].edges[0].reversed = true,
            |m| m.faces[0].loops.pop().map(drop).unwrap(),
            |m| m.edges[12].vertices[0] = VertexId(0),
            |m| {
                if let Surface::Cylinder(c) = &mut m.faces[6].surface {
                    c.inward = false
                }
            },
            |m| m.faces[1].try_plane_mut().unwrap().origin[2] += 1e-9,
            |m| m.solids.clear(),
        ];
        for (i, c) in changes.into_iter().enumerate() {
            let mut b = m.clone();
            c(&mut b);
            assert!(!b.validate(t).valid, "mutation {i}");
            assert!(b.mass_properties(t).is_err());
            assert!(b.volume().is_nan());
        }
    }
    #[test]
    fn convenience_volume_covers_off_center_bore_projection_range() {
        let t = Tolerance::default();
        let model = extrude_polygon_with_circular_holes(
            &[[-40., -20.], [40., -20.], [40., 20.], [-40., 20.]],
            &[],
            &[CircleHole {
                center: [-30., 0.],
                radius: 2.,
            }],
            5.,
            t,
        )
        .unwrap()
        .model;
        let expected = 16000. - 20. * std::f64::consts::PI;
        for body in [
            model.clone(),
            transform(&model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                .unwrap()
                .model,
        ] {
            assert!(body.validate(t).valid);
            close(body.volume(), expected);
            close(body.mass_properties(t).unwrap().volume, expected);
        }
    }

    #[test]
    fn scale_specific_tolerance_preserves_exact_volume() {
        for scale in [1e-6, 1., 1e3] {
            let t = Tolerance {
                linear: scale * 1e-8,
            };
            let outer = [
                [0., 0.],
                [40. * scale, 0.],
                [40. * scale, 30. * scale],
                [0., 30. * scale],
            ];
            let m = extrude_polygon_with_circular_holes(
                &outer,
                &[],
                &[CircleHole {
                    center: [10. * scale, 12. * scale],
                    radius: 3. * scale,
                }],
                8. * scale,
                t,
            )
            .unwrap()
            .model;
            let expected = (9600. - 72. * std::f64::consts::PI) * scale.powi(3);
            assert!((m.volume() / expected - 1.).abs() < 1e-12);
            assert!((m.mass_properties(t).unwrap().volume / expected - 1.).abs() < 1e-12);
        }
    }
    #[test]
    fn circles_require_material_containment_and_strict_separation() {
        let t = Tolerance::default();
        let outer = [[0., 0.], [40., 0.], [40., 30.], [0., 30.]];
        for circles in [
            vec![CircleHole {
                center: [2., 12.],
                radius: 3.,
            }],
            vec![CircleHole {
                center: [3., 12.],
                radius: 3.,
            }],
            vec![
                CircleHole {
                    center: [10., 12.],
                    radius: 3.,
                },
                CircleHole {
                    center: [16., 12.],
                    radius: 3.,
                },
            ],
            vec![CircleHole {
                center: [f64::NAN, 12.],
                radius: 3.,
            }],
        ] {
            assert!(validate_polygon_with_circular_holes(&outer, &[], &circles, t).is_err());
        }
        let holes = vec![vec![[20., 10.], [20., 20.], [30., 20.], [30., 10.]]];
        assert!(
            extrude_polygon_with_circular_holes(
                &outer,
                &holes,
                &[CircleHole {
                    center: [10., 12.],
                    radius: 3.
                }],
                8.,
                t
            )
            .unwrap()
            .model
            .validate(t)
            .valid
        );
        assert!(validate_polygon_with_circular_holes(
            &outer,
            &holes,
            &[CircleHole {
                center: [25., 15.],
                radius: 2.
            }],
            t
        )
        .is_err());
        let u = [
            [0., 0.],
            [40., 0.],
            [40., 30.],
            [30., 30.],
            [30., 10.],
            [10., 10.],
            [10., 30.],
            [0., 30.],
        ];
        assert!(extrude_polygon_with_circular_holes(
            &u,
            &[],
            &[CircleHole {
                center: [5., 20.],
                radius: 2.
            }],
            8.,
            t
        )
        .is_ok());
        assert!(validate_polygon_with_circular_holes(
            &u,
            &[],
            &[CircleHole {
                center: [20., 20.],
                radius: 2.
            }],
            t
        )
        .is_err());
    }
}
