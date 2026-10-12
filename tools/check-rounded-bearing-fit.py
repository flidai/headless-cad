#!/usr/bin/env python3
"""Independent STEP acceptance for a keyed shaft inside a rounded through bore.

OCCT is a test-only geometry oracle. Native certificates, not sampled mesh gaps,
are the product evidence. This is nominal geometry, not a manufactured fit rating.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


f = module('bearing_fillet', 'check-longitudinal-fillet.py')
g = module('bearing_gap', 'check-keyed-clearance.py')
d = module('bearing_pose', 'check-retained-drive-module.py')
k, c = f.k, g.c
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut, BRepAlgoAPI_Common, BRepAlgoAPI_Fuse
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder
from OCP.gp import gp_Ax2, gp_Pnt, gp_Dir


def references(parameters):
    radius = parameters.get('shaft_radius', 6)
    gap = parameters.get('bore_gap', .15)
    housing = f.filleted([(-20,-15),(20,-15),(20,15),(-20,15)],12,4,[0,1,2,3])
    for x, bore in [(0,radius+gap),(-14,2),(14,2)]:
        tool = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x,0,0),gp_Dir(0,0,1)),bore,12).Shape()
        housing = k.boolean(BRepAlgoAPI_Cut,housing,tool)
    shaft = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0,0,-10),gp_Dir(0,0,1)),radius,40).Shape()
    if parameters.get('stepped'):
        flange = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0,0,-10),gp_Dir(0,0,1)),8,9.95).Shape()
        shaft = k.boolean(BRepAlgoAPI_Fuse,shaft,flange)
    cutter = BRepPrimAPI_MakeBox(gp_Pnt(radius-2,-2,6),4,4,8).Shape()
    shaft = k.boolean(BRepAlgoAPI_Cut,shaft,cutter)
    pose = parameters.get('pose',0)
    if pose:
        housing = d.placed(housing,[13,-7,15],[.5,.5,.5,.5])
        shaft = d.placed(shaft,[13,-7,15],[.5,.5,.5,.5])
    return {('fixture','housing'):housing,('fixture','shaft'):shaft}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=c.ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args = parser.parse_args(); args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();source=c.ROOT/'examples/rounded_bearing_fit.star'
    cases=[]
    configurations=[{},dict(pose=1),dict(shaft_radius=5,bore_gap=.3,pose=1),dict(shaft_radius=7,bore_gap=.12),dict(stepped=True,required_clearance=.025),dict(stepped=True,required_clearance=.025,pose=1)]
    for index,parameters in enumerate(configurations):
        out=args.out_dir/f'case-{index}'
        recipe=source
        arguments={key:value for key,value in parameters.items() if key!='stepped'}
        if parameters.get('stepped'):
            recipe=args.out_dir/f'stepped-{index}.star'
            text=source.read_text()
            original='shaft_stock = extrude(id = "shaft_stock", profile = circle(id = "shaft_profile", radius = shaft_radius), height = 40)'
            replacement='shaft_stock = revolve(id = "shaft_stock", profile = polygon(id = "shaft_profile", points = [[0,0],[8,0],[8,9.95],[6,9.95],[6,40],[0,40]]))'
            c.require(original in text,'exact stepped variant source edit')
            text=text.replace(original,replacement)
            # The standalone journal diameter in the unmodified source selects
            # the original cylinder wall; the stepped stock has multiple walls.
            text='\n'.join(line for line in text.splitlines() if 'section_circle_dimension(view = "shaft_journal"' not in line)+'\n'
            recipe.write_text(text)
        result=c.run(cad,recipe,out,arguments);c.require(result.returncode==0,result.stderr)
        manifest=json.loads(result.stdout)
        assembly=json.loads((out/'rounded_bearing_fit.assembly.json').read_text())
        leaves=c.imported_leaves(out/'rounded_bearing_fit.step');refs=references(parameters)
        c.require(set(leaves)==set(refs),'two exact source occurrence paths')
        differences={}
        for path,reference in refs.items():
            actual=leaves[path]['shape']
            values=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(actual,reference),(reference,actual)]]
            for value in values:d.close(value,0,'independent actual solid difference',absolute=2e-6)
            differences['/'.join(path)]=values
        check=manifest['clearance_checks']['running_gap']
        c.require(check['status']=='passed' and check['query']['method']=='bore_trim_witness','nested bore certificate')
        evidence=g.compare(check,leaves,assembly,.05 if parameters.get('stepped') else parameters.get('bore_gap',.15))
        shapes=[leaf['shape'] for leaf in leaves.values()]
        d.close(k.properties(k.boolean(BRepAlgoAPI_Common,*shapes))[0],0,'no material overlap',absolute=2e-6)
        replay=args.out_dir/f'replay-{index}'
        result=c.run(cad,out/'recipe.star',replay,arguments);c.require(result.returncode==0,result.stderr)
        c.require((replay/'rounded_bearing_fit.step').read_bytes()==(out/'rounded_bearing_fit.step').read_bytes(),'captured source exact STEP replay')
        verified=k.command(cad,'verify',out);c.require(verified.returncode==0,verified.stderr)
        cases.append(dict(parameters=parameters,clearance=evidence,symmetric_difference_mm3=differences,
                          kernel_build_id=manifest['kernel_build_id'],executor_build_id=manifest['executor_build_id']))
    out=args.out_dir/'preserved'
    result=c.run(cad,source,out,{});c.require(result.returncode==0,result.stderr);before=c.inventory(out)
    rejected=[]
    for parameters in [dict(required_clearance=.3),dict(bore_gap=0)]:
        result=c.run(cad,source,out,parameters);c.require(result.returncode!=0,'invalid requested gap must fail')
        c.require(c.inventory(out)==before,'failed edit preserves publication')
        rejected.append(dict(parameters=parameters,error=json.loads(result.stderr)))
    original=source.read_text()
    for name,replacement in [('eccentric','offset = [0.1,0,-10]'),('tilted','offset = [0,0,-10], rotation = [0,0.008726535498374,0,0.999961923064171]')]:
        text=original.replace('part = shaft, offset = [0,0,-10]', 'part = shaft, '+replacement)
        c.require(text!=original,'adversarial source edit')
        recipe=args.out_dir/(name+'.star');recipe.write_text(text)
        result=c.run(cad,recipe,out,{})
        c.require(result.returncode!=0,'unsupported alignment must refuse '+name)
        c.require(c.inventory(out)==before,'alignment failure preserves publication')
        rejected.append(dict(variant=name,error=json.loads(result.stderr)))
    report=dict(valid=True,cases=cases,rejected_edits=rejected,
                scope='Coaxial nominal running clearance; no load, manufactured fit, contact or general collision certification')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))


if __name__=='__main__':main()
