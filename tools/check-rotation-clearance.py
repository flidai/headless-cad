#!/usr/bin/env python3
"""Independent continuous rotation coverage and imported STEP pose oracle."""
from fractions import Fraction
import importlib.util
import math
from pathlib import Path

SPEC=importlib.util.spec_from_file_location('translation_oracle',Path(__file__).with_name('check-translation-clearance.py'))
translation=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(translation)
convex=translation.convex
exact=translation.exact
covers_unit_interval=translation.covers_unit_interval


def orbit_minimum(orbit_radius, guard_radius, angle, moving_radius=2., fixed_radius=3.):
    """Center separation over signed angle sweep, guard located on world +Y.
    Candidate extrema are endpoints plus all included quarter-turn alignments.
    """
    start,end=min(0.,angle),max(0.,angle)
    candidates=[0.,angle]
    for k in range(math.floor((start-math.pi/2)/(2*math.pi))-1,math.ceil((end-math.pi/2)/(2*math.pi))+2):
        theta=math.pi/2+2*math.pi*k
        if start<=theta<=end:candidates.append(theta)
    center=min(math.hypot(orbit_radius*math.cos(t),orbit_radius*math.sin(t)-guard_radius) for t in candidates)
    return max(0.,center-moving_radius-fixed_radius)


def audit(report):
    import copy
    sweep=report['rotation_sweep'];radius=exact(sweep['radius_bound_mm']);speed=exact(sweep['speed_bound_mm'])
    convex.require(radius>=22,'radius must cover farthest tool point from rotation axis')
    convex.require(speed>=radius*abs(exact(sweep['angle_radians'])),'speed bound underestimates radius times angle')
    adapted=copy.deepcopy(report);adapted['sweep']=adapted['rotation_sweep'];adapted['sweep']['translation']=[sweep['speed_bound_mm'],0,0]
    for sample in adapted['sweep']['samples']:sample['translation_uncertainty_mm']=sample.pop('rotation_uncertainty_mm')
    return translation.audit(adapted)


def at_pose(name,p,time):
    angle=p.get('angle',math.tau)*time
    fixed=p.get('guard_radius',30) if name=='tool_guard' else 20
    radial=max(0.,math.hypot(20*math.cos(angle),20*math.sin(angle)-fixed)-5)
    return math.hypot(radial,p.get('axial_gap',6) if name=='tool_overhead' else 0.)


def compare_samples(output,report,name,p):
    from OCP.gp import gp_Trsf,gp_Ax1,gp_Pnt,gp_Dir
    from OCP.TopLoc import TopLoc_Location
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopAbs import TopAbs_FACE
    leaves=convex.imported_leaves(output/'rotation_clearance.step');left=leaves[tuple(report['left']['instance_path'])]['shape'];right=leaves[tuple(report['right']['instance_path'])]['shape']
    matrix=convex.placement_matrix(report['right']['frame']);pivot=[matrix[i][3]-20*matrix[i][0] for i in range(3)];axis=[matrix[i][2] for i in range(3)]
    world=report['right_rotation_world']
    for key,expected in [('pivot',pivot),('axis',axis)]:
        for a,b in zip(world[key],expected):convex.close(a,b,1e-10)
    convex.close(world['angle_radians'],p.get('angle',math.tau),1e-14)
    for side in ['left','right']:
        for row,wanted in zip(convex.placement_matrix(report[side]['frame']),leaves[tuple(report[side]['instance_path'])]['matrix']):
            for a,b in zip(row,wanted):convex.close(a,b,1e-8)
    samples=report['rotation_sweep']['samples'];chosen={min(range(len(samples)),key=lambda i:abs(samples[i]['time']-t)) for t in [0,.25,.5,.75,1]};evidence=[]
    for index in sorted(chosen):
        sample=samples[index];time=sample['time'];transform=gp_Trsf();transform.SetRotation(gp_Ax1(gp_Pnt(*pivot),gp_Dir(*axis)),p.get('angle',math.tau)*time)
        moved=right.Moved(TopLoc_Location(transform));actual=convex.distance(left,moved);expected=at_pose(name,p,time);convex.close(actual,expected,2e-6)
        convex.require(sample['lower_bound_mm']-2e-6<=actual<=sample['upper_bound_mm']+2e-6,'rotated STEP distance outside sample interval')
        surfaces=[]
        for side,shape in [('left',left),('right',moved)]:
            explorer=TopExp_Explorer(shape,TopAbs_FACE);values=[]
            while explorer.More():values.append(convex.witness_distance(sample['query'][side+'_point'],explorer.Current()));explorer.Next()
            surfaces.append(min(values));convex.require(min(values)<=2e-6,'rotated witness off imported boundary')
        evidence.append(dict(time=time,imported_distance_mm=actual,analytic_distance_mm=expected,boundary_errors_mm=surfaces))
    return evidence


def main():
    import argparse,importlib.metadata,json,subprocess
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=convex.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False);output=args.out_dir/'generation';recipe=convex.ROOT/'examples/rotation_clearance.star';cases=[]
    configurations=[dict(angle=angle,pose=pose) for angle in [math.tau,-math.tau,math.pi/2,-math.pi/2] for pose in range(3)]
    configurations += [dict(guard_radius=r,axial_gap=g) for r,g in [(26,1),(35,3),(40,10)]]
    for p in configurations:
        result=convex.run(args.cad,recipe,output,p);convex.require(result.returncode==0,'safe rotation build failed: '+result.stderr)
        summary=json.loads(result.stdout);checks=summary['clearance_checks'];measurements={}
        for name in ['tool_guard','tool_overhead']:
            report=checks[name];evidence=audit(report);angle=p.get('angle',math.tau)
            minimum=orbit_minimum(20,p.get('guard_radius',30),angle) if name=='tool_guard' else math.hypot(orbit_minimum(20,20,angle),p.get('axial_gap',6))
            convex.require(report['lower_bound_mm']<=minimum<=report['upper_bound_mm'],'analytic whole-rotation minimum outside interval')
            evidence.update(analytic_path_minimum_mm=minimum,representative_poses=compare_samples(output,report,name,p));measurements[name]=evidence
        verify=subprocess.run([str(args.cad),'verify',str(output)],capture_output=True,text=True);convex.require(verify.returncode==0 and json.loads(verify.stdout)['valid'],'generation integrity')
        cases.append(dict(parameters=p,measurements=measurements))
    rejected=[]
    for p in [dict(guard_radius=20),dict(guard_radius=20,angle=-math.tau),dict(axial_gap=0),dict(required_clearance=100)]:
        before=convex.inventory(output);result=convex.run(args.cad,recipe,output,p);convex.require(result.returncode!=0,'unsafe rotation passed');convex.require(convex.inventory(output)==before,'failed rotation changed publication');rejected.append(dict(parameters=p,diagnostic=json.loads(result.stderr),generation_preserved=True))
    report=dict(valid=True,cases=cases,rejected=rejected,ocp=importlib.metadata.version('cadquery-ocp'),scope='continuous fixed-axis rigid rotation with complete Lipschitz interval coverage; no combined screw motion or deformation')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':main()
