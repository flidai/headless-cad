#!/usr/bin/env python3
"""Independent sampled STEP evidence for a native continuous rotation certificate.

The native proof covers the entire path. OCCT independently checks declared
geometry, sampled distances and all pair overlaps; sampling alone is not proof.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import subprocess

SPEC=importlib.util.spec_from_file_location('retained_drive',Path(__file__).with_name('check-retained-drive-module.py'))
d=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(d)
k=d.k;c=d.coupling.convex
f=d.oracle('rounded_drive_fillet_oracle','check-longitudinal-fillet.py')
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common,BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.gp import gp_Trsf,gp_Ax1,gp_Pnt,gp_Dir
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_FACE

MOVING={('shaft',),('drive_disc',),('parallel_key',),('shaft_washer',),('shaft_ring',),
        ('hub_left_washer',),('hub_left_ring',),('hub_right_washer',),('hub_right_ring',)}

def posed(shape,angle):
    frame=gp_Trsf();frame.SetRotation(gp_Ax1(gp_Pnt(10,0,32),gp_Dir(1,0,0)),angle)
    return BRepBuilderAPI_Transform(shape,frame,True).Shape()

def boundary_distance(point,shape):
    faces=[];iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():faces.append(iterator.Current());iterator.Next()
    return min(c.witness_distance(point,face) for face in faces)

def run_case(cad,directory,diameter,pitch,sign,radius):
    tag=f'd{diameter}-p{pitch}-turn{sign}-r{radius}';out=directory/tag
    result=c.run(cad,d.ROOT/'examples/drive_module_rotation.star',out,
                 dict(shaft_diameter=diameter,mounting_pitch=pitch,turn_radians=sign*math.tau,base_corner_radius=radius))
    d.require(result.returncode==0,result.stderr);manifest=json.loads(result.stdout)
    report=json.loads((out/'rigid-rotation-groups.json').read_text());group=report['groups']['rotor']
    d.require(group['status']=='named_pairs_passed' and group['coverage']=='named_pairs_only','explicit named-pair scope')
    d.require({tuple(member['instance_path']) for member in group['members']}==MOVING,'nine exact moving members')
    d.require(group['assembly_content_hash']==manifest['outputs']['drive_module_release']['content_hash'],'source-bound assembly')
    for actual,wanted in zip(group['rotation_world']['pivot'],[10,0,32]):d.close(actual,wanted,'world pivot')
    for actual,wanted in zip(group['rotation_world']['axis'],[1,0,0]):d.close(actual,wanted,'world axis')
    d.close(group['rotation_world']['angle_radians'],sign*math.tau,'signed full turn')
    leaves=c.imported_leaves(out/'drive_module_release.step')
    _,references,_=d.geometries(diameter,pitch)
    points=[(0,-40),(pitch+90,-40),(pitch+90,40),(0,40)]
    base=f.filleted(points,6,radius,[0,1,2,3])
    for x in [33,pitch+33]:
        for y in [-28,28]:
            cutter=f.BRepPrimAPI_MakeCylinder(f.gp_Ax2(f.gp_Pnt(x,y,0),f.gp_Dir(0,0,1)),3,6).Shape()
            base=k.boolean(BRepAlgoAPI_Cut,base,cutter)
    references[('base',)]=base
    d.close(k.properties(base)[0],6*(80*(pitch+90)-36*math.pi-(4-math.pi)*radius**2),'rounded base closed-form volume')
    d.require(set(leaves)==set(references),'all twenty-four STEP occurrences')
    shapes={path:leaf['shape'] for path,leaf in leaves.items()}
    for path,shape in shapes.items():
        for first,second in [(shape,references[path]),(references[path],shape)]:
            d.close(k.properties(k.boolean(BRepAlgoAPI_Cut,first,second))[0],0,'independent authored geometry',absolute=3e-6)
    expected={'left_support_running_gap':.1,'right_support_running_gap':.15,
              'left_support_disc_gap':pitch/2-16,'right_support_disc_gap':pitch/2-14,'disc_shaft_gap':.15,
              'hub_left_washer_shaft_gap':.1,'hub_right_washer_shaft_gap':.1,'shaft_washer_shaft_gap':.1,
              'hub_left_washer_hub_gap':.1,'hub_right_washer_hub_gap':.1,'shaft_washer_bush_gap':.1}
    checks=manifest['clearance_checks'];d.require(set(checks)==set(expected),'eleven explicit pairs')
    query_count=0;proofs={}
    for name,check in checks.items():
        d.require(check['rotation_group']=='rotor' and check['status']=='passed','group-bound passed check')
        left=tuple(check['left']['instance_path']);right=tuple(check['right']['instance_path'])
        both=left in MOVING and right in MOVING
        d.require(check['lower_bound_mm']<=expected[name]+1e-7 and check['upper_bound_mm']>=expected[name]-1e-7,'whole-path expected minimum')
        d.require(check['lower_bound_mm']>=check['declaration']['min_clearance'],'native lower bound meets requirement')
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
    d.require(query_count==21,'twenty-one actual native group queries')
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
        d.require(pairs==276,'all occurrence pairs independently checked')
        sampled.append(dict(phase=phase,distances_mm=distances,pairs=pairs,maximum_overlap_mm3=maximum_overlap))
    for name in ['shaft_axial_endplay','hub_axial_endplay']:
        stack=manifest['tolerance_stacks'][name]
        d.require(stack['measurement_interval_certified'],'retained native endplay obligation')
        d.close(stack['nominal_mm'],.2,'nominal endplay')
    verified=k.command(cad,'verify',out);d.require(verified.returncode==0,verified.stderr)
    return dict(case=tag,actual_native_queries=query_count,proofs=proofs,sampled_step_evidence=sampled)

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
    return failures

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=d.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cases=[run_case(args.cad.resolve(),args.out_dir,*configuration) for configuration in [(12,60,1,6),(14,72,-1,2),(10,90,1,10)]]
    failures=rejected_edits(args.cad.resolve(),args.out_dir)
    report=dict(valid=True,cases=cases,rejected_edits=failures,scope='Native continuous named-pair proofs with independent sampled STEP checks; no general assembly collision, installation or load certification')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))

if __name__=='__main__':main()
