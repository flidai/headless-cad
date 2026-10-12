//! Prescribed one-coordinate joint motion; no free-coordinate or loop solver.
use super::*;
pub const MAX_PRESCRIBED_JOINTS: usize = 1024;
pub const JOINT_ANGULAR_TOLERANCE_RAD: f64 = 1024. * f64::EPSILON;
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrescribedJoint {
    Revolute { position: f64, min: f64, max: f64 },
    Prismatic { position: f64, min: f64, max: f64 },
}
impl<'de> Deserialize<'de> for PrescribedJoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            kind: String,
            position: f64,
            min: f64,
            max: f64,
        }
        let Wire {
            kind,
            position,
            min,
            max,
        } = Wire::deserialize(deserializer)?;
        match kind.as_str() {
            "revolute" => Ok(Self::Revolute { position, min, max }),
            "prismatic" => Ok(Self::Prismatic { position, min, max }),
            _ => Err(serde::de::Error::unknown_variant(
                &kind,
                &["revolute", "prismatic"],
            )),
        }
    }
}
impl PrescribedJoint {
    pub fn validate(&self) -> Result<(), AssemblyError> {
        let (position, min, max) = match *self {
            Self::Revolute { position, min, max } | Self::Prismatic { position, min, max } => {
                (position, min, max)
            }
        };
        if ![position, min, max].into_iter().all(f64::is_finite)
            || min > max
            || position < min
            || position > max
        {
            return Err(AssemblyError::Attachment(
                "prescribed joint requires finite ordered limits containing its position".into(),
            ));
        }
        Ok(())
    }
    pub fn motion(&self, tolerance: Tolerance) -> Result<RigidTransform, AssemblyError> {
        if !tolerance.is_valid() {
            return Err(AssemblyError::Tolerance);
        }
        self.validate()?;
        let frame = match *self {
            Self::Revolute { position, .. } => RigidTransform {
                translation: [0.; 3],
                rotation: [0., 0., (position / 2.).sin(), (position / 2.).cos()],
            },
            Self::Prismatic { position, .. } => RigidTransform {
                translation: [0., 0., position],
                ..Default::default()
            },
        };
        frame.validate()?;
        Ok(frame)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JointReport {
    pub definition_id: String,
    pub occurrence_id: String,
    pub occurrence_kind: &'static str,
    pub coordinate_frame: &'static str,
    pub joint: PrescribedJoint,
    pub coordinate_units: &'static str,
    pub intrinsic_dof: usize,
    pub remaining_dof: usize,
    pub source_resolved_frame: RigidTransform,
    pub target_resolved_frame: RigidTransform,
    pub expected_source_frame: RigidTransform,
    pub residual_translation_mm: [f64; 3],
    pub residual_translation_norm_mm: f64,
    pub residual_angle_rad: f64,
    pub angular_tolerance_rad: f64,
}
impl JointReport {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn checked(
        definition_id: &str,
        occurrence_id: &str,
        occurrence_kind: &'static str,
        joint: PrescribedJoint,
        source: RigidTransform,
        target: RigidTransform,
        expected: RigidTransform,
        tolerance: Tolerance,
    ) -> Result<Self, AssemblyError> {
        let residual = std::array::from_fn(|i| source.translation[i] - expected.translation[i]);
        let length = residual[0].hypot(residual[1]).hypot(residual[2]);
        // Quaternion angular separation uses atan2, preserving small residuals
        // that acos(dot) would round to zero. q and -q denote the same rotation.
        let [x, y, z, w] = source.rotation;
        let [a, b, c, d] = expected.rotation;
        let v = [
            d * x - a * w - b * z + c * y,
            d * y + a * z - b * w - c * x,
            d * z - a * y + b * x - c * w,
        ];
        let scalar = a * x + b * y + c * z + d * w;
        let angle = 2. * v[0].hypot(v[1]).hypot(v[2]).atan2(scalar.abs());
        if !length.is_finite()
            || !angle.is_finite()
            || length > tolerance.linear
            || angle > JOINT_ANGULAR_TOLERANCE_RAD
        {
            return Err(AssemblyError::NumericRange);
        }
        Ok(Self {
            definition_id: definition_id.into(),
            occurrence_id: occurrence_id.into(),
            occurrence_kind,
            coordinate_frame: "assembly_definition",
            joint,
            coordinate_units: match joint {
                PrescribedJoint::Revolute { .. } => "rad",
                PrescribedJoint::Prismatic { .. } => "mm",
            },
            intrinsic_dof: 1,
            remaining_dof: 0,
            source_resolved_frame: source,
            target_resolved_frame: target,
            expected_source_frame: expected,
            residual_translation_mm: residual,
            residual_translation_norm_mm: length,
            residual_angle_rad: angle,
            angular_tolerance_rad: JOINT_ANGULAR_TOLERANCE_RAD,
        })
    }
}
impl BuiltAssembly {
    /// Reports shared-definition local joints, not expanded/global degrees of freedom.
    /// Re-resolves authoritative attachment frames and certifies each residual.
    pub fn joint_reports(&self, tolerance: Tolerance) -> Result<Vec<JointReport>, AssemblyError> {
        Ok(crate::datum::resolve_with_reports(&self.parts, &self.hierarchy, tolerance)?.1)
    }
}
