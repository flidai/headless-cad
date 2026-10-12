#!/usr/bin/env python3
"""Independent finite rectangular keyseat acceptance; OCP is test-only.

Integrates the removed circular strip analytically, constructs cylinder-minus-box
in OCCT independently, and checks STEP trims, complete moments and oriented STL.
No rounded cutter end or general cylinder Boolean capability is implied.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'crates/cad-export/tools'))
from check_arc_prisms_ocp import mesh
from check_common_ocp import load, boolean
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder, BRepPrimAPI_MakeBox
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.gp import gp_Pnt, gp_Trsf, gp_Quaternion, gp_Vec
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def close(a, b, scale=1):
    require(math.isfinite(a) and abs(a-b) <= max(2e-7*scale, abs(b)*2e-9), (a,b))


def expected(r,h,a,b,c,d):
    s=math.sqrt(r*r-b*b);theta=math.asin(b/r);length=d-c
    area=b*s+r*r*theta-2*a*b
    mx=b*(r*r-a*a)-b**3/3
    xx=b*s*(5*r*r-2*b*b)/12+r**4*theta/4-2*b*a**3/3
    yy=r**4*theta/4-b*s*(r*r-2*b*b)/4-2*a*b**3/3
    cylinder=math.pi*r*r*h;v=cylinder-area*length
    first=[-mx*length,0,cylinder*h/2-area*(d*d-c*c)/2]
    raw=[[math.pi*r**4*h/4-xx*length,0,-mx*(d*d-c*c)/2],
         [0,math.pi*r**4*h/4-yy*length,0],
         [-mx*(d*d-c*c)/2,0,math.pi*r*r*h**3/3-area*(d**3-c**3)/3]]
    center=[x/v for x in first]
    covariance=[[raw[i][j]-first[i]*first[j]/v for j in range(3)] for i in range(3)]
    trace=sum(covariance[i][i] for i in range(3))
    inertia=[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]
    return v,center,inertia


def properties(shape):
    p=GProp_GProps();BRepGProp.VolumeProperties_s(shape,p)
    c=p.CentreOfMass();i=p.MatrixOfInertia()
    return p.Mass(),[c.X(),c.Y(),c.Z()],[[i.Value(a+1,b+1) for b in range(3)] for a in range(3)]


def pose_values(pose):
    q=[[0,0,0,1],[0,0,1,0],[.5,.5,0,math.sqrt(.5)]][pose]
    offset=[0,0,0] if pose==0 else [25,-12,7]
    transform=gp_Trsf();transform.SetRotation(gp_Quaternion(*q));transform.SetTranslationPart(gp_Vec(*offset))
    return q,offset,transform


def transformed(values,trsf):
    v,c,inertia=values
    rotation=[[trsf.Value(i+1,j+1) for j in range(3)] for i in range(3)]
    center=[sum(rotation[i][j]*c[j] for j in range(3))+trsf.Value(i+1,4) for i in range(3)]
    matrix=[[sum(rotation[i][a]*inertia[a][b]*rotation[j][b] for a in range(3) for b in range(3)) for j in range(3)] for i in range(3)]
    return v,center,matrix


def recipe(scale,pose,invalid=None,shallow=False):
    r,h,a,b,c,d=[v*scale for v in [6,40,4,2,10,24]]
    if shallow:a,b=5.996*scale,.2*scale
    if invalid=='floor':a=-scale
    if invalid=='width':b=5*scale
    if invalid=='end':c=0
    q,offset,_=pose_values(pose)
    return f'''stock = extrude(id="stock", profile=circle(id="circle", radius={r}), height={h})
box = extrude(id="box", profile=rectangle(id="rectangle", width={8*scale-a}, depth={2*b}), height={d-c})
tool = translate(id="tool", body=box, offset={[a,-b,c]})
stock_world = transform(id="stock_world", body=stock, rotation={q}, offset={offset})
tool_world = transform(id="tool_world", body=tool, rotation={q}, offset={offset})
finished = cut(id="finished", left=stock_world, right=tool_world)
output(name="keyed", feature=finished)
'''


def command(cad,*args):
    return subprocess.run([str(cad),*map(str,args)],capture_output=True,text=True)


def inventory(directory):
    return {str(p.relative_to(directory)):hashlib.sha256(p.read_bytes()).hexdigest() for p in directory.rglob('*') if p.is_file()}


def case(cad,directory,scale,pose,shallow=False,authored=False):
    tag='authored' if authored else f's{scale}-p{pose}'+('-shallow' if shallow else '')
    source=directory/(tag+'.star');output=directory/tag
    name='keyed_shaft' if authored else 'keyed'
    source.write_text((ROOT/'examples/keyed_shaft.star').read_text() if authored else recipe(scale,pose,shallow=shallow))
    result=command(cad,'build',source,'--out-dir',output)
    require(result.returncode==0,result.stderr)
    result=command(cad,'properties',source,'--output',name)
    require(result.returncode==0,result.stderr);native=json.loads(result.stdout)
    (directory/(tag+'.properties.json')).write_text(json.dumps(native,indent=2))
    r,h,a,b,c,d=[v*scale for v in [6,40,4,2,10,24]]
    if shallow:a,b=5.996*scale,.2*scale
    _,_,trsf=pose_values(pose)
    oracle=boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(r,h).Shape(),BRepPrimAPI_MakeBox(gp_Pnt(a,-b,c),8*scale-a,2*b,d-c).Shape())
    oracle=BRepBuilderAPI_Transform(oracle,trsf,True).Shape()
    imported=load(str(output/(name+'.step')));expected_values=transformed(expected(r,h,a,b,c,d),trsf)
    values=[properties(oracle),properties(imported),(native['volume'],native['volume_centroid'],native['central_volume_inertia'])]
    for volume,center,inertia in values:
        close(volume,expected_values[0],scale**3)
        for i in range(3):
            close(center[i],expected_values[1][i],scale)
            for j in range(3):close(inertia[i][j],expected_values[2][i][j],scale**5)
    differences=[]
    for left,right in [(oracle,imported),(imported,oracle)]:
        difference=boolean(BRepAlgoAPI_Cut,left,right)
        value=properties(difference)[0];close(value,0,scale**3);differences.append(value)
    surfaces={'cylinder':0,'plane':0};exp=TopExp_Explorer(imported,TopAbs_FACE)
    while exp.More():
        surface=BRepAdaptor_Surface(TopoDS.Face(exp.Current()))
        if surface.GetType()==GeomAbs_Cylinder:
            surfaces['cylinder']+=1;close(surface.Cylinder().Radius(),r,scale)
        else:
            require(surface.GetType()==GeomAbs_Plane,'nonanalytic or unexpected support');surfaces['plane']+=1
        exp.Next()
    require(surfaces=={'cylinder':4,'plane':7},surfaces)
    # Default tessellation is 0.01 mm chord error; the swept outer radial
    # shell bounds volume error at every scale (a fixed relative bound does not).
    shell_bound=2*math.pi*r*h*.01
    mesh(output/(name+'.stl'),expected_values[0],absolute_error=shell_bound)
    vertices=[list(map(float,line.split()[1:])) for line in (output/(name+'.stl')).read_text().splitlines() if line.split()[:1]==['vertex']]
    # Independently retain the oriented triangulated volume and explicit
    # absolute shell allowance, separate from exact STEP/native tolerances.
    mesh_volume=math.fsum((u[0]*(v[1]*w[2]-v[2]*w[1])+u[1]*(v[2]*w[0]-v[0]*w[2])+u[2]*(v[0]*w[1]-v[1]*w[0]))/6 for u,v,w in zip(vertices[::3],vertices[1::3],vertices[2::3]))
    mesh_evidence=dict(chordal_tolerance_mm=.01,swept_cylindrical_shell_bound_mm3=shell_bound,oriented_volume_mm3=mesh_volume,absolute_volume_error_mm3=abs(mesh_volume-expected_values[0]))
    verified=command(cad,'verify',output);require(verified.returncode==0,verified.stderr)
    return dict(case=tag,volume_mm3=expected_values[0],centroid=expected_values[1],inertia=expected_values[2],surfaces=surfaces,directed_difference_mm3=differences,mesh=mesh_evidence)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cases=[case(args.cad.resolve(),args.out_dir,s,p) for s in [.5,1,2] for p in range(3)]
    cases.extend(case(args.cad.resolve(),args.out_dir,1,p,shallow=True) for p in [0,2])
    cases.append(case(args.cad.resolve(),args.out_dir,1,0,authored=True))
    source=args.out_dir/'rejection.star';output=args.out_dir/'preserved'
    source.write_text(recipe(1,0));result=command(args.cad,'build',source,'--out-dir',output);require(result.returncode==0,result.stderr)
    before=inventory(output);failures=[]
    for invalid in ['floor','width','end']:
        source.write_text(recipe(1,0,invalid));result=command(args.cad,'build',source,'--out-dir',output)
        require(result.returncode!=0,invalid+' unexpectedly supported')
        require(inventory(output)==before,'failure changed published generation')
        failures.append(dict(case=invalid,error=json.loads(result.stderr)))
    report=dict(valid=True,cases=cases,rejections=failures)
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))


if __name__=='__main__':main()
