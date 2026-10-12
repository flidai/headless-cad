//! Source-owned, explicitly scaled paper layouts of validated native section views.
//! This is not projected hidden-line drawing or a GD&T interpretation layer.
use super::*;

pub const MAX_DRAWING_SHEETS: usize = 16;
pub const MAX_DRAWING_VIEWPORTS_PER_SHEET: usize = 32;
pub const MAX_DRAWING_VIEWPORTS: usize = 256;
pub const DRAWING_MARGIN_MM: f64 = 10.;
pub const DRAWING_TITLE_BLOCK_MM: f64 = 25.;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingViewport {
    pub view: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingSheet {
    pub id: String,
    pub title: String,
    pub revision: String,
    pub width: f64,
    pub height: f64,
    pub views: Vec<DrawingViewport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrawingSheetReport {
    pub declaration: DrawingSheet,
    pub status: String,
    /// Sorted unique references into BuildResult.section_views, retaining its
    /// native queries and witnesses without duplicating geometry per viewport.
    pub section_view_ids: Vec<String>,
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        && !matches!(
            id.to_ascii_uppercase().as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}
impl DrawingSheet {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidDrawingSheet {
            id: self.id.clone(),
            message: message.into(),
        };
        if !safe_id(&self.id) {
            return Err(fail("ID must be 1..128 portable ASCII letters, digits, underscores or hyphens; reserved device names are excluded"));
        }
        let text = |s: &str, max: usize| {
            !s.is_empty() && s.len() <= max && s.trim() == s && !s.chars().any(char::is_control)
        };
        if !text(&self.title, 256) || !text(&self.revision, 64) {
            return Err(fail("title/revision must be nonempty trimmed text without controls, at most 256/64 UTF-8 bytes"));
        }
        if [self.width, self.height]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > 2000.)
        {
            return Err(fail(
                "paper dimensions must be finite positive millimetres at most 2000",
            ));
        }
        if self.views.is_empty() || self.views.len() > MAX_DRAWING_VIEWPORTS_PER_SHEET {
            return Err(fail("sheet requires 1..32 viewports"));
        }
        let mut view_ids = BTreeSet::new();
        for (i, view) in self.views.iter().enumerate() {
            if !view_ids.insert(&view.view) {
                return Err(fail("a section view may appear only once per sheet"));
            }
            if view.view.is_empty() || view.view.len() > 128 || view.view.trim() != view.view {
                return Err(fail("viewport requires an exact section view ID"));
            }
            if [view.x, view.y, view.width, view.height, view.scale]
                .iter()
                .any(|v| !v.is_finite())
                || view.width <= 0.
                || view.height <= 0.
                || view.scale <= 0.
            {
                return Err(fail(
                    "viewport coordinates must be finite, size and scale strictly positive",
                ));
            }
            if view.x < DRAWING_MARGIN_MM
                || view.y < DRAWING_MARGIN_MM
                || view.width > self.width - DRAWING_MARGIN_MM - view.x
                || view.height > self.height - DRAWING_MARGIN_MM - DRAWING_TITLE_BLOCK_MM - view.y
            {
                return Err(fail("viewport exceeds printable area: 10 mm margins and additional bottom 25 mm title block"));
            }
            for other in &self.views[..i] {
                if view.x < other.x + other.width
                    && other.x < view.x + view.width
                    && view.y < other.y + other.height
                    && other.y < view.y + view.height
                {
                    return Err(fail("viewport interiors overlap"));
                }
            }
        }
        Ok(())
    }
}

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version == 1 && !recipe.drawing_sheets.is_empty() {
        return Err(BuildError::DrawingSheetSchema);
    }
    if recipe.drawing_sheets.len() > MAX_DRAWING_SHEETS {
        return Err(BuildError::ResourceLimit {
            resource: "drawing sheets",
            limit: MAX_DRAWING_SHEETS,
        });
    }
    let mut count = 0;
    let mut ids = BTreeSet::new();
    let sections: BTreeSet<_> = recipe
        .section_views
        .iter()
        .map(|view| view.id.as_str())
        .collect();
    for sheet in &recipe.drawing_sheets {
        sheet.validate()?;
        if !ids.insert(sheet.id.to_ascii_lowercase()) {
            return Err(BuildError::InvalidDrawingSheet {
                id: sheet.id.clone(),
                message: "duplicate case-insensitive sheet ID".into(),
            });
        }
        count += sheet.views.len();
        if count > MAX_DRAWING_VIEWPORTS {
            return Err(BuildError::ResourceLimit {
                resource: "drawing viewports",
                limit: MAX_DRAWING_VIEWPORTS,
            });
        }
        for viewport in &sheet.views {
            if !sections.contains(viewport.view.as_str()) {
                return Err(BuildError::InvalidDrawingSheet {
                    id: sheet.id.clone(),
                    message: format!("unknown section view '{}'", viewport.view),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn evaluate(
    recipe: &Recipe,
    sections: &BTreeMap<String, SectionViewReport>,
) -> Result<BTreeMap<String, DrawingSheetReport>, BuildError> {
    recipe
        .drawing_sheets
        .iter()
        .map(|sheet| {
            let ids: BTreeSet<_> = sheet.views.iter().map(|v| v.view.clone()).collect();
            for id in &ids {
                if !sections.contains_key(id) {
                    return Err(BuildError::InvalidDrawingSheet {
                        id: sheet.id.clone(),
                        message: format!("section view '{id}' was not evaluated"),
                    });
                }
            }
            Ok((
                sheet.id.clone(),
                DrawingSheetReport {
                    declaration: sheet.clone(),
                    status: "layout_validated".into(),
                    section_view_ids: ids.into_iter().collect(),
                },
            ))
        })
        .collect()
}
