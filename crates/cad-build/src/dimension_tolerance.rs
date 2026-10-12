//! Authored manufacturing limits on nominal design dimensions, not inspection
//! results for manufactured parts. Geometry must match nominal independently.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DimensionTolerance {
    pub nominal: f64,
    pub lower: f64,
    pub upper: f64,
}

impl<'de> Deserialize<'de> for DimensionTolerance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            nominal: f64,
            lower: f64,
            upper: f64,
        }
        let value = serde_json::Value::deserialize(deserializer)?;
        if !value.is_object() {
            return Err(serde::de::Error::custom(
                "dimensional tolerance must be an object",
            ));
        }
        let raw: Raw = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            nominal: raw.nominal,
            lower: raw.lower,
            upper: raw.upper,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DimensionToleranceReport {
    pub nominal_mm: f64,
    pub lower_deviation_mm: f64,
    pub upper_deviation_mm: f64,
    pub lower_limit_mm: f64,
    pub upper_limit_mm: f64,
}

impl DimensionTolerance {
    pub fn validate(&self) -> Result<(), String> {
        let lower = self.nominal + self.lower;
        let upper = self.nominal + self.upper;
        let width = self.upper - self.lower;
        let scale = [self.nominal, self.lower, self.upper, lower, upper]
            .into_iter()
            .map(f64::abs)
            .fold(0., f64::max);
        let resolution = scale * (128. * f64::EPSILON);
        if !self.nominal.is_finite()
            || self.nominal <= 0.
            || !self.lower.is_finite()
            || !self.upper.is_finite()
            || self.lower >= self.upper
            || !width.is_finite()
            || width <= 0.
            || !lower.is_finite()
            || !upper.is_finite()
            || lower <= 0.
            || upper <= 0.
            || lower >= upper
            || width <= resolution
            || upper - lower <= resolution
        {
            return Err("dimensional tolerance requires finite positive nominal, ordered finite deviations with positive finite width, and distinct finite positive resulting limits with width above 128 machine epsilons of operand/limit scale".into());
        }
        Ok(())
    }

    pub fn report(
        &self,
        measured: f64,
        tolerance: cad_kernel::Tolerance,
    ) -> Result<DimensionToleranceReport, String> {
        self.validate()?;
        if !tolerance.is_valid()
            || !measured.is_finite()
            || (measured - self.nominal).abs() > tolerance.linear
        {
            return Err("native design dimension does not match authored nominal within modeling linear tolerance; manufacturing deviations do not relax this check".into());
        }
        Ok(DimensionToleranceReport {
            nominal_mm: self.nominal,
            lower_deviation_mm: self.lower,
            upper_deviation_mm: self.upper,
            lower_limit_mm: self.nominal + self.lower,
            upper_limit_mm: self.nominal + self.upper,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manufacturing_deviations_do_not_define_native_nominal_acceptance() {
        for (lower, upper) in [(-0.2, 0.3), (0., 0.3), (0.1, 0.3), (-0.3, -0.1)] {
            let t = DimensionTolerance {
                nominal: 10.,
                lower,
                upper,
            };
            let r = t.report(10., cad_kernel::Tolerance::default()).unwrap();
            assert_eq!(r.lower_limit_mm, 10. + lower);
            assert_eq!(r.upper_limit_mm, 10. + upper);
            assert!(t.report(10.2, cad_kernel::Tolerance::default()).is_err());
            assert!(t
                .report(10. + 0.5e-7, cad_kernel::Tolerance::default())
                .is_ok());
            assert!(t
                .report(10. + 2e-7, cad_kernel::Tolerance::default())
                .is_err());
        }
    }
    #[test]
    fn malformed_overflow_and_collapsed_intervals_fail() {
        for (nominal, lower, upper) in [
            (0., -1., 1.),
            (10., 0., 0.),
            (10., 1., -1.),
            (10., -10., 1.),
            (10., f64::NAN, 1.),
            (f64::INFINITY, 0., 1.),
            (f64::MAX, 0., f64::MAX),
            (1e20, 0., 1.),
            (10., 0., 1e-14),
            (1e308, -1e308, 1e308),
        ] {
            assert!(DimensionTolerance {
                nominal,
                lower,
                upper
            }
            .validate()
            .is_err());
        }
        assert!(serde_json::from_str::<DimensionTolerance>(
            r#"{"nominal":10,"lower":0,"upper":1,"unknown":true}"#
        )
        .is_err());
    }
}
