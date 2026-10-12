"""Check associative projected dimensions against separately imported STEP sections."""
import argparse
import json
import math
import pathlib
import tempfile
import xml.etree.ElementTree as ET
from check_section_anchors_ocp import check as check_anchors
from check_sections_ocp import close
from check_constrained_fixture_ocp import require


def check(cad, directory):
    result = check_anchors(cad, directory)
    for generation, report_path, case in zip(result['retained_generations'], result['retained_reports'], result['cases']):
        for view, section in case['sections'].items():
            native = section['native']
            dimensions = native['section_view']['dimensions']
            require(len(dimensions) == 1, 'one pitch dimension per section')
            dim = dimensions[0]
            require(dim['status'] == 'passed' and dim['declaration']['id'] == 'station_pitch', 'dimension identity')
            require(dim['declaration']['axis'] == 'x', 'projected dimension direction')
            pins = [s for s in section['imported'] if abs(s['area']-math.pi*2.8**2) < 1e-6]
            require(len(pins) == 2, 'two independently sectioned locating pins')
            points = sorted([s['centroid'] for s in pins])
            if view == 'right_seat':
                points.reverse()
            for actual, expected in zip([dim['from_point'], dim['to_point']], points):
                for a,b in zip(actual, expected):
                    close(a,b,'datum-origin projection agrees with independently imported pin centre')
            delta = points[1][0]-points[0][0]
            close(dim['signed_delta_mm'],delta,'independent signed projected distance')
            close(dim['value_mm'],abs(delta),'independent projected dimension')
            close(dim['value_mm'],case['parameters']['spacing'],'source spacing design intent')
            for field, side in zip(['from_witness','to_witness'], ['left','right'] if view=='left_seat' else ['right','left']):
                witness = dim[field]
                require(witness['instance_path']==['carrier',side,'housing'], 'dimension exact nested path')
                require(witness['datum']=='top' and witness['semantic_datum']['selection']['status']=='resolved', 'dimension semantic resolution')
            svg = ET.fromstring((directory / generation / f'section-{view}.svg').read_bytes())
            groups = [g for g in svg.iter('{http://www.w3.org/2000/svg}g') if g.attrib.get('data-id')=='station_pitch']
            require(len(groups)==1,'visible dimension group')
            close(float(groups[0].attrib['data-value-mm']),abs(delta),'SVG dimension value')
            text = ' '.join(''.join(g.itertext()) for g in groups)
            require(f"station_pitch: {case['parameters']['spacing']} mm" in text,'visible dimension label')
            section['independent_dimension'] = dict(imported_pin_centres=points, signed_delta_mm=delta, value_mm=abs(delta))
        (directory / report_path).write_text(json.dumps(case,indent=2,allow_nan=False)+'\n')
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=pathlib.Path,required=True)
    parser.add_argument('--out-dir',type=pathlib.Path)
    args=parser.parse_args()
    if args.out_dir:
        args.out_dir.mkdir(parents=True,exist_ok=False)
        result=check(args.cad.resolve(),args.out_dir)
    else:
        with tempfile.TemporaryDirectory(prefix='cad-section-dimensions-') as temporary:
            result=check(args.cad.resolve(),pathlib.Path(temporary))
    print(json.dumps(result,indent=2,allow_nan=False))


if __name__=='__main__':
    main()
