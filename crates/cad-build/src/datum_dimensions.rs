//! Directed native planar datum measurements, independent of drawing projection.
use super::*;
use cad_assembly::{BuiltAssembly, RigidTransform};
use cad_kernel::SectionPlane;
pub const MAX_DATUM_DIMENSIONS: usize = 128;
pub const MAX_DATUM_DIMENSION_WORK: usize = 10_000_000;
fn direction_default() -> i8 {
    1
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatumDimensionEndpoint {
    pub instance_path: Vec<String>,
    pub datum: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatumDimension {
    pub id: String,
    pub output: String,
    pub from: DatumDimensionEndpoint,
    pub to: DatumDimensionEndpoint,
    pub direction: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionTolerance>,
}
fn strict_object<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<T, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    if !value.is_object() {
        return Err(serde::de::Error::custom(
            "datum dimensions and endpoints must be objects",
        ));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
impl<'de> Deserialize<'de> for DatumDimensionEndpoint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            instance_path: Vec<String>,
            datum: String,
        }
        let r: Raw = strict_object(d)?;
        Ok(Self {
            instance_path: r.instance_path,
            datum: r.datum,
        })
    }
}
impl<'de> Deserialize<'de> for DatumDimension {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            id: String,
            output: String,
            from: DatumDimensionEndpoint,
            to: DatumDimensionEndpoint,
            #[serde(default = "direction_default")]
            direction: i8,
            #[serde(default)]
            min: Option<f64>,
            #[serde(default)]
            max: Option<f64>,
            #[serde(default)]
            tolerance: Option<DimensionTolerance>,
        }
        let r: Raw = strict_object(d)?;
        Ok(Self {
            id: r.id,
            output: r.output,
            from: r.from,
            to: r.to,
            direction: r.direction,
            min: r.min,
            max: r.max,
            tolerance: r.tolerance,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct DatumDimensionWitness {
    pub instance_id: String,
    pub instance_path: Vec<String>,
    pub part_id: String,
    pub part_content_hash: String,
    pub datum: String,
    pub datum_frame: RigidTransform,
    pub occurrence_frame: RigidTransform,
    pub world_frame: RigidTransform,
    pub native_plane: SectionPlane,
    pub native_extent_mm: f64,
    pub world_plane: SectionPlane,
    pub semantic_datum: FaceDatumResolution,
}
#[derive(Debug, Clone, Serialize)]
pub struct DatumDimensionReport {
    pub declaration: DatumDimension,
    pub status: String,
    pub value_mm: f64,
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
    pub arithmetic_uncertainty_mm: f64,
    pub axis: [f64; 3],
    pub from: DatumDimensionWitness,
    pub to: DatumDimensionWitness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionToleranceReport>,
}
fn safe(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
impl DatumDimension {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidDatumDimension {
            id: self.id.clone(),
            output: self.output.clone(),
            message: message.into(),
        };
        if !safe(&self.id)
            || self.output.is_empty()
            || self.output.len() > 256
            || ![-1, 1].contains(&self.direction)
        {
            return Err(fail(
                "require portable ID, bounded output and direction exactly -1 or 1",
            ));
        }
        for endpoint in [&self.from, &self.to] {
            if endpoint.datum.is_empty()
                || endpoint.datum.len() > 128
                || endpoint.instance_path.is_empty()
                || endpoint.instance_path.len() > 64
                || endpoint
                    .instance_path
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 256)
            {
                return Err(fail("require bounded exact occurrence path and datum"));
            }
        }
        if self.from == self.to {
            return Err(fail("measurement endpoints must differ"));
        }
        if self
            .min
            .into_iter()
            .chain(self.max)
            .any(|v| !v.is_finite() || v < 0.)
            || matches!((self.min,self.max),(Some(a),Some(b)) if a>b)
        {
            return Err(fail(
                "measurement bounds must be finite, nonnegative and ordered",
            ));
        }
        if let Some(t) = &self.tolerance {
            t.validate().map_err(|e| fail(&e))?;
        }
        Ok(())
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.datum_dimensions.len() > MAX_DATUM_DIMENSIONS {
        return Err(BuildError::ResourceLimit {
            resource: "datum dimensions",
            limit: MAX_DATUM_DIMENSIONS,
        });
    }
    let mut ids = BTreeSet::new();
    for d in &recipe.datum_dimensions {
        d.validate()?;
        if recipe.schema_version != 2
            || !ids.insert(d.id.to_ascii_lowercase())
            || !recipe.assembly_outputs.iter().any(|o| o.name == d.output)
        {
            return Err(BuildError::InvalidDatumDimension {
                id: d.id.clone(),
                output: d.output.clone(),
                message:
                    "require schema 2, unique case-insensitive ID and declared assembly output"
                        .into(),
            });
        }
    }
    Ok(())
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}
pub(super) fn resolve(
    endpoint: &DatumDimensionEndpoint,
    assembly: &BuiltAssembly,
    datums: &ResolvedFaceDatums,
    t: cad_kernel::Tolerance,
    work: &mut usize,
) -> Result<DatumDimensionWitness, String> {
    let path_cost = endpoint
        .instance_path
        .iter()
        .fold(1usize, |n, s| n.saturating_add(s.len().saturating_add(1)));
    *work = work.saturating_add(assembly.instances.len().saturating_mul(path_cost));
    if *work > MAX_DATUM_DIMENSION_WORK {
        return Err("datum dimension path work exhausted".into());
    }
    let mut matches = assembly
        .instances
        .iter()
        .filter(|i| assembly.instance_paths.get(&i.id) == Some(&endpoint.instance_path));
    let instance = matches
        .next()
        .ok_or("exact datum occurrence path not found")?;
    if matches.next().is_some() {
        return Err("ambiguous datum occurrence path".into());
    }
    let part = &assembly.parts[&instance.part];
    let semantic = datums
        .get(&instance.part)
        .and_then(|d| d.get(&endpoint.datum))
        .ok_or(
            "measurement requires a native semantic face datum, not an authored coordinate datum",
        )?;
    if semantic.definition.offset != [0.; 3]
        || semantic.selection.status != FaceResolutionStatus::Resolved
        || semantic.selection.witnesses.len() != 1
        || semantic.target_content_hash != part.content_hash
    {
        return Err(
            "measurement requires one current native planar face and zero datum offset".into(),
        );
    }
    let id = semantic.selection.witnesses[0].target_face;
    let plane = part
        .result
        .model
        .faces
        .get(id.0)
        .and_then(|f| f.try_plane())
        .ok_or("datum must resolve to a native planar face")?;
    let datum_frame = *part
        .datums
        .get(&endpoint.datum)
        .ok_or("resolved datum frame missing")?;
    let world_frame = instance
        .transform
        .compose(&datum_frame, t)
        .map_err(|e| e.to_string())?;
    let native_plane = SectionPlane {
        origin: plane.origin,
        normal: plane.normal,
        x_direction: datum_frame.vector([1., 0., 0.]),
    };
    native_plane.validate().map_err(|e| e.to_string())?;
    let world_plane = SectionPlane {
        origin: instance.transform.point(plane.origin),
        normal: instance.transform.vector(plane.normal),
        x_direction: instance.transform.vector(native_plane.x_direction),
    };
    world_plane.validate().map_err(|e| e.to_string())?;
    // Include the full native body, not just a convenient supporting-plane
    // origin: a tiny admitted angular departure acts over the entire face.
    let model = &part.result.model;
    let geometry_work = model
        .vertices
        .len()
        .saturating_add(model.edges.len())
        .saturating_add(model.faces.len())
        .saturating_add(
            model
                .loops
                .iter()
                .fold(0usize, |n, l| n.saturating_add(l.edges.len())),
        );
    *work = work.saturating_add(geometry_work);
    if *work > MAX_DATUM_DIMENSION_WORK {
        return Err("datum dimension native geometry work exhausted".into());
    }
    let bounds = part.result.model.bounds();
    let native_extent_mm = norm(std::array::from_fn(|i| {
        (bounds.min[i] - plane.origin[i])
            .abs()
            .max((bounds.max[i] - plane.origin[i]).abs())
    }))
    .next_up();
    if !native_extent_mm.is_finite() {
        return Err("native support extent exceeds numeric range".into());
    }
    Ok(DatumDimensionWitness {
        instance_id: instance.id.clone(),
        instance_path: endpoint.instance_path.clone(),
        part_id: instance.part.clone(),
        part_content_hash: part.content_hash.clone(),
        datum: endpoint.datum.clone(),
        datum_frame,
        occurrence_frame: instance.transform,
        world_frame,
        native_plane,
        native_extent_mm,
        world_plane,
        semantic_datum: semantic.clone(),
    })
}
pub(super) fn evaluate(
    recipe: &Recipe,
    assemblies: &BTreeMap<String, BuiltAssembly>,
    datums: &ResolvedFaceDatums,
    t: cad_kernel::Tolerance,
) -> Result<BTreeMap<String, DatumDimensionReport>, BuildError> {
    let mut result = BTreeMap::new();
    let mut work = 0;
    for d in &recipe.datum_dimensions {
        let fail = |message: String, actual_value: Option<f64>| BuildError::DatumDimensionFailed {
            id: d.id.clone(),
            output: d.output.clone(),
            message,
            actual_value,
        };
        let assembly = assemblies
            .get(&d.output)
            .ok_or_else(|| fail("assembly output not evaluated".into(), None))?;
        let from = resolve(&d.from, assembly, datums, t, &mut work).map_err(|e| fail(e, None))?;
        let to = resolve(&d.to, assembly, datums, t, &mut work).map_err(|e| fail(e, None))?;
        let n = from.world_plane.normal;
        let m = to.world_plane.normal;
        if (norm(n) - 1.).abs() > 128. * f64::EPSILON
            || (norm(m) - 1.).abs() > 128. * f64::EPSILON
            || norm(sub(
                m,
                n.map(|v| v * if dot(n, m) >= 0. { 1. } else { -1. }),
            )) > 256. * f64::EPSILON
        {
            return Err(fail(
                "native datum support planes must be arithmetic-parallel or opposed".into(),
                None,
            ));
        }
        let axis = n.map(|v| v * d.direction as f64 / norm(n));
        let delta = sub(to.world_plane.origin, from.world_plane.origin);
        let value_mm = dot(delta, axis);
        let scale = norm(from.world_plane.origin)
            + norm(to.world_plane.origin)
            + norm(from.native_plane.origin)
            + norm(to.native_plane.origin)
            + norm(delta)
            + from.native_extent_mm
            + to.native_extent_mm
            + norm(from.datum_frame.translation)
            + norm(to.datum_frame.translation)
            + 1.;
        let normal_departure = norm(sub(
            m.map(|v| v / norm(m)),
            n.map(|v| v / norm(n) * if dot(n, m) >= 0. { 1. } else { -1. }),
        ));
        let alignment_allowance = normal_departure * (from.native_extent_mm + to.native_extent_mm);
        let arithmetic_uncertainty_mm =
            (4096. * f64::EPSILON * scale + alignment_allowance).next_up();
        let lower_bound_mm = (value_mm - arithmetic_uncertainty_mm).next_down();
        let upper_bound_mm = (value_mm + arithmetic_uncertainty_mm).next_up();
        if !arithmetic_uncertainty_mm.is_normal()
            || arithmetic_uncertainty_mm > t.linear / 16.
            || !upper_bound_mm.is_finite()
            || lower_bound_mm <= t.linear
        {
            return Err(fail(
                "directed dimension is nonpositive, unresolved or exceeds numeric precision".into(),
                Some(value_mm),
            ));
        }
        if d.min.is_some_and(|v| lower_bound_mm < v) || d.max.is_some_and(|v| upper_bound_mm > v) {
            return Err(fail(
                "native measurement interval exceeds declared design bounds".into(),
                Some(value_mm),
            ));
        }
        let tolerance = d
            .tolerance
            .as_ref()
            .map(|spec| spec.report(value_mm, t))
            .transpose()
            .map_err(|e| fail(e, Some(value_mm)))?;
        result.insert(
            d.id.clone(),
            DatumDimensionReport {
                declaration: d.clone(),
                status: "passed".into(),
                value_mm,
                lower_bound_mm,
                upper_bound_mm,
                arithmetic_uncertainty_mm,
                axis,
                from,
                to,
                tolerance,
            },
        );
    }
    Ok(result)
}
