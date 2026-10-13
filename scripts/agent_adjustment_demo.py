#!/usr/bin/env python3
"""Exercise a loaded drive slot study through durable local MCP acceptance.

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
SOURCE = 'drive_module_adjustment.star'
MODULE = 'drive_module_rotation.star'
LOCK = 'drive_module_adjustment.star.lock.json'
PATH_DECLARATION = 'translation = [base_slot_length,0,0])'


def changed(source, replacement):
    if source.count(PATH_DECLARATION) != 1:
        raise ValueError('expected one source-authored upper slot path')
    return source.replace(PATH_DECLARATION, replacement, 1)


def fixture(manifest, expected_travel):
    assembly = manifest['outputs']['drive_module_adjustment']
    if (assembly['instances'], assembly['definitions']) != (30, 14):
        raise ValueError('unexpected drive adjustment occurrence or definition count')
    group = manifest['rigid_translation_groups']['upper_slot_travel']
    if (group['translation_world'] != [expected_travel, 0, 0]
            or len(group['members']) != 29
            or len(group['clearance_check_ids']) != 4
            or group['coverage'] != 'named_pairs_only'):
        raise ValueError('source translation group changed unexpectedly')
    checks = manifest['clearance_checks']
    details = {}
    for i in range(4):
        name = f'mounting_slot_{i}_shank_gap'
        check = checks[name]
        proof = check['sweep']['slot_motion_proof']
        if (check['status'] != 'passed' or check['translation_group'] != 'upper_slot_travel'
                or check['motion_proof'] != 'slot_translation'
                or check['moving_endpoint'] != 'right'
                or check['lower_bound_mm'] < check['declaration']['min_clearance']
                or check['sweep']['query_count'] != 2
                or check['sweep']['intervals'] != []
                or proof['complete_path_lower_bound_mm'] < check['declaration']['min_clearance']):
            raise ValueError(f'uncertified mounting slot check {name}')
        details[name] = {'lower_bound_mm': check['lower_bound_mm'],
                         'native_method': check['query']['method'],
                         'proof': check['motion_proof'],
                         'query_count': check['sweep']['query_count']}
    return {'occurrences': 30, 'definitions': 14, 'moving_members': 29,
            'named_slot_pairs': 4, 'travel_mm': expected_travel,
            'checks': details,
            'scope': 'Nominal unthreaded clamp envelopes; named slot/shank pairs only. Upper assembly sliding contact, threads, preload, tightening and rated loads are not certified.'}


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
    shorter_source = changed(root_source, 'translation = [base_slot_length/2,0,0])')
    overshoot_source = changed(root_source, 'translation = [base_slot_length+1,0,0])')
    destination.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.adjustment-demo-', dir=destination.parent))
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
            short_snapshot = first.tool('cad_revise', {
                'snapshot_id': base_snapshot['snapshot_id'], 'source': shorter_source})
            overshoot_snapshot = first.tool('cad_revise', {
                'snapshot_id': base_snapshot['snapshot_id'], 'source': overshoot_source})
            before_args = {'snapshot_id': base_snapshot['snapshot_id'], 'mode': 'build',
                           'outputs': ['drive_module_adjustment'],
                           'request_id': 'adjustment-full-travel'}
            after_args = {'snapshot_id': short_snapshot['snapshot_id'], 'mode': 'build',
                          'outputs': ['drive_module_adjustment'],
                          'request_id': 'adjustment-half-travel'}
            rejected_args = {'snapshot_id': overshoot_snapshot['snapshot_id'], 'mode': 'build',
                             'outputs': ['drive_module_adjustment'],
                             'request_id': 'adjustment-overshoot'}
            before = first.tool('cad_evaluate', before_args)
            after = first.tool('cad_evaluate', after_args)
            if not before['worker']['success'] or not after['worker']['success']:
                raise ValueError('accepted candidates did not build')
            if first.tool('cad_release', {}) != {'revision': None}:
                raise ValueError('durable state already had an accepted head')
            accept_before_args = {'request_id': 'accept-adjustment-full',
                                  'job_id': before['job_id'],
                                  'manifest_digest': before['manifest_hash'],
                                  'expected_base_revision': None}
            accepted_before = first.tool('cad_accept', accept_before_args)
            if first.tool('cad_release', {}) != {'revision': accepted_before['revision']}:
                raise ValueError('base acceptance did not establish the expected head')
            rejected = first.tool('cad_evaluate', rejected_args, error=True)
            if rejected['worker']['result']['code'] != 'clearance_check_failed':
                raise ValueError('overshoot was not refused by native clearance')
            accepted_head_after_rejection = first.tool('cad_release', {})
            if accepted_head_after_rejection != {'revision': accepted_before['revision']}:
                raise ValueError('rejected overshoot changed the accepted head')
            stale_acceptance = first.tool('cad_accept', {
                'request_id': 'accept-adjustment-stale', 'job_id': after['job_id'],
                'manifest_digest': after['manifest_hash'],
                'expected_base_revision': None}, error=True)
            accept_after_args = {'request_id': 'accept-adjustment-half',
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
            expected_snapshots = {base_snapshot['snapshot_id'], short_snapshot['snapshot_id'],
                                  overshoot_snapshot['snapshot_id']}
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
            if len(second.tool('cad_jobs', {})['jobs']) != 3:
                raise ValueError('conflicting retry changed durable jobs')
            if (second.tool('cad_accept', accept_before_args) != accepted_before
                    or second.tool('cad_accept', accept_after_args) != accepted_after
                    or second.tool('cad_release', {}) != {'revision': accepted_after['revision']}):
                raise ValueError('accepted revision did not recover after restart')
            deliveries = []
            manifests = {}
            mechanical = {}
            for name, job, travel in [('before', before, 6), ('after', after, 3)]:
                verified = second.tool('cad_verify', {
                    'job_id': job['job_id'], 'manifest_digest': job['manifest_hash']})
                if not verified['success']:
                    raise ValueError(f'{name} generation failed server verification')
                deliveries.append(second.download(job, stage / name, stream=True))
                run(pinned_cad, 'verify', stage / name, '--manifest-hash', job['manifest_hash'])
                manifest = json.loads((stage / name / 'manifest.json').read_text())
                manifests[name] = manifest
                mechanical[name] = fixture(manifest, travel)
            if (manifests['before']['feature_hashes'] != manifests['after']['feature_hashes']
                    or manifests['before']['outputs']['drive_module_adjustment']['content_hash']
                    != manifests['after']['outputs']['drive_module_adjustment']['content_hash']):
                raise ValueError('path-only revision changed native source geometry')
            trace = first.trace + second.trace
        finally:
            second.close()

        for name, digest in source_hashes.items():
            if hashlib.sha256((workspace / name).read_bytes()).hexdigest() != digest:
                raise ValueError(f'source workspace changed: {name}')
        replay = replay_accepted(pinned_cad, stage / 'after', accepted_after,
                                 after, executable_sha256)
        if (len(replay['semantic_manifest_fields']) != 29
                or 'rigid_translation_groups' not in replay['semantic_manifest_fields']):
            raise ValueError('offline replay omitted translation engineering semantics')
        if sha256(pinned_cad) != executable_sha256:
            raise ValueError('pinned executable changed')
        script_sha256_end = sha256(script_path)
        if script_sha256_end != script_sha256_start:
            raise ValueError('demo script changed during execution')
        pinned_cad.unlink()
        evidence = {'valid': True,
                    'scope': 'Scripted local MCP acceptance of named nominal slot/shank clearances; no autonomous design, preload, all-contact collision, manufacturing release or hosted service claim',
                    'source_files_unchanged': True,
                    'source_files_sha256': source_hashes,
                    'demo_script_sha256_start': script_sha256_start,
                    'demo_script_sha256_end': script_sha256_end,
                    'pinned_executable_sha256': executable_sha256,
                    'durable_jobs': len(jobs), 'recovered_retry_results': 3,
                    'accepted_revisions': [accepted_before, accepted_after],
                    'accepted_head_after_rejection': accepted_head_after_rejection,
                    'stale_acceptance': stale_acceptance,
                    'rejected_overshoot': rejected,
                    'conflicting_retry': conflict,
                    'deliveries': deliveries,
                    'offline_replay': replay,
                    'mechanical_fixture': mechanical,
                    'snapshots': discovered,
                    'trace': trace}
        (stage / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
        title = 'Source-controlled slot adjustment'
        (stage / 'index.html').write_text(f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{html.escape(title)}</title>
<style>body{{font:17px system-ui;max-width:1080px;margin:60px auto;padding:24px;color:#dceefa;background:#101a28;line-height:1.55}}h1{{font-size:clamp(2rem,5vw,3.2rem);line-height:1.1}}a{{color:#91d1ff}}.cards{{display:flex;gap:20px;flex-wrap:wrap}}section{{background:#1b2a3d;border:1px solid #334b62;border-radius:12px;padding:25px;flex:1;min-width:270px}}small{{color:#adc2d3}}li{{margin:9px 0}}</style>
<small>HEADLESS CAD · EXPERIMENTAL · LOCAL SCRIPTED ACCEPTANCE</small><h1>{html.escape(title)}</h1>
<p>The captured Starlark recipe loads the released drive and prescribes one common translation of 29 upper occurrences. A half-travel source revision was accepted after the full-travel base. A one-millimetre overshoot failed a native whole-path slot check and left the accepted state intact.</p>
<div class="cards"><section><h2>Full travel · 6 mm</h2><p>Four mounting slot/shank pairs have typed native full-path proofs.</p><a href="before/translation-upper_slot_travel.html">Inspect original travel and source →</a></section>
<section><h2>Half travel · 3 mm</h2><p>The same native parts follow a shorter source-authored path.</p><a href="after/translation-upper_slot_travel.html">Inspect accepted revision and source →</a></section></div>
<ul><li>30 occurrences from 14 part definitions; 29 move together in the adjustment study.</li><li>Four named slot/shank pairs have a certified minimum above the 0.49 mm requirement over their entire path.</li><li>The rejected 7 mm overshoot and stale acceptance changed neither the accepted head nor source files.</li><li>Three durable jobs, verified generation streams, recovery after process restart and offline replay of 29 semantic manifest fields are recorded.</li></ul>
<p><a href="evidence.json">Protocol, acceptance and replay evidence</a> · <a href="after/rigid-translation-groups.html">Translation group evidence</a> · <a href="after/clearance-checks.html">Native clearance checks</a> · <a href="after/drive_module_adjustment.step">Download STEP</a></p>
<small>Nominal unthreaded fastener envelopes and named clearances only. Slider motion is visual inspection of a source-prescribed path. Sliding contact, threads, preload, tightening, every assembly contact and rated loads remain unverified.</small></html>''')
        publish_new(stage, destination)
        print(json.dumps({'valid': True, 'directory': str(destination),
                          'index': str(destination / 'index.html'),
                          'durable_jobs': 3, 'protocol_calls': len(trace)}, indent=2))
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
