#!/usr/bin/env python3
"""Independent STEP/OCCT convex-revolution clearance acceptance (test-only OCP)."""
import importlib.util
from pathlib import Path

HELPER = Path(__file__).with_name('check-cylinder-clearance.py')
SPEC = importlib.util.spec_from_file_location('cylinder_clearance_oracle', HELPER)
cylinder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cylinder)
convex = cylinder.convex
compare_pair = cylinder.compare_pair


def polygon_distance(left, right):
    import math
    def point_segment(p,a,b):
        v=[b[i]-a[i] for i in range(2)];length=sum(x*x for x in v)
        t=max(0.,min(1.,sum((p[i]-a[i])*v[i] for i in range(2))/length)) if length else 0.
        return math.hypot(*(p[i]-a[i]-t*v[i] for i in range(2)))
    return min(point_segment(p,polygon[i],polygon[(i+1)%len(polygon)])
               for points,polygon in [(left,right),(right,left)] for p in points for i in range(len(polygon)))


def profile(shape):
    radii={0:[(6,0),(3,20)],1:[(6,0),(0,20)],2:[(6,0),(8,10),(3,20)]}[shape]
    return [(-r,z) for r,z in radii]+[(r,z) for r,z in reversed(radii)]


def meridian_distance(shape, radial_offset, axial_offset):
    return polygon_distance(profile(shape),[(radial_offset-4,axial_offset),(radial_offset,axial_offset+16),(radial_offset+4,axial_offset)])


def guard_distance(parameters):
    # Both convex bodies are reflection-symmetric about local x=0. Averaging
    # a closest pair and its reflection proves a minimum exists in this plane.
    import math
    angle=math.pi/6 if parameters.get('guard_turn',1) else 0.
    c,s=math.cos(angle),math.sin(angle)
    guard=[(24+y*c-z*s,y*s+z*c) for y,z in [(0,0),(10,0),(10,24),(0,24)]]
    return polygon_distance(profile(parameters.get('shape',0)),guard)


def compare_revolved(output, report, assembly, analytic=None):
    import math
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopAbs import TopAbs_FACE
    leaves=convex.imported_leaves(output/'revolved_clearance.step')
    selected={side:leaves[tuple(report[side]['instance_path'])] for side in ['left','right']}
    def faces(shape):
        result=[];explorer=TopExp_Explorer(shape,TopAbs_FACE)
        while explorer.More():result.append(explorer.Current());explorer.Next()
        return result
    face_sets={side:faces(selected[side]['shape']) for side in selected}
    whole=convex.distance(selected['left']['shape'],selected['right']['shape'])
    subshape=min(convex.distance(a,b) for a in face_sets['left'] for b in face_sets['right'])
    independent=analytic if analytic is not None else min(whole,subshape)
    allowance=1e-10 if analytic is not None else 2e-6
    for bounds in [report,report['query']]:
        convex.require(bounds['lower_bound_mm']-allowance<=independent<=bounds['upper_bound_mm']+allowance,'independent clearance outside native interval')
    surfaces={}
    for side in selected:
        identity=report[side];point=report['query'][side+'_point']
        surfaces[side]=min(convex.witness_distance(point,face) for face in face_sets[side])
        convex.require(surfaces[side]<=2e-6,'native witness off imported boundary faces')
        instances=[i for i in assembly['instances'] if tuple(assembly['instance_paths'][i['id']])==tuple(identity['instance_path'])]
        convex.require(len(instances)==1,'unique occurrence path')
        convex.require(identity['instance_id']==instances[0]['id'] and identity['part_id']==instances[0]['part'],'occurrence identity')
        for frame in [identity['frame'],instances[0]['transform']]:
            for row,wanted in zip(convex.placement_matrix(frame),selected[side]['matrix']):
                for actual,expected in zip(row,wanted):convex.close(actual,expected,1e-8)
    witness=math.dist(report['query']['left_point'],report['query']['right_point'])
    convex.close(witness,report['query']['upper_bound_mm'],2e-6)
    return dict(imported_distance_mm=whole,occt_boundary_pair_distance_mm=subshape,
                independent_distance_mm=independent,independent_method='symmetry-plane analytic polygon distance' if analytic is not None else 'OCCT boundary extrema',
                comparison_allowance_mm=allowance,occt_disagrees=abs(whole-independent)>2e-6,
                native_interval_mm=[report['lower_bound_mm'],report['upper_bound_mm']],
                witness_boundary_distances_mm=surfaces,witness_distance_mm=witness)


def main():
    import argparse
    import importlib.metadata
    import json
    import platform
    import subprocess
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=convex.ROOT/'target/release/cad')
    parser.add_argument('--recipe',type=Path,default=convex.ROOT/'examples/revolved_clearance.star')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    output=args.out_dir/'generation';cases=[]
    configurations=[(dict(shape=shape,gap=gap,pose=pose),None) for shape in [0,1,2] for gap in [5,10,20] for pose in [0,1,2]]
    configurations += [(dict(shape=shape,lateral=12,axial=10),None) for shape in [0,1,2]]
    for shape in [0,1,2]:
        for gap,axial in [(10,0),(-10,30),(10,30)]:
            parameters=dict(shape=shape,turn=0,guard_turn=0,pose=0,gap=gap,axial=axial)
            expected=meridian_distance(shape,10+gap,axial)
            configurations.append((parameters,expected))
    for parameters,expected in configurations:
        result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode==0,'positive revolved clearance failed: '+result.stderr)
        summary=json.loads(result.stdout);assembly=json.loads((output/'revolved_clearance.assembly.json').read_text())
        reports=summary['clearance_checks'];convex.require(set(reports)=={'tool_pair','tool_guard'},'both declared pairs required')
        measured={}
        for name,right in [('tool_pair','moving'),('tool_guard','guard')]:
            report=reports[name];convex.require(report['status']=='passed','native check status')
            convex.require(report['left']['instance_path']==['cell','fixed'] and report['right']['instance_path']==['cell',right],'exact pair paths')
            measured[name]=compare_revolved(output,report,assembly,guard_distance(parameters) if name=='tool_guard' else None)
        if expected is not None:
            convex.close(measured['tool_pair']['imported_distance_mm'],expected)
            convex.close(measured['tool_guard']['independent_distance_mm'],16. if parameters['shape']==2 else 18.)
            guard_exact=16. if parameters['shape']==2 else 18.
            guard_interval=measured['tool_guard']['native_interval_mm']
            convex.require(guard_interval[0] <= guard_exact <= guard_interval[1], 'exact analytic guard distance outside native interval')
            exact_pair = 10. if (parameters['gap']==-10 and parameters['axial']==30) or (parameters['shape'] in [0,1] and parameters['gap']==10 and parameters['axial']==0) else None
            if exact_pair is not None:
                pair_interval=measured['tool_pair']['native_interval_mm']
                convex.require(pair_interval[0] <= exact_pair <= pair_interval[1], 'exact analytic pair distance outside native interval')
            else:
                # Endpoint/segment projection evaluates irrational distances in
                # binary64; this comparison is an independent estimate.
                pair_interval=measured['tool_pair']['native_interval_mm']
                convex.require(pair_interval[0]-1e-10 <= expected <= pair_interval[1]+1e-10, 'independent meridian estimate outside native interval')
        verify=subprocess.run([str(args.cad),'verify',str(output)],capture_output=True,text=True)
        convex.require(verify.returncode==0 and json.loads(verify.stdout)['valid'],'generation integrity')
        cases.append(dict(parameters=parameters,measurements=measured,analytic_meridian_distance_mm=expected,analytic_projection_comparison_allowance_mm=1e-10 if expected is not None else None))
    rejected=[]
    for parameters in [dict(shape=3),dict(shape=4),dict(required_clearance=100),dict(gap=-10,turn=0)]:
        before=convex.inventory(output);result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode!=0,'unsupported/unmet check unexpectedly passed')
        convex.require(before==convex.inventory(output),'failed build modified prior publication')
        diagnostic=json.loads(result.stderr);convex.require('tool_' in json.dumps(diagnostic),'check identity absent')
        rejected.append(dict(parameters=parameters,diagnostic=diagnostic,generation_preserved=True))
    report=dict(valid=True,cases=cases,rejected=rejected,python=platform.python_version(),ocp=importlib.metadata.version('cadquery-ocp'),scope='static convex solid full-turn linear-meridian revolution separation; no hollow, nonconvex or continuous-motion claim')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':
    main()
