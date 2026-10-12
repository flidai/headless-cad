#!/usr/bin/env python3
"""Independent offset finite-annulus distance and imported STEP witnesses."""
import importlib.util
from pathlib import Path

SPEC=importlib.util.spec_from_file_location('coaxial_oracle',Path(__file__).with_name('check-coaxial-clearance.py'))
coaxial=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coaxial)
convex=coaxial.convex


def annular_distance(inner_a, outer_a, z_a, inner_b, outer_b, z_b, offset):
    """Closed radial annuli × finite axial intervals; independently exact formula.
    Overlapping radial sets include off-center circle intersections, not merely
    overlap of their restrictions to the line joining the two centers.
    """
    import math
    d=math.hypot(*offset)
    radial=max(0.,d-outer_a-outer_b,inner_a-d-outer_b,inner_b-d-outer_a)
    axial=max(0.,z_a[0]-z_b[1],z_b[0]-z_a[1])
    return math.hypot(radial,axial)


def compare_parallel(output, report, assembly, analytic=None):
    import math
    from OCP.TopExp import TopExp_Explorer
    from OCP.TopAbs import TopAbs_FACE
    leaves=convex.imported_leaves(output/'parallel_clearance.step')
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
                independent_distance_mm=independent,independent_method='independent offset-annuli radial/axial interval formula' if analytic is not None else 'OCCT boundary extrema',
                comparison_allowance_mm=allowance,occt_disagrees=abs(whole-independent)>2e-6,
                native_interval_mm=[report['lower_bound_mm'],report['upper_bound_mm']],
                witness_boundary_distances_mm=surfaces,witness_distance_mm=witness)


def expected_distances(p):
    slide=p.get('slide',0);axial=p.get('axial_gap',6)
    return {'eccentric_shaft':annular_distance(p.get('shaft_inner',0),p.get('shaft_radius',6),[-6+slide,26+slide],7,12,[0,20],[p.get('eccentric',.25),p.get('lateral',0)]),
            'offset_sleeve':annular_distance(7,12,[0,20],10,12,[20+axial,24+axial],[p.get('spacer_x',8),0])}


def main():
    import argparse,importlib.metadata,json,platform,subprocess
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=convex.ROOT/'target/release/cad')
    parser.add_argument('--recipe',type=Path,default=convex.ROOT/'examples/parallel_clearance.star')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    output=args.out_dir/'generation';cases=[]
    configurations=[dict(eccentric=e,pose=pose) for e in [.1,.25,.75] for pose in range(3)]
    configurations += [dict(lateral=.25),dict(eccentric=24),dict(shaft_inner=2),dict(shaft_radius=20,shaft_inner=14,eccentric=1),dict(slide=36)]
    configurations += [dict(opposed=1,spacer_opposed=1,pose=pose) for pose in range(3)]
    configurations += [dict(spacer_x=x,axial_gap=z) for x in [8,20,30] for z in [1,6,12]]
    configurations += [dict(spacer_x=30,axial_gap=-1),dict(eccentric=0)]
    for parameters in configurations:
        result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode==0,'positive parallel clearance failed: '+result.stderr)
        summary=json.loads(result.stdout);assembly=json.loads((output/'parallel_clearance.assembly.json').read_text())
        expected=expected_distances(parameters);checks=summary['clearance_checks']
        convex.require(set(checks)==set(expected),'both declared pairs required');measured={}
        for name,left,right in [('eccentric_shaft','shaft','bearing'),('offset_sleeve','bearing','spacer')]:
            report=checks[name];method='coaxial_meridian' if name=='eccentric_shaft' and parameters.get('eccentric',.25)==0 and parameters.get('lateral',0)==0 else 'parallel_annular'
            convex.require(report['status']=='passed' and report['query']['method']==method,'method/status mismatch')
            convex.require(report['left']['instance_path']==['cell',left] and report['right']['instance_path']==['cell',right],'exact paths')
            measured[name]=compare_parallel(output,report,assembly,expected[name])
            # Integer and exactly binary fractional fixtures must be enclosed
            # with no importer comparison allowance.
            exact=(name=='offset_sleeve' and parameters.get('spacer_x',8)<=24 and parameters.get('axial_gap',6)>0) or (name=='eccentric_shaft' and parameters.get('lateral',0)==0 and parameters.get('slide',0)==0 and parameters.get('eccentric',.25) in [0,.25,.75,1,24])
            if exact:convex.require(report['lower_bound_mm']<=expected[name]<=report['upper_bound_mm'],'exact radial/axial value outside native interval')
        verify=subprocess.run([str(args.cad),'verify',str(output)],capture_output=True,text=True)
        convex.require(verify.returncode==0 and json.loads(verify.stdout)['valid'],'generation integrity')
        cases.append(dict(parameters=parameters,measurements=measured))
    rejected=[]
    for parameters in [dict(eccentric=1),dict(eccentric=2),dict(shaft_radius=13),dict(tilt=1),dict(axial_gap=0),dict(axial_gap=-1),dict(required_clearance=100)]:
        before=convex.inventory(output);result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode!=0,'unsupported/contact check unexpectedly passed')
        convex.require(before==convex.inventory(output),'failed publication modified previous generation')
        diagnostic=json.loads(result.stderr);convex.require('eccentric_shaft' in json.dumps(diagnostic) or 'offset_sleeve' in json.dumps(diagnostic),'check identity missing')
        rejected.append(dict(parameters=parameters,diagnostic=diagnostic,generation_preserved=True))
    report=dict(valid=True,cases=cases,rejected=rejected,python=platform.python_version(),ocp=importlib.metadata.version('cadquery-ocp'),scope='parallel finite annular material, including offset circular footprints; no tilted hollow or continuous-motion claim')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':main()
