#!/usr/bin/env python3
"""Local Unix benchmark of exact dependency evaluation, excluding export/viewing.

Uses wait4 per child for peak resident KiB on Linux; no geometry cache supplied.
Not a hardware-independent latency or memory guarantee.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', default='target/release/cad')
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--report', required=True)
    args = parser.parse_args()
    if args.runs < 1 or platform.system() != 'Linux':
        parser.error('requires Linux and a positive run count (wait4 RSS units)')
    root = Path(__file__).resolve().parents[1]
    cad = Path(args.cad).resolve()
    recipe = root / 'examples/workcell_line.star'
    base = [str(cad), 'inspect', str(recipe), '--param', 'stations=1000',
            '--param', 'columns=25', '--param', 'variants=40']
    samples = {'all': [], 'service_station': []}
    for _ in range(args.runs):
        for mode in samples:
            command = base + (['--output', 'service_station'] if mode != 'all' else [])
            with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
                start = time.perf_counter()
                child = subprocess.Popen(command, stdout=out, stderr=err, cwd=root)
                _, status, usage = os.wait4(child.pid, 0)
                wall = time.perf_counter() - start
                child.returncode = os.waitstatus_to_exitcode(status)
                if child.returncode:
                    err.seek(0)
                    raise RuntimeError(err.read().decode())
                out.seek(0)
                report = json.load(out)
            scope = report['scope']
            samples[mode].append({
                'wall_seconds': wall, 'peak_rss_kib': usage.ru_maxrss,
                'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
                'stats': report['stats'],
                'evaluated_features': len(scope['evaluated_features']),
                'skipped_features': len(scope['skipped_features']),
                'outputs': sorted(report['outputs']),
            })
    document = {
        'schema_version': 1,
        'description': 'Inspect 1000 stations / 40 tool variants; no explicit geometry cache; alternating all/selected fresh processes.',
        'limitations': 'Local shared-host observations, not release gates. RSS includes compiler, full structural checks, selected geometry, assembly materialization and JSON reporting. OS file caches are uncontrolled. No exact-body residency limit.',
        'platform': platform.platform(),
        'logical_cpus': os.cpu_count(),
        'binary_sha256': hashlib.sha256(cad.read_bytes()).hexdigest(),
        'recipe_sha256': hashlib.sha256(recipe.read_bytes()).hexdigest(),
        'executor_build_id': report['executor_build_id'],
        'parameters': {'stations': 1000, 'columns': 25, 'variants': 40},
        'samples': samples,
        'medians': {mode: {
            'wall_seconds': statistics.median(s['wall_seconds'] for s in data),
            'peak_rss_kib': statistics.median(s['peak_rss_kib'] for s in data),
        } for mode, data in samples.items()},
    }
    Path(args.report).write_text(json.dumps(document, indent=2) + '\n')
    print(json.dumps(document['medians'], indent=2))


if __name__ == '__main__':
    main()
