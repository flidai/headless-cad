#!/usr/bin/env python3
"""Independent OCCT acceptance for inserts seated on native blind-pocket floors.

The native certificate concerns a local disk on each selected floor. This
oracle separately checks the entire authored insert solids against exact STEP
geometry and requires zero positive material intersection with the plate.
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
SPEC=importlib.util.spec_from_file_location('mixed_oracle',ROOT/'tools/check-mixed-slots.py')
m=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(m)
SEAT_SPEC=importlib.util.spec_from_file_location('tightened_oracle',ROOT/'tools/check-tightened-mounting.py')
t=importlib.util.module_from_spec(SEAT_SPEC);SEAT_SPEC.loader.exec_module(t)
PAD_SPEC=importlib.util.spec_from_file_location('pocketed_mounting_oracle',
                                               ROOT/'tools/check-pocketed-mounting.py')
p=importlib.util.module_from_spec(PAD_SPEC);PAD_SPEC.loader.exec_module(p)
k=m.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common,BRepAlgoAPI_Cut,BRepAlgoAPI_Fuse
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox,BRepPrimAPI_MakeCylinder
from OCP.gp import gp_Ax2,gp_Dir,gp_Pnt


def require(ok,reason):
    if not ok:raise AssertionError(reason)


def close(actual,expected,reason,absolute=2e-6):
    m.close(actual,expected,reason,absolute)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [SOURCE,ROOT/'examples/mixed_slot_fixture.star',
              ROOT/'examples/seated_pocket_inserts.star.lock.json']
    require(all(path.is_file() for path in paths),'all source and lock dependencies exist')
    return {str(path.relative_to(ROOT)):sha(path) for path in sorted(paths)}


def capsule_solid(center,length,radius,z0,height):
    """Independent OCC union of a rectangle and two full cylindrical caps."""
    x,y=center;half=length/2
    solid=BRepPrimAPI_MakeBox(gp_Pnt(x-radius,y-half,z0),2*radius,length,height).Shape()
    for end_y in (y-half,y+half):
        cap=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x,end_y,z0),gp_Dir(0,0,1)),
                                    radius,height).Shape()
        solid=k.boolean(BRepAlgoAPI_Fuse,solid,cap)
    return solid


def references(width,first_depth,second_depth,side,clearance=.2):
    through,blind=m.authored_slots(width,first_depth,second_depth)
    plate=m.s.fixture_reference(width,first_depth,second_depth,side)
    for slot in through:plate=m.cut_capsule(plate,slot,-3,18)
    inserts={}
    for name,slot in zip(('first_insert','second_insert'),blind):
        depth=slot['depth'];z0=12-depth if side==1 else -2
        inserts[name]=capsule_solid(slot['center'],slot['centerline_length'],
                                    slot['radius']-clearance,z0,depth+2)
    return plate,inserts,through,blind


def params(width=120,first_depth=3,second_depth=7,side=1,clearance=.2):
    values=[('plate_width',width),('first_depth',first_depth),
            ('second_depth',second_depth),('pocket_side',side),
            ('insert_clearance',clearance)]
    return [word for key,value in values for word in ('--param',f'{key}={value}')]


def directed_difference(actual,expected,label):
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
                 for a,b in ((actual,expected),(expected,actual))]
    for value in differences:close(value,0,label+' directed STEP/OCCT solid difference',3e-6)
    return differences


def assert_floor_disk(report,plate,insert,expected_z,label,plate_hash,insert_hash,floor_role):
    require(report['status']=='passed',label+' native floor seat passed')
    center=report['query']['disk_center'];radius=report['disk_radius_lower_mm']
    require(radius>=report['declaration']['min_shared_disk_radius_mm'],
            label+' positive guarded native contact disk')
    close(center[2],expected_z,label+' disk lies on actual floor plane')
    selected={side:report[side] for side in ('left','right')}
    floor_side=next(side for side,witness in selected.items()
                    if witness['part_id']=='mixed_fixture_part')
    insert_side='right' if floor_side=='left' else 'left'
    require(selected[floor_side]['part_content_hash']==plate_hash and
            selected[insert_side]['part_content_hash']==insert_hash,
            label+' seats current exact plate and insert definitions')
    require(selected[floor_side]['semantic_datum']['definition']['role']==floor_role,
            label+' selected native pocket floor rather than opening cap')
    floor=t.plane_face_with_disk(plate,center,expected_z,radius,label+' STEP floor')
    cap=t.plane_face_with_disk(insert,center,expected_z,radius,label+' STEP insert cap')
    require(floor['oriented_normal_z']*cap['oriented_normal_z']<-1+1e-9,
            label+' actual floor and insert cap normals oppose')
    require(report[floor_side]['semantic_datum']['selection']['status']=='resolved' and
            report[insert_side]['semantic_datum']['selection']['status']=='resolved',
            label+' both actual native source faces uniquely selected')
    require(report['signed_gap_lower_mm']<=0<=report['signed_gap_upper_mm'],
            label+' guarded native gap contains contact')
    close(k.properties(k.boolean(BRepAlgoAPI_Common,plate,insert))[0],0,
          label+' complete STEP insert does not penetrate plate',3e-6)
    return {'disk_center_mm':center,'native_disk_radius_lower_mm':radius,
            'floor_step_margin_mm':floor['nearest_step_boundary_mm'],
            'insert_step_margin_mm':cap['nearest_step_boundary_mm'],
            'floor_face':report[floor_side]['semantic_datum']['selection']['witnesses'][0]['target_face']}


def check_case(cad,root,tag,width=120,first_depth=3,second_depth=7,side=1,clearance=.2):
    arguments=params(width,first_depth,second_depth,side,clearance)
    out=root/tag;manifest=m.o.command(cad,'build',SOURCE,'--out-dir',out,*arguments)
    m.o.command(cad,'verify',out)
    assembly=manifest['outputs']['seated_pocket_inserts']
    require(assembly['instances']==3,'exact plate and two removable inserts')
    plate_ref,insert_refs,through,blind=references(width,first_depth,second_depth,side,clearance)
    leaves=t.c.imported_leaves(out/'seated_pocket_inserts.step')
    require(set(leaves)=={('fixture',),('first_insert',),('second_insert',)},
            'three named exact STEP occurrence paths')
    shapes={name:leaves[(name,)]['shape'] for name in ('fixture','first_insert','second_insert')}
    for name,reference in [('fixture',plate_ref),*insert_refs.items()]:
        directed_difference(shapes[name],reference,tag+' '+name)
        exact=k.properties(reference);actual=k.properties(shapes[name])
        m.b.compare_moments(actual,exact,tag+' '+name+' full mass moments')
        if name!='fixture':
            slot=blind[0 if name=='first_insert' else 1]
            area=(2*(slot['radius']-clearance)*slot['centerline_length']+
                  math.pi*(slot['radius']-clearance)**2)
            close(actual[0],area*(slot['depth']+2),name+' closed-form capsule volume')
        k.mesh(out/(name+'.stl'),actual[0]) if (out/(name+'.stl')).exists() else None
    k.mesh(out/'fixture.stl',k.properties(plate_ref)[0])
    contacts={};floor_z={
        'first_insert':12-first_depth if side==1 else first_depth,
        'second_insert':12-second_depth if side==1 else second_depth}
    floors={'first_insert':'slot_locating_floor','second_insert':'slot_relief_floor'}
    plate_hash=manifest['feature_hashes']['relief_pocket']
    for name in ('first_insert','second_insert'):
        report=manifest['nominal_seating_checks'][name+'_floor_seat']
        insert_hash=manifest['feature_hashes'][name+'_body']
        require(report['assembly_content_hash']==assembly['content_hash'],
                name+' seat certificate binds current assembly')
        contacts[name]=assert_floor_disk(report,shapes['fixture'],shapes[name],floor_z[name],
                                         tag+' '+name,plate_hash,insert_hash,floors[name])
        close(k.properties(k.boolean(BRepAlgoAPI_Common,shapes['fixture'],shapes[name]))[0],
              0,name+' complete plate/insert nonpenetration')
        top=14 if side==1 else -2
        bounds=t.d.d.bounds(shapes[name])
        close(bounds[1 if side==1 else 0][2],top,
              name+' actual STEP protrusion endpoint')
    close(k.properties(k.boolean(BRepAlgoAPI_Common,shapes['first_insert'],
                                      shapes['second_insert']))[0],0,
          'two separate inserts do not intersect')
    first_floor=manifest['face_assertions']['first_floor_survives']
    require(first_floor['status']=='resolved' and len(first_floor['witnesses'])==1,
            'first floor survives serial second cut')
    first_witness=first_floor['witnesses'][0]
    require(first_witness['anchor_faces'][0]!=first_witness['target_face'] and
            contacts['first_insert']['floor_face']==first_witness['target_face'],
            'first insert seats on remapped actual first-floor FaceId')
    for name in ('front_lower','front_upper','rear_lower','rear_upper'):
        selected=manifest['face_assertions'][name+'_walls_survive']
        require(selected['status']=='resolved' and len(selected['witnesses'])==4 and
                all(w['anchor_faces']==[w['target_face']] for w in selected['witnesses']),
                'original actual through-slot walls survive both pockets')
    expected_dims={'locating_depth':first_depth,'relief_depth':second_depth,
                   'remaining_web':12-second_depth,
                   'first_insert_proud_height':2,'second_insert_proud_height':2}
    for name,want in expected_dims.items():
        report=manifest['datum_dimensions'][name]
        require(report['status']=='passed',name+' native dimension passed')
        close(report['value_mm'],want,name+' source-bound native dimension')
    sheet=(out/'drawing-seated_insert_heights.svg').read_text()
    for name in ('first_insert_proud_height','second_insert_proud_height'):
        require(f'data-dimension="{name}"' in sheet and '2.000 mm' in sheet,
                name+' actual native protrusion printed in drawing table')
    section_report=json.loads((out/'section-insert_tops.json').read_text())
    section=section_report['section']
    expected_area=sum((2*(slot['radius']-clearance)*slot['centerline_length']+
                       math.pi*(slot['radius']-clearance)**2) for slot in blind)
    close(section['area'],expected_area,'two native protruding section loops',2e-5)
    sections={tuple(item['instance_path']):item['section']
              for item in section_report['occurrences']}
    require(len(sections[('first_insert',)]['loops'])==1 and
            len(sections[('second_insert',)]['loops'])==1 and
            len(sections[('fixture',)]['loops'])==0,
            'two insert contours and no base material in protruding section')
    captured=root/(tag+'-replay')
    repeated=m.o.command(cad,'build',out/'recipe.bundle.json','--out-dir',captured,*arguments)
    require(manifest['feature_hashes']==repeated['feature_hashes'] and
            manifest['nominal_seating_checks']==repeated['nominal_seating_checks'],
            'captured bundle replays all feature and floor-seat witnesses')
    for artifact in ('seated_pocket_inserts.step','nominal-seating-checks.json',
                     'drawing-seated_insert_heights.svg','recipe.bundle.json'):
        require((out/artifact).read_bytes()==(captured/artifact).read_bytes(),
                tag+' exact captured replay '+artifact)
    return {'case':tag,'parameters':{'plate_width':width,'first_depth':first_depth,
            'second_depth':second_depth,'pocket_side':side,'insert_clearance':clearance},
            'assembly_content_hash':assembly['content_hash'],
            'contacts':contacts,'native_dimensions_mm':expected_dims,
            'first_floor_remap':first_witness,
            'plate_volume_mm3':k.properties(plate_ref)[0],
            'insert_volumes_mm3':{name:k.properties(shape)[0]
                                  for name,shape in insert_refs.items()}}


def refusal_cases(cad,root):
    folder=root/'revisions';folder.mkdir()
    source=SOURCE.read_text();copied=folder/SOURCE.name;copied.write_text(source)
    shutil.copyfile(ROOT/'examples/mixed_slot_fixture.star',
                    folder/'mixed_slot_fixture.star')
    shutil.copyfile(ROOT/'examples/seated_pocket_inserts.star.lock.json',
                    folder/'seated_pocket_inserts.star.lock.json')
    out=root/'preserved';cache=root/'cache'
    m.o.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache)
    baseline=m.o.inventory(out);failures=[]
    edits=[
        ('lifted_insert_height_bound',source,['--param','insert_lift=0.1']),
        ('lifted_insert_native_seat',source.replace(
            'min = proud_height-0.000001, max = proud_height+0.000001',
            'min = 0, max = 4'),['--param','insert_lift=0.1']),
        ('impossible_disk',source,['--param','required_seat_radius=7']),
        ('oversized_insert',source,['--param','insert_clearance=-0.1']),
        ('wrong_floor',source.replace('target_datum = "first_floor"',
                                      'target_datum = "second_floor"',1),[]),
        ('fully_displaced_insert',source.replace('offset = [0,0,insert_lift]',
                                                 'offset = [20,0,insert_lift]',1),[]),
    ]
    for label,changed,args in edits:
        require(changed!=source or bool(args),label+' refusal mutation present')
        copied.write_text(changed)
        failed=m.o.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache,
                           *args,success=False)
        if label=='lifted_insert_native_seat':
            require(failed['code']=='nominal_seating_check_failed',
                    'lifted insert rejected by native floor contact after authored height bound widened')
        require(m.o.inventory(out)==baseline,
                label+' refusal preserves every published artifact byte')
        failures.append({'case':label,'diagnostic':failed})
    copied.write_text(source);m.o.command(cad,'verify',out)
    # The new floor-seat support must not turn an opening-cap void into a seat.
    pad_recipe=root/'opening_pad.star';pad_out=root/'opening_pad_preserved'
    pad_recipe.write_text(p.fixture_source(False))
    baseline_pad=m.o.command(cad,'build',pad_recipe,'--out-dir',pad_out)
    require(baseline_pad['nominal_seating_checks']['pad_seat']['status']=='passed',
            'unmachined annular pad really seats before pocket revision')
    pad_inventory=m.o.inventory(pad_out)
    pad_recipe.write_text(p.fixture_source(True))
    pad_failure=m.o.command(cad,'build',pad_recipe,'--out-dir',pad_out,
                            success=False)
    require(pad_failure['code']=='nominal_seating_check_failed',
            'pad entirely over blind opening still explicitly refuses')
    require(m.o.inventory(pad_out)==pad_inventory,
            'opening-pad refusal preserves prior published artifact inventory')
    m.o.command(cad,'verify',pad_out)
    failures.append({'case':'pad_over_blind_opening','diagnostic':pad_failure})
    return failures


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary=sha(cad);script=sha(__file__);inputs=pinned_inputs()
    cases=[check_case(cad,args.out_dir,tag,width,d1,d2,side) for tag,width,d1,d2,side in [
        ('top_default',120,3,7,1),('bottom_default',120,3,7,0),
        ('narrow_deep',96,2,9,1),('wide_shallow',144,5,6,1)]]
    failures=refusal_cases(cad,args.out_dir)
    require(binary==sha(cad) and script==sha(__file__) and inputs==pinned_inputs(),
            'frozen executable, oracle and source inventory unchanged')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'source_inventory_sha256':inputs,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'refusals':failures,
            'scope':'Two removable capsule inserts on exact, separate native blind-pocket floors; local guarded floor-seat disks checked inside actual trimmed STEP faces, independent whole-solid OCCT nonpenetration and source-bound proud heights. No retention, preload or rated load claim.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refusals':len(failures),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
