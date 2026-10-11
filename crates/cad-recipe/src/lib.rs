//! Starlark is a graph-construction frontend, never a geometry evaluator.
//! All lengths in this first API are millimetres. Imports and external IO are
//! deliberately not exposed. Native functions only append declarative nodes.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use cad_build::{
    Recipe, MAX_ASSEMBLIES, MAX_FACE_ASSERTIONS, MAX_FEATURES, MAX_INSTANCES, MAX_OUTPUTS,
    MAX_PARTS,
};
use starlark::any::ProvidesStaticType;
use starlark::environment::{GlobalsBuilder, Module};
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::syntax::{AstModule, Dialect};
use starlark::values::{none::NoneType, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecipeError {
    #[error("{0}")]
    Evaluation(String),
    #[error("invalid recipe graph: {0}")]
    Graph(#[from] serde_json::Error),
}

#[derive(Default, Debug, ProvidesStaticType)]
struct GraphStore {
    features: RefCell<Vec<serde_json::Value>>,
    outputs: RefCell<Vec<serde_json::Value>>,
    parts: RefCell<Vec<serde_json::Value>>,
    instances: RefCell<BTreeMap<String, serde_json::Value>>,
    used_instances: RefCell<BTreeSet<String>>,
    assemblies: RefCell<Vec<serde_json::Value>>,
    assembly_outputs: RefCell<Vec<serde_json::Value>>,
    face_assertions: RefCell<Vec<serde_json::Value>>,
    embedded_instances: RefCell<usize>,
    overrides: BTreeMap<String, f64>,
    parameters: RefCell<BTreeMap<String, f64>>,
}

const MAX_PARAMETERS: usize = 1_000;

impl GraphStore {
    fn feature(&self, feature: serde_json::Value) -> anyhow::Result<()> {
        let mut features = self.features.borrow_mut();
        anyhow::ensure!(
            features.len() < MAX_FEATURES,
            "recipe exceeds {MAX_FEATURES} features"
        );
        features.push(feature);
        Ok(())
    }

    fn output(&self, output: serde_json::Value) -> anyhow::Result<()> {
        let mut outputs = self.outputs.borrow_mut();
        anyhow::ensure!(
            outputs.len() + self.assembly_outputs.borrow().len() < MAX_OUTPUTS,
            "recipe exceeds {MAX_OUTPUTS} outputs"
        );
        outputs.push(output);
        Ok(())
    }
}

fn store<'a>(eval: &'a Evaluator<'_, '_, '_>) -> anyhow::Result<&'a GraphStore> {
    eval.extra
        .and_then(|extra| extra.downcast_ref::<GraphStore>())
        .ok_or_else(|| anyhow::anyhow!("missing graph construction context"))
}

fn number(value: Value<'_>) -> anyhow::Result<f64> {
    let number: f64 = serde_json::from_str(&value.to_json()?)
        .map_err(|_| anyhow::anyhow!("expected a finite number in millimetres"))?;
    anyhow::ensure!(number.is_finite(), "length must be finite");
    Ok(number)
}

#[starlark_module]
fn cad_functions(builder: &mut GlobalsBuilder) {
    /// Millimetre annotation. Dimension-aware arithmetic is not implemented yet.
    fn mm(value: Value) -> anyhow::Result<f64> {
        number(value)
    }

    /// Declare a named numeric parameter. Repeated declarations must agree on
    /// their default. CLI overrides are applied before dimensions are computed.
    fn param(name: &str, default: Value, eval: &mut Evaluator) -> anyhow::Result<f64> {
        anyhow::ensure!(
            !name.is_empty() && name.trim() == name,
            "parameter name must be nonempty with no surrounding whitespace"
        );
        let default = number(default)?;
        let graph = store(eval)?;
        let mut parameters = graph.parameters.borrow_mut();
        if let Some(previous) = parameters.get(name) {
            anyhow::ensure!(
                *previous == default,
                "parameter '{name}' has conflicting defaults"
            );
        } else {
            anyhow::ensure!(
                parameters.len() < MAX_PARAMETERS,
                "recipe exceeds {MAX_PARAMETERS} parameters"
            );
            parameters.insert(name.into(), default);
        }
        Ok(graph.overrides.get(name).copied().unwrap_or(default))
    }

    fn rectangle(
        id: &str,
        width: Value,
        depth: Value,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({
            "op": "rectangle", "id": id, "op_version": 1,
            "width": number(width)?, "depth": number(depth)?,
        }))?;
        Ok(id.to_owned())
    }

    fn extrude(
        id: &str,
        profile: &str,
        height: Value,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({
            "op": "extrude", "id": id, "op_version": 1,
            "profile": profile, "height": number(height)?,
        }))?;
        Ok(id.to_owned())
    }

    fn translate(
        id: &str,
        body: &str,
        offset: Option<Value>,
        x: Option<Value>,
        y: Option<Value>,
        z: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let offset: [f64; 3] = if let Some(offset) = offset {
            anyhow::ensure!(
                x.is_none() && y.is_none() && z.is_none(),
                "use offset or x/y/z, not both"
            );
            serde_json::from_str(&offset.to_json()?)
                .map_err(|_| anyhow::anyhow!("offset must contain exactly three finite lengths"))?
        } else {
            [
                x.map(number).transpose()?.unwrap_or(0.0),
                y.map(number).transpose()?.unwrap_or(0.0),
                z.map(number).transpose()?.unwrap_or(0.0),
            ]
        };
        anyhow::ensure!(
            offset.iter().all(|x| x.is_finite()),
            "offset must contain exactly three finite lengths"
        );
        store(eval)?.feature(serde_json::json!({
            "op":"translate", "id":id, "op_version":1, "body":body, "offset":offset,
        }))?;
        Ok(id.to_owned())
    }

    fn union(id: &str, left: &str, right: &str, eval: &mut Evaluator) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({
            "op":"union", "id":id, "op_version":1, "left":left, "right":right,
        }))?;
        Ok(id.to_owned())
    }

    fn cut(id: &str, left: &str, right: &str, eval: &mut Evaluator) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({
            "op":"cut", "id":id, "op_version":1, "left":left, "right":right,
        }))?;
        Ok(id.to_owned())
    }

    fn intersect(
        id: &str,
        left: &str,
        right: &str,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({
            "op":"intersect", "id":id, "op_version":1, "left":left, "right":right,
        }))?;
        Ok(id.to_owned())
    }

    fn output(name: &str, feature: &str, eval: &mut Evaluator) -> anyhow::Result<NoneType> {
        store(eval)?.output(serde_json::json!({
            "name": name, "feature": feature,
        }))?;
        Ok(NoneType)
    }

    /// Require a semantic face reference to resolve when the graph is built.
    /// Omitted count requires one face; all=True explicitly accepts splits.
    fn assert_faces(
        id: &str,
        from_feature: &str,
        role: &str,
        in_feature: &str,
        count: Option<i32>,
        #[starlark(default = false)] all: bool,
        eval: &mut Evaluator,
    ) -> anyhow::Result<NoneType> {
        anyhow::ensure!(
            !(all && count.is_some()),
            "assert_faces accepts count or all=True, not both"
        );
        let cardinality = if all {
            serde_json::json!({"kind":"all"})
        } else {
            let count = count.unwrap_or(1);
            anyhow::ensure!(
                (1..=100_000).contains(&count),
                "assert_faces count must be an integer from 1 to 100000"
            );
            serde_json::json!({"kind":"exactly","count":count})
        };
        let graph = store(eval)?;
        let mut assertions = graph.face_assertions.borrow_mut();
        anyhow::ensure!(
            assertions.len() < MAX_FACE_ASSERTIONS,
            "recipe exceeds {MAX_FACE_ASSERTIONS} face assertions"
        );
        assertions.push(serde_json::json!({"id":id,"selector":{
            "anchor_feature":from_feature,"role":role,"target_feature":in_feature,"cardinality":cardinality,
        }}));
        Ok(NoneType)
    }

    /// Give a solid feature a reusable part-definition identity.
    fn part(id: &str, body: &str, eval: &mut Evaluator) -> anyhow::Result<String> {
        let graph = store(eval)?;
        let mut parts = graph.parts.borrow_mut();
        anyhow::ensure!(parts.len() < MAX_PARTS, "recipe exceeds {MAX_PARTS} parts");
        parts.push(serde_json::json!({"id":id,"feature":body}));
        Ok(id.to_owned())
    }

    /// Instance placement does not modify or duplicate its part's exact B-rep.
    /// Rotation is a unit quaternion [x,y,z,w]; offset is in millimetres.
    fn instance(
        id: &str,
        part: &str,
        offset: Option<Value>,
        rotation: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let translation: [f64; 3] = match offset {
            Some(value) => serde_json::from_str(&value.to_json()?).map_err(|_| {
                anyhow::anyhow!("instance offset must contain exactly three finite lengths")
            })?,
            None => [0.0; 3],
        };
        let rotation: [f64; 4] = match rotation {
            Some(value) => serde_json::from_str(&value.to_json()?).map_err(|_| {
                anyhow::anyhow!("instance rotation must be a unit quaternion [x,y,z,w]")
            })?,
            None => [0.0, 0.0, 0.0, 1.0],
        };
        let transform = cad_build::RigidTransform {
            translation,
            rotation,
        };
        transform
            .validate()
            .map_err(|error| anyhow::anyhow!("instance '{id}': {error}"))?;
        let graph = store(eval)?;
        let mut instances = graph.instances.borrow_mut();
        anyhow::ensure!(
            instances.len() < MAX_INSTANCES,
            "recipe exceeds {MAX_INSTANCES} instances"
        );
        anyhow::ensure!(!instances.contains_key(id), "duplicate instance ID '{id}'");
        instances.insert(
            id.to_owned(),
            serde_json::json!({"id":id,"part":part,"transform":transform}),
        );
        Ok(id.to_owned())
    }

    /// Collect named instances into a flat assembly. Nested assemblies are a
    /// future schema extension; each current instance references a part.
    fn assembly(id: &str, instances: Value, eval: &mut Evaluator) -> anyhow::Result<String> {
        let ids: Vec<String> = serde_json::from_str(&instances.to_json()?)
            .map_err(|_| anyhow::anyhow!("assembly instances must be a list of instance IDs"))?;
        let graph = store(eval)?;
        let mut assemblies = graph.assemblies.borrow_mut();
        anyhow::ensure!(
            assemblies.len() < MAX_ASSEMBLIES,
            "recipe exceeds {MAX_ASSEMBLIES} assemblies"
        );
        let count = graph.embedded_instances.borrow().saturating_add(ids.len());
        anyhow::ensure!(
            count <= MAX_INSTANCES,
            "recipe exceeds {MAX_INSTANCES} assembly instances"
        );
        let records = graph.instances.borrow();
        let mut seen = BTreeSet::new();
        let mut embedded = Vec::with_capacity(ids.len());
        for instance in &ids {
            anyhow::ensure!(
                seen.insert(instance),
                "assembly '{id}' repeats instance ID '{instance}'"
            );
            embedded.push(
                records
                    .get(instance)
                    .ok_or_else(|| {
                        anyhow::anyhow!("assembly '{id}' references unknown instance '{instance}'")
                    })?
                    .clone(),
            );
        }
        graph.used_instances.borrow_mut().extend(ids);
        *graph.embedded_instances.borrow_mut() = count;
        assemblies.push(serde_json::json!({"id":id,"instances":embedded}));
        Ok(id.to_owned())
    }

    fn assembly_output(
        name: &str,
        assembly: &str,
        eval: &mut Evaluator,
    ) -> anyhow::Result<NoneType> {
        let graph = store(eval)?;
        let mut outputs = graph.assembly_outputs.borrow_mut();
        anyhow::ensure!(
            outputs.len() + graph.outputs.borrow().len() < MAX_OUTPUTS,
            "recipe exceeds {MAX_OUTPUTS} outputs"
        );
        outputs.push(serde_json::json!({"name":name,"assembly":assembly}));
        Ok(NoneType)
    }
}

/// Evaluate a recipe into a versioned graph without constructing any geometry.
/// Interpreter limits are best effort; this is not a security sandbox.
pub fn from_starlark(filename: &str, source: &str) -> Result<Recipe, RecipeError> {
    from_starlark_with_params(filename, source, &BTreeMap::new())
}

/// Evaluate with explicit numeric parameter overrides. Unknown overrides are
/// errors so a misspelled engineering dimension cannot silently do nothing.
pub fn from_starlark_with_params(
    filename: &str,
    source: &str,
    overrides: &BTreeMap<String, f64>,
) -> Result<Recipe, RecipeError> {
    for (name, value) in overrides {
        if name.is_empty() || name.trim() != name || !value.is_finite() {
            return Err(RecipeError::Evaluation(format!(
                "{filename}: override '{name}' must have a nonempty name and finite numeric value"
            )));
        }
    }
    let ast = AstModule::parse(filename, source.to_owned(), &Dialect::Standard)
        .map_err(|err| RecipeError::Evaluation(err.to_string()))?;
    let globals = GlobalsBuilder::standard().with(cad_functions).build();
    let graph = GraphStore {
        overrides: overrides.clone(),
        ..GraphStore::default()
    };
    Module::with_temp_heap(|module| {
        let mut evaluator = Evaluator::new(&module);
        evaluator.extra = Some(&graph);
        evaluator
            .set_max_callstack_size(64)
            .map_err(|err| RecipeError::Evaluation(err.to_string()))?;
        evaluator
            .set_max_heap_size(16 * 1024 * 1024)
            .map_err(|err| RecipeError::Evaluation(err.to_string()))?;
        evaluator
            .set_max_tick_count(1_000_000)
            .map_err(|err| RecipeError::Evaluation(err.to_string()))?;
        evaluator
            .eval_module(ast, &globals)
            .map_err(|err| RecipeError::Evaluation(err.to_string()))?;
        Ok::<_, RecipeError>(())
    })?;
    let unused: Vec<_> = overrides
        .keys()
        .filter(|name| !graph.parameters.borrow().contains_key(*name))
        .collect();
    if !unused.is_empty() {
        return Err(RecipeError::Evaluation(format!(
            "{filename}: unused parameter overrides: {}",
            unused
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let unused_instances: Vec<_> = graph
        .instances
        .borrow()
        .keys()
        .filter(|id| !graph.used_instances.borrow().contains(*id))
        .cloned()
        .collect();
    if !unused_instances.is_empty() {
        return Err(RecipeError::Evaluation(format!(
            "{filename}: instances not included in an assembly: {}",
            unused_instances.join(", ")
        )));
    }
    let parts = graph.parts.into_inner();
    let assemblies = graph.assemblies.into_inner();
    let assembly_outputs = graph.assembly_outputs.into_inner();
    let face_assertions = graph.face_assertions.into_inner();
    let assembly_schema =
        !parts.is_empty() || !assemblies.is_empty() || !assembly_outputs.is_empty();
    let mut value = serde_json::json!({
        "schema_version": if assembly_schema || !face_assertions.is_empty() { 2 } else { 1 },
        "units": "mm",
        "features": graph.features.into_inner(),
        "outputs": graph.outputs.into_inner(),
    });
    if assembly_schema {
        value["parts"] = serde_json::json!(parts);
        value["assemblies"] = serde_json::json!(assemblies);
        value["assembly_outputs"] = serde_json::json!(assembly_outputs);
    }
    if !face_assertions.is_empty() {
        value["face_assertions"] = serde_json::json!(face_assertions);
    }
    Ok(serde_json::from_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_and_parameters_build_real_geometry() {
        let recipe = from_starlark(
            "plate.star",
            r#"
def plate(prefix, width):
    profile = rectangle(id = prefix + "/outline", width = mm(width), depth = mm(40))
    return extrude(id = prefix + "/blank", profile = profile, height = mm(6))

output(name = "plate", feature = plate("plate", 80))
"#,
        )
        .unwrap();
        let built = cad_build::build(&recipe).unwrap();
        assert!((built.outputs["plate"].result.model.volume() - 19_200.0).abs() < 1e-8);
    }

    #[test]
    fn errors_identify_source_file() {
        let error = from_starlark(
            "broken.star",
            "rectangle(id = \"bad\", width = \"wide\", depth = 5)",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("broken.star"), "{error}");
    }

    #[test]
    fn arbitrary_imports_are_unavailable() {
        assert!(from_starlark("unsafe.star", "load(\"/etc/passwd\", \"password\")").is_err());
    }

    #[test]
    fn host_graph_allocations_are_bounded() {
        let error = from_starlark(
            "large.star",
            r#"
def lots_of_features():
    for i in range(10001):
        rectangle(id = str(i), width = 1, depth = 2)
lots_of_features()
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("exceeds 10000 features"), "{error}");
    }

    #[test]
    fn equivalent_starlark_and_json_have_identical_hashes() {
        let recipe = from_starlark(
            "plate.star",
            r#"
p = rectangle(id = "p", width = 4, depth = 5)
s = extrude(id = "s", profile = p, height = 6)
output(name = "part", feature = s)
"#,
        )
        .unwrap();
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        assert_eq!(
            cad_build::build(&recipe).unwrap().feature_hashes,
            cad_build::build(&decoded).unwrap().feature_hashes
        );
    }
}

#[cfg(test)]
mod extension_tests {
    use super::*;

    const PARAMETRIC: &str = r#"
width = param(name = "width", default = 10)
shift = param(name = "shift", default = 5)
p = rectangle(id = "profile", width = width, depth = 10)
a = extrude(id = "box", profile = p, height = 10)
b = translate(id = "moved", body = a, offset = [shift, 0, 0])
output(name = "union", feature = union(id = "union", left = a, right = b))
output(name = "cut", feature = cut(id = "cut", left = a, right = b))
output(name = "intersection", feature = intersect(id = "intersection", left = a, right = b))
"#;

    #[test]
    fn starlark_boolean_pipeline_and_overrides_build_expected_geometry() {
        let baseline = cad_build::build(&from_starlark("parts.star", PARAMETRIC).unwrap()).unwrap();
        assert_eq!(baseline.outputs["union"].result.model.volume(), 1500.0);
        assert_eq!(baseline.outputs["cut"].result.model.volume(), 500.0);
        assert_eq!(
            baseline.outputs["intersection"].result.model.volume(),
            500.0
        );
        let overrides = BTreeMap::from([("width".into(), 12.0), ("shift".into(), 2.5)]);
        let recipe = from_starlark_with_params("parts.star", PARAMETRIC, &overrides).unwrap();
        let changed = cad_build::build(&recipe).unwrap();
        assert_eq!(changed.outputs["union"].result.model.volume(), 1450.0);
        assert_eq!(changed.outputs["cut"].result.model.volume(), 250.0);
        assert_eq!(changed.outputs["intersection"].result.model.volume(), 950.0);
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        assert_eq!(
            changed.feature_hashes,
            cad_build::build(&decoded).unwrap().feature_hashes
        );
    }

    #[test]
    fn parameter_typos_nonfinite_values_and_conflicting_defaults_are_errors() {
        let unknown = BTreeMap::from([("widht".into(), 12.0)]);
        let error = from_starlark_with_params("params.star", PARAMETRIC, &unknown)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("widht") && error.contains("unused") && error.contains("params.star"),
            "{error}"
        );
        let nonfinite = BTreeMap::from([("width".into(), f64::NAN)]);
        assert!(from_starlark_with_params("params.star", PARAMETRIC, &nonfinite).is_err());
        assert!(from_starlark(
            "params.star",
            "param(name='width',default=1)\nparam(name='width',default=2)"
        )
        .is_err());
        assert!(from_starlark("params.star", "param(name='',default=1)").is_err());
        assert!(from_starlark("params.star", "param(name='width',default='large')").is_err());
        assert!(from_starlark(
            "params.star",
            "param(name='width',default=1)\nparam(name='width',default=1)"
        )
        .is_ok());
    }

    #[test]
    fn translate_requires_exactly_three_numeric_coordinates() {
        for offset in ["[1,2]", "[1,2,3,4]", "['x',0,0]", "None"] {
            let source = format!("translate(id='move',body='box',offset={offset})");
            let error = from_starlark("move.star", &source).unwrap_err().to_string();
            assert!(error.contains("move.star"), "{error}");
        }
    }

    #[test]
    fn translate_axis_arguments_match_offset_and_cannot_be_mixed() {
        let axis =
            from_starlark("axes.star", "translate(id='move',body='box',x=0.5,z=-2)").unwrap();
        let offset = from_starlark(
            "offset.star",
            "translate(id='move',body='box',offset=[0.5,0,-2])",
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(axis).unwrap(),
            serde_json::to_value(offset).unwrap()
        );
        assert!(from_starlark(
            "mixed.star",
            "translate(id='move',body='box',offset=[0,0,0],x=1)"
        )
        .is_err());
    }
}

#[cfg(test)]
mod assembly_tests {
    use super::*;
    const SOURCE: &str = r#"
shift = param(name="shift", default=100.5)
profile = rectangle(id="profile", width=10, depth=20)
body = extrude(id="body", profile=profile, height=3)
plate = part(id="plate", body=body)
a = instance(id="a", part=plate)
b = instance(id="b", part=plate, offset=[shift,0,0], rotation=[0,0,1,0])
assembly_output(name="pair", assembly=assembly(id="pair", instances=[a,b]))
"#;

    #[test]
    fn assembly_starlark_builds_shared_parts_and_parameterized_placements() {
        let recipe = from_starlark("pair.star", SOURCE).unwrap();
        assert_eq!(recipe.schema_version, 2);
        assert!(recipe.outputs.is_empty());
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        let mut engine = cad_build::BuildEngine::default();
        let initial = engine.build(&decoded).unwrap();
        let pair = &initial.assemblies["pair"];
        assert_eq!(pair.parts.len(), 1);
        assert_eq!(pair.material_volume(), 1200.0);
        assert_eq!(pair.bounds().unwrap().min, [0.0, -20.0, 0.0]);
        assert_eq!(pair.bounds().unwrap().max, [100.5, 20.0, 3.0]);
        let moved = from_starlark_with_params(
            "pair.star",
            SOURCE,
            &BTreeMap::from([("shift".into(), 200.25)]),
        )
        .unwrap();
        let changed = engine.build(&moved).unwrap();
        assert_eq!(changed.stats.computed_solids, 0);
        assert_eq!(initial.feature_hashes, changed.feature_hashes);
        assert_ne!(pair.content_hash, changed.assemblies["pair"].content_hash);
    }

    #[test]
    fn assembly_host_rejects_invalid_or_discarded_instances() {
        for source in [
            "instance(id='i',part='p')",
            "instance(id='i',part='p',offset=[1,2])",
            "instance(id='i',part='p',rotation=[0,0,0,0])",
            "instance(id='i',part='p',rotation=[0,0,1])",
            "instance(id='i',part='p')\ninstance(id='i',part='p')",
            "assembly(id='a',instances=['missing'])",
            "i=instance(id='i',part='p')\nassembly(id='a',instances=[i,i])",
        ] {
            let error = from_starlark("bad-assembly.star", source)
                .unwrap_err()
                .to_string();
            assert!(error.contains("bad-assembly.star"), "{error}");
        }
    }

    #[test]
    fn solid_recipes_keep_schema_one_and_assembly_collections_are_bounded() {
        let recipe = from_starlark("solid.star", "rectangle(id='p',width=1,depth=2)").unwrap();
        assert_eq!(recipe.schema_version, 1);
        assert!(serde_json::to_value(recipe).unwrap().get("parts").is_none());
        let source =
            "def many():\n    for i in range(10001):\n        part(id=str(i),body='body')\nmany()";
        let error = from_starlark("limit.star", source).unwrap_err().to_string();
        assert!(error.contains("10000 parts"), "{error}");
    }
}

#[cfg(test)]
mod assertion_tests {
    use super::*;

    const MODEL: &str = r#"
p = rectangle(id='p',width=10,depth=8)
base = extrude(id='base',profile=p,height=3)
moved = translate(id='moved',body=base,offset=[20,0,0])
q = rectangle(id='q',width=2,depth=10)
c = extrude(id='c',profile=q,height=5)
c = translate(id='positioned',body=c,offset=[4,-1,-1])
split = cut(id='split',left=base,right=c)
output(name='part',feature=moved)
"#;

    #[test]
    fn semantic_assertions_emit_schema_two_and_build_roundtrip() {
        let source=format!("{MODEL}\nassert_faces(id='cap',from_feature=base,role='end_cap',in_feature=moved)\nassert_faces(id='split_cap',from_feature=base,role='end_cap',in_feature=split,count=2)\nassert_faces(id='all_walls',from_feature=base,role='walls',in_feature=moved,all=True)\n");
        let recipe = from_starlark("assertions.star", &source).unwrap();
        assert_eq!(recipe.schema_version, 2);
        let raw = serde_json::to_value(&recipe).unwrap();
        assert_eq!(
            raw["face_assertions"][0]["selector"]["cardinality"],
            serde_json::json!({"kind":"exactly","count":1})
        );
        assert_eq!(
            raw["face_assertions"][2]["selector"]["cardinality"],
            serde_json::json!({"kind":"all"})
        );
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let roundtrip = cad_build::build(&decoded).unwrap();
        assert_eq!(built.face_assertions.len(), 3);
        assert_eq!(built.face_assertions["cap"].witnesses.len(), 1);
        assert_eq!(built.face_assertions["split_cap"].witnesses.len(), 2);
        assert_eq!(built.face_assertions["all_walls"].witnesses.len(), 4);
        assert_eq!(
            serde_json::to_value(&built.face_assertions).unwrap(),
            serde_json::to_value(&roundtrip.face_assertions).unwrap()
        );
    }

    #[test]
    fn unresolved_assertions_fail_geometry_build_instead_of_silently_selecting() {
        let source=format!("{MODEL}\nassert_faces(id='must_be_one',from_feature=base,role='end_cap',in_feature=split)\n");
        let recipe = from_starlark("ambiguous.star", &source).unwrap();
        match cad_build::build(&recipe).unwrap_err() {
            cad_build::BuildError::FaceAssertionFailed {
                assertion_id,
                resolution,
                ..
            } => {
                assert_eq!(assertion_id, "must_be_one");
                assert_eq!(
                    resolution.status,
                    cad_build::FaceResolutionStatus::Ambiguous
                );
                assert_eq!(resolution.witnesses.len(), 2);
            }
            error => panic!("unexpected failure: {error}"),
        }
        let duplicate=format!("{MODEL}\nassert_faces(id='duplicate',from_feature=base,role='end_cap',in_feature=moved)\nassert_faces(id='duplicate',from_feature=base,role='end_cap',in_feature=moved)\n");
        let recipe = from_starlark("duplicate.star", &duplicate).unwrap();
        assert!(matches!(
            cad_build::build(&recipe),
            Err(cad_build::BuildError::InvalidFaceAssertion { .. })
        ));
    }

    #[test]
    fn assertion_arguments_are_strict_and_host_allocations_bounded() {
        for arguments in [
            "count=0",
            "count=-1",
            "count=100001",
            "count=1.5",
            "count=1.0",
            "count=True",
            "count=None",
            "count='1'",
            "all=1",
            "all='yes'",
            "count=1,all=True",
        ] {
            let source=format!("assert_faces(id='a',from_feature='base',role='end_cap',in_feature='moved',{arguments})");
            let error = from_starlark("invalid-assertion.star", &source)
                .expect_err(arguments)
                .to_string();
            assert!(error.contains("invalid-assertion.star"), "{error}");
        }
        let source="def many():\n    for i in range(1001):\n        assert_faces(id=str(i),from_feature='base',role='end_cap',in_feature='base')\nmany()";
        let error = from_starlark("bounded-assertions.star", source)
            .unwrap_err()
            .to_string();
        assert!(error.contains("1000 face assertions"), "{error}");
        assert_eq!(
            from_starlark("legacy.star", MODEL).unwrap().schema_version,
            1
        );
    }
}
