use super::*;
fn source() -> PlanarMechanism {
    super::mechanism_tests::four_bar(0.5, true)
}
fn target(angle: f64) -> BTreeMap<String, f64> {
    BTreeMap::from([("motor".into(), angle)])
}
fn study() -> MotionStudy {
    MotionStudy {
        targets: vec![target(0.5), target(1.3), target(0.5)],
        max_drive_step_radians: 0.08,
        max_pin_step_mm: 2.,
    }
}
#[test]
fn deterministic_forward_reverse_continuation_preserves_branch_and_bounds() {
    let input = source();
    let config = study();
    let t = Tolerance::default();
    let mut budget = 2048;
    let report = continue_planar_mechanism(&input, &config, t, &mut budget).unwrap();
    assert_eq!(budget, 2048 - report.attempted_solves);
    assert_eq!(report.completed_requested_frames, 3);
    assert_eq!(report.accepted_steps, report.frames.len() - 1);
    assert!(report.frames.len() > 20);
    assert!(report.subdivisions > 0);
    assert!(report.attempted_solves >= report.frames.len());
    assert_eq!(
        report
            .frames
            .iter()
            .filter_map(|f| f.requested_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    for (index, frame) in report.frames.iter().enumerate() {
        assert_eq!(frame.index, index);
        assert_eq!(frame.status, cad_sketch::SolveStatus::FullyConstrained);
        assert_eq!(frame.degrees_of_freedom, 0);
        assert!(frame.max_residual_mm <= t.linear);
        assert!(
            frame.branches[0].signed_double_area_mm2 > frame.branches[0].uncertainty_margin_mm2
        );
        let angle = frame.coordinates["motor"];
        assert!((frame.pins["B"][0] - 30. * angle.cos()).abs() < 1e-6);
        assert!((frame.pins["B"][1] - 30. * angle.sin()).abs() < 1e-6);
        if index > 0 {
            let previous = &report.frames[index - 1];
            let drive = (angle - previous.coordinates["motor"]).abs();
            let pin = frame
                .pins
                .iter()
                .map(|(id, p)| {
                    let q = previous.pins[id];
                    (p[0] - q[0]).hypot(p[1] - q[1])
                })
                .fold(0., f64::max);
            assert_eq!(drive, frame.max_drive_step_radians);
            assert_eq!(pin, frame.max_pin_step_mm);
            assert!(drive <= config.max_drive_step_radians);
            assert!(pin <= config.max_pin_step_mm);
        }
    }
    let replay = continue_planar_mechanism(&input, &config, t, &mut 2048).unwrap();
    assert_eq!(report, replay);
    let pin_limited = MotionStudy {
        max_drive_step_radians: 0.5,
        max_pin_step_mm: 1.,
        ..config
    };
    let mut budget = 2048;
    let refined = continue_planar_mechanism(&input, &pin_limited, t, &mut budget).unwrap();
    assert!(
        refined.attempted_solves > refined.frames.len(),
        "rejected coarse pin steps must consume attempts"
    );
    assert_eq!(budget, 2048 - refined.attempted_solves);
    assert!(refined.frames.iter().all(|f| f.max_pin_step_mm <= 1.));
}
#[test]
fn adaptive_failures_preserve_prefix_and_budget_diagnostics() {
    let input = source();
    let config = study();
    let mut budget = 4;
    let error =
        continue_planar_mechanism(&input, &config, Tolerance::default(), &mut budget).unwrap_err();
    let failure = error.failure_report().unwrap();
    assert_eq!(budget, 0);
    assert_eq!(failure.accepted.attempted_solves, 4);
    assert!(!failure.accepted.frames.is_empty());
    assert!(failure.accepted.completed_requested_frames < 3);
    assert!(failure.message.contains("budget"));
    let mut singular = input.clone();
    singular.branches[0].side = MechanismBranchSide::Right;
    let error =
        continue_planar_mechanism(&singular, &config, Tolerance::default(), &mut 100).unwrap_err();
    let failure = error.failure_report().unwrap();
    assert!(failure.accepted.frames.is_empty());
    assert!(failure.solver_report.is_some());
    assert_eq!(failure.failed_coordinates, target(0.5));
    let mut tiny = config.clone();
    tiny.max_drive_step_radians = 1e-12;
    let error =
        continue_planar_mechanism(&input, &tiny, Tolerance::default(), &mut 100).unwrap_err();
    let failure = error.failure_report().unwrap();
    assert!(failure.message.contains("depth"));
    assert_eq!(failure.accepted.attempted_solves, 1);
    assert_eq!(failure.accepted.frames.len(), 1);
}
#[test]
fn strict_preflight_does_not_consume_budget_and_shared_budget_is_aggregate() {
    let input = source();
    for modify in 0..4 {
        let mut config = study();
        match modify {
            0 => config.targets[0] = target(0.6),
            1 => {
                config.targets[1].clear();
            }
            2 => config.max_pin_step_mm = f64::NAN,
            _ => config.targets.truncate(1),
        };
        let mut budget = 100;
        assert!(
            continue_planar_mechanism(&input, &config, Tolerance::default(), &mut budget).is_err()
        );
        assert_eq!(budget, 100);
    }
    let config = MotionStudy {
        targets: vec![target(0.5), target(0.51)],
        max_drive_step_radians: 0.1,
        max_pin_step_mm: 100.,
    };
    let mut budget = 3;
    let report =
        continue_planar_mechanism(&input, &config, Tolerance::default(), &mut budget).unwrap();
    assert_eq!(report.attempted_solves, 2);
    assert_eq!(budget, 1);
    let error =
        continue_planar_mechanism(&input, &config, Tolerance::default(), &mut budget).unwrap_err();
    assert_eq!(budget, 0);
    assert_eq!(error.failure_report().unwrap().accepted.attempted_solves, 1);
    let mut value = serde_json::to_value(config).unwrap();
    value["dynamics"] = serde_json::json!(true);
    assert!(serde_json::from_value::<MotionStudy>(value).is_err());
}
