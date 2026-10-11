//! Material sums over occurrences, not Boolean-union properties.
use super::*;

/// Assembly-frame properties; coordinates are mm, volume mm³, central volume
/// inertia mm⁵, mass kg, and central mass inertia kg·mm². Tensor off-diagonals
/// are negative products of inertia. Missing material on any occurrence makes
/// all mass-weighted totals unknown, including for an empty part occurrence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyMassProperties {
    pub volume: f64,
    pub volume_centroid: Option<[f64; 3]>,
    pub central_volume_inertia: Option<[[f64; 3]; 3]>,
    pub mass_kg: Option<f64>,
    pub center_of_mass: Option<[f64; 3]>,
    pub central_mass_inertia: Option<[[f64; 3]; 3]>,
    pub missing_density_occurrences: usize,
}

#[derive(Clone, Copy, Default)]
struct Sum {
    value: f64,
    correction: f64,
    absolute: f64,
}
impl Sum {
    fn add(&mut self, value: f64) -> Result<(), AssemblyError> {
        numeric(value)?;
        let adjusted = value - self.correction;
        let next = numeric(self.value + adjusted)?;
        self.correction = (next - self.value) - adjusted;
        self.value = next;
        self.absolute = numeric(self.absolute + value.abs())?;
        Ok(())
    }
    fn error(&self) -> f64 {
        16.0 * f64::EPSILON * self.absolute + self.correction.abs()
    }
}
fn numeric(value: f64) -> Result<f64, AssemblyError> {
    if value == 0.0 || value.is_normal() {
        Ok(value)
    } else {
        Err(AssemblyError::NumericRange)
    }
}
fn product(a: f64, b: f64) -> Result<f64, AssemblyError> {
    let value = numeric(a * b)?;
    if value == 0.0 && a != 0.0 && b != 0.0 {
        Err(AssemblyError::NumericRange)
    } else {
        Ok(value)
    }
}
type Tensor = [[f64; 3]; 3];
type Aggregate = (f64, Option<[f64; 3]>, Option<Tensor>);
struct Occurrence {
    center: [f64; 3],
    center_error: [f64; 3],
    volume: f64,
    inertia: Tensor,
    inertia_error: Tensor,
    density: Option<f64>,
}
fn two_sum(a: f64, b: f64) -> Result<(f64, f64), AssemblyError> {
    let sum = numeric(a + b)?;
    let recovered = sum - a;
    let error = (a - (sum - recovered)) + (b - recovered);
    Ok((sum, numeric(error)?.abs()))
}
fn columns(transform: &RigidTransform) -> Tensor {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|axis| transform.vector(axis))
}
fn signed_permutation(matrix: Tensor) -> bool {
    (0..3).all(|i| {
        matrix
            .iter()
            .filter(|column| column[i].abs() == 1.0)
            .count()
            == 1
    }) && matrix.iter().all(|column| {
        column.iter().all(|v| *v == 0.0 || v.abs() == 1.0)
            && column.iter().filter(|v| v.abs() == 1.0).count() == 1
    })
}
fn placed_center(
    local: [f64; 3],
    transform: &RigidTransform,
    reference: [f64; 3],
    tolerance: Tolerance,
) -> Result<([f64; 3], [f64; 3]), AssemblyError> {
    let matrix = columns(transform);
    let exact = signed_permutation(matrix);
    let mut center = [0.0; 3];
    let mut error = [0.0; 3];
    for axis in 0..3 {
        let mut sum = Sum::default();
        for input in 0..3 {
            sum.add(product(matrix[input][axis], local[input])?)?;
        }
        let rotation_error = if exact {
            0.0
        } else {
            sum.error() + 32.0 * f64::EPSILON * local.iter().map(|v| v.abs()).sum::<f64>()
        };
        let (delta, delta_error) = two_sum(transform.translation[axis], -reference[axis])?;
        let (value, add_error) = two_sum(delta, sum.value)?;
        center[axis] = value;
        error[axis] = numeric(rotation_error + delta_error + add_error)?;
        if error[axis] > tolerance.linear {
            return Err(AssemblyError::NumericRange);
        }
    }
    Ok((center, error))
}
fn rotate_tensor(
    tensor: Tensor,
    transform: &RigidTransform,
) -> Result<(Tensor, Tensor), AssemblyError> {
    let matrix = columns(transform);
    let exact = signed_permutation(matrix);
    let absolute = tensor.iter().flatten().map(|v| v.abs()).sum::<f64>();
    numeric(absolute)?;
    let mut result = [[0.0; 3]; 3];
    let mut error = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in i..3 {
            let mut sum = Sum::default();
            for a in 0..3 {
                for b in 0..3 {
                    sum.add(product(product(matrix[a][i], tensor[a][b])?, matrix[b][j])?)?;
                }
            }
            result[i][j] = sum.value;
            result[j][i] = sum.value;
            error[i][j] = if exact {
                0.0
            } else {
                numeric(sum.error() + 64.0 * f64::EPSILON * absolute)?
            };
            error[j][i] = error[i][j];
        }
    }
    Ok((result, error))
}

fn aggregate(
    occurrences: &[Occurrence],
    reference: [f64; 3],
    mass: bool,
    tolerance: Tolerance,
) -> Result<Aggregate, AssemblyError> {
    let weights: Vec<f64> = occurrences
        .iter()
        .map(|o| {
            if mass {
                bom::mass(o.volume, o.density.expect("mass completeness checked"))
            } else {
                Ok(o.volume)
            }
        })
        .collect::<Result<_, _>>()?;
    let mut total = Sum::default();
    for weight in &weights {
        total.add(*weight)?;
    }
    if total.value == 0.0 {
        return Ok((0.0, None, None));
    }
    let mut center = [Sum::default(); 3];
    let mut input_error = [Sum::default(); 3];
    for (o, weight) in occurrences.iter().zip(&weights) {
        let fraction = numeric(*weight / total.value)?;
        if fraction == 0.0 {
            return Err(AssemblyError::NumericRange);
        }
        for (axis, sum) in center.iter_mut().enumerate() {
            sum.add(product(o.center[axis], fraction)?)?;
            input_error[axis].add(product(o.center_error[axis], fraction)?)?;
        }
    }
    let relative = center.map(|sum| sum.value);
    let mean_error = [0, 1, 2].map(|axis| center[axis].error() + input_error[axis].value);
    let mut world = [0.0; 3];
    for axis in 0..3 {
        let (value, roundoff) = two_sum(reference[axis], relative[axis])?;
        world[axis] = value;
        let error = mean_error[axis] + roundoff;
        if !error.is_finite()
            || error > tolerance.linear
            || ((world[axis] - reference[axis]) - relative[axis]).abs() > tolerance.linear
        {
            return Err(AssemblyError::NumericRange);
        }
    }
    let mut sums = [[Sum::default(); 3]; 3];
    let mut uncertainty = [[Sum::default(); 3]; 3];
    for (o, weight) in occurrences.iter().zip(weights) {
        let mut d = [0.0; 3];
        let mut de = [0.0; 3];
        for axis in 0..3 {
            let (value, roundoff) = two_sum(o.center[axis], -relative[axis])?;
            d[axis] = value;
            de[axis] = numeric(o.center_error[axis] + mean_error[axis] + roundoff)?;
        }
        let displacement_error = |i: usize, j: usize| -> Result<f64, AssemblyError> {
            product(
                weight,
                numeric(
                    product(d[i].abs(), de[j])?
                        + product(d[j].abs(), de[i])?
                        + product(de[i], de[j])?,
                )?,
            )
        };
        for i in 0..3 {
            for j in i..3 {
                let local = if mass {
                    bom::mass(o.inertia[i][j].abs(), o.density.unwrap())?.copysign(o.inertia[i][j])
                } else {
                    o.inertia[i][j]
                };
                sums[i][j].add(local)?;
                uncertainty[i][j].add(if mass {
                    bom::mass(o.inertia_error[i][j], o.density.unwrap())?
                } else {
                    o.inertia_error[i][j]
                })?;
                if i == j {
                    for (axis, delta) in d.iter().enumerate() {
                        if axis != i {
                            sums[i][j].add(product(product(weight, *delta)?, *delta)?)?;
                            uncertainty[i][j].add(displacement_error(axis, axis)?)?;
                        }
                    }
                } else {
                    sums[i][j].add(-product(product(weight, d[i])?, d[j])?)?;
                    uncertainty[i][j].add(displacement_error(i, j)?)?;
                }
            }
        }
    }
    let trace = numeric(sums[0][0].value + sums[1][1].value + sums[2][2].value)?;
    if trace <= 0.0 {
        return Err(AssemblyError::NumericRange);
    }
    let mut result = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in i..3 {
            if sums[i][j].error() + uncertainty[i][j].value > trace * 1e-10
                || (i == j && sums[i][j].value <= 0.0)
            {
                return Err(AssemblyError::NumericRange);
            }
            result[i][j] = sums[i][j].value;
            result[j][i] = result[i][j];
        }
    }
    Ok((total.value, Some(world), Some(result)))
}

impl BuiltAssembly {
    /// Validate identity/placements before sharing any same-hash computation.
    /// Integration is performed once per unique definition hash. Centroid and
    /// inertia sums use a common translation reference, avoiding subtraction of
    /// large world-origin second moments to recover small central inertias.
    pub fn mass_properties(
        &self,
        tolerance: Tolerance,
    ) -> Result<AssemblyMassProperties, AssemblyError> {
        self.validate(tolerance)?;
        let mut definitions = BTreeMap::new();
        for (id, part) in &self.parts {
            if !definitions.contains_key(&part.content_hash) {
                let properties = part
                    .result
                    .model
                    .mass_properties(tolerance)
                    .map_err(|error| AssemblyError::InvalidPart {
                        part: id.clone(),
                        message: error.to_string(),
                    })?;
                definitions.insert(part.content_hash.clone(), properties);
            }
        }
        let mut reference = None;
        let mut occurrences = Vec::new();
        let mut missing = 0;
        for instance in &self.instances {
            let part = &self.parts[&instance.part];
            let density = part.metadata.material.as_ref().map(|m| m.density_kg_per_m3);
            missing += usize::from(density.is_none());
            let properties = &definitions[&part.content_hash];
            if properties.volume == 0.0 {
                continue;
            }
            let origin = *reference.get_or_insert(instance.transform.translation);
            let (center, center_error) = placed_center(
                properties.centroid.expect("nonempty kernel centroid"),
                &instance.transform,
                origin,
                tolerance,
            )?;
            let (inertia, inertia_error) = rotate_tensor(
                properties.central_inertia.expect("nonempty kernel inertia"),
                &instance.transform,
            )?;
            occurrences.push(Occurrence {
                center,
                center_error,
                volume: properties.volume,
                inertia,
                inertia_error,
                density,
            });
        }
        let reference = reference.unwrap_or([0.0; 3]);
        let (volume, volume_centroid, central_volume_inertia) =
            aggregate(&occurrences, reference, false, tolerance)?;
        let (mass_kg, center_of_mass, central_mass_inertia) = if missing == 0 {
            let (mass, center, tensor) = aggregate(&occurrences, reference, true, tolerance)?;
            (Some(mass), center, tensor)
        } else {
            (None, None, None)
        };
        Ok(AssemblyMassProperties {
            volume,
            volume_centroid,
            central_volume_inertia,
            mass_kg,
            center_of_mass,
            central_mass_inertia,
            missing_density_occurrences: missing,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn part(density: Option<f64>) -> BuiltPart {
        BuiltPart {
            datums: BTreeMap::new(),
            metadata: PartMetadata {
                material: density.map(|density_kg_per_m3| Material {
                    name: "test material".into(),
                    density_kg_per_m3,
                }),
                ..Default::default()
            },
            feature_id: "box".into(),
            content_hash: "shared-box".into(),
            result: Arc::new(
                cad_kernel::extrude_rectangle(2.0, 4.0, 6.0, Tolerance::default()).unwrap(),
            ),
        }
    }
    fn instance(id: &str, part: &str, transform: RigidTransform) -> Instance {
        Instance {
            id: id.into(),
            part: part.into(),
            transform,
            attachment: None,
        }
    }
    fn nested(translation: [f64; 3], second_density: Option<f64>) -> BuiltAssembly {
        let half = std::f64::consts::FRAC_1_SQRT_2;
        let module = AssemblyDefinition {
            id: "module".into(),
            instances: vec![
                instance("first", "a", RigidTransform::default()),
                instance(
                    "second",
                    "b",
                    RigidTransform {
                        translation: [10.0, 0.0, 0.0],
                        rotation: [0.0, 0.0, half, half],
                    },
                ),
            ],
            assemblies: vec![],
        };
        let root = AssemblyDefinition {
            id: "root".into(),
            instances: vec![],
            assemblies: vec![AssemblyInstance {
                attachment: None,
                id: "occurrence".into(),
                assembly: "module".into(),
                transform: RigidTransform {
                    translation,
                    rotation: [half, 0.0, 0.0, half],
                },
            }],
        };
        BuiltAssembly::from_definitions(
            "root".into(),
            BTreeMap::from([
                ("a".into(), part(Some(1000.0))),
                ("b".into(), part(second_density)),
            ]),
            BTreeMap::from([("root".into(), root), ("module".into(), module)]),
            Tolerance::default(),
        )
        .unwrap()
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12_f64.max(b.abs() * 1e-11), "{a} != {b}");
    }
    fn tensor(actual: [[f64; 3]; 3], expected: [[f64; 3]; 3]) {
        for i in 0..3 {
            for j in 0..3 {
                close(actual[i][j], expected[i][j]);
            }
        }
    }
    #[test]
    fn mixed_density_nested_rotations_match_independent_parallel_axis_solution() {
        let assembly = nested([100.0, 200.0, 300.0], Some(3000.0));
        let p = assembly.mass_properties(Tolerance::default()).unwrap();
        close(p.volume, 96.0);
        close(p.mass_kg.unwrap(), 0.000192);
        for (a, b) in p
            .volume_centroid
            .unwrap()
            .into_iter()
            .zip([104.5, 197.0, 301.5])
        {
            close(a, b);
        }
        for (a, b) in p
            .center_of_mass
            .unwrap()
            .into_iter()
            .zip([106.25, 197.0, 301.25])
        {
            close(a, b);
        }
        tensor(
            p.central_volume_inertia.unwrap(),
            [
                [392.0, 0.0, 168.0],
                [0.0, 1360.0, 0.0],
                [168.0, 0.0, 1544.0],
            ],
        );
        tensor(
            p.central_mass_inertia.unwrap(),
            [
                [0.000724, 0.0, 0.000252],
                [0.0, 0.002120, 0.0],
                [0.000252, 0.0, 0.002548],
            ],
        );
        assert_eq!(p.missing_density_occurrences, 0);
        let shifted = nested([1000100.0, -1999800.0, 3000300.0], Some(3000.0))
            .mass_properties(Tolerance::default())
            .unwrap();
        tensor(
            shifted.central_volume_inertia.unwrap(),
            p.central_volume_inertia.unwrap(),
        );
        tensor(
            shifted.central_mass_inertia.unwrap(),
            p.central_mass_inertia.unwrap(),
        );
        for (a, b) in shifted
            .center_of_mass
            .unwrap()
            .into_iter()
            .zip([1000106.25, -1999803.0, 3000301.25])
        {
            close(a, b);
        }
    }
    #[test]
    fn overlaps_sum_material_and_missing_density_never_implies_zero_mass() {
        let a = BuiltAssembly::new(
            "a".into(),
            BTreeMap::from([("box".into(), part(Some(1000.0)))]),
            vec![
                instance("a", "box", RigidTransform::default()),
                instance("b", "box", RigidTransform::default()),
            ],
            Tolerance::default(),
        )
        .unwrap();
        let p = a.mass_properties(Tolerance::default()).unwrap();
        close(p.volume, 96.0);
        close(p.mass_kg.unwrap(), 0.000096);
        tensor(
            p.central_volume_inertia.unwrap(),
            [[416.0, 0.0, 0.0], [0.0, 320.0, 0.0], [0.0, 0.0, 160.0]],
        );
        let unknown = nested([0.0; 3], None)
            .mass_properties(Tolerance::default())
            .unwrap();
        assert_eq!(unknown.missing_density_occurrences, 1);
        assert_eq!(unknown.mass_kg, None);
        assert_eq!(unknown.center_of_mass, None);
        assert_eq!(unknown.central_mass_inertia, None);
        assert!(unknown.volume_centroid.is_some());
        assert!(unknown.central_volume_inertia.is_some());
    }
    #[test]
    fn empty_geometry_and_absent_occurrences_have_explicit_unknown_centers() {
        for known in [false, true] {
            let mut p = part(known.then_some(1000.0));
            p.result = Arc::new(
                cad_kernel::boolean(
                    &p.result.model,
                    &p.result.model,
                    cad_kernel::BooleanOp::Difference,
                    Tolerance::default(),
                )
                .unwrap(),
            );
            let assembly = BuiltAssembly::new(
                "a".into(),
                BTreeMap::from([("empty".into(), p)]),
                vec![instance("empty", "empty", RigidTransform::default())],
                Tolerance::default(),
            )
            .unwrap();
            let properties = assembly.mass_properties(Tolerance::default()).unwrap();
            assert_eq!(properties.volume, 0.0);
            assert_eq!(properties.volume_centroid, None);
            assert_eq!(properties.central_volume_inertia, None);
            assert_eq!(properties.mass_kg, known.then_some(0.0));
            assert_eq!(properties.center_of_mass, None);
            assert_eq!(properties.central_mass_inertia, None);
            assert_eq!(properties.missing_density_occurrences, usize::from(!known));
            assert_eq!(
                properties.mass_kg,
                assembly
                    .bill_of_materials(Tolerance::default())
                    .unwrap()
                    .total_mass_kg
            );
        }
        let empty = BuiltAssembly::new("a".into(), BTreeMap::new(), vec![], Tolerance::default())
            .unwrap()
            .mass_properties(Tolerance::default())
            .unwrap();
        assert_eq!(empty.mass_kg, Some(0.0));
        assert_eq!(empty.volume_centroid, None);
        assert_eq!(empty.center_of_mass, None);
    }
    #[test]
    fn same_hash_conflicting_models_are_rejected_before_definition_reuse() {
        let mut assembly = nested([0.0; 3], Some(3000.0));
        assembly.parts.get_mut("b").unwrap().result =
            Arc::new(cad_kernel::extrude_rectangle(3.0, 4.0, 6.0, Tolerance::default()).unwrap());
        assert!(matches!(
            assembly.mass_properties(Tolerance::default()),
            Err(AssemblyError::DefinitionHashCollision(_))
        ));
    }
    #[test]
    fn cancelled_world_centers_cannot_hide_rotation_and_parallel_axis_uncertainty() {
        let local = [1e10, 1e10, 1e10];
        let mut frame = RigidTransform {
            translation: [0.0; 3],
            rotation: [
                0.0,
                0.0,
                (std::f64::consts::PI / 12.0).sin(),
                (std::f64::consts::PI / 12.0).cos(),
            ],
        };
        frame.translation = frame.vector(local).map(|v| -v);
        assert!(matches!(
            placed_center(local, &frame, [0.0; 3], Tolerance::default()),
            Err(AssemblyError::NumericRange)
        ));
        // Exact signed permutations retain their exact-addition residual check.
        frame = RigidTransform {
            translation: [-1e10; 3],
            ..Default::default()
        };
        assert_eq!(
            placed_center(local, &frame, [0.0; 3], Tolerance::default()).unwrap(),
            ([0.0; 3], [0.0; 3])
        );
        let occurrences = [1e10, 1e10 + 2.0].map(|x| Occurrence {
            center: [x, 0.0, 0.0],
            center_error: [0.0; 3],
            volume: 1.0,
            inertia: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            inertia_error: [[0.0; 3]; 3],
            density: None,
        });
        // A permissive positional tolerance does not waive the relative tensor
        // error bound when close centroids are subtracted from large coordinates.
        assert!(matches!(
            aggregate(
                &occurrences,
                [-1e10, 0.0, 0.0],
                false,
                Tolerance { linear: 1.0 }
            ),
            Err(AssemblyError::NumericRange)
        ));
    }
}
