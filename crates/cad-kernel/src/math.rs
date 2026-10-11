//! Small, explicit numerical foundation. Coordinates and linear tolerances are mm.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    pub linear: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self { linear: 1.0e-7 }
    }
}

impl Tolerance {
    pub fn is_valid(self) -> bool {
        self.linear.is_finite() && self.linear > 0.0
    }
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn norm(v: [f64; 3]) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}

pub(crate) fn finite(v: [f64; 3]) -> bool {
    v.into_iter().all(f64::is_finite)
}
