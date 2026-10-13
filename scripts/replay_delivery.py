"""Rebuild a locally accepted delivery from its verified captured source, offline.

This checks deterministic geometry and engineering artifacts with the same pinned
executable. It does not authenticate an acceptance record outside the MCP store,
nor claim manufacturing release or byte-identical build manifests.
"""
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def command(cad, *args, cwd):
    result = subprocess.run([str(cad), *map(str, args)], cwd=cwd,
                            capture_output=True, text=True, timeout=120)
    if result.returncode:
        raise ValueError(f"offline CAD command failed: {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError('offline CAD command did not return JSON') from error


def safe_basename(name):
    require(isinstance(name, str) and re.fullmatch(r'[A-Za-z0-9_.-]{1,255}', name)
            and name not in ('.', '..') and not name.endswith('.'),
            'invalid captured source filename')
    return name


def replay_accepted(cad: Path, delivery: Path, accepted: dict, job: dict,
                    executable_sha256: str) -> dict:
    """Verify, rebuild, and compare an accepted candidate without MCP or workspace.

    `cad` must be the same pinned executable used by the MCP sessions; SHA-256
    checks that its bytes did not change across this replay. The acceptance
    record's BLAKE3 build identity is retained as provenance, not equated to SHA.
    """
    require(not Path(delivery).is_symlink(), 'delivery directory must not be a symlink')
    cad, delivery = Path(cad).resolve(), Path(delivery).resolve()
    require(re.fullmatch(r'[0-9a-f]{64}', executable_sha256) is not None,
            'missing pinned executable SHA-256')
    require(sha256(cad) == executable_sha256, 'pinned CAD executable changed')
    revision = accepted.get('revision')
    require(accepted.get('accepted') is True and isinstance(revision, dict),
            'missing accepted revision')
    require(revision.get('job_id') == job.get('job_id') and
            revision.get('snapshot_id') == job.get('snapshot_id') and
            revision.get('manifest_digest') == job.get('manifest_hash') and
            job.get('worker', {}).get('success') is True and job.get('mode') == 'build',
            'accepted revision does not bind this successful build job')
    digest = revision['manifest_digest']
    require(isinstance(digest, str) and re.fullmatch(r'[0-9a-f]{64}', digest),
            'invalid accepted manifest digest')
    require(delivery.is_dir() and not delivery.is_symlink(), 'invalid delivery directory')
    verified = command(cad, 'verify', delivery, '--manifest-hash', digest,
                       cwd=delivery.parent)
    require(verified.get('valid') is True, 'accepted delivery integrity check failed')
    manifest = json.loads((delivery / 'manifest.json').read_text())
    source = manifest.get('recipe_source')
    scope = manifest.get('scope')
    require(isinstance(source, dict) and isinstance(scope, dict),
            'accepted manifest lacks source or build scope')
    source_name = safe_basename(source.get('name'))
    source_artifact = safe_basename(source.get('artifact'))
    language = source.get('language')
    require((language, source_artifact) in (
        ('starlark', 'recipe.star'), ('json', 'recipe.source.json')),
        'unsupported accepted source language or artifact')
    params = source.get('parameter_overrides')
    require(isinstance(params, dict) and len(params) <= 128, 'invalid accepted parameters')
    for name, value in params.items():
        require(isinstance(name, str) and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name)
                and type(value) in (int, float) and math.isfinite(value),
                'invalid accepted parameter override')
    mode = scope.get('mode')
    outputs = scope.get('requested_outputs')
    require(mode in ('all', 'selected') and isinstance(outputs, list) and outputs and
            len(outputs) <= 64 and len(set(outputs)) == len(outputs) and
            all(isinstance(name, str) and re.fullmatch(r'[A-Za-z0-9_.-]{1,255}', name)
                for name in outputs), 'invalid accepted output scope')
    generation_mode = manifest.get('generation_mode')
    require(generation_mode == 'full',
            'offline accepted replay currently requires a full generation with native and STEP exports')
    files = manifest.get('artifact_files')
    require(isinstance(files, list) and all(isinstance(name, str) for name in files),
            'accepted artifact inventory missing')

    with tempfile.TemporaryDirectory(prefix='cad-accepted-replay-') as temporary:
        root = Path(temporary)
        captured = delivery / 'recipe.bundle.json'
        if captured.is_file():
            bundle = json.loads(captured.read_text())
            require(bundle.get('recipe_bundle_schema_version') == 1 and
                    bundle.get('source_name') == source_name and
                    bundle.get('language') == language and
                    bundle.get('source') == (delivery / source_artifact).read_text(),
                    'captured bundle does not match accepted root source')
            replay_source = root / 'recipe.bundle.json'
            shutil.copyfile(captured, replay_source)
            source_kind = 'captured_locked_bundle'
        else:
            replay_source = root / source_name
            shutil.copyfile(delivery / source_artifact, replay_source)
            source_kind = 'captured_root_source'
        argv = ['build', replay_source, '--cache-dir', root / 'cache',
                '--out-dir', root / 'generation']
        if mode == 'selected':
            for name in outputs:
                argv.extend(('--output', name))
        for name, value in sorted(params.items()):
            argv.extend(('--param', f'{name}={value!r}'))
        command(cad, *argv, cwd=root)
        replay = root / 'generation'
        replay_verify = command(cad, 'verify', replay, cwd=root)
        require(replay_verify.get('valid') is True, 'offline rebuild integrity check failed')
        rebuilt = json.loads((replay / 'manifest.json').read_text())
        require(rebuilt.get('recipe_source') == source and
                rebuilt.get('scope') == scope and
                rebuilt.get('generation_mode') == generation_mode and
                rebuilt.get('recipe_context_hash') == manifest.get('recipe_context_hash'),
                'offline rebuild used different source, parameters, scope or context')
        semantic_fields = (
            'schema_version', 'manifest_schema_version', 'units', 'tolerance',
            'kernel_version', 'kernel_build_id', 'executor_build_id', 'solver_build_id',
            'recipe_source_hash', 'face_datums', 'mechanism_context_hash',
            'feature_hashes', 'outputs', 'face_assertions', 'property_assertions',
            'clearance_checks', 'datum_dimensions', 'tolerance_stacks',
            'nominal_seating_checks',
            'rigid_rotation_groups', 'section_views', 'drawing_sheets',
            'exchange_requirements', 'exchange_policy_status', 'sketch_reports',
            'mechanism_reports', 'motion_studies', 'mechanism_bindings',
        )
        if 'rigid_translation_groups' in manifest or 'rigid_translation_groups' in rebuilt:
            semantic_fields += ('rigid_translation_groups',)
        for field in semantic_fields:
            require(field in manifest and rebuilt.get(field) == manifest[field],
                    f'offline engineering semantics differ: {field}')
        # Build statistics in manifest.json can depend on cache state. Browser
        # HTML embeds presentation scripts. Compare native and engineering bytes.
        def comparable(name):
            return name.endswith(('.json', '.step', '.stl', '.svg', '.star')) and name != 'manifest.json'
        expected = {name for name in files if comparable(name)}
        actual_files = rebuilt.get('artifact_files')
        require(isinstance(actual_files, list), 'offline rebuild inventory missing')
        actual = {name for name in actual_files if comparable(name)}
        require(expected == actual and any(name.endswith('.brep.json') for name in expected)
                and any(name.endswith('.step') for name in expected),
                'offline deterministic artifact set differs')
        compared = []
        for name in sorted(expected):
            safe_basename(name)
            original = delivery / name
            regenerated = replay / name
            original_hash, replay_hash = sha256(original), sha256(regenerated)
            require(original_hash == replay_hash,
                    f'offline deterministic artifact differs: {name}')
            compared.append({'name': name, 'bytes': original.stat().st_size,
                             'sha256': original_hash})
        require(sha256(cad) == executable_sha256, 'pinned CAD executable changed during replay')
        return {'valid': True, 'accepted_revision_id': revision['revision_id'],
                'accepted_job_id': revision['job_id'],
                'accepted_manifest_digest': digest,
                'accepted_build_executable_blake3': revision['build_executable_digest'],
                'replay_executable_sha256': executable_sha256,
                'source_kind': source_kind, 'source_name': source_name,
                'parameter_overrides': params, 'scope': scope,
                'generation_mode': generation_mode,
                'compared_files': compared,
                'excluded_files': sorted(set(files) - expected),
                'comparison': 'SHA-256 exact bytes for native geometry, exchange, source and engineering JSON/SVG; manifest and HTML excluded',
                'semantic_manifest_fields': list(semantic_fields),
                'delivery_verify': verified, 'replay_verify': replay_verify}
