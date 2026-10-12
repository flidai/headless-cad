#!/usr/bin/env python3
"""Independent source-meridian and STEP boundary checks for coaxial clearance."""
import importlib.util
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('revolved_oracle', Path(__file__).with_name('check-revolved-clearance.py'))
revolved = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(revolved)
convex = revolved.convex


def meridian_material_distance(left, right):
    """Distance between filled, simple material polygons; not boundary-only.
    Inputs are independently constructed (radius,z) source profiles, not native
    witnesses or tessellations. Containment and segment contact return zero.
    """
    def orient(a,b,c):
        return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
    def on_segment(p,a,b):
        return orient(a,b,p)==0 and all(min(a[i],b[i])<=p[i]<=max(a[i],b[i]) for i in range(2))
    def crossing(a,b,c,d):
        ab_c,ab_d,cd_a,cd_b=orient(a,b,c),orient(a,b,d),orient(c,d,a),orient(c,d,b)
        return ((ab_c>0) != (ab_d>0) and (cd_a>0) != (cd_b>0) and all(v!=0 for v in [ab_c,ab_d,cd_a,cd_b])) or any([on_segment(c,a,b),on_segment(d,a,b),on_segment(a,c,d),on_segment(b,c,d)])
    def inside(p,polygon):
        result=False
        for a,b in zip(polygon,polygon[1:]+polygon[:1]):
            if on_segment(p,a,b):return True
            if (a[1]>p[1])!=(b[1]>p[1]) and p[0] < a[0]+(p[1]-a[1])*(b[0]-a[0])/(b[1]-a[1]):result=not result
        return result
    if any(crossing(a,b,c,d) for a,b in zip(left,left[1:]+left[:1]) for c,d in zip(right,right[1:]+right[:1])) or inside(left[0],right) or inside(right[0],left):
        return 0.
    return revolved.polygon_distance(left,right)


def compare_coaxial(output, report, assembly, analytic=None):
    import math
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopAbs import TopAbs_FACE
    leaves=convex.imported_leaves(output/'coaxial_clearance.step')
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
                independent_distance_mm=independent,independent_method='source radius-z material polygon distance' if analytic is not None else 'OCCT boundary extrema',
                comparison_allowance_mm=allowance,occt_disagrees=abs(whole-independent)>2e-6,
                native_interval_mm=[report['lower_bound_mm'],report['upper_bound_mm']],
                witness_boundary_distances_mm=surfaces,witness_distance_mm=witness)


def profiles(parameters):
    shape=parameters.get('shape',1);bore=6+parameters.get('gap',1)
    if shape==2:
        bearing=[(bore,0),(12,0),(12,20),(bore+2,20)]
        shaft=[(0,0),(6,0),(8,20),(0,20)];base=0;height=20
    else:
        bearing=[(bore,0),(12,0),(12,20),(bore,20)]
        if shape==3:bearing += [(bore,14),(bore+1,14),(bore+1,6),(bore,6)]
        shaft=[(0,0),(6,0),(6,32),(0,32)] if shape!=1 else [(0,0),(5,0),(5,4),(6,4),(6,28),(5,28),(5,32),(0,32)]
        base=-6;height=32
    sign=-1 if parameters.get('opposed',0) else 1
    shift=base+parameters.get('slide',0)+(height if sign==-1 else 0)
    shaft=[(r,sign*z+shift) for r,z in shaft]
    spacer=[(10,20+parameters.get('axial_gap',6)),(12,20+parameters.get('axial_gap',6)),(12,24+parameters.get('axial_gap',6)),(10,24+parameters.get('axial_gap',6))]
    return shaft,bearing,spacer


def main():
    import argparse
    import importlib.metadata
    import json
    import platform
    import subprocess
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=convex.ROOT/'target/release/cad')
    parser.add_argument('--recipe',type=Path,default=convex.ROOT/'examples/coaxial_clearance.star')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    output=args.out_dir/'generation';cases=[]
    configurations=[dict(gap=gap,shape=shape,pose=pose) for gap in [.75,1,2] for shape in range(4) for pose in range(3)]
    configurations += [dict(shape=shape,opposed=1,pose=pose) for shape in [0,1,3] for pose in range(3)]
    configurations += [dict(shape=shape,slide=36) for shape in range(4)]
    configurations += [dict(axial_gap=gap) for gap in [1,12]]
    for parameters in configurations:
        result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode==0,'positive coaxial clearance failed: '+result.stderr)
        summary=json.loads(result.stdout);assembly=json.loads((output/'coaxial_clearance.assembly.json').read_text())
        shaft,bearing,spacer=profiles(parameters)
        expected={'shaft_bore':meridian_material_distance(shaft,bearing),'bearing_spacer':meridian_material_distance(bearing,spacer)}
        checks=summary['clearance_checks'];convex.require(set(checks)==set(expected),'both pairs required')
        measurements={}
        for name,left,right in [('shaft_bore','shaft','bearing'),('bearing_spacer','bearing','spacer')]:
            report=checks[name]
            convex.require(report['status']=='passed' and report['query']['method']=='coaxial_meridian','native method/status')
            convex.require(report['left']['instance_path']==['cell',left] and report['right']['instance_path']==['cell',right],'exact pair paths')
            measurements[name]=compare_coaxial(output,report,assembly,expected[name])
            if name=='bearing_spacer' or (parameters.get('shape',1)!=2 and parameters.get('slide',0)==0):
                exact=parameters.get('axial_gap',6) if name=='bearing_spacer' else parameters.get('gap',1)
                convex.require(report['lower_bound_mm']<=exact<=report['upper_bound_mm'],'exact radial/axial distance outside bounds')
        verify=subprocess.run([str(args.cad),'verify',str(output)],capture_output=True,text=True)
        convex.require(verify.returncode==0 and json.loads(verify.stdout)['valid'],'generation integrity')
        cases.append(dict(parameters=parameters,measurements=measurements))
    rejected=[]
    for parameters in [dict(gap=0),dict(gap=-.5),dict(oversize=1),dict(eccentric=1),dict(tilt=1),dict(axial_gap=0),dict(axial_gap=-1),dict(required_clearance=100),dict(shape=2,opposed=1)]:
        before=convex.inventory(output);result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode!=0,'unsupported/contact requirement passed')
        convex.require(before==convex.inventory(output),'failed publication changed prior generation')
        diagnostic=json.loads(result.stderr)
        convex.require('shaft_bore' in json.dumps(diagnostic) or 'bearing_spacer' in json.dumps(diagnostic),'check identity absent')
        rejected.append(dict(parameters=parameters,diagnostic=diagnostic,generation_preserved=True))
    report=dict(valid=True,cases=cases,rejected=rejected,python=platform.python_version(),ocp=importlib.metadata.version('cadquery-ocp'),scope='coaxial radius-z material separation; witness direction is not a global separating-plane certificate')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':
    main()
