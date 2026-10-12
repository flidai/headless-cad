#!/usr/bin/env python3
"""Build a reviewable drive-module revision and preserve evidence of rejected edits.

This is a scripted source-edit demonstration, not an AI agent or manufacturing
certification. It uses the same CLI and source artifacts an agent would use.
"""
import argparse
import difflib
import hashlib
import html
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from evidence import publish_new

ROOT = Path(__file__).resolve().parents[1]


def inventory(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(directory.rglob('*')) if p.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=ROOT / 'target/release/cad')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--recipe', type=Path, default=ROOT / 'examples/drive_module_release.star')
    args = parser.parse_args()
    cad = args.cad.resolve()
    destination = args.out_dir.resolve()
    if destination.exists():
        parser.error('output already exists; use a new directory to retain earlier evidence')
    destination.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.drive-demo-', dir=destination.parent))
    commands = []

    def run(*arguments, failure=False):
        result = subprocess.run([str(cad), *map(str, arguments)], capture_output=True, text=True)
        commands.append({'arguments': list(map(str, arguments)), 'exit_code': result.returncode})
        if (result.returncode != 0) != failure:
            raise RuntimeError(f'unexpected command result: {arguments}\n{result.stderr}')
        return json.loads(result.stderr if failure else result.stdout)

    def save(name, value):
        (stage / name).write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')

    try:
        source = args.recipe.read_text()
        if source.count('design_revision = "A"') != 1:
            raise ValueError("expected unique design revision")
        revised = source.replace('design_revision = "A"', 'design_revision = "B"')
        for name, before, after in [('shaft_diameter', 12, 14), ('mounting_pitch', 60, 72)]:
            old = f'param(name = "{name}", default = {before})'
            new = f'param(name = "{name}", default = {after})'
            if revised.count(old) != 1:
                raise ValueError(f'expected unique source parameter {name}')
            revised = revised.replace(old, new)
        (stage / 'before.star').write_text(source)
        (stage / 'after.star').write_text(revised)
        (stage / 'revision.patch').write_text(''.join(difflib.unified_diff(
            source.splitlines(True), revised.splitlines(True), 'before.star', 'after.star')))
        before = run('build', stage / 'before.star', '--out-dir', stage / 'before', '--cache-dir', stage / 'cache')
        after = run('build', stage / 'after.star', '--out-dir', stage / 'after', '--cache-dir', stage / 'cache')
        save('semantic-diff.json', run('diff', stage / 'before.star', stage / 'after.star'))
        for name in ['before', 'after']:
            save(f'{name}-verification.json', run('verify', stage / name))
        baseline = inventory(stage / 'after')
        failure = run('build', stage / 'after.star', '--out-dir', stage / 'after',
                      '--cache-dir', stage / 'cache', '--param', 'required_clearance=0.3', failure=True)
        if failure.get('code') != 'clearance_check_failed':
            raise ValueError(f'expected native clearance rejection: {failure}')
        if baseline != inventory(stage / 'after'):
            raise ValueError('rejected revision changed published artifacts')
        save('rejected-edit.json', failure)
        # Rebuild captured source after removing the working copy: a release must
        # not depend on mutable authoring files or a warm geometry cache.
        (stage / 'after.star').unlink()
        replay = run('build', stage / 'after/recipe.star', '--out-dir', stage / 'replay')
        for key in ['recipe_source_hash', 'feature_hashes', 'clearance_checks']:
            if after[key] != replay[key]:
                raise ValueError(f'offline source replay differs: {key}')
        if (stage / 'replay/recipe.star').read_text() != revised:
            raise ValueError('captured replay source bytes differ')
        for name, output in after['outputs'].items():
            if output['content_hash'] != replay['outputs'][name]['content_hash']:
                raise ValueError(f'replay geometry differs: {name}')
        save('replay-verification.json', run('verify', stage / 'replay'))
        (stage / 'after.star').write_text(revised)
        shutil.rmtree(stage / 'cache')
        save('walkthrough.json', {
            'schema_version': 1, 'kind': 'scripted_source_revision',
            'experimental': True, 'source_recipe': args.recipe.name, 'binary_sha256': hashlib.sha256(cad.read_bytes()).hexdigest(),
            'before_recipe_context_hash': before['recipe_context_hash'],
            'after_recipe_context_hash': after['recipe_context_hash'],
            'changes': {'design_revision': ['A', 'B'], 'shaft_diameter_mm': [12, 14], 'mounting_pitch_mm': [60, 72]},
            'rejected_edit_preserved_generation': True, 'captured_source_replayed': True,
            'commands': commands,
            'scope': 'Supported geometry demonstration; nominal fits and hardware envelopes do not establish rated torque, axial retention or manufacturing certification.'})
        links = []
        for name, label in [('before', 'Original design'), ('after', 'Revised design')]:
            files = sorted((stage / name).glob('*'))
            artifact_links = ' · '.join(f'<a href="{name}/{html.escape(p.name, quote=True)}">{html.escape(p.name)}</a>'
                                      for p in files if p.suffix in ['.step', '.svg', '.csv'] or p.name in ['clearance-checks.html', 'manifest.json'])
            links.append(f'<section><h2>{label}</h2><p><a href="{name}/drive_module_release.preview.html">Inspect assembly and captured recipe</a></p><p>{artifact_links}</p></section>')
        (stage / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Drive module · Review a design revision</title><style>body{font:17px system-ui;background:#101a28;color:#e6edf5;max-width:1000px;margin:48px auto;padding:0 24px;line-height:1.6}a{color:#91d1ff}section{border-top:1px solid #40546a;margin-top:24px}p{overflow-wrap:anywhere}</style><h1>Review a drive-module revision</h1><p>Experimental, scripted source workflow. Shaft diameter changes from 12 to 14 mm; mounting pitch changes from 60 to 72 mm.</p><p><a href="revision.patch">Review source patch</a> · <a href="semantic-diff.json">Geometry and engineering diff</a> · <a href="walkthrough.json">Execution record</a></p>''' + ''.join(links) + '''<section><h2>Rejected edit and replay</h2><p>A 0.3 mm clearance requirement fails. Every byte of the last valid revised generation is preserved. The captured recipe also rebuilds without the working source or geometry cache.</p><p><a href="rejected-edit.json">Failure diagnostic</a> · <a href="replay-verification.json">Replay integrity</a></p></section><p>Component geometry and hardware envelopes remain experimental. Nominal fits do not certify rated torque, axial retention, manufacturing readiness, or general CAD support.</p></html>''')
        # Publish only after the complete walkthrough succeeds. Refuse to replace
        # another evidence run, including one created while this run was building.
        if destination.exists():
            raise ValueError('output appeared while building; refusing to replace it')
        publish_new(stage, destination)
        print(json.dumps({'valid': True, 'directory': str(destination), 'index': str(destination / 'index.html')}))
    finally:
        if stage.exists():
            shutil.rmtree(stage)


if __name__ == '__main__':
    main()
