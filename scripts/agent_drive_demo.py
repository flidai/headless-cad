#!/usr/bin/env python3
"""Exercise the rotating drive through a real MCP client; no model/provider required.

This scripted fixture validates the client interface, not autonomous AI reasoning.
Only the client writes the requested new delivery directory. The MCP server cannot
write the source workspace or choose the client's delivery destination.
"""
import argparse
import hashlib
import html
import json
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


class Client:
    def __init__(self, cad, workspace, state_dir=None):
        command = [str(cad), 'mcp', '--workspace', str(workspace)]
        if state_dir is not None:
            command += ['--state-dir', str(state_dir)]
        self.process = subprocess.Popen(command,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.replies = queue.Queue()
        self.sequence = 0
        self.trace = []
        def reader():
            try:
                for line in self.process.stdout:
                    self.replies.put(json.loads(line))
            finally:
                self.replies.put(None)
        self.reader = threading.Thread(target=reader, daemon=True)
        self.reader.start()
        self.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                    'clientInfo': {'name': 'headless-cad-drive-fixture', 'version': '1'}})
        self.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})

    def send(self, value):
        self.process.stdin.write(json.dumps(value, ensure_ascii=False) + '\n')
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        self.send({'jsonrpc': '2.0', 'id': self.sequence, 'method': method, 'params': params})
        while True:
            reply = self.replies.get(timeout=80)
            if reply is None:
                raise RuntimeError('MCP closed before responding')
            if 'id' not in reply:
                continue
            assert reply['id'] == self.sequence, reply
            if 'error' in reply:
                raise RuntimeError(reply['error'])
            return reply['result']

    def tool(self, name, arguments, error=False):
        response = self.request('tools/call', {'name': name, 'arguments': arguments})
        result = response.get('structuredContent')
        assert response.get('isError', False) == error, (name, result)
        assert isinstance(result, dict) and json.loads(response['content'][0]['text']) == result
        self.trace.append({'tool': name, 'is_error': error, 'job_id': result.get('job_id'),
                           'snapshot_id': result.get('snapshot_id')})
        return result

    def download(self, job, destination, stream=False):
        started=time.perf_counter()
        delivery_stream=self.tool('cad_open_generation_stream',{'job_id':job['job_id'],'manifest_digest':job['manifest_hash']}) if stream else None
        destination.mkdir()
        read_calls=0
        for artifact in job['artifacts']:
            name = artifact['name']
            assert name and Path(name).name == name and name not in ('.', '..')
            offset = 0
            with (destination / name).open('wb') as file:
                while True:
                    read_args={'name':name,'offset':offset,'max_bytes':65536}
                    if stream:
                        read_args['stream_id']=delivery_stream['stream_id']
                        chunk=self.tool('cad_read_generation_stream',read_args)
                    else:
                        read_args['job_id']=job['job_id']
                        chunk=self.tool('cad_read_artifact',read_args)
                    read_calls+=1
                    assert chunk['blake3'] == artifact['blake3'] and chunk['offset'] == offset
                    data = chunk['text'].encode()
                    file.write(data)
                    assert chunk['next_offset'] == offset + len(data)
                    offset = chunk['next_offset']
                    if chunk['eof']:
                        assert offset == artifact['bytes']
                        break
                    assert data
        if stream:
            closed=self.tool('cad_close_generation_stream',{'stream_id':delivery_stream['stream_id']})
            assert closed['closed'] and closed['sealed'] and closed['manifest_digest']==job['manifest_hash']
        return {'mode':'verified_generation_stream' if stream else 'whole_job_check_per_read',
                'seconds':time.perf_counter()-started,'read_calls':read_calls,
                'bytes':sum(a['bytes'] for a in job['artifacts']),'artifacts':len(job['artifacts'])}

    def close(self):
        try:
            self.process.stdin.close()
            self.process.wait(timeout=5)
        except (BrokenPipeError, subprocess.TimeoutExpired):
            self.process.kill()
            self.process.wait()
        self.process.stdout.close()
        self.reader.join(timeout=2)


def run(cad, *args):
    result = subprocess.run([str(cad), *map(str, args)], capture_output=True, text=True, timeout=80)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


def drive_summary(manifest):
    """Check the delivered mechanical fixture and retain its explicit scope."""
    assembly = manifest['outputs']['drive_module_release']
    assert assembly['instances'] == 30
    assert assembly['definitions'] == 14
    members = manifest['rigid_rotation_groups']['rotor']['members']
    assert len(members) == 9
    has_machining_sheet = 'end_cover_machining' in manifest['drawing_sheets']
    has_axial_allowance = 'left_cover_minus_tip_allowance' in manifest['tolerance_stacks']
    assert len(manifest['datum_dimensions']) == (26 if has_axial_allowance else 20 if has_machining_sheet else 10)
    for feature in ('housing_left_pilots', 'housing_right_pilots'):
        assert feature in manifest['sketch_reports']
    return {'occurrences': assembly['instances'], 'definitions': assembly['definitions'],
            'rotor_members': len(members),
            'named_continuous_clearance_checks': len(manifest['clearance_checks']),
            'native_axial_dimensions': len(manifest['datum_dimensions']),
            'native_machining_dimension_rows': 14 if has_axial_allowance else 10 if has_machining_sheet else 0,
            'native_tip_allowance_stacks': 4 if has_axial_allowance else 0,
            'face_attached_pilot_patterns': ['housing_left_pilots', 'housing_right_pilots'],
            'scope': ('Native face-to-face machining rows and nominal 0.5 mm screw-tip floor gaps; authored size bands are conditional on seating and do not establish threads, preload or rated load'
                      if has_machining_sheet else
                      'Nominal end covers and unthreaded screw envelopes; named clearance checks do not certify every contact, thread engagement or load capacity')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=Path('target/release/cad'))
    parser.add_argument('--workspace', type=Path, default=Path('.'))
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    cad, workspace, destination = args.cad.resolve(), args.workspace.resolve(), args.out_dir.absolute()
    if destination.exists():
        raise SystemExit('Choose a new output directory; existing deliveries are never overwritten')
    source_path = workspace / 'examples/drive_module_rotation.star'
    source = source_path.read_text()
    initial_hash = hashlib.sha256(source_path.read_bytes()).hexdigest()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.agent-drive-', dir=destination.parent) as staging:
        root = Path(staging) / 'delivery'
        root.mkdir()
        client = Client(cad, workspace)
        try:
            snapshot = client.tool('cad_snapshot', {'recipe': 'examples/drive_module_rotation.star'})
            assert snapshot['source'] == source
            before = client.tool('cad_evaluate', {'snapshot_id': snapshot['snapshot_id'], 'mode': 'build',
                                                  'outputs': ['drive_module_release']})
            assert before['worker']['success']
            invalid_source = source.replace('name = "required_clearance", default = 0.09',
                                            'name = "required_clearance", default = 0.3')
            assert invalid_source != source
            invalid = client.tool('cad_revise', {'snapshot_id': snapshot['snapshot_id'], 'source': invalid_source})
            rejected = client.tool('cad_evaluate', {'snapshot_id': invalid['snapshot_id'], 'mode': 'build',
                                                    'outputs': ['drive_module_release']}, error=True)
            assert rejected['worker']['result']['code'] == 'clearance_check_failed', rejected
            revised_source = (invalid_source.replace('name = "required_clearance", default = 0.3',
                                                     'name = "required_clearance", default = 0.09')
                              .replace('name = "shaft_diameter", default = 12', 'name = "shaft_diameter", default = 14')
                              .replace('name = "mounting_pitch", default = 60', 'name = "mounting_pitch", default = 72')
                              .replace('design_revision = "A"', 'design_revision = "B"'))
            repaired = client.tool('cad_revise', {'snapshot_id': invalid['snapshot_id'], 'source': revised_source})
            after = client.tool('cad_evaluate', {'snapshot_id': repaired['snapshot_id'], 'mode': 'build',
                                                 'outputs': ['drive_module_release']})
            assert after['worker']['success']
            assert client.tool('cad_job', {'job_id': before['job_id']}) == before
            summaries = client.tool('cad_jobs', {})['jobs']
            assert len(summaries) == 3
            manifests = []
            for name, job in [('before', before), ('after', after)]:
                verified = client.tool('cad_verify', {'job_id': job['job_id'], 'manifest_digest': job['manifest_hash']})
                assert verified['success']
                client.download(job, root / name)
                run(cad, 'verify', root / name, '--manifest-hash', job['manifest_hash'])
                manifest = json.loads((root / name / 'manifest.json').read_text())
                drive_summary(manifest)
                manifests.append(manifest)
            trace = client.trace
        finally:
            client.close()
        assert hashlib.sha256(source_path.read_bytes()).hexdigest() == initial_hash
        # Offline delivery replay uses only captured bundle bytes, after server exit.
        with tempfile.TemporaryDirectory(prefix='cad-agent-drive-replay-') as replay:
            captured = root / 'after' / 'recipe.bundle.json'
            if not captured.exists():
                captured = root / 'after' / 'recipe.star'
            generation = Path(replay) / 'generation'
            run(cad, 'build', captured, '--output', 'drive_module_release',
                '--cache-dir', Path(replay) / 'cache', '--out-dir', generation)
            assert (generation / 'drive_module_release.step').read_bytes() == (root / 'after' / 'drive_module_release.step').read_bytes()
            for path in (root / 'after').glob('*.brep.json'):
                assert (generation / path.name).read_bytes() == path.read_bytes()
        report = {'valid': True, 'scope': 'Scripted MCP client acceptance; no autonomous AI reasoning claim',
                  'workspace_unchanged': True, 'offline_replay_native_and_step_equal': True,
                  'before_snapshot': snapshot['snapshot_id'], 'rejected_snapshot': invalid['snapshot_id'],
                  'after_snapshot': repaired['snapshot_id'], 'native_rejection': rejected['worker']['result'],
                  'jobs': summaries, 'worker_admission': {name: job['worker'].get('admission') for name, job in [('before', before), ('rejected', rejected), ('after', after)]},
                  'mechanical_fixture': {name: drive_summary(manifest) for name, manifest in zip(('before', 'after'), manifests)},
                  'tool_calls': len(trace), 'trace': trace}
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        title = 'Recipe proposal → native rejection → repaired rotating drive'
        (root / 'index.html').write_text(f'''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title><style>body{{font:17px system-ui;background:#0c1722;color:#e6f0f6;max-width:980px;margin:70px auto;padding:24px}}a{{color:#75d9c6}}.cards{{display:flex;gap:24px;flex-wrap:wrap}}section{{background:#182938;padding:26px;flex:1;border-radius:12px;min-width:250px}}small{{color:#acbecb}}h1{{line-height:1.15}}li{{margin:10px 0}}</style>
<small>HEADLESS CAD · EXPERIMENTAL · SOURCE-AUTHORITATIVE</small><h1>{html.escape(title)}</h1>
<p>A real local MCP client captured the Starlark recipe, built a candidate, received a native clearance failure, repaired the source, and delivered verified artifacts. The source workspace was unchanged.</p>
<div class="cards"><section><h2>Revision A</h2><p>12 mm shaft · 60 mm mounting pitch</p><a href="before/rotation-rotor.html">Inspect rotating drive and captured source →</a></section>
<section><h2>Revision B</h2><p>14 mm shaft · 72 mm mounting pitch</p><a href="after/rotation-rotor.html">Inspect repaired revision and captured source →</a></section></div>
<ul><li>30 components from 14 definitions, including two removable bearing covers and four nominal screw envelopes.</li><li>9 rotor members, {len(manifests[1]['clearance_checks'])} named continuous clearance checks, {len(manifests[1]['datum_dimensions'])} native axial dimensions, including fourteen measured machining rows.</li><li>Nominal screw-tip floor gap: 0.5 mm; threads, preload and rated loads remain unverified.</li><li>Invalid 0.3 mm minimum clearance rejected; earlier candidate preserved.</li><li>Captured source replayed offline after server exit with identical native B-reps and STEP.</li></ul>
<p><a href="report.json">Client trace and native rejection</a> · <a href="after/clearance-checks.html">Clearance evidence</a> · <a href="after/drawing-end_cover_machining.svg">End-cover machining sheet</a> · <a href="after/tolerance-stacks.html">Tolerance stacks</a> · <a href="after/drive_module_release.step">Download STEP</a></p>
<small>This is a scripted interface acceptance fixture, not a claim of autonomous engineering. Viewer controls inspect only. Named-pair proofs do not certify all assembly contacts or rated loads.</small>''')
        root.rename(destination)
    print(json.dumps({'valid': True, 'url_path': str(destination / 'index.html'), 'tool_calls': len(trace)}, indent=2))


if __name__ == '__main__':
    main()
