//! Source locations are an evaluation sidecar, never part of the recipe graph.
use std::collections::BTreeMap;

use cad_build::Recipe;
use serde::{Deserialize, Serialize};
use starlark::codemap::{FileSpan, Span};
use starlark::eval::Evaluator;
use starlark::syntax::ast::{AstExpr, Expr};
use starlark::syntax::AstModule;

use crate::RecipeError;

/// A half-open UTF-8 byte range in the supplied immutable source bundle.
/// Lines and Unicode-character columns are one-based.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub file: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// The executed native call and its surrounding Starlark caller locations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationSource {
    pub declaration: SourceSpan,
    /// Caller locations, nearest first. The declaration itself is omitted.
    pub call_stack: Vec<SourceSpan>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProvenance {
    pub features: BTreeMap<String, DeclarationSource>,
    pub parts: BTreeMap<String, DeclarationSource>,
}

#[derive(Clone, Debug)]
pub struct RecipeEvaluation {
    pub recipe: Recipe,
    pub provenance: SourceProvenance,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DeclarationKind {
    Feature,
    Part,
}

#[derive(Debug)]
pub(crate) struct RawDeclaration {
    kind: DeclarationKind,
    id: String,
    locations: Vec<FileSpan>,
}

const MAX_PROVENANCE_DEPTH: usize = 64;

pub(crate) fn capture(
    kind: DeclarationKind,
    id: String,
    eval: &Evaluator<'_, '_, '_>,
) -> anyhow::Result<RawDeclaration> {
    let count = eval.call_stack_count();
    anyhow::ensure!(
        count > 0 && count <= MAX_PROVENANCE_DEPTH,
        "cannot attribute '{id}' to a bounded Starlark call stack"
    );
    let mut locations = Vec::with_capacity(count);
    for n in 0..count {
        // The native function itself is a stack frame without a Starlark
        // location. Only executable Starlark frames belong in the breadcrumb.
        if let Some(location) = eval.call_stack_nth_location(n) {
            locations.push(location);
        }
    }
    anyhow::ensure!(
        !locations.is_empty(),
        "cannot locate a Starlark declaration call for '{id}'"
    );
    Ok(RawDeclaration {
        kind,
        id,
        locations,
    })
}

/// Collect parsed call expressions, including calls nested in arguments and
/// inside function bodies. The AST supplies the exact multiline call range.
pub(crate) fn calls(ast: &AstModule) -> Vec<Span> {
    fn visit_expr(expr: &AstExpr, spans: &mut Vec<Span>) {
        if matches!(expr.node, Expr::Call(..)) {
            spans.push(expr.span);
        }
        expr.node.visit_expr(|child| visit_expr(child, spans));
    }
    let mut spans = Vec::new();
    // Stmt::visit_expr already descends through nested statements.
    ast.statement()
        .node
        .visit_expr(|expr| visit_expr(expr, &mut spans));
    spans.sort_by_key(|span| (span.end().get() - span.begin().get(), span.begin().get()));
    spans
}

fn normalize(
    location: &FileSpan,
    inventory: &BTreeMap<String, &str>,
    calls: &BTreeMap<String, Vec<Span>>,
    require_call: bool,
) -> Result<SourceSpan, RecipeError> {
    let file = location.filename();
    let source = inventory.get(file).ok_or_else(|| {
        RecipeError::Evaluation(format!("provenance points outside source bundle: '{file}'"))
    })?;
    let raw_start = location.span.begin().get() as usize;
    let raw_end = location.span.end().get() as usize;
    if raw_start > raw_end || source.get(raw_start..raw_end) != Some(location.source_span()) {
        return Err(RecipeError::Evaluation(format!(
            "provenance span does not match source '{file}'"
        )));
    }
    let call = calls.get(file).and_then(|spans| {
        spans.iter().find(|span| {
            span.begin().get() as usize <= raw_start && raw_end <= span.end().get() as usize
        })
    });
    if require_call && call.is_none() {
        return Err(RecipeError::Evaluation(format!(
            "cannot attribute declaration to a parsed Starlark call in '{file}'"
        )));
    }
    let expanded = FileSpan {
        file: location.file.clone(),
        span: call.copied().unwrap_or(location.span),
    };
    let start_byte = expanded.span.begin().get() as usize;
    let end_byte = expanded.span.end().get() as usize;
    if start_byte >= end_byte || source.get(start_byte..end_byte) != Some(expanded.source_span()) {
        return Err(RecipeError::Evaluation(format!(
            "invalid call span in source '{file}'"
        )));
    }
    let lines = expanded.resolve_span();
    Ok(SourceSpan {
        file: file.to_owned(),
        start_byte,
        end_byte,
        start_line: lines.begin.line + 1,
        start_column: lines.begin.column + 1,
        end_line: lines.end.line + 1,
        end_column: lines.end.column + 1,
    })
}

pub(crate) fn finish(
    declarations: Vec<RawDeclaration>,
    root_filename: &str,
    root_source: &str,
    modules: &BTreeMap<String, String>,
    calls: &BTreeMap<String, Vec<Span>>,
) -> Result<SourceProvenance, RecipeError> {
    let mut inventory: BTreeMap<String, &str> = modules
        .iter()
        .map(|(name, source)| (name.clone(), source.as_str()))
        .collect();
    inventory.insert(root_filename.to_owned(), root_source);
    let mut result = SourceProvenance::default();
    for raw in declarations {
        let mut frames = raw.locations.iter();
        let declaration = normalize(
            frames.next().expect("captured at least one Starlark frame"),
            &inventory,
            calls,
            true,
        )?;
        let mut call_stack = Vec::new();
        for frame in frames {
            let span = normalize(frame, &inventory, calls, false)?;
            if span != declaration && call_stack.last() != Some(&span) {
                call_stack.push(span);
            }
        }
        let source = DeclarationSource {
            declaration,
            call_stack,
        };
        match raw.kind {
            DeclarationKind::Feature => {
                result.features.insert(raw.id, source);
            }
            DeclarationKind::Part => {
                result.parts.insert(raw.id, source);
            }
        }
    }
    Ok(result)
}
