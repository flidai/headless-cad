//! Engineering metadata is definition identity, never geometric identity.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub name: String,
    pub density_kg_per_m3: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<Material>,
}

impl PartMetadata {
    pub fn validate(&self) -> Result<(), AssemblyError> {
        for value in [&self.part_number, &self.revision].into_iter().flatten() {
            if value.is_empty() || value.trim() != value {
                return Err(AssemblyError::Metadata(
                    "part number and revision must be nonempty without surrounding whitespace",
                ));
            }
        }
        if self
            .description
            .as_ref()
            .is_some_and(|text| text.trim().is_empty())
        {
            return Err(AssemblyError::Metadata(
                "description must contain non-whitespace text",
            ));
        }
        if let Some(material) = &self.material {
            if material.name.is_empty() || material.name.trim() != material.name {
                return Err(AssemblyError::Metadata(
                    "material name must be nonempty without surrounding whitespace",
                ));
            }
            if !material.density_kg_per_m3.is_finite() || material.density_kg_per_m3 <= 0.0 {
                return Err(AssemblyError::Metadata(
                    "material density must be finite and positive in kg/m³",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomItem {
    pub part_id: String,
    pub feature_id: String,
    pub content_hash: String,
    pub metadata: PartMetadata,
    pub quantity: usize,
    pub unit_volume_mm3: f64,
    pub total_volume_mm3: f64,
    pub unit_mass_kg: Option<f64>,
    pub total_mass_kg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomReport {
    pub assembly_id: String,
    pub content_hash: String,
    /// Sorted by logical part ID. Equal geometry, part numbers, or revisions do
    /// not merge independently defined parts. Placements count as occurrences,
    /// even when they overlap. Mass/volume are sums, not Boolean-union measures.
    pub items: Vec<BomItem>,
    pub total_occurrences: usize,
    pub total_volume_mm3: f64,
    /// None if *any* occurrence lacks material, including empty part geometry.
    pub total_mass_kg: Option<f64>,
    pub known_mass_kg: f64,
    pub missing_material_occurrences: usize,
}

fn normal_nonnegative(value: f64) -> Result<f64, AssemblyError> {
    if value == 0.0 || (value.is_normal() && value > 0.0) {
        Ok(value)
    } else {
        Err(AssemblyError::NumericRange)
    }
}

pub(super) fn mass(volume: f64, density: f64) -> Result<f64, AssemblyError> {
    if volume == 0.0 {
        return Ok(0.0);
    }
    // Reorder the three positive factors to avoid avoidable intermediate
    // overflow/underflow. Never accept a nonzero mass rounded to zero/subnormal.
    for (a, b, c) in [
        (volume, 1e-9, density),
        (density, 1e-9, volume),
        (volume, density, 1e-9),
    ] {
        let intermediate = a * b;
        let result = intermediate * c;
        if intermediate.is_normal() && result.is_normal() && result > 0.0 {
            return Ok(result);
        }
    }
    Err(AssemblyError::NumericRange)
}

fn accumulate(total: &mut f64, compensation: &mut f64, value: f64) -> Result<(), AssemblyError> {
    let corrected = value - *compensation;
    let next = normal_nonnegative(*total + corrected)?;
    *compensation = (next - *total) - corrected;
    *total = next;
    Ok(())
}

impl BuiltAssembly {
    /// Validate the authoritative hierarchy and return a deterministic leaf-part
    /// BOM. Material density is kg/m³; geometric volume is mm³ (factor 1e-9).
    pub fn bill_of_materials(&self, tolerance: Tolerance) -> Result<BomReport, AssemblyError> {
        self.validate(tolerance)?;
        self.calculate_bom()
    }

    pub(super) fn calculate_bom(&self) -> Result<BomReport, AssemblyError> {
        let mut quantities = BTreeMap::<&str, usize>::new();
        for instance in &self.instances {
            let quantity = quantities.entry(&instance.part).or_default();
            *quantity = quantity
                .checked_add(1)
                .ok_or(AssemblyError::ResourceLimit)?;
        }
        let mut items = Vec::with_capacity(quantities.len());
        let (mut volume, mut volume_compensation) = (0.0, 0.0);
        let (mut known_mass, mut mass_compensation) = (0.0, 0.0);
        let mut missing = 0usize;
        for (id, quantity) in quantities {
            let part = self
                .parts
                .get(id)
                .ok_or_else(|| AssemblyError::MissingPart {
                    instance: "BOM".into(),
                    part: id.into(),
                })?;
            part.metadata.validate()?;
            let unit_volume = normal_nonnegative(part.result.model.volume())?;
            let total_volume = normal_nonnegative(unit_volume * quantity as f64)?;
            let unit_mass = part
                .metadata
                .material
                .as_ref()
                .map(|material| mass(unit_volume, material.density_kg_per_m3))
                .transpose()?;
            let total_mass = unit_mass
                .map(|mass| normal_nonnegative(mass * quantity as f64))
                .transpose()?;
            accumulate(&mut volume, &mut volume_compensation, total_volume)?;
            if let Some(mass) = total_mass {
                accumulate(&mut known_mass, &mut mass_compensation, mass)?;
            } else {
                missing = missing
                    .checked_add(quantity)
                    .ok_or(AssemblyError::ResourceLimit)?;
            }
            items.push(BomItem {
                part_id: id.into(),
                feature_id: part.feature_id.clone(),
                content_hash: part.content_hash.clone(),
                metadata: part.metadata.clone(),
                quantity,
                unit_volume_mm3: unit_volume,
                total_volume_mm3: total_volume,
                unit_mass_kg: unit_mass,
                total_mass_kg: total_mass,
            });
        }
        Ok(BomReport {
            assembly_id: self.assembly_id.clone(),
            content_hash: self.content_hash.clone(),
            items,
            total_occurrences: self.instances.len(),
            total_volume_mm3: volume,
            total_mass_kg: if missing == 0 { Some(known_mass) } else { None },
            known_mass_kg: known_mass,
            missing_material_occurrences: missing,
        })
    }
}
