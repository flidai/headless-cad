#!/usr/bin/env python3
"""Collect bounded release evidence; verify integrity without executing bundled code.

Integrity is not independent proof that commands ran. Trust the collector and pin
its manifest hash through a trusted channel. Validation covers only the named
ledger domain, never general CAD or performance claims.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile

MAX_JSON = 32 * 1024 * 1024
MAX_FILES = 100000


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, f'duplicate JSON key: {key}')
        result[key] = value
    return result


def parse(data):
    require(len(data) <= MAX_JSON, 'JSON resource limit exceeded')
    return json.loads(data, object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def read_json(path):
    with path.open('rb') as stream:
        return parse(stream.read(MAX_JSON + 1))


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def safe(name):
    require(isinstance(name, str) and name and '\\' not in name and ':' not in name and '\x00' not in name,
            'unsafe path')
    p = PurePosixPath(name)
    require(not p.is_absolute() and all(x not in ('', '.', '..') for x in name.split('/')),
            f'unsafe path: {name}')
    return name


def record(path):
    require(path.is_file() and not path.is_symlink(), f'not a regular file: {path}')
    digest = hashlib.sha256()
    size = 0
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
            size += len(chunk)
    return {'sha256': digest.hexdigest(), 'size': size}


def is_link(path):
    return path.is_symlink() or getattr(path, 'is_junction', lambda: False)()


def inventory(root):
    require(root.is_dir() and not is_link(root), 'bundle must be a real directory')
    result = {}
    directory_count = 0
    for directory, dirs, files in os.walk(root, followlinks=False):
        directory_count += 1
        require(directory_count <= MAX_FILES, 'directory count limit exceeded')
        require(dirs or files or Path(directory) == root, 'empty directory forbidden')
        for name in dirs:
            require(not is_link(Path(directory)/name), 'symlink directory forbidden')
        for name in files:
            path = Path(directory)/name
            relative = safe(path.relative_to(root).as_posix())
            result[relative] = record(path)
            require(len(result) <= MAX_FILES, 'file count limit exceeded')
    return result


def output(argv, cwd):
    return subprocess.check_output(argv, cwd=cwd, text=True).strip()


def source_state(root):
    require(not output(['git', 'status', '--porcelain', '--untracked-files=all'], root),
            'collection requires a clean Git checkout')
    revision = output(['git', 'rev-parse', 'HEAD'], root)
    tree = output(['git', 'rev-parse', 'HEAD^{tree}'], root)
    names = subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode().split('\0')
    files = {safe(name): record(root/name) for name in names if name}
    require(len(files) <= MAX_FILES, 'source file count limit exceeded')
    return {'revision': revision, 'tree': tree, 'source_files': files}


def ledger_selection(ledger, capability_id):
    require(ledger.get('schema_version') == 1, 'unsupported ledger schema')
    capabilities = ledger['capabilities']
    checks = ledger['checks']
    require(len({c['id'] for c in capabilities}) == len(capabilities), 'duplicate capability')
    require(len({c['id'] for c in checks}) == len(checks), 'duplicate check')
    selected = [c for c in capabilities if c['id'] == capability_id]
    require(len(selected) == 1, 'unknown capability')
    cap = selected[0]
    require(cap['status'] in ('planned', 'experimental', 'validated'), 'invalid capability status')
    require(cap['status'] != 'planned', 'planned capabilities cannot be promoted')
    require(cap.get('kind') == 'engineering',
            'only engineering evidence is supported; performance promotion needs a hardware/workload protocol')
    require(cap['supported_domain'] and cap['domain_ids'] and cap['fixture'], 'missing capability scope')
    require(len(set(cap['checks'])) == len(cap['checks']), 'duplicate required check')
    selected_checks = []
    for identifier in cap['checks']:
        require(re.fullmatch(r'[A-Za-z0-9_.-]+', identifier) is not None, 'unsafe check id')
        found = [c for c in checks if c['id'] == identifier]
        require(len(found) == 1, 'missing required check')
        check = found[0]
        require(check['kind'] in ('native', 'independent'), 'unsupported evidence kind')
        require(isinstance(check['argv'], list) and check['argv'] and
                all(isinstance(x, str) and x for x in check['argv']), 'invalid check command')
        selected_checks.append(check)
    require({c['kind'] for c in selected_checks} == {'native', 'independent'},
            'native and independent evidence required')
    return cap, selected_checks


def run(argv, cwd, root, stem):
    stdout, stderr = f'logs/{stem}.stdout', f'logs/{stem}.stderr'
    (root/'logs').mkdir(exist_ok=True)
    with (root/stdout).open('wb') as out, (root/stderr).open('wb') as err:
        status = subprocess.run(argv, cwd=cwd, stdout=out, stderr=err, check=False).returncode
    require(status == 0, f'{stem} failed ({status}); see {root/stderr}')
    return {'argv': argv, 'exit_code': status, 'stdout': stdout, 'stderr': stderr}


def expanded(argv, substitutions):
    result = []
    for arg in argv:
        for key, value in substitutions.items():
            arg = arg.replace('{' + key + '}', value)
        require(not re.search(r'\{(?:python|cad|artifacts)\}', arg), 'unexpanded command')
        result.append(arg)
    return result


def verify(directory, manifest_hash=None, source_root=None):
    root = Path(directory).absolute()
    actual = inventory(root)
    manifest_record = actual.pop('evidence.json', None)
    require(manifest_record is not None, 'missing evidence.json')
    if manifest_hash is not None:
        require(manifest_record['sha256'] == manifest_hash, 'manifest hash mismatch')
    data = read_json(root/'evidence.json')
    require(data.get('schema_version') == 1, 'unsupported evidence schema')
    require(actual == data['files'], 'bundle membership, size or hash mismatch')
    def bound(value):
        name = safe(value['path'])
        require(actual.get(name) == {k: value[k] for k in ('sha256', 'size')}, 'unbound file record')
        return root/name
    ledger_path = bound(data['ledger'])
    cap_path = bound(data['capabilities'])
    require(data['binary']['path'] == 'tools/cad', 'invalid binary path')
    bound(data['binary'])
    ledger = read_json(ledger_path)
    cap, specs = ledger_selection(ledger, data['capability']['id'])
    require(data['capability'] == dict(cap, status='validated'), 'capability envelope mismatch')
    known = {domain['id'] for domain in read_json(cap_path)['domains']}
    require(set(cap['domain_ids']) <= known, 'unknown capability domain')
    require(data['ledger']['path'] == 'ledger.json' and data['capabilities']['path'] == 'capabilities.json',
            'invalid ledger or capabilities path')
    substitutions = data['substitutions']
    require(set(substitutions) == {'python', 'cad', 'artifacts'} and
            all(isinstance(value, str) and Path(value).is_absolute() for value in substitutions.values()),
            'invalid execution paths')
    require(Path(substitutions['cad']).name == 'cad' and
            Path(substitutions['cad']).parent.name == 'tools' and
            Path(substitutions['cad']).parent.parent / 'artifacts' == Path(substitutions['artifacts']),
            'execution paths do not bind the captured binary and artifacts')
    source = data['source']
    require(re.fullmatch(r'[0-9a-f]{40,64}', source['revision']) is not None, 'invalid source revision')
    require(re.fullmatch(r'[0-9a-f]{40,64}', source['tree']) is not None, 'invalid source tree')
    for name, value in source['source_files'].items():
        require(actual.get('source/' + safe(name)) == value, 'source snapshot mismatch')
    require({p[7:] for p in actual if p.startswith('source/')} == set(source['source_files']),
            'source snapshot membership mismatch')
    require(actual.get('source/' + safe(data['ledger_source'])) == actual[data['ledger']['path']],
            'ledger is not the captured source ledger')
    require(safe(cap['fixture']) in source['source_files'], 'fixture not in source snapshot')
    require(len(data['checks']) == len(specs), 'check coverage mismatch')
    for check, spec in zip(data['checks'], specs):
        require(check['id'] == spec['id'] and check['kind'] == spec['kind'], 'check identity mismatch')
        require(check['argv'] == expanded(spec['argv'], data['substitutions']), 'command binding mismatch')
        require(type(check['exit_code']) is int and check['exit_code'] == 0, 'failed evidence check')
        for channel in ('stdout', 'stderr'):
            require(safe(check[channel]) == f"logs/{spec['id']}.{channel}" and check[channel] in actual,
                    'missing or rebound check log')
        if spec['kind'] == 'independent':
            result = read_json(root/check['stdout'])
            require(result.get('valid') is True and check['result'] == result, 'invalid independent result')
    manifests = sorted(p for p in actual if p.startswith('artifacts/') and p.endswith('/manifest.json'))
    require(manifests and [v['manifest'] for v in data['generation_verifications']] == manifests,
            'generation verification coverage mismatch')
    for index, entry in enumerate(data['generation_verifications']):
        require(entry['result'] == f'logs/generation-{index}.stdout' and entry['result'] in actual,
                'generation verification log mismatch')
        result = read_json(root/safe(entry['result']))
        require(entry['manifest_record'] == actual[entry['manifest']], 'generation manifest binding mismatch')
        require(result.get('valid') is True and result.get('schema_version') == 1 and
                result.get('algorithm') == 'blake3' and
                re.fullmatch(r'[0-9a-f]{64}', result.get('manifest_hash', '')) is not None,
                'invalid artifact verification result')
        manifest = read_json(root/entry['manifest'])
        require(manifest.get('manifest_schema_version') == 1, 'unsupported generation manifest')
        prefix = str(PurePosixPath(entry['manifest']).parent) + '/'
        names = manifest['artifact_files']
        require(isinstance(names, list) and len(names) == len(set(names)) and 'manifest.json' in names,
                'invalid generation file inventory')
        require({prefix + safe(name) for name in names} == {p for p in actual if p.startswith(prefix)},
                'generation inventory mismatch')
        payload = [name for name in names if name != 'manifest.json']
        require(type(result.get('verified_files')) is int and result['verified_files'] == len(payload) and
                type(result.get('verified_bytes')) is int and
                result['verified_bytes'] == sum(actual[prefix + name]['size'] for name in payload),
                'generation verification counts mismatch')
    if source_root is not None:
        require(source_state(Path(source_root).resolve()) == source, 'source checkout mismatch')
    return {'valid': True, 'capability': cap['id'], 'manifest_sha256': manifest_record['sha256'],
            'source_revision': source['revision'], 'files': len(actual),
            'trust': 'Integrity only; trust collector execution or pin a trusted manifest hash.'}


def publish_new(source, destination):
    """Linux renameat2 provides atomic publication without replacing any path."""
    require(sys.platform.startswith('linux'), 'atomic no-replace publication currently requires Linux')
    libc = ctypes.CDLL(None, use_errno=True)
    rename = getattr(libc, 'renameat2', None)
    require(rename is not None, 'atomic no-replace rename is unavailable')
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(source), -100, os.fsencode(destination), 1) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(destination))


def collect(args):
    source_root = Path(__file__).resolve().parents[1]
    source = source_state(source_root)
    ledger_path = Path(args.ledger).resolve()
    ledger_source = safe(ledger_path.relative_to(source_root).as_posix())
    require(ledger_source in source['source_files'], 'ledger must be tracked')
    ledger = read_json(ledger_path)
    cap, checks = ledger_selection(ledger, args.capability)
    destination = Path(args.out_dir).absolute()
    require(not destination.exists() and not destination.is_symlink(), 'destination already exists')
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix='.evidence-', dir=destination.parent))
    try:
        for name in source['source_files']:
            path = staging/'source'/name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_root/name, path)
        shutil.copyfile(ledger_path, staging/'ledger.json')
        run(['cargo', 'build', '--release', '--locked', '-p', 'cad-cli', '--target-dir', str(source_root/'target')], source_root, staging, 'build')
        (staging/'tools').mkdir()
        shutil.copy2(source_root/'target/release/cad', staging/'tools/cad')
        cad = str(staging/'tools/cad')
        capability_run = run([cad, 'capabilities'], source_root, staging, 'capabilities')
        shutil.copyfile(staging/capability_run['stdout'], staging/'capabilities.json')
        tools = {tool: output([tool, '--version'], source_root) for tool in ('cargo', 'rustc')}
        python = shutil.which(args.python) or args.python
        python = str(Path(python).absolute())
        tools['python'] = output([python, '--version'], source_root)
        (staging/'artifacts').mkdir()
        substitutions = {'python': python, 'cad': cad, 'artifacts': str(staging/'artifacts')}
        results = []
        for spec in checks:
            result = run(expanded(spec['argv'], substitutions), source_root, staging, spec['id'])
            result.update(id=spec['id'], kind=spec['kind'])
            if spec['kind'] == 'independent':
                result['result'] = read_json(staging/result['stdout'])
                require(result['result'].get('valid') is True, 'independent oracle did not validate')
            results.append(result)
        generations = []
        for index, path in enumerate(sorted((staging/'artifacts').rglob('manifest.json'))):
            result = run([cad, 'verify', str(path.parent)], source_root, staging, f'generation-{index}')
            require(read_json(staging/result['stdout']).get('valid') is True, 'invalid generation')
            generations.append({'manifest': path.relative_to(staging).as_posix(),
                                'manifest_record': record(path), 'result': result['stdout']})
        require(source_state(source_root) == source, 'source changed during collection')
        data = {'schema_version': 1, 'source': source, 'ledger_source': ledger_source,
                'capability': dict(cap, status='validated'), 'checks': results, 'tools': tools,
                'substitutions': substitutions, 'generation_verifications': generations}
        for key, name in [('binary', 'tools/cad'), ('ledger', 'ledger.json'), ('capabilities', 'capabilities.json')]:
            data[key] = dict(record(staging/name), path=name)
        data['files'] = inventory(staging)
        write_json(staging/'evidence.json', data)
        result = verify(staging)
        require(not destination.exists() and not destination.is_symlink(), 'destination already exists')
        publish_new(staging, destination)
        result['directory'] = str(destination)
        return result
    except BaseException:
        # Keep failed command logs for diagnosis without publishing a success bundle.
        print(f'Unpublished evidence retained at {staging}', file=sys.stderr)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    collect_parser = commands.add_parser('collect')
    collect_parser.add_argument('--ledger', default='evidence/ledger.json')
    collect_parser.add_argument('--capability', required=True)
    collect_parser.add_argument('--python', default=sys.executable)
    collect_parser.add_argument('--out-dir', required=True)
    verify_parser = commands.add_parser('verify')
    verify_parser.add_argument('directory')
    verify_parser.add_argument('--manifest-hash')
    verify_parser.add_argument('--source-root')
    args = parser.parse_args()
    try:
        result = collect(args) if args.command == 'collect' else verify(args.directory, args.manifest_hash, args.source_root)
        print(json.dumps(result, indent=2, sort_keys=True))
    except (ValueError, OSError, KeyError, TypeError, AttributeError, IndexError, RecursionError, subprocess.SubprocessError) as error:
        print(json.dumps({'valid': False, 'error': str(error)}), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
