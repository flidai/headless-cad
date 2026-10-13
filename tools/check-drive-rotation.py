#!/usr/bin/env python3
"""Independent sampled STEP evidence for a native continuous rotation certificate.

The native proof covers the entire path. OCCT independently checks declared
geometry, sampled distances and all pair overlaps; sampling alone is not proof.
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
import subprocess
import sys

SPEC=importlib.util.spec_from_file_location('retained_drive',Path(__file__).with_name('check-retained-drive-module.py'))
d=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(d)
k=d.k;c=d.coupling.convex
f=d.oracle('rounded_drive_fillet_oracle','check-longitudinal-fillet.py')
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common,BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder
from OCP.gp import gp_Trsf,gp_Ax1,gp_Pnt,gp_Dir
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_FACE

MOVING={('shaft',),('drive_disc',),('parallel_key',),('shaft_washer',),('shaft_ring',),
        ('hub_left_washer',),('hub_left_ring',),('hub_right_washer',),('hub_right_ring',)}
COVER_SHAFT_CHECKS={
    'left_cover_shaft_gap':(('left_support','left_cover'),2.5,2.0),
    'right_cover_shaft_gap':(('right_support','right_cover'),4.5,4.0),
}

HOUSING_OUTLINE=[(-22,-26),(22,-26),(22,14),(14,22),(-14,22),(-22,14)]
COVER_CENTERS=(-17,17)

def cover_references(diameter,pitch):
    """Construct the new stationary solids without reading native BReps/meshes."""
    r=diameter/2;p=pitch;q=[.5,.5,.5,.5]
    housing=d.bored_polygon(HOUSING_OUTLINE,16,[(0,0,r+4.025)])
    left,right=housing,housing
    for x in COVER_CENTERS:
        left=k.boolean(BRepAlgoAPI_Cut,left,BRepPrimAPI_MakeCylinder(
            f.gp_Ax2(gp_Pnt(x,0,-1),gp_Dir(0,0,1)),1.75,6).Shape())
        right=k.boolean(BRepAlgoAPI_Cut,right,BRepPrimAPI_MakeCylinder(
            f.gp_Ax2(gp_Pnt(x,0,11),gp_Dir(0,0,1)),1.75,6).Shape())
    cover=d.bored_polygon(HOUSING_OUTLINE,2,[(0,0,r+4.5)]+[(x,0,2.05) for x in COVER_CENTERS])
    screw=d.retained.revolve([[0,0],[1.65,0],[1.65,9],[3,9],[3,11],[0,11]])
    definitions={'housing_left':left,'housing_right':right,'end_cover':cover,
                 'cover_screw_envelope':screw}
    left_support=[25,0,32];right_support=[25+p,0,32]
    references={
        ('left_support','housing'):d.placed(left,left_support,q),
        ('right_support','right_housing'):d.placed(right,right_support,q),
        ('left_support','left_cover'):d.placed(d.placed(cover,[0,0,-4]),left_support,q),
        ('right_support','right_cover'):d.placed(d.placed(cover,[0,0,18]),right_support,q),
    }
    for side,support,origin,rotation in (
            ('left',left_support,5,[1,0,0,0]),
            ('right',right_support,11,None)):
        for label,x in (('minus',-17),('plus',17)):
            body=d.placed(screw,[x,0,origin],rotation)
            references[(side+'_support',side+'_cover_screw_'+label)]=d.placed(body,support,q)
    part_ids={path:('housing_left' if path==('left_support','housing') else
                    'housing_right' if path==('right_support','right_housing') else
                    'end_cover' if path[-1].endswith('_cover') else
                    'cover_screw_envelope') for path in references}
    expected={
        'housing_left':16*(2048-math.pi*(r+4.025)**2)-2*math.pi*1.75**2*5,
        'housing_right':16*(2048-math.pi*(r+4.025)**2)-2*math.pi*1.75**2*5,
        'end_cover':2*(2048-math.pi*(r+4.5)**2-2*math.pi*2.05**2),
        'cover_screw_envelope':math.pi*(1.65**2*9+3**2*2),
    }
    return definitions,references,part_ids,expected

def rotor_envelopes(diameter,pitch):
    """Conservative axial/radial bounds invariant under the authored X rotation."""
    r=diameter/2;p=pitch;d0=25+p/2
    return {
        ('shaft',):(10,p+80,r+2),
        ('drive_disc',):(d0,d0+14,24),
        ('parallel_key',):(d0+.5,d0+13.5,math.hypot(r+2,2)),
        ('hub_left_washer',):(d0-1.6,d0-.1,r+2),
        ('hub_left_ring',):(d0-2.8,d0-1.6,r+1.5),
        ('hub_right_washer',):(d0+14.1,d0+15.6,r+2),
        ('hub_right_ring',):(d0+15.6,d0+16.8,r+1.5),
        ('shaft_washer',):(p+43.1,p+44.6,r+2),
        ('shaft_ring',):(p+44.6,p+45.8,r+1.5),
    }

def stationary_envelopes(diameter,pitch):
    """Each fixed cover/screw lies outside a coaxial radial exclusion cylinder."""
    r=diameter/2;p=pitch
    envelopes={
        ('left_support','left_cover'):(21,23,r+4.5),
        ('right_support','right_cover'):(p+43,p+45,r+4.5),
    }
    for side,first,last in [('left',19,30),('right',p+36,p+47)]:
        for label in ['minus','plus']:
            envelopes[(side+'_support',side+'_cover_screw_'+label)]=(first,last,17-3)
    return envelopes

def all_phase_new_pair_certificates(diameter,pitch):
    """Positive separation bounds for every new fixed/rotor pair, not samples."""
    fixed=stationary_envelopes(diameter,pitch)
    moving=rotor_envelopes(diameter,pitch)
    d.require(set(moving)==MOVING and len(fixed)==6,'complete moving/new fixed envelope inventory')
    certificates={}
    # Far larger than floating-point subtraction error at this assembly scale;
    # reserves 0.1 micron for transformed/exported nominal surfaces as well.
    guard=max(1e-4,256*sys.float_info.epsilon*(pitch+100))
    for fixed_path,(fmin,fmax,radial_floor) in fixed.items():
        for moving_path,(mmin,mmax,radial_ceiling) in moving.items():
            # The shaft's only r+2 shoulder ends at world X=10+12.9;
            # later grooves/keyseat remove material from the r journal.
            if moving_path==('shaft',) and fmin>=10+12.9:
                radial_ceiling=diameter/2
            axial_gap=max(fmin-mmax,mmin-fmax,0)
            radial_gap=radial_floor-radial_ceiling
            raw_bound=max(axial_gap,radial_gap)
            bound=raw_bound-guard
            d.require(bound>0,'uncertified fixed/rotor pair '+str((fixed_path,moving_path)))
            certificates['/'.join(fixed_path)+' | '+'/'.join(moving_path)]={
                'minimum_distance_lower_bound_mm':bound,
                'raw_geometric_bound_mm':raw_bound,
                'arithmetic_and_export_guard_mm':guard,
                'rotor_radial_ceiling_mm':radial_ceiling,
                'method':'invariant_axial_interval' if axial_gap>=radial_gap else 'invariant_radial_exclusion',
            }
    d.require(len(certificates)==54,'all 6 new fixed × 9 rotor pairs certified')
    return certificates

def posed(shape,angle):
    frame=gp_Trsf();frame.SetRotation(gp_Ax1(gp_Pnt(10,0,32),gp_Dir(1,0,0)),angle)
    return BRepBuilderAPI_Transform(shape,frame,True).Shape()

def boundary_distance(point,shape):
    faces=[];iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():faces.append(iterator.Current());iterator.Next()
    return min(c.witness_distance(point,face) for face in faces)

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def run_case(cad,directory,diameter,pitch,sign,radius):
    tag=f'd{diameter}-p{pitch}-turn{sign}-r{radius}';out=directory/tag
    parameters=dict(shaft_diameter=diameter,mounting_pitch=pitch,
                    turn_radians=sign*math.tau,base_corner_radius=radius)
    result=c.run(cad,d.ROOT/'examples/drive_module_rotation.star',out,parameters)
    d.require(result.returncode==0,result.stderr);manifest=json.loads(result.stdout)
    report=json.loads((out/'rigid-rotation-groups.json').read_text());group=report['groups']['rotor']
    d.require(group['status']=='named_pairs_passed' and group['coverage']=='named_pairs_only','explicit named-pair scope')
    d.require({tuple(member['instance_path']) for member in group['members']}==MOVING,'nine exact moving members')
    d.require(group['assembly_content_hash']==manifest['outputs']['drive_module_release']['content_hash'],'source-bound assembly')
    for actual,wanted in zip(group['rotation_world']['pivot'],[10,0,32]):d.close(actual,wanted,'world pivot')
    for actual,wanted in zip(group['rotation_world']['axis'],[1,0,0]):d.close(actual,wanted,'world axis')
    d.close(group['rotation_world']['angle_radians'],sign*math.tau,'signed full turn')
    native=manifest['outputs']['drive_module_release']
    d.require(native['instances']==30 and native['definitions']==14,
              'thirty placed solids and fourteen shared definitions')
    leaves=c.imported_leaves(out/'drive_module_release.step')
    _,references,part_ids=d.geometries(diameter,pitch)
    additions,new_refs,new_parts,expected_new=cover_references(diameter,pitch)
    references.update(new_refs);part_ids.update(new_parts)
    expected_paths=set(references)
    d.require(len(references)==30 and len(set(part_ids.values()))==14,
              'independent exact occurrence/definition inventory')
    d.require(set(leaves)==expected_paths,'all thirty STEP occurrence paths')
    expected_quantities={part:list(part_ids.values()).count(part) for part in set(part_ids.values())}
    bom={item['part_id']:item for item in native['bom']['items']}
    d.require(set(bom)==set(expected_quantities) and native['bom']['missing_material_occurrences']==0,
              'complete fourteen-definition material BOM')
    for part,shape in additions.items():
        d.close(k.properties(shape)[0],expected_new[part],'independent '+part+' volume',absolute=3e-6)
        d.require(bom[part]['quantity']==expected_quantities[part],
                  'BOM quantity '+part)
        d.close(bom[part]['unit_volume_mm3'],expected_new[part],
                'native BOM unit volume '+part,absolute=3e-6)
    points=[(0,-40),(pitch+90,-40),(pitch+90,40),(0,40)]
    base=f.filleted(points,6,radius,[0,1,2,3])
    for x in [33,pitch+33]:
        for y in [-28,28]:
            cutter=f.BRepPrimAPI_MakeCylinder(f.gp_Ax2(f.gp_Pnt(x,y,0),f.gp_Dir(0,0,1)),3,6).Shape()
            base=k.boolean(BRepAlgoAPI_Cut,base,cutter)
    references[('base',)]=base
    d.close(k.properties(base)[0],6*(80*(pitch+90)-36*math.pi-(4-math.pi)*radius**2),'rounded base closed-form volume')
    shapes={path:leaf['shape'] for path,leaf in leaves.items()}
    for path,shape in shapes.items():
        d.require(leaves[path]['part']==part_ids[path],'STEP part identity '+str(path))
        for first,second in [(shape,references[path]),(references[path],shape)]:
            d.close(k.properties(k.boolean(BRepAlgoAPI_Cut,first,second))[0],0,'independent authored geometry',absolute=3e-6)
    new_certificates=all_phase_new_pair_certificates(diameter,pitch)
    r=diameter/2
    nominal_contacts={}
    for side,sleeve in [('left','sleeve'),('right','right_sleeve')]:
        cover=(side+'_support',side+'_cover')
        flange=(side+'_support',sleeve)
        d.close(c.distance(shapes[cover],shapes[flange]),0,
                'nominal cover/flange plane contact '+side,absolute=2e-6)
        area=math.pi*((r+6)**2-(r+4.5)**2)
        d.require(area>0,'positive analytic annular cover/flange contact '+side)
        nominal_contacts[side+'_cover_flange']={
            'occt_min_distance_mm':0,'analytic_contact_area_mm2':area,
            'basis':'matched coplanar annular faces; distinct exact STEP solids'}
        for position_tag in ['minus','plus']:
            screw=(side+'_support',side+'_cover_screw_'+position_tag)
            d.close(c.distance(shapes[cover],shapes[screw]),0,
                    'nominal screw-head seating '+side+position_tag,absolute=2e-6)
            seat_area=math.pi*(3**2-2.05**2)
            d.require(seat_area>0,'positive analytic screw-head seat')
            nominal_contacts[side+'_screw_'+position_tag+'_seat']={
                'occt_min_distance_mm':0,'analytic_contact_area_mm2':seat_area,
                'basis':'matched coplanar head/clearance annulus; distinct exact STEP solids'}
    expected={'left_support_running_gap':.1,'right_support_running_gap':.15,
              'left_support_disc_gap':pitch/2-16,'right_support_disc_gap':pitch/2-14,'disc_shaft_gap':.15,
              'hub_left_washer_shaft_gap':.1,'hub_right_washer_shaft_gap':.1,'shaft_washer_shaft_gap':.1,
              'hub_left_washer_hub_gap':.1,'hub_right_washer_hub_gap':.1,'shaft_washer_bush_gap':.1,
              **{name:values[1] for name,values in COVER_SHAFT_CHECKS.items()}}
    checks=manifest['clearance_checks'];d.require(set(checks)==set(expected),'thirteen explicit pairs')
    query_count=0;proofs={}
    for name,check in checks.items():
        d.require(check['rotation_group']=='rotor' and check['status']=='passed','group-bound passed check')
        left=tuple(check['left']['instance_path']);right=tuple(check['right']['instance_path'])
        both=left in MOVING and right in MOVING
        d.require(check['lower_bound_mm']<=expected[name]+1e-7 and check['upper_bound_mm']>=expected[name]-1e-7,'whole-path expected minimum')
        d.require(check['lower_bound_mm']>=check['declaration']['min_clearance'],'native lower bound meets requirement')
        if name in COVER_SHAFT_CHECKS:
            fixed_path,_,required=COVER_SHAFT_CHECKS[name]
            d.require(left==fixed_path and right==('shaft',) and not both,
                      'cover-shaft endpoint orientation '+name)
            d.close(check['declaration']['min_clearance'],required,
                    'cover-shaft authored minimum '+name)
            d.require(check['query']['method']=='bore_trim_witness' and
                      check['motion_proof']=='continuous_rotation' and
                      check['moving_endpoint']=='right',
                      'native circular-bore/shaft witness '+name)
            sweep=check['rotation_sweep']
            d.require(sweep['proof_kind']=='bore_witness_orbit' and
                      sweep['query_count']==3 and sweep['intervals']==[],
                      'native full-turn bore witness orbit '+name)
            orbit=sweep['bore_orbit']
            d.require('symmetry' not in sweep and orbit['start']==0 and orbit['end']==1,
                      'explicit complete bore orbit interval '+name)
            d.close(orbit['bore_radius_mm'],diameter/2+4.5,
                    'selected actual cover bore radius '+name)
            d.require(orbit['guarded_radius_mm']>=orbit['bore_radius_mm'] and
                      orbit['shaft_stock_outer_radius_mm']<=diameter/2+2+1e-8 and
                      orbit['axis_offset_mm']<1e-6 and
                      orbit['axis_direction_departure']<1e-6 and
                      orbit['distance_variation_bound_mm']>0 and
                      orbit['lower_bound_mm']<=expected[name]<=orbit['upper_bound_mm'],
                      'finite source-bound bore/stock full-turn geometry '+name)
            d.require(sorted(sample['time'] for sample in sweep['samples'])==[0,.5,1],
                      'three actual bore orbit samples '+name)
            for sample in sweep['samples']:
                moved=posed(shapes[right],sign*math.tau*sample['time'])
                for side,shape in [('left',shapes[left]),('right',moved)]:
                    d.require(boundary_distance(sample['query'][side+'_point'],shape)<2e-6,
                              'native bore witness on exact STEP boundary '+name)
            query_count+=3;proofs[name]='bore_witness_orbit'
            continue
        if both:
            d.require(check['motion_proof']=='common_rigid_motion' and 'rotation_sweep' not in check,'same rigid transform preserves distance')
            query_count+=1;proofs[name]='common_rigid_motion'
        else:
            sweep=check['rotation_sweep'];proof=sweep['symmetry']
            d.require(sweep['proof_kind']=='rotational_symmetry' and sweep['query_count']==3,'native three-query symmetry proof')
            d.require(sweep['intervals']==[] and proof['start']==0 and proof['end']==1,'explicit full-path certificate, no synthetic zero-displacement interval')
            d.require(sweep['speed_bound_mm']>0 and sweep['radius_bound_mm']>0,'physical motion preserved')
            d.require(proof['distance_variation_bound_mm']>0,'arithmetic allowance retained')
            stationary=left if right in MOVING else right;moving=right if right in MOVING else left
            d.require(check['moving_endpoint']==('right' if right in MOVING else 'left'),'authored endpoint orientation')
            d.require(sorted(s['time'] for s in sweep['samples'])==[0,.5,1],'three actual native samples')
            for sample in sweep['samples']:
                moved=posed(shapes[moving],sign*math.tau*sample['time'])
                for side,shape in [('left',shapes[stationary]),('right',moved)]:
                    d.require(boundary_distance(sample['query'][side+'_point'],shape)<2e-6,'native sample witness on actual STEP boundary')
            query_count+=3;proofs[name]='rotational_symmetry'
    d.require(query_count==27,'twenty-seven actual native group queries')
    sampled=[]
    for phase in [0,.125,.25,.375,.5,.625,.75,.875,1]:
        actual={path:posed(shape,sign*math.tau*phase) if path in MOVING else shape for path,shape in shapes.items()}
        distances={}
        for name,check in checks.items():
            left=actual[tuple(check['left']['instance_path'])];right=actual[tuple(check['right']['instance_path'])]
            value=c.distance(left,right);d.close(value,expected[name],'independent rotated distance '+name,absolute=2e-6)
            distances[name]=value
        boxes={path:d.bounds(shape) for path,shape in actual.items()};paths=list(actual);maximum_overlap=0.;pairs=0
        for i,left in enumerate(paths):
            for right in paths[i+1:]:
                volume=0. if d.separated_boxes(boxes[left],boxes[right]) else k.properties(k.boolean(BRepAlgoAPI_Common,actual[left],actual[right]))[0]
                d.close(volume,0,'sampled assembly overlap',absolute=3e-6);maximum_overlap=max(maximum_overlap,volume);pairs+=1
        d.require(pairs==435,'all 30 choose 2 occurrence pairs independently checked')
        sampled.append(dict(phase=phase,distances_mm=distances,pairs=pairs,maximum_overlap_mm3=maximum_overlap))
    for name in ['shaft_axial_endplay','hub_axial_endplay']:
        stack=manifest['tolerance_stacks'][name]
        d.require(stack['measurement_interval_certified'],'retained native endplay obligation')
        d.close(stack['nominal_mm'],.2,'nominal endplay')
    verified=k.command(cad,'verify',out);d.require(verified.returncode==0,verified.stderr)
    replay_source=directory/(tag+'-replay-input')
    replay_source.mkdir()
    captured=replay_source/'drive_module_rotation.star'
    shutil.copyfile(out/'recipe.star',captured)
    replay=directory/(tag+'-captured-replay')
    rebuilt=c.run(cad,captured,replay,parameters)
    d.require(rebuilt.returncode==0,'captured source replay: '+rebuilt.stderr)
    replay_verify=k.command(cad,'verify',replay)
    d.require(replay_verify.returncode==0,'captured source replay verify: '+replay_verify.stderr)
    compared=[]
    for name in ['drive_module_release.step','housing.step','right_housing.step',
                 'end_cover.step','cover_screw_envelope.step',
                 'rigid-rotation-groups.json',
                 'section-left_cover_pilot_section.json',
                 'section-right_cover_pilot_section.json',
                 'section-end_cover_section.json']:
        d.require(digest(out/name)==digest(replay/name),'captured replay differs: '+name)
        compared.append(name)
    rebuilt_manifest=json.loads((replay/'manifest.json').read_text())
    for field in ['recipe_source_hash','feature_hashes','outputs','face_assertions',
                  'section_views','clearance_checks','rigid_rotation_groups']:
        d.require(manifest[field]==rebuilt_manifest[field],
                  'captured replay semantics differ: '+field)
    return dict(case=tag,occurrences=30,definitions=14,
                fixed_rotor_certificates=new_certificates,
                nominal_contacts=nominal_contacts,
                actual_native_queries=query_count,proofs=proofs,
                sampled_step_evidence=sampled,
                captured_source_replay={'valid':True,'compared_files':compared})

def rejected_edits(cad,directory):
    source=d.ROOT/'examples/drive_module_rotation.star'
    compiled=k.command(cad,'compile',source);d.require(compiled.returncode==0,compiled.stderr)
    original=json.loads(compiled.stdout);path=directory/'failure-source.json';out=directory/'failure-publication';cache=directory/'cache'
    path.write_text(json.dumps(original))
    def build(extra=()):return k.command(cad,'build',path,'--out-dir',out,'--cache-dir',cache,*extra)
    first=build();d.require(first.returncode==0,first.stderr);baseline=c.inventory(out);failures=[]
    for label in ['offset_axis','missing_reference','excessive_gap']:
        changed=json.loads(json.dumps(original))
        if label=='offset_axis':changed['rigid_rotation_groups'][0]['pivot']=[1,0,0]
        elif label=='missing_reference':changed['rigid_rotation_groups'][0]['reference_instance_path']=['base']
        else:changed['clearance_checks'][0]['min_clearance']=.3
        path.write_text(json.dumps(changed));result=build(['--output','sleeve'])
        d.require(result.returncode!=0,'invalid motion must fail '+label)
        diagnostic=json.loads(result.stderr);d.require(diagnostic['code'] in ['invalid_rotation_group','rotation_group_failed','clearance_check_failed'],'structured failure')
        d.require(c.inventory(out)==baseline,'invalid motion preserves every published byte')
        failures.append(dict(case=label,diagnostic=diagnostic))
    source_text=source.read_text()
    for label,before,after in [
            ('pilot_reaches_opposite_cap','pilot_depth = 5','pilot_depth = 16'),
            ('pilot_breaks_outer_wall',
             'end_cover_pilots("housing_left_pilots", "start_cap", cap_centroid_y)',
             'end_cover_pilots("housing_left_pilots", "start_cap", cap_centroid_y+30)')]:
        d.require(source_text.count(before)==1,'unique source edit '+label)
        edited=directory/(label+'.star')
        edited.write_text(source_text.replace(before,after))
        result=c.run(cad,edited,out,{})
        d.require(result.returncode!=0,'invalid drilled housing must fail '+label)
        d.require(c.inventory(out)==baseline,'invalid housing preserves every published byte '+label)
        failures.append(dict(case=label,diagnostic=json.loads(result.stderr)))
    return failures

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=d.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();source=d.ROOT/'examples/drive_module_rotation.star'
    executable_sha256,source_sha256,script_sha256=digest(cad),digest(source),digest(__file__)
    cases=[run_case(cad,args.out_dir,*configuration) for configuration in
           [(12,60,1,6),(14,72,-1,2),(10,90,1,10),(16,90,1,10)]]
    failures=rejected_edits(cad,args.out_dir)
    d.require(digest(cad)==executable_sha256 and digest(source)==source_sha256
              and digest(__file__)==script_sha256,
              'pinned executable/source/oracle changed during oracle run')
    report=dict(valid=True,executable_sha256=executable_sha256,
                source_sha256=source_sha256,oracle_script_sha256=script_sha256,
                python_version=platform.python_version(),
                ocp_distribution=importlib.metadata.version('cadquery-ocp'),
                cases=cases,rejected_edits=failures,
                scope='Native continuous named-pair proofs plus conservative full-turn bounds for 54 new fixed/rotor pairs; independent sampled STEP checks of all pairs. No general assembly collision, installation or load certification')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))

if __name__=='__main__':main()
