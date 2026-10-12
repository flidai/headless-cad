#!/usr/bin/env python3
"""Independent clearance on actual trimmed keyed-body STEP boundaries.

Containing-stock bounds are checked against independent distances of the actual
cut bodies; witness membership is checked against faces, not solid interiors.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path

SPEC=importlib.util.spec_from_file_location('convex_oracle',Path(__file__).with_name('check-convex-clearance.py'))
c=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(c)
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_FACE


def faces(shape):
    result=[];iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():result.append(iterator.Current());iterator.Next()
    return result


def compare(report,leaves,assembly,expected):
    selected={side:leaves[tuple(report[side]['instance_path'])] for side in ['left','right']}
    boundary={side:faces(leaf['shape']) for side,leaf in selected.items()}
    whole=c.distance(selected['left']['shape'],selected['right']['shape'])
    face_distance=min(c.distance(a,b) for a in boundary['left'] for b in boundary['right'])
    # Importer comparison allowance is not part of the native interval proof.
    for distance in [whole,face_distance]:c.close(distance,expected,2e-6)
    for bound in [report,report['query']]:
        c.require(bound['lower_bound_mm']<=expected<=bound['upper_bound_mm'],'analytic minimum outside published interval')
    witness={}
    for side,leaf in selected.items():
        identity=report[side];point=report['query'][side+'_point']
        witness[side]=min(c.witness_distance(point,face) for face in boundary[side])
        c.require(witness[side]<=2e-6,'native witness not on actual trimmed boundary')
        instances=[i for i in assembly['instances'] if assembly['instance_paths'][i['id']]==identity['instance_path']]
        c.require(len(instances)==1,'exact occurrence identity')
        c.require(identity['instance_id']==instances[0]['id'] and identity['part_id']==instances[0]['part'],'part identity')
        for frame in [identity['frame'],instances[0]['transform']]:
            for actual,wanted in zip(c.placement_matrix(frame),leaf['matrix']):
                for a,b in zip(actual,wanted):c.close(a,b,1e-8)
    distance=math.dist(report['query']['left_point'],report['query']['right_point'])
    c.close(distance,report['query']['upper_bound_mm'],2e-6)
    return dict(method=report['query']['method'],analytic_distance_mm=expected,occt_whole_distance_mm=whole,occt_face_pair_distance_mm=face_distance,witness_boundary_distances_mm=witness,native_interval_mm=[report['lower_bound_mm'],report['upper_bound_mm']])


def source_variant(source,variant):
    if variant=='swapped':
        import re
        source=re.sub(r'left_instance_path = (\[[^\]]+\]), right_instance_path = (\[[^\]]+\])',r'left_instance_path = \2, right_instance_path = \1',source)
    elif variant=='opposed':
        source=source.replace('pairs = assembly(', 'hub_rotation = [1,0,0,0]\npairs = assembly(')
        source=source.replace('[180*scale,0,10*scale]', '[180*scale,0,24*scale]').replace('[240*scale,0,0], rotation = hub_rotation','[240*scale,0,14*scale], rotation = hub_rotation')
    elif variant=='eccentric':source=source.replace('[180*scale,0,10*scale]','[180*scale+0.1,0,10*scale]')
    elif variant=='tilted':source=source.replace('pairs = assembly(', 'hub_rotation = [0,0.043619387365336,0,0.999048221581858]\npairs = assembly(')
    elif variant=='axial_contact':source=source.replace('[240*scale,0,16*scale]','[240*scale,0,14*scale]')
    elif variant=='malformed_trim':source=source.replace('floor = r-2*scale','floor = -scale')
    return source


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=c.ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False);cad=args.cad.resolve()
    source=(c.ROOT/'examples/keyed_clearance.star').read_text()
    configs=[(dict(pose=pose,hub_turn=turn),'standard') for pose in range(3) for turn in range(3)]
    configs += [(dict(scale=scale,pose=2),'standard') for scale in [.5,2]]
    configs += [(dict(bore_gap=.3),'standard'),(dict(shaft_radius=7),'standard'),(dict(pose=2),'swapped'),(dict(pose=2),'opposed')]
    cases=[]
    for index,(params,variant) in enumerate(configs):
        recipe=args.out_dir/f'case-{index}.star';recipe.write_text(source_variant(source,variant));output=args.out_dir/f'case-{index}'
        result=c.run(cad,recipe,output,params);c.require(result.returncode==0,result.stderr)
        summary=json.loads(result.stdout);assembly=json.loads((output/'keyed_clearance.assembly.json').read_text());leaves=c.imported_leaves(output/'keyed_clearance.step')
        reports=summary['clearance_checks'];c.require(set(reports)=={'journal_gap','seat_gap','boundary_gap','hub_gap','axial_gap'},'all declared pairs checked')
        observed={}
        for name,report in reports.items():
            c.require(report['status']=='passed' and report['query']['method']=='native_trim_witness','actual trimmed geometry method')
            expected=(2 if name=='axial_gap' else params.get('bore_gap',.15))*params.get('scale',1)
            observed[name]=compare(report,leaves,assembly,expected)
        import subprocess
        verify=subprocess.run([str(cad),'verify',str(output)],capture_output=True,text=True);c.require(verify.returncode==0,verify.stderr)
        cases.append(dict(parameters=params,variant=variant,pairs=observed))
    output=args.out_dir/'preserved';recipe=args.out_dir/'rejection.star';recipe.write_text(source)
    result=c.run(cad,recipe,output,{});c.require(result.returncode==0,result.stderr);before=c.inventory(output);failures=[]
    for params,variant in [(dict(bore_gap=0),'standard'),(dict(required_clearance=1),'standard'),({},'eccentric'),({},'tilted'),({},'axial_contact'),({},'malformed_trim')]:
        recipe.write_text(source_variant(source,variant));result=c.run(cad,recipe,output,params)
        c.require(result.returncode!=0,'expected rejected domain '+variant);c.require(c.inventory(output)==before,'rejection changed published generation')
        failures.append(dict(parameters=params,variant=variant,error=json.loads(result.stderr)))
    report=dict(valid=True,cases=cases,rejections=failures)
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))


if __name__=='__main__':main()
