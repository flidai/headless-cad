//! Publication obligations. Native builds validate declarations but do not claim
//! an exchange requirement passed: exporters and byte publication enforce it.
use super::*;

pub const MAX_EXCHANGE_REQUIREMENTS: usize = 256;
pub const EXCHANGE_ASPECTS: [&str; 19] = [
    "geometry",
    "units",
    "assembly_definitions",
    "assembly_hierarchy",
    "occurrence_paths",
    "occurrence_placements",
    "part_numbers",
    "part_descriptions",
    "part_revisions",
    "material_names",
    "material_density",
    "datums",
    "attachment_intent",
    "operation_face_history",
    "semantic_face_references",
    "modeling_tolerance",
    "manufacturing_pmi",
    "recipe_history",
    "source_revision",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExchangeRequirement {
    pub id: String,
    pub output: String,
    pub format: String,
    pub aspect: String,
}

impl ExchangeRequirement {
    /// Validate declaration fields without checking output availability or export.
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidExchangeRequirement {
            id: self.id.clone(),
            message: message.into(),
        };
        if self.id.is_empty()
            || self.id.trim() != self.id
            || self.output.is_empty()
            || self.output.trim() != self.output
        {
            return Err(fail(
                "id and output must be nonempty identifiers without surrounding whitespace",
            ));
        }
        if !matches!(self.format.as_str(), "step" | "stl") {
            return Err(fail("format must be step or stl"));
        }
        if !EXCHANGE_ASPECTS.contains(&self.aspect.as_str()) {
            return Err(fail("unknown exchange fidelity aspect"));
        }
        Ok(())
    }
}

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version == 1 && !recipe.exchange_requirements.is_empty() {
        return Err(BuildError::ExchangeRequirementSchema);
    }
    if recipe.exchange_requirements.len() > MAX_EXCHANGE_REQUIREMENTS {
        return Err(BuildError::ResourceLimit {
            resource: "exchange requirements",
            limit: MAX_EXCHANGE_REQUIREMENTS,
        });
    }
    let outputs: BTreeSet<_> = recipe
        .outputs
        .iter()
        .map(|o| &o.name)
        .chain(recipe.assembly_outputs.iter().map(|o| &o.name))
        .collect();
    let mut ids = BTreeSet::new();
    for requirement in &recipe.exchange_requirements {
        requirement.validate()?;
        let fail = |message: &str| BuildError::InvalidExchangeRequirement {
            id: requirement.id.clone(),
            message: message.into(),
        };
        if !ids.insert(&requirement.id) {
            return Err(fail("duplicate exchange requirement ID"));
        }
        if !outputs.contains(&requirement.output) {
            return Err(fail("target output does not exist"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe() -> Recipe {
        serde_json::from_value(serde_json::json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":2,"depth":3},{"op":"extrude","id":"b","profile":"p","height":4}],"outputs":[{"name":"body","feature":"b"}],"exchange_requirements":[{"id":"units","output":"body","format":"stl","aspect":"units"}]})).unwrap()
    }
    #[test]
    fn native_build_exposes_unsatisfied_publication_obligations_as_pending() {
        let recipe = recipe();
        let result = build(&recipe).unwrap();
        assert_eq!(
            result.pending_exchange_requirements,
            recipe.exchange_requirements
        );
        assert!(result.property_assertions.is_empty());
        // STL units cannot be preserved, but source validation/native geometry
        // intentionally do not masquerade as publication enforcement.
        assert_eq!(result.outputs["body"].result.model.volume(), 24.);
        let roundtrip: Recipe =
            serde_json::from_value(serde_json::to_value(&recipe).unwrap()).unwrap();
        assert_eq!(
            roundtrip.exchange_requirements,
            recipe.exchange_requirements
        );
    }
    #[test]
    fn strict_schema_presence_and_fields_are_checked() {
        let mut json = serde_json::to_value(recipe()).unwrap();
        json["schema_version"] = 1.into();
        json["exchange_requirements"] = serde_json::json!([]);
        assert!(serde_json::from_value::<Recipe>(json.clone()).is_err());
        json["schema_version"] = 2.into();
        json["exchange_requirements"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<Recipe>(json).is_err());
        let mut json = serde_json::to_value(recipe()).unwrap();
        json["exchange_requirements"][0]["unexpected"] = true.into();
        assert!(serde_json::from_value::<Recipe>(json).is_err());
        let mut r = recipe();
        r.schema_version = 1;
        assert!(matches!(
            build(&r),
            Err(BuildError::ExchangeRequirementSchema)
        ));
    }
    #[test]
    fn malformed_duplicates_and_unknown_outputs_fail_before_geometry() {
        for (field, value) in [
            ("id", ""),
            ("id", " spaced"),
            ("output", "unknown"),
            ("output", " body"),
            ("format", "obj"),
            ("format", "STEP"),
            ("aspect", "unknown"),
        ] {
            let mut json = serde_json::to_value(recipe()).unwrap();
            json["exchange_requirements"][0][field] = value.into();
            let r: Recipe = serde_json::from_value(json).unwrap();
            assert!(
                matches!(
                    build(&r),
                    Err(BuildError::InvalidExchangeRequirement { .. })
                ),
                "{field}:{value}"
            );
        }
        let mut r = recipe();
        r.exchange_requirements
            .push(r.exchange_requirements[0].clone());
        assert!(matches!(
            build(&r),
            Err(BuildError::InvalidExchangeRequirement { .. })
        ));
        r.exchange_requirements = (0..MAX_EXCHANGE_REQUIREMENTS)
            .map(|i| ExchangeRequirement {
                id: i.to_string(),
                ..r.exchange_requirements[0].clone()
            })
            .collect();
        assert!(preflight(&r).is_ok());
        r.exchange_requirements.push(ExchangeRequirement {
            id: "overflow".into(),
            ..r.exchange_requirements[0].clone()
        });
        assert!(matches!(
            build(&r),
            Err(BuildError::ResourceLimit {
                resource: "exchange requirements",
                limit: 256
            })
        ));
    }
    #[test]
    fn assembly_output_names_are_valid_targets() {
        let mut r = recipe();
        r.assembly_outputs.push(AssemblyOutput {
            name: "assembled".into(),
            assembly: "root".into(),
        });
        r.exchange_requirements[0].output = "assembled".into();
        assert!(preflight(&r).is_ok());
        for aspect in EXCHANGE_ASPECTS {
            r.exchange_requirements[0].aspect = aspect.into();
            assert!(preflight(&r).is_ok());
        }
    }
}
