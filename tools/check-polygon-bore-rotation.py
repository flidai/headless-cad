#!/usr/bin/env python3
"""Independent OCCT acceptance for a polygon plate's coaxial through-bore proof.

Generated Starlark sources and independent OCCT solids exercise a smaller domain
than the drive assembly. Full-turn native proof is checked separately from sampled
OCCT poses; sampled poses alone cannot establish continuous clearance.
"""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import subprocess
import sys


ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location('retained_drive_oracle',ROOT/'tools/check-retained-drive-module.py')
d=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(d)
k=d.k;c=d.coupling.convex
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common,BRepAlgoAPI_Cut  # noqa: E402
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform  # noqa: E402
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder  # noqa: E402
from OCP.gp import gp_Ax1,gp_Dir,gp_Pnt,gp_Quaternion,gp_Trsf,gp_Vec  # noqa: E402


OUTLINE=[[-24,-18],[24,-18],[24,12],[18,18],[-18,18],[-24,12]]
CONCAVE=[[-24,-18],[24,-18],[24,12],[18,18],[0,10],[-18,18],[-24,12]]
OBLIQUE=[.1,.2,.3,math.sqrt(.86)]


def require(condition,message):
    if not condition:raise AssertionError(message)


def close(actual,expected,label,absolute=2e-6):
    require(math.isfinite(actual) and abs(actual-expected)<=max(absolute,abs(expected)*2e-9),
            f'{label}: {actual} versus {expected}')


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source_text(shaft_kind='plain',pose='identity',variant='valid'):
    require(shaft_kind in ('plain','stepped') and pose in ('identity','oblique'),
            'fixture variant')
    outline=CONCAVE if variant=='concave_outer' else OUTLINE
    bore=6 if variant=='contact' else 9
    side_x=10.5 if variant=='tangent_other_bore' else 17
    inner='holes = [[[12,-2],[14,-2],[14,2],[12,2]]], ' if variant=='inner_polygon_hole' else ''
    offset=.25 if variant=='offset_axis' else 0
    required={'plain':2.9,'stepped':2.4}[shaft_kind]
    if variant=='excessive_requirement':required=3.1 if shaft_kind=='plain' else 2.6
    quaternion=[0,0,0,1] if pose=='identity' else OBLIQUE
    translation=[0,0,0] if pose=='identity' else [11,-7,13]
    stock=(f'shaft_stock = extrude(id="shaft_stock", profile=circle(id="shaft_profile", radius=6), height=7)\n'
           f'shaft_body = translate(id="shaft_body", body=shaft_stock, offset=[0,0,-3])') if shaft_kind=='plain' else (
           'shaft_body = revolve(id="shaft_body", profile=polygon(id="shaft_profile", '
           'points=[[0,-3],[6.5,-3],[6.5,1],[5.5,1],[5.5,4],[0,4]]))')
    return f'''# Generated independent polygon-bore clearance fixture: {shaft_kind}, {pose}, {variant}.
plate_body = extrude(id="plate_body", profile=polygon(id="plate_profile",
    points={json.dumps(outline)}, {inner}circular_holes=[
        {{"center":[0,0],"radius":{bore}}},
        {{"center":[-{side_x},0],"radius":1.5}},
        {{"center":[{side_x},0],"radius":1.5}},
    ]), height=3)
{stock}
plate = part(id="plate", body=plate_body, part_number="PB-PLATE", material="Aluminum", density_kg_per_m3=2700)
shaft = part(id="shaft", body=shaft_body, part_number="PB-SHAFT", material="Steel", density_kg_per_m3=7800)
pair = assembly(id="pair", instances=[instance(id="plate",part=plate),
    instance(id="shaft",part=shaft,offset=[{offset},0,0])])
scene = assembly(id="scene",instances=[assembly_instance(id="fixture",assembly=pair,
    rotation={json.dumps(quaternion)},offset={json.dumps(translation)})])
assembly_output(name="polygon_bore_pair",assembly=scene)
output(name="plate",feature=plate_body)
output(name="shaft",feature=shaft_body)
clearance_check(id="plate_shaft_gap",output="polygon_bore_pair",
    left_instance_path=["fixture","plate"],right_instance_path=["fixture","shaft"],
    min_clearance={required},rotation_group="rotor")
rigid_rotation_group(id="rotor",output="polygon_bore_pair",
    instance_paths=[["fixture","shaft"]],reference_instance_path=["fixture","shaft"],
    pivot=[0,0,0],axis=[0,0,1],angle_radians={math.tau})
'''


def command(cad,*arguments):
    return subprocess.run([str(cad),*map(str,arguments)],capture_output=True,text=True)


def reference(shaft_kind,pose):
    plate=d.bored_polygon([tuple(point) for point in OUTLINE],3,
                          [(0,0,9),(-17,0,1.5),(17,0,1.5)])
    if shaft_kind=='plain':
        shaft=d.placed(BRepPrimAPI_MakeCylinder(6,7).Shape(),[0,0,-3])
    else:
        shaft=d.retained.revolve([[0,-3],[6.5,-3],[6.5,1],[5.5,1],[5.5,4],[0,4]])
    if pose=='oblique':
        plate=d.placed(plate,[11,-7,13],OBLIQUE)
        shaft=d.placed(shaft,[11,-7,13],OBLIQUE)
    return {('fixture','plate'):plate,('fixture','shaft'):shaft}


def world_rotation(pose):
    if pose=='identity':return [0,0,0],[0,0,1]
    transform=gp_Trsf();transform.SetRotation(gp_Quaternion(*OBLIQUE))
    axis=gp_Vec(0,0,1);axis.Transform(transform)
    return [11,-7,13],[axis.X(),axis.Y(),axis.Z()]


def rotated(shape,pivot,axis,angle):
    transform=gp_Trsf();transform.SetRotation(gp_Ax1(gp_Pnt(*pivot),gp_Dir(*axis)),angle)
    return BRepBuilderAPI_Transform(shape,transform,True).Shape()


def polygon_area(points):
    return abs(sum(x*y2-x2*y for (x,y),(x2,y2) in
                   zip(points,points[1:]+points[:1])))/2


def accepted_case(cad,root,name,shaft_kind,pose):
    source=source_text(shaft_kind,pose)
    recipe=root/(name+'.star');recipe.write_text(source)
    output=root/name
    result=command(cad,'build',recipe,'--out-dir',output)
    require(result.returncode==0,name+' build: '+result.stderr)
    verify=command(cad,'verify',output)
    require(verify.returncode==0,name+' verify: '+verify.stderr)
    manifest=json.loads((output/'manifest.json').read_text())
    check=manifest['clearance_checks']['plate_shaft_gap']
    group=json.loads((output/'rigid-rotation-groups.json').read_text())['groups']['rotor']
    expected_gap=3 if shaft_kind=='plain' else 2.5
    require(check['status']=='passed' and check['motion_proof']=='continuous_rotation',
            name+' continuous native proof')
    require(check['query']['method']=='bore_trim_witness',name+' native bore witness')
    sweep=check['rotation_sweep']
    require(sweep['proof_kind']=='bore_witness_orbit' and sweep['query_count']==3,
            name+' full-turn bore witness orbit')
    orbit=sweep['bore_orbit']
    require(orbit['start']==0 and orbit['end']==1 and sweep['intervals']==[] and
            orbit['guarded_radius_mm']>=orbit['bore_radius_mm'] and
            orbit['axis_offset_mm']<1e-6 and
            orbit['axis_direction_departure']<1e-6 and
            orbit['distance_variation_bound_mm']>0,
            name+' explicit complete bore/stock orbit')
    close(orbit['bore_radius_mm'],9,name+' selected actual bore radius')
    close(orbit['shaft_stock_outer_radius_mm'],6 if shaft_kind=='plain' else 6.5,
          name+' raw stepped/circular stock outer radius')
    require(check['lower_bound_mm']>=check['declaration']['min_clearance'],
            name+' requirement certified')
    require(check['lower_bound_mm']<=expected_gap<=check['upper_bound_mm'],
            name+' native interval includes independent exact gap')
    require(len(group['members'])==1 and group['coverage']=='named_pairs_only',
            name+' source-bound named-pair scope')
    pivot,axis=world_rotation(pose)
    for observed,expected in zip(group['rotation_world']['pivot'],pivot):
        close(observed,expected,name+' world pivot',1e-8)
    for observed,expected in zip(group['rotation_world']['axis'],axis):
        close(observed,expected,name+' world axis',1e-8)
    leaves=c.imported_leaves(output/'polygon_bore_pair.step')
    references=reference(shaft_kind,pose)
    require(set(leaves)==set(references),name+' two exact STEP leaves')
    differences={}
    for path,wanted in references.items():
        actual=leaves[path]['shape']
        pair=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
              for a,b in ((actual,wanted),(wanted,actual))]
        require(max(abs(x) for x in pair)<3e-6,name+' directed STEP difference '+str(path))
        differences['/'.join(path)]=pair
    expected_plate_volume=3*(polygon_area(OUTLINE)-math.pi*(9**2+2*1.5**2))
    close(k.properties(references[('fixture','plate')])[0],expected_plate_volume,
          name+' closed-form plate volume')
    samples=[]
    for phase in [0,.125,.25,.375,.5,.625,.75,.875,1]:
        turned=rotated(leaves[('fixture','shaft')]['shape'],pivot,axis,math.tau*phase)
        gap=c.distance(leaves[('fixture','plate')]['shape'],turned)
        close(gap,expected_gap,name+' independent OCCT phase '+str(phase),3e-6)
        common=k.boolean(BRepAlgoAPI_Common,leaves[('fixture','plate')]['shape'],turned)
        close(k.properties(common)[0],0,name+' sampled intersection',3e-6)
        samples.append({'phase':phase,'occt_distance_mm':gap})
    return {'case':name,'shaft_kind':shaft_kind,'pose':pose,
            'source_sha256':hashlib.sha256(source.encode()).hexdigest(),
            'source_text':source,'exact_gap_mm':expected_gap,
            'native_lower_bound_mm':check['lower_bound_mm'],
            'native_upper_bound_mm':check['upper_bound_mm'],
            'proof_kind':sweep['proof_kind'],'occt_directed_differences_mm3':differences,
            'sampled_phases':samples}


def rejected_cases(cad,root,published):
    baseline=c.inventory(published)
    variants=['excessive_requirement','contact','offset_axis','concave_outer',
              'inner_polygon_hole','tangent_other_bore']
    failures=[]
    for variant in variants:
        source=source_text('plain','identity',variant)
        recipe=root/(variant+'.star');recipe.write_text(source)
        result=command(cad,'build',recipe,'--out-dir',published)
        require(result.returncode!=0,'unsafe polygon-bore case accepted: '+variant)
        require(c.inventory(published)==baseline,
                'failed polygon-bore build modified published bytes: '+variant)
        try:diagnostic=json.loads(result.stderr)
        except json.JSONDecodeError as error:
            raise AssertionError('non-JSON refusal '+variant+': '+result.stderr) from error
        failures.append({'case':variant,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),
                         'source_text':source,'diagnostic':diagnostic})
    return failures


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args()
    cad=args.cad.resolve();root=args.out_dir.resolve();root.mkdir(parents=True,exist_ok=False)
    binary_hash,script_hash=sha256(cad),sha256(__file__)
    cases=[accepted_case(cad,root,*case) for case in [
        ('plain_identity','plain','identity'),
        ('stepped_identity','stepped','identity'),
        ('stepped_oblique','stepped','oblique'),
    ]]
    refused=rejected_cases(cad,root,root/'plain_identity')
    require(sha256(cad)==binary_hash and sha256(__file__)==script_hash,
            'pinned executable/oracle changed during run')
    report={'valid':True,'executable_sha256':binary_hash,'oracle_script_sha256':script_hash,
            'python_version':platform.python_version(),
            'ocp_distribution':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'rejected_cases':refused,
            'scope':'Native full-turn named-pair proof; independent exact OCCT solids and sampled phases. No general collision, fastener, load or manufacturing certificate.'}
    (root/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refused':len(refused),
                      'report':str(root/'report.json')},indent=2))


if __name__=='__main__':main()
