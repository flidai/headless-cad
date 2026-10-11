//! Revision-bound engineering obligations, independent of display geometry.
use super::*;
pub const MAX_PROPERTY_ASSERTIONS: usize = 1000;
/// Aggregate native topology and occurrence visits charged before property queries.
pub const MAX_PROPERTY_ASSERTION_WORK: usize = 10_000_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineeringProperty {
    #[serde(rename = "volume_mm3")]
    VolumeMm3,
    #[serde(rename = "mass_kg")]
    MassKg,
    #[serde(rename = "volume_centroid_x_mm")]
    VolumeCentroidXMm,
    #[serde(rename = "volume_centroid_y_mm")]
    VolumeCentroidYMm,
    #[serde(rename = "volume_centroid_z_mm")]
    VolumeCentroidZMm,
    #[serde(rename = "mass_centroid_x_mm")]
    MassCentroidXMm,
    #[serde(rename = "mass_centroid_y_mm")]
    MassCentroidYMm,
    #[serde(rename = "mass_centroid_z_mm")]
    MassCentroidZMm,
    #[serde(rename = "volume_inertia_xx_mm5")]
    VolumeInertiaXxMm5,
    #[serde(rename = "volume_inertia_yy_mm5")]
    VolumeInertiaYyMm5,
    #[serde(rename = "volume_inertia_zz_mm5")]
    VolumeInertiaZzMm5,
    #[serde(rename = "volume_inertia_xy_mm5")]
    VolumeInertiaXyMm5,
    #[serde(rename = "volume_inertia_xz_mm5")]
    VolumeInertiaXzMm5,
    #[serde(rename = "volume_inertia_yz_mm5")]
    VolumeInertiaYzMm5,
    #[serde(rename = "mass_inertia_xx_kg_mm2")]
    MassInertiaXxKgMm2,
    #[serde(rename = "mass_inertia_yy_kg_mm2")]
    MassInertiaYyKgMm2,
    #[serde(rename = "mass_inertia_zz_kg_mm2")]
    MassInertiaZzKgMm2,
    #[serde(rename = "mass_inertia_xy_kg_mm2")]
    MassInertiaXyKgMm2,
    #[serde(rename = "mass_inertia_xz_kg_mm2")]
    MassInertiaXzKgMm2,
    #[serde(rename = "mass_inertia_yz_kg_mm2")]
    MassInertiaYzKgMm2,
}
impl EngineeringProperty {
    fn is_mass(self) -> bool {
        matches!(
            self,
            Self::MassKg
                | Self::MassCentroidXMm
                | Self::MassCentroidYMm
                | Self::MassCentroidZMm
                | Self::MassInertiaXxKgMm2
                | Self::MassInertiaYyKgMm2
                | Self::MassInertiaZzKgMm2
                | Self::MassInertiaXyKgMm2
                | Self::MassInertiaXzKgMm2
                | Self::MassInertiaYzKgMm2
        )
    }
    fn value(self, p: &cad_assembly::AssemblyMassProperties) -> Option<f64> {
        match self {
            Self::VolumeMm3 => Some(p.volume),
            Self::MassKg => p.mass_kg,
            Self::VolumeCentroidXMm => p.volume_centroid.map(|v| v[0]),
            Self::VolumeCentroidYMm => p.volume_centroid.map(|v| v[1]),
            Self::VolumeCentroidZMm => p.volume_centroid.map(|v| v[2]),
            Self::MassCentroidXMm => p.center_of_mass.map(|v| v[0]),
            Self::MassCentroidYMm => p.center_of_mass.map(|v| v[1]),
            Self::MassCentroidZMm => p.center_of_mass.map(|v| v[2]),
            Self::VolumeInertiaXxMm5 => p.central_volume_inertia.map(|v| v[0][0]),
            Self::VolumeInertiaYyMm5 => p.central_volume_inertia.map(|v| v[1][1]),
            Self::VolumeInertiaZzMm5 => p.central_volume_inertia.map(|v| v[2][2]),
            Self::VolumeInertiaXyMm5 => p.central_volume_inertia.map(|v| v[0][1]),
            Self::VolumeInertiaXzMm5 => p.central_volume_inertia.map(|v| v[0][2]),
            Self::VolumeInertiaYzMm5 => p.central_volume_inertia.map(|v| v[1][2]),
            Self::MassInertiaXxKgMm2 => p.central_mass_inertia.map(|v| v[0][0]),
            Self::MassInertiaYyKgMm2 => p.central_mass_inertia.map(|v| v[1][1]),
            Self::MassInertiaZzKgMm2 => p.central_mass_inertia.map(|v| v[2][2]),
            Self::MassInertiaXyKgMm2 => p.central_mass_inertia.map(|v| v[0][1]),
            Self::MassInertiaXzKgMm2 => p.central_mass_inertia.map(|v| v[0][2]),
            Self::MassInertiaYzKgMm2 => p.central_mass_inertia.map(|v| v[1][2]),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyAssertion {
    pub id: String,
    pub output: String,
    pub property: EngineeringProperty,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyAssertionStatus {
    Passed,
    OutOfRange,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyAssertionReport {
    pub output: String,
    pub property: EngineeringProperty,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub actual: Option<f64>,
    pub status: PropertyAssertionStatus,
    pub target_content_hash: String,
    pub message: Option<String>,
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version == 1 && !recipe.property_assertions.is_empty() {
        return Err(BuildError::PropertyAssertionSchema);
    }
    if recipe.property_assertions.len() > MAX_PROPERTY_ASSERTIONS {
        return Err(BuildError::ResourceLimit {
            resource: "property assertions",
            limit: MAX_PROPERTY_ASSERTIONS,
        });
    }
    let mut ids = BTreeSet::new();
    let bodies: BTreeMap<_, _> = recipe
        .outputs
        .iter()
        .map(|o| (o.name.as_str(), o.feature.as_str()))
        .collect();
    let features: BTreeMap<_, _> = recipe.features.iter().map(|f| (f.id(), f)).collect();
    let assemblies: BTreeSet<_> = recipe
        .assembly_outputs
        .iter()
        .map(|o| o.name.as_str())
        .collect();
    for a in &recipe.property_assertions {
        identifier("property assertion", &a.id)?;
        identifier("property assertion output", &a.output)?;
        let fail = |message: &str| BuildError::InvalidPropertyAssertion {
            id: a.id.clone(),
            message: message.into(),
        };
        if !ids.insert(&a.id) {
            return Err(fail("duplicate assertion ID"));
        }
        if a.min.is_none() && a.max.is_none() {
            return Err(fail("at least one explicit finite range bound is required"));
        }
        if a.min.into_iter().chain(a.max).any(|v| !v.is_finite())
            || a.min.zip(a.max).is_some_and(|(min, max)| min > max)
        {
            return Err(fail("bounds must be finite and min <= max"));
        }
        if let Some(feature) = bodies.get(a.output.as_str()) {
            if !features.get(feature).is_some_and(|f| f.is_solid()) {
                return Err(fail("target body output must reference an existing solid"));
            }
        } else if !assemblies.contains(a.output.as_str()) {
            return Err(fail("target output does not exist"));
        }
    }
    Ok(())
}
fn geometry_work(m: &cad_kernel::ModelSnapshot) -> usize {
    m.vertices
        .len()
        .saturating_add(m.edges.len())
        .saturating_add(m.faces.len())
        .saturating_add(m.loops.iter().map(|l| l.edges.len()).sum::<usize>())
}
pub(super) fn evaluate(
    recipe: &Recipe,
    hashes: &BTreeMap<String, String>,
    models: &HashMap<String, Arc<OperationResult>>,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, PropertyAssertionReport>, BuildError> {
    let bodies: BTreeMap<_, _> = recipe
        .outputs
        .iter()
        .map(|o| (o.name.as_str(), o.feature.as_str()))
        .collect();
    let targets: BTreeSet<_> = recipe
        .property_assertions
        .iter()
        .map(|a| a.output.as_str())
        .collect();
    // Reserve aggregate work for every distinct target before any query runs.
    let mut work = 0usize;
    for target in &targets {
        let charge = if let Some(feature) = bodies.get(target) {
            geometry_work(&models[*feature].model)
        } else {
            let a = &assemblies[*target];
            a.instances.iter().fold(a.instances.len(), |n, i| {
                n.saturating_add(geometry_work(&a.parts[&i.part].result.model))
            })
        };
        work = work.saturating_add(charge);
        if work > MAX_PROPERTY_ASSERTION_WORK {
            return Err(BuildError::ResourceLimit {
                resource: "property assertion geometry work",
                limit: MAX_PROPERTY_ASSERTION_WORK,
            });
        }
    }
    let mut values = BTreeMap::new();
    for target in targets {
        let (hash, properties) = if let Some(feature) = bodies.get(target) {
            let properties = models[*feature]
                .model
                .mass_properties(tolerance)
                .map(|p| cad_assembly::AssemblyMassProperties {
                    volume: p.volume,
                    volume_centroid: p.centroid,
                    central_volume_inertia: p.central_inertia,
                    mass_kg: None,
                    center_of_mass: None,
                    central_mass_inertia: None,
                    missing_density_occurrences: 0,
                })
                .map_err(|e| e.to_string());
            (hashes[*feature].clone(), properties)
        } else {
            let a = &assemblies[target];
            (
                a.content_hash.clone(),
                a.mass_properties(tolerance).map_err(|e| e.to_string()),
            )
        };
        values.insert(target, (hash, properties));
    }
    let mut reports = BTreeMap::new();
    for a in &recipe.property_assertions {
        let (hash, properties) = &values[a.output.as_str()];
        let actual = properties
            .as_ref()
            .ok()
            .and_then(|p| a.property.value(p))
            .filter(|x| x.is_finite());
        let status = match actual {
            None => PropertyAssertionStatus::Unavailable,
            Some(v) if a.min.is_some_and(|min| v < min) || a.max.is_some_and(|max| v > max) => {
                PropertyAssertionStatus::OutOfRange
            }
            _ => PropertyAssertionStatus::Passed,
        };
        let message=match status{
            PropertyAssertionStatus::Passed=>None,
            PropertyAssertionStatus::OutOfRange=>Some("value is outside the explicit closed bounds".into()),
            PropertyAssertionStatus::Unavailable=>Some(match properties {
                Err(error)=>error.clone(),
                Ok(_) if a.property.is_mass() && bodies.contains_key(a.output.as_str())=>"body output has no material density; mass properties require a material-bearing assembly output".into(),
                Ok(p) if a.property.is_mass() && p.missing_density_occurrences>0=>format!("mass property unavailable: {} occurrence(s) lack material density",p.missing_density_occurrences),
                Ok(_) if a.property.is_mass()=>"mass centroid or central inertia is undefined for zero total mass".into(),
                Ok(_)=>"volume centroid or central inertia is undefined for empty geometry".into(),
            }),
        };
        let report = PropertyAssertionReport {
            output: a.output.clone(),
            property: a.property,
            min: a.min,
            max: a.max,
            actual,
            status,
            target_content_hash: hash.clone(),
            message,
        };
        if status != PropertyAssertionStatus::Passed {
            return Err(BuildError::PropertyAssertionFailed {
                assertion_id: a.id.clone(),
                report: Box::new(report),
            });
        }
        reports.insert(a.id.clone(), report);
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Recipe {
        serde_json::from_value(serde_json::json!({
        "schema_version":2,"units":"mm","features":[
          {"op":"rectangle","id":"p","width":2,"depth":3},{"op":"extrude","id":"b","profile":"p","height":4},
          {"op":"rectangle","id":"other_p","width":7,"depth":8},{"op":"extrude","id":"other","profile":"other_p","height":9}],
        "outputs":[{"name":"box","feature":"b"},{"name":"other","feature":"other"}],
        "parts":[{"id":"light","feature":"b","metadata":{"material":{"name":"light","density_kg_per_m3":1000}}},
          {"id":"heavy","feature":"b","metadata":{"material":{"name":"heavy","density_kg_per_m3":3000}}}],
        "assemblies":[{"id":"pair","instances":[{"id":"a","part":"light"},{"id":"b","part":"heavy","transform":{"translation":[10,0,0]}}]}],
        "assembly_outputs":[{"name":"pair","assembly":"pair"}]
    })).unwrap()
    }
    fn assertion(
        id: &str,
        output: &str,
        property: EngineeringProperty,
        value: f64,
    ) -> PropertyAssertion {
        PropertyAssertion {
            id: id.into(),
            output: output.into(),
            property,
            min: Some(value - 1e-10),
            max: Some(value + 1e-10),
        }
    }
    #[test]
    fn native_body_and_mixed_density_moments_have_revision_bound_reports() {
        let mut recipe = fixture();
        recipe.property_assertions = vec![
            assertion("body_volume", "box", EngineeringProperty::VolumeMm3, 24.),
            assertion(
                "body_center",
                "box",
                EngineeringProperty::VolumeCentroidZMm,
                2.,
            ),
            assertion(
                "body_ixx",
                "box",
                EngineeringProperty::VolumeInertiaXxMm5,
                50.,
            ),
            assertion("pair_volume", "pair", EngineeringProperty::VolumeMm3, 48.),
            assertion("pair_mass", "pair", EngineeringProperty::MassKg, 96e-6),
            assertion(
                "pair_center",
                "pair",
                EngineeringProperty::MassCentroidXMm,
                8.5,
            ),
            assertion(
                "pair_iyy",
                "pair",
                EngineeringProperty::MassInertiaYyKgMm2,
                0.00196,
            ),
        ];
        let result = build(&recipe).unwrap();
        assert_eq!(result.property_assertions.len(), 7);
        for (id, report) in &result.property_assertions {
            assert_eq!(report.status, PropertyAssertionStatus::Passed);
            assert!(report.actual.is_some());
            assert_eq!(
                report.target_content_hash,
                if id.starts_with("body") {
                    &result.outputs["box"].content_hash
                } else {
                    &result.assemblies["pair"].content_hash
                }
                .to_string()
            );
        }
    }
    #[test]
    fn selected_build_enforces_unpublished_targets_and_cached_range_edits() {
        let mut recipe = fixture();
        recipe.property_assertions = vec![
            assertion("mass", "pair", EngineeringProperty::MassKg, 96e-6),
            assertion("volume", "box", EngineeringProperty::VolumeMm3, 24.),
        ];
        let mut engine = BuildEngine::default();
        let selected = engine.build_outputs(&recipe, &["other".into()]).unwrap();
        assert_eq!(selected.outputs.len(), 1);
        assert!(selected.assemblies.is_empty());
        assert_eq!(selected.scope.requested_outputs, vec!["other"]);
        assert!(selected.scope.validation_roots.contains(&"b".into()));
        assert_eq!(selected.property_assertions.len(), 2);
        recipe.property_assertions[0].max = Some(1e-6);
        recipe.property_assertions[0].min = None;
        let BuildError::PropertyAssertionFailed {
            assertion_id,
            report,
        } = engine
            .build_outputs(&recipe, &["other".into()])
            .unwrap_err()
        else {
            panic!()
        };
        assert_eq!(assertion_id, "mass");
        assert_eq!(report.status, PropertyAssertionStatus::OutOfRange);
        assert_eq!(
            report.target_content_hash,
            selected.property_assertions["mass"].target_content_hash
        );
        assert!((report.actual.unwrap() - 96e-6).abs() < 1e-15);
    }
    #[test]
    fn unavailable_metrics_fail_instead_of_substituting_zero() {
        let mut recipe = fixture();
        recipe.property_assertions = vec![PropertyAssertion {
            id: "mass".into(),
            output: "box".into(),
            property: EngineeringProperty::MassKg,
            min: Some(0.),
            max: None,
        }];
        let BuildError::PropertyAssertionFailed { report, .. } = build(&recipe).unwrap_err() else {
            panic!()
        };
        assert_eq!(report.status, PropertyAssertionStatus::Unavailable);
        assert_eq!(report.actual, None);
        recipe.property_assertions[0].output = "pair".into();
        recipe.parts[0].metadata.material = None;
        let BuildError::PropertyAssertionFailed { report, .. } = build(&recipe).unwrap_err() else {
            panic!()
        };
        assert_eq!(report.status, PropertyAssertionStatus::Unavailable);
        recipe.assemblies[0].instances.clear();
        recipe.property_assertions[0].property = EngineeringProperty::VolumeCentroidXMm;
        let BuildError::PropertyAssertionFailed { report, .. } = build(&recipe).unwrap_err() else {
            panic!()
        };
        assert_eq!(report.status, PropertyAssertionStatus::Unavailable);
        recipe.property_assertions[0].property = EngineeringProperty::VolumeMm3;
        recipe.property_assertions[0].max = Some(0.);
        assert_eq!(
            build(&recipe).unwrap().property_assertions["mass"].actual,
            Some(0.)
        );
    }
    #[test]
    fn every_scalar_metric_matches_independent_two_cube_moments() {
        let mut r = fixture();
        if let Feature::Rectangle { width, depth, .. } = &mut r.features[0] {
            *width = 1.;
            *depth = 1.;
        }
        if let Feature::Extrude { height, .. } = &mut r.features[1] {
            *height = 1.;
        }
        r.assemblies[0].instances[1].transform.translation = [2., 4., 6.];
        use EngineeringProperty::*;
        let expected: [(EngineeringProperty, f64); 20] = [
            (VolumeMm3, 2.),
            (VolumeCentroidXMm, 1.5),
            (VolumeCentroidYMm, 2.5),
            (VolumeCentroidZMm, 3.5),
            (VolumeInertiaXxMm5, 79. / 3.),
            (VolumeInertiaYyMm5, 61. / 3.),
            (VolumeInertiaZzMm5, 31. / 3.),
            (VolumeInertiaXyMm5, -4.),
            (VolumeInertiaXzMm5, -6.),
            (VolumeInertiaYzMm5, -12.),
            (MassKg, 4e-6),
            (MassCentroidXMm, 2.),
            (MassCentroidYMm, 3.5),
            (MassCentroidZMm, 5.),
            (MassInertiaXxKgMm2, 119e-6 / 3.),
            (MassInertiaYyKgMm2, 92e-6 / 3.),
            (MassInertiaZzKgMm2, 47e-6 / 3.),
            (MassInertiaXyKgMm2, -6e-6),
            (MassInertiaXzKgMm2, -9e-6),
            (MassInertiaYzKgMm2, -18e-6),
        ];
        r.property_assertions = expected
            .iter()
            .enumerate()
            .map(|(i, (property, value))| PropertyAssertion {
                id: format!("metric_{i}"),
                output: "pair".into(),
                property: *property,
                min: Some(value - value.abs() * 1e-12),
                max: Some(value + value.abs() * 1e-12),
            })
            .collect();
        let result = build(&r).unwrap();
        assert_eq!(result.property_assertions.len(), 20);
        for (i, (_, value)) in expected.into_iter().enumerate() {
            let actual = result.property_assertions[&format!("metric_{i}")]
                .actual
                .unwrap();
            assert!((actual - value).abs() < value.abs() * 1e-12);
        }
    }
    #[test]
    fn aggregate_property_work_is_reserved_before_mass_queries() {
        let mut r = fixture();
        r.features[0] = Feature::Polygon {
            id: "p".into(),
            points: (0..10)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 10.;
                    [10. * a.cos(), 10. * a.sin()]
                })
                .collect(),
            holes: vec![],
            circular_holes: vec![],
            op_version: 1,
        };
        r.assemblies[0].instances = (0..1000)
            .map(|i| Instance {
                id: format!("i{i}"),
                part: "light".into(),
                transform: Default::default(),
                attachment: None,
            })
            .collect();
        r.assembly_outputs = (0..100)
            .map(|i| AssemblyOutput {
                name: format!("pair_{i}"),
                assembly: "pair".into(),
            })
            .collect();
        r.property_assertions = r
            .assembly_outputs
            .iter()
            .map(|o| PropertyAssertion {
                id: o.name.clone(),
                output: o.name.clone(),
                property: EngineeringProperty::VolumeMm3,
                min: Some(0.),
                max: None,
            })
            .collect();
        assert!(matches!(
            BuildEngine::default().build_outputs(&r, &["other".into()]),
            Err(BuildError::ResourceLimit {
                resource: "property assertion geometry work",
                ..
            })
        ));
    }
    #[test]
    fn schema_ranges_output_types_and_limits_fail_before_geometry() {
        let base = fixture();
        let a = assertion("volume", "box", EngineeringProperty::VolumeMm3, 24.);
        for mutation in 0..8 {
            let mut recipe = base.clone();
            recipe.property_assertions = vec![a.clone()];
            match mutation {
                0 => recipe.schema_version = 1,
                1 => recipe.property_assertions.push(a.clone()),
                2 => recipe.property_assertions[0].output = "missing".into(),
                3 => recipe.outputs[0].feature = "p".into(),
                4 => {
                    recipe.property_assertions[0].min = None;
                    recipe.property_assertions[0].max = None;
                }
                5 => recipe.property_assertions[0].min = Some(f64::NAN),
                6 => recipe.property_assertions[0].min = Some(100.),
                _ => recipe.property_assertions = vec![a.clone(); 1001],
            }
            if let Feature::Extrude { height, .. } = &mut recipe.features[1] {
                *height = 1e308;
            }
            let error = build(&recipe).unwrap_err();
            assert!(!matches!(error, BuildError::Kernel { .. }), "{error}");
        }
        assert!(serde_json::from_value::<Recipe>(serde_json::json!({"schema_version":1,"units":"mm","features":[],"property_assertions":[]})).is_err());
        assert!(serde_json::from_value::<PropertyAssertion>(
            serde_json::json!({"id":"a","output":"b","property":"display_volume","min":0})
        )
        .is_err());
    }
}
