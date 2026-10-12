"""Independent assembly/section checks and captured-source replay for locked modules."""
import argparse
import importlib.metadata
import json
import math
import pathlib
import platform
import sys
import OCP
from check_constrained_fixture_ocp import body_properties, require, json_output
from check_sections_ocp import shape_section, close
from OCP.STEPControl import STEPControl_Reader
from OCP.IFSelect import IFSelect_RetDone
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_SOLID
from check_step_metadata_ocp import command

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT / 'examples/modular_linear_stage.star'

def assembly_section(path, plane):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone and reader.TransferRoots() == 1, 'STEP import')
    explorer = TopExp_Explorer(reader.OneShape(), TopAbs_SOLID)
    sections = []
    while explorer.More():
        sections.append(shape_section(explorer.Current(), plane))
        explorer.Next()
    require(len(sections) == 4, 'four STEP solid occurrences')
    area = sum(section['area'] for section in sections)
    require(area > 0, 'nonempty assembly section')
    center = [sum(section['area']*section['centroid'][i] for section in sections if section['centroid'] is not None)/area for i in range(2)]
    return dict(area=area, centroid=center, occurrences=sections)

def case(cad, directory, length, setback, bore):
    flags = [item for k, v in dict(length=length, setback=setback, bore=bore).items() for item in ['--param', f'{k}={v}']]
    generation = directory / 'generation'
    command([cad, 'build', RECIPE, '--out-dir', generation, *flags])
    manifest = json.loads((generation / 'manifest.json').read_text())
    rail_volume = length * (400 - setback**2)
    carriage_volume = (3600 - 4*math.pi*bore**2)*8
    expected_volume = 2*(rail_volume + carriage_volume)
    rail_z = (length*400*10 - length*setback**2*(20-setback/3))/rail_volume
    center = [length/2, 45, (rail_volume*rail_z + carriage_volume*24)/(rail_volume+carriage_volume)]
    step = generation / 'modular_linear_stage.step'
    imported = body_properties(step)
    close(imported[0], expected_volume, 'assembly volume')
    for actual, expected in zip(imported[1], center):
        close(actual, expected, 'assembly centroid')
    hierarchy = json_output([sys.executable, str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')), str(step), '--instances', '4', '--definitions', '2', '--solids', '4', '--assembly-json', str(generation/'modular_linear_stage.assembly.json')])
    sections = {}
    for name, area in [('rails', 40*length), ('carriages', 2*(3600-4*math.pi*bore**2))]:
        report = json.loads((generation / f'section-{name}.json').read_text())
        section = assembly_section(step, report['section']['plane'])
        close(section['area'], area, name+' area')
        for a, b in zip(section['centroid'], [length/2, 45]):
            close(a, b, name+' centroid')
        sections[name] = section
    replay = directory / 'replay'
    command([cad, 'build', generation/'recipe.bundle.json', '--out-dir', replay, *flags])
    reproduced = json.loads((replay/'manifest.json').read_text())
    require(reproduced['outputs']['modular_linear_stage']['content_hash'] == manifest['outputs']['modular_linear_stage']['content_hash'], 'replayed content identity')
    require((replay/'modular_linear_stage.step').read_bytes() == step.read_bytes(), 'replayed STEP bytes')
    for output in [generation, replay]:
        require(command([cad, 'verify', output])['valid'], 'published integrity')
    report = dict(parameters=dict(length=length,setback=setback,bore=bore), expected_volume=expected_volume, expected_centroid=center, imported=imported, hierarchy=hierarchy, sections=sections, offline_replay=True)
    (directory/'report.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    return report

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=pathlib.Path, required=True)
    parser.add_argument('--out-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=False)
    cases=[]
    for label, values in [('low',(140,1,2)),('default',(160,2,3)),('high',(180,3,4))]:
        directory=args.out_dir/label;directory.mkdir()
        cases.append(case(args.cad.resolve(), directory, *values))
    print(json.dumps(dict(valid=True,cases=cases,retained_generations=[f'{label}/{name}' for label in ['low','default','high'] for name in ['generation','replay']],retained_reports=[f'{label}/report.json' for label in ['low','default','high']],oracle=dict(python_version=platform.python_version(),ocp_version=str(OCP.__version__),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'))),indent=2,allow_nan=False))

if __name__ == '__main__':
    main()
