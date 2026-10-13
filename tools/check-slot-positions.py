#!/usr/bin/env python3
"""Independent STEP and SVG oracle for source-owned native slot positions."""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('slot_dimension_support', ROOT/'tools/check-slot-dimensions.py')
support = importlib.util.module_from_spec(spec)
spec.loader.exec_module(support)
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepGProp import BRepGProp
from OCP.GeomAbs import GeomAbs_Plane
from OCP.GProp import GProp_GProps
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def close(actual, expected, message, tolerance=2e-6):
    require(math.isfinite(actual) and math.isfinite(expected)
            and abs(actual-expected) <= tolerance,
            f'{message}: {actual} != {expected}')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def inputs():
    paths = list((ROOT/'tools').glob('*.py')) + list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [ROOT/path for path in (
        'examples/drive_module_slot_positions.star',
        'examples/drive_module_slot_positions.star.lock.json',
        'examples/drive_module_rotation.star',
        'crates/cad-kernel/src/slot_datum.rs',
        'crates/cad-build/src/slot_center_datums.rs',
        'crates/cad-build/src/section_views.rs',
        'crates/cad-recipe/src/lib.rs',
        'crates/cad-cli/src/section.rs',
        'crates/cad-cli/src/drawing.rs',
        'crates/cad-export/src/section.rs',
        'crates/cad-export/src/drawing.rs',
    )]
    return {str(path.relative_to(ROOT)): sha(path) for path in sorted(set(paths))}


def exact_step_geometry(path):
    """Read actual STEP half-cylinder and surviving planar wall supports."""
    caps = support.step_caps(path)
    remaining = set(range(len(caps)))
    pairs = []
    while remaining:
        i = min(remaining)
        nearest = sorted((math.dist(caps[i]['center'], caps[j]['center']),j)
                         for j in remaining if j != i)
        require(nearest and 1 < nearest[0][0] < 20, 'one unambiguous STEP capsule end pair')
        distance,j = nearest[0]
        require(not nearest[1:] or nearest[1][0] > distance+1, 'STEP end pair is distinct from adjacent slots')
        remaining.remove(i);remaining.remove(j)
        a,b = caps[i]['center'],caps[j]['center']
        pairs.append({'center':[(a[k]+b[k])/2 for k in range(2)],
                      'endpoints':[a,b], 'centerline_length_mm':distance,
                      'radius_mm':caps[i]['radius']})
    require(len(pairs)==4, 'four complete native STEP capsules')
    pairs.sort(key=lambda pair:(pair['center'][0],pair['center'][1]))
    slots = dict(zip(('front_minus','front_plus','rear_minus','rear_plus'),pairs))
    require(slots['front_minus']['center'][1] < slots['front_plus']['center'][1]
            and slots['rear_minus']['center'][1] < slots['rear_plus']['center'][1],
            'actual STEP lower and upper slot ordering')
    shape = support.o.k.load(str(path))
    candidates = {'left_edge':[], 'lower_edge':[]}
    it = TopExp_Explorer(shape,TopAbs_FACE)
    while it.More():
        face = TopoDS.Face(it.Current())
        surface = BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Plane:
            plane = surface.Plane();normal=plane.Axis().Direction();at=plane.Location()
            props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
            mass=props.Mass();point=props.CentreOfMass()
            if mass>10 and abs(abs(normal.X())-1)<1e-9 and abs(at.X())<2e-6:
                candidates['left_edge'].append((mass,[point.X(),point.Y(),point.Z()]))
            if mass>10 and abs(abs(normal.Y())-1)<1e-9 and abs(at.Y()+40)<2e-6:
                candidates['lower_edge'].append((mass,[point.X(),point.Y(),point.Z()]))
        it.Next()
    edges={}
    for name,found in candidates.items():
        require(len(found)==1, f'one surviving STEP {name} planar face')
        edges[name]=found[0][1]
    close(edges['left_edge'][0],0,'STEP left-edge plane')
    close(edges['lower_edge'][1],-40,'STEP lower-edge plane')
    return slots,edges


SPECS={
    'front_x':('slot_positions','left_edge','front_minus','x'),
    'rear_x':('slot_positions','left_edge','rear_minus','x'),
    'lower_y':('slot_positions','lower_edge','front_minus','y'),
    'upper_y':('slot_positions','lower_edge','front_plus','y'),
    'pitch_x':('slot_pitches','front_plus','rear_plus','x'),
    'pitch_y':('slot_pitches','rear_minus','rear_plus','y'),
}


def element_name(element):
    return element.tag.rsplit('}',1)[-1]


def groups(root):
    return {node.attrib['data-id']:node for node in root.iter()
            if node.attrib.get('class')=='section-dimension'}


def svg_check(source, dimensions, independent, paper_scale=None):
    root=ET.parse(source).getroot() if isinstance(source,Path) else source
    nodes=groups(root)
    require(set(nodes)==set(dimensions),'exact projected dimension IDs in one SVG view')
    lines=[];boxes=[]
    for name,node in nodes.items():
        dimension=dimensions[name]
        frm,to=independent[name]
        axis=0 if dimension['declaration']['axis']=='x' else 1
        offset=dimension['declaration']['offset']
        a,b=list(frm),list(to);a[1-axis]=offset;b[1-axis]=offset
        expected=[(frm,a),(to,b),(a,b)]
        actual=[child for child in node if element_name(child)=='line']
        arrows=[child for child in node if element_name(child)=='polygon']
        half_stroke=float(node.attrib['stroke-width'])/2
        require(len(actual)==3 and len(arrows)==2,'three projected lines and two arrowheads')
        close(float(node.attrib['data-value-mm']),abs(to[axis]-frm[axis]),'SVG native projected value')
        for line,(start,end) in zip(actual,expected):
            for key,value in zip(('x1','y1','x2','y2'),(start[0],-start[1],end[0],-end[1])):
                close(float(line.attrib[key]),value,'SVG line native point or offset')
            lines.append(([float(line.attrib['x1']),float(line.attrib['y1'])],
                          [float(line.attrib['x2']),float(line.attrib['y2'])],half_stroke))
        for arrow,endpoint in zip(arrows,(a,b)):
            first=[float(value) for value in arrow.attrib['points'].replace(',', ' ').split()[:2]]
            close(first[0],endpoint[0],'SVG arrow tip x')
            close(first[1],-endpoint[1],'SVG arrow tip y')
        if paper_scale is not None:
            text=next(child for child in node if element_name(child)=='text')
            font=float(text.attrib['font-size'])
            close(font*paper_scale,2.8,'declared paper label font',tolerance=2e-5)
            require(text.text and text.text.startswith({'front_x':'X1','rear_x':'X2','lower_y':'Y1',
                    'upper_y':'Y2','pitch_x':'PX','pitch_y':'PY'}[name]+':'),
                    'visible short label identifies native dimension')
            x=float(text.attrib['x']);y=float(text.attrib['y'])
            width=len(text.text.encode())*font*.7
            box=(x-width/2,y-font*1.2,x+width/2,y+font*.4)
            if root.attrib.get('viewBox'):
                view_box=[float(value) for value in root.attrib['viewBox'].split()]
                require(box[0]>=view_box[0]-1e-6 and box[2]<=view_box[0]+view_box[2]+1e-6
                        and box[1]>=view_box[1]-1e-6 and box[3]<=view_box[1]+view_box[3]+1e-6,
                        'printed label remains inside paper viewport')
            boxes.append((name,box))
        else:
            text=next(child for child in node if element_name(child)=='text')
            require(text.text and text.text.startswith(name+':'),'section SVG retains source ID')
    if paper_scale is not None:
        for i,(name,box) in enumerate(boxes):
            for other,old in boxes[:i]:
                require(box[2]<=old[0] or old[2]<=box[0] or box[3]<=old[1] or old[3]<=box[1],
                        f'paper labels {name}/{other} do not overlap')
            require(not any(support.segment_crosses_box(a,b,(box[0]-stroke,box[1]-stroke,
                    box[2]+stroke,box[3]+stroke)) for a,b,stroke in lines),
                    f'paper label {name} clears projected lines')
    return len(nodes)


def check_case(cad,destination,case_name,parameters):
    directory=destination/case_name
    arguments=[word for key,value in parameters.items() for word in ('--param',f'{key}={value}')]
    manifest=support.o.command(cad,'build',ROOT/'examples/drive_module_slot_positions.star',
                               '--out-dir',directory,*arguments)
    support.o.command(cad,'verify',directory)
    slots,edges=exact_step_geometry(directory/'base.step')
    native={**{key:value['center'] for key,value in slots.items()},**edges}
    scale=parameters.get('position_drawing_scale',1)
    p=parameters.get('mounting_pitch',60)
    length=parameters.get('base_slot_length',6)
    for key,point in slots.items():
        close(point['center'][0],33 if key.startswith('front') else p+33,'STEP longitudinal slot center')
        close(point['center'][1],-28 if key.endswith('minus') else 28,'STEP transverse slot center')
        close(point['centerline_length_mm'],length,'STEP slot centerline size')
    reports={view:read(directory/f'section-{view}.json') for view in ('slot_positions','slot_pitches')}
    dimensions={}
    for view,report in reports.items():
        require(report['snapshot']['recipe_source_hash']==manifest['recipe_source_hash'],
                'section source hash bound to build')
        require(report['section_view']['declaration']['output']=='base_machining_layout',
                'native assembly section output')
        for d in report['section_view']['dimensions']:
            dimension_id=d['declaration']['id']
            require(dimension_id in SPECS and SPECS[dimension_id][0]==view and dimension_id not in dimensions,
                    'exact view and dimension identity')
            dimensions[dimension_id]=d
    require(set(dimensions)==set(SPECS),'six native section position/pitch dimensions')
    independently={}
    for name,(view,from_name,to_name,axis_name) in SPECS.items():
        d=dimensions[name];axis=0 if axis_name=='x' else 1
        frm=native[from_name][:2];to=native[to_name][:2]
        independently[name]=(frm,to)
        for reported,expected,label in ((d['from_point'],frm,'from'),(d['to_point'],to,'to')):
            for j in range(2):close(reported[j],expected[j],f'{name} STEP {label} point')
        close(d['value_mm'],abs(to[axis]-frm[axis]),f'{name} STEP native projected distance')
        guard=d.get('arithmetic_uncertainty_mm')
        require(isinstance(guard,(int,float)) and 0<guard<1e-6,
                f'{name} retains finite native/placement arithmetic guard')
        require(d['declaration']['min'] <= d['value_mm']-guard
                and d['value_mm']+guard <= d['declaration']['max'],
                f'{name} authored limits enclose complete arithmetic interval')
        require(d['status']=='passed' and d['declaration']['axis']==axis_name,
                f'{name} passed intended projected axis')
        for role,witness in ((from_name,d['from_witness']),(to_name,d['to_witness'])):
            require(witness['instance_path']==['machining_leaf'] and witness['part_id']=='machining_base',
                    'one-leaf native assembly provenance')
            require(witness['datum']==role,'declared datum identity retained')
            for j in range(2):close(witness['world_frame']['translation'][j],native[role][j],
                                    f'{name}/{role} native world datum frame')
            if role in slots:
                evidence=witness['slot_datum']
                require(evidence['definition']['role']=='slot_'+role+'_walls'
                        and len(evidence['walls']['witnesses'])==4
                        and len({w['target_face'] for w in evidence['walls']['witnesses']})==4
                        and len(evidence['cap']['witnesses'])==1,
                        'four native slot walls and one cap bound to datum')
                require(evidence['target_content_hash']==manifest['outputs']['base']['content_hash']
                        and evidence['native']==manifest['slot_center_datums']['machining_base'][role]['native'],
                        'slot witness binds current native base and manifest datum')
                point=evidence['native']['frame']['origin']
                for j in range(2):close(point[j],slots[role]['center'][j],
                                        f'{role} native slot frame matches STEP')
                require(evidence['native']['arithmetic_uncertainty_mm']>0,
                        'native slot datum carries arithmetic guard')
            else:
                require('semantic_datum' in witness and 'slot_datum' not in witness,
                        'native planar wall face datum retained separately')
    require(set(manifest['slot_center_datums']['machining_base'])==set(slots),
            'manifest inventories exactly four slot center datums')
    counts={}
    for view in ('slot_positions','slot_pitches'):
        subset={name:d for name,d in dimensions.items() if SPECS[name][0]==view}
        independent_subset={name:independently[name] for name in subset}
        counts[view]=svg_check(directory/f'section-{view}.svg',subset,independent_subset)
    paper=ET.parse(directory/'drawing-base_slot_positions.svg').getroot()
    viewports={node.attrib['id'].removeprefix('drawing-view-'):node for node in paper.iter()
               if node.attrib.get('id','').startswith('drawing-view-')}
    require(set(viewports)=={'slot_positions','slot_pitches'},'two published paper viewports')
    for view,viewport in viewports.items():
        box=[float(v) for v in viewport.attrib['viewBox'].split()]
        actual_scale=float(viewport.attrib['width'])/box[2]
        close(actual_scale,scale,'actual paper viewport scale')
        subset={key:value for key,value in dimensions.items() if SPECS[key][0]==view}
        independent_subset={key:independently[key] for key in subset}
        counts['paper_'+view]=svg_check(viewport,subset,independent_subset,paper_scale=scale)
    replay=destination/(case_name+'-replay')
    repeated=support.o.command(cad,'build',directory/'recipe.bundle.json','--out-dir',replay,*arguments)
    artifacts=('base.step','base.brep.json','base_machining_layout.step',
               'section-slot_positions.json','section-slot_positions.svg',
               'section-slot_pitches.json','section-slot_pitches.svg',
               'drawing-base_slot_positions.json','drawing-base_slot_positions.svg',
               'source-provenance.json')
    for artifact in artifacts:
        require((directory/artifact).read_bytes()==(replay/artifact).read_bytes(),
                f'{case_name} exact captured replay {artifact}')
    require(manifest['feature_hashes']==repeated['feature_hashes'],'native feature hash replay')
    return {'name':case_name,'parameters':parameters,'slot_centers':{key:value['center'] for key,value in slots.items()},
            'edge_faces':edges,'dimensions':{key:d['value_mm'] for key,d in dimensions.items()},
            'svg_counts':counts,'replayed_files':len(artifacts),
            'feature_hashes':manifest['feature_hashes'],
            'base_content_hash':manifest['outputs']['base']['content_hash'],
            'recipe_context_hash':manifest['recipe_context_hash']}


def refusals(cad,destination):
    directory=destination/'revisions';directory.mkdir()
    source=directory/'drive_module_slot_positions.star'
    original=(ROOT/'examples/drive_module_slot_positions.star').read_text()
    for name in ('drive_module_rotation.star','drive_module_slot_positions.star.lock.json'):
        (directory/name).write_bytes((ROOT/'examples'/name).read_bytes())
    source.write_text(original)
    out=destination/'preserved'
    support.o.command(cad,'build',source,'--out-dir',out)
    baseline=support.o.inventory(out)
    changes=[('missing_role','role = "slot_"+name+"_walls"','role = "missing_slot"'),
             ('partial_wall','role = "slot_"+name+"_walls"','role = "slot_front_minus_wall_0"'),
             ('wrong_cap','role = "slot_"+name+"_walls"','role = "slot_"+name+"_walls", cap = "wall_0"'),
             ('impossible_bound','min = nominal-0.000001, max = nominal+0.000001','min = nominal+1, max = nominal+2')]
    results=[]
    for label,before,after in changes:
        require(before in original,f'{label} source edit anchor exists')
        source.write_text(original.replace(before,after,1))
        failure=support.o.command(cad,'build',source,'--out-dir',out,success=False)
        if label=='impossible_bound':
            require(failure['code']=='section_dimension_failed' and failure.get('dimension_id')=='front_x',
                    'impossible bound refused by intended native projection')
        else:
            require(failure['code']=='slot_center_datum_failed' and failure.get('part')=='machining_base'
                    and failure.get('datum')=='front_minus',f'{label} refused by intended native slot datum')
        require(support.o.inventory(out)==baseline,f'{label} preserves accepted complete generation')
        results.append({'case':label,'code':failure['code'],
                        'diagnostic':failure.get('diagnostic',failure.get('error'))})
    return results


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args()
    cad=args.cad.resolve()
    args.out_dir.mkdir(parents=True,exist_ok=False)
    binary=sha(cad);sources=inputs();script=sha(__file__)
    cases=[check_case(cad,args.out_dir,name,parameters) for name,parameters in (
        ('default',{}),('wide_pitch',{'mounting_pitch':90}),
        ('long_slot',{'base_slot_length':8}),
        ('paper_three_quarter_scale',{'position_drawing_scale':0.75}))]
    require(cases[0]['feature_hashes']==cases[3]['feature_hashes'],
            'paper scale preserves native feature hashes')
    require((args.out_dir/'default/base.step').read_bytes()==
            (args.out_dir/'paper_three_quarter_scale/base.step').read_bytes(),
            'paper scale preserves STEP bytes')
    require(cases[0]['dimensions']==cases[2]['dimensions'],
            'slot length changes size without changing measured centers and pitches')
    refused=refusals(cad,args.out_dir)
    require(sha(cad)==binary and inputs()==sources and sha(__file__)==script,
            'frozen binary, source and oracle helpers remain byte identical')
    report={'valid':True,'executable_sha256':binary,'oracle_script_sha256':script,
            'source_inventory_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),'cases':cases,'refusals':refused,
            'scope':'Native slot-center and surviving planar-edge datum positions from STEP; projected paper dimensions, not GD&T or manufactured-part conformance.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refusals':len(refused),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
