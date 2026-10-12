#!/usr/bin/env python3
"""Independent audit of native Lipschitz-certified translation clearance."""
from fractions import Fraction
import importlib.util
import math
from pathlib import Path

SPEC=importlib.util.spec_from_file_location('parallel_oracle',Path(__file__).with_name('check-parallel-clearance.py'))
parallel=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(parallel)
convex=parallel.convex


def exact(value):
    return Fraction.from_float(float(value))


def covers_unit_interval(intervals):
    """Reject gaps, overlap and zero-width leaves; audit dyadic arithmetic exactly."""
    ordered=sorted((exact(a),exact(b)) for a,b in intervals)
    convex.require(bool(ordered) and ordered[0][0]==0 and ordered[-1][1]==1,'coverage endpoints')
    for index,(a,b) in enumerate(ordered):
        convex.require(0<=a<b<=1,'valid interval extent')
        for t in [a,b]:
            convex.require(t.denominator & (t.denominator-1)==0,'dyadic interval endpoint')
        if index:convex.require(ordered[index-1][1]==a,'coverage gap or overlap')
    return ordered


def audit(report):
    sweep=report['sweep'];samples=sweep['samples'];intervals=sweep['intervals']
    convex.require(sweep['query_count']==len(samples),'query accounting')
    covers_unit_interval([(i['start'],i['end']) for i in intervals])
    times=[s['time'] for s in samples];convex.require(0 in times and 1 in times,'endpoint evidence')
    speed_squared=sum(exact(v)**2 for v in sweep['translation'])
    for interval in intervals:
        a,b=exact(interval['start']),exact(interval['end']);mid=exact(interval['midpoint'])
        convex.require(mid==(a+b)/2,'dyadic midpoint')
        sample=samples[interval['sample_index']];convex.require(exact(sample['time'])==mid,'interval sample identity')
        radius=exact(interval['displacement_bound_mm'])
        convex.require(radius>=0 and radius**2>=speed_squared*((b-a)/2)**2,'Lipschitz displacement underbound')
        convex.require(exact(interval['lower_bound_mm'])<=exact(sample['lower_bound_mm'])-radius,'interval subtraction not outward')
        convex.require(interval['lower_bound_mm']>=sweep['required_minimum_mm'],'uncertified interval admitted')
    for sample in samples:
        uncertainty=exact(sample['translation_uncertainty_mm'])
        convex.require(uncertainty>=0,'negative pose uncertainty')
        convex.require(exact(sample['lower_bound_mm'])<=exact(sample['query']['lower_bound_mm'])-uncertainty,'sample lower not widened')
        convex.require(exact(sample['upper_bound_mm'])>=exact(sample['query']['upper_bound_mm'])+uncertainty,'sample upper not widened')
    convex.require(sweep['lower_bound_mm']<=min(i['lower_bound_mm'] for i in intervals),'coverage minimum underived')
    convex.require(sweep['upper_bound_mm']>=min(s['upper_bound_mm'] for s in samples),'sample upper minimum underived')
    convex.require(report['lower_bound_mm']<=sweep['lower_bound_mm'] and report['upper_bound_mm']>=sweep['upper_bound_mm'],'path placement widening')
    return dict(samples=len(samples),coverage_intervals=len(intervals))


def expected_distance(name,p,time):
    if name=='carriage_pass':
        x=25+p.get('traverse',20)*time
        return math.hypot(max(35-(x+4),x-39,0),max(p.get('pass_y',10)-4,0))
    coaxial=parallel.coaxial
    shaft,bearing,_=coaxial.profiles(dict(shape=1,gap=p.get('bore_gap',1),opposed=p.get('opposed',0),slide=p.get('travel',40)*time*(-1 if p.get('opposed',0) else 1)))
    return coaxial.meridian_material_distance(shaft,bearing)


def compare_samples(output,report,name,p):
    from OCP.gp import gp_Trsf,gp_Vec
    from OCP.TopLoc import TopLoc_Location
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopAbs import TopAbs_FACE
    leaves=convex.imported_leaves(output/'translation_clearance.step')
    left=leaves[tuple(report['left']['instance_path'])]['shape'];right=leaves[tuple(report['right']['instance_path'])]['shape']
    local=[0,0,p.get('travel',40)] if name=='shaft_travel' else [p.get('traverse',20),0,0]
    matrix=convex.placement_matrix(report['right']['frame'])
    world=[sum(matrix[i][j]*local[j] for j in range(3)) for i in range(3)]
    for a,b in zip(world,report['right_translation_world']):convex.close(a,b,1e-10)
    for side in ['left','right']:
        for row,wanted in zip(convex.placement_matrix(report[side]['frame']),leaves[tuple(report[side]['instance_path'])]['matrix']):
            for a,b in zip(row,wanted):convex.close(a,b,1e-8)
    samples=report['sweep']['samples'];chosen={min(range(len(samples)),key=lambda i:abs(samples[i]['time']-t)) for t in [0,.5,1]};evidence=[]
    for index in sorted(chosen):
        sample=samples[index];time=sample['time'];transform=gp_Trsf();transform.SetTranslation(gp_Vec(*(v*time for v in world)))
        moved=right.Moved(TopLoc_Location(transform));actual=convex.distance(left,moved);expected=expected_distance(name,p,time)
        convex.close(actual,expected,2e-6)
        convex.require(sample['lower_bound_mm']-2e-6<=actual<=sample['upper_bound_mm']+2e-6,'STEP pose distance outside sample bounds')
        surfaces=[]
        for side,shape in [('left',left),('right',moved)]:
            explorer=TopExp_Explorer(shape,TopAbs_FACE);values=[]
            while explorer.More():values.append(convex.witness_distance(sample['query'][side+'_point'],explorer.Current()));explorer.Next()
            surfaces.append(min(values));convex.require(min(values)<=2e-6,'sample witness off imported boundary')
        evidence.append(dict(time=time,imported_distance_mm=actual,analytic_distance_mm=expected,boundary_errors_mm=surfaces))
    return evidence


def main():
    import argparse,importlib.metadata,json,subprocess
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=convex.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False);output=args.out_dir/'generation';recipe=convex.ROOT/'examples/translation_clearance.star';cases=[]
    configurations=[dict(travel=travel,pose=pose) for travel in [-40,10,40] for pose in range(3)]
    configurations += [dict(pass_y=y,traverse=t) for y in [8,10,12] for t in [20,40]]
    configurations += [dict(opposed=1,pose=pose) for pose in range(3)]+[dict(traverse=-20)]
    for p in configurations:
        result=convex.run(args.cad,recipe,output,p);convex.require(result.returncode==0,'safe translation build failed: '+result.stderr)
        summary=json.loads(result.stdout);checks=summary['clearance_checks'];measurements={}
        for name in ['shaft_travel','carriage_pass']:
            report=checks[name];evidence=audit(report)
            minimum=p.get('bore_gap',1) if name=='shaft_travel' else (p.get('pass_y',10)-4 if p.get('traverse',20)>=6 else math.hypot(6,p.get('pass_y',10)-4))
            convex.require(report['lower_bound_mm']<=minimum<=report['upper_bound_mm'],'analytic whole-path minimum outside interval')
            evidence.update(analytic_path_minimum_mm=minimum,representative_poses=compare_samples(output,report,name,p));measurements[name]=evidence
        verify=subprocess.run([str(args.cad),'verify',str(output)],capture_output=True,text=True);convex.require(verify.returncode==0 and json.loads(verify.stdout)['valid'],'published integrity')
        cases.append(dict(parameters=p,measurements=measurements))
    rejected=[]
    for p in [dict(pass_y=0,traverse=40),dict(required_clearance=.9999),dict(travel=0),dict(bore_gap=.25)]:
        before=convex.inventory(output);result=convex.run(args.cad,recipe,output,p);convex.require(result.returncode!=0,'unsafe/unresolved path passed');convex.require(convex.inventory(output)==before,'failed path modified publication');rejected.append(dict(parameters=p,diagnostic=json.loads(result.stderr),generation_preserved=True))
    report=dict(valid=True,cases=cases,rejected=rejected,ocp=importlib.metadata.version('cadquery-ocp'),scope='continuous rigid straight translation certified by complete Lipschitz interval coverage; not rotation or deformation')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':main()
