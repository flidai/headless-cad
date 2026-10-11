//! Starlark is a graph-construction frontend, never a geometry evaluator.
//! All lengths in this first API are millimetres. Imports and external IO are
//! deliberately not exposed. Native functions only append declarative nodes.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use cad_build::{
    Recipe, MAX_ASSEMBLIES, MAX_FACE_ASSERTIONS, MAX_FEATURES, MAX_INSTANCES, MAX_OUTPUTS,
    MAX_PARTS, MAX_POLYGON_HOLES, MAX_POLYGON_POINTS,
};
use starlark::any::ProvidesStaticType;
use starlark::environment::{GlobalsBuilder, Module};
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::syntax::{AstModule, Dialect};
use starlark::values::{none::NoneType, Heap, Value};
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
    face_datums: RefCell<Vec<serde_json::Value>>,
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

fn frame(
    offset: Option<Value>,
    rotation: Option<Value>,
) -> anyhow::Result<cad_build::RigidTransform> {
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
        .map_err(|error| anyhow::anyhow!("invalid datum/placement frame: {error}"))?;
    Ok(transform)
}

fn register_instance(
    id: &str,
    reference_key: &str,
    reference: &str,
    attachment: Option<cad_build::DatumAttachment>,
    offset: Option<Value>,
    rotation: Option<Value>,
    eval: &mut Evaluator,
) -> anyhow::Result<String> {
    let transform = frame(offset, rotation)?;
    let graph = store(eval)?;
    let mut instances = graph.instances.borrow_mut();
    anyhow::ensure!(
        instances.len() < MAX_INSTANCES,
        "recipe exceeds {MAX_INSTANCES} instances"
    );
    anyhow::ensure!(!instances.contains_key(id), "duplicate instance ID '{id}'");
    let mut record = serde_json::json!({"id":id,reference_key:reference,"transform":transform});
    if let Some(attachment) = attachment {
        record["attachment"] = serde_json::to_value(attachment)?;
    }
    instances.insert(id.to_owned(), record);
    Ok(id.to_owned())
}

#[starlark_module]
fn cad_functions(builder: &mut GlobalsBuilder) {
    /// Millimetre annotation. Dimension-aware arithmetic is not implemented yet.
    fn mm(value: Value) -> anyhow::Result<f64> {
        number(value)
    }

    /// Simple counter-clockwise outer ring and optional clockwise convex holes.
    /// Rings omit their repeated closing point; hole order is semantic.
    fn polygon(
        id: &str,
        points: Value,
        holes: Option<Value>,
        circular_holes: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let points: Vec<[f64; 2]> = serde_json::from_str(&points.to_json()?)
            .map_err(|_| anyhow::anyhow!("polygon points must be pairs of finite coordinates"))?;
        anyhow::ensure!(
            (3..=MAX_POLYGON_POINTS).contains(&points.len()),
            "polygon requires 3 to {MAX_POLYGON_POINTS} points"
        );
        anyhow::ensure!(
            points.iter().flatten().all(|value| value.is_finite()),
            "polygon coordinates must be finite"
        );
        let holes: Vec<Vec<[f64; 2]>> = match holes {
            Some(value) => serde_json::from_str(&value.to_json()?)
                .map_err(|_| anyhow::anyhow!("polygon holes must be lists of coordinate pairs"))?,
            None => vec![],
        };
        anyhow::ensure!(holes.len() <= MAX_POLYGON_HOLES, "too many polygon holes");
        anyhow::ensure!(
            holes.iter().all(|hole| hole.len() >= 3),
            "each polygon hole requires at least three points"
        );
        let total = holes
            .iter()
            .fold(points.len(), |n, hole| n.saturating_add(hole.len()));
        anyhow::ensure!(
            total <= MAX_POLYGON_POINTS,
            "polygon exceeds total point budget"
        );
        anyhow::ensure!(
            holes.iter().flatten().flatten().all(|v| v.is_finite()),
            "polygon hole coordinates must be finite"
        );
        let circular_holes: Vec<cad_build::CircleHole> = match circular_holes {
            Some(value) => serde_json::from_str(&value.to_json()?).map_err(|_| {
                anyhow::anyhow!("circular_holes must be objects with center:[x,y] and radius")
            })?,
            None => vec![],
        };
        anyhow::ensure!(
            holes.len() + circular_holes.len() <= MAX_POLYGON_HOLES,
            "too many profile holes"
        );
        anyhow::ensure!(
            circular_holes
                .iter()
                .all(|h| h.radius.is_finite() && h.center.iter().all(|v| v.is_finite())),
            "circular hole values must be finite"
        );
        store(eval)?.feature(serde_json::json!({"op":"polygon","id":id,"points":points,"holes":holes,"circular_holes":circular_holes,"op_version":1}))?;
        Ok(id.to_owned())
    }

    /// Declare a numeric parameter. Repeated declarations must agree on their
    /// default. CLI overrides are applied before dimensions are computed.
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

    fn circle(
        id: &str,
        radius: Value,
        inner_radius: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({"op": "circle", "id": id, "op_version": 1, "radius": number(radius)?, "inner_radius": inner_radius.map(number).transpose()?.unwrap_or(0.)}))?;
        Ok(id.to_owned())
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

    fn revolve(id: &str, profile: &str, eval: &mut Evaluator) -> anyhow::Result<String> {
        store(eval)?.feature(serde_json::json!({
            "op": "revolve", "id": id, "op_version": 1, "profile": profile,
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

    /// Apply a native proper rigid transform; this produces a new solid feature.
    fn transform(
        id: &str,
        body: &str,
        offset: Option<Value>,
        rotation: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let frame = frame(offset, rotation)?;
        store(eval)?.feature(serde_json::json!({
            "op":"transform", "id":id, "op_version":1, "body":body,
            "offset":frame.translation, "rotation":frame.rotation,
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

    /// A named local rigid frame; attach it to a part with part(datums={...}).
    fn datum<'v>(
        offset: Option<Value<'v>>,
        rotation: Option<Value<'v>>,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(heap.alloc(serde_json::to_value(frame(offset, rotation)?)?))
    }

    /// A semantic face reference, resolved against the owning part's solid.
    fn face_datum<'v>(
        from_feature: &str,
        role: &str,
        x_direction: Value<'v>,
        offset: Option<Value<'v>>,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        let x_direction: [f64; 3] = serde_json::from_str(&x_direction.to_json()?)
            .map_err(|_| anyhow::anyhow!("x_direction must contain three finite numbers"))?;
        let offset: [f64; 3] = match offset {
            Some(value) => serde_json::from_str(&value.to_json()?)?,
            None => [0.0; 3],
        };
        anyhow::ensure!(
            x_direction.iter().all(|v| v.is_finite()) && x_direction.iter().any(|v| *v != 0.0),
            "x_direction must be finite and nonzero"
        );
        anyhow::ensure!(
            offset.iter().all(|v| v.is_finite()),
            "offset must be finite"
        );
        anyhow::ensure!(
            !from_feature.is_empty()
                && from_feature.trim() == from_feature
                && !role.is_empty()
                && role.trim() == role,
            "feature and role must be nonempty identifiers"
        );
        Ok(heap.alloc(serde_json::json!({"kind":"face_datum","anchor_feature":from_feature,"role":role,"x_direction":x_direction,"offset":offset})))
    }

    /// Give a solid feature a reusable part-definition identity.
    #[allow(clippy::too_many_arguments)]
    fn part(
        id: &str,
        body: &str,
        part_number: Option<&str>,
        revision: Option<&str>,
        description: Option<&str>,
        material: Option<&str>,
        density_kg_per_m3: Option<Value>,
        datums: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let material = match (material, density_kg_per_m3) {
            (None, None) => None,
            (Some(name), Some(density)) => Some(cad_build::Material {
                name: name.to_owned(),
                density_kg_per_m3: number(density).map_err(|_| {
                    anyhow::anyhow!("part density_kg_per_m3 must be a finite number")
                })?,
            }),
            _ => anyhow::bail!("part '{id}' requires material and density_kg_per_m3 together"),
        };
        let metadata = cad_build::PartMetadata {
            part_number: part_number.map(str::to_owned),
            revision: revision.map(str::to_owned),
            description: description.map(str::to_owned),
            material,
        };
        metadata
            .validate()
            .map_err(|error| anyhow::anyhow!("part '{id}': {error}"))?;
        let values: BTreeMap<String, serde_json::Value> = match datums {
            Some(value) => serde_json::from_str(&value.to_json()?).map_err(|error| {
                anyhow::anyhow!("part '{id}' datums must map names to frames: {error}")
            })?,
            None => BTreeMap::new(),
        };
        anyhow::ensure!(values.len() <= 1024, "part exceeds 1024 datum frames");
        let mut datums = BTreeMap::new();
        let graph = store(eval)?;
        for (name, mut value) in values {
            anyhow::ensure!(
                !name.is_empty() && name.trim() == name,
                "datum names must be nonempty without surrounding whitespace"
            );
            if value.get("kind").and_then(serde_json::Value::as_str) == Some("face_datum") {
                let object = value.as_object_mut().unwrap();
                object.remove("kind");
                object.insert("part".into(), id.into());
                object.insert("name".into(), name.into());
                let definition: cad_build::FaceDatumDefinition = serde_json::from_value(value)?;
                let mut references = graph.face_datums.borrow_mut();
                anyhow::ensure!(
                    references.len() < cad_build::MAX_FACE_DATUMS,
                    "recipe exceeds face datum limit"
                );
                references.push(serde_json::to_value(definition)?);
            } else {
                datums.insert(
                    name,
                    serde_json::from_value::<cad_build::RigidTransform>(value)?,
                );
            }
        }
        cad_build::validate_datums(&datums)
            .map_err(|error| anyhow::anyhow!("part '{id}': {error}"))?;
        let mut parts = graph.parts.borrow_mut();
        anyhow::ensure!(parts.len() < MAX_PARTS, "recipe exceeds {MAX_PARTS} parts");
        let mut definition = serde_json::json!({"id":id,"feature":body});
        if metadata != cad_build::PartMetadata::default() {
            definition["metadata"] = serde_json::to_value(metadata)?;
        }
        if !datums.is_empty() {
            definition["datums"] = serde_json::to_value(datums)?;
        }
        parts.push(definition);
        Ok(id.to_owned())
    }

    /// Instance placement does not modify or duplicate its part's exact B-rep.
    /// Rotation is a unit quaternion [x,y,z,w]; offset is in millimetres.
    #[allow(clippy::too_many_arguments)]
    fn instance(
        id: &str,
        part: &str,
        offset: Option<Value>,
        rotation: Option<Value>,
        attach_to: Option<&str>,
        datum: Option<&str>,
        target_datum: Option<&str>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let attachment = match (attach_to, datum, target_datum) {
            (None, None, None) => None,
            (Some(instance), Some(datum), Some(target_datum)) => Some(cad_build::DatumAttachment {
                instance: instance.into(),
                datum: datum.into(),
                target_datum: target_datum.into(),
            }),
            _ => {
                anyhow::bail!("instance '{id}' requires attach_to, datum and target_datum together")
            }
        };
        register_instance(id, "part", part, attachment, offset, rotation, eval)
    }

    /// Place an assembly definition without duplicating its parts or geometry.
    fn assembly_instance(
        id: &str,
        assembly: &str,
        offset: Option<Value>,
        rotation: Option<Value>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        register_instance(id, "assembly", assembly, None, offset, rotation, eval)
    }

    /// Collect part and nested assembly instances into a reusable definition.
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
        let mut embedded = Vec::new();
        let mut nested = Vec::new();
        for instance in &ids {
            anyhow::ensure!(
                seen.insert(instance),
                "assembly '{id}' repeats instance ID '{instance}'"
            );
            let record = records
                .get(instance)
                .ok_or_else(|| {
                    anyhow::anyhow!("assembly '{id}' references unknown instance '{instance}'")
                })?
                .clone();
            if record.get("assembly").is_some() {
                nested.push(record);
            } else {
                embedded.push(record);
            }
        }
        graph.used_instances.borrow_mut().extend(ids);
        *graph.embedded_instances.borrow_mut() = count;
        let mut definition = serde_json::json!({"id":id,"instances":embedded});
        if !nested.is_empty() {
            definition["assemblies"] = serde_json::json!(nested);
        }
        assemblies.push(definition);
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
    let face_datums = graph.face_datums.into_inner();
    let assembly_schema =
        !parts.is_empty() || !assemblies.is_empty() || !assembly_outputs.is_empty();
    let mut value = serde_json::json!({
        "schema_version": if assembly_schema || !face_assertions.is_empty() || !face_datums.is_empty() { 2 } else { 1 },
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
    if !face_datums.is_empty() {
        value["face_datums"] = serde_json::json!(face_datums);
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

#[cfg(test)]
mod nested_assembly_tests {
    use super::*;
    const NESTED: &str = r#"
gap = param(name="gap", default=30)
p = rectangle(id="p", width=2, depth=3)
b = extrude(id="b", profile=p, height=4)
d = part(id="block", body=b)
leaf = assembly(id="module", instances=[instance(id="bolt", part=d, offset=[1,0,0])])
mid = assembly(id="mid", instances=[assembly_instance(id="inner", assembly=leaf, offset=[0,5,0], rotation=[0,0,1,0])])
a = assembly_instance(id="left", assembly=mid)
b = assembly_instance(id="right", assembly=mid, offset=[gap,0,0])
assembly_output(name="pair", assembly=assembly(id="pair",instances=[a,b]))
"#;
    #[test]
    fn nested_starlark_roundtrip_keeps_paths_and_placement_only_cache_hits() {
        let recipe = from_starlark("nested.star", NESTED).unwrap();
        assert_eq!(recipe.schema_version, 2);
        assert_eq!(recipe.assemblies[2].assemblies.len(), 2);
        assert!(recipe.assemblies[2].instances.is_empty());
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        let mut engine = cad_build::BuildEngine::default();
        let first = engine.build(&decoded).unwrap();
        let assembly = &first.assemblies["pair"];
        assert_eq!(assembly.instance_count(), 2);
        assert_eq!(assembly.parts.len(), 1);
        assert_eq!(assembly.material_volume(), 48.0);
        assert_eq!(assembly.bounds().unwrap().min, [-3.0, 2.0, 0.0]);
        assert_eq!(assembly.bounds().unwrap().max, [29.0, 5.0, 4.0]);
        let modified = from_starlark_with_params(
            "nested.star",
            NESTED,
            &BTreeMap::from([("gap".into(), 40.0)]),
        )
        .unwrap();
        let moved = engine.build(&modified).unwrap();
        assert_eq!(moved.stats.computed_solids, 0);
        assert_eq!(first.feature_hashes, moved.feature_hashes);
        assert_ne!(assembly.content_hash, moved.assemblies["pair"].content_hash);
    }
    #[test]
    fn nested_instance_registry_rejects_bad_shapes_duplicates_and_discarded_records() {
        for source in [
            "assembly_instance(id='a',assembly='module')",
            "assembly_instance(id='a',assembly='module',offset=[])",
            "assembly_instance(id='a',assembly='module',rotation=[0,0,0,0])",
            "instance(id='a',part='p')\nassembly_instance(id='a',assembly='module')",
        ] {
            assert!(from_starlark("bad.star", source).is_err(), "{source}");
        }
        let recipe = from_starlark(
            "missing.star",
            "a=assembly_instance(id='a',assembly='missing')\nassembly(id='root',instances=[a])",
        )
        .unwrap();
        assert!(cad_build::build(&recipe).is_err());
        let recipe = from_starlark(
            "cycle.star",
            "a=assembly_instance(id='a',assembly='root')\nassembly(id='root',instances=[a])",
        )
        .unwrap();
        assert!(cad_build::build(&recipe).is_err());
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    const SOURCE: &str = "p=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\nd=part(id='part',body=b,part_number='P-01',revision='A',description='Support',material='Steel',density_kg_per_m3=7850)\ni=instance(id='leaf',part=d)\na=assembly(id='a',instances=[i])\nassembly_output(name='a',assembly=a)\n";

    #[test]
    fn part_metadata_roundtrips_and_edits_reuse_geometry() {
        let recipe = from_starlark("material.star", SOURCE).unwrap();
        assert_eq!(
            recipe.parts[0].metadata.part_number.as_deref(),
            Some("P-01")
        );
        assert_eq!(
            recipe.parts[0]
                .metadata
                .material
                .as_ref()
                .unwrap()
                .density_kg_per_m3,
            7850.0
        );
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        let mut engine = cad_build::BuildEngine::default();
        let first = engine.build(&decoded).unwrap();
        let revised = from_starlark(
            "material.star",
            &SOURCE
                .replace("revision='A'", "revision='B'")
                .replace("7850", "2700"),
        )
        .unwrap();
        let next = engine.build(&revised).unwrap();
        assert_eq!(next.stats.computed_solids, 0);
        assert_eq!(first.feature_hashes, next.feature_hashes);
        assert_ne!(
            first.assemblies["a"].content_hash,
            next.assemblies["a"].content_hash
        );
    }

    #[test]
    fn metadata_dsl_rejects_incomplete_pairs_bad_strings_and_invalid_density() {
        for args in [
            "material='Steel'",
            "density_kg_per_m3=7850",
            "material='Steel',density_kg_per_m3=0",
            "material='Steel',density_kg_per_m3=-1",
            "material='',density_kg_per_m3=1",
            "material='Steel',density_kg_per_m3='heavy'",
            "material='Steel',density_kg_per_m3=True",
            "part_number=''",
            "revision=' A'",
            "description=''",
            "part_number=42",
            "part_nubmer='typo'",
        ] {
            let source = format!("part(id='part',body='body',{args})");
            assert!(
                from_starlark("bad-metadata.star", &source).is_err(),
                "accepted {args}"
            );
        }
    }
}

#[cfg(test)]
mod polygon_tests {
    use super::*;
    #[test]
    fn parameterized_polygon_recipe_builds_and_keeps_semantic_roles() {
        let source="w=param(name='width',default=4)\np=polygon(id='p',points=[[0,0],[w,0],[1,3]])\nb=extrude(id='b',profile=p,height=2)\nm=translate(id='m',body=b,offset=[0.5,1e-3,1])\nassert_faces(id='wall',from_feature=b,role='wall_0',in_feature=m)\noutput(name='prism',feature=m)";
        let recipe = from_starlark("triangle.star", source).unwrap();
        let first = cad_build::build(&recipe).unwrap();
        assert!((first.outputs["prism"].result.model.volume() - 12.0).abs() < 1e-10);
        assert_eq!(first.face_assertions["wall"].witnesses.len(), 1);
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        assert_eq!(
            first.feature_hashes,
            cad_build::build(&decoded).unwrap().feature_hashes
        );
        let wider = from_starlark_with_params(
            "triangle.star",
            source,
            &BTreeMap::from([("width".into(), 6.0)]),
        )
        .unwrap();
        assert!(
            (cad_build::build(&wider).unwrap().outputs["prism"]
                .result
                .model
                .volume()
                - 18.0)
                .abs()
                < 1e-10
        );
    }
    #[test]
    fn polygon_frontend_rejects_malformed_or_unbounded_coordinate_lists() {
        for points in [
            "[]",
            "[[0,0],[1,0]]",
            "[[0,0,0],[1,0],[0,1]]",
            "[[0,0],[True,0],[0,1]]",
            "[[0,0],['x',0],[0,1]]",
            "[[0,0]]*257",
        ] {
            let source = format!("polygon(id='p',points={points})");
            assert!(
                from_starlark("bad.star", &source).is_err(),
                "accepted {points}"
            );
        }
        let clockwise = from_starlark(
            "clockwise.star",
            "polygon(id='p',points=[[0,0],[0,1],[1,0]])",
        )
        .unwrap();
        assert!(matches!(
            cad_build::build(&clockwise),
            Err(cad_build::BuildError::PolygonProfile { .. })
        ));
    }
}

#[cfg(test)]
mod datum_tests {
    use super::*;
    const SOURCE:&str="gap=param(name='gap',default=3)\np=rectangle(id='p',width=2,depth=3)\nb=extrude(id='b',profile=p,height=4)\nd=part(id='part',body=b,datums={'bottom':datum(),'top':datum(offset=[0,0,4])})\nchild=instance(id='child',part=d,attach_to='parent',datum='bottom',target_datum='top',offset=[0,0,gap])\nparent=instance(id='parent',part=d)\na=assembly(id='a',instances=[child,parent])\nassembly_output(name='a',assembly=a)";
    #[test]
    fn datum_dsl_roundtrips_attachment_intent_and_keeps_placement_cache_shared() {
        let recipe = from_starlark("datums.star", SOURCE).unwrap();
        assert_eq!(recipe.parts[0].datums.len(), 2);
        assert!(recipe.assemblies[0].instances[0].attachment.is_some());
        let decoded: Recipe =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        let mut engine = cad_build::BuildEngine::default();
        let first = engine.build(&decoded).unwrap();
        let child = first.assemblies["a"]
            .instances
            .iter()
            .find(|instance| instance.id == "child")
            .unwrap();
        assert_eq!(child.transform.translation, [0.0, 0.0, 7.0]);
        let moved = from_starlark_with_params(
            "datums.star",
            SOURCE,
            &BTreeMap::from([("gap".into(), 5.0)]),
        )
        .unwrap();
        let changed = engine.build(&moved).unwrap();
        assert_eq!(changed.stats.computed_solids, 0);
        assert_eq!(first.feature_hashes, changed.feature_hashes);
        assert_ne!(
            first.assemblies["a"].content_hash,
            changed.assemblies["a"].content_hash
        );
    }
    #[test]
    fn datum_dsl_rejects_partial_attachment_arguments_and_invalid_frames() {
        for source in [
            "datum(offset=[1,2])",
            "datum(rotation=[0,0,0,0])",
            "part(id='p',body='b',datums={'':datum()})",
            "part(id='p',body='b',datums={'mount':{'typo':1}})",
            "instance(id='i',part='p',attach_to='a')",
            "instance(id='i',part='p',datum='mount',target_datum='top')",
            "part(id='p',body='b',datums=[])",
        ] {
            assert!(
                from_starlark("bad.star", source).is_err(),
                "accepted {source}"
            );
        }
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn native_transform_roundtrip_defaults_and_face_assertions() {
        let source = "p=rectangle(id='p',width=8,depth=4)\nb=extrude(id='b',profile=p,height=2)\ni=transform(id='identity',body=b)\nm=transform(id='moved',body=i,offset=[0.5,1e-3,2],rotation=[0,0,1,0])\nassert_faces(id='cap',from_feature=b,role='end_cap',in_feature=m)\noutput(name='result',feature=m)";
        let recipe = from_starlark("transform.star", source).unwrap();
        let json = serde_json::to_string(&recipe).unwrap();
        let restored: cad_build::Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&restored).unwrap());
        let first = cad_build::build(&recipe).unwrap();
        let second = cad_build::build(&restored).unwrap();
        assert_eq!(first.feature_hashes, second.feature_hashes);
        assert_eq!(
            first.face_assertions["cap"].status,
            cad_build::FaceResolutionStatus::Resolved
        );
        let bounds = first.outputs["result"].result.model.bounds();
        assert_eq!(bounds.min, [-7.5, -3.999, 2.0]);
        assert_eq!(bounds.max, [0.5, 0.001, 4.0]);
    }

    #[test]
    fn native_transform_rejects_invalid_quaternions_coordinates_and_fields() {
        for arguments in [
            "rotation=[0,0,0,0]",
            "rotation=[0,0,0,2]",
            "rotation=[0,0,1]",
            "rotation=[False,0,0,1]",
            "offset=[1,2]",
            "offset=[1,2,True]",
            "scale=2",
        ] {
            assert!(
                from_starlark(
                    "invalid.star",
                    &format!("transform(id='t',body='b',{arguments})")
                )
                .is_err(),
                "{arguments}"
            );
        }
    }
}

#[cfg(test)]
mod convex_intersection_tests {
    use super::*;

    #[test]
    fn transformed_intersection_parameter_edits_compile_reproducibly() {
        let source = "r=param(name='radius',default=3)\na=extrude(id='a',profile=rectangle(id='ap',width=4,depth=4),height=2)\nb=extrude(id='b',profile=rectangle(id='bp',width=r*1.4142135623730951,depth=r*1.4142135623730951),height=2)\nt=transform(id='t',body=b,offset=[2,2-r,0],rotation=[0,0,0.3826834323650898,0.9238795325112867])\nc=intersect(id='c',left=a,right=t)\nassert_faces(id='a_cap',from_feature=a,role='end_cap',in_feature=c)\nassert_faces(id='b_cap',from_feature=b,role='end_cap',in_feature=c)\noutput(name='part',feature=c)";
        let mut engine = cad_build::BuildEngine::default();
        let first = from_starlark("clip.star", source).unwrap();
        let built = engine.build(&first).unwrap();
        assert!((built.outputs["part"].result.model.volume() - 28.0).abs() < 1e-8);
        let compiled = serde_json::to_string(&first).unwrap();
        let restored: Recipe = serde_json::from_str(&compiled).unwrap();
        assert_eq!(
            built.feature_hashes,
            engine.build(&restored).unwrap().feature_hashes
        );
        let edited = from_starlark_with_params(
            "clip.star",
            source,
            &BTreeMap::from([("radius".into(), 2.5)]),
        )
        .unwrap();
        let changed = engine.build(&edited).unwrap();
        assert!((changed.outputs["part"].result.model.volume() - 23.0).abs() < 1e-8);
        assert_eq!(changed.stats.computed_solids, 3);
        assert_eq!(changed.stats.cache_hits, 1);
        assert_eq!(built.feature_hashes["a"], changed.feature_hashes["a"]);
        assert_ne!(built.feature_hashes["c"], changed.feature_hashes["c"]);
        assert_eq!(
            changed.face_assertions["a_cap"].witnesses[0].target_face,
            changed.face_assertions["b_cap"].witnesses[0].target_face
        );
    }
}

#[cfg(test)]
mod face_datum_tests {
    use super::*;
    #[test]
    fn inline_face_datums_preserve_recipe_intent_and_resolve_after_geometry() {
        let source="h=param(name='height',default=4)\nb=extrude(id='b',profile=rectangle(id='p',width=8,depth=6),height=h)\np=part(id='part',body=b,datums={'fixed':datum(),'seat':face_datum(from_feature=b,role='end_cap',x_direction=[1,0,0])})\na=assembly(id='a',instances=[instance(id='i',part=p)])\nassembly_output(name='a',assembly=a)";
        let recipe = from_starlark("seat.star", source).unwrap();
        assert_eq!(recipe.schema_version, 2);
        assert_eq!(recipe.face_datums.len(), 1);
        assert_eq!(recipe.parts[0].datums.len(), 1);
        let encoded = serde_json::to_string(&recipe).unwrap();
        let restored: Recipe = serde_json::from_str(&encoded).unwrap();
        assert_eq!(encoded, serde_json::to_string(&restored).unwrap());
        let built = cad_build::build(&restored).unwrap();
        assert_eq!(
            built.face_datums["part"]["seat"].frame.translation,
            [4.0, 3.0, 4.0]
        );
        let edited = from_starlark_with_params(
            "seat.star",
            source,
            &BTreeMap::from([("height".into(), 7.0)]),
        )
        .unwrap();
        assert_eq!(
            cad_build::build(&edited).unwrap().face_datums["part"]["seat"]
                .frame
                .translation,
            [4.0, 3.0, 7.0]
        );
    }
    #[test]
    fn face_datum_helpers_reject_malformed_or_nonfinite_frame_inputs() {
        for args in [
            "x_direction=[0,0,0]",
            "x_direction=[1,True,0]",
            "x_direction=[1,0]",
            "x_direction=[1,0,0],offset=[1,2]",
            "x_direction=[1,0,0],typo=2",
        ] {
            assert!(from_starlark(
                "bad.star",
                &format!("face_datum(from_feature='b',role='end_cap',{args})")
            )
            .is_err());
        }
        assert!(from_starlark("bad.star","part(id='p',body='b',datums={'x':{'kind':'face_datum','anchor_feature':'b','role':'end_cap','x_direction':[1,0,0],'typo':1}})").is_err());
    }
}

#[cfg(test)]
mod holed_polygon_tests {
    use super::*;
    #[test]
    fn holed_profile_dsl_roundtrips_and_exposes_inner_wall_roles() {
        let source="w=param(name='hole_width',default=2)\np=polygon(id='p',points=[[0,0],[10,0],[10,8],[0,8]],holes=[[[2,2],[2,4],[2+w,4],[2+w,2]]])\nb=extrude(id='b',profile=p,height=3)\nassert_faces(id='inner',from_feature=b,role='hole_0_wall_0',in_feature=b)\noutput(name='body',feature=b)";
        let mut engine = cad_build::BuildEngine::default();
        for (width, expected) in [(2.0, 228.0), (3.0, 222.0)] {
            let recipe = from_starlark_with_params(
                "holes.star",
                source,
                &BTreeMap::from([("hole_width".into(), width)]),
            )
            .unwrap();
            let roundtrip: Recipe =
                serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
            let built = engine.build(&roundtrip).unwrap();
            assert_eq!(built.outputs["body"].result.model.volume(), expected);
            assert_eq!(built.face_assertions.len(), 1);
            assert_eq!(engine.build(&roundtrip).unwrap().stats.computed_solids, 0);
        }
        for holes in [
            "None",
            "[[[1,1],[2,1]]]",
            "[[[True,1],[1,2],[2,1]]]",
            "[[[1,1,3],[1,2],[2,1]]]",
            "[[[1,1],[1,2],[2,1]]]*33",
        ] {
            let source =
                format!("polygon(id='p',points=[[0,0],[10,0],[10,10],[0,10]],holes={holes})");
            assert!(
                from_starlark("invalid.star", &source).is_err(),
                "accepted {holes}"
            );
        }
    }
}

#[cfg(test)]
mod concave_polygon_tests {
    use super::*;
    #[test]
    fn concave_starlark_profile_preserves_parameters_roles_and_reference_intent() {
        let source = r#"
notch = param(name="notch_floor", default=2)
p = polygon(id="p", points=[[0,0],[10,0],[10,10],[8,10],[8,notch],[2,notch],[2,10],[0,10]], holes=[[[4,0.5],[4,1.5],[6,1.5],[6,0.5]]])
b = extrude(id="b", profile=p, height=3)
m = translate(id="m", body=b, offset=[1,2,3])
part(id="plate", body=m, datums={"center":face_datum(from_feature=b, role="end_cap", x_direction=[1,0,0])})
assert_faces(id="notch_face", from_feature=b, role="wall_4", in_feature=m)
assert_faces(id="hole_face", from_feature=b, role="hole_0_wall_0", in_feature=m)
output(name="body", feature=m)
"#;
        let mut engine = cad_build::BuildEngine::default();
        for (floor, volume, center_y) in [(2.0, 150.0, 6.2), (3.0, 168.0, 2.0 + 225.0 / 56.0)] {
            let recipe = from_starlark_with_params(
                "concave.star",
                source,
                &BTreeMap::from([("notch_floor".into(), floor)]),
            )
            .unwrap();
            let roundtrip: Recipe =
                serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
            assert_eq!(roundtrip.face_datums.len(), 1);
            assert!(roundtrip.parts[0].datums.is_empty());
            let built = engine.build(&roundtrip).unwrap();
            assert!((built.outputs["body"].result.model.volume() - volume).abs() < 1e-10);
            assert_eq!(built.face_assertions.len(), 2);
            assert!(
                (built.face_datums["plate"]["center"].frame.translation[1] - center_y).abs()
                    < 1e-10
            );
            assert_eq!(engine.build(&roundtrip).unwrap().stats.computed_solids, 0);
        }
    }
}

#[cfg(test)]
mod circular_hole_tests {
    use super::*;

    #[test]
    fn circular_hole_starlark_rejects_malformed_objects_and_combined_hole_budget() {
        for holes in [
            "None",
            "[{'center':[1,2]}]",
            "[{'center':[1,2], 'radius':1,'typo':0}]",
            "[{'center':[True,2], 'radius':1}]",
            "[{'center':[1,2,3], 'radius':1}]",
            "[{'center':[1,2], 'radius':True}]",
            "[{'center':[1,2], 'radius':1}]*33",
        ] {
            let source = format!(
                "polygon(id='p',points=[[0,0],[10,0],[10,10],[0,10]],circular_holes={holes})"
            );
            assert!(
                from_starlark("invalid.star", &source).is_err(),
                "accepted {holes}"
            );
        }
        assert!(from_starlark("invalid.star", "polygon(id='p',points=[[0,0],[10,0],[10,10],[0,10]],holes=[[[1,1],[1,2],[2,1]]],circular_holes=[{'center':[5,5],'radius':1}]*32)").is_err());
    }

    #[test]
    fn drilled_bearing_mount_parameters_preserve_analytic_volume_and_attachments() {
        let source = include_str!("../../../examples/drilled_bearing_mount.star");
        let mut engine = cad_build::BuildEngine::default();
        for (thickness, bore, bolt) in [(8.0, 12.0, 3.0), (4.0, 10.0, 2.0), (12.0, 14.0, 4.0)] {
            let recipe = from_starlark_with_params(
                "drilled_bearing_mount.star",
                source,
                &BTreeMap::from([
                    ("plate_thickness".into(), thickness),
                    ("bearing_bore".into(), bore),
                    ("bolt_radius".into(), bolt),
                ]),
            )
            .unwrap();
            let encoded = serde_json::to_string(&recipe).unwrap();
            let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
            assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
            let built = engine.build(&decoded).unwrap();
            let pi = std::f64::consts::PI;
            let plate_volume = (6632.0 - pi * (bore * bore + 4.0 * bolt * bolt)) * thickness;
            let sleeve_volume = pi * (bore * bore - (bore - 4.0).powi(2)) * (thickness + 8.0);
            let shaft_volume = pi * (bore - 5.0).powi(2) * (thickness + 22.0);
            let fastener_volume =
                4.0 * pi * ((bolt - 0.5).powi(2) * (thickness + 4.0) + (bolt + 2.0).powi(2) * 3.0);
            for (name, expected) in [
                ("plate", plate_volume),
                ("sleeve", sleeve_volume),
                ("shaft", shaft_volume),
            ] {
                assert!(
                    (built.outputs[name].result.model.volume() - expected).abs() < 1e-8,
                    "{name}"
                );
            }
            let assembly = &built.assemblies["drilled_bearing_mount"];
            assert_eq!(assembly.instances.len(), 11);
            assert_eq!(assembly.parts.len(), 5);
            assert_eq!(assembly.hierarchy.len(), 2);
            assert!(
                (assembly.material_volume()
                    - (plate_volume + sleeve_volume + shaft_volume + fastener_volume))
                    .abs()
                    < 1e-8
            );
            for (actual, expected) in built.face_datums["plate"]["center"]
                .frame
                .translation
                .iter()
                .zip([50.0, 35.0, thickness / 2.0])
            {
                assert!((actual - expected).abs() < 1e-10);
            }
            let bounds = assembly.bounds().unwrap();
            assert_eq!(bounds.min, [0.0, 0.0, -11.0]);
            assert_eq!(bounds.max, [100.0, 70.0, thickness + 11.0]);
            assert_eq!(built.face_assertions.len(), 1);
            assert_eq!(engine.build(&decoded).unwrap().stats.computed_solids, 0);
        }
        for (name, value) in [
            ("plate_thickness", 3.0),
            ("bearing_bore", 15.0),
            ("bolt_radius", 1.0),
        ] {
            assert!(from_starlark_with_params(
                "invalid.star",
                source,
                &BTreeMap::from([(name.into(), value)])
            )
            .is_err());
        }
    }
}

#[cfg(test)]
mod revolve_tests {
    use super::*;

    #[test]
    fn starlark_revolve_retains_parameterized_meridian_and_native_volume() {
        let source = "r=param(name='journal_radius',default=3)\np=polygon(id='meridian',points=[[0,0],[5,0],[5,10],[r,10],[r,20],[0,20]])\ns=revolve(id='shaft',profile=p)\noutput(name='shaft',feature=s)";
        for radius in [2., 3., 4.] {
            let recipe = from_starlark_with_params(
                "shaft.star",
                source,
                &BTreeMap::from([("journal_radius".into(), radius)]),
            )
            .unwrap();
            let built = cad_build::build(&recipe).unwrap();
            let expected = std::f64::consts::PI * (25. * 10. + radius * radius * 10.);
            assert!((built.outputs["shaft"].result.model.volume() - expected).abs() < 1e-9);
            assert!(
                matches!(&recipe.features[1], cad_build::Feature::Revolve { profile, op_version: 1, .. } if profile == "meridian")
            );
        }
    }

    #[test]
    fn starlark_revolve_rejects_unknown_angle_and_missing_profile() {
        for source in [
            "revolve(id='r',profile='p',angle=180)",
            "revolve(id='r')",
            "revolve(id='r',profile=42)",
        ] {
            assert!(
                from_starlark("bad-revolve.star", source).is_err(),
                "accepted {source}"
            );
        }
    }
}
