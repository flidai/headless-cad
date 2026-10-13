#!/usr/bin/env python3
"""Independent STEP and SVG checks for native source-owned slot dimensions."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import platform
import importlib.metadata
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('slot_dimension_support', ROOT/'tools/check-obround-slots.py')
o = importlib.util.module_from_spec(spec)
spec.loader.exec_module(o)
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS


def require(value, why):
    if not value:
        raise AssertionError(why)


def close(a, b, why, tolerance=2e-6):
    require(math.isfinite(a) and math.isfinite(b) and abs(a-b) <= tolerance,
            f'{why}: {a} != {b}')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def inputs():
    paths = list((ROOT/'tools').glob('*.py')) + list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [ROOT/'examples'/name for name in ('drive_module_slot_drawing.star',
              'drive_module_slot_drawing.star.lock.json', 'drive_module_rotation.star')]
    return {str(path.relative_to(ROOT)): sha(path) for path in sorted(paths)}


def step_caps(path):
    solid = o.k.load(str(path))
    result = []
    it = TopExp_Explorer(solid, TopAbs_FACE)
    while it.More():
        surface = BRepAdaptor_Surface(TopoDS.Face(it.Current()))
        if surface.GetType() == GeomAbs_Cylinder:
            cylinder = surface.Cylinder()
            if abs(cylinder.Radius()-3) < 2e-6:
                axis = cylinder.Axis().Direction()
                close(abs(axis.Z()), 1, 'slot cylinder axis')
                close(surface.LastUParameter()-surface.FirstUParameter(), math.pi, 'native half-cylinder trim')
                close(surface.LastVParameter()-surface.FirstVParameter(), 6, 'through plate height')
                p = cylinder.Location()
                result.append({'center': [p.X(), p.Y()], 'radius': cylinder.Radius()})
        it.Next()
    require(len(result) == 8, 'four slots have eight actual STEP semicylinder walls')
    return result


def groups(path):
    return [node for node in ET.parse(path).getroot().iter()
            if node.attrib.get('class') == 'slot-dimension']


def svg_check(path, dimensions, independent):
    nodes = groups(path)
    require(len(nodes) == 12, f'{path.name} contains twelve native slot dimensions')
    require({n.attrib['data-dimension-id'] for n in nodes}==set(dimensions),'exact SVG dimension inventory')
    for node in nodes:
        name = node.attrib['data-dimension-id']
        d = dimensions[name]
        m = d['measurement']
        expected_geometry=independent[name]
        require(node.attrib['data-kind'] == d['declaration']['kind'], 'SVG kind agrees with report')
        close(float(node.attrib['data-value-mm']), d['value_mm'], 'SVG measured value')
        for axis, expected in zip(('x', 'y'), m['center']):
            close(float(node.attrib[f'data-center-{axis}']), expected, 'SVG native center')
        native_from = [float(node.attrib[f'data-native-from-{axis}']) for axis in ('x', 'y')]
        native_to = [float(node.attrib[f'data-native-to-{axis}']) for axis in ('x', 'y')]
        require(min(math.dist(native_from, expected_geometry['points'][0])+math.dist(native_to,expected_geometry['points'][1]),
                    math.dist(native_from, expected_geometry['points'][1])+math.dist(native_to,expected_geometry['points'][0]))<4e-6,
                'rendered native endpoints match independently measured STEP capsule')
        close(math.dist(native_from, native_to), d['value_mm'], 'native dimension endpoint distance')
        lines = [e for e in node if e.tag.rsplit('}',1)[-1] == 'line']
        arrows = [e for e in node if e.tag.rsplit('}',1)[-1] == 'polygon']
        require(len(lines) == 3 and len(arrows) == 2, 'actual SVG extension lines and arrowheads')
        # Rendered section Y is inverted. Extension lines must start on the
        # independent native endpoints, not on the annotation label position.
        for line, point in zip(lines[:2], (native_from, native_to)):
            close(float(line.attrib['x1']), point[0], 'rendered extension starts on native x')
            close(float(line.attrib['y1']), -point[1], 'rendered extension starts on native y')
        line = lines[2]
        ends = [[float(line.attrib[a]) for a in pair] for pair in (('x1','y1'),('x2','y2'))]
        close(math.dist(*ends), d['value_mm'], 'rendered dimension line has native measured length')
        for point,actual in zip((native_from,native_to),ends):
            close(actual[0],point[0]+d['declaration']['label_offset'][0],'dimension line offset x')
            close(actual[1],-point[1]-d['declaration']['label_offset'][1],'dimension line offset y')
        marks=[e for e in node if e.attrib.get('class')=='slot-center-mark']
        require(len(marks)==3,'three actual center marks')
        measured_marks=[]
        for mark in marks:
            v=list(map(float,re.findall(r'[-+]?(?:\d*\.)?\d+(?:[eE][-+]?\d+)?',mark.attrib['d'])))
            require(len(v)==8,'two straight center mark strokes')
            point=[(v[0]+v[2])/2,-(v[1]+v[3])/2]
            close((v[4]+v[6])/2,point[0],'vertical mark center x')
            close(-(v[5]+v[7])/2,point[1],'vertical mark center y')
            measured_marks.append(point)
        require(all(any(math.dist(p,q)<2e-6 for q in measured_marks) for p in expected_geometry['marks']),
                'printed center marks match independent STEP centers')
        text=next(e for e in node if e.tag.rsplit('}',1)[-1]=='text').text
        value=f"{expected_geometry['value']:g}"
        if d['declaration']['kind']!='centerline_length':
            require(value+' +0.1/+0' in text,'visible authored nominal and size deviations')
        else:
            require(f': {value} mm centerline_length' in text or f':C{value}mm' in text,'visible center spacing')
        if path.name.startswith('drawing-'):
            name_parts=name.split('_');tag={'front':'F','rear':'R'}[name_parts[0]]+{'minus':'M','plus':'P'}[name_parts[1]]+{'width':'W','overall':'O','centerline':'C'}[name_parts[2]]
            require(text.startswith(tag+':'),'printed short tag identifies exact dimension')
        else:require(text.startswith(name+':'),'section label identifies exact dimension')
        for arrow, endpoint in zip(arrows, ends):
            first = re.split(r'[ ,]+', arrow.attrib['points'].strip())[:2]
            for a,b in zip(map(float, first), endpoint):
                close(a,b,'arrow tip lies on dimension endpoint')
    return len(nodes)



def segment_crosses_box(a,b,box):
    # Clip a finite SVG segment against the interior of a conservative text box.
    low,high=0.,1.
    for axis in range(2):
        delta=b[axis]-a[axis]
        if abs(delta)<1e-12:
            if not box[axis]<a[axis]<box[axis+2]:return False
        else:
            enter=(box[axis]-a[axis])/delta;leave=(box[axis+2]-a[axis])/delta
            if enter>leave:enter,leave=leave,enter
            low=max(low,enter);high=min(high,leave)
            if low>=high:return False
    return high>max(low,0) and low<1


def check_case(cad, destination, name, parameters):
    directory = destination/name
    arguments = [word for k,v in parameters.items() for word in ('--param', f'{k}={v}')]
    manifest = o.command(cad, 'build', ROOT/'examples/drive_module_slot_drawing.star',
                         '--out-dir', directory, *arguments)
    report = read(directory/'section-base_slot_plan.json')
    o.command(cad,'verify',directory)
    require(report['snapshot']['recipe_source_hash']==manifest['recipe_source_hash']
            and report['snapshot']['content_hash']==manifest['outputs']['base']['content_hash'], 'source and native target report binding')
    dimensions = {d['declaration']['id']: d for d in report['section_view']['slot_dimensions']}
    require(set(dimensions)=={name+'_'+kind for name in ('front_minus','front_plus','rear_minus','rear_plus')
                             for kind in ('width','overall','centerline')}, 'exact four slots times three dimensions')
    for dimension_name,d in dimensions.items():
        suffix=dimension_name.rsplit('_',1)[1]
        require(d['declaration']['role']=='slot_'+dimension_name.rsplit('_',1)[0]+'_walls'
                and d['declaration']['kind']=={'width':'width','overall':'overall_length','centerline':'centerline_length'}[suffix], 'exact authored role and kind')
    caps = step_caps(directory/'base.step')
    covered = set()
    native = {}
    independent = {}
    for d in dimensions.values():
        require(d['status'] == 'passed' and d['target_content_hash']==manifest['outputs']['base']['content_hash'], 'passed native dimension bound to actual base')
        m = d['measurement']; selected = []
        for point in m['centerline_endpoints']:
            matches = [i for i,c in enumerate(caps) if math.dist(c['center'], point)<2e-6]
            require(len(matches)==1, 'each reported slot endpoint matches one exact STEP cylinder')
            selected.append(matches[0])
        require(len(set(selected))==2, 'two distinct capsule ends')
        centers = [caps[i]['center'] for i in selected]
        radius = caps[selected[0]]['radius']
        length = math.dist(*centers)
        close(m['radius_mm'],radius,'STEP slot radius')
        close(m['width_mm'],radius*2,'STEP slot width')
        close(m['centerline_length_mm'],length,'STEP slot centerline length')
        close(m['overall_length_mm'],length+2*radius,'STEP slot overall length')
        for j in range(2):close(m['center'][j],(centers[0][j]+centers[1][j])/2,'STEP slot center')
        kind=d['declaration']['kind']
        close(d['value_mm'],m[kind+'_mm'],'requested native size')
        axis=[(centers[1][i]-centers[0][i])/length for i in range(2)]
        center=[(centers[0][i]+centers[1][i])/2 for i in range(2)]
        if kind=='width':
            normal=[-axis[1],axis[0]]
            points=[[center[i]+sign*radius*normal[i] for i in range(2)] for sign in (-1,1)]
            value=radius*2
        elif kind=='overall_length':
            points=[[centers[0][i]-radius*axis[i] for i in range(2)], [centers[1][i]+radius*axis[i] for i in range(2)]]
            value=length+2*radius
        else:points=centers;value=length
        independent[d['declaration']['id']]={'points':points,'marks':[center,*centers],'value':round(value,6)}
        if kind!='centerline_length':
            for field,expected in [('nominal_mm',value),('lower_limit_mm',value),('upper_limit_mm',value+.1),('lower_deviation_mm',0),('upper_deviation_mm',.1)]:
                close(d['tolerance'][field],expected,'retained authored size band')
        else:require('tolerance' not in d,'center spacing remains a design bound without invented size band')
        role = d['declaration']['role']
        native[role] = {'center':m['center'],'radius_mm':radius,'centerline_length_mm':length,'overall_length_mm':length+2*radius}
        covered.update(selected)
        require(len(m['segments'])==4 and len({s['face'] for s in m['segments']})==4,
                'four distinct native section wall witnesses')
        require(m['arithmetic_uncertainty_mm']>0 and m['arithmetic_uncertainty_mm']<1e-6,
                'finite bounded arithmetic interval')
    require(len(native)==4 and len(covered)==8, 'all four STEP slot pairs measured')
    svg_count=svg_check(directory/'section-base_slot_plan.svg',dimensions,independent)
    paper_path=directory/'drawing-base_slot_machining.svg'
    paper_count=svg_check(paper_path,dimensions,independent)
    paper=ET.parse(paper_path).getroot()
    viewport=next(n for n in paper.iter() if n.attrib.get('id')=='drawing-view-base_slot_plan')
    view_box=list(map(float,viewport.attrib['viewBox'].split()))
    scale=float(viewport.attrib['width'])/view_box[2]
    close(scale,parameters.get('slot_drawing_scale',1),'actual paper viewport scale')
    boxes=[]
    annotation_lines=[]
    for group in groups(paper_path):
        for line in group:
            if line.tag.rsplit('}',1)[-1]=='line':
                annotation_lines.append(([float(line.attrib['x1']),float(line.attrib['y1'])],
                                         [float(line.attrib['x2']),float(line.attrib['y2'])]))
    for group in groups(paper_path):
        text=next(n for n in group if n.tag.rsplit('}',1)[-1]=='text')
        font=float(text.attrib['font-size']);close(font*scale,2.8,'printed slot label height',tolerance=2e-5)
        x=float(text.attrib['x']);y=float(text.attrib['y']);w=float(text.attrib['textLength'])
        box=(x-w/2,y-font,x+w/2,y+font*.3)
        require(box[0]>=view_box[0]-1e-6 and box[2]<=view_box[0]+view_box[2]+1e-6
                and box[1]>=view_box[1]-1e-6 and box[3]<=view_box[1]+view_box[3]+1e-6,'paper label inside viewport')
        for old in boxes:
            require(box[2]<=old[0] or old[2]<=box[0] or box[3]<=old[1] or old[3]<=box[1], 'printed label boxes do not overlap')
        require(not any(segment_crosses_box(a,b,box) for a,b in annotation_lines),'printed text clears all dimension and extension lines')
        boxes.append(box)
    replay=destination/(name+'-replay')
    repeated=o.command(cad,'build',directory/'recipe.bundle.json','--out-dir',replay,*arguments)
    for artifact in ('base.step','base.brep.json','section-base_slot_plan.json',
                     'section-base_slot_plan.svg','drawing-base_slot_machining.json',
                     'drawing-base_slot_machining.svg','source-provenance.json'):
        require((directory/artifact).read_bytes()==(replay/artifact).read_bytes(),f'exact replay {artifact}')
    require(manifest['feature_hashes']==repeated['feature_hashes'],'native feature replay')
    return {'name':name,'parameters':parameters,'native_slots':native,
            'section_dimensions':svg_count,'paper_dimensions':paper_count,
            'base_content_hash':manifest['outputs']['base']['content_hash'],
            'feature_hashes':manifest['feature_hashes'],
            'recipe_context_hash':manifest['recipe_context_hash'],'replayed_files':7}


def refusals(cad,destination):
    directory=destination/'revisions';directory.mkdir()
    source=directory/'drive_module_slot_drawing.star'
    original=(ROOT/'examples/drive_module_slot_drawing.star').read_text()
    for name in ('drive_module_rotation.star','drive_module_slot_drawing.star.lock.json'):
        (directory/name).write_bytes((ROOT/'examples'/name).read_bytes())
    source.write_text(original)
    out=destination/'preserved'
    o.command(cad,'build',source,'--out-dir',out)
    baseline=o.inventory(out)
    cases=[('missing_role','role = role','role = "missing_slot"'),
           ('incomplete_role','role = role','role = "slot_front_minus_wall_0"'),
           ('wrong_width','"nominal":6','"nominal":7'),
           ('impossible_length','min = base_slot_length-0.000001, max = base_slot_length+0.000001','min = base_slot_length+1, max = base_slot_length+2'),
           ('event_plane','origin = [0,0,3]','origin = [0,0,0]'),
           ('bad_label','label_offset = [width_offset,0]','label_offset = [0,0]'),
           ('crossing_label','center_offset = -50 if i < 2 else 50','center_offset = 0')]
    results=[]
    for label,before,after in cases:
        require(before in original,'refusal edit anchor')
        source.write_text(original.replace(before,after,1))
        failure=o.command(cad,'build',source,'--out-dir',out,success=False)
        if label in ('missing_role','incomplete_role','wrong_width','impossible_length'):
            require(failure['code']=='section_dimension_failed' and failure['section_view_id']=='base_slot_plan', f'{label} refused by intended native dimension')
            suffix='centerline' if label=='impossible_length' else 'width'
            require(failure['dimension_id']=='front_minus_'+suffix, f'{label} intended dimension ID')
        elif label=='event_plane':
            require(failure['code'] in ('section_view_failed','section_dimension_failed') and failure.get('section_view_id')=='base_slot_plan','event plane refused by native section')
        elif label=='crossing_label':
            require(failure['code']=='export_failed' and 'paper drawing slot line crosses an annotation label' in failure['error'],'crossing label refused by paper layout')
        else:require(failure['code']=='recipe_failed' and 'slot label offset must be finite nonzero' in failure['error'],'bad label refused by annotation admission')
        require(o.inventory(out)==baseline,f'{label} preserves accepted generation')
        results.append({'case':label,'code':failure['code'],'diagnostic':failure.get('diagnostic',failure.get('error'))})
    return results


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();cad=args.cad.resolve();args.out_dir.mkdir(parents=True,exist_ok=False)
    binary=sha(cad);sources=inputs()
    cases=[check_case(cad,args.out_dir,name,params) for name,params in
           [('default',{}),('short',{'base_slot_length':2}),('wide',{'base_slot_length':8,'mounting_pitch':90}),('paper_three_quarter_scale',{'slot_drawing_scale':0.75})]]
    changed={k for k,v in cases[0]['feature_hashes'].items() if cases[1]['feature_hashes'][k]!=v}
    require(changed=={'base_slotted'}, 'slot length edit changes only native slotted base feature')
    require(cases[0]['feature_hashes']==cases[3]['feature_hashes'], 'paper scale preserves native feature hashes')
    require((args.out_dir/'default/base.step').read_bytes()==(args.out_dir/'paper_three_quarter_scale/base.step').read_bytes(), 'paper scale preserves exact STEP bytes')
    failed=refusals(cad,args.out_dir)
    require(sha(cad)==binary and inputs()==sources,'frozen binary/source/helper bytes')
    report={'valid':True,'executable_sha256':binary,'oracle_script_sha256':sha(__file__),
            'source_inventory_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),'cases':cases,'refusals':failed,
            'scope':'Native through-slot section sizes and drawing endpoints independently measured from STEP. Authored size bands are not manufactured-part conformance or GD&T.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refusals':len(failed),'report':str(args.out_dir/'report.json')}))

if __name__=='__main__':main()
