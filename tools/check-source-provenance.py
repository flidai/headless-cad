#!/usr/bin/env python3
"""Independent acceptance of captured Starlark declaration provenance.

The checks deliberately read the captured bundle, rather than the live source
tree, so an offset cannot appear valid merely because the source moved later.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(cad, *args):
    proc = subprocess.run([str(cad), *map(str, args)], capture_output=True,
                          text=True, timeout=240)
    require(proc.returncode == 0,
            f"cad {' '.join(map(str, args[:2]))}: {proc.stderr[:2000]}")
    return json.loads(proc.stdout)


def read_json(path):
    return json.loads(Path(path).read_text())


def source_bytes(bundle):
    result = {bundle['source_name']: bundle['source'].encode('utf-8')}
    for label, text in bundle['modules'].items():
        require(label not in result, 'distinct root and module labels')
        result[label] = text.encode('utf-8')
    return result


def verify_span(span, sources, expected_call=None):
    required = ('file', 'start_byte', 'end_byte', 'start_line', 'start_column',
                'end_line', 'end_column')
    require(all(field in span for field in required), 'complete source span')
    label = span['file']
    require(label in sources, f'captured module label {label}')
    data = sources[label]
    first, last = span['start_byte'], span['end_byte']
    require(type(first) is int and type(last) is int and 0 <= first < last <= len(data),
            f'bounded half-open UTF-8 byte span in {label}')
    fragment = data[first:last].decode('utf-8')
    require(fragment and not fragment.isspace(), 'nonempty source declaration')
    before = data[:first]
    through = data[:last]
    require(span['start_line'] == before.count(b'\n') + 1,
            f'start line derived from captured bytes: {label}')
    require(span['end_line'] == through.count(b'\n') + 1,
            f'end line derived from captured bytes: {label}')
    start_column = len(before.rsplit(b'\n', 1)[-1].decode('utf-8')) + 1
    end_column = len(through.rsplit(b'\n', 1)[-1].decode('utf-8')) + 1
    require(span['start_column'] == start_column and span['end_column'] == end_column,
            f'one-based Unicode source columns derived from captured bytes: {label}')
    if expected_call:
        require(expected_call in fragment, f'{expected_call} in exact source slice')
    return fragment


def validate_provenance(out, expected_features, expected_parts, helper_call=None):
    bundle = read_json(out / 'recipe.bundle.json')
    prov = read_json(out / 'source-provenance.json')
    manifest = read_json(out / 'manifest.json')
    sources = source_bytes(bundle)
    require(prov['schema_version'] == 1 and prov['kind'] == 'source_provenance',
            'versioned provenance document')
    for name in ('recipe_context_hash', 'recipe_source_hash', 'source_bundle_hash'):
        require(prov[name] == manifest[name], f'manifest {name} binding')
    inventory = prov['source_inventory']
    require(set(inventory) == set(sources), 'exact captured root and module inventory')
    for label, data in sources.items():
        require(inventory[label]['bytes'] == len(data), f'{label} byte count')
        require(len(inventory[label]['blake3']) == 64, f'{label} BLAKE3 identifier')
        require(inventory[label]['sha256'] == hashlib.sha256(data).hexdigest(),
                f'{label} independently hashed captured UTF-8 bytes')
    for kind, expected in (('features', expected_features), ('parts', expected_parts)):
        for identifier, call in expected.items():
            require(identifier in prov[kind], f'{kind} runtime ID {identifier}')
            origin = prov[kind][identifier]
            declaration = origin['declaration']
            verify_span(declaration, sources, call)
            for parent in origin['call_stack']:
                verify_span(parent, sources)
            if helper_call and identifier in helper_call:
                require(declaration['file'] == helper_call[identifier]['file'],
                        f'{identifier} helper declaration module')
                require(any(helper_call[identifier]['call'] in verify_span(s, sources)
                            for s in origin['call_stack']),
                        f'{identifier} actual caller breadcrumb')
    for part, feature in prov['part_body_features'].items():
        require(part in prov['parts'] and feature in prov['features'],
                f'{part} points to a declared native body feature')
    assemblies = list(out.glob('*.assembly.json'))
    require(assemblies, 'published assembly definitions bind source to native bodies')
    for assembly_file in assemblies:
        assembly = read_json(assembly_file)
        for part, definition in assembly['definitions'].items():
            require(prov['part_body_features'].get(part) == definition['feature_id'],
                    f'{part} declaration maps to published native body feature')
    require(manifest['source_provenance']['artifact'] == 'source-provenance.json',
            'manifest resolves current source sidecar')
    return {'root': bundle['source_name'], 'modules': sorted(bundle['modules']),
            'captured_source_sha256': {label: hashlib.sha256(data).hexdigest()
                                       for label, data in sources.items()},
            'features': len(prov['features']), 'parts': len(prov['parts']),
            'recipe_context_hash': prov['recipe_context_hash'],
            'source_bundle_hash': prov['source_bundle_hash'],
            'sidecar_sha256': sha256(out / 'source-provenance.json')}


def fixture(work):
    lib = work / 'lib'
    lib.mkdir(parents=True)
    # The multibyte characters precede declarations; source offsets must be
    # byte offsets even when Starlark's human line/column view differs.
    module = '''# π and 🚀 prove UTF-8 offsets are not character indexes.\n\
def make(i):\n\
    profile = rectangle(\n\
        id = "profile_" + str(i), width = 10 + i, depth = 4)\n\
    body = extrude(\n\
        id = "body_" + str(i), profile = profile, height = 2)\n\
    item = part(\n\
        id = "item_" + str(i), body = body)\n\
    return item\n'''
    root = '''# Source provenance acceptance root.\n\
load("lib/parts.star", "make")\n\
items = [instance(id = "occ_"+str(i), part = make(i),\n\
    offset = [12*i, 0, 0]) for i in range(2)]\n\
unit = assembly(id = "unit", instances = items)\n\
assembly_output(name = "unit", assembly = unit)\n'''
    (lib / 'parts.star').write_text(module)
    path = work / 'root.star'
    path.write_text(root)
    return path


def output_hashes(manifest):
    return {name: info['content_hash'] for name, info in manifest['outputs'].items()}


def check(cad, work):
    path = fixture(work)
    run(cad, 'lock', path)
    first = work / 'first'
    manifest_a = run(cad, 'build', path, '--out-dir', first)
    expected_features = {**{f'profile_{i}': 'rectangle(' for i in range(2)},
                         **{f'body_{i}': 'extrude(' for i in range(2)}}
    expected_parts = {f'item_{i}': 'part(' for i in range(2)}
    helper = {f'item_{i}': {'file': 'lib/parts.star', 'call': 'make(i)'}
              for i in range(2)}
    detail_a = validate_provenance(first, expected_features, expected_parts, helper)
    require((first / 'unit.step').exists(), 'native assembled exchange file')
    bundle_a = first / 'recipe.bundle.json'
    replay = work / 'replay'
    manifest_replay = run(cad, 'build', bundle_a, '--out-dir', replay)
    detail_replay = validate_provenance(replay, expected_features, expected_parts, helper)
    require((first / 'source-provenance.json').read_bytes() ==
            (replay / 'source-provenance.json').read_bytes(),
            'offline replay preserves exact declaration navigation')
    require(output_hashes(manifest_a) == output_hashes(manifest_replay),
            'offline replay preserves native output identities')

    # A comment changes source navigation and source context; cached geometry
    # stays the same. Re-pin explicitly, as a real author must do.
    module = work / 'lib/parts.star'
    module.write_text(module.read_text().replace(
        '# π and 🚀 prove UTF-8 offsets are not character indexes.',
        '# π and 🚀 prove UTF-8 offsets are not character indexes. Edited comment.'))
    run(cad, 'lock', path)
    revised = work / 'revised'
    manifest_b = run(cad, 'build', path, '--out-dir', revised)
    detail_b = validate_provenance(revised, expected_features, expected_parts, helper)
    require(manifest_b['recipe_context_hash'] != manifest_a['recipe_context_hash'],
            'comment edit changes captured recipe context')
    require(output_hashes(manifest_a) == output_hashes(manifest_b),
            'comment edit preserves native output identities')
    require((first / 'unit.step').read_bytes() ==
            (revised / 'unit.step').read_bytes(),
            'comment edit preserves native assembled STEP bytes')
    require(detail_a['sidecar_sha256'] != detail_b['sidecar_sha256'],
            'comment edit updates navigation sidecar')

    drive = ROOT / 'examples/drive_module_adjustment.star'
    if drive.exists():
        drive_out = work / 'drive'
        drive_manifest = run(cad, 'build', drive, '--out-dir', drive_out)
        drive_prov = read_json(drive_out / 'source-provenance.json')
        drive_bundle = read_json(drive_out / 'recipe.bundle.json')
        sources = source_bytes(drive_bundle)
        require('drive_module_rotation.star' in sources, 'captured imported drive source')
        for part_id in ('shaft', 'end_cover', 'cover_screw_envelope'):
            declaration = drive_prov['parts'][part_id]['declaration']
            require(declaration['file'] == 'drive_module_rotation.star',
                    f'{part_id} declared in loaded drive module')
            verify_span(declaration, sources, 'part(')
        require(drive_prov['parts']['shaft']['declaration']['file'] !=
                drive_bundle['source_name'], 'imported declaration is not root proxy')
        require(drive_prov['recipe_context_hash'] == drive_manifest['recipe_context_hash'],
                'drive provenance current source context')
        drive_info = {'source_sha256': sha256(drive),
                      'loaded_source_sha256': sha256(ROOT / 'examples/drive_module_rotation.star'),
                      'source_bundle_hash': drive_manifest['source_bundle_hash'],
                      'sidecar_sha256': sha256(drive_out / 'source-provenance.json')}
    else:
        drive_info = None
    return {'fixture': {'initial': detail_a, 'replay': detail_replay,
                        'comment_revision': detail_b}, 'drive': drive_info}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cad', type=Path, required=True)
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    cad = args.cad.resolve()
    args.out_dir.mkdir(parents=True, exist_ok=True)
    require(not any(args.out_dir.iterdir()), 'fresh empty oracle output directory')
    binary_hash, script_hash = sha256(cad), sha256(__file__)
    result = check(cad, args.out_dir)
    require(sha256(cad) == binary_hash and sha256(__file__) == script_hash,
            'pinned executable and oracle source remained unchanged')
    report = {'valid': True, 'executable_sha256': binary_hash,
              'oracle_script_sha256': script_hash, **result}
    (args.out_dir / 'report.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
