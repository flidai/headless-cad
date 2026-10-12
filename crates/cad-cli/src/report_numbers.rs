//! Rounded display values; JSON remains the full-precision evidence.
pub(super) fn number(value: f64) -> String {
    if value != 0. && value.abs() < 1e-6 {
        format!("{value:.3e}")
    } else {
        let rounded = format!("{value:.6}");
        let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
        if trimmed == "-0" {
            "0".into()
        } else {
            trimmed.into()
        }
    }
}

pub(super) fn measured(lower: f64, upper: f64) -> String {
    format!(
        "≈ {} ± {:.2e}",
        number(lower + (upper - lower) / 2.),
        (upper - lower) / 2.
    )
}
