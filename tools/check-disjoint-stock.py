#!/usr/bin/env python3
"""Independent exact disconnected-stock acceptance; OCP is a test-only oracle.

Checks native/STEP full moments, solid counts, directed geometric differences,
oriented closed STL, later intersecting cuts, and atomic resource rejection.
Optional baseline timings distinguish a refused workload from a successful one.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics
import subprocess
import time

SPEC = importlib.util.spec_from_file_location('keyseat_helpers', Path(__file__).with_name('check-keyseat.py'))
k = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(k)
from OCP.BRep import BRep_Builder
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox
from OCP.TopAbs import TopAbs_SOLID, TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS_Compound
from OCP.gp import gp_Pnt


def command(cad, *args):
    started = time.monotonic()
    result = subprocess.run([str(cad), *map(str, args)], capture_output=True, text=True, timeout=120)
    return result, time.monotonic() - started


def success(cad, *args):
    result, elapsed = command(cad, *args)
    k.require(result.returncode == 0, result.stderr)
    return json.loads(result.stdout), elapsed


def count(shape, kind):
    explorer, n = TopExp_Explorer(shape, kind), 0
    while explorer.More():
        n += 1
        explorer.Next()
    return n


def compound(boxes):
    result = TopoDS_Compound()
    builder = BRep_Builder()
    builder.MakeCompound(result)
    for origin, dimensions in boxes:
        builder.Add(result, BRepPrimAPI_MakeBox(gp_Pnt(*origin), *dimensions).Shape())
    return result


def moments(boxes):
    entries = []
    for origin, dims in boxes:
        v = dims[0] * dims[1] * dims[2]
        center = [origin[i] + dims[i] / 2 for i in range(3)]
        local = [v * (dims[(i+1) % 3]**2 + dims[(i+2) % 3]**2) / 12 for i in range(3)]
        entries.append((v, center, local))
    volume = sum(e[0] for e in entries)
    center = [sum(v*c[i] for v, c, _ in entries) / volume for i in range(3)]
    inertia = [[0.0]*3 for _ in range(3)]
    for v, c, local in entries:
        r = [c[i] - center[i] for i in range(3)]
        for i in range(3):
            for j in range(3):
                inertia[i][j] += (local[i] + v*sum(x*x for x in r) if i == j else 0) - v*r[i]*r[j]
    return volume, center, inertia


def validate(cad, source, directory, name, boxes, parameters=()):
    exact = moments(boxes)
    native, _ = success(cad, 'properties', source, '--output', name, *parameters)
    actual = k.load(str(directory / f'{name}.step'))
    expected = compound(boxes)
    imported = k.properties(actual)
    k.close(native['volume'], exact[0])
    for observed in [imported, (native['volume'], native['volume_centroid'], native['central_volume_inertia'])]:
        k.close(observed[0], exact[0])
        for i in range(3):
            k.close(observed[1][i], exact[1][i])
            for j in range(3):
                k.close(observed[2][i][j], exact[2][i][j])
    k.require(count(actual, TopAbs_SOLID) == len(boxes), 'exact separate material components')
    differences = [k.properties(k.boolean(BRepAlgoAPI_Cut, a, b))[0] for a, b in [(actual, expected), (expected, actual)]]
    for difference in differences:
        k.close(difference, 0)
    k.mesh(directory / f'{name}.stl', exact[0], absolute_error=1e-6)
    return {'solids': len(boxes), 'step_faces': count(actual, TopAbs_FACE), 'volume_mm3': exact[0],
            'centroid_mm': exact[1], 'central_inertia_mm5': exact[2], 'directed_difference_mm3': differences}


def diagonal_source(n, overlap=False):
    ending = '    output(name="cluster",feature=result)\n'
    if overlap:
        ending = ('    shifted=translate(id="shifted",body=result,offset=[0.5,0.5,0.5])\n'
                  '    result=union(id="overlap",left=result,right=shifted)\n' + ending)
    return ('def model():\n    p=rectangle(id="p",width=1,depth=1)\n'
            '    b=extrude(id="b",profile=p,height=1)\n    result=b\n'
            f'    for i in range(1,{n}):\n'
            '        placed=translate(id="at_"+str(i),body=b,offset=[2*i,2*i,2*i])\n'
            '        result=union(id="union_"+str(i),left=result,right=placed)\n' + ending + 'model()\n')


def inventory(path):
    return {str(p.relative_to(path)): hashlib.sha256(p.read_bytes()).hexdigest() for p in path.rglob('*') if p.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=k.ROOT / 'target/release/cad')
    parser.add_argument('--baseline-cad', type=Path)
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    cad, root = args.cad.resolve(), args.out_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)
    cases, timings = [], []
    for n in [8, 16, 24, 36, 64]:
        source, output = root / f'diagonal-{n}.star', root / f'diagonal-{n}'
        source.write_text(diagonal_source(n))
        built, elapsed = success(cad, 'build', source, '--out-dir', output)
        k.require(built['outputs']['cluster']['topology']['faces'] == 6*n, 'no artificial face splitting')
        case = validate(cad, source, output, 'cluster', [([2*i]*3, [1]*3) for i in range(n)])
        case.update({'case': f'diagonal-{n}', 'build_seconds': elapsed})
        cases.append(case)
        for label, binary in [('current', cad), ('baseline', args.baseline_cad)]:
            if binary is None:
                continue
            samples, statuses = [], []
            for _ in range(3):
                r, seconds = command(binary, 'inspect', source)
                samples.append(seconds)
                payload = json.loads(r.stdout if r.returncode == 0 else r.stderr)
                statuses.append({'exit_code': r.returncode, 'code': payload.get('code'), 'feature': payload.get('feature')})
            timings.append({'case': f'diagonal-{n}', 'binary': label, 'median_seconds': statistics.median(samples), 'outcomes': statuses})
        success(cad, 'verify', output)
    source = k.ROOT / 'examples/billet_batch.star'
    for rows, columns in [(1, 1), (8, 8), (12, 12)]:
        output = root / f'batch-{rows}-{columns}'
        parameters = ['--param', f'rows={rows}', '--param', f'columns={columns}']
        built, elapsed = success(cad, 'build', source, '--out-dir', output, *parameters)
        boxes = [([12*x, 8*y, 0], [10, 6, 3]) for y in range(rows) for x in range(columns)]
        k.require(built['outputs']['batch']['topology']['faces'] == 6*rows*columns, 'native batch face count')
        for name in ['batch', 'trimmed']:
            expected = boxes if name == 'batch' else [([2, 0, 0], [8, 6, 3]), *boxes[1:]]
            case = validate(cad, source, output, name, expected, parameters)
            case.update({'case': f'batch-{rows}-{columns}-{name}', 'build_seconds': elapsed})
            cases.append(case)
        success(cad, 'verify', output)
    accepted = root / 'diagonal-36'
    before = inventory(accepted)
    bad = root / 'overlapping-compounds.star'
    bad.write_text(diagonal_source(36, overlap=True))
    rejected, _ = command(cad, 'build', bad, '--out-dir', accepted)
    k.require(rejected.returncode != 0, 'overlapping grid must retain its resource guard')
    diagnostic = json.loads(rejected.stderr)
    k.require(diagnostic['feature'] == 'overlap' and 'resource limit' in diagnostic['diagnostic'], diagnostic)
    k.require(inventory(accepted) == before, 'resource failure preserves accepted generation')
    report = {'valid': True, 'cases': cases, 'timings': timings, 'native_resource_rejection': diagnostic,
              'scope': 'Exact separated orthogonal solids; validation remains quadratic; timings are local observations, not a hardware acceptance gate'}
    (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
