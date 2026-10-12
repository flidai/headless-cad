use super::*;
fn frame(translation: [f64; 3]) -> RigidTransform {
    RigidTransform {
        translation,
        ..Default::default()
    }
}
fn parts() -> BTreeMap<String, BuiltPart> {
    BTreeMap::from([(
        "p".into(),
        BuiltPart {
            datums: BTreeMap::from([
                ("origin".into(), frame([0.; 3])),
                ("mount".into(), frame([2., 0., 0.])),
                ("tip".into(), frame([4., 0., 0.])),
            ]),
            metadata: PartMetadata::default(),
            feature_id: "shape".into(),
            content_hash: "shapehash".into(),
            result: Arc::new(
                cad_kernel::extrude_rectangle(2., 3., 4., Tolerance::default()).unwrap(),
            ),
        },
    )])
}
fn leaf(id: &str) -> Instance {
    Instance {
        id: id.into(),
        part: "p".into(),
        transform: Default::default(),
        attachment: None,
    }
}
fn attachment(target: &str, datum: &str, joint: PrescribedJoint) -> DatumAttachment {
    DatumAttachment {
        instance: target.into(),
        target_path: vec![],
        datum: "mount".into(),
        target_datum: datum.into(),
        joint: Some(joint),
    }
}
fn near(actual: [f64; 3], expected: [f64; 3]) {
    for (a, b) in actual.into_iter().zip(expected) {
        assert!((a - b).abs() < 1e-10, "{actual:?} != {expected:?}")
    }
}
fn chain() -> AssemblyDefinition {
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let mut base = leaf("base");
    base.transform = RigidTransform {
        translation: [10., 20., 30.],
        rotation: [half, 0., 0., half],
    };
    let mut hinge = leaf("hinge");
    hinge.transform = frame([4., 0., 0.]);
    hinge.attachment = Some(attachment(
        "base",
        "origin",
        PrescribedJoint::Revolute {
            position: std::f64::consts::FRAC_PI_2,
            min: -2.,
            max: 2.,
        },
    ));
    let mut slide = leaf("slide");
    slide.attachment = Some(attachment(
        "hinge",
        "tip",
        PrescribedJoint::Prismatic {
            position: 5.,
            min: 0.,
            max: 10.,
        },
    ));
    AssemblyDefinition {
        id: "chain".into(),
        instances: vec![slide, base, hinge],
        assemblies: vec![],
    }
}
#[test]
fn prescribed_chain_matches_independent_positions_and_reports_definition_dof() {
    let t = Tolerance::default();
    let d = chain();
    let built = BuiltAssembly::from_definitions(
        "chain".into(),
        parts(),
        BTreeMap::from([("chain".into(), d)]),
        t,
    )
    .unwrap();
    let hinge = built.instances.iter().find(|x| x.id == "hinge").unwrap();
    let slide = built.instances.iter().find(|x| x.id == "slide").unwrap();
    near(hinge.transform.translation, [14., 20., 28.]);
    near(hinge.transform.point([2., 0., 0.]), [14., 20., 30.]);
    near(slide.transform.translation, [14., 15., 30.]);
    let reports = built.joint_reports(t).unwrap();
    assert_eq!(reports.len(), 2);
    for r in reports {
        assert_eq!(r.coordinate_frame, "assembly_definition");
        assert_eq!(r.intrinsic_dof, 1);
        assert_eq!(r.remaining_dof, 0);
        assert!(r.residual_translation_norm_mm < t.linear);
        assert!(r.residual_angle_rad <= JOINT_ANGULAR_TOLERANCE_RAD);
    }
    built.validate(t).unwrap();
}
#[test]
fn repeated_child_attachments_keep_shared_reports_and_resolve_source_descendants() {
    let t = Tolerance::default();
    let child = chain();
    let mut base = leaf("fixed");
    base.transform = frame([100., 0., 0.]);
    let attached = AssemblyInstance {
        id: "moving".into(),
        assembly: "chain".into(),
        transform: frame([3., 0., 0.]),
        attachment: Some(AssemblyAttachment {
            instance: "fixed".into(),
            target_path: vec![],
            source_path: vec!["slide".into()],
            datum: "tip".into(),
            target_datum: "origin".into(),
            joint: Some(PrescribedJoint::Prismatic {
                position: 7.,
                min: 0.,
                max: 10.,
            }),
        }),
    };
    let root = AssemblyDefinition {
        id: "root".into(),
        instances: vec![base],
        assemblies: vec![
            attached,
            AssemblyInstance {
                id: "copy".into(),
                assembly: "chain".into(),
                transform: frame([-100., 0., 0.]),
                attachment: None,
            },
        ],
    };
    let built = BuiltAssembly::from_definitions(
        "root".into(),
        parts(),
        BTreeMap::from([("chain".into(), child), ("root".into(), root)]),
        t,
    )
    .unwrap();
    let reports = built.joint_reports(t).unwrap();
    assert_eq!(reports.len(), 3);
    let r = reports.iter().find(|r| r.definition_id == "root").unwrap();
    near(r.source_resolved_frame.translation, [103., 0., 7.]);
    assert_eq!(r.occurrence_kind, "assembly");
    assert_eq!(built.instances.len(), 7);
}
#[test]
fn invalid_limits_cycles_and_unrepresentable_motion_fail_native_construction() {
    let t = Tolerance::default();
    for (position, min, max) in [
        (f64::NAN, 0., 1.),
        (2., 0., 1.),
        (0., 1., -1.),
        (0., 0., f64::INFINITY),
    ] {
        assert!(PrescribedJoint::Prismatic { position, min, max }
            .validate()
            .is_err());
    }
    let mut d = chain();
    d.instances[1].attachment = Some(attachment(
        "slide",
        "origin",
        PrescribedJoint::Prismatic {
            position: 0.,
            min: 0.,
            max: 1.,
        },
    ));
    assert!(BuiltAssembly::from_definitions(
        "chain".into(),
        parts(),
        BTreeMap::from([("chain".into(), d)]),
        t
    )
    .is_err());
    let mut d = chain();
    d.instances[1].transform.translation = [1e20; 3];
    assert!(BuiltAssembly::from_definitions(
        "chain".into(),
        parts(),
        BTreeMap::from([("chain".into(), d)]),
        t
    )
    .is_err());
    for text in [
        r#"{"kind":"prismatic","position":0,"min":0,"max":1,"axis":[1,0,0]}"#,
        r#"{"kind":"revolute","position":0,"min":0}"#,
    ] {
        assert!(serde_json::from_str::<PrescribedJoint>(text).is_err());
    }
}
#[test]
fn residual_measurement_is_not_forced_to_zero_and_rejects_wrong_frames() {
    let t = Tolerance::default();
    let joint = PrescribedJoint::Prismatic {
        position: 0.,
        min: 0.,
        max: 0.,
    };
    let r = JointReport::checked(
        "d",
        "i",
        "part",
        joint,
        frame([1e-9, 0., 0.]),
        frame([0.; 3]),
        frame([0.; 3]),
        t,
    )
    .unwrap();
    assert_eq!(r.residual_translation_mm, [1e-9, 0., 0.]);
    assert_eq!(r.residual_translation_norm_mm, 1e-9);
    assert!(JointReport::checked(
        "d",
        "i",
        "part",
        joint,
        frame([1., 0., 0.]),
        frame([0.; 3]),
        frame([0.; 3]),
        t
    )
    .is_err());
}

#[test]
fn noncommuting_source_target_and_offset_frames_follow_the_declared_motion_order() {
    fn rotate(v: [f64; 3], axis: usize, angle: f64) -> [f64; 3] {
        let (s, c) = angle.sin_cos();
        match axis {
            0 => [v[0], c * v[1] - s * v[2], s * v[1] + c * v[2]],
            1 => [c * v[0] + s * v[2], v[1], -s * v[0] + c * v[2]],
            _ => [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]],
        }
    }
    fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| a[i] + b[i])
    }
    fn rotation(axis: usize, angle: f64, translation: [f64; 3]) -> RigidTransform {
        let mut q = [0., 0., 0., (angle / 2.).cos()];
        q[axis] = (angle / 2.).sin();
        RigidTransform {
            translation,
            rotation: q,
        }
    }
    let t = Tolerance::default();
    let mut p = parts();
    p.get_mut("p")
        .unwrap()
        .datums
        .insert("mount".into(), rotation(0, 0.31, [1.3, -2.7, 0.4]));
    p.get_mut("p")
        .unwrap()
        .datums
        .insert("origin".into(), rotation(1, 0.23, [-1.2, 3.4, 2.1]));
    let mut base = leaf("base");
    base.transform = rotation(2, 0.19, [20., -9., 4.]);
    let mut moving = leaf("moving");
    moving.transform = rotation(2, 0.17, [2., -1., 3.]);
    moving.attachment = Some(attachment(
        "base",
        "origin",
        PrescribedJoint::Revolute {
            position: -0.6,
            min: -1.,
            max: 1.,
        },
    ));
    let d = AssemblyDefinition {
        id: "d".into(),
        instances: vec![base, moving],
        assemblies: vec![],
    };
    let built =
        BuiltAssembly::from_definitions("d".into(), p, BTreeMap::from([("d".into(), d)]), t)
            .unwrap();
    let actual = built.instances.iter().find(|i| i.id == "moving").unwrap();
    for point in [[0.; 3], [1., 0., 0.], [0., 2., -3.]] {
        let in_part = add(rotate(point, 0, 0.31), [1.3, -2.7, 0.4]);
        let expected = add(
            rotate(
                add(
                    rotate(
                        add(rotate(rotate(point, 2, -0.6), 2, 0.17), [2., -1., 3.]),
                        1,
                        0.23,
                    ),
                    [-1.2, 3.4, 2.1],
                ),
                2,
                0.19,
            ),
            [20., -9., 4.],
        );
        near(actual.transform.point(in_part), expected);
    }
    let reports = built.joint_reports(t).unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].residual_translation_norm_mm > 0. || reports[0].residual_angle_rad > 0.);
    let encoded = serde_json::to_string(&built).unwrap();
    let decoded: BuiltAssembly = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        decoded.hierarchy, built.hierarchy,
        "source hierarchy float roundtrip"
    );
    assert_eq!(
        decoded.instances, built.instances,
        "derived placement float roundtrip"
    );
    decoded.validate(t).unwrap();
}

#[test]
fn joint_count_is_preflighted_and_limit_intent_changes_assembly_identity_only() {
    let t = Tolerance::default();
    let mut d = chain();
    let first = BuiltAssembly::from_definitions(
        "chain".into(),
        parts(),
        BTreeMap::from([("chain".into(), d.clone())]),
        t,
    )
    .unwrap();
    d.instances[0].attachment.as_mut().unwrap().joint = Some(PrescribedJoint::Prismatic {
        position: 5.,
        min: -1.,
        max: 11.,
    });
    let second = BuiltAssembly::from_definitions(
        "chain".into(),
        parts(),
        BTreeMap::from([("chain".into(), d)]),
        t,
    )
    .unwrap();
    assert_ne!(first.content_hash, second.content_hash);
    assert_eq!(first.instances, second.instances);
    assert_eq!(
        first.parts["p"].content_hash,
        second.parts["p"].content_hash
    );
    let mut leaves = vec![leaf("base")];
    for i in 0..=MAX_PRESCRIBED_JOINTS {
        let mut item = leaf(&format!("joint{i}"));
        item.attachment = Some(attachment(
            "base",
            "origin",
            PrescribedJoint::Prismatic {
                position: 0.,
                min: 0.,
                max: 0.,
            },
        ));
        leaves.push(item);
    }
    let d = AssemblyDefinition {
        id: "too_many".into(),
        instances: leaves,
        assemblies: vec![],
    };
    assert!(matches!(
        resolve_hierarchy_placements(&parts(), &BTreeMap::from([("too_many".into(), d)]), t),
        Err(AssemblyError::ResourceLimit)
    ));
}
