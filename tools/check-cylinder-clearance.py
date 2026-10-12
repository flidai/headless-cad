#!/usr/bin/env python3
"""Independent OCCT finite-cylinder clearance acceptance; OCP is test-only."""
import importlib.util
from pathlib import Path

HELPER = Path(__file__).with_name('check-convex-clearance.py')
SPEC = importlib.util.spec_from_file_location('convex_clearance_oracle', HELPER)
convex = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(convex)


def compare_pair(output, assembly_name, report, assembly, tolerance=2e-6):
    """Compare actual STEP solids and native support witnesses, never mesh bounds."""
    import math
    leaves = convex.imported_leaves(output / f'{assembly_name}.step')
    query = report['query']
    sides = {side: leaves[tuple(report[side]['instance_path'])] for side in ('left', 'right')}
    actual = convex.distance(sides['left']['shape'], sides['right']['shape'])
    for bounds in (report, query):
        convex.require(bounds['lower_bound_mm'] - tolerance <= actual <= bounds['upper_bound_mm'] + tolerance,
                       'OCCT distance outside native interval')
    witness = math.dist(query['left_point'], query['right_point'])
    convex.close(witness, query['upper_bound_mm'], tolerance)
    surfaces = {}
    for side in ('left', 'right'):
        identity = report[side]
        path = tuple(identity['instance_path'])
        surfaces[side] = convex.witness_distance(query[f'{side}_point'], sides[side]['shape'])
        convex.require(surfaces[side] <= tolerance, f'{side} witness off STEP surface')
        instances = [i for i in assembly['instances'] if tuple(assembly['instance_paths'][i['id']]) == path]
        convex.require(len(instances) == 1, 'exact occurrence path must be unique')
        instance = instances[0]
        convex.require(identity['instance_id'] == instance['id'] and identity['part_id'] == instance['part'], 'occurrence identity')
        for frame in (identity['frame'], instance['transform']):
            for row, reference in zip(convex.placement_matrix(frame), sides[side]['matrix']):
                for value, expected in zip(row, reference):
                    convex.close(value, expected, 1e-8)
    return dict(imported_distance_mm=actual, native_interval_mm=[report['lower_bound_mm'], report['upper_bound_mm']],
                witness_distance_mm=witness, witness_surface_distances_mm=surfaces,
                importer_comparison_allowance_mm=tolerance)


def main():
    import argparse
    import importlib.metadata
    import json
    import math
    import platform
    import subprocess
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=convex.ROOT/'target/release/cad')
    parser.add_argument('--recipe',type=Path,default=convex.ROOT/'examples/cylinder_clearance.star')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args()
    args.out_dir.mkdir(parents=True,exist_ok=False)
    output=args.out_dir/'generation'
    cases=[]
    configurations=[(dict(gap=gap,pose=pose),None) for gap in [3,10,20] for pose in [0,1,2]]
    configurations += [(dict(lateral=12,axial=10),None)]
    for values,expected in [(dict(gap=10),10.),(dict(gap=-10,axial=30),10.),
                            (dict(gap=10,axial=30),math.sqrt(200)),(dict(gap=6,lateral=12),10.)]:
        configurations.append((dict(turn=0,guard_turn=0,pose=0,**values),expected))
    for parameters,expected in configurations:
        result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode==0,'positive cylinder clearance build failed: '+result.stderr)
        summary=json.loads(result.stdout)
        assembly=json.loads((output/'cylinder_clearance.assembly.json').read_text())
        reports=summary['clearance_checks']
        convex.require(set(reports)=={'roller_pair','roller_guard'},'both declared pairs required')
        measurements={}
        for name,right in [('roller_pair','moving'),('roller_guard','guard')]:
            report=reports[name]
            convex.require(report['status']=='passed','native check passes')
            convex.require(report['left']['instance_path']==['cell','fixed'] and report['right']['instance_path']==['cell',right],'exact selected pair paths')
            measurements[name]=compare_pair(output,'cylinder_clearance',report,assembly)
        if expected is not None:
            convex.close(measurements['roller_pair']['imported_distance_mm'],expected)
            convex.close(measurements['roller_guard']['imported_distance_mm'],18.)
        verify=subprocess.run([str(args.cad),'verify',str(output)],capture_output=True,text=True)
        convex.require(verify.returncode==0 and json.loads(verify.stdout)['valid'],'generation integrity')
        cases.append(dict(parameters=parameters,measurements=measurements,analytic_pair_distance_mm=expected))
    failures=[]
    for parameters in [dict(tube=1),dict(gap=-10,turn=0),dict(required_clearance=100)]:
        before=convex.inventory(output)
        result=convex.run(args.cad,args.recipe,output,parameters)
        convex.require(result.returncode!=0,'unsupported/unmet clearance unexpectedly passed')
        convex.require(convex.inventory(output)==before,'failed cylinder check modified publication')
        diagnostic=json.loads(result.stderr)
        convex.require('roller_' in json.dumps(diagnostic),'typed check identity missing')
        failures.append(dict(parameters=parameters,diagnostic=diagnostic,generation_preserved=True))
    report=dict(valid=True,cases=cases,rejected=failures,python=platform.python_version(),ocp=importlib.metadata.version('cadquery-ocp'),scope='static finite solid cylinder/cylinder and cylinder/convex-box separation; no motion or hollow-cylinder claim')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':
    main()
