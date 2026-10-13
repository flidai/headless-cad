#!/usr/bin/env python3
"""Independent OCCT acceptance for source-owned obround bolt adjustment.

The native kernel certifies the complete translation. This test independently
constructs the solids, checks STEP topology and several path positions, and
checks that the published endpoint proof is tied to real STEP boundaries.
Sampling is supporting evidence, not the continuous proof.
"""

import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('adjustment_drive_oracle', Path(__file__).with_name('check-drive-rotation.py'))
d = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(d)
k = d.k
c = d.c
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common, BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder
from OCP.gp import gp_Ax2, gp_Dir, gp_Pnt, gp_Trsf, gp_Vec
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS


def require(value, label):
    if not value:
        raise AssertionError(label)


def close(actual, expected, label, absolute=2e-6):
    require(math.isfinite(actual) and abs(actual-expected) <= max(absolute, abs(expected)*2e-12),
            f'{label}: {actual} differs from {expected}')


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inventory(directory):
    return {p.relative_to(directory).as_posix(): digest(p)
            for p in directory.rglob('*') if p.is_file()}


def command(cad, *args, success=True):
    process = subprocess.run([str(cad), *map(str, args)], capture_output=True, text=True, timeout=240)
    require((process.returncode == 0) == success,
            f'cad {args[0]} exit={process.returncode}: {process.stderr[:1800]}')
    return json.loads(process.stdout if process.returncode == 0 else process.stderr)


def move(shape, vector):
    frame=gp_Trsf();frame.SetTranslation(gp_Vec(*vector))
    return BRepBuilderAPI_Transform(shape,frame,True).Shape()


def capsule_plate(width=50, depth=30, height=6, length=6, radius=3,
                  center=(25,15)):
    """Three independent OCCT cutters: box and full round end cylinders."""
    solid=BRepPrimAPI_MakeBox(width,depth,height).Shape()
    x,y=center;half=length/2
    cutter=BRepPrimAPI_MakeBox(gp_Pnt(x-half,y-radius,-1),length,2*radius,height+2).Shape()
    solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
    for cx in (x-half,x+half):
        cylinder=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(cx,y,-1),gp_Dir(0,0,1)),
                                          radius,height+2).Shape()
        solid=k.boolean(BRepAlgoAPI_Cut,solid,cylinder)
    return solid


def stepped_bolt(shank_radius=2.5, shank_length=8, head_radius=4.5,
                 head_height=2, origin=(25,15,-1)):
    """Independent OCCT union-free revolved reference for one stepped bolt."""
    body=d.d.retained.revolve([[0,0],[shank_radius,0],[shank_radius,shank_length],
                               [head_radius,shank_length],
                               [head_radius,shank_length+head_height],
                               [0,shank_length+head_height]])
    return d.d.placed(body,origin)


def directed_differences(actual, reference, label):
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
                 for a,b in ((actual,reference),(reference,actual))]
    for value in differences:close(value,0,label+' STEP directed difference',absolute=3e-6)
    return differences


def pair_volume(a,b):
    return k.properties(k.boolean(BRepAlgoAPI_Common,a,b))[0]


def surface_inventory(shape):
    types=[];radii=[];it=TopExp_Explorer(shape,TopAbs_FACE)
    while it.More():
        adaptor=BRepAdaptor_Surface(TopoDS.Face(it.Current()))
        if adaptor.GetType()==GeomAbs_Plane:types.append('plane')
        elif adaptor.GetType()==GeomAbs_Cylinder:
            types.append('cylinder');radii.append(adaptor.Cylinder().Radius())
        else:types.append(str(adaptor.GetType()))
        it.Next()
    require(set(types)=={'plane','cylinder'},'bounded analytic STEP surface inventory')
    return {'plane_faces':types.count('plane'),'cylinder_radii_mm':sorted(radii)}


def point_segment_distance(point, a, b):
    delta=[b[i]-a[i] for i in range(3)]
    length2=sum(x*x for x in delta)
    require(length2>0,'positive capsule centerline length')
    t=min(1,max(0,sum((point[i]-a[i])*delta[i] for i in range(3))/length2))
    return math.dist(point,[a[i]+t*delta[i] for i in range(3)])


def certified_endpoint_bound(start,end,a,b,slot_radius,bolt_radius):
    """Euclidean distance to a convex segment is convex along straight travel.

    The maximum distance on the closed travel segment therefore occurs at an
    endpoint. The kernel's own arithmetic guard is checked separately.
    """
    ds=[point_segment_distance(p,a,b) for p in (start,end)]
    return slot_radius-bolt_radius-max(ds),ds


def verify_source_binding(source,out,manifest,output,group_id,check_ids):
    require(digest(source)==digest(out/'recipe.star'),'captured source exact SHA256')
    require(manifest['outputs'][output]['instances']>=2,'published assembly leaf count')
    group=manifest['rigid_translation_groups'][group_id]
    require(group['status']=='named_pairs_passed' and group['coverage']=='named_pairs_only',
            'explicit named pair coverage')
    require(group['assembly_content_hash']==manifest['outputs'][output]['content_hash'],
            'current assembly content hash')
    require(set(group['clearance_check_ids'])==set(check_ids),'all source named pair checks')
    for name in check_ids:
        report=manifest['clearance_checks'][name]
        require(report['status']=='passed' and report['declaration']['output']==output,
                'passed source-owned translation check '+name)
        require(report['declaration']['translation_group']==group_id,
                'source-owned group reference '+name)
        require(report['group_translation_world']==group['translation_world'],
                'same actual group translation '+name)
    return group


def verify_replay(original,replay,expected_output):
    a=json.loads((original/'manifest.json').read_text())
    b=json.loads((replay/'manifest.json').read_text())
    for key in ('recipe_context_hash','kernel_build_id',
                'executor_build_id','rigid_translation_groups','clearance_checks'):
        require(a[key]==b[key],'captured offline replay semantic identity '+key)
    require(a.get('source_bundle_hash')==b.get('source_bundle_hash'),
            'captured offline replay optional bundle identity')
    require(a['outputs'][expected_output]['content_hash']==
            b['outputs'][expected_output]['content_hash'],
            'captured offline replay assembly content identity')
    required=['rigid-translation-groups.json','clearance-checks.json',expected_output+'.step']
    required.insert(0,'recipe.bundle.json' if (original/'recipe.bundle.json').exists()
                    else 'recipe.star')
    for name in required:
        require((original/name).exists() and (replay/name).exists(),
                'required replay evidence asset '+name)
        require((original/name).read_bytes()==(replay/name).read_bytes(),
                'captured offline replay exact artifact bytes '+name)
    translated=json.loads((original/'rigid-translation-groups.json').read_text())
    clearance=json.loads((original/'clearance-checks.json').read_text())
    require(translated['recipe_context_hash']==clearance['recipe_context_hash']==
            a['recipe_context_hash'],
            'published reports bind captured source context')
    require(translated['groups']==a['rigid_translation_groups'] and
            clearance['checks']==a['clearance_checks'],
            'published native reports equal current manifest witnesses')
    require(translated['clearance_report']['digest']==
            a['artifact_integrity']['files']['clearance-checks.json']['digest'],
            'translation publication binds exact staged clearance report')
    return {'replayed_exact_files':required,
            'recipe_context_hash':a['recipe_context_hash'],
            'clearance_report_digest':translated['clearance_report']['digest']}


def check_native_path(check,stationary,moving,travel,expected):
    require(check['motion_proof']=='slot_translation','native slot path proof')
    require(check['query']['method']=='slot_trim_witness',
            'published initial query uses strict native slot supports')
    sweep=check['sweep']
    require(sweep['query_count']==2 and sweep['intervals']==[] and
            [s['time'] for s in sweep['samples']]==[0,1],
            'native endpoint path proof with exactly two actual samples')
    proof=sweep['slot_motion_proof']
    require(proof['complete_path_lower_bound_mm']<=expected+1e-11,
            'source-bound native full-path lower bound')
    require(proof['arithmetic_guard_mm']>0 and
            proof['complete_path_lower_bound_mm']<=
            proof['capsule_radius_mm']-proof['shank_radius_mm']-
            max(proof['endpoint_centerline_distances_mm']),
            'guarded convex endpoint path formula')
    close(proof['complete_path_lower_bound_mm'],
          expected-proof['arithmetic_guard_mm'],
          'independent guarded endpoint formula',absolute=1e-11)
    require(len(proof['stock_slab_cap_faces'])==2 and
            len(set(map(str,proof['stock_slab_cap_faces'])))==2 and
            proof['worst_endpoint_index']==
            (0 if proof['endpoint_centerline_distances_mm'][0]>=
                  proof['endpoint_centerline_distances_mm'][1] else 1),
            'both native slab caps and actual worst endpoint')
    for actual,want in zip(proof['stock_slab_limits_mm'],[0,6]):
        close(actual,want,'native stock slab axial limit')
    require(check['lower_bound_mm']<=expected+1e-11 and
            check['upper_bound_mm']>=expected-1e-11,
            'native interval encloses independent path result')
    sampled=[]
    for sample in sweep['samples']:
        t=sample['time'];moved=move(moving,[t*x for x in travel])
        query=sample['query']
        require(query['method']=='slot_trim_witness',
                'actual native slot wall and shank endpoint query')
        for label,shape in [('left',stationary),('right',moved)]:
            require(c.witness_distance(query[label+'_point'],shape)<2e-6,
                    'reported native endpoint on exact STEP boundary '+label)
        actual=c.distance(stationary,moved)
        require(query['lower_bound_mm']-2e-6<=actual<=query['upper_bound_mm']+2e-6,
                'endpoint native interval encloses independent STEP distance')
        sampled.append({'time':t,'actual_step_distance_mm':actual,
                        'native_witness_distance_mm':math.dist(query['left_point'],query['right_point'])})
    return {'proof':proof,'endpoint_step_queries':sampled}


def sampled_pairs(shapes,moving_paths,travel,times):
    records=[]
    for t in times:
        posed={path:move(shape,[t*x for x in travel]) if path in moving_paths else shape
               for path,shape in shapes.items()}
        bounds={path:d.d.bounds(shape) for path,shape in posed.items()}
        paths=list(posed);maximum=0;count=0;contacts=0
        for i,left in enumerate(paths):
            for right in paths[i+1:]:
                count+=1
                if d.d.separated_boxes(bounds[left],bounds[right]):continue
                volume=pair_volume(posed[left],posed[right])
                maximum=max(maximum,volume)
                close(volume,0,f'sampled placed pair {left} {right} at {t}',absolute=3e-6)
                if c.distance(posed[left],posed[right])<2e-6:contacts+=1
        records.append({'time':t,'pairs':count,'maximum_common_volume_mm3':maximum,
                        'contact_pairs':contacts})
    return records


def shared_pose(shape,pose):
    return d.d.placed(shape,pose['offset'],pose['rotation']) if pose else shape


def pose_point(point,pose):
    if not pose:return point
    theta=math.pi/4;co=math.cos(theta);si=math.sin(theta)
    x,y,z=point;ox,oy,oz=pose['offset']
    return [ox+co*x-si*y,oy+si*x+co*y,oz+z]


def run_isolated(cad,root,tag,source,parameters,pose=None,reversed_operands=False,
                 shank_length=8):
    output=root/tag
    args=[]
    for key,value in parameters.items():args+=['--param',f'{key}={value}']
    manifest=command(cad,'build',source,'--out-dir',output,*args)
    command(cad,'verify',output)
    group=verify_source_binding(source,output,manifest,'slotted_clamp_adjustment',
                                'adjustment',['slot_shank_running_gap'])
    report=manifest['clearance_checks']['slot_shank_running_gap']
    L=parameters.get('slot_length',6);R=parameters.get('slot_radius',3)
    r=parameters.get('bolt_radius',2.5);travel=parameters.get('slide_length',6)
    expected_travel=[travel,0,0] if not pose else [travel*math.cos(math.pi/4),travel*math.sin(math.pi/4),0]
    for i,(actual,want) in enumerate(zip(group['translation_world'],expected_travel)):
        close(actual,want,'authored world translation '+str(i))
    leaves=c.imported_leaves(output/'slotted_clamp_adjustment.step')
    prefix=('cell',) if pose else ()
    require(set(leaves)=={prefix+('plate',),prefix+('bolt',)},'two exact STEP leaf paths')
    shapes={p:x['shape'] for p,x in leaves.items()}
    plate=shared_pose(capsule_plate(length=L,radius=R),pose)
    bolt=shared_pose(stepped_bolt(shank_radius=r,shank_length=shank_length,
                                  origin=(25-L/2,15,-1)),pose)
    plate_path=prefix+('plate',);bolt_path=prefix+('bolt',)
    diffs={'plate':directed_differences(shapes[plate_path],plate,'plate'),
           'bolt':directed_differences(shapes[bolt_path],bolt,'bolt')}
    analytical_volume=6*(50*30-(2*R*L+math.pi*R*R))
    close(k.properties(shapes[plate_path])[0],analytical_volume,'capsule plate volume')
    close(k.properties(shapes[bolt_path])[0],
          math.pi*(r*r*shank_length+4.5**2*2),'stepped bolt volume')
    surfaces={'plate':surface_inventory(shapes[plate_path]),
              'bolt':surface_inventory(shapes[bolt_path])}
    require(sum(abs(x-R)<2e-6 for x in surfaces['plate']['cylinder_radii_mm'])>=2,
            'actual exported capsule semicylinder supports')
    require(any(abs(x-r)<2e-6 for x in surfaces['bolt']['cylinder_radii_mm']),
            'actual exported shank wall')
    proof=report['sweep']['slot_motion_proof']
    require(all(math.isfinite(x) for x in proof['stock_slab_origin']),
            'finite reconstructed stock slab origin')
    close(proof['stock_slab_origin'][2],3 if pose else 0,
          'stock slab source support plane')
    for actual,want in zip(proof['stock_slab_axis'],[0,0,1]):
        close(actual,want,'stock slab analytic axis')
    center=pose_point([25-L/2,15,0],pose)
    end=[center[i]+expected_travel[i] for i in range(3)]
    a,b=[pose_point(x,pose) for x in ([25-L/2,15,0],[25+L/2,15,0])]
    expected,ds=certified_endpoint_bound(center,end,a,b,R,r)
    close(expected,R-r,'independent complete path capsule clearance')
    close(proof['capsule_radius_mm'],R,'native capsule radius')
    close(proof['shank_radius_mm'],r,'native shank radius')
    for actual,want in zip(proof['endpoint_centerline_distances_mm'],ds):
        close(actual,want,'native endpoint-to-segment distance')
    for actual,want in zip(proof['capsule_endpoints'],[a,b]):
        for a,b in zip(actual,want):close(a,b,'native reconstructed slot endpoint')
    require(len(proof['stock_slot_wall_faces'])==4 and
            len(set(map(str,proof['stock_slot_wall_faces'])))==4 and
            proof['other_section_axial_gap_mm']>=shank_length-7-1e-6,
            'four source slot faces and full head axial relief')
    require(report['moving_endpoint']==('left' if reversed_operands else 'right'),
            'source-selected moving endpoint')
    native=check_native_path(report,shapes[plate_path],shapes[bolt_path],
                             group['translation_world'],expected)
    phases=[]
    for t in [0,.125,.25,.375,.5,.625,.75,.875,1]:
        moved=move(shapes[bolt_path],[t*x for x in expected_travel]);actual=c.distance(shapes[plate_path],moved)
        close(actual,expected,'independent STEP complete study sampled distance')
        volume=pair_volume(shapes[plate_path],moved)
        close(volume,0,'independent STEP moving bolt/plate common volume',absolute=3e-6)
        phases.append({'time':t,'distance_mm':actual,'common_volume_mm3':volume})
    replay=root/(tag+'-replay')
    if (output/'recipe.bundle.json').exists():
        captured=output/'recipe.bundle.json'
    else:
        source_copy=root/(tag+'-captured')
        source_copy.mkdir()
        captured=source_copy/source.name
        captured.write_bytes((output/'recipe.star').read_bytes())
    command(cad,'build',captured,'--out-dir',replay,*args)
    replay_evidence=verify_replay(output,replay,'slotted_clamp_adjustment')
    return {'case':tag,'parameters':parameters,'shank_length_mm':shank_length,
            'source_sha256':digest(source),
            'recipe_context_hash':manifest['recipe_context_hash'],
            'assembly_content_hash':manifest['outputs']['slotted_clamp_adjustment']['content_hash'],
            'native_check':native,'independent_complete_path_minimum_mm':expected,
            'step_directed_differences_mm3':diffs,'surface_inventory':surfaces,
            'sampled_phases':phases,'captured_replay':replay_evidence}


def revised_source(root,label,original,edits):
    source=root/(label+'.star')
    for before,after in edits:
        require(original.count(before)==1,'unique source mutation anchor '+label+': '+before)
        original=original.replace(before,after,1)
    source.write_text(original)
    return source


def refusal_cases(cad,root,baseline_source):
    """Every rejected revision must preserve all bytes of the prior release."""
    source_text=baseline_source.read_text();source=root/'refusal.star'
    out=root/'preserved';cache=root/'cache'
    source.write_text(source_text)
    command(cad,'build',source,'--out-dir',out,'--cache-dir',cache)
    published=inventory(out)
    mutations=[
        ('endpoint_unsafe',[('slide_length > slot_length','slide_length > 100')],{'slide_length':7}),
        ('lateral_offset',[('25-slot_length/2,15,-1','25-slot_length/2,15.2,-1')],{}),
        ('tilted_shank',[('offset = [25-slot_length/2,15,-1]','offset = [25-slot_length/2,15,-1], rotation = [0.01,0,0,0.9999499987499375]')],{}),
        ('short_shank',[('[bolt_radius,8],[4.5,8],[4.5,10],[0,10]',
                         '[bolt_radius,6.5],[4.5,6.5],[4.5,8.5],[0,8.5]')],{}),
        ('head_enters_stock',[('[bolt_radius,8],[4.5,8],[4.5,10],[0,10]',
                              '[bolt_radius,6.8],[4.5,6.8],[4.5,8.8],[0,8.8]')],{}),
        ('excessive_minimum',[('min_clearance = 0.49','min_clearance = 0.6')],{}),
        ('bad_reference',[('reference_instance_path = ["bolt"]','reference_instance_path = ["missing"]')],{}),
        ('narrow_radial_band',[],{'bolt_radius':2.6}),
        ('group_budget',[('output(name = "adjustment_bolt", feature = bolt_body)',
            'output(name = "adjustment_bolt", feature = bolt_body)\n'
            'def over_budget():\n'
            '    for i in range(17):\n'
            '        rigid_translation_group(id = "extra_"+str(i), '
            'output = "slotted_clamp_adjustment", instance_paths = [["bolt"]], '
            'reference_instance_path = ["bolt"], translation = [1,0,0])\n'
            'over_budget()')],{}),
    ]
    evidence=[]
    for label,edits,parameters in mutations:
        mutated=source_text
        for anchor,replacement in edits:
            require(mutated.count(anchor)==1,'unique refusal mutation '+label)
            mutated=mutated.replace(anchor,replacement,1)
        source.write_text(mutated)
        extra=[]
        for key,value in parameters.items():extra+=['--param',f'{key}={value}']
        diagnostic=command(cad,'build',source,'--out-dir',out,'--cache-dir',cache,*extra,success=False)
        current=inventory(out)
        changed=[name for name in sorted(set(published)|set(current))
                 if published.get(name)!=current.get(name)]
        require(not changed,'refusal preserves all publication bytes '+label+': '+str(changed))
        evidence.append({'case':label,'diagnostic':diagnostic,'preserved_files':len(published)})
    command(cad,'verify',out)
    return evidence


def old_drive_path(path):
    if path==('adjustment_base',):return ('base',),[0,0,0]
    require(path[0]=='moving','all upper occurrences under one named parent')
    rest=path[1:]
    if len(rest)==1 and rest[0].startswith('adjustment_bolt_'):
        return ('clamp_bolt_'+rest[0].rsplit('_',1)[1],),[0,0,1]
    if len(rest)==1 and rest[0].startswith('adjustment_nut_'):
        return ('clamp_nut_'+rest[0].rsplit('_',1)[1],),[0,0,-1]
    return rest,[0,0,0]


def run_drive(cad,root,tag,parameters):
    """Reference every adjusted occurrence to the independently tested release."""
    original_source=ROOT/'examples/drive_module_rotation.star'
    adjustment_source=ROOT/'examples/drive_module_adjustment.star'
    original_out=root/(tag+'-original');adjusted_out=root/(tag+'-adjusted')
    args=[]
    for name,value in parameters.items():args+=['--param',f'{name}={value}']
    original=command(cad,'build',original_source,'--out-dir',original_out,*args)
    adjusted=command(cad,'build',adjustment_source,'--out-dir',adjusted_out,*args)
    command(cad,'verify',adjusted_out)
    require(digest(adjustment_source)==digest(adjusted_out/'recipe.star'),
            'captured adjustment root source exact bytes')
    require((adjusted_out/'recipe.bundle.json').exists(),
            'loaded original drive captured in durable source bundle')
    bundle=json.loads((adjusted_out/'recipe.bundle.json').read_text())
    require(adjusted['source_bundle_hash']!=original.get('source_bundle_hash') and
            adjusted['recipe_context_hash']!=original['recipe_context_hash'],
            'distinct adjustment source and context revision')
    length=parameters.get('base_slot_length',6);pitch=parameters.get('mounting_pitch',60)
    ids=['mounting_slot_'+str(i)+'_shank_gap' for i in range(4)]
    group=verify_source_binding(adjustment_source,adjusted_out,adjusted,
                                'drive_module_adjustment','upper_slot_travel',ids)
    require(len(group['members'])==29,'all twenty-nine upper leaves in source group')
    moving={tuple(w['instance_path']) for w in group['members']}
    require(group['reference']['instance_path']==['moving','left_clamp'],
            'exact authored upper reference occurrence')
    require(len(moving)==29 and ('adjustment_base',) not in moving,
            'fixed base excluded from path motion')
    for actual,want in zip(group['translation_world'],[length,0,0]):
        close(actual,want,'drive group world translation')
    previous=c.imported_leaves(original_out/'drive_module_release.step')
    current=c.imported_leaves(adjusted_out/'drive_module_adjustment.step')
    require(len(previous)==len(current)==30,'both releases have thirty exact STEP leaves')
    require(set(current)==moving|{('adjustment_base',)},'complete study occurrence inventory')
    shapes={path:leaf['shape'] for path,leaf in current.items()}
    differences={}
    for path,leaf in current.items():
        old_path,delta=old_drive_path(path)
        require(old_path in previous,'mapped original drive occurrence '+str(path))
        shift=delta if path==('adjustment_base',) else [-length/2+delta[0],delta[1],delta[2]]
        reference=move(previous[old_path]['shape'],shift) if shift!=[0,0,0] else previous[old_path]['shape']
        diffs=directed_differences(leaf['shape'],reference,'adjusted occurrence '+str(path))
        differences['/'.join(path)]=diffs
    bolt_checks={}
    centers=[(x,y) for x in [33,pitch+33] for y in [-28,28]]
    for i,(x,y) in enumerate(centers):
        name=ids[i];check=adjusted['clearance_checks'][name]
        require(check['moving_endpoint']=='right' and
                check['left']['instance_path']==['adjustment_base'] and
                check['right']['instance_path']==['moving','adjustment_bolt_'+str(i)],
                'exact fixed base/moving bolt occurrence '+name)
        proof=check['sweep']['slot_motion_proof']
        require(all(math.isfinite(x) for x in proof['stock_slab_origin']),
                'finite drive base native slab origin '+name)
        close(proof['stock_slab_origin'][2],0,'drive base lower support plane '+name)
        for actual,want in zip(proof['stock_slab_axis'],[0,0,1]):
            close(actual,want,'drive base slab axis '+name)
        require(proof['other_section_axial_gap_mm']>=1.0-1e-6,
                'stepped head relieved above base '+name)
        start=[x-length/2,y,0];end=[x+length/2,y,0]
        bound,ds=certified_endpoint_bound(start,end,start,end,3.,2.5)
        close(bound,.5,'independent full drive bolt path lower bound')
        for actual,want in zip(proof['capsule_endpoints'],[start,end]):
            for a,b in zip(actual,want):close(a,b,'actual mounting capsule endpoint '+name)
        for actual,want in zip(proof['endpoint_centerline_distances_mm'],ds):
            close(actual,want,'independent drive endpoint centerline distance '+name)
        close(proof['capsule_radius_mm'],3,'native drive slot radius '+name)
        close(proof['shank_radius_mm'],2.5,'native drive bolt shank radius '+name)
        bolt_checks[name]=check_native_path(check,shapes[('adjustment_base',)],
                                            shapes[('moving','adjustment_bolt_'+str(i))],
                                            group['translation_world'],bound)
    phases=sampled_pairs(shapes,moving,group['translation_world'],
                         [0,.125,.25,.375,.5,.625,.75,.875,1])
    require(all(p['pairs']==435 for p in phases),'all 435 pairs at each study phase')
    replay=root/(tag+'-replay')
    command(cad,'build',adjusted_out/'recipe.bundle.json','--out-dir',replay,*args)
    replay_evidence=verify_replay(adjusted_out,replay,'drive_module_adjustment')
    return {'case':tag,'parameters':parameters,
            'source_sha256':digest(adjustment_source),'loaded_release_source_sha256':digest(original_source),
            'captured_bundle_sha256':digest(adjusted_out/'recipe.bundle.json'),
            'source_bundle_hash':adjusted['source_bundle_hash'],
            'recipe_context_hash':adjusted['recipe_context_hash'],
            'assembly_content_hash':adjusted['outputs']['drive_module_adjustment']['content_hash'],
            'published_occurrences':30,'moving_occurrences':29,'fixed_occurrences':1,
            'directed_step_differences_mm3':differences,
            'bolt_path_certificates':bolt_checks,'sampled_all_pair_phases':phases,
            'bundle_replay':replay_evidence,
            'captured_bundle_module_count':len(bundle.get('modules',{}))}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary_hash=digest(cad);script_hash=digest(__file__)
    source=ROOT/'examples/slotted_clamp_adjustment.star'
    original=source.read_text()
    tall_bolt_source=revised_source(args.out_dir,'tall_bolt',original,[
        ('[bolt_radius,8],[4.5,8],[4.5,10],[0,10]',
         '[bolt_radius,9],[4.5,9],[4.5,11],[0,11]')])
    cases=[run_isolated(cad,args.out_dir,'nominal',source,{}),
           run_isolated(cad,args.out_dir,'wider_slot',tall_bolt_source,
                        {'slot_length':8,'slot_radius':3.5,'bolt_radius':2.5,'slide_length':8},
                        shank_length=9),
           run_isolated(cad,args.out_dir,'narrow_bolt',tall_bolt_source,
                        {'slot_length':6,'slot_radius':3,'bolt_radius':2,'slide_length':4},
                        shank_length=9)]
    reversed_source=revised_source(args.out_dir,'reversed_operands',original,[
        ('left_instance_path = ["plate"], right_instance_path = ["bolt"]',
         'left_instance_path = ["bolt"], right_instance_path = ["plate"]')])
    cases.append(run_isolated(cad,args.out_dir,'reversed_operands_case',
                              reversed_source,{},reversed_operands=True))
    q=[0,0,math.sin(math.pi/8),math.cos(math.pi/8)]
    prefix='posed_fixture = assembly(id = "posed_fixture", instances = [assembly_instance(\n' \
           f'    id = "cell", assembly = fixture, offset = [10,-5,3], rotation = {q})])\n'
    oblique_source=revised_source(args.out_dir,'oblique',original,[
        ('assembly_output(name = "slotted_clamp_adjustment", assembly = fixture)',
         prefix+'assembly_output(name = "slotted_clamp_adjustment", assembly = posed_fixture)'),
        ('instance_paths = [["bolt"]], reference_instance_path = ["bolt"]',
         'instance_paths = [["cell","bolt"]], reference_instance_path = ["cell","bolt"]'),
        ('left_instance_path = ["plate"], right_instance_path = ["bolt"]',
         'left_instance_path = ["cell","plate"], right_instance_path = ["cell","bolt"]')])
    cases.append(run_isolated(cad,args.out_dir,'oblique_case',oblique_source,{},
                              pose={'offset':[10,-5,3],'rotation':q}))
    rejections=refusal_cases(cad,args.out_dir,source)
    drive_cases=[run_drive(cad,args.out_dir,tag,parameters) for tag,parameters in [
        ('drive_default',{}),('drive_long_pitch',{'mounting_pitch':90,'base_slot_length':8})]]
    require(digest(cad)==binary_hash,'oracle pinned executable identity')
    require(digest(__file__)==script_hash,'oracle pinned script identity')
    report={'valid':True,'executable_sha256':binary_hash,'oracle_script_sha256':script_hash,
            'cases':cases,'drive_cases':drive_cases,'rejections':rejections,
            'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'scope':'Native continuous certificates cover the named bolt shank/slot pairs only: a canonical through capsule, parallel stepped shank, axially relieved head, and complete planar rigid translation by convex endpoint distance. Independent OCCT confirms exact STEP solids, path endpoints and nine sampled phases of every drive pair; sampled all-pair nonintersection does not certify unsampled positions, retention, preload, installation or load.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'drive_cases':len(drive_cases),
                      'rejections':len(rejections),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':
    main()
