use super::*;
fn cylinder() -> ModelSnapshot {
    cad_kernel::extrude_annulus(5., 2., 8., Tolerance::default())
        .unwrap()
        .model
}
fn analytic_equal(a: &CylinderMesh, b: &CylinderMesh) {
    assert_eq!(a.segments, b.segments);
    assert_eq!(a.chordal_error_bound, b.chordal_error_bound);
    assert_eq!(a.patches.len(), b.patches.len());
    for (a, b) in a.patches.iter().zip(&b.patches) {
        assert_eq!(a.face, b.face);
        assert_eq!(a.points, b.points);
        assert_eq!(a.normal, b.normal);
        assert_eq!(a.boundary_edges, b.boundary_edges);
    }
}
#[test]
fn analytic_reuse_exact_identity_full_policy_and_caller_mutation() {
    let model = cylinder();
    let tolerance = Tolerance::default();
    let policy = TessellationPolicy::default();
    let mut cache = GenerationMeshCache::default();
    let first = cache.analytic(&model, tolerance, policy).unwrap();
    let direct = cad_kernel::tessellate_analytic(&model, tolerance, policy).unwrap();
    analytic_equal(&first, &direct);
    let hit = cache.analytic(&model.clone(), tolerance, policy).unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
    assert_eq!(cache.stats().hits, 1);
    assert_eq!(cache.stats().builds, 1);
    let mut caller = hit;
    Arc::make_mut(&mut caller).patches[0].points[0][0] += 10.;
    let preserved = cache.analytic(&model, tolerance, policy).unwrap();
    analytic_equal(&preserved, &direct);
    for policy in [
        TessellationPolicy {
            chordal_error: 0.1,
            ..policy
        },
        TessellationPolicy {
            max_segments: 2048,
            ..policy
        },
    ] {
        cache.analytic(&model, tolerance, policy).unwrap();
    }
    cache
        .analytic(&model, Tolerance { linear: 1e-6 }, policy)
        .unwrap();
    let changed = cad_kernel::extrude_annulus(6., 2., 8., tolerance)
        .unwrap()
        .model;
    cache.analytic(&changed, tolerance, policy).unwrap();
    assert_eq!(cache.stats().builds, 5);
    assert_eq!(cache.stats().entries, 5);
    assert!(cache.stats().retained_bytes <= cache.stats().max_retained_bytes);
    let stats = serde_json::to_value(cache.stats()).unwrap();
    assert_eq!(stats["kernel_build_id"], KERNEL_BUILD_ID);
    assert_eq!(stats["mesher_build_id"].as_str().unwrap().len(), 64);
}
#[test]
fn planar_faces_preserve_certified_indices_and_scope() {
    let tolerance = Tolerance::default();
    let model = cad_kernel::extrude_polygon_with_holes(
        &[[0., 0.], [10., 0.], [10., 10.], [0., 10.]],
        &[vec![[2., 2.], [2., 4.], [4., 4.], [4., 2.]]],
        3.,
        tolerance,
    )
    .unwrap()
    .model;
    let mut cache = GenerationMeshCache::default();
    for face in [FaceId(0), FaceId(1)] {
        let first = cache.planar_face(&model, tolerance, face).unwrap();
        assert_eq!(
            *first,
            cad_kernel::triangulate_face(&model, face, tolerance).unwrap()
        );
        let second = cache.planar_face(&model, tolerance, face).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
    }
    assert_eq!(cache.stats().builds, 2);
    assert_eq!(cache.stats().hits, 2);
    assert!(cache
        .planar_face(&model, tolerance, FaceId(usize::MAX))
        .is_err());
    assert_eq!(cache.stats().entries, 2);
}
#[test]
fn invalid_requests_and_failed_tessellation_never_enter_cache() {
    let model = cylinder();
    let mut cache = GenerationMeshCache::default();
    let tolerance = Tolerance::default();
    let policy = TessellationPolicy::default();
    for _ in 0..2 {
        assert!(cache
            .analytic(
                &model,
                tolerance,
                TessellationPolicy {
                    max_segments: 12,
                    ..policy
                }
            )
            .is_err());
    }
    assert_eq!(cache.stats().builds, 0);
    assert_eq!(cache.stats().entries, 0);
    assert_eq!(cache.stats().misses, 2);
    cache.analytic(&model, tolerance, policy).unwrap();
    for bad in [f64::NAN, f64::INFINITY] {
        let mut changed = model.clone();
        changed.vertices[0].position[0] = bad;
        assert!(cache.analytic(&changed, tolerance, policy).is_err());
    }
    assert!(cache
        .analytic(&model, Tolerance { linear: f64::NAN }, policy)
        .is_err());
    assert!(cache
        .analytic(
            &model,
            tolerance,
            TessellationPolicy {
                chordal_error: f64::NAN,
                ..policy
            }
        )
        .is_err());
    assert_eq!(cache.stats().hits, 0);
    assert_eq!(cache.stats().builds, 1);
    assert_eq!(cache.stats().entries, 1);
    // Simulate a digest collision internally: byte-equality proof still rejects.
    cache.entries.values_mut().next().unwrap().witness[0] ^= 1;
    assert!(matches!(
        cache.analytic(&model, tolerance, policy),
        Err(MeshError::IdentityCollision)
    ));
}
#[test]
fn bounded_retention_eviction_clear_and_uncached_results() {
    let model = cylinder();
    let tolerance = Tolerance::default();
    let policy = TessellationPolicy::default();
    let mut unbounded = GenerationMeshCache::default();
    let reference = unbounded.analytic(&model, tolerance, policy).unwrap();
    let charge = unbounded.stats().retained_bytes;
    let mut none = GenerationMeshCache::new(0);
    for _ in 0..2 {
        analytic_equal(
            &none.analytic(&model, tolerance, policy).unwrap(),
            &reference,
        );
    }
    assert_eq!(none.stats().builds, 2);
    assert_eq!(none.stats().hits, 0);
    assert_eq!(none.stats().retained_bytes, 0);
    let mut small = GenerationMeshCache::new(charge - 1);
    small.analytic(&model, tolerance, policy).unwrap();
    assert_eq!(small.stats().entries, 0);
    let mut one = GenerationMeshCache::new(charge);
    let held = one.analytic(&model, tolerance, policy).unwrap();
    one.analytic(
        &model,
        tolerance,
        TessellationPolicy {
            chordal_error: 0.1,
            ..policy
        },
    )
    .unwrap();
    assert_eq!(one.stats().evictions, 1);
    assert_eq!(one.stats().entries, 1);
    assert!(one.stats().retained_bytes <= charge);
    analytic_equal(&held, &reference);
    one.clear();
    assert_eq!(one.stats().retained_bytes, 0);
    assert_eq!(one.stats().entries, 0);
    assert_eq!(one.stats().builds, 2);
    analytic_equal(&held, &reference);
}
