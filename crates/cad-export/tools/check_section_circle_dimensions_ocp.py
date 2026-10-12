"""Independent STEP circle intersections for semantic radius/diameter annotations."""
import argparse
import json
import math
import pathlib
import tempfile
import xml.etree.ElementTree as ET
from check_section_dimensions_ocp import check as check_dimensions
from check_section_views_ocp import check as check_cover_views
from check_sections_ocp import close, imported_section
from check_step_metadata_ocp import command
from check_constrained_fixture_ocp import require

ROOT=pathlib.Path(__file__).resolve().parents[3]


def validate(native, imported, radius, kind, center, path, svg_path, index=0):
    dimensions=native['section_view']['circle_dimensions']
    require(len(dimensions)>index,'semantic circle dimension absent')
    dim=dimensions[index]
    require(dim['status']=='passed' and dim['selection']['status']=='resolved','resolved circle dimension')
    require(len(dim['selection']['witnesses'])==1,'unique semantic face')
    require(dim['declaration']['kind']==kind,'dimension kind')
    require(dim.get('instance_path')==path,'exact occurrence scope')
    close(dim['radius'],radius,'analytic slice radius')
    for a,b in zip(dim['center'],center):close(a,b,'analytic circle centre')
    factor=2 if kind=='diameter' else 1
    close(dim['value_mm'],factor*radius,'analytic dimension')
    matches=[]
    for section in imported:
        for curve in section['curves']:
            if curve['kind']=='arc' and abs(curve['radius']-radius)<1e-7 and all(abs(a-b)<1e-7 for a,b in zip(curve['center'],center)):
                matches.append(curve)
    require(matches,'independently imported circle absent')
    close(sum(abs(curve['sweep']) for curve in matches),math.tau,'one complete imported circle')
    for curve in matches:
        close(factor*curve['radius'],dim['value_mm'],'independent STEP dimension')
        for a,b in zip(curve['center'],dim['center']):close(a,b,'independent STEP centre')
    svg=ET.fromstring(svg_path.read_bytes())
    require(json.loads(svg.find('{http://www.w3.org/2000/svg}metadata').text)==native,'SVG bound circle evidence')
    groups=[g for g in svg.iter('{http://www.w3.org/2000/svg}g') if g.attrib.get('class')=='section-circle-dimension' and g.attrib.get('data-id')==dim['declaration']['id']]
    require(len(groups)==1,'visible circle annotation')
    require(groups[0].attrib['data-kind']==kind,'visible circle kind')
    close(float(groups[0].attrib['data-value-mm']),factor*radius,'SVG native circle value')
    return dict(imported_curves=matches,expected_radius=radius,expected_value_mm=factor*radius)


def check(cad,directory):
    result=check_dimensions(cad,directory)
    for generation, report_path, case in zip(result['retained_generations'],result['retained_reports'],result['cases']):
        for name,section in case['sections'].items():
            side='left' if name=='left_seat' else 'right'
            native=section['native']
            require(native['section_view']['circle_dimensions'][0]['selection']['selector']['anchor_feature']=='sink_body','cone lineage source')
            section['independent_circle_dimension']=validate(native,section['imported'],3+case['parameters']['depth']/2,'diameter' if side=='left' else 'radius',[0,0],['carrier',side,'housing'],directory/generation/f'section-{name}.svg')
        (directory/report_path).write_text(json.dumps(case,indent=2,allow_nan=False)+'\n')
    body_cases=[]
    recipe=ROOT/'examples/countersink.star'
    for depth in [2,4,5]:
        folder=directory/f'body-{depth}';folder.mkdir();generation=folder/'generation'
        built=command([cad,'build',recipe,'--out-dir',generation,'--param',f'depth={depth}'])
        manifest=json.loads((generation/'manifest.json').read_text())
        sections={}
        for name,radius in [('pilot',3),('seat',3+depth/2)]:
            native=json.loads((generation/f'section-{name}.json').read_text())
            require(native['section_view']['circle_dimensions']==manifest['section_views'][name]['circle_dimensions'],'manifest circle witness binding')
            require(native['snapshot']['recipe_source_hash']==manifest['recipe_source_hash'],'source binding')
            dim=native['section_view']['circle_dimensions'][0]
            require(dim['target_content_hash']==built['outputs']['countersunk']['content_hash'],'target content binding')
            require(dim['selection']['selector']['anchor_feature']==('pilot' if name=='pilot' else 'sink_body'),'body selector source')
            actual=imported_section(generation/'countersunk.step',native['section']['plane'])
            comparison=validate(native,[actual],radius,'diameter',[30,20],None,generation/f'section-{name}.svg')
            sections[name]=dict(native=native,imported=actual,independent_circle_dimension=comparison)
        require(command([cad,'verify',generation])['valid'],'body generation integrity')
        case=dict(depth=depth,sections=sections)
        report_path=f'body-{depth}/report.json'
        (directory/report_path).write_text(json.dumps(case,indent=2,allow_nan=False)+'\n')
        body_cases.append(case);result['retained_generations'].append(f'body-{depth}/generation');result['retained_reports'].append(report_path)
    result['body_cases']=body_cases
    cover_dir=directory/'covers';cover_dir.mkdir()
    covers=check_cover_views(cad,cover_dir)
    for generation,report_path,case in zip(covers['retained_generations'],covers['retained_reports'],covers['cases']):
        section=case['sections']['seat'];native=section['native']
        require(len(native['section_view']['circle_dimensions'])==2,'cover dimension inventory')
        comparisons=[]
        for index,radius,center,anchor,role in [(0,3.2+case['parameters']['seat_depth']/2,[case['parameters']['bolt_pitch'],0],'seat_cut_0','bore_wall'),(1,20,[0,0],'stock','inner_wall')]:
            selector=native['section_view']['circle_dimensions'][index]['selection']['selector']
            require(selector['anchor_feature']==anchor and selector['role']==role,'pattern-specific semantic identity')
            comparisons.append(validate(native,[section['imported']],radius,'diameter',center,None,cover_dir/generation/'section-seat.svg',index))
        section['independent_circle_dimensions']=comparisons
        (cover_dir/report_path).write_text(json.dumps(case,indent=2,allow_nan=False)+'\n')
        result['retained_generations'].append('covers/'+generation);result['retained_reports'].append('covers/'+report_path)
    result['cover_cases']=covers['cases']
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);parser.add_argument('--out-dir',type=pathlib.Path);args=parser.parse_args()
    if args.out_dir:
        args.out_dir.mkdir(parents=True,exist_ok=False);result=check(args.cad.resolve(),args.out_dir)
    else:
        with tempfile.TemporaryDirectory(prefix='cad-circle-dimensions-') as temporary:result=check(args.cad.resolve(),pathlib.Path(temporary))
    print(json.dumps(result,indent=2,allow_nan=False))


if __name__=='__main__':main()
