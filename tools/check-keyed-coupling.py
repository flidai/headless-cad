#!/usr/bin/env python3
"""Independent nominal engagement checks, not torque/retention certification."""
import argparse
import importlib.util
import json
from pathlib import Path


def module(name,file):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file));value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

h=module('hub_keyway_oracle','check-hub-keyway.py');k=h.k
convex=module('convex_oracle','check-convex-clearance.py')
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common, BRepAlgoAPI_Cut
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder, BRepPrimAPI_MakeBox
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.gp import gp_Pnt, gp_Trsf, gp_Vec
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Plane


def placed(shape,offset):
    transform=gp_Trsf();transform.SetTranslation(gp_Vec(*offset));return BRepBuilderAPI_Transform(shape,transform,True).Shape()


def plane(shape,axis,coordinate):
    found=[];explorer=TopExp_Explorer(shape,TopAbs_FACE)
    while explorer.More():
        face=TopoDS.Face(explorer.Current());surface=BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Plane:
            support=surface.Plane();normal=support.Axis().Direction();origin=support.Location()
            if abs(abs([normal.X(),normal.Y(),normal.Z()][axis])-1)<1e-10 and abs([origin.X(),origin.Y(),origin.Z()][axis]-coordinate)<1e-7:
                found.append(face)
        explorer.Next()
    k.require(len(found)==1,('ambiguous/missing physical plane',axis,coordinate,len(found)))
    return found[0]


def case(cad,directory,index,parameters):
    output=directory/f'case-{index}'
    result=convex.run(cad,k.ROOT/'examples/keyed_coupling.star',output,parameters);k.require(result.returncode==0,result.stderr)
    r=parameters.get('shaft_radius',6);sg=parameters.get('side_gap',.02);rg=parameters.get('roof_gap',.05);width=4;w=width+2*sg;floor=r-2
    shaft=k.boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(r,40).Shape(),BRepPrimAPI_MakeBox(gp_Pnt(floor,-w/2,10),4,w,13.5).Shape())
    tube=k.boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(16,14).Shape(),BRepPrimAPI_MakeCylinder(r+.15,14).Shape())
    hub=k.boolean(BRepAlgoAPI_Cut,tube,BRepPrimAPI_MakeBox(gp_Pnt(0,-w/2,-1),r+2+rg,w,16).Shape())
    key=BRepPrimAPI_MakeBox(gp_Pnt(floor,-width/2,10.25),4,width,13).Shape()
    expected={'shaft':shaft,'hub':placed(hub,[0,0,10]),'key':key}
    volumes={'shaft':k.expected(r,40,floor,w/2,10,23.5)[0],'hub':h.expected(16,r+.15,14,r+2+rg,w/2)[0],'key':4*width*13}
    leaves=convex.imported_leaves(output/'keyed_coupling.step');k.require(set(leaves)=={(n,) for n in expected},'exact occurrence paths')
    differences={}
    for name,oracle in expected.items():
        actual=leaves[(name,)]['shape'];k.close(k.properties(actual)[0],volumes[name]);k.close(k.properties(oracle)[0],volumes[name])
        differences[name]=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(actual,oracle),(oracle,actual)]]
        for value in differences[name]:k.close(value,0)
    overlap={}
    for a,b in [('shaft','hub'),('shaft','key'),('hub','key')]:
        value=k.properties(k.boolean(BRepAlgoAPI_Common,leaves[(a,)]['shape'],leaves[(b,)]['shape']))[0];k.close(value,0);overlap[a+'-'+b]=value
    actual_shaft=leaves[('shaft',)]['shape'];actual_hub=leaves[('hub',)]['shape'];actual_key=leaves[('key',)]['shape']
    distances={'roof':convex.distance(plane(actual_hub,0,r+2+rg),plane(actual_key,0,r+2)),
               'hub_side_positive':convex.distance(plane(actual_hub,1,w/2),plane(actual_key,1,width/2)),
               'hub_side_negative':convex.distance(plane(actual_hub,1,-w/2),plane(actual_key,1,-width/2)),
               'seat_side_positive':convex.distance(plane(actual_shaft,1,w/2),plane(actual_key,1,width/2)),
               'seat_side_negative':convex.distance(plane(actual_shaft,1,-w/2),plane(actual_key,1,-width/2)),
               'floor_contact':convex.distance(plane(actual_shaft,0,floor),plane(actual_key,0,floor))}
    for name,value in distances.items():k.close(value,rg if name=='roof' else (0 if name=='floor_contact' else sg))
    manifest=json.loads((output/'manifest.json').read_text());native=manifest['outputs']['keyed_coupling'];total=sum(volumes.values())
    k.require(native['instances']==3 and native['definitions']==3,'three real parts')
    k.close(native['volume_mm3'],total);k.close(native['bom']['total_mass_kg'],total*7800/1e9)
    k.close(k.properties(k.load(str(output/'keyed_coupling.step')))[0],total)
    verify=k.command(cad,'verify',output);k.require(verify.returncode==0,verify.stderr)
    return dict(parameters=parameters,analytical_volume_mm3=volumes,total_volume_mm3=total,directed_difference_mm3=differences,pairwise_overlap_mm3=overlap,physical_face_distances_mm=distances)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=k.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cases=[case(args.cad.resolve(),args.out_dir,i,p) for i,p in enumerate([{},dict(shaft_radius=7),dict(side_gap=.05,roof_gap=.1),dict(shaft_radius=7,side_gap=.01,roof_gap=.02)])]
    report=dict(valid=True,scope='Nominal geometric engagement only; no axial retention or torque capacity certification',cases=cases)
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))


if __name__=='__main__':main()
