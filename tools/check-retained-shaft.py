#!/usr/bin/env python3
"""Independent stepped keyed-shaft and nominal axial-retention geometry.

Checks actual solids and signed thrust gaps. Ring elasticity/load capacity and
hub retention are deliberately outside this geometric acceptance.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path

SPEC=importlib.util.spec_from_file_location('keyed_coupling_oracle',Path(__file__).with_name('check-keyed-coupling.py'))
q=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(q);k=q.k
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakePolygon,BRepBuilderAPI_MakeFace,BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeRevol,BRepPrimAPI_MakeBox,BRepPrimAPI_MakeCylinder,BRepPrimAPI_MakePrism
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut,BRepAlgoAPI_Common
from OCP.gp import gp_Pnt,gp_Ax1,gp_Dir,gp_Vec,gp_Trsf,gp_Quaternion


def revolve(profile):
    wire=BRepBuilderAPI_MakePolygon()
    for r,z in profile:wire.Add(gp_Pnt(r,0,z))
    wire.Close();face=BRepBuilderAPI_MakeFace(wire.Wire()).Face()
    return BRepPrimAPI_MakeRevol(face,gp_Ax1(gp_Pnt(0,0,0),gp_Dir(0,0,1)),2*math.pi,True).Shape()


def transformed(shape,z,opposed=False):
    t=gp_Trsf()
    if opposed:t.SetRotation(gp_Quaternion(1,0,0,0))
    t.SetTranslationPart(gp_Vec(0,0,z));return BRepBuilderAPI_Transform(shape,t,True).Shape()


def profile(r,p):
    return [[0,0],[r+2,0],[r+2,12.9],[r,12.9],[r,p+34.55],[r-.6,p+34.55],[r-.6,p+35.8],[r,p+35.8],[r,p+70],[0,p+70]]


def expected(r,p):
    intervals=[(r+2,0,12.9),(r,12.9,p+34.55),(r-.6,p+34.55,p+35.8),(r,p+35.8,p+70)]
    v=mz=xx=zz=0.
    for radius,a,b in intervals:
        v+=math.pi*radius**2*(b-a);mz+=math.pi*radius**2*(b*b-a*a)/2
        xx+=math.pi*radius**4*(b-a)/4;zz+=math.pi*radius**2*(b**3-a**3)/3
    a=r-2;b=2.02;c=15+p/2;d=c+13.5
    s=math.sqrt(r*r-b*b);theta=math.asin(b/r)
    area=b*s+r*r*theta-2*a*b;mx=b*(r*r-a*a)-b**3/3
    sx=b*s*(5*r*r-2*b*b)/12+r**4*theta/4-2*b*a**3/3
    sy=r**4*theta/4-b*s*(r*r-2*b*b)/4-2*a*b**3/3
    v-=area*(d-c);first=[-mx*(d-c),0,mz-area*(d*d-c*c)/2]
    raw=[[xx-sx*(d-c),0,-mx*(d*d-c*c)/2],[0,xx-sy*(d-c),0],[-mx*(d*d-c*c)/2,0,zz-area*(d**3-c**3)/3]]
    covariance=[[raw[i][j]-first[i]*first[j]/v for j in range(3)] for i in range(3)];trace=sum(covariance[i][i] for i in range(3))
    return v,[x/v for x in first],[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]


def reference(r,p):
    shaft=k.boolean(BRepAlgoAPI_Cut,revolve(profile(r,p)),BRepPrimAPI_MakeBox(gp_Pnt(r-2,-2.02,15+p/2),4,4.04,13.5).Shape())
    bush=revolve([[r+.15,0],[r+6,0],[r+6,2],[r+4,2],[r+4,18],[r+.15,18]])
    washer=k.boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(r+2,1.5).Shape(),BRepPrimAPI_MakeCylinder(r+.1,1.5).Shape())
    ring=k.boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(r+1.5,1.2).Shape(),BRepPrimAPI_MakeCylinder(r-.5,1.2).Shape())
    # Independent oversize wedge removes exactly the ±30 degree opening.
    wire=BRepBuilderAPI_MakePolygon()
    for x,y in [(0,0),(4*r,-4*r/math.sqrt(3)),(4*r,4*r/math.sqrt(3))]:wire.Add(gp_Pnt(x,y,0))
    wire.Close();wedge=BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire()).Face(),gp_Vec(0,0,1.2)).Shape()
    ring=k.boolean(BRepAlgoAPI_Cut,ring,wedge)
    return shaft,bush,washer,ring


def check(cad,directory,r,p):
    output=directory/f'r{r}-p{p}';source=k.ROOT/'examples/retained_shaft.star'
    result=q.convex.run(cad,source,output,dict(shaft_radius=r,mounting_pitch=p));k.require(result.returncode==0,result.stderr)
    shaft,bush,washer,ring=reference(r,p);wanted=expected(r,p)
    native_result=k.command(cad,'properties',source,'--output','shaft','--param',f'shaft_radius={r}','--param',f'mounting_pitch={p}')
    k.require(native_result.returncode==0,native_result.stderr);native=json.loads(native_result.stdout)
    for volume,center,inertia in [k.properties(shaft),k.properties(k.load(str(output/'shaft.step'))),(native['volume'],native['volume_centroid'],native['central_volume_inertia'])]:
        k.close(volume,wanted[0])
        for i in range(3):
            k.close(center[i],wanted[1][i])
            for j in range(3):k.close(inertia[i][j],wanted[2][i][j])
    shell=2*math.pi*.01*sum(radius*(b-a) for radius,a,b in [(r+2,0,12.9),(r,12.9,p+34.55),(r-.6,p+34.55,p+35.8),(r,p+35.8,p+70)])
    k.mesh(output/'shaft.stl',wanted[0],absolute_error=shell)
    leaves=q.convex.imported_leaves(output/'retained_shaft.step')
    refs={'shaft':transformed(shaft,10),'left_bush':transformed(bush,23),'right_bush':transformed(bush,p+43,True),'washer':transformed(washer,p+43.1),'retaining_ring':transformed(ring,p+44.6)}
    k.require(set(leaves)=={(name,) for name in refs},('unexpected retention leaf paths',list(leaves)))
    actual={name:leaves[(name,)]['shape'] for name in refs};differences={}
    for name,shape in refs.items():
        differences[name]=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(shape,actual[name]),(actual[name],shape)]]
        for value in differences[name]:k.close(value,0)
    overlaps={};names=list(actual)
    for i,a in enumerate(names):
        for b in names[i+1:]:
            value=k.properties(k.boolean(BRepAlgoAPI_Common,actual[a],actual[b]))[0];k.close(value,0);overlaps[a+'-'+b]=value
    contacts={'washer_ring':q.convex.distance(q.plane(actual['washer'],2,p+44.6),q.plane(actual['retaining_ring'],2,p+44.6)),
              'ring_groove_wall':q.convex.distance(q.plane(actual['retaining_ring'],2,p+45.8),q.plane(actual['shaft'],2,p+45.8))}
    for value in contacts.values():k.close(value,0)
    # Signed coordinates come from the independently located actual thrust planes.
    from OCP.BRepAdaptor import BRepAdaptor_Surface
    z=lambda shape,coordinate:BRepAdaptor_Surface(q.plane(shape,2,coordinate)).Plane().Location().Z()
    left_gap=z(actual['left_bush'],23)-z(actual['shaft'],22.9)
    right_gap=z(actual['washer'],p+43.1)-z(actual['right_bush'],p+43)
    k.close(left_gap,.1);k.close(right_gap,.1);k.close(left_gap+right_gap,.2)
    verify=k.command(cad,'verify',output);k.require(verify.returncode==0,verify.stderr)
    return dict(parameters=dict(shaft_radius=r,mounting_pitch=p),shaft_volume_mm3=wanted[0],shaft_centroid=wanted[1],shaft_inertia=wanted[2],mesh_chordal_tolerance_mm=.01,mesh_swept_shell_bound_mm3=shell,directed_difference_mm3=differences,pairwise_overlap_mm3=overlaps,contacts_mm=contacts,signed_thrust_gaps_mm=[left_gap,right_gap],total_endplay_mm=left_gap+right_gap)



def isolated(cad,directory,scale,pose):
    tag=f'isolated-s{scale}-p{pose}';source=directory/(tag+'.star');output=directory/tag
    points=[[x*scale,z*scale] for x,z in profile(6,60)];quaternion,offset,trsf=k.pose_values(pose)
    source.write_text(f'''stock=revolve(id="stock",profile=polygon(id="profile",points={points}))
box=extrude(id="box",profile=rectangle(id="rectangle",width={4*scale},depth={4.04*scale}),height={13.5*scale})
tool=translate(id="tool",body=box,offset={[4*scale,-2.02*scale,45*scale]})
stock_world=transform(id="stock_world",body=stock,rotation={quaternion},offset={offset})
tool_world=transform(id="tool_world",body=tool,rotation={quaternion},offset={offset})
finished=cut(id="finished",left=stock_world,right=tool_world)
output(name="shaft",feature=finished)
''')
    result=k.command(cad,'build',source,'--out-dir',output);k.require(result.returncode==0,result.stderr)
    oracle=k.boolean(BRepAlgoAPI_Cut,revolve(points),BRepPrimAPI_MakeBox(gp_Pnt(4*scale,-2.02*scale,45*scale),4*scale,4.04*scale,13.5*scale).Shape())
    oracle=BRepBuilderAPI_Transform(oracle,trsf,True).Shape();imported=k.load(str(output/'shaft.step'))
    volume,center,inertia=expected(6,60)
    wanted=k.transformed((volume*scale**3,[x*scale for x in center],[[x*scale**5 for x in row] for row in inertia]),trsf)
    query=k.command(cad,'properties',source,'--output','shaft');k.require(query.returncode==0,query.stderr);native=json.loads(query.stdout)
    (directory/(tag+'.properties.json')).write_text(json.dumps(native,indent=2))
    for v,c,m in [k.properties(oracle),k.properties(imported),(native['volume'],native['volume_centroid'],native['central_volume_inertia'])]:
        k.close(v,wanted[0],scale**3)
        for i in range(3):
            k.close(c[i],wanted[1][i],scale)
            for j in range(3):k.close(m[i][j],wanted[2][i][j],scale**5)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(oracle,imported),(imported,oracle)]]
    for value in differences:k.close(value,0,scale**3)
    # Sum radial shells along every axial stock segment; planar shoulders add
    # no volume because the bound integrates cross-sectional material error.
    bound=2*math.pi*.01*scale**2*sum(radius*(b-a) for radius,a,b in [(8,0,12.9),(6,12.9,94.55),(5.4,94.55,95.8),(6,95.8,130)])
    k.mesh(output/'shaft.stl',wanted[0],absolute_error=bound)
    verify=k.command(cad,'verify',output);k.require(verify.returncode==0,verify.stderr)
    return dict(case=tag,volume_mm3=wanted[0],centroid=wanted[1],inertia=wanted[2],directed_difference_mm3=differences,mesh_chordal_tolerance_mm=.01,mesh_swept_shell_bound_mm3=bound)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=k.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cases=[check(args.cad.resolve(),args.out_dir,r,p) for r,p in [(6,60),(7,72),(5,48)]]
    isolated_cases=[isolated(args.cad.resolve(),args.out_dir,scale,pose) for scale in [.5,1,2] for pose in [0,2]]
    report=dict(valid=True,cases=cases,isolated_cases=isolated_cases,scope='Nominal shaft axial retention geometry only; ring elasticity, strength and hub retention unverified')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))


if __name__=='__main__':main()
