#!/usr/bin/env python3
"""Independent material and continuous-proof evidence for a rounded rotor.

Closed-form clearance checks every native sample and the true full-turn minimum.
OCCT imports actual STEP geometry and independently checks distances/witnesses.
Sampling is supporting evidence; the native Lipschitz intervals cover the path.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path


def module(name,file):
    s=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
f=module('fillet','check-longitudinal-fillet.py');c=module('convex','check-convex-clearance.py');k=f.k
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.gp import gp_Trsf,gp_Ax1,gp_Ax2,gp_Pnt,gp_Dir,gp_Quaternion,gp_Vec
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer


def pose_transform(pose):
    t=gp_Trsf()
    if pose:
        t.SetRotation(gp_Quaternion(.5,.5,.5,.5));t.SetTranslationPart(gp_Vec(13,-7,15))
    return t


def rotated(shape,rotation,time):
    t=gp_Trsf();t.SetRotation(gp_Ax1(gp_Pnt(*rotation['pivot']),gp_Dir(*rotation['axis'])),rotation['angle_radians']*time)
    return BRepBuilderAPI_Transform(shape,t,True).Shape()


def boundary_distance(point,shape):
    e=TopExp_Explorer(shape,TopAbs_FACE);distances=[]
    while e.More():distances.append(c.witness_distance(point,e.Current()));e.Next()
    return min(distances)


def nominal(radius,offset,theta):
    x=max(abs(offset*math.cos(theta))-(10-radius),0)
    y=max(abs(offset*math.sin(theta))-(6-radius),0)
    return math.hypot(x,y)-radius-2


def case(cad,root,radius,pose,sign):
    tag=f'r{radius}-p{pose}-s{sign}';out=root/tag
    params={'corner_radius':radius,'pose':pose,'turn_radians':sign*math.tau}
    p=c.run(cad,k.ROOT/'examples/rounded_rotor_fixture.star',out,params);k.require(p.returncode==0,p.stderr)
    manifest=json.loads(p.stdout);f.run(cad,'verify',out)
    group=json.loads((out/'rigid-rotation-groups.json').read_text())['groups']['rotor_rotation']
    k.require(group['coverage']=='named_pairs_only' and group['status']=='named_pairs_passed','named-pair continuous proof')
    k.require(len(group['members'])==1 and group['members'][0]['instance_path']==['rig','rotor'],'one exact moving occurrence')
    k.require(group['assembly_content_hash']==manifest['outputs']['rounded_rotor_fixture']['content_hash'],'group assembly identity')
    check=manifest['clearance_checks']['rotor_roller_path'];sweep=check['rotation_sweep']
    k.require(check['left']['instance_path']==['rig','roller'] and check['right']['instance_path']==['rig','rotor'],'authored endpoint identity')
    k.require(sweep['proof_kind']=='lipschitz_subdivision' and sweep.get('symmetry') is None,'genuine whole-path subdivision')
    k.require(sweep['query_count']==len(sweep['samples'])<=1025,'actual bounded query count')
    minimum=18-2-(math.hypot(10-radius,6-radius)+radius)
    k.require(check['lower_bound_mm']<=minimum<=check['upper_bound_mm'],'closed-form whole-turn minimum enclosure')
    k.require(check['lower_bound_mm']>=5,'requested native minimum')
    rotation=group['rotation_world'];k.close(rotation['angle_radians'],sign*math.tau)
    k.require(sweep['speed_bound_mm']>=sweep['radius_bound_mm']*abs(rotation['angle_radians']),'Lipschitz speed bound')
    k.require(sweep['radius_bound_mm']>=math.hypot(math.hypot(10-radius,6-radius)+radius,3),'true farthest native radius covered')
    leaves=c.imported_leaves(out/'rounded_rotor_fixture.step')
    k.require(set(leaves)=={('rig','rotor'),('rig','roller')},'two exact STEP occurrences')
    rotor=leaves[('rig','rotor')]['shape'];roller=leaves[('rig','roller')]['shape']
    expected_rotor=f.filleted([[-10,-6],[10,-6],[10,6],[-10,6]],6,radius,[0,1,2,3])
    expected_roller=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(18,0,0),gp_Dir(0,0,1)),2,6).Shape()
    for actual,expected in [(rotor,expected_rotor),(roller,expected_roller)]:
        expected=BRepBuilderAPI_Transform(expected,pose_transform(pose),True).Shape()
        for a,b in [(actual,expected),(expected,actual)]:k.close(k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0],0)
    max_distance_error=0.;max_boundary_error=0.
    for sample in sweep['samples']:
        time=sample['time'];exact=nominal(radius,18,sign*math.tau*time);query=sample['query']
        k.require(query['method']=='convex_support','native convex curved support method')
        k.require(sample['lower_bound_mm']<=exact<=sample['upper_bound_mm'],'closed-form sample enclosed')
        moved=rotated(rotor,rotation,time);distance=c.distance(roller,moved)
        c.close(distance,exact);max_distance_error=max(max_distance_error,abs(distance-exact))
        for side,shape in [('left',roller),('right',moved)]:
            error=boundary_distance(query[side+'_point'],shape);k.require(error<=2e-6,'native witness off actual STEP boundary')
            max_boundary_error=max(max_boundary_error,error)
        c.close(math.dist(query['left_point'],query['right_point']),query['upper_bound_mm'])
    intervals=sorted(sweep['intervals'],key=lambda i:i['start']);end=0.
    for interval in intervals:
        k.require(interval['start']==end and interval['end']>end,'no interval gap or overlap')
        sample=sweep['samples'][interval['sample_index']]
        k.require(sample['time']==interval['midpoint']==(interval['start']+interval['end'])/2,'actual midpoint sample')
        k.require(interval['displacement_bound_mm']>=sweep['speed_bound_mm']*(interval['end']-interval['start'])/2,'whole-interval displacement')
        k.require(5<=interval['lower_bound_mm']<=sample['lower_bound_mm']-interval['displacement_bound_mm'],'whole-interval conservative lower bound')
        end=interval['end']
    k.require(end==1,'complete signed turn covered')
    # Probe the actual unsampled analytic critical orientation independently too.
    critical=math.atan2(6-radius,10-radius)/math.tau
    c.close(c.distance(roller,rotated(rotor,rotation,critical)),minimum)
    replay=root/(tag+'-replay');args=[]
    for key,value in params.items():args += ['--param',f'{key}={value}']
    f.run(cad,'build',out/'recipe.star','--out-dir',replay,*args)
    k.require((replay/'rounded_rotor_fixture.step').read_bytes()==(out/'rounded_rotor_fixture.step').read_bytes(),'captured source and parameters reproduce STEP')
    return {'case':tag,'parameters':params,'true_full_turn_minimum_mm':minimum,
            'native_interval_mm':[check['lower_bound_mm'],check['upper_bound_mm']],
            'queries':sweep['query_count'],'certified_intervals':len(intervals),
            'max_occt_distance_error_mm':max_distance_error,'max_witness_boundary_distance_mm':max_boundary_error,
            'kernel_build_id':manifest['kernel_build_id'],'executor_build_id':manifest['executor_build_id']}


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cad',type=Path,default=k.ROOT/'target/release/cad');p.add_argument('--out-dir',type=Path,required=True)
    args=p.parse_args();root=args.out_dir.resolve();root.mkdir(parents=True,exist_ok=False);cad=args.cad.resolve()
    cases=[case(cad,root,*config) for config in [(2,0,1),(2,1,-1),(3,1,1)]]
    accepted=root/'r2-p0-s1';before=c.inventory(accepted)
    failure=f.run(cad,'build',k.ROOT/'examples/rounded_rotor_fixture.star','--param','corner_radius=1','--output','rotor_inspection','--out-dir',accepted,success=False)
    k.require(c.inventory(accepted)==before,'interior-path failed edit preserved generation')
    k.require(nominal(1,18,0)==6 and nominal(1,18,math.pi)==6,'failed edit has clear endpoints and midpoint')
    true_failed=16-(math.hypot(9,5)+1);k.require(true_failed<5,'independent true failed interior minimum')
    report={'valid':True,'cases':cases,'failed_interior_minimum_mm':true_failed,'atomic_rejection':failure}
    (root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))

if __name__=='__main__':main()
