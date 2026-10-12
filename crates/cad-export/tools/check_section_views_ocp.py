"""Independent acceptance of recipe-owned sections in immutable release generations."""
import argparse
import importlib.metadata
import json
import math
import pathlib
import platform
import tempfile
import xml.etree.ElementTree as ET
import OCP
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_SOLID
from OCP.TopExp import TopExp_Explorer
from check_sections_ocp import close, imported_section, shape_section
from check_step_metadata_ocp import command
from check_constrained_fixture_ocp import require

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT / 'examples/countersunk_bearing_cover.star'


def assembly_sections(path, plane):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone and reader.TransferRoots() == 1,
            'assembly STEP import failed')
    solids = TopExp_Explorer(reader.OneShape(), TopAbs_SOLID)
    sections = []
    while solids.More():
        sections.append(shape_section(solids.Current(), plane))
        solids.Next()
    require(len(sections) == 20, 'imported occurrence count')
    return sections


def case(cad, directory, pitch, depth, height):
    parameters = dict(bolt_pitch=pitch, seat_depth=depth, flange_height=height)
    flags = [item for k, v in parameters.items() for item in ['--param', f'{k}={v}']]
    generation = directory / 'generation'
    build = command([cad, 'build', RECIPE, '--out-dir', generation, *flags])
    require((generation / 'recipe.star').read_bytes() == RECIPE.read_bytes(), 'captured recipe mismatch')
    manifest = json.loads((generation / 'manifest.json').read_text())
    require(set(manifest['section_views']) == {'pilot', 'socket', 'seat', 'assembly'}, 'view inventory')
    pi = math.pi
    pilot = pi * (3200 - 6 * 3.2**2)
    nominal = {
        'pilot': (height - 6, pilot),
        'socket': (height - (5 + depth) / 2, pilot - 27*pi),
        'seat': (height - depth/2, pi * (3200 - 6*(3.2 + depth/2)**2 - 27)),
        'assembly': (height - 1.5, pi * (3200 - 6*(1.7 + depth)**2 - 27
                      + 19.8**2 - 10**2 + 9.8**2 + 3*2.8**2 + 6*(1.5 + depth)**2)),
    }
    reports = {}
    for name, (z, area) in nominal.items():
        native = json.loads((generation / f'section-{name}.json').read_text())
        entry = manifest['section_views'][name]
        require(native['section_view']['status'] == entry['status'] == 'passed', 'view status')
        require(native['section_view']['declaration'] == entry['declaration'], 'view declaration binding')
        require(native['snapshot'] == entry['snapshot'], 'manifest query snapshot binding')
        require(native['snapshot']['recipe_source_hash'] == manifest['recipe_source_hash'], 'source hash')
        require(native['snapshot']['parameter_overrides'] == parameters, 'effective parameter snapshot')
        require(native['publication']['recipe_context_hash'] == manifest['recipe_context_hash'], 'generation context')
        require(native['publication']['generation_mode'] == 'full', 'publication mode')
        require(native['exchange_policy_status'] == manifest['exchange_policy_status'] == 'passed',
                'published view has stale exchange policy status')
        require(native['exchange_requirements'] == manifest['exchange_requirements'], 'exchange receipt binding')
        require(native['snapshot']['content_hash'] == build['outputs'][native['output_name']]['content_hash'], 'target content')
        plane = native['section']['plane']
        require(plane['normal'] == [0, 0, 1] and plane['x_direction'] == [1, 0, 0], 'source plane orientation')
        close(plane['origin'][2], z, 'source plane follows parameters')
        close(native['section']['area'], area, 'native source area')
        for value in native['section']['centroid']:
            close(value, 0, 'native symmetric section centroid')
        svg = ET.fromstring((generation / f'section-{name}.svg').read_bytes())
        require(json.loads(svg.find('{http://www.w3.org/2000/svg}metadata').text) == native,
                'SVG metadata differs from the published query')
        if name == 'assembly':
            actual = assembly_sections(generation / 'countersunk_bearing_cover.step', plane)
            close(sum(s['area'] for s in actual), area, 'OCCT occurrence area sum')
            require(len(native['occurrences']) == 20 and 'loops' not in native['section'], 'occurrence provenance')
            require(len(svg.findall('{http://www.w3.org/2000/svg}path')) == 20, 'independent occurrence SVG paths')
            unmatched = list(actual)
            for occurrence in native['occurrences']:
                s = occurrence['section']
                match = next((i for i, other in enumerate(unmatched)
                              if abs(s['area'] - other['area']) < 2e-6
                              and ((s['centroid'] is None and other['centroid'] is None)
                                   or (s['centroid'] is not None and other['centroid'] is not None
                                       and all(abs(a-b) < 2e-7 for a,b in zip(s['centroid'], other['centroid']))))), None)
                require(match is not None, 'native occurrence lacks an independent placed STEP section')
                unmatched.pop(match)
            require(not unmatched, 'unmatched imported occurrences')
        else:
            actual = imported_section(generation / 'flange.step', plane)
            close(actual['area'], area, 'OCCT source section area')
            for value in actual['centroid']:
                close(value, 0, 'OCCT source section centroid')
        reports[name] = dict(native=native, imported=actual, expected=dict(z=z, area=area))
    require(command([cad, 'verify', generation])['valid'], 'generation integrity')
    result = dict(parameters=parameters, sections=reports, recipe_context_hash=manifest['recipe_context_hash'])
    (directory / 'report.json').write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    return result


def check(cad, directory):
    results = []
    for label, p, h, d in [('low', 40, 2, 18), ('default', 42, 3, 21), ('high', 44, 4, 24)]:
        target = directory / label
        target.mkdir()
        results.append(case(cad, target, p, h, d))
    require(len({r['recipe_context_hash'] for r in results}) == 3, 'revisions share a context hash')
    return dict(valid=True, cases=results,
                retained_generations=[f'{name}/generation' for name in ['low', 'default', 'high']],
                retained_reports=[f'{name}/report.json' for name in ['low', 'default', 'high']],
                oracle=dict(python_version=platform.python_version(), ocp_version=str(OCP.__version__),
                            cadquery_ocp_version=importlib.metadata.version('cadquery-ocp')))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=pathlib.Path, required=True)
    parser.add_argument('--out-dir', type=pathlib.Path)
    args = parser.parse_args()
    if args.out_dir:
        args.out_dir.mkdir(parents=True, exist_ok=False)
        result = check(args.cad.resolve(), args.out_dir)
    else:
        with tempfile.TemporaryDirectory(prefix='cad-section-views-') as temporary:
            result = check(args.cad.resolve(), pathlib.Path(temporary))
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
