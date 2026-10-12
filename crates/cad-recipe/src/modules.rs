//! Explicit source-bundle loading. All module labels address the same bundle root.
use super::*;
use starlark::environment::{FrozenModule, Module};
use starlark::eval::ReturnFileLoader;
use starlark::syntax::{AstModule, Dialect};
use std::collections::HashMap;

pub const MAX_RECIPE_MODULES: usize = 32;
pub const MAX_MODULE_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_RECIPE_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MODULE_TICKS: u64 = 1_000_000;
pub const MAX_RECIPE_TICKS: u64 = 4_000_000;
pub const MAX_MODULE_HEAP_BYTES: usize = 16 * 1024 * 1024;
/// Sum of evaluation heap peaks (including frozen allocations), conservatively
/// charged once per evaluated module. This is not a bound on process RSS.
pub const MAX_RECIPE_HEAP_BYTES: usize = 64 * 1024 * 1024;
fn fail(message: impl Into<String>) -> RecipeError {
    RecipeError::Evaluation(message.into())
}

pub fn validate_module_label(label: &str) -> Result<(), RecipeError> {
    if !label.ends_with(".star")
        || label.len() > 1024
        || label.contains(['\\', ':'])
        || label.chars().any(|c| c.is_control())
        || label
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.trim() != part)
    {
        return Err(fail(format!(
            "invalid module label '{label}': expected a canonical project-root-relative .star path"
        )));
    }
    Ok(())
}
fn parse(filename: &str, source: &str) -> Result<AstModule, RecipeError> {
    if source.len() > MAX_MODULE_SOURCE_BYTES {
        return Err(fail(format!(
            "{filename}: module source exceeds {MAX_MODULE_SOURCE_BYTES} bytes"
        )));
    }
    let ast = AstModule::parse(filename, source.to_owned(), &Dialect::Standard)
        .map_err(|e| fail(e.to_string()))?;
    for load in ast.loads() {
        validate_module_label(load.module_id)?;
    }
    Ok(ast)
}
/// Parse direct load labels without accessing or evaluating their sources.
pub fn starlark_loads(filename: &str, source: &str) -> Result<Vec<String>, RecipeError> {
    Ok(parse(filename, source)?
        .loads()
        .into_iter()
        .map(|l| l.module_id.to_owned())
        .collect())
}

fn visit(
    label: &str,
    modules: &BTreeMap<String, String>,
    pending: &mut BTreeSet<String>,
    asts: &mut BTreeMap<String, AstModule>,
    order: &mut Vec<String>,
) -> Result<(), RecipeError> {
    if asts.contains_key(label) {
        return Ok(());
    }
    if !pending.insert(label.to_owned()) {
        return Err(fail(format!("module load cycle at '{label}'")));
    }
    let source = modules
        .get(label)
        .ok_or_else(|| fail(format!("missing supplied module '{label}'")))?;
    let ast = parse(label, source)?;
    for load in ast.loads() {
        visit(load.module_id, modules, pending, asts, order)?;
    }
    pending.remove(label);
    asts.insert(label.to_owned(), ast);
    order.push(label.to_owned());
    Ok(())
}

pub(super) fn evaluate(
    filename: &str,
    source: &str,
    modules: &BTreeMap<String, String>,
    graph: &GraphStore,
) -> Result<(), RecipeError> {
    if modules.len() > MAX_RECIPE_MODULES {
        return Err(fail(format!(
            "recipe exceeds {MAX_RECIPE_MODULES} supplied modules"
        )));
    }
    let mut bytes = source.len();
    for (label, text) in modules {
        validate_module_label(label)?;
        if label == filename {
            return Err(fail("entry source cannot also be a supplied module"));
        }
        if text.len() > MAX_MODULE_SOURCE_BYTES {
            return Err(fail(format!(
                "{label}: module source exceeds {MAX_MODULE_SOURCE_BYTES} bytes"
            )));
        }
        bytes = bytes.saturating_add(text.len());
    }
    if bytes > MAX_RECIPE_SOURCE_BYTES {
        return Err(fail(format!(
            "recipe source bundle exceeds {MAX_RECIPE_SOURCE_BYTES} bytes"
        )));
    }
    let root = parse(filename, source)?;
    let mut asts = BTreeMap::new();
    let mut order = Vec::new();
    let mut pending = BTreeSet::from([filename.to_owned()]);
    for load in root.loads() {
        visit(load.module_id, modules, &mut pending, &mut asts, &mut order)?;
    }
    // Preflight the complete reachable load graph before any native declaration.
    let globals = GlobalsBuilder::standard().with(cad_functions).build();
    let mut frozen: BTreeMap<String, FrozenModule> = BTreeMap::new();
    let mut ticks = 0u64;
    let mut heap_charge = 0usize;
    asts.insert(filename.to_owned(), root);
    order.push(filename.to_owned());
    for label in order {
        let ast = asts.remove(&label).expect("preflighted module");
        let loader_modules: HashMap<_, _> = frozen.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let loader = ReturnFileLoader {
            modules: &loader_modules,
        };
        let tick_limit = MAX_MODULE_TICKS.min(MAX_RECIPE_TICKS.saturating_sub(ticks));
        let heap_limit =
            MAX_MODULE_HEAP_BYTES.min(MAX_RECIPE_HEAP_BYTES.saturating_sub(heap_charge));
        if tick_limit == 0 || heap_limit == 0 {
            return Err(fail("recipe module aggregate evaluation budget exhausted"));
        }
        let (module, used_ticks, used_heap) = Module::with_temp_heap(|module| {
            let mut evaluator = Evaluator::new(&module);
            evaluator.extra = Some(graph);
            evaluator.set_loader(&loader);
            evaluator
                .set_max_callstack_size(64)
                .map_err(|e| fail(e.to_string()))?;
            evaluator
                .set_max_heap_size(heap_limit)
                .map_err(|e| fail(e.to_string()))?;
            evaluator
                .set_max_tick_count(tick_limit)
                .map_err(|e| fail(e.to_string()))?;
            evaluator
                .eval_module(ast, &globals)
                .map_err(|e| fail(e.to_string()))?;
            let used_ticks = evaluator.get_total_tick_count();
            drop(evaluator);
            let used_heap = module
                .heap()
                .peak_allocated_bytes()
                .saturating_add(module.frozen_heap().allocated_bytes());
            if used_ticks > tick_limit || used_heap > heap_limit {
                return Err(fail(format!(
                    "{label}: module evaluation resource budget exceeded"
                )));
            }
            let module = module
                .freeze()
                .map_err(|e| fail(anyhow::Error::from(e).to_string()))?;
            let used_heap = used_heap.max(module.frozen_heap().allocated_bytes());
            if used_heap > heap_limit {
                return Err(fail(format!("{label}: frozen module heap budget exceeded")));
            }
            Ok::<_, RecipeError>((module, used_ticks, used_heap))
        })?;
        ticks = ticks.saturating_add(used_ticks);
        heap_charge = heap_charge.saturating_add(used_heap);
        frozen.insert(label, module);
    }
    Ok(())
}
