//! Source-owned, explicitly scaled paper layouts of validated native section views.
//! This is not projected hidden-line drawing or a GD&T interpretation layer.
use super::*;

pub const MAX_DRAWING_SHEETS: usize = 16;
pub const MAX_DRAWING_VIEWPORTS_PER_SHEET: usize = 32;
pub const MAX_DRAWING_VIEWPORTS: usize = 256;
pub const MAX_DRAWING_DIMENSION_TABLES_PER_SHEET: usize = 8;
pub const MAX_DRAWING_DIMENSION_ROWS_PER_TABLE: usize = 16;
pub const MAX_DRAWING_DIMENSION_ROWS: usize = 128;
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
pub struct DrawingDimensionRow {
    pub dimension: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingDimensionTable {
    pub id: String,
    pub title: String,
    pub output: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rows: Vec<DrawingDimensionRow>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimension_tables: Vec<DrawingDimensionTable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrawingSheetReport {
    pub declaration: DrawingSheet,
    pub status: String,
    /// Sorted unique references into BuildResult.section_views, retaining its
    /// native queries and witnesses without duplicating geometry per viewport.
    pub section_view_ids: Vec<String>,
    /// Sorted unique native datum-dimension references. Resolved witnesses live
    /// in BuildResult.datum_dimensions and the published engineering report.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub datum_dimension_ids: Vec<String>,
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
fn native_dimension_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
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
            if !view_ids.insert(view.view.to_ascii_lowercase()) {
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
        if self.dimension_tables.len() > MAX_DRAWING_DIMENSION_TABLES_PER_SHEET {
            return Err(fail("at most 8 native dimension tables per sheet"));
        }
        let mut table_ids = view_ids;
        let mut generated_table_ids = BTreeSet::new();
        for (i, table) in self.dimension_tables.iter().enumerate() {
            if !safe_id(&table.id) || !table_ids.insert(table.id.to_ascii_lowercase()) {
                return Err(fail("dimension table IDs must be unique portable IDs and not collide with viewport IDs"));
            }
            if !generated_table_ids
                .insert(format!("drawing-table-{}", table.id).to_ascii_lowercase())
            {
                return Err(fail("generated drawing table IDs collide"));
            }
            if !text(&table.title, 128)
                || table.output.is_empty()
                || table.output.len() > 256
                || table.output.trim() != table.output
                || table.output.chars().any(char::is_control)
            {
                return Err(fail(
                    "dimension table requires bounded title and exact output ID",
                ));
            }
            if table.rows.is_empty() || table.rows.len() > MAX_DRAWING_DIMENSION_ROWS_PER_TABLE {
                return Err(fail("dimension table requires 1..16 rows"));
            }
            let mut row_ids = BTreeSet::new();
            for row in &table.rows {
                if !generated_table_ids.insert(
                    format!("drawing-table-{}-{}", table.id, row.dimension).to_ascii_lowercase(),
                ) {
                    return Err(fail("generated drawing table row IDs collide"));
                }
                if !native_dimension_id(&row.dimension)
                    || !text(&row.label, 96)
                    || !row_ids.insert(row.dimension.to_ascii_lowercase())
                {
                    return Err(fail("dimension table rows require unique native dimension IDs and bounded labels"));
                }
            }
            if [table.x, table.y, table.width, table.height]
                .iter()
                .any(|v| !v.is_finite())
                || table.width <= 0.
                || table.height <= 0.
                || table.x < DRAWING_MARGIN_MM
                || table.y < DRAWING_MARGIN_MM
                || table.width > self.width - DRAWING_MARGIN_MM - table.x
                || table.height > self.height - DRAWING_MARGIN_MM - DRAWING_TITLE_BLOCK_MM - table.y
            {
                return Err(fail("dimension table exceeds printable area"));
            }
            // The renderer reserves a 10 mm header/footer and 8 mm per row.
            // Reserve sufficient geometry so it cannot silently clip rows.
            if table.width < 80. || table.height < 10. + table.rows.len() as f64 * 8. {
                return Err(fail("dimension table is too small for its rows"));
            }
            // Match the renderer's conservative fixed type geometry. UTF-8
            // byte count overestimates rendered glyphs, so never admits a
            // label the renderer would later clip.
            if table.title.len() as f64 * 3.2 * 0.7 > table.width - 6.
                || table
                    .rows
                    .iter()
                    .any(|r| r.label.len() as f64 * 2.8 * 0.7 > table.width - 42.)
            {
                return Err(fail("dimension table text does not fit its declared width"));
            }
            for view in &self.views {
                if table.x < view.x + view.width
                    && view.x < table.x + table.width
                    && table.y < view.y + view.height
                    && view.y < table.y + table.height
                {
                    return Err(fail("dimension table overlaps a viewport"));
                }
            }
            for other in &self.dimension_tables[..i] {
                if table.x < other.x + other.width
                    && other.x < table.x + table.width
                    && table.y < other.y + other.height
                    && other.y < table.y + table.height
                {
                    return Err(fail("dimension table interiors overlap"));
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
    let mut row_count = 0;
    let mut ids = BTreeSet::new();
    let sections: BTreeSet<_> = recipe
        .section_views
        .iter()
        .map(|view| view.id.as_str())
        .collect();
    let dimensions: BTreeMap<_, _> = recipe
        .datum_dimensions
        .iter()
        .map(|d| (d.id.as_str(), d.output.as_str()))
        .collect();
    for sheet in &recipe.drawing_sheets {
        sheet.validate()?;
        row_count += sheet
            .dimension_tables
            .iter()
            .map(|t| t.rows.len())
            .sum::<usize>();
        if row_count > MAX_DRAWING_DIMENSION_ROWS {
            return Err(BuildError::ResourceLimit {
                resource: "drawing dimension rows",
                limit: MAX_DRAWING_DIMENSION_ROWS,
            });
        }
        for table in &sheet.dimension_tables {
            if !recipe
                .assembly_outputs
                .iter()
                .any(|o| o.name == table.output)
            {
                return Err(BuildError::InvalidDrawingSheet {
                    id: sheet.id.clone(),
                    message: format!("unknown dimension table output '{}'", table.output),
                });
            }
            for row in &table.rows {
                if dimensions.get(row.dimension.as_str()) != Some(&table.output.as_str()) {
                    return Err(BuildError::InvalidDrawingSheet {
                        id: sheet.id.clone(),
                        message: format!(
                            "native dimension '{}' is unknown or belongs to a different output",
                            row.dimension
                        ),
                    });
                }
            }
        }
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
    dimensions: &BTreeMap<String, DatumDimensionReport>,
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
            let dimension_ids: BTreeSet<_> = sheet
                .dimension_tables
                .iter()
                .flat_map(|t| t.rows.iter().map(|r| r.dimension.clone()))
                .collect();
            for id in &dimension_ids {
                if !dimensions.contains_key(id) {
                    return Err(BuildError::InvalidDrawingSheet {
                        id: sheet.id.clone(),
                        message: format!("native dimension '{id}' was not evaluated"),
                    });
                }
            }
            Ok((
                sheet.id.clone(),
                DrawingSheetReport {
                    declaration: sheet.clone(),
                    status: "layout_validated".into(),
                    section_view_ids: ids.into_iter().collect(),
                    datum_dimension_ids: dimension_ids.into_iter().collect(),
                },
            ))
        })
        .collect()
}
