"""Independent constrained-sketch → STEP acceptance (optional OCP environment).

Run after `cargo build -p cad-cli`:
  python crates/cad-export/tools/check_constrained_fixture_ocp.py \
    --cad target/debug/cad --out-dir /tmp/constrained-fixture-oracle

Builds three recipe configurations, checks captured-source/solver evidence against
returned coordinates, derives the positive triangle branch from three lengths,
and independently imports body and hierarchical assembly STEP. OCP remains a
check-only dependency; no solver output is used to derive nominal expectations.
"""
import argparse
import json
import math
import pathlib
import subprocess
import sys
import tempfile

from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from check_assembly_ocp import placement_matrix

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT / 'examples/constrained_slope_fixture.star'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def close(actual, expected, tolerance, label):
    require(math.isfinite(actual) and abs(actual - expected) <= tolerance,
            f'{label}: {actual} != {expected} (tolerance {tolerance})')


def json_output(command):
    output = subprocess.run(command, check=True, capture_output=True, text=True)
    # OCP can print reader diagnostics before the checker's final JSON object.
    return json.loads(output.stdout[output.stdout.index('{'):])


def body_properties(step):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(step)) == IFSelect_RetDone, 'STEP read failed')
    require(reader.TransferRoots() > 0, 'STEP transfer failed')
    shape = reader.OneShape()
    require(BRepCheck_Analyzer(shape).IsValid(), 'imported support is invalid')
    props = GProp_GProps()
    BRepGProp.VolumeProperties_s(shape, props)
    center = props.CentreOfMass()
    inertia = props.MatrixOfInertia()
    return props.Mass(), [center.X(), center.Y(), center.Z()], [
        [inertia.Value(i + 1, j + 1) for j in range(3)] for i in range(3)]


def check(cad, root, name, span, left, right):
    directory = root / name
    parameters = {'span': span, 'left_length': left, 'right_length': right}
    command = [str(cad), 'build', str(RECIPE), '--out-dir', str(directory)]
    for key, value in parameters.items():
        command += ['--param', f'{key}={value}']
    subprocess.run(command, check=True, capture_output=True, text=True)
    manifest = json.loads((directory / 'manifest.json').read_text())
    require((directory / 'recipe.star').read_bytes() == RECIPE.read_bytes(),
            'published recipe differs from the executed source')
    require(manifest['recipe_source']['parameter_overrides'] == parameters,
            'captured parameter overrides differ')
    report = manifest['sketch_reports']['support_sketch']
    require(report['status'] == 'fully_constrained', 'sketch did not solve fully')
    require(report['rank'] == 6 and report['degrees_of_freedom'] == 0,
            'unexpected local rank or freedom')
    require(report['solver_build_id'] == manifest['solver_build_id']
            and len(report['solver_build_id']) == 64, 'solver evidence identity mismatch')
    tolerance = manifest['tolerance']['linear']
    require(report['tolerance_mm'] == tolerance, 'solver tolerance mismatch')
    condition = report['condition_bound']
    require(math.isfinite(condition) and condition <= 1e8, 'unstable sketch accepted')
    a, b, c = [report['points'][key] for key in ['a', 'b', 'c']]
    residuals = {'ground': math.hypot(*a), 'level_base': abs(a[1] - b[1]),
                 'base_length': abs(math.dist(a, b) - span),
                 'left_length': abs(math.dist(a, c) - left),
                 'right_length': abs(math.dist(b, c) - right)}
    require(max(residuals.values()) <= tolerance, 'returned points violate source dimensions')
    reported = {item['id']: item['residual_mm'] for item in report['constraint_residuals']}
    require(reported.keys() == residuals.keys(), 'missing or extraneous constraint evidence')
    for key, value in residuals.items():
        close(reported[key], value, 128 * sys.float_info.epsilon * max(span, left, right),
              f'{key} published residual')
    close(report['max_residual_mm'], max(residuals.values()),
          128 * sys.float_info.epsilon * max(span, left, right), 'maximum residual')

    # Positive circle-circle intersection; independent of iterates and topology.
    x = (left * left + span * span - right * right) / (2 * span)
    y = math.sqrt(left * left - x * x)
    linear_error = max(1e-6, 4 * condition * tolerance)
    for actual, expected in zip(c, [x, y]):
        close(actual, expected, linear_error, 'solved positive triangle branch')
    volume = 20 * span * y
    expected_center = [(span + x) / 3, 20, y / 3]
    var_x = (span * span + x * x + span * x) / 6 - ((span + x) / 3) ** 2
    var_z = y * y / 18
    covariance = y * (span + 2 * x) / 12 - (span + x) * y / 9
    expected_inertia = [[volume * (var_z + 1600 / 12), 0, -volume * covariance],
                        [0, volume * (var_x + var_z), 0],
                        [-volume * covariance, 0, volume * (var_x + 1600 / 12)]]
    actual_volume, center, inertia = body_properties(directory / 'support.step')
    volume_error = 40 * span * linear_error
    close(actual_volume, volume, volume_error, 'imported support volume')
    for actual, expected in zip(center, expected_center):
        close(actual, expected, linear_error, 'imported support centroid')
    moment_error = 20 * linear_error * volume * max(span, 40)
    for i in range(3):
        for j in range(3):
            close(inertia[i][j], expected_inertia[i][j], moment_error,
                  f'imported support inertia {i},{j}')

    assembly_path = directory / 'constrained_slope_fixture.assembly.json'
    assembly = json.loads(assembly_path.read_text())
    deck = next(item for item in assembly['instances'] if item['id'] == 'deck')
    matrix = placement_matrix(deck['transform'])
    normal = [y / right, 0, (span - x) / right]
    axis = [normal[2], 0, -normal[0]]
    seat = [10 + (span + x) / 2, 30, 6 + y / 2]
    origin = [seat[i] - 12 * axis[i] - (24 if i == 1 else 0) for i in range(3)]
    for i in range(3):
        close(matrix[i][3], origin[i], linear_error, 'independent deck seat translation')
        close(matrix[i][0], axis[i], linear_error / right, 'independent deck tangent')
        close(matrix[i][2], normal[i], linear_error / right, 'independent deck normal')
    # Existing independent importer verifies every native hierarchy relationship,
    # shared definition, local placement and recursively composed leaf placement.
    imported = json_output([sys.executable, str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),
                            str(directory / 'constrained_slope_fixture.step'),
                            '--instances', '9', '--definitions', '5', '--solids', '9',
                            '--assembly-json', str(assembly_path)])
    require(imported['hierarchy_checked'] and imported['placements_checked'],
            'hierarchical placement oracle did not run')
    total = 360 * span + 6336 + volume + 4176 + 342 * math.pi
    close(imported['material_volume_mm3'], total, volume_error, 'imported assembly material volume')
    for actual, expected in zip(imported['bounds_mm'], [0, 0, 0, span + 20, 60, y + 6]):
        close(actual, expected, linear_error, 'imported assembly bounds')
    return {'case': name, 'parameters': parameters, 'support_volume_mm3': actual_volume,
            'assembly_volume_mm3': imported['material_volume_mm3'],
            'max_residual_mm': report['max_residual_mm'], 'condition_bound': condition,
            'volume_tolerance_mm3': volume_error, 'linear_tolerance_mm': linear_error,
            'feature_hash': manifest['feature_hashes']['support_sketch'],
            'solver_build_id': report['solver_build_id'], 'hierarchy_checked': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=pathlib.Path, required=True)
    parser.add_argument('--out-dir', type=pathlib.Path)
    args = parser.parse_args()
    temporary = tempfile.TemporaryDirectory(prefix='cad-constrained-oracle-') if args.out_dir is None else None
    root = args.out_dir.resolve() if args.out_dir else pathlib.Path(temporary.name)
    root.mkdir(parents=True, exist_ok=True)
    results = [check(args.cad.resolve(), root, *case) for case in [
        ('default', 80., 60., 70.), ('left_edit', 80., 62., 70.), ('span_edit', 100., 80., 85.)]]
    require(len({r['feature_hash'] for r in results}) == 3, 'dimension edits did not change sketch identity')
    require(len({r['solver_build_id'] for r in results}) == 1, 'solver changed during oracle run')
    print(json.dumps({'valid': True, 'cases': results}, indent=2))
    if temporary:
        temporary.cleanup()


if __name__ == '__main__':
    main()
