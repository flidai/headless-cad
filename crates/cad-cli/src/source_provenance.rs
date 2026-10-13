//! Exact source navigation bound to the same captured recipe bundle as a build.
use super::*;
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct IndexedSource<'a> {
    text: &'a str,
    char_offsets: Vec<usize>,
    // Byte offset and Unicode character ordinal at each line start.
    line_starts: Vec<(usize, usize)>,
}

impl<'a> IndexedSource<'a> {
    fn new(text: &'a str) -> Self {
        let mut char_offsets = Vec::with_capacity(text.chars().count() + 1);
        let mut line_starts = vec![(0, 0)];
        for (ordinal, (byte, ch)) in text.char_indices().enumerate() {
            char_offsets.push(byte);
            if ch == '\n' {
                line_starts.push((byte + 1, ordinal + 1));
            }
        }
        char_offsets.push(text.len());
        Self {
            text,
            char_offsets,
            line_starts,
        }
    }

    fn line_column(&self, byte: usize) -> Option<(usize, usize)> {
        let ordinal = self.char_offsets.binary_search(&byte).ok()?;
        let line = self
            .line_starts
            .partition_point(|(start, _)| *start <= byte);
        let (_, first_ordinal) = self.line_starts[line.checked_sub(1)?];
        Some((line, ordinal - first_ordinal + 1))
    }
}

fn validate_span(
    span: &cad_recipe::SourceSpan,
    sources: &BTreeMap<String, IndexedSource<'_>>,
) -> Result<(), Box<dyn Error>> {
    let source = sources
        .get(&span.file)
        .ok_or("provenance names uncaptured source")?;
    if span.start_byte >= span.end_byte
        || span.end_byte > source.text.len()
        || source.line_column(span.start_byte) != Some((span.start_line, span.start_column))
        || source.line_column(span.end_byte) != Some((span.end_line, span.end_column))
    {
        return Err("source provenance contains invalid captured byte span".into());
    }
    Ok(())
}

fn validate_declaration(
    item: &cad_recipe::DeclarationSource,
    sources: &BTreeMap<String, IndexedSource<'_>>,
) -> Result<(), Box<dyn Error>> {
    validate_span(&item.declaration, sources)?;
    if item.call_stack.len() > 64 {
        return Err("source provenance helper call chain exceeds 64 frames".into());
    }
    for frame in &item.call_stack {
        validate_span(frame, sources)?;
    }
    Ok(())
}

pub(super) fn report(
    snapshot: &source::Snapshot,
    recipe: &Recipe,
    provenance: &cad_recipe::SourceProvenance,
    recipe_context_hash: &str,
) -> Result<Value, Box<dyn Error>> {
    if snapshot.language != "starlark"
        || snapshot.source_name.is_empty()
        || !snapshot.source_name.ends_with(".star")
    {
        return Err("exact source provenance requires captured Starlark".into());
    }
    let mut sources = BTreeMap::from([(snapshot.source_name.clone(), snapshot.source.as_str())]);
    for (label, source) in &snapshot.modules {
        if sources.insert(label.clone(), source.as_str()).is_some() {
            return Err("root source collides with locked module label".into());
        }
    }
    let indexed: BTreeMap<_, _> = sources
        .iter()
        .map(|(name, source)| (name.clone(), IndexedSource::new(source)))
        .collect();
    let feature_ids: BTreeSet<_> = recipe.features.iter().map(|feature| feature.id()).collect();
    let part_ids: BTreeSet<_> = recipe.parts.iter().map(|part| part.id.as_str()).collect();
    if feature_ids != provenance.features.keys().map(String::as_str).collect()
        || part_ids != provenance.parts.keys().map(String::as_str).collect()
    {
        return Err("provenance does not cover the evaluated feature and part declarations".into());
    }
    for item in provenance
        .features
        .values()
        .chain(provenance.parts.values())
    {
        validate_declaration(item, &indexed)?;
    }
    let source_inventory: BTreeMap<_, _> = sources
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                json!({"blake3":blake3::hash(bytes.as_bytes()).to_hex().to_string(),
                             "sha256":sha256_hex(bytes.as_bytes()),
                             "bytes":bytes.len()}),
            )
        })
        .collect();
    let body_features: BTreeMap<_, _> = recipe
        .parts
        .iter()
        .map(|part| (part.id.clone(), part.feature.clone()))
        .collect();
    Ok(json!({
        "schema_version":1,
        "kind":"source_provenance",
        "recipe_context_hash":recipe_context_hash,
        "recipe_source_hash":blake3::hash(snapshot.source.as_bytes()).to_hex().to_string(),
        "source_bundle_hash":snapshot.bundle_hash(),
        "lock_source_hash":snapshot.lock_source.as_ref()
            .map(|source| blake3::hash(source.as_bytes()).to_hex().to_string()),
        "source_inventory":source_inventory,
        "features":provenance.features,
        "parts":provenance.parts,
        "part_body_features":body_features,
    }))
}

pub(super) fn diff_report(
    snapshot: &source::Snapshot,
    recipe: &Recipe,
    provenance: &cad_recipe::SourceProvenance,
) -> Result<Value, Box<dyn Error>> {
    let mut value = report(snapshot, recipe, provenance, "")?;
    value.as_object_mut().unwrap().remove("recipe_context_hash");
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_unicode_byte_offsets_and_exact_line_columns() {
        let source = "# α\nπ = part(id='p')\n";
        let indexed = BTreeMap::from([("p.star".to_owned(), IndexedSource::new(source))]);
        let start = source.find("part(").unwrap();
        let end = start + "part(id='p')".len();
        let span = cad_recipe::SourceSpan {
            file: "p.star".to_owned(),
            start_byte: start,
            end_byte: end,
            start_line: 2,
            start_column: 5,
            end_line: 2,
            end_column: 17,
        };
        validate_span(&span, &indexed).unwrap();
        let mut bad = span.clone();
        bad.start_column += 1;
        assert!(validate_span(&bad, &indexed).is_err());
        bad = span.clone();
        bad.start_byte = 2; // Inside the two-byte α.
        assert!(validate_span(&bad, &indexed).is_err());
    }
}
