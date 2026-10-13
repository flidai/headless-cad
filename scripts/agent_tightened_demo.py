#!/usr/bin/env python3
"""Exercise seated mounting endpoints through durable local MCP acceptance.

This is a scripted client fixture. It does not claim autonomous design,
manufacturing release, fastener preload, or collision coverage beyond named pairs.
"""

import argparse
import hashlib
import html
import json
from pathlib import Path
import shutil
import tempfile

from agent_drive_demo import Client, run
from evidence import publish_new
from replay_delivery import replay_accepted, sha256


# The MCP snapshot's load labels are workspace relative. Use examples/ as the
# confined workspace so the source's `load("drive_module_rotation.star", ...)`
# resolves to the immutable sibling module without changing either source.
SOURCE = 'drive_module_tightened.star'
MODULE = 'drive_module_rotation.star'
LOCK = 'drive_module_tightened.star.lock.json'
ENDPOINT_DECLARATION = 'mounting_endpoint = param(name = "mounting_endpoint", default = -1)'
NUT_DECLARATION = 'nut_drop = param(name = "nut_drop", default = 0)'


def changed(source, declaration, replacement):
    if source.count(declaration) != 1:
        raise ValueError('expected one source-authored mounting parameter')
    return source.replace(declaration, replacement, 1)


def fixture(manifest, endpoint):
    assembly = manifest['outputs']['drive_module_tightened']
    if (assembly['instances'], assembly['definitions']) != (30, 14):
        raise ValueError('unexpected tightened drive occurrence or definition count')
    seats, gaps = {}, {}
    for i in range(4):
        for name in (f'tight_bolt_{i}_head_seat', f'tight_nut_{i}_base_seat'):
            check = manifest['nominal_seating_checks'][name]
            if (check['status'] != 'passed'
                    or check['assembly_content_hash'] != assembly['content_hash']
                    or check['disk_radius_lower_mm'] < 0.4
                    or not check['signed_gap_lower_mm'] <= 0 <= check['signed_gap_upper_mm']):
                raise ValueError(f'uncertified seating check {name}')
            seats[name] = {'disk_radius_lower_mm': check['disk_radius_lower_mm'],
                           'signed_gap_lower_mm': check['signed_gap_lower_mm'],
                           'signed_gap_upper_mm': check['signed_gap_upper_mm']}
        name = f'tight_slot_{i}_shank_gap'
        check = manifest['clearance_checks'][name]
        if (check['status'] != 'passed' or check['lower_bound_mm'] < 0.49
                or check['query']['method'] != 'slot_trim_witness'):
            raise ValueError(f'uncertified slot gap {name}')
        gaps[name] = {'lower_bound_mm': check['lower_bound_mm'],
                      'native_method': check['query']['method']}
    return {'occurrences': 30, 'definitions': 14, 'endpoint': endpoint,
            'native_seats': seats, 'named_slot_pairs': gaps,
            'scope': 'Nominal seating and named slot/shank gaps only; no threads, preload, torque or rated loads.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=Path('target/release/cad'))
    parser.add_argument('--workspace', type=Path, default=Path('.'))
    parser.add_argument('--state-dir', type=Path, required=True)
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    cad, workspace = args.cad.resolve(), args.workspace.resolve()
    if not (workspace / SOURCE).is_file() and (workspace / 'examples' / SOURCE).is_file():
        workspace = workspace / 'examples'
    state, destination = args.state_dir.absolute(), args.out_dir.absolute()
    if not cad.is_file() or any(not (workspace / name).is_file()
                                for name in (SOURCE, MODULE, LOCK)):
        parser.error('pinned CAD executable, both Starlark sources and module lock are required')
    if destination.exists():
        parser.error('choose a new delivery directory')
    if state.exists() and (not state.is_dir() or any(state.iterdir())):
        parser.error('choose an empty separate state directory')
    if state == workspace or state.is_relative_to(workspace):
        parser.error('state directory must be outside the source workspace')
    if destination == state or destination.is_relative_to(state) or state.is_relative_to(destination):
        parser.error('delivery and durable state must be separate')

    script_path = Path(__file__).resolve()
    script_sha256_start = sha256(script_path)
    root_source = (workspace / SOURCE).read_text()
    source_hashes = {name: hashlib.sha256((workspace / name).read_bytes()).hexdigest()
                     for name in (SOURCE, MODULE, LOCK)}
    original_executable_sha256 = sha256(cad)
    right_source = changed(root_source, ENDPOINT_DECLARATION, ENDPOINT_DECLARATION.replace('default = -1', 'default = 1'))
    unseated_source = changed(root_source, NUT_DECLARATION, NUT_DECLARATION.replace('default = 0', 'default = 0.2'))
    destination.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.tightened-demo-', dir=destination.parent))
    pinned_cad = stage / '.pinned-cad'
    try:
        shutil.copyfile(cad, pinned_cad)
        pinned_cad.chmod(0o555)
        executable_sha256 = sha256(pinned_cad)
        if executable_sha256 != original_executable_sha256:
            raise ValueError('CAD executable changed while pinning the demonstration')
        first = Client(pinned_cad, workspace, state)
        try:
            base_snapshot = first.tool('cad_snapshot', {'recipe': SOURCE})
            if base_snapshot['source'] != root_source:
                raise ValueError('snapshot did not capture the supplied root source')
            right_snapshot = first.tool('cad_revise', {
                'snapshot_id': base_snapshot['snapshot_id'], 'source': right_source})
            unseated_snapshot = first.tool('cad_revise', {
                'snapshot_id': base_snapshot['snapshot_id'], 'source': unseated_source})
            before_args = {'snapshot_id': base_snapshot['snapshot_id'], 'mode': 'build',
                           'outputs': ['drive_module_tightened'],
                           'request_id': 'tightened-left-endpoint'}
            after_args = {'snapshot_id': right_snapshot['snapshot_id'], 'mode': 'build',
                          'outputs': ['drive_module_tightened'],
                          'request_id': 'tightened-right-endpoint'}
            rejected_args = {'snapshot_id': unseated_snapshot['snapshot_id'], 'mode': 'build',
                             'outputs': ['drive_module_tightened'],
                             'request_id': 'tightened-unseated-nut'}
            before = first.tool('cad_evaluate', before_args)
            after = first.tool('cad_evaluate', after_args)
            if not before['worker']['success'] or not after['worker']['success']:
                raise ValueError('accepted candidates did not build')
            if first.tool('cad_release', {}) != {'revision': None}:
                raise ValueError('durable state already had an accepted head')
            accept_before_args = {'request_id': 'accept-tightened-left',
                                  'job_id': before['job_id'],
                                  'manifest_digest': before['manifest_hash'],
                                  'expected_base_revision': None}
            accepted_before = first.tool('cad_accept', accept_before_args)
            if first.tool('cad_release', {}) != {'revision': accepted_before['revision']}:
                raise ValueError('base acceptance did not establish the expected head')
            rejected = first.tool('cad_evaluate', rejected_args, error=True)
            if (rejected['worker']['result']['code'] != 'nominal_seating_check_failed'
                    or rejected['worker']['result']['check_id'] not in {f'tight_nut_{i}_base_seat' for i in range(4)}
                    or rejected['worker']['result']['output_name'] != 'drive_module_tightened'):
                raise ValueError('unseated-nut was not refused by native seating')
            accepted_head_after_rejection = first.tool('cad_release', {})
            if accepted_head_after_rejection != {'revision': accepted_before['revision']}:
                raise ValueError('rejected unseated-nut changed the accepted head')
            stale_acceptance = first.tool('cad_accept', {
                'request_id': 'accept-tightened-stale', 'job_id': after['job_id'],
                'manifest_digest': after['manifest_hash'],
                'expected_base_revision': None}, error=True)
            if stale_acceptance.get('code') != 'agent_request_failed' or 'stale expected_base_revision' not in stale_acceptance.get('error', ''):
                raise ValueError('stale acceptance did not receive the expected refusal')
            accept_after_args = {'request_id': 'accept-tightened-right',
                                 'job_id': after['job_id'],
                                 'manifest_digest': after['manifest_hash'],
                                 'expected_base_revision': accepted_before['revision']['revision_id']}
            accepted_after = first.tool('cad_accept', accept_after_args)
            if first.tool('cad_release', {}) != {'revision': accepted_after['revision']}:
                raise ValueError('accepted head did not advance')
            first.process.kill()
            first.process.wait(timeout=5)
        finally:
            first.close()

        second = Client(pinned_cad, workspace, state)
        try:
            discovered = second.tool('cad_snapshots', {})
            expected_snapshots = {base_snapshot['snapshot_id'], right_snapshot['snapshot_id'],
                                  unseated_snapshot['snapshot_id']}
            if {s['snapshot_id'] for s in discovered['snapshots']} != expected_snapshots:
                raise ValueError('durable snapshots did not recover')
            jobs = second.tool('cad_jobs', {})['jobs']
            if len(jobs) != 3:
                raise ValueError('durable job ledger did not recover all three outcomes')
            if (second.tool('cad_evaluate', before_args) != before
                    or second.tool('cad_evaluate', after_args) != after
                    or second.tool('cad_evaluate', rejected_args, error=True) != rejected):
                raise ValueError('request retry changed a durable result')
            conflict = second.tool('cad_evaluate', {**after_args,
                'params': {'base_slot_length': 4}}, error=True)
            if conflict.get('code') != 'agent_request_failed' or 'request_id conflicts' not in conflict.get('error', ''):
                raise ValueError('conflicting retry did not receive the expected refusal')
            if len(second.tool('cad_jobs', {})['jobs']) != 3:
                raise ValueError('conflicting retry changed durable jobs')
            if (second.tool('cad_accept', accept_before_args) != accepted_before
                    or second.tool('cad_accept', accept_after_args) != accepted_after
                    or second.tool('cad_release', {}) != {'revision': accepted_after['revision']}):
                raise ValueError('accepted revision did not recover after restart')
            deliveries = []
            manifests = {}
            mechanical = {}
            assembly_instances = {}
            for name, job, endpoint in [('before', before, -1), ('after', after, 1)]:
                verified = second.tool('cad_verify', {
                    'job_id': job['job_id'], 'manifest_digest': job['manifest_hash']})
                if not verified['success']:
                    raise ValueError(f'{name} generation failed server verification')
                deliveries.append(second.download(job, stage / name, stream=True))
                run(pinned_cad, 'verify', stage / name, '--manifest-hash', job['manifest_hash'])
                manifest = json.loads((stage / name / 'manifest.json').read_text())
                manifests[name] = manifest
                mechanical[name] = fixture(manifest, endpoint)
                if (stage / name / 'recipe.star').read_text() != (root_source if name == 'before' else right_source):
                    raise ValueError('delivered recipe is not the expected endpoint revision')
                model = json.loads((stage / name / 'drive_module_tightened.assembly.json').read_text())
                moving = [node for node in model['hierarchy'][model['assembly_id']]['assemblies'] if node['id'] == 'moving']
                if len(moving) != 1 or moving[0]['transform']['translation'] != [endpoint * 3, 0, 0]:
                    raise ValueError('delivered upper assembly is not at the authored endpoint')
                assembly_instances[name] = {node['id']: node for node in model['instances']}

            if (manifests['before']['feature_hashes'] != manifests['after']['feature_hashes']
                    or manifests['before']['outputs']['drive_module_tightened']['content_hash']
                    == manifests['after']['outputs']['drive_module_tightened']['content_hash']):
                raise ValueError('endpoint revision must preserve native features and change the assembly pose')
            moved = 0
            for key, left in assembly_instances['before'].items():
                right = assembly_instances['after'][key]
                expected = [6, 0, 0] if json.loads(key)[0] == 'moving' else [0, 0, 0]
                delta = [b-a for a,b in zip(left['transform']['translation'], right['transform']['translation'])]
                if delta != expected or left['part'] != right['part'] or left['transform']['rotation'] != right['transform']['rotation']:
                    raise ValueError('endpoint revision did not preserve common upper placement')
                moved += expected[0] == 6
            if moved != 29 or len(assembly_instances['after']) != 30:
                raise ValueError('endpoint revision changed leaf membership')
            trace = first.trace + second.trace
        finally:
            second.close()

        for name, digest in source_hashes.items():
            if hashlib.sha256((workspace / name).read_bytes()).hexdigest() != digest:
                raise ValueError(f'source workspace changed: {name}')
        replay = replay_accepted(pinned_cad, stage / 'after', accepted_after,
                                 after, executable_sha256)
        semantic_fields = replay['semantic_manifest_fields']
        if ('nominal_seating_checks' not in semantic_fields
                or ('source_provenance' in manifests['after'] and 'source_provenance' not in semantic_fields)):
            raise ValueError('offline replay omitted seating or source engineering semantics')
        if sha256(pinned_cad) != executable_sha256:
            raise ValueError('pinned executable changed')
        script_sha256_end = sha256(script_path)
        if script_sha256_end != script_sha256_start:
            raise ValueError('demo script changed during execution')
        pinned_cad.unlink()
        evidence = {'valid': True,
                    'scope': 'Scripted local MCP acceptance of eight nominal seats and four named slot/shank gaps; no autonomous design, preload, all-contact collision, manufacturing release or hosted service claim',
                    'source_files_unchanged': True,
                    'source_files_sha256': source_hashes,
                    'demo_script_sha256_start': script_sha256_start,
                    'demo_script_sha256_end': script_sha256_end,
                    'pinned_executable_sha256': executable_sha256,
                    'durable_jobs': len(jobs), 'recovered_retry_results': 3,
                    'accepted_revisions': [accepted_before, accepted_after],
                    'accepted_head_after_rejection': accepted_head_after_rejection,
                    'stale_acceptance': stale_acceptance,
                    'rejected_unseated_nut': rejected,
                    'conflicting_retry': conflict,
                    'deliveries': deliveries,
                    'offline_replay': replay,
                    'mechanical_fixture': mechanical,
                    'snapshots': discovered,
                    'trace': trace}
        (stage / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
        title = 'Source-controlled seated mounting positions'
        (stage / 'index.html').write_text(f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{html.escape(title)}</title>
<style>body{{font:17px system-ui;max-width:1080px;margin:60px auto;padding:24px;color:#dceefa;background:#101a28;line-height:1.55}}h1{{font-size:clamp(2rem,5vw,3.2rem);line-height:1.1}}a{{color:#91d1ff}}.cards{{display:flex;gap:20px;flex-wrap:wrap}}section{{background:#1b2a3d;border:1px solid #334b62;border-radius:12px;padding:25px;flex:1;min-width:270px}}small{{color:#adc2d3}}li{{margin:9px 0}}</style>
<small>HEADLESS CAD · EXPERIMENTAL · LOCAL SCRIPTED ACCEPTANCE</small><h1>{html.escape(title)}</h1>
<p>The captured Starlark recipe places the drive at either end of its mounting slots. Each accepted endpoint has eight native face-seating witnesses and four native shank-to-slot gap checks. A revision that lowered the nuts by 0.2 mm failed seating and preserved the accepted state.</p>
<div class="cards"><section><h2>Left endpoint</h2><p>The upper drive is shifted −3 mm, with all four heads and nuts nominally seated.</p><a href="before/drive_module_tightened.preview.html">Inspect original position and source →</a></section>
<section><h2>Right endpoint</h2><p>The same native parts are shifted +3 mm in the accepted source revision.</p><a href="after/drive_module_tightened.preview.html">Inspect accepted position and source →</a></section></div>
<ul><li>30 occurrences from 14 shared native part definitions.</li><li>Eight protected seating disks exceed the 0.4 mm required radius; four shank-to-slot gaps exceed 0.49 mm.</li><li>The rejected unseated-nut edit and stale acceptance preserved the accepted head.</li><li>Three durable jobs, verified generation streams, recovery after restart and offline replay of {len(semantic_fields)} semantic fields are recorded.</li></ul>
<p><a href="evidence.json">Protocol, acceptance and replay evidence</a> · <a href="after/nominal-seating-checks.html">Native seating checks</a> · <a href="after/clearance-checks.html">Native clearance checks</a> · <a href="after/drive_module_tightened.step">Download STEP</a></p>
<small>Nominal unthreaded fastener envelopes only. Local face seating does not establish thread engagement, preload, friction, torque capacity, every assembly contact or rated loads.</small></html>''')
        publish_new(stage, destination)
        print(json.dumps({'valid': True, 'directory': str(destination),
                          'index': str(destination / 'index.html'),
                          'durable_jobs': 3, 'protocol_calls': len(trace)}, indent=2))
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
