#!/usr/bin/env python3
"""Independent OCCT acceptance for source-owned seated mounting endpoints.

The Rust kernel publishes the nominal seat and static slot certificates. This
oracle independently imports STEP, finds the actual trimmed planar faces and
their nearest boundaries, and checks all thirty placed solids at both slot
ends. It does not treat a nominal seat as a preload or threaded connection.
"""

import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import shutil


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    'tightened_slot_support', Path(__file__).with_name('check-slot-adjustment.py'))
s = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(s)
d = s.d
k = s.k
c = s.c
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeVertex
from OCP.BRepClass import BRepClass_FaceClassifier
from OCP.BRepExtrema import BRepExtrema_DistShapeShape
from OCP.BRepGProp import BRepGProp
from OCP.GeomAbs import GeomAbs_Plane
from OCP.GProp import GProp_GProps
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_IN, TopAbs_REVERSED
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.gp import gp_Pnt


def require(ok, label):
    if not ok:
        raise AssertionError(label)


def close(actual, expected, label, absolute=2e-6):
    require(math.isfinite(actual) and abs(actual-expected) <= max(absolute, abs(expected)*2e-12),
            f'{label}: {actual} differs from {expected}')


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source_inventory():
    # These directories contain every Python helper reached by the dynamically
    # imported OCCT oracle chain, including its export and assembly support.
    sources=[*sorted((ROOT/'tools').glob('*.py')),
             *sorted((ROOT/'crates/cad-export/tools').glob('*.py'))]
    for name in ('drive_module_tightened.star','drive_module_rotation.star',
                 'drive_module_tightened.star.lock.json'):
        sources.append(ROOT/'examples'/name)
    require(all(path.is_file() for path in sources),'all pinned helper and recipe sources exist')
    return {path.relative_to(ROOT).as_posix():digest(path) for path in sources}


def read(path):
    return json.loads(Path(path).read_text())


def args_for(parameters):
    return [word for name, value in parameters.items() for word in ('--param', f'{name}={value}')]


def plane_face_with_disk(shape, center, expected_z, radius, label):
    """Classify a reported disk against an actual exported trimmed STEP face."""
    candidates=[]
    iterator=TopExp_Explorer(shape,TopAbs_FACE)
    point=gp_Pnt(*center)
    vertex=BRepBuilderAPI_MakeVertex(point).Vertex()
    while iterator.More():
        face=TopoDS.Face(iterator.Current());surface=BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Plane:
            plane=surface.Plane();normal=plane.Axis().Direction()
            if abs(plane.Location().Z()-expected_z)<2e-6 and abs(abs(normal.Z())-1)<1e-9:
                classifier=BRepClass_FaceClassifier();classifier.Perform(face,point,1e-8)
                if classifier.State()==TopAbs_IN:
                    closest=float('inf');edges=0
                    edge_iterator=TopExp_Explorer(face,TopAbs_EDGE)
                    while edge_iterator.More():
                        measure=BRepExtrema_DistShapeShape(vertex,edge_iterator.Current())
                        measure.Perform()
                        require(measure.IsDone(),f'{label} STEP trim edge distance')
                        closest=min(closest,measure.Value());edges+=1
                        edge_iterator.Next()
                    require(edges>=2 and math.isfinite(closest),f'{label} actual trimmed boundary')
                    props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
                    oriented=normal.Z()*(-1 if face.Orientation()==TopAbs_REVERSED else 1)
                    candidates.append((face,plane.Location().Z(),oriented,closest,edges,props.Mass()))
        iterator.Next()
    require(len(candidates)==1,f'{label} uniquely selected actual STEP face: {len(candidates)}')
    face,z,normal,closest,edges,area=candidates[0]
    require(closest+2e-6>=radius,
            f'{label} disk radius {radius} lies inside STEP boundary distance {closest}')
    return {'face':face,'support_z_mm':z,'oriented_normal_z':normal,
            'nearest_step_boundary_mm':closest,'edge_count':edges,'face_area_mm2':area}


def pairwise_nonpenetration(shapes,label):
    bounds={path:d.d.bounds(shape) for path,shape in shapes.items()}
    paths=list(shapes);pairs=0;tested=0;maximum=0.
    for i,left in enumerate(paths):
        for right in paths[i+1:]:
            pairs+=1
            if d.d.separated_boxes(bounds[left],bounds[right]):
                continue
            tested+=1
            volume=k.properties(k.boolean(BRepAlgoAPI_Common,shapes[left],shapes[right]))[0]
            maximum=max(maximum,volume)
            close(volume,0,f'{label} {left} / {right} STEP common volume',absolute=3e-6)
    require(pairs==435,f'{label} all thirty choose two STEP pairs')
    return {'pair_count':pairs,'nontrivial_bounding_pairs':tested,
            'maximum_common_volume_mm3':maximum}


def release_path(path):
    if path==('fixed_base',):
        return ('base',)
    require(path[0]=='moving',f'moving upper path {path}')
    rest=path[1:]
    if len(rest)==1 and rest[0].startswith('tight_bolt_'):
        return ('clamp_bolt_'+rest[0].rsplit('_',1)[1],)
    if len(rest)==1 and rest[0].startswith('tight_nut_'):
        return ('clamp_nut_'+rest[0].rsplit('_',1)[1],)
    return rest


def seat_check(name,report,shapes,manifest,expected_z,expected_left,expected_right):
    declaration=report['declaration'];query=report['query']
    require(report['status']=='passed' and declaration['id']==name and
            declaration['output']=='drive_module_tightened',f'{name} passed source seat')
    require(report['assembly_content_hash']==manifest['outputs']['drive_module_tightened']['content_hash'],
            f'{name} current assembled content')
    close(declaration['min_shared_disk_radius_mm'],.4,f'{name} authored minimum seat disk')
    require(declaration['left']['instance_path']==list(expected_left) and
            declaration['right']['instance_path']==list(expected_right),
            f'{name} exact intended occurrence pair')
    center=query['disk_center'];radius=report['disk_radius_lower_mm']
    require(len(center)==3 and all(math.isfinite(v) for v in center),f'{name} finite disk center')
    close(center[2],expected_z,f'{name} world seat plane')
    require(radius>=.4 and radius<=query['disk_radius_lower_mm'] and
            0<report['disk_area_lower_mm2']<=math.pi*radius*radius+1e-8,
            f'{name} conservative positive disk bounds')
    support={}
    for side,path in (('left',expected_left),('right',expected_right)):
        native=report[side]
        require(native['instance_path']==list(path) and
                native['semantic_datum']['selection']['status']=='resolved' and
                len(native['semantic_datum']['selection']['witnesses'])==1 and
                native['semantic_datum']['target_content_hash']==native['part_content_hash'],
                f'{name} resolved native face {side}')
        require(query[side+'_face']==native['semantic_datum']['selection']['witnesses'][0]['target_face'],
                f'{name} query uses selected native face {side}')
        support[side]=plane_face_with_disk(shapes[path],center,expected_z,radius,name+' '+side)
        close(native['world_plane']['origin'][2],support[side]['support_z_mm'],
              f'{name} native/STEP support {side}')
    require(support['left']['oriented_normal_z']*support['right']['oriented_normal_z']<-1+1e-9,
            f'{name} actual opposed STEP face normals')
    step_gap=(support['right']['support_z_mm']-support['left']['support_z_mm'])*support['left']['oriented_normal_z']
    require(report['signed_gap_lower_mm']<=0<=report['signed_gap_upper_mm'] and
            report['signed_gap_upper_mm']-report['signed_gap_lower_mm']<1e-5 and
            report['signed_gap_lower_mm']-2e-8<=step_gap<=report['signed_gap_upper_mm']+2e-8,
            f'{name} guarded coincidence includes actual STEP gap')
    pair_volume=k.properties(k.boolean(BRepAlgoAPI_Common,shapes[expected_left],shapes[expected_right]))[0]
    close(pair_volume,0,f'{name} paired exact STEP bodies do not penetrate',absolute=3e-6)
    return {'disk_center_mm':center,'disk_radius_lower_mm':radius,
            'disk_area_lower_mm2':report['disk_area_lower_mm2'],
            'actual_step_gap_mm':step_gap,'pair_common_volume_mm3':pair_volume,
            'left_step':{key:value for key,value in support['left'].items() if key!='face'},
            'right_step':{key:value for key,value in support['right'].items() if key!='face'}}


def validate_publication(out,manifest,source,loaded_source):
    require(digest(source)==digest(out/'recipe.star'),'exact captured tightened root source')
    bundle=read(out/'recipe.bundle.json')
    require(bundle['source']==source.read_text() and
            bundle['modules']['drive_module_rotation.star']==loaded_source.read_text(),
            'exact captured imported release recipe')
    require(manifest['source_bundle_hash'] and manifest['recipe_context_hash'],
            'source-owned release identity')
    provenance=read(out/'source-provenance.json')
    require(provenance['recipe_context_hash']==manifest['recipe_context_hash'] and
            provenance['source_bundle_hash']==manifest['source_bundle_hash'],
            'exact declaration sidecar bound to current captured bundle')
    require(provenance['parts']['base']['declaration']['file']=='drive_module_rotation.star' and
            provenance['parts']['clamp_bolt_envelope']['declaration']['file']=='drive_module_rotation.star' and
            provenance['parts']['nut_envelope']['declaration']['file']=='drive_module_rotation.star',
            'loaded module owns actual three mounting definitions')
    for label,text in [(bundle['source_name'],bundle['source']),*bundle['modules'].items()]:
        require(provenance['source_inventory'][label]['sha256']==hashlib.sha256(text.encode()).hexdigest(),
                f'{label} captured source inventory digest')
    seating=read(out/'nominal-seating-checks.json')
    clearance=read(out/'clearance-checks.json')
    require(seating['recipe_context_hash']==clearance['recipe_context_hash']==manifest['recipe_context_hash'] and
            seating['checks']==manifest['nominal_seating_checks'] and
            clearance['checks']==manifest['clearance_checks'],
            'published engineering reports equal current manifest')
    return {'bundle_sha256':digest(out/'recipe.bundle.json'),
            'source_provenance_sha256':digest(out/'source-provenance.json'),
            'seating_report_sha256':digest(out/'nominal-seating-checks.json'),
            'clearance_report_sha256':digest(out/'clearance-checks.json')}


def run_case(cad,root,tag,parameters):
    source=ROOT/'examples/drive_module_tightened.star'
    loaded=ROOT/'examples/drive_module_rotation.star'
    args=args_for(parameters)
    release_args=args_for({name:value for name,value in parameters.items()
                           if name not in ('mounting_endpoint','bolt_lift','nut_drop')})
    release_out=root/(tag+'-release');out=root/(tag+'-tightened')
    release=s.command(cad,'build',loaded,'--out-dir',release_out,*release_args)
    manifest=s.command(cad,'build',source,'--out-dir',out,*args)
    s.command(cad,'verify',out)
    publication=validate_publication(out,manifest,source,loaded)
    endpoint=parameters.get('mounting_endpoint',-1)
    length=parameters.get('base_slot_length',6)
    shift=endpoint*length/2
    previous=c.imported_leaves(release_out/'drive_module_release.step')
    current=c.imported_leaves(out/'drive_module_tightened.step')
    require(len(previous)==len(current)==30,'thirty exact STEP leaves in both releases')
    shapes={path:leaf['shape'] for path,leaf in current.items()}
    require(set(shapes)=={('fixed_base',)}|{('moving',)+path for path in previous if path!=('base',)
                and not(path[0].startswith('clamp_bolt_') or path[0].startswith('clamp_nut_'))}|{
                    ('moving','tight_bolt_'+str(i)) for i in range(4)}|{
                    ('moving','tight_nut_'+str(i)) for i in range(4)},
            'exact fixed base and twenty-nine upper instance paths')
    differences={}
    for path,leaf in current.items():
        original=release_path(path)
        require(original in previous,f'{path} maps to a released exact part')
        reference=s.move(previous[original]['shape'],[shift,0,0]) if path!=('fixed_base',) else previous[original]['shape']
        differences['/'.join(path)]=s.directed_differences(leaf['shape'],reference,str(path))
    pairs=pairwise_nonpenetration(shapes,tag)
    names={f'tight_bolt_{i}_head_seat' for i in range(4)}|{f'tight_nut_{i}_base_seat' for i in range(4)}
    seats={name:report for name,report in manifest['nominal_seating_checks'].items()
           if report['declaration']['output']=='drive_module_tightened'}
    require(set(seats)==names,'eight source-named tightened mounting seats')
    details={}
    for i in range(4):
        clamp=('moving','left_clamp' if i<2 else 'right_clamp')
        bolt=('moving',f'tight_bolt_{i}')
        nut=('moving',f'tight_nut_{i}')
        details[f'tight_bolt_{i}_head_seat']=seat_check(
            f'tight_bolt_{i}_head_seat',seats[f'tight_bolt_{i}_head_seat'],
            shapes,manifest,60,bolt,clamp)
        details[f'tight_nut_{i}_base_seat']=seat_check(
            f'tight_nut_{i}_base_seat',seats[f'tight_nut_{i}_base_seat'],
            shapes,manifest,0,nut,('fixed_base',))
    gaps={}
    for i in range(4):
        name=f'tight_slot_{i}_shank_gap';check=manifest['clearance_checks'][name]
        path=('moving',f'tight_bolt_{i}')
        require(check['status']=='passed' and check['declaration']['output']=='drive_module_tightened' and
                check['left']['instance_path']==['fixed_base'] and
                check['right']['instance_path']==list(path) and
                check['query']['method']=='slot_trim_witness',f'{name} native static slot proof')
        close(check['declaration']['min_clearance'],.49,f'{name} authored clearance')
        actual=c.distance(shapes[('fixed_base',)],shapes[path])
        close(actual,.5,f'{name} independent STEP surface separation')
        require(check['lower_bound_mm']>.49 and check['lower_bound_mm']<=actual+2e-6 and
                check['upper_bound_mm']+2e-6>=actual,
                f'{name} native interval encloses actual STEP distance')
        gaps[name]={'actual_step_distance_mm':actual,
                    'native_lower_bound_mm':check['lower_bound_mm'],
                    'native_upper_bound_mm':check['upper_bound_mm']}
    replay=root/(tag+'-replay')
    repeated=s.command(cad,'build',out/'recipe.bundle.json','--out-dir',replay,*args)
    for key in ('recipe_context_hash','source_bundle_hash','nominal_seating_checks','clearance_checks'):
        require(repeated[key]==manifest[key],f'{tag} captured offline replay {key}')
    require(repeated['outputs']['drive_module_tightened']['content_hash']==
            manifest['outputs']['drive_module_tightened']['content_hash'],
            f'{tag} captured replay geometry identity')
    for name in ('drive_module_tightened.step','nominal-seating-checks.json',
                 'clearance-checks.json','source-provenance.json','recipe.bundle.json'):
        require((out/name).read_bytes()==(replay/name).read_bytes(),
                f'{tag} captured replay exact {name}')
    return {'case':tag,'parameters':parameters,'endpoint':endpoint,
            'translation_x_mm':shift,'source_sha256':digest(source),
            'loaded_source_sha256':digest(loaded),
            'recipe_context_hash':manifest['recipe_context_hash'],
            'assembly_content_hash':manifest['outputs']['drive_module_tightened']['content_hash'],
            'published_occurrences':30,'pairwise_nonpenetration':pairs,
            'directed_step_differences_mm3':differences,
            'protected_seats':details,'static_slot_gaps':gaps,
            'captured_replay':publication}


def refused_revisions(cad,root):
    source=ROOT/'examples/drive_module_tightened.star'
    loaded=ROOT/'examples/drive_module_rotation.star'
    folder=root/'revisions';folder.mkdir()
    copied=folder/source.name
    copied.write_bytes(source.read_bytes())
    shutil.copy2(loaded,folder/loaded.name)
    lock=Path(str(source)+'.lock.json')
    if lock.exists():shutil.copy2(lock,folder/lock.name)
    else:s.command(cad,'lock',copied)
    out=root/'preserved';cache=root/'cache'
    s.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache)
    accepted=s.inventory(out)
    baseline=source.read_text()
    cases=[
        ('lifted_head',[],{'bolt_lift':.2}),
        ('dropped_nut',[],{'nut_drop':.2}),
        ('head_intrudes',[('-5+bolt_lift','-5-bolt_lift')],{'bolt_lift':.2}),
        ('insufficient_seat',[('min_shared_disk_radius_mm = 0.4','min_shared_disk_radius_mm = 2.0')],{}),
        ('insufficient_slot_gap',[('min_clearance = 0.49','min_clearance = 0.6')],{}),
    ]
    evidence=[]
    for label,edits,parameters in cases:
        changed=baseline
        for before,after in edits:
            require(changed.count(before)>=1,f'{label} unique mutation anchor present')
            changed=changed.replace(before,after)
        copied.write_text(changed)
        diagnostic=s.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache,
                             *args_for(parameters),success=False)
        require(s.inventory(out)==accepted,f'{label} preserves every accepted artifact byte')
        evidence.append({'case':label,'diagnostic':diagnostic,
                         'preserved_files':len(accepted)})
    s.command(cad,'verify',out)
    return evidence


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args()
    args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary_hash=digest(cad);script_hash=digest(__file__)
    sources=source_inventory()
    for dependency in ('tools/check-slot-adjustment.py','tools/check-drive-rotation.py',
                       'tools/check-retained-drive-module.py',
                       'tools/check-keyed-drive-module.py','tools/check-retained-shaft.py',
                       'tools/check-longitudinal-fillet.py',
                       'crates/cad-export/tools/check_assembly_ocp.py'):
        require(dependency in sources,f'pinned transitive OCCT helper {dependency}')
    cases=[run_case(cad,args.out_dir,name,parameters) for name,parameters in [
        ('near_endpoint',{}),
        ('far_endpoint',{'mounting_endpoint':1}),
        ('wide_near_endpoint',{'mounting_pitch':90,'base_slot_length':8}),
        ('wide_far_endpoint',{'mounting_pitch':90,'base_slot_length':8,'mounting_endpoint':1}),
    ]]
    refusals=refused_revisions(cad,args.out_dir)
    require(digest(cad)==binary_hash and digest(__file__)==script_hash,
            'pinned executable and oracle remained unchanged')
    require(source_inventory()==sources,
            'all original recipe, lock and transitive oracle helper bytes unchanged throughout run')
    report={'valid':True,'executable_sha256':binary_hash,
            'oracle_script_sha256':script_hash,
            'dependency_script_sha256':digest(Path(__file__).with_name('check-slot-adjustment.py')),
            'source_inventory_sha256':sources,
            'python':platform.python_version(),'ocp':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'refusals':refusals,
            'scope':'Eight local nominal face seats and four named static slot/shank gaps are independently checked against exact imported STEP at each slot endpoint. All 435 placed pairs have zero positive common volume at those endpoint poses. No preload, thread engagement, torque, load, motion or manufactured tolerance claim.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refusals':len(refusals),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':
    main()
