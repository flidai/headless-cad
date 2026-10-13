#!/usr/bin/env python3
"""Independent OCCT acceptance of seating and travel on the pocketed drive.

Nominal source certificates remain native. OCCT independently inspects their
actual exported faces, trims, whole-body contacts and sampled travel poses.
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
SPEC=importlib.util.spec_from_file_location('tightened_oracle',ROOT/'tools/check-tightened-mounting.py')
t=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(t)
a=t.s
k=t.k


def require(ok,message):
    if not ok:raise AssertionError(message)


def close(actual,expected,message,tolerance=2e-6):
    t.close(actual,expected,message,tolerance)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    for name in ('drive_module_rotation','drive_module_tightened','drive_module_adjustment'):
        paths.append(ROOT/'examples'/f'{name}.star')
        lock=ROOT/'examples'/f'{name}.star.lock.json'
        if lock.exists():paths.append(lock)
    require(all(path.is_file() for path in paths),'all source and lock dependencies exist')
    return {str(path.relative_to(ROOT)):sha(path) for path in sorted(paths)}


def assert_mixed_base(report,expected_hash):
    require(report['part_content_hash']==expected_hash,
            'mounting proof bound to actual pocketed base content')
    require(report['semantic_datum']['selection']['status']=='resolved',
            'actual base semantic datum uniquely resolved')


def check_tightened(cad,root,tag,parameters):
    # The frozen prior checker performs all 435 placed OCCT pair intersections,
    # all eight actual STEP trimmed-face seat disks, all four STEP shank gaps,
    # exact released-to-tightened leaf correspondence and captured replay.
    data=t.run_case(cad,root,tag,parameters)
    manifest=t.read(root/(tag+'-tightened')/'manifest.json')
    base=manifest['outputs']['base']['content_hash']
    require(parameters['base_pocket_depth']>0,'pocketed mounting case')
    for name,seat in manifest['nominal_seating_checks'].items():
        if seat['declaration']['output']!='drive_module_tightened':continue
        require(seat['status']=='passed','all eight pocketed mounting seats pass')
        if name.endswith('_base_seat'):
            side='right' if seat['right']['part_id']=='base' else 'left'
            assert_mixed_base(seat[side],base)
            require(seat[side]['semantic_datum']['definition']['role']=='start_cap',
                    'underside nut seat uses actual untouched opposite cap')
        for support in ('left','right'):
            require(seat[support]['semantic_datum']['definition']['role']!=
                    'slot_shallow_floor',
                    'floor cannot silently replace opening or opposite cap seat')
    for i in range(4):
        proof=manifest['clearance_checks'][f'tight_slot_{i}_shank_gap']
        require(proof['query']['method']=='slot_trim_witness',
                'static shank gap uses strict native through-slot trim')
        require(proof['left']['part_content_hash']==base,
                'static proof uses actual pocketed base')
    return {'case':tag,'parameters':parameters,'base_content_hash':base,
            'published_occurrences':data['published_occurrences'],
            'pairwise_nonpenetration':data['pairwise_nonpenetration'],
            'protected_seats':data['protected_seats'],
            'static_slot_gaps':data['static_slot_gaps'],
            'captured_replay':data['captured_replay']}


def check_adjustment(cad,root,tag,parameters):
    # This frozen checker independently constructs the four exact complete
    # capsule path bounds and tests each STEP pair at nine corroborating poses.
    data=a.run_drive(cad,root,tag,parameters)
    manifest=t.read(root/(tag+'-adjusted')/'manifest.json')
    base=manifest['outputs']['base']['content_hash']
    group=manifest['rigid_translation_groups']['upper_slot_travel']
    require(group['status']=='named_pairs_passed' and len(group['clearance_check_ids'])==4,
            'all four source-owned continuous pocketed mounting proofs')
    for i in range(4):
        name=f'mounting_slot_{i}_shank_gap';check=manifest['clearance_checks'][name]
        require(check['motion_proof']=='slot_translation' and
                check['query']['method']=='slot_trim_witness' and
                check['left']['part_content_hash']==base,
                name+' complete path bound to actual finished base through-slot walls')
        proof=check['sweep']['slot_motion_proof']
        require(len(proof['stock_slot_wall_faces'])==4 and
                len(set(map(str,proof['stock_slot_wall_faces'])))==4,
                name+' exact four native through-slot wall witnesses')
        role=('front_minus','front_plus','rear_minus','rear_plus')[i]
        retained=manifest['face_assertions'][f'base_{role}_survives']
        require(retained['status']=='resolved' and len(retained['witnesses'])==4 and
                all(w['anchor_faces']==[w['target_face']] for w in retained['witnesses']),
                name+' exact original through-wall lineage survives reliefs')
        require(set(proof['stock_slot_wall_faces'])==
                {w['target_face'] for w in retained['witnesses']},
                name+' translation proof uses original through walls, never blind walls')
        close(proof['capsule_radius_mm'],3.,name+' original through-slot radius')
        close(proof['complete_path_lower_bound_mm']+proof['arithmetic_guard_mm'],.5,
              name+' independent continuous endpoint minimum',1e-8)
    return {'case':tag,'parameters':parameters,'base_content_hash':base,
            'moving_occurrences':data['moving_occurrences'],
            'bolt_path_certificates':data['bolt_path_certificates'],
            'sampled_all_pair_phases':data['sampled_all_pair_phases'],
            'bundle_replay':data['bundle_replay']}


def fixture_source(pocket,opening='end_cap',bad=None,pad_x=15,seat_cap=None):
    if seat_cap is None:seat_cap=opening
    lines=[
        'stock = extrude(id="stock",profile=polygon(id="outline",points=[[0,0],[80,0],[80,50],[0,50]]),height=8)',
        'through = slot_pattern(id="through",body=stock,frame_origin=[0,0,0],slots=[{"id":"mount","center":[40,25],"direction":[1,0],"centerline_length":8,"radius":3}])',
    ]
    body='through'
    if pocket:
        role='end_cap' if opening=='end_cap' else 'start_cap'
        origin=[0,0,8 if opening=='end_cap' else 0]
        if bad=='support_misplacement':origin[2]+=.125
        lines.append(f'pocket = blind_slot_pattern(id="pocket",body=through,from_feature=through,cap="{role}",depth=3,frame_origin={origin},slots=[{{"id":"under_pad","center":[15,25],"direction":[1,0],"centerline_length":8,"radius":6}}])')
        body='pocket'
    anchor='through' if bad!='floor_as_seat' else 'pocket'
    face=seat_cap
    if bad=='floor_as_seat':face='slot_under_pad_floor'
    if bad=='unknown_role':face='missing_cap'
    pad_face='start_cap' if seat_cap=='end_cap' else 'end_cap'
    pad_z=8 if seat_cap=='end_cap' else -2
    lines += [
        f'plate = part(id="plate",body={body},datums={{"seat":face_datum(from_feature={anchor},role="{face}",x_direction=[1,0,0])}})',
        'pad_body = extrude(id="pad_body",profile=circle(id="pad_outline",radius=2,inner_radius=0.5),height=2)',
        f'pad = part(id="pad",body=pad_body,datums={{"seat":face_datum(from_feature=pad_body,role="{pad_face}",x_direction=[1,0,0])}})',
        f'fixture = assembly(id="fixture",instances=[instance(id="plate",part=plate),instance(id="pad",part=pad,offset={[pad_x,25,pad_z]})])',
        'assembly_output(name="fixture",assembly=fixture)',
        'nominal_seating_check(id="pad_seat",output="fixture",left_instance_path=["pad"],left_datum="seat",right_instance_path=["plate"],right_datum="seat",min_shared_disk_radius_mm=0.4)',
    ]
    return '\n'.join(lines)+'\n'


def fixture_refusals(cad,root):
    recipe=root/'pad_fixture.star';out=root/'pad_publication';cache=root/'pad_cache'
    recipe.write_text(fixture_source(False))
    baseline=a.command(cad,'build',recipe,'--out-dir',out,'--cache-dir',cache)
    a.command(cad,'verify',out)
    require(baseline['nominal_seating_checks']['pad_seat']['status']=='passed',
            'unmachined source pad genuinely seats before pocket')
    published=a.inventory(out);failures=[]
    safe=[]
    for label,source in [('opening_outside_pocket',fixture_source(True,pad_x=65)),
                         ('opposite_cap_under_pocket',fixture_source(True,seat_cap='start_cap'))]:
        recipe.write_text(source)
        safe_out=root/label
        accepted=a.command(cad,'build',recipe,'--out-dir',safe_out,'--cache-dir',cache)
        seat=accepted['nominal_seating_checks']['pad_seat']
        require(seat['status']=='passed',
                label+' actual pocketed stock seat passes outside opening')
        leaves=t.c.imported_leaves(safe_out/'fixture.step')
        require(set(leaves)=={('plate',),('pad',)},
                label+' two actual published STEP occurrences')
        plate=leaves[('plate',)]['shape'];pad=leaves[('pad',)]['shape']
        expected_plate=80*50*8-(2*3*8+math.pi*9)*8-(2*6*8+math.pi*36)*3
        close(k.properties(plate)[0],expected_plate,
              label+' independently calculated through-plus-blind STEP volume')
        center=seat['query']['disk_center'];radius=seat['disk_radius_lower_mm']
        z=8 if label=='opening_outside_pocket' else 0
        plate_support=t.plane_face_with_disk(plate,center,z,radius,label+' plate')
        pad_support=t.plane_face_with_disk(pad,center,z,radius,label+' pad')
        close(k.properties(k.boolean(t.BRepAlgoAPI_Common,plate,pad))[0],0,
              label+' actual STEP pad does not penetrate pocketed plate')
        safe.append({'case':label,'actual_plate_volume_mm3':expected_plate,
                     'reported_disk_radius_lower_mm':radius,
                     'plate_nearest_trim_mm':plate_support['nearest_step_boundary_mm'],
                     'pad_nearest_trim_mm':pad_support['nearest_step_boundary_mm']})
    for label,source in [
        ('pad_over_blind_opening',fixture_source(True)),
        ('floor_as_seat',fixture_source(True,bad='floor_as_seat')),
        ('unknown_role',fixture_source(True,bad='unknown_role')),
        ('support_misplacement',fixture_source(True,bad='support_misplacement')),
    ]:
        recipe.write_text(source)
        diagnostic=a.command(cad,'build',recipe,'--out-dir',out,'--cache-dir',cache,success=False)
        require(a.inventory(out)==published,label+' leaves prior publication byte-identical')
        failures.append({'case':label,'diagnostic':diagnostic})
    a.command(cad,'verify',out)
    return {'safe':safe,'refusals':failures}


def drive_refusals(cad,root):
    source=ROOT/'examples/drive_module_adjustment.star';loaded=ROOT/'examples/drive_module_rotation.star'
    folder=root/'drive_revisions';folder.mkdir()
    copied=folder/source.name;shutil.copyfile(source,copied);shutil.copyfile(loaded,folder/loaded.name)
    shutil.copyfile(Path(str(source)+'.lock.json'),folder/(source.name+'.lock.json'))
    out=root/'drive_publication';cache=root/'drive_cache'
    a.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache,
              '--param','base_pocket_depth=1')
    published=a.inventory(out);text=source.read_text()
    before='translation = [base_slot_length,0,0]'
    require(text.count(before)==1,'single authored travel declaration')
    copied.write_text(text.replace(before,'translation = [base_slot_length+2,0,0]'))
    diagnostic=a.command(cad,'build',copied,'--out-dir',out,'--cache-dir',cache,
                         '--param','base_pocket_depth=1',success=False)
    require(a.inventory(out)==published,'overtravel refusal preserves entire pocketed publication')
    a.command(cad,'verify',out)
    return [{'case':'pocketed_endpoint_overtravel','diagnostic':diagnostic}]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary=sha(cad);script=sha(__file__);sources=pinned_inputs()
    tightened=[check_tightened(cad,args.out_dir,tag,parameters) for tag,parameters in [
        ('pocketed_near',{'base_pocket_depth':1}),
        ('pocketed_far',{'base_pocket_depth':1,'mounting_endpoint':1}),
        ('shallow_near',{'base_pocket_depth':.5}),
        ('deep_far',{'base_pocket_depth':1.5,'mounting_endpoint':1}),
    ]]
    adjustment=[check_adjustment(cad,args.out_dir,tag,parameters)
                for tag,parameters in [
                    ('pocketed_travel',{'base_pocket_depth':1}),
                    ('wide_pocketed_travel',{'base_pocket_depth':1,
                                              'mounting_pitch':90,'base_slot_length':8}),
                ]]
    fixture=fixture_refusals(cad,args.out_dir)
    refused=fixture['refusals']+drive_refusals(cad,args.out_dir)
    require(binary==sha(cad) and script==sha(__file__) and sources==pinned_inputs(),
            'frozen executable, oracle and input inventory unchanged')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'source_inventory_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'tightened':tightened,'adjustment':adjustment,
            'safe_pocket_seats':fixture['safe'],'refusals':refused,
            'scope':'Exact pocketed base mounted at both slot endpoints: eight actual trimmed STEP seat disks and four static slot gaps per endpoint, four complete native through-slot translation proofs with sampled OCCT corroboration, and explicit refusal of a pad fully over a blind opening.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'tightened':len(tightened),
                      'adjustment':len(adjustment),'refusals':len(refused),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
