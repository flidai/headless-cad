#!/usr/bin/env python3
"""Independent through hub-keyway cylinder/box Boolean acceptance.

Closed-form annulus-minus-notch moments and independent OCCT construction are
separate from native history. OCP is an optional test dependency, never runtime.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import sys

SPEC=importlib.util.spec_from_file_location('keyseat_oracle',Path(__file__).with_name('check-keyseat.py'))
k=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(k)
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder, BRepPrimAPI_MakeBox
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.gp import gp_Pnt
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane


def expected(r_outer,r,h,c,b):
    q=math.sqrt(r*r-b*b);theta=math.asin(b/r)
    removed=2*b*c-b*q-r*r*theta
    mx=b*(c*c-r*r)+b**3/3
    xx=2*b*c**3/3-b*q*(5*r*r-2*b*b)/12-r**4*theta/4
    yy=2*c*b**3/3-r**4*theta/4+b*q*(r*r-2*b*b)/4
    area=math.pi*(r_outer*r_outer-r*r)-removed
    cx=-mx/area;v=area*h
    raw=math.pi*(r_outer**4-r**4)/4
    central_xx=(raw-xx)*h-v*cx*cx;central_yy=(raw-yy)*h
    return v,[cx,0,h/2],[[central_yy+v*h*h/12,0,0],[0,central_xx+v*h*h/12,0],[0,0,central_xx+central_yy]]


def dimensions(scale,variant):
    outer,r,h,c,b=10,6,20,8,2
    if variant=='shallow':c,b=6.004,.2
    if variant=='near_outer':c=math.sqrt(outer*outer-b*b)-.0001
    return [x*scale for x in [outer,r,h,c,b]]


def recipe(scale,pose,variant='standard',invalid=None):
    outer,r,h,c,b=dimensions(scale,variant);x0=0;z0=-scale;z1=h+scale
    if invalid=='outer_breakthrough':c=outer
    if invalid=='partial_length':z0=scale
    if invalid=='inside_bore':c=r
    if invalid=='bore_width':b=r
    if invalid=='radial_start':x0=math.sqrt(r*r-b*b)+.1*scale
    q,offset,_=k.pose_values(pose)
    return f'''stock=extrude(id="stock",profile=circle(id="annulus",radius={outer},inner_radius={r}),height={h})
box=extrude(id="box",profile=rectangle(id="rectangle",width={c-x0},depth={2*b}),height={z1-z0})
tool=translate(id="tool",body=box,offset={[x0,-b,z0]})
stock_world=transform(id="stock_world",body=stock,rotation={q},offset={offset})
tool_world=transform(id="tool_world",body=tool,rotation={q},offset={offset})
finished=cut(id="finished",left=stock_world,right=tool_world)
output(name="hub",feature=finished)
'''


def case(cad,directory,scale,pose,variant='standard'):
    tag=f'{variant}-s{scale}-p{pose}';source=directory/(tag+'.star');output=directory/tag
    authored=variant=='authored'
    name='keyed_hub' if authored else 'hub'
    source.write_text((k.ROOT/'examples/keyed_hub.star').read_text() if authored else recipe(scale,pose,variant))
    built=k.command(cad,'build',source,'--out-dir',output);k.require(built.returncode==0,built.stderr)
    result=k.command(cad,'properties',source,'--output',name);k.require(result.returncode==0,result.stderr)
    native=json.loads(result.stdout);(directory/(tag+'.properties.json')).write_text(json.dumps(native,indent=2))
    outer,r,h,c,b=([24,6.15,14,8.05,2.02] if authored else dimensions(scale,variant));_,_,trsf=k.pose_values(pose)
    tube=k.boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(outer,h).Shape(),BRepPrimAPI_MakeCylinder(r,h).Shape())
    box=BRepPrimAPI_MakeBox(gp_Pnt(0,-b,-scale),c,2*b,h+2*scale).Shape()
    oracle=BRepBuilderAPI_Transform(k.boolean(BRepAlgoAPI_Cut,tube,box),trsf,True).Shape()
    imported=k.load(str(output/(name+'.step')));expect=k.transformed(expected(outer,r,h,c,b),trsf)
    errors=[]
    for v,center,inertia in [k.properties(oracle),k.properties(imported),(native['volume'],native['volume_centroid'],native['central_volume_inertia'])]:
        k.close(v,expect[0],scale**3)
        for i in range(3):
            k.close(center[i],expect[1][i],scale)
            for j in range(3):k.close(inertia[i][j],expect[2][i][j],scale**5)
        errors.append(dict(volume_mm3=abs(v-expect[0]),centroid_mm=max(abs(center[i]-expect[1][i]) for i in range(3)),inertia_mm5=max(abs(inertia[i][j]-expect[2][i][j]) for i in range(3) for j in range(3))))
    differences=[]
    for left,right in [(oracle,imported),(imported,oracle)]:
        volume=k.properties(k.boolean(BRepAlgoAPI_Cut,left,right))[0];k.close(volume,0,scale**3);differences.append(volume)
    surfaces={'outer_cylinder':0,'inner_cylinder':0,'plane':0};faces=TopExp_Explorer(imported,TopAbs_FACE)
    while faces.More():
        surface=BRepAdaptor_Surface(TopoDS.Face(faces.Current()))
        if surface.GetType()==GeomAbs_Cylinder:
            radius=surface.Cylinder().Radius();key='outer_cylinder' if abs(radius-outer)<abs(radius-r) else 'inner_cylinder'
            k.close(radius,outer if key=='outer_cylinder' else r,scale);surfaces[key]+=1
        else:
            k.require(surface.GetType()==GeomAbs_Plane,'unexpected nonanalytic support');surfaces['plane']+=1
        faces.Next()
    k.require(surfaces=={'outer_cylinder':1,'inner_cylinder':2,'plane':5},surfaces)
    # Each approximated inner/outer arc stays in its 0.01mm radial shell.
    # Summing both shell volumes bounds signed material-volume error.
    shell=2*math.pi*(outer+r)*h*.01
    k.mesh(output/(name+'.stl'),expect[0],absolute_error=shell)
    vertices=[list(map(float,line.split()[1:])) for line in (output/(name+'.stl')).read_text().splitlines() if line.split()[:1]==['vertex']]
    mesh_volume=math.fsum((u[0]*(v[1]*w[2]-v[2]*w[1])+u[1]*(v[2]*w[0]-v[0]*w[2])+u[2]*(v[0]*w[1]-v[1]*w[0]))/6 for u,v,w in zip(vertices[::3],vertices[1::3],vertices[2::3]))
    verified=k.command(cad,'verify',output);k.require(verified.returncode==0,verified.stderr)
    return dict(case=tag,volume_mm3=expect[0],centroid=expect[1],inertia=expect[2],oracle_step_native_errors=errors,directed_difference_mm3=differences,surfaces=surfaces,mesh=dict(chordal_tolerance_mm=.01,swept_shell_bound_mm3=shell,oriented_volume_mm3=mesh_volume,absolute_volume_error_mm3=abs(mesh_volume-expect[0])))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=k.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False);cad=args.cad.resolve()
    cases=[case(cad,args.out_dir,s,p) for s in [.5,1,2] for p in range(3)]
    cases.extend(case(cad,args.out_dir,1,p,variant) for variant in ['shallow','near_outer'] for p in [0,2])
    cases.append(case(cad,args.out_dir,1,0,'authored'))
    source=args.out_dir/'rejection.star';output=args.out_dir/'preserved';source.write_text(recipe(1,0))
    result=k.command(cad,'build',source,'--out-dir',output);k.require(result.returncode==0,result.stderr);before=k.inventory(output);failures=[]
    for invalid in ['outer_breakthrough','partial_length','inside_bore','bore_width','radial_start']:
        source.write_text(recipe(1,0,invalid=invalid));result=k.command(cad,'build',source,'--out-dir',output)
        k.require(result.returncode!=0,'unexpected unsupported-domain success: '+invalid);k.require(k.inventory(output)==before,'failed publication changed generation')
        failures.append(dict(case=invalid,error=json.loads(result.stderr)))
    report=dict(valid=True,cases=cases,rejections=failures)
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))


if __name__=='__main__':main()
