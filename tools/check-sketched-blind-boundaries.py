#!/usr/bin/env python3
"""Test safe and refusing blind-hole boundaries against actual CLI generations.

Four configurations cover upper/lower caps and identity/oblique body poses.
Rejected builds target a previously published generation and must preserve all
of its artifact bytes. This is a bounded parameter sweep, not a general proof
for all tolerances, tools, and stock geometries.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def inventory(directory):
    return {path.name: sha256(path) for path in directory.iterdir() if path.is_file()}


def invoke(cad, source, out, parameters):
    args = [str(cad), 'build', str(source), '--out-dir', str(out)]
    for name, value in sorted(parameters.items()):
        args += ['--param', f'{name}={value}']
    result = subprocess.run(args, capture_output=True, text=True, timeout=30)
    return result, json.loads(result.stdout if result.returncode == 0 else result.stderr)


def verify(cad, out):
    result = subprocess.run([str(cad), 'verify', str(out)],
                            capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)['valid'] is True


def expected_volume(parameters):
    radius = parameters.get('hole_radius', 2)
    depth = parameters.get('hole_depth', 5)
    return (50*36-(4-math.pi)*16-math.pi*49)*12-2*math.pi*radius*radius*depth


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=ROOT / 'target/release/cad')
    parser.add_argument('--source', type=Path,
                        default=ROOT / 'examples/sketched_blind_housing.star')
    parser.add_argument('--out-report', type=Path, required=True)
    args = parser.parse_args()
    cad, source, report_path = args.cad.resolve(), args.source.resolve(), args.out_report.resolve()
    assert not report_path.exists(), 'choose a new report path'
    binary_sha, source_sha = sha256(cad), sha256(source)
    records = []
    with tempfile.TemporaryDirectory(prefix='cad-blind-adversarial-') as temporary:
        root = Path(temporary)
        for underside in (0, 1):
            for pose in (0, 2):
                base = {'underside': underside, 'pose': pose,
                        'pattern_y': 3 if pose == 2 else 0}
                out = root / f'cap-{underside}-pose-{pose}'
                result, _ = invoke(cad, source, out, base)
                assert result.returncode == 0, result.stderr
                verify(cad, out)
                # With nonzero frame Y, the central bore tangent is radial:
                # pitch² + offset² = (central radius + drill radius)².
                central_tangent_pitch = math.sqrt((7+2)**2 - base['pattern_y']**2)
                cases = (
                    ('central-clearance-0.25', {'half_pitch': central_tangent_pitch+0.25}, True),
                    ('central-tangent', {'half_pitch': central_tangent_pitch}, False),
                    ('central-overlap-0.25', {'half_pitch': central_tangent_pitch-0.25}, False),
                    ('outer-clearance-0.25', {'half_pitch': 22.75}, True),
                    ('outer-tangent', {'half_pitch': 23}, False),
                    ('outer-breakthrough-0.25', {'half_pitch': 23.25}, False),
                    ('floor-remains-0.5', {'hole_depth': 11.5}, True),
                    ('floor-breakthrough', {'hole_depth': 12}, False),
                )
                for label, extra, should_pass in cases:
                    parameters = {**base, **extra}
                    before = inventory(out)
                    result, payload = invoke(cad, source, out, parameters)
                    assert (result.returncode == 0) == should_pass, (
                        label, parameters, result.stderr)
                    if should_pass:
                        actual = payload['outputs']['housing']['volume_mm3']
                        expected = expected_volume(parameters)
                        assert abs(actual-expected) < 1e-6, (label, actual, expected)
                        verify(cad, out)
                        records.append({'case': label, 'parameters': parameters,
                                        'accepted': True, 'volume_mm3': actual,
                                        'expected_volume_mm3': expected})
                    else:
                        assert inventory(out) == before, (label, 'generation changed')
                        assert not list(root.glob('.cad-stage-*')), (label, 'stage remains')
                        verify(cad, out)
                        records.append({'case': label, 'parameters': parameters,
                                        'accepted': False, 'error_code': payload.get('code'),
                                        'preserved_files': len(before)})

    assert sha256(cad) == binary_sha, 'CAD binary changed during sweep'
    assert sha256(source) == source_sha, 'recipe source changed during sweep'
    report = {
        'valid': True, 'executable_sha256': binary_sha, 'source_sha256': source_sha,
        'domain': 'rounded 50x36x12 R4 stock, existing central through bore R7, two blind R2 holes; identity and oblique pose, both caps',
        'acceptance_rule': 'safe-clearance offset 0.25 mm, positive floor 0.5 mm; exact tangent and overlapping/breakthrough cases refuse',
        'cases': records, 'accepted': sum(x['accepted'] for x in records),
        'rejected': sum(not x['accepted'] for x in records),
    }
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'valid': True, 'accepted': report['accepted'],
                      'rejected': report['rejected'], 'report': str(report_path)}))


if __name__ == '__main__':
    main()
