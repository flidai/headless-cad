use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/motion_four_bar.star");
fn recipe(length: f64, high: f64, step: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_modules(
        "motion_four_bar.star",
        SOURCE,
        &BTreeMap::from([
            ("coupler_length".into(), length),
            ("high_angle".into(), high),
            ("max_pin_step".into(), step),
        ]),
        &BTreeMap::from([(
            "solved_four_bar.star".into(),
            include_str!("../../../examples/solved_four_bar.star").into(),
        )]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}")
}
fn pins(angle: f64, length: f64) -> BTreeMap<String, [f64; 2]> {
    let b = [30. * angle.cos(), 30. * angle.sin()];
    let delta = [80. - b[0], -b[1]];
    let d = delta[0].hypot(delta[1]);
    let u = [delta[0] / d, delta[1] / d];
    let along = (length * length - 2500. + d * d) / (2. * d);
    let h = (length * length - along * along).sqrt();
    BTreeMap::from([
        ("a".into(), [0., 0.]),
        ("b".into(), b),
        (
            "c".into(),
            [
                b[0] + along * u[0] - h * u[1],
                b[1] + along * u[1] + h * u[0],
            ],
        ),
        ("d".into(), [80., 0.]),
    ])
}
#[test]
fn sampled_roundtrips_match_independent_closure_and_leave_nominal_geometry_unchanged() {
    let mut engine = cad_build::BuildEngine::default();
    for length in [65., 70., 75.] {
        let mut first = true;
        let mut hash = None;
        for high in [1.1, 1.3, 1.5] {
            for step in [0.8, 2., 4.] {
                let build = engine.build(&recipe(length, high, step)).unwrap();
                if !first {
                    assert_eq!(build.stats.computed_solids, 0)
                }
                first = false;
                let actual_hash = &build.assemblies["solved_four_bar"].content_hash;
                if let Some(old) = &hash {
                    assert_eq!(actual_hash, old)
                } else {
                    hash = Some(actual_hash.clone())
                }
                let report = &build.motion_studies["crank_cycle"].solution;
                assert_eq!(report.completed_requested_frames, 4);
                assert!(report.frames.len() > 4);
                assert!(report.attempted_solves <= 512);
                let requested: Vec<_> = report
                    .frames
                    .iter()
                    .filter_map(|f| f.requested_index.map(|i| (i, f.coordinates["input_angle"])))
                    .collect();
                assert_eq!(requested, vec![(0, 0.9), (1, high), (2, 0.5), (3, 0.9)]);
                for frame in &report.frames {
                    let expected = pins(frame.coordinates["input_angle"], length);
                    for (id, p) in &expected {
                        for (a, b) in frame.pins[id].into_iter().zip(p) {
                            near(a, *b)
                        }
                    }
                    assert_eq!(frame.degrees_of_freedom, 0);
                    assert_eq!(
                        serde_json::to_value(frame.status).unwrap(),
                        "fully_constrained"
                    );
                    assert!(frame.max_drive_step_radians <= 0.08);
                    assert!(frame.max_pin_step_mm <= step);
                    assert!(
                        frame.branches[0].signed_double_area_mm2
                            > frame.branches[0].uncertainty_margin_mm2
                    );
                }
                for pair in report.frames.windows(2) {
                    let angle_delta = (pair[1].coordinates["input_angle"]
                        - pair[0].coordinates["input_angle"])
                        .abs();
                    near(pair[1].max_drive_step_radians, angle_delta);
                    let max_pin = pair[1]
                        .pins
                        .iter()
                        .map(|(id, p)| {
                            let q = pair[0].pins[id];
                            (p[0] - q[0]).hypot(p[1] - q[1])
                        })
                        .fold(0., f64::max);
                    near(pair[1].max_pin_step_mm, max_pin);
                }
                let first_frame = &report.frames[0];
                let last = report.frames.last().unwrap();
                for (id, p) in &first_frame.pins {
                    for (a, b) in p.iter().zip(last.pins[id]) {
                        near(*a, b)
                    }
                }
            }
        }
    }
}
