//! Source-owned independent-interval linear stacks. This contract does not infer
//! geometric relationships, correlations, GD&T or manufactured conformance.
use super::*;
pub const MAX_TOLERANCE_STACKS: usize = 64;
pub const MAX_TOLERANCE_STACK_TERMS: usize = 32;
pub const MAX_TOLERANCE_STACK_TERMS_TOTAL: usize = 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToleranceStackTerm {
    pub view: String,
    pub dimension: String,
    pub coefficient: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToleranceStack {
    pub id: String,
    pub terms: Vec<ToleranceStackTerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}
fn object<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<T, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    if !v.is_object() {
        return Err(serde::de::Error::custom(
            "tolerance stack declarations and terms must be objects",
        ));
    }
    serde_json::from_value(v).map_err(serde::de::Error::custom)
}
impl<'de> Deserialize<'de> for ToleranceStackTerm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            view: String,
            dimension: String,
            coefficient: f64,
        }
        let r: Raw = object(d)?;
        Ok(Self {
            view: r.view,
            dimension: r.dimension,
            coefficient: r.coefficient,
        })
    }
}
impl<'de> Deserialize<'de> for ToleranceStack {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            id: String,
            terms: Vec<ToleranceStackTerm>,
            #[serde(default)]
            min: Option<f64>,
            #[serde(default)]
            max: Option<f64>,
        }
        let r: Raw = object(d)?;
        Ok(Self {
            id: r.id,
            terms: r.terms,
            min: r.min,
            max: r.max,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ToleranceStackTermReport {
    pub view: String,
    pub dimension: String,
    pub coefficient: f64,
    pub native_value_mm: f64,
    pub tolerance: DimensionToleranceReport,
    pub lower_limit_mm: f64,
    pub upper_limit_mm: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ToleranceStackReport {
    pub declaration: ToleranceStack,
    pub status: String,
    pub nominal_mm: f64,
    pub lower_limit_mm: f64,
    pub upper_limit_mm: f64,
    pub terms: Vec<ToleranceStackTermReport>,
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
impl ToleranceStack {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidToleranceStack {
            id: self.id.clone(),
            message: message.into(),
        };
        if !safe_id(&self.id) {
            return Err(fail(
                "ID requires 1..128 ASCII letters, digits, underscores or hyphens",
            ));
        }
        if self.terms.is_empty() || self.terms.len() > MAX_TOLERANCE_STACK_TERMS {
            return Err(fail("stack requires 1..32 terms"));
        }
        if self.min.is_none() && self.max.is_none()
            || [self.min, self.max]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite())
            || matches!((self.min,self.max),(Some(a),Some(b)) if a>b)
        {
            return Err(fail(
                "at least one finite bound required; min <= max; signed bounds allowed",
            ));
        }
        let mut refs = BTreeSet::new();
        for t in &self.terms {
            if !safe_id(&t.view)
                || !safe_id(&t.dimension)
                || !t.coefficient.is_finite()
                || t.coefficient == 0.
                || t.coefficient.abs() > 1e6
            {
                return Err(fail("terms require exact view/dimension IDs and finite nonzero coefficient of magnitude at most 1000000"));
            }
            if !refs.insert((&t.view, &t.dimension)) {
                return Err(fail("duplicate view/dimension pair in stack"));
            }
        }
        Ok(())
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version == 1 && !recipe.tolerance_stacks.is_empty() {
        return Err(BuildError::InvalidToleranceStack {
            id: String::new(),
            message: "tolerance stacks require schema version 2".into(),
        });
    }
    if recipe.tolerance_stacks.len() > MAX_TOLERANCE_STACKS {
        return Err(BuildError::ResourceLimit {
            resource: "tolerance stacks",
            limit: MAX_TOLERANCE_STACKS,
        });
    }
    let mut ids = BTreeSet::new();
    let mut count = 0;
    for s in &recipe.tolerance_stacks {
        s.validate()?;
        if !ids.insert(s.id.to_ascii_lowercase()) {
            return Err(BuildError::InvalidToleranceStack {
                id: s.id.clone(),
                message: "duplicate case-insensitive tolerance stack ID".into(),
            });
        }
        count += s.terms.len();
        if count > MAX_TOLERANCE_STACK_TERMS_TOTAL {
            return Err(BuildError::ResourceLimit {
                resource: "tolerance stack terms",
                limit: MAX_TOLERANCE_STACK_TERMS_TOTAL,
            });
        }
        for t in &s.terms {
            let view = recipe.section_views.iter().find(|v| v.id == t.view);
            let tolerance = view.and_then(|v| {
                v.dimensions
                    .iter()
                    .find(|d| d.id == t.dimension)
                    .and_then(|d| d.tolerance.as_ref())
                    .or_else(|| {
                        v.circle_dimensions
                            .iter()
                            .find(|d| d.id == t.dimension)
                            .and_then(|d| d.tolerance.as_ref())
                    })
            });
            if tolerance.is_none() {
                return Err(BuildError::InvalidToleranceStack {
                    id: s.id.clone(),
                    message: format!(
                        "term {}/{} requires an existing dimension with authored tolerance",
                        t.view, t.dimension
                    ),
                });
            }
        }
    }
    Ok(())
}
// Every operation encloses its real-valued result for the exact binary64 source
// operands. Never reconstruct an interval from already-rounded report limits.
fn interval(t: &DimensionToleranceReport, coefficient: f64) -> Result<[f64; 2], String> {
    let low = (t.nominal_mm + t.lower_deviation_mm).next_down();
    let high = (t.nominal_mm + t.upper_deviation_mm).next_up();
    let (a, b) = if coefficient > 0. {
        (low, high)
    } else {
        (high, low)
    };
    let result = [(a * coefficient).next_down(), (b * coefficient).next_up()];
    if [low, high]
        .into_iter()
        .chain(result)
        .any(|v| !v.is_finite())
        || result[0] > result[1]
    {
        return Err("stack arithmetic exceeds finite interval range".into());
    }
    Ok(result)
}
pub(super) fn evaluate(
    recipe: &Recipe,
    sections: &BTreeMap<String, SectionViewReport>,
) -> Result<BTreeMap<String, ToleranceStackReport>, BuildError> {
    let mut reports = BTreeMap::new();
    for s in &recipe.tolerance_stacks {
        let fail = |message: String, lower_limit_mm: Option<f64>, upper_limit_mm: Option<f64>| {
            BuildError::ToleranceStackFailed {
                id: s.id.clone(),
                message,
                lower_limit_mm,
                upper_limit_mm,
            }
        };
        let mut lower = 0.;
        let mut upper = 0.;
        let mut nominal = 0.;
        let mut terms = Vec::new();
        for t in &s.terms {
            let view = sections
                .get(&t.view)
                .ok_or_else(|| fail("referenced section report missing".into(), None, None))?;
            let resolved = view
                .dimensions
                .iter()
                .find(|d| d.declaration.id == t.dimension)
                .map(|d| (d.value_mm, d.tolerance.as_ref()))
                .or_else(|| {
                    view.circle_dimensions
                        .iter()
                        .find(|d| d.declaration.id == t.dimension)
                        .map(|d| (d.value_mm, d.tolerance.as_ref()))
                });
            let (native_value_mm, tolerance) = resolved
                .and_then(|(value, tol)| tol.map(|t| (value, t)))
                .ok_or_else(|| {
                    fail(
                        "referenced native dimension tolerance witness missing".into(),
                        None,
                        None,
                    )
                })?;
            let [lo, hi] = interval(tolerance, t.coefficient).map_err(|e| fail(e, None, None))?;
            let next_lower = (lower + lo).next_down();
            let next_upper = (upper + hi).next_up();
            let product = t.coefficient * tolerance.nominal_mm;
            let next_nominal = nominal + product;
            if [next_lower, next_upper, product, next_nominal]
                .iter()
                .any(|v| !v.is_finite())
            {
                return Err(fail(
                    "stack accumulation exceeds finite numeric range".into(),
                    None,
                    None,
                ));
            }
            lower = next_lower;
            upper = next_upper;
            nominal = next_nominal;
            terms.push(ToleranceStackTermReport {
                view: t.view.clone(),
                dimension: t.dimension.clone(),
                coefficient: t.coefficient,
                native_value_mm,
                tolerance: tolerance.clone(),
                lower_limit_mm: lo,
                upper_limit_mm: hi,
            });
        }
        if s.min.is_some_and(|v| lower < v) || s.max.is_some_and(|v| upper > v) {
            return Err(fail("outward worst-case interval exceeds required bounds; no modeling-tolerance slack applies".into(),Some(lower),Some(upper)));
        }
        reports.insert(
            s.id.clone(),
            ToleranceStackReport {
                declaration: s.clone(),
                status: "passed".into(),
                nominal_mm: nominal,
                lower_limit_mm: lower,
                upper_limit_mm: upper,
                terms,
            },
        );
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outward_arithmetic_uses_operands_not_rounded_report_limits() {
        let t = DimensionToleranceReport {
            nominal_mm: 10.,
            lower_deviation_mm: -0.125,
            upper_deviation_mm: 0.25,
            lower_limit_mm: 99.,
            upper_limit_mm: 100.,
        };
        let [lo, hi] = interval(&t, -2.).unwrap();
        assert!(lo < -20.5 && hi > -19.75);
        assert!(lo > -20.500000001 && hi < -19.749999999);
        let tiny = DimensionToleranceReport {
            nominal_mm: 1e-300,
            lower_deviation_mm: 0.,
            upper_deviation_mm: 1e-302,
            lower_limit_mm: 0.,
            upper_limit_mm: 0.,
        };
        let [lo, hi] = interval(&tiny, 1e-300).unwrap();
        assert!(lo < 0. && hi > 0.);
    }
    #[test]
    fn arithmetic_overflow_is_not_published_as_infinite_clearance() {
        let t = DimensionToleranceReport {
            nominal_mm: 1e308,
            lower_deviation_mm: 0.,
            upper_deviation_mm: 1e300,
            lower_limit_mm: 0.,
            upper_limit_mm: 0.,
        };
        assert!(interval(&t, 1e6).is_err());
    }
}
