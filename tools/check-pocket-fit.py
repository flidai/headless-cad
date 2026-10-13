#!/usr/bin/env python3
"""Independent OCCT acceptance for native finite capsule-pocket side fit.

Native fit certifies a finite capsule against four actual pocket walls. This
checker measures the exported trimmed STEP walls and whole inserted solids
separately, including a shifted insert that still seats on its pocket floor.
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

ROOT=Path(__file__).resolve().parents[1]
SOURCE=ROOT/'examples/seated_pocket_inserts.star'
SPEC=importlib.util.spec_from_file_location('insert_oracle',ROOT/'tools/check-pocketed-inserts.py')
prior=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(prior)
k=prior.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common,BRepAlgoAPI_Cut
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeVertex
from OCP.BRepExtrema import BRepExtrema_DistShapeShape
from OCP.GeomAbs import GeomAbs_Cylinder,GeomAbs_Plane
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.gp import gp_Pnt


def require(ok,reason):
    if not ok:raise AssertionError(reason)


def close(actual,expected,reason,absolute=2e-6):
    prior.close(actual,expected,reason,absolute)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [SOURCE,ROOT/'examples/mixed_slot_fixture.star',
              ROOT/'examples/seated_pocket_inserts.star.lock.json']
    require(all(path.is_file() for path in paths),'all recipe and helper dependencies exist')
    return {path.relative_to(ROOT).as_posix():sha(path) for path in sorted(paths)}


def face_catalog(shape):
    explorer=TopExp_Explorer(shape,TopAbs_FACE);result=[]
    while explorer.More():
        face=TopoDS.Face(explorer.Current());kind=BRepAdaptor_Surface(face).GetType()
        result.append({'face':face,'kind':kind,'bounds':prior.t.d.d.bounds(face)})
        explorer.Next()
    return result


def wall_faces(plate,slot,side):
    """Select four finite physical STEP walls without trusting native IDs."""
    x,y=slot['center'];length=slot['centerline_length'];radius=slot['radius']
    depth=slot['depth'];floor=12-depth if side==1 else depth
    zlow,zhigh=(floor,12) if side==1 else (0,floor)
    xy=(x-radius,y-length/2-radius,x+radius,y+length/2+radius)
    selected=[]
    for candidate in face_catalog(plate):
        (lo,hi)=candidate['bounds']
        if candidate['kind'] not in (GeomAbs_Plane,GeomAbs_Cylinder):continue
        if abs(lo[2]-zlow)>2e-5 or abs(hi[2]-zhigh)>2e-5:continue
        if lo[0]<xy[0]-2e-5 or lo[1]<xy[1]-2e-5 or hi[0]>xy[2]+2e-5 or hi[1]>xy[3]+2e-5:continue
        selected.append(candidate)
    require(len(selected)==4 and sum(face['kind']==GeomAbs_Plane for face in selected)==2
            and sum(face['kind']==GeomAbs_Cylinder for face in selected)==2,
            'exact two planar and two semicylindrical finite STEP pocket walls')
    return selected


def distance(left,right,label):
    measure=BRepExtrema_DistShapeShape(left,right);measure.Perform()
    require(measure.IsDone() and measure.NbSolution()>0,label+' OCCT extrema solved')
    value=measure.Value()
    require(math.isfinite(value) and value>=0,label+' finite nonnegative distance')
    return value


def witness_on_faces(point,faces,label):
    require(len(point)==3 and all(math.isfinite(v) for v in point),label+' finite point')
    vertex=BRepBuilderAPI_MakeVertex(gp_Pnt(*point)).Vertex()
    nearest=min(distance(vertex,face['face'],label+' to trimmed face') for face in faces)
    close(nearest,0,label+' belongs to actual STEP trimmed support',3e-6)
    return nearest


def insert_lateral_faces(insert,floor,side):
    zlow,zhigh=(floor,14) if side==1 else (-2,floor)
    return [candidate for candidate in face_catalog(insert)
            if candidate['kind'] in (GeomAbs_Plane,GeomAbs_Cylinder)
            and abs(candidate['bounds'][0][2]-zlow)<2e-5
            and abs(candidate['bounds'][1][2]-zhigh)<2e-5]


def world_point(matrix,point):
    return [sum(matrix[i][j]*point[j] for j in range(3))+matrix[i][3]
            for i in range(3)]


def world_axis(matrix,axis):
    return [sum(matrix[i][j]*axis[j] for j in range(3)) for i in range(3)]


def scalar(left,right):
    return sum(a*b for a,b in zip(left,right))


def cross(left,right):
    return [left[1]*right[2]-left[2]*right[1],
            left[2]*right[0]-left[0]*right[2],
            left[0]*right[1]-left[1]*right[0]]


def support_matches(raw,candidate,matrix):
    """Match a native FaceId to one STEP face by transformed analytic support."""
    geometry=raw['surface']['geometry'];kind=raw['surface']['kind']
    adapter=BRepAdaptor_Surface(candidate['face'])
    if kind=='plane' and candidate['kind']==GeomAbs_Plane:
        plane=adapter.Plane();direction=plane.Axis().Direction()
        actual=[direction.X(),direction.Y(),direction.Z()]
        normal=world_axis(matrix,geometry['normal'])
        origin=world_point(matrix,geometry['origin']);location=plane.Location()
        delta=[origin[i]-v for i,v in enumerate((location.X(),location.Y(),location.Z()))]
        return abs(abs(scalar(normal,actual))-1)<1e-8 and abs(scalar(delta,actual))<2e-6
    if kind=='cylinder' and candidate['kind']==GeomAbs_Cylinder:
        cylinder=adapter.Cylinder();axis=cylinder.Axis();direction=axis.Direction()
        actual=[direction.X(),direction.Y(),direction.Z()]
        native=world_axis(matrix,geometry['axis'])
        origin=world_point(matrix,geometry['origin']);location=axis.Location()
        delta=[origin[i]-v for i,v in enumerate((location.X(),location.Y(),location.Z()))]
        return abs(cylinder.Radius()-geometry['radius'])<2e-6 and \
            abs(abs(scalar(native,actual))-1)<1e-8 and \
            math.hypot(*cross(delta,actual))<2e-6
    return False


def bind_native_face_ids(output,part_name,face_ids,candidates,matrix):
    model=json.loads((output/(part_name+'.brep.json')).read_text())['model']
    mappings={}
    for face_id in face_ids:
        require(isinstance(face_id,int) and 0<=face_id<len(model['faces']),
                part_name+' native FaceId exists in exact exported raw model')
        raw=model['faces'][face_id]
        matching=[i for i,step in enumerate(candidates)
                  if support_matches(raw,step,matrix)]
        require(len(matching)==1,
                part_name+f' FaceId {face_id} matches one physical trimmed STEP wall')
        mappings[face_id]=candidates[matching[0]]
    require(len({id(face['face']) for face in mappings.values()})==len(face_ids),
            part_name+' native FaceIds bind distinct actual STEP faces')
    return mappings


def exact_references(width,first_depth,second_depth,side,clearance,shift):
    plate,inserts,through,blind=prior.references(width,first_depth,second_depth,side,clearance)
    first=blind[0];floor=12-first_depth if side==1 else first_depth
    z0=floor if side==1 else -2
    inserts['first_insert']=prior.capsule_solid(
        [first['center'][0]+shift,first['center'][1]],first['centerline_length'],
        first['radius']-clearance,z0,first_depth+2)
    return plate,inserts,through,blind


def args_for(width=120,first_depth=3,second_depth=7,side=1,
             clearance=.2,shift=0,required=.04):
    values=[('plate_width',width),('first_depth',first_depth),
            ('second_depth',second_depth),('pocket_side',side),
            ('insert_clearance',clearance),('first_insert_shift',shift),
            ('required_side_clearance',required)]
    return [word for key,value in values for word in ('--param',f'{key}={value}')]


def check_case(cad,root,tag,width=120,first_depth=3,second_depth=7,
               side=1,clearance=.2,shift=0,required=.04):
    arguments=args_for(width,first_depth,second_depth,side,clearance,shift,required)
    out=root/tag;manifest=prior.m.o.command(cad,'build',SOURCE,'--out-dir',out,*arguments)
    prior.m.o.command(cad,'verify',out)
    plate_ref,insert_refs,_,slots=exact_references(
        width,first_depth,second_depth,side,clearance,shift)
    leaves=prior.t.c.imported_leaves(out/'seated_pocket_inserts.step')
    require(set(leaves)=={('fixture',),('first_insert',),('second_insert',)},
            'exact three placed STEP leaf paths')
    shapes={name:leaves[(name,)]['shape'] for name in
            ('fixture','first_insert','second_insert')}
    for name,reference in [('fixture',plate_ref),*insert_refs.items()]:
        prior.directed_difference(shapes[name],reference,tag+' '+name)
        prior.m.b.compare_moments(k.properties(shapes[name]),k.properties(reference),
                                  tag+' '+name+' complete physical STEP moments')
        if (out/(name+'.stl')).is_file():
            k.mesh(out/(name+'.stl'),k.properties(reference)[0])
    k.mesh(out/'fixture.stl',k.properties(plate_ref)[0])
    result={}
    for name,slot in zip(('first_insert','second_insert'),slots):
        insert=shapes[name];floor=12-slot['depth'] if side==1 else slot['depth']
        walls=wall_faces(shapes['fixture'],slot,side)
        physical=min(distance(insert,wall['face'],name+' actual STEP wall')
                     for wall in walls)
        margin=clearance-abs(shift) if name=='first_insert' else clearance
        close(physical,margin,name+' physical finite capsule side gap',3e-6)
        require(physical+3e-6>=required,name+' independent required side gap')
        require(len(insert_lateral_faces(insert,floor,side))==4,
                name+' four actual insert lateral STEP faces')
        reference_z=(floor,14) if side==1 else (-2,floor)
        actual_z=prior.t.d.d.bounds(insert)
        close(actual_z[0][2],reference_z[0],name+' does not extend below floor')
        close(actual_z[1][2],reference_z[1],name+' authored protrusion')
        close(k.properties(k.boolean(BRepAlgoAPI_Common,shapes['fixture'],insert))[0],0,
              name+' whole-solid nonpenetration',3e-6)
        seat=manifest['nominal_seating_checks'][name+'_floor_seat']
        prior.assert_floor_disk(seat,shapes['fixture'],insert,floor,
            tag+' '+name,manifest['feature_hashes']['relief_pocket'],
            manifest['feature_hashes'][name+'_body'],
            'slot_locating_floor' if name=='first_insert' else 'slot_relief_floor')
        dimension=manifest['datum_dimensions'][name+'_proud_height']
        require(dimension['status']=='passed',name+' native height passes')
        close(dimension['value_mm'],2,name+' native proud height')
        fit=manifest['pocket_fit_checks'][name+'_pocket_fit']
        require(fit['status']=='passed',name+' source-owned native fit passes')
        query=fit['query']
        require(fit['assembly_content_hash']==
                manifest['outputs']['seated_pocket_inserts']['content_hash'],
                name+' fit binds current exact assembly')
        require(fit['pocket']['part_id']=='mixed_fixture_part' and
                fit['pocket']['part_content_hash']==
                manifest['feature_hashes']['relief_pocket'] and
                fit['insert']['part_id']==name and
                fit['insert']['part_content_hash']==
                manifest['feature_hashes'][name+'_body'],
                name+' fit binds current physical source parts')
        require(fit['pocket']['semantic_datum']['selection']['status']=='resolved' and
                fit['insert']['semantic_datum']['selection']['status']=='resolved',
                name+' source-owned pocket and insert face datums resolved')
        require(fit['pocket']['semantic_datum']['selection']['witnesses'][0]
                ['target_face']==query['pocket_floor_face'] and
                fit['insert']['semantic_datum']['selection']['witnesses'][0]
                ['target_face']==query['insert_floor_face'],
                name+' fit floor witness IDs belong to exact resolved source datums')
        require(len(query['pocket_wall_faces'])==4 and
                len(set(query['pocket_wall_faces']))==4,
                name+' four distinct source-bound native wall witnesses')
        require(query['pocket_witness_face'] in query['pocket_wall_faces'],
                name+' native witness belongs to selected physical pocket')
        pocket_binding=bind_native_face_ids(out,'fixture',query['pocket_wall_faces'],
            walls,leaves[('fixture',)]['matrix'])
        chosen=pocket_binding[query['pocket_witness_face']]
        require(chosen['kind']==GeomAbs_Plane,
                name+' selected wall is actual straight side rather than adjacent cap seam')
        witness_x=(chosen['bounds'][0][0]+chosen['bounds'][1][0])/2
        expected_side=(math.copysign(1,shift)
                       if name=='first_insert' and abs(shift)>1e-8
                       else (-1 if side==1 else 1))
        close(witness_x,slot['center'][0]+expected_side*slot['radius'],
              name+' witness FaceId binds correct physical STEP side plane')
        witness_on_faces(query['pocket_point'],
            [chosen],
            name+' native pocket point on selected FaceId support')
        insert_binding=bind_native_face_ids(out,name,[query['insert_witness_face']],
            insert_lateral_faces(insert,floor,side),leaves[(name,)]['matrix'])
        witness_on_faces(query['insert_point'],
            [insert_binding[query['insert_witness_face']]],
            name+' native insert point on selected FaceId support')
        close(math.dist(query['pocket_point'],query['insert_point']),physical,
              name+' native finite wall-point pair agrees with independent STEP extrema',3e-6)
        require(fit['side_clearance_lower_mm']<=physical+3e-6 and
                physical<=fit['side_clearance_upper_mm']+3e-6 and
                query['side_clearance_lower_mm']<=physical+3e-6 and
                physical<=query['side_clearance_upper_mm']+3e-6,
                name+' guarded native fit interval contains independent STEP distance')
        require(fit['side_clearance_lower_mm']>=required,
                name+' native bound supports authored minimum')
        require(fit['axial_overlap_lower_mm']>0 and
                fit['axial_overlap_lower_mm']<=slot['depth']+3e-6 and
                fit['axial_overlap_upper_mm']>=slot['depth']-3e-6,
                name+' guarded finite pocket axial overlap')
        require(fit['signed_floor_gap_lower_mm']<=0<=
                fit['signed_floor_gap_upper_mm'],
                name+' guarded floor alignment contains zero')
        result[name]={'physical_wall_distance_mm':physical,
                      'expected_capsule_margin_mm':margin,
                      'native_side_clearance_interval_mm':[
                       fit['side_clearance_lower_mm'],
                       fit['side_clearance_upper_mm']],
                      'wall_face_count':len(walls),
                      'native_wall_faces':query['pocket_wall_faces'],
                      'axial_overlap_lower_mm':query['axial_overlap_lower_mm']}
    close(k.properties(k.boolean(BRepAlgoAPI_Common,shapes['first_insert'],
                                      shapes['second_insert']))[0],0,
          'two complete inserts remain disjoint')
    first=manifest['face_assertions']['first_floor_survives']['witnesses'][0]
    require(first['anchor_faces'][0]!=first['target_face'],
            'serial first floor still has exact remapped ancestry')
    for id in ('front_lower','front_upper','rear_lower','rear_upper'):
        wall=manifest['face_assertions'][id+'_walls_survive']
        require(wall['status']=='resolved' and len(wall['witnesses'])==4 and
                all(w['anchor_faces']==[w['target_face']] for w in wall['witnesses']),
                'actual through walls retain original lineage')
    captured=root/(tag+'-replay')
    repeated=prior.m.o.command(cad,'build',out/'recipe.bundle.json',
                               '--out-dir',captured,*arguments)
    require(manifest['pocket_fit_checks']==repeated['pocket_fit_checks'] and
            manifest['nominal_seating_checks']==repeated['nominal_seating_checks'] and
            manifest['feature_hashes']==repeated['feature_hashes'],
            'captured recipe replays exact native fit, seat and source features')
    for artifact in ('seated_pocket_inserts.step','pocket-fit-checks.json',
                     'nominal-seating-checks.json','recipe.bundle.json'):
        require((out/artifact).read_bytes()==(captured/artifact).read_bytes(),
                tag+' exact captured artifact replay '+artifact)
    return {'case':tag,'parameters':{'plate_width':width,'first_depth':first_depth,
            'second_depth':second_depth,'pocket_side':side,'insert_clearance':clearance,
            'first_insert_shift':shift,'required_side_clearance':required},
            'assembly_content_hash':manifest['outputs']['seated_pocket_inserts']['content_hash'],
            'fit':result,'first_floor_remap':first}


def copied_source(folder,source):
    folder.mkdir(parents=True,exist_ok=True)
    target=folder/SOURCE.name;target.write_text(source)
    shutil.copyfile(ROOT/'examples/mixed_slot_fixture.star',
                    folder/'mixed_slot_fixture.star')
    shutil.copyfile(ROOT/'examples/seated_pocket_inserts.star.lock.json',
                    folder/'seated_pocket_inserts.star.lock.json')
    return target


def refusal_cases(cad,root):
    source=SOURCE.read_text();folder=root/'revisions'
    copied=copied_source(folder,source);out=root/'preserved';cache=root/'cache'
    baseline=prior.m.o.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache)
    require(all(baseline['pocket_fit_checks'][name+'_pocket_fit']['status']=='passed'
                for name in ('first_insert','second_insert')),
            'published baseline has both physical pocket fits')
    inventory=prior.m.o.inventory(out);failures=[]
    variants=[
        ('just_below_requested_margin',source,['--param','first_insert_shift=0.161']),
        ('opposite_boundary',source,['--param','first_insert_shift=-0.161']),
        ('positive_material_collision',source,['--param','first_insert_shift=0.3']),
        ('higher_required_clearance',source,['--param','required_side_clearance=0.21']),
        ('wrong_pocket_floor',source.replace('pocket_datum = floor',
                                             'pocket_datum = "opening"',1),[]),
        ('wrong_insert_cap',source.replace('insert_datum = "seat"',
                                           'insert_datum = "top"',1),[]),
    ]
    for label,changed,args in variants:
        require(changed!=source or bool(args),label+' source or parameter revision exists')
        copied.write_text(changed)
        failed=prior.m.o.command(cad,'build',copied,'--out-dir',out,
                                 '--cache-dir',cache,*args,success=False)
        require(failed['code']=='pocket_fit_check_failed',
                label+' must be refused by native fit rather than unrelated source obligation')
        require(prior.m.o.inventory(out)==inventory,
                label+' failed revision preserves every previously published artifact byte')
        failures.append({'case':label,'diagnostic':failed})
    copied.write_text(source);prior.m.o.command(cad,'verify',out)

    # Remove only the new fit declaration to prove a shifted insert still
    # passes floor seating and proud-height obligations while it intersects
    # the plate. Production must reject that revision through pocket fit.
    fit_block=('    pocket_fit_check(id = name+"_pocket_fit", output = "seated_pocket_inserts",\n'
               '        pocket_instance_path = ["fixture"], pocket_datum = floor,\n'
               '        insert_instance_path = [name], insert_datum = "seat",\n'
               '        min_side_clearance_mm = required_side_clearance)\n')
    require(source.count(fit_block)==1,'isolated source revision removes only pocket fit')
    isolated=copied_source(root/'without_fit',source.replace(fit_block,''))
    isolated_out=root/'without_fit_built'
    without=prior.m.o.command(cad,'build',isolated,'--out-dir',isolated_out,
                              '--param','first_insert_shift=0.3')
    prior.m.o.command(cad,'verify',isolated_out)
    require(not without['pocket_fit_checks'] and
            all(without['nominal_seating_checks'][name+'_floor_seat']['status']=='passed'
                and abs(without['datum_dimensions'][name+'_proud_height']['value_mm']-2)<1e-7
                for name in ('first_insert','second_insert')),
            'collision revision independently retains passing floor seats and native heights')
    leaves=prior.t.c.imported_leaves(isolated_out/'seated_pocket_inserts.step')
    plate=leaves[('fixture',)]['shape'];first=leaves[('first_insert',)]['shape']
    collision=k.properties(k.boolean(BRepAlgoAPI_Common,plate,first))[0]
    require(collision>1,'rejected shift has positive actual STEP plate/insert collision volume')
    reference_plate,reference_inserts,_,_=exact_references(120,3,7,1,.2,.3)
    independent_collision=k.properties(k.boolean(BRepAlgoAPI_Common,reference_plate,
                                                   reference_inserts['first_insert']))[0]
    close(collision,independent_collision,'isolated physical sidewall collision',3e-5)
    return failures,{'shift_mm':.3,'floor_seats_passed':2,
                     'native_proud_heights_mm':[2,2],
                     'actual_step_collision_volume_mm3':collision}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary=sha(cad);script=sha(__file__);inputs=pinned_inputs()
    cases=[check_case(cad,args.out_dir,*case) for case in [
        ('top_nominal',120,3,7,1,.2,0,.04),
        ('top_shifted',120,3,7,1,.2,.1,.04),
        ('top_near_boundary',120,3,7,1,.2,.159,.04),
        ('bottom_shifted',120,3,7,0,.2,.1,.04),
        ('narrow_deep',96,2,9,1,.2,.1,.04),
        ('wide_shallow',144,5,6,1,.2,.1,.04),
    ]]
    failures,isolated=refusal_cases(cad,args.out_dir)
    require(binary==sha(cad) and script==sha(__file__) and inputs==pinned_inputs(),
            'binary, oracle, source and imported helpers remained frozen')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'source_inventory_sha256':inputs,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'refusals':failures,'isolated_collision':isolated,
            'scope':'Native source-bound finite capsule side fit for two blind pockets. Independent STEP wall extrema, whole-solid nonpenetration, floor seating, axial extent, near-threshold refusal and exact captured replay; no retention, preload or rated load claim.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refusals':len(failures),
                      'isolated_collision_mm3':isolated['actual_step_collision_volume_mm3'],
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
