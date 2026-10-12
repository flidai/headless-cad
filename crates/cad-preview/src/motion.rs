//! Read-only inspection of recorded planar motion samples; never executes a solver.
use crate::{script_json, PreviewError, RecipeContext};

pub fn to_motion_study_html(
    mechanism: &cad_assembly::PlanarMechanism,
    study: &cad_assembly::MotionReport,
    context: &RecipeContext,
) -> Result<String, PreviewError> {
    if study.frames.is_empty() {
        return Err(PreviewError::Recipe("motion study has no accepted samples"));
    }
    let data =
        script_json(&serde_json::json!({"mechanism":mechanism,"study":study,"recipe":context}))?;
    Ok(include_str!("motion.html").replacen("/*MOTION_DATA*/null", &data, 1))
}
