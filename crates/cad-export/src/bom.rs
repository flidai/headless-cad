//! UTF-8 RFC 4180 bill-of-materials export. Text is preserved verbatim, including
//! formula-like prefixes: spreadsheet consumers must import textual columns as
//! text. Missing mass is an empty field, never an inferred zero. Material metadata
//! is not currently mapped into STEP; this CSV and the BOM JSON carry that data.
use crate::ExportError;

const HEADER: &str = "part_id,part_number,revision,description,material,density_kg_per_m3,quantity,unit_volume_mm3,total_volume_mm3,unit_mass_kg,total_mass_kg\r\n";

fn cell(output: &mut String, value: &str) {
    if value.contains([',', '"', '\r', '\n']) {
        output.push('"');
        for character in value.chars() {
            output.push(character);
            if character == '"' {
                output.push('"');
            }
        }
        output.push('"');
    } else {
        output.push_str(value);
    }
}

fn number(value: f64) -> Result<String, ExportError> {
    if !value.is_finite() || value < 0.0 {
        return Err(ExportError::InvalidBom(
            "numeric values must be finite and nonnegative".into(),
        ));
    }
    Ok(if value == 0.0 {
        "0".into()
    } else {
        value.to_string()
    })
}

fn optional_number(value: Option<f64>) -> Result<String, ExportError> {
    value.map(number).transpose().map(Option::unwrap_or_default)
}

/// Serialize a BOM in stable part-ID order. Empty optional cells mean unknown.
/// Report totals remain in the JSON report; CSV contains one row per part.
/// Expects a report produced by the validated kernel assembly. Guards row values
/// and metadata, but cannot certify geometric volumes or aggregate totals.
pub fn bom_to_csv(report: &cad_assembly::BomReport) -> Result<String, ExportError> {
    let mut output = HEADER.to_owned();
    let mut items: Vec<_> = report.items.iter().collect();
    items.sort_by(|a, b| a.part_id.cmp(&b.part_id));
    if items
        .windows(2)
        .any(|pair| pair[0].part_id == pair[1].part_id)
    {
        return Err(ExportError::InvalidBom("duplicate part IDs".into()));
    }
    for item in items {
        if item.part_id.is_empty() || item.quantity == 0 {
            return Err(ExportError::InvalidBom(
                "parts require an ID and positive quantity".into(),
            ));
        }
        let metadata = &item.metadata;
        metadata
            .validate()
            .map_err(|error| ExportError::InvalidBom(error.to_string()))?;
        let density = optional_number(
            metadata
                .material
                .as_ref()
                .map(|material| material.density_kg_per_m3),
        )?;
        let quantity = item.quantity.to_string();
        let unit_volume = number(item.unit_volume_mm3)?;
        let total_volume = number(item.total_volume_mm3)?;
        let unit_mass = optional_number(item.unit_mass_kg)?;
        let total_mass = optional_number(item.total_mass_kg)?;
        let values = [
            item.part_id.as_str(),
            metadata.part_number.as_deref().unwrap_or_default(),
            metadata.revision.as_deref().unwrap_or_default(),
            metadata.description.as_deref().unwrap_or_default(),
            metadata
                .material
                .as_ref()
                .map(|material| material.name.as_str())
                .unwrap_or_default(),
            &density,
            &quantity,
            &unit_volume,
            &total_volume,
            &unit_mass,
            &total_mass,
        ];
        for (index, value) in values.into_iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            cell(&mut output, value);
        }
        output.push_str("\r\n");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_assembly::{BomItem, BomReport, Material, PartMetadata};

    fn report() -> BomReport {
        BomReport {
            assembly_id: "example".into(),
            content_hash: "assembly".into(),
            items: vec![
                BomItem {
                    part_id: "z".into(),
                    feature_id: "body".into(),
                    content_hash: "geometry".into(),
                    metadata: PartMetadata {
                        part_number: Some("=ABC".into()),
                        revision: Some("a,b".into()),
                        description: Some("Équerre \"large\"\r\nsecond line".into()),
                        material: Some(Material {
                            name: "Steel".into(),
                            density_kg_per_m3: 8000.,
                        }),
                    },
                    quantity: 2,
                    unit_volume_mm3: 1000.,
                    total_volume_mm3: 2000.,
                    unit_mass_kg: Some(0.008),
                    total_mass_kg: Some(0.016),
                },
                BomItem {
                    part_id: "a".into(),
                    feature_id: "other".into(),
                    content_hash: "other".into(),
                    metadata: PartMetadata::default(),
                    quantity: 1,
                    unit_volume_mm3: 300.,
                    total_volume_mm3: 300.,
                    unit_mass_kg: None,
                    total_mass_kg: None,
                },
            ],
            total_occurrences: 3,
            total_volume_mm3: 2300.,
            total_mass_kg: None,
            known_mass_kg: 0.016,
            missing_material_occurrences: 1,
        }
    }

    #[test]
    fn stable_columns_quoted_text_unicode_and_unknown_mass() {
        let report = report();
        let csv = bom_to_csv(&report).unwrap();
        assert_eq!(csv, format!("{HEADER}a,,,,,,1,300,300,,\r\nz,=ABC,\"a,b\",\"Équerre \"\"large\"\"\r\nsecond line\",Steel,8000,2,1000,2000,0.008,0.016\r\n"));
        let mut reordered = report;
        reordered.items.reverse();
        assert_eq!(csv, bom_to_csv(&reordered).unwrap());
    }

    #[test]
    fn externally_supplied_invalid_material_metadata_is_rejected() {
        // Public/deserialized reports can bypass the assembly's BOM constructor.
        let mut zero_density = report();
        zero_density.items[0]
            .metadata
            .material
            .as_mut()
            .unwrap()
            .density_kg_per_m3 = 0.;
        assert!(matches!(
            bom_to_csv(&zero_density),
            Err(ExportError::InvalidBom(_))
        ));
        let mut whitespace_name = report();
        whitespace_name.items[0]
            .metadata
            .material
            .as_mut()
            .unwrap()
            .name = "  ".into();
        assert!(matches!(
            bom_to_csv(&whitespace_name),
            Err(ExportError::InvalidBom(_))
        ));
    }

    #[test]
    fn invalid_numbers_and_duplicate_rows_are_rejected() {
        for value in [f64::NAN, f64::INFINITY, -1.] {
            let mut invalid = report();
            invalid.items[0].unit_mass_kg = Some(value);
            assert!(bom_to_csv(&invalid).is_err());
        }
        let mut duplicate = report();
        duplicate.items.push(duplicate.items[0].clone());
        assert!(bom_to_csv(&duplicate).is_err());
        let mut invalid = report();
        invalid.items[0].quantity = 0;
        assert!(bom_to_csv(&invalid).is_err());
    }
}
