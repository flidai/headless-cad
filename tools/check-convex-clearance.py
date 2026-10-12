#!/usr/bin/env python3
"""Independent OCCT placed-solid clearance oracle. OCP is test-only."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'crates/cad-export/tools'))
from check_assembly_ocp import label_entry, label_name, placement_matrix, require, LabelSequence
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeVertex
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepExtrema import BRepExtrema_DistShapeShape
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.TCollection import TCollection_ExtendedString
from OCP.TDF import TDF_Label
from OCP.TDocStd import TDocStd_Document
from OCP.TopLoc import TopLoc_Location
from OCP.XCAFDoc import XCAFDoc_DocumentTool
from OCP.gp import gp_Pnt


def imported_leaves(path):
    reader = STEPCAFControl_Reader()
    reader.SetNameMode(True)
    require(reader.ReadFile(str(path)) == IFSelect_RetDone, 'STEP parse failed')
    document = TDocStd_Document(TCollection_ExtendedString('clearance oracle'))
    require(reader.Transfer(document), 'XCAF transfer failed')
    tool = XCAFDoc_DocumentTool.ShapeTool_s(document.Main())
    roots = LabelSequence()
    tool.GetFreeShapes(roots)
    require(roots.Length() == 1, 'one hierarchical assembly root required')
    leaves = {}

    def walk(parent, path, placement, active):
        identity = label_entry(parent)
        require(identity not in active, 'cyclic imported hierarchy')
        components = LabelSequence()
        require(tool.GetComponents_s(parent, components), 'enumerate components')
        for index in range(1, components.Length() + 1):
            component = components.Value(index)
            reference = TDF_Label()
            require(tool.GetReferredShape_s(component, reference), 'missing component definition')
            key = path + (label_name(component),)
            world = placement.Multiplied(tool.GetLocation_s(component))
            if tool.IsAssembly_s(reference):
                walk(reference, key, world, active | {identity})
            else:
                require(key not in leaves, 'duplicate exact leaf path')
                shape = tool.GetShape_s(reference).Moved(world)
                require(not shape.IsNull() and BRepCheck_Analyzer(shape).IsValid(), 'invalid placed leaf')
                transform = world.Transformation()
                matrix = [[transform.Value(r, c) for c in range(1, 5)] for r in range(1, 4)]
                leaves[key] = dict(shape=shape, matrix=matrix, part=label_name(reference))
    walk(roots.Value(1), (), TopLoc_Location(), set())
    return leaves


def distance(left, right):
    query = BRepExtrema_DistShapeShape(left, right)
    require(query.IsDone() and query.NbSolution() > 0, 'independent distance query failed')
    result = query.Value()
    require(math.isfinite(result) and result >= 0, 'nonfinite independent distance')
    return result


def witness_distance(point, shape):
    require(len(point) == 3 and all(math.isfinite(v) for v in point), 'invalid native witness')
    return distance(BRepBuilderAPI_MakeVertex(gp_Pnt(*point)).Vertex(), shape)


def run(cad, recipe, output, parameters):
    args = [str(cad), 'build', str(recipe), '--out-dir', str(output)]
    for key, value in parameters.items():
        args += ['--param', f'{key}={value}']
    return subprocess.run(args, capture_output=True, text=True)


def inventory(directory):
    import hashlib
    return {str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(directory.rglob('*')) if path.is_file()}


def close(actual, expected, tolerance=2e-6):
    require(abs(actual - expected) <= tolerance, f'{actual} differs from {expected}')


def check(cad, output, parameters, summary):
    report = summary['clearance_checks']['jaw_separation']
    require(report['status'] == 'passed', 'native check must pass')
    query = report['query']
    leaves = imported_leaves(output / 'clearance_station.step')
    left = leaves[('cell', 'fixed')]
    right = leaves[('cell', 'moving')]
    actual = distance(left['shape'], right['shape'])
    # OCCT STEP import and independent extrema have floating numerical error;
    # this tolerance compares implementations, not a proof of exact enclosure.
    allowance = 2e-6
    require(report['lower_bound_mm'] - allowance <= actual <= report['upper_bound_mm'] + allowance,
            'independent solid distance outside native interval')
    require(query['lower_bound_mm'] - allowance <= actual <= query['upper_bound_mm'] + allowance,
            'independent distance outside query interval')
    witness = math.dist(query['left_point'], query['right_point'])
    close(witness, query['upper_bound_mm'])
    left_surface = witness_distance(query['left_point'], left['shape'])
    right_surface = witness_distance(query['right_point'], right['shape'])
    require(left_surface <= allowance and right_surface <= allowance, 'native witnesses lie off imported surfaces')
    assembly = json.loads((output / 'clearance_station.assembly.json').read_text())
    for side, path in [('left', ('cell', 'fixed')), ('right', ('cell', 'moving'))]:
        bound = report[side]
        require(tuple(bound['instance_path']) == path, 'report exact occurrence path')
        matching = [i for i in assembly['instances'] if tuple(assembly['instance_paths'][i['id']]) == path]
        require(len(matching) == 1, 'unique native exact path')
        instance = matching[0]
        require(bound['instance_id'] == instance['id'] and bound['part_id'] == instance['part'], 'native occurrence identity')
        for matrix in [placement_matrix(instance['transform']), placement_matrix(bound['frame'])]:
            for row, reference in zip(matrix, leaves[path]['matrix']):
                for value, expected in zip(row, reference):
                    close(value, expected, 1e-8)
    verified = subprocess.run([str(cad), 'verify', str(output)], capture_output=True, text=True)
    require(verified.returncode == 0 and json.loads(verified.stdout)['valid'], 'generation integrity')
    return dict(parameters=parameters, imported_distance_mm=actual,
                native_interval_mm=[report['lower_bound_mm'], report['upper_bound_mm']],
                witness_distance_mm=witness, witness_surface_distances_mm=[left_surface, right_surface],
                importer_comparison_allowance_mm=allowance)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=ROOT / 'target/release/cad')
    parser.add_argument('--recipe', type=Path, default=ROOT / 'examples/clearance_station.star')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=False)
    output = args.out_dir / 'generation'
    cases = []
    configurations = [dict(gap=gap, pose=pose, turn=1) for gap in [5, 10, 20] for pose in [0, 1, 2]]
    configurations += [dict(gap=10, pose=0, turn=0), dict(gap=10, y_gap=-20, z_gap=-10, pose=0, turn=0)]
    for parameters in configurations:
        result = run(args.cad, args.recipe, output, parameters)
        require(result.returncode == 0, 'positive clearance build failed: ' + result.stderr)
        case = check(args.cad, output, parameters, json.loads(result.stdout))
        if parameters.get('y_gap') == -20:
            close(case['imported_distance_mm'], 10.)
        cases.append(case)
    failures = []
    for parameters in [dict(gap=-40, y_gap=-20, z_gap=-10, turn=0), dict(required_clearance=100)]:
        before = inventory(output)
        result = run(args.cad, args.recipe, output, parameters)
        require(result.returncode != 0, 'invalid/unmet clearance unexpectedly passed')
        require(inventory(output) == before, 'failed clearance modified publication')
        diagnostic = json.loads(result.stderr)
        require('jaw_separation' in json.dumps(diagnostic), 'failure must identify source check')
        failures.append(dict(parameters=parameters, diagnostic=diagnostic, generation_preserved=True))
    import importlib.metadata
    import platform
    result = dict(valid=True, cases=cases, rejected=failures, python=platform.python_version(),
                  ocp=importlib.metadata.version('cadquery-ocp'),
                  scope='independent static convex placed-solid separation; no motion or general curved clearance claim')
    (args.out_dir / 'report.json').write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
