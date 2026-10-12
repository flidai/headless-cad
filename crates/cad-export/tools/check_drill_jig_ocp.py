"""Independent constrained drill-jig STEP acceptance (optional OCP environment).
Run with --cad target/release/cad. Formula tolerances are fixed corpus budgets,
not estimates taken from the solver's condition number.
"""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_constrained_fixture_ocp import body_properties, close, json_output, require
ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT / 'examples/constrained_drill_jig.star'


def check(cad, output, pitch, radius, thickness):
    flags = ['--param', f'pitch={pitch}', '--param', f'bore_radius={radius}', '--param', f'thickness={thickness}']
    built = json_output([str(cad), 'build', str(RECIPE), '--out-dir', str(output), *flags])
    require((output / 'recipe.star').read_bytes() == RECIPE.read_bytes(), 'captured source differs')
    report = built['sketch_reports']['guide_layout']
    require(report['status'] == 'fully_constrained' and report['degrees_of_freedom'] == 0, 'unsolved layout')
    for name, x in [('left', 50-pitch), ('middle', 50), ('right', 50+pitch)]:
        center = report['points'][name]
        rim = report['points'][name+'_rim']
        close(center[0], x, 1e-5, 'hole x')
        close(center[1], 30, 1e-5, 'hole y')
        close(math.dist(center, rim), radius, 1e-5, 'hole radius')
    volume = (6000-3*math.pi*radius**2)*thickness
    ix = 100*60**3/12-3*math.pi*radius**4/4
    iy = 60*100**3/12-3*math.pi*radius**4/4-2*math.pi*radius**2*pitch**2
    tensor = [[thickness*ix+volume*thickness**2/12, 0, 0], [0, thickness*iy+volume*thickness**2/12, 0], [0, 0, thickness*(ix+iy)]]
    actual_volume, centroid, inertia = body_properties(output / 'plate.step')
    close(actual_volume, volume, .003, 'plate STEP volume')
    for a, b in zip(centroid, [50, 30, thickness/2]):
        close(a, b, 1e-5, 'plate STEP centroid')
    for i in range(3):
        for j in range(3):
            close(inertia[i][j], tensor[i][j], 2e-4*volume, 'plate STEP central inertia')
    total = volume+19200+3*math.pi*(2*radius-1)*(thickness+6)
    imported = json_output([sys.executable, str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')), str(output/'constrained_drill_jig.step'), '--instances', '6', '--definitions', '3', '--solids', '6', '--assembly-json', str(output/'constrained_drill_jig.assembly.json')])
    close(imported['material_volume_mm3'], total, .003, 'assembly STEP volume')
    for a, b in zip(imported['bounds_mm'], [0, 0, 0, 100, 60, 18+thickness]):
        close(a, b, 1e-5, 'assembly STEP bounds')
    # Verify authored placement independently, not only native/export agreement.
    assembly = json.loads((output/'constrained_drill_jig.assembly.json').read_text())
    placements = {p['id']: p['transform'] for p in assembly['instances']}
    for i, x in enumerate([50-pitch, 50, 50+pitch]):
        require(placements[f'guide_{i}']['translation'] == [x, 30, 12], 'wrong guide placement')
        require(placements[f'guide_{i}']['rotation'] == [0, 0, 0, 1], 'wrong guide rotation')
    return dict(pitch=pitch, radius=radius, thickness=thickness, volume_mm3=imported['material_volume_mm3'], placements_checked=imported['placements_checked'], hierarchy_checked=imported['hierarchy_checked'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=pathlib.Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='drill-jig-ocp-') as directory:
        results = [check(args.cad.resolve(), pathlib.Path(directory)/str(i), *params) for i, params in enumerate([(25,4,8), (20,3,6), (30,5,10)])]
    print(json.dumps(dict(valid=True, cases=results), indent=2))


if __name__ == '__main__':
    main()
