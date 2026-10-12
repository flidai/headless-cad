"""Independent STEP sections and analytic frame checks for datum-associated views."""
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
from check_sections_ocp import close, shape_section
from check_step_metadata_ocp import command
from check_constrained_fixture_ocp import require

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT / 'examples/datum_section_fixture.star'


def rotate(vector, pose):
    x, y, z = vector
    # Independent closed-form matrices for the three authored rigid poses.
    if pose == 0:
        return [x, y, z]
    if pose == 1:
        return [x, -z, y]
    s = math.sqrt(0.5)
    return [0.5*x + 0.5*y + s*z, 0.5*x + 0.5*y - s*z, -s*x + s*y]


def vector_close(actual, expected, label):
    require(len(actual) == len(expected), label + ' dimension')
    for a, b in zip(actual, expected):
        close(a, b, label)


def imported(path, plane):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone and reader.TransferRoots() == 1,
            'STEP import failed')
    explorer = TopExp_Explorer(reader.OneShape(), TopAbs_SOLID)
    result = []
    while explorer.More():
        result.append(shape_section(explorer.Current(), plane))
        explorer.Next()
    require(len(result) == 6, 'six imported occurrences')
    return result


def case(cad, directory, height, depth, spacing, pose, shift):
    parameters = dict(height=height, depth=depth, spacing=spacing, pose=pose, shift=shift)
    flags = [item for k, v in parameters.items() for item in ['--param', f'{k}={v}']]
    generation = directory / 'generation'
    build = command([cad, 'build', RECIPE, '--out-dir', generation, *flags])
    require((generation / 'recipe.star').read_bytes() == RECIPE.read_bytes(), 'captured source')
    manifest = json.loads((generation / 'manifest.json').read_text())
    require(set(manifest['section_views']) == {'left_seat', 'right_seat'}, 'view inventory')
    expected_area = 2*(2400 - math.pi*(3+depth/2)**2 + math.pi*2.8**2)
    reports = {}
    for side, dx, cx in [('left', 0, spacing/2), ('right', spacing, -spacing/2)]:
        name = side + '_seat'
        native = json.loads((generation / f'section-{name}.json').read_text())
        entry = manifest['section_views'][name]
        view = native['section_view']
        witness = view['anchor_witness']
        require(witness == entry['anchor_witness'], 'manifest anchor binding')
        require(view['status'] == entry['status'] == 'passed', 'view status')
        require(witness['instance_path'] == ['carrier', side, 'housing'], 'exact occurrence path')
        require(witness['datum'] == 'top' and witness['part_id'] == 'housing', 'named part datum')
        semantic = witness['semantic_datum']
        require(semantic['selection']['status'] == 'resolved', 'semantic datum resolution')
        require(len(semantic['selection']['witnesses']) == 1, 'unique semantic face')
        require(semantic['definition']['anchor_feature'] == 'stock' and
                semantic['definition']['role'] == 'end_cap', 'authored semantic selector')
        require(semantic['target_content_hash'] == witness['part_content_hash'] ==
                build['outputs']['housing']['content_hash'], 'semantic target content binding')
        require(native['snapshot'] == entry['snapshot'], 'manifest query binding')
        require(native['snapshot']['parameter_overrides'] == parameters, 'parameter binding')
        require(native['snapshot']['recipe_source_hash'] == manifest['recipe_source_hash'], 'source binding')
        require(native['publication']['recipe_context_hash'] == manifest['recipe_context_hash'], 'generation binding')
        require(native['exchange_policy_status'] == manifest['exchange_policy_status'], 'exchange gate')
        plane = native['section']['plane']
        require(witness['resolved_plane'] == plane, 'resolved plane witness')
        vector_close(view['declaration']['origin'], [0, 0, -depth/2], 'local authored plane')
        expected_origin = [a+b for a,b in zip(rotate([30+dx, 20, height-depth/2], pose), [shift, -12, 7])]
        vector_close(plane['origin'], expected_origin, 'independent world origin')
        vector_close(plane['normal'], rotate([0, 0, 1], pose), 'independent normal')
        vector_close(plane['x_direction'], rotate([1, 0, 0], pose), 'independent drawing x')
        close(native['section']['area'], expected_area, 'analytic area')
        vector_close(native['section']['centroid'], [cx, 0], 'analytic drawing centroid')
        svg = ET.fromstring((generation / f'section-{name}.svg').read_bytes())
        require(json.loads(svg.find('{http://www.w3.org/2000/svg}metadata').text) == native, 'SVG witness binding')
        actual = imported(generation / 'datum_section_fixture.step', plane)
        close(sum(s['area'] for s in actual), expected_area, 'imported section area')
        vector_close([sum(s['area']*s['centroid'][i] for s in actual if s['centroid'] is not None)/expected_area
                      for i in range(2)], [cx, 0], 'imported drawing centroid')
        require(len(native['occurrences']) == 6, 'native occurrence inventory')
        require(sum(s['centroid'] is None for s in actual) == 2, 'feet outside plane')
        unmatched = list(actual)
        for occurrence in native['occurrences']:
            section = occurrence['section']
            match = next((i for i, other in enumerate(unmatched)
                          if abs(section['area']-other['area']) < 2e-6 and
                          ((section['centroid'] is None and other['centroid'] is None) or
                           (section['centroid'] is not None and other['centroid'] is not None and
                            all(abs(a-b) < 2e-7 for a,b in zip(section['centroid'], other['centroid']))))), None)
            require(match is not None, 'placed occurrence section lacks independent match')
            unmatched.pop(match)
        reports[name] = dict(native=native, imported=actual, expected_area=expected_area)
    require(command([cad, 'verify', generation])['valid'], 'generation integrity')
    result = dict(parameters=parameters, sections=reports, recipe_context_hash=manifest['recipe_context_hash'])
    (directory / 'report.json').write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    return result


def check(cad, directory):
    labels = ['identity', 'quarter_turn', 'oblique']
    values = [(12,2,80,0,0), (15,3,90,1,25), (18,4,100,2,50)]
    results = []
    for label, args in zip(labels, values):
        target = directory / label
        target.mkdir()
        results.append(case(cad, target, *args))
    require(len({r['recipe_context_hash'] for r in results}) == 3, 'distinct generation contexts')
    return dict(valid=True, cases=results,
                retained_generations=[f'{label}/generation' for label in labels],
                retained_reports=[f'{label}/report.json' for label in labels],
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
        with tempfile.TemporaryDirectory(prefix='cad-section-anchors-') as temporary:
            result = check(args.cad.resolve(), pathlib.Path(temporary))
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
