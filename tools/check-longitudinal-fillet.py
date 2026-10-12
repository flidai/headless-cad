#!/usr/bin/env python3
"""Independent native longitudinal fillet acceptance against OCCT edge fillets.

OCP is test-only. No general rolling-ball, cap-edge, or arbitrary curved fillet
support is implied by these bounded right-prism cases.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('helpers', Path(__file__).with_name('check-keyseat.py'))
k = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(k)
from OCP.BRepAdaptor import BRepAdaptor_Curve, BRepAdaptor_Surface
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakePolygon, BRepBuilderAPI_MakeFace, BRepBuilderAPI_Transform
from OCP.BRepFilletAPI import BRepFilletAPI_MakeFillet
from OCP.BRepPrimAPI import BRepPrimAPI_MakePrism, BRepPrimAPI_MakeCylinder
from OCP.GeomAbs import GeomAbs_Cylinder
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.gp import gp_Pnt, gp_Vec, gp_Ax2, gp_Dir


def run(cad, *args, success=True):
    p = subprocess.run([str(cad), *map(str, args)], capture_output=True, text=True, timeout=180)
    k.require((p.returncode == 0) == success, (p.returncode, p.stderr, p.stdout[:1000]))
    return json.loads(p.stdout if p.returncode == 0 else p.stderr)


def filleted(points, h, radius, corners):
    polygon = BRepBuilderAPI_MakePolygon()
    for x, y in points:
        polygon.Add(gp_Pnt(x, y, 0))
    polygon.Close()
    solid = BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(polygon.Wire()).Face(), gp_Vec(0, 0, h)).Shape()
    # Select real full-height edges on the original prism, independently of the
    # implementation's 2D tangent-point construction and topology numbering.
    op = BRepFilletAPI_MakeFillet(solid)
    seen = set()
    explorer = TopExp_Explorer(solid, TopAbs_EDGE)
    while explorer.More():
        edge = TopoDS.Edge(explorer.Current())
        curve = BRepAdaptor_Curve(edge)
        a, b = curve.Value(curve.FirstParameter()), curve.Value(curve.LastParameter())
        if abs(a.X()-b.X()) < 1e-8 and abs(a.Y()-b.Y()) < 1e-8 and abs(abs(a.Z()-b.Z())-h) < 1e-8:
            for corner in corners:
                x, y = points[corner]
                if corner not in seen and abs(a.X()-x) < 1e-8 and abs(a.Y()-y) < 1e-8:
                    op.Add(radius, edge)
                    seen.add(corner)
        explorer.Next()
    k.require(seen == set(corners), 'independent edge selection')
    op.Build()
    k.require(op.IsDone(), 'OCCT fillet construction')
    return op.Shape()



def rectangle_moments(w, d, h, r, corners, hole):
    # Closed-form rectangle minus each corner square plus its quarter disk.
    # Integrals are area, first moments and raw second moments about the origin.
    pieces = [[w*d, w*w*d/2, w*d*d/2, w**3*d/3, w*d**3/3, w*w*d*d/4]]
    def shift(v, x, y, sign=1):
        a,mx,my,xx,yy,xy=v
        pieces.append([sign*t for t in [a,mx+x*a,my+y*a,xx+2*x*mx+x*x*a,
                                       yy+2*y*my+y*y*a,xy+x*my+y*mx+x*y*a]])
    for corner in corners:
        sx,sy=[(-1,-1),(1,-1),(1,1),(-1,1)][corner]
        x,y=(r if sx<0 else w-r),(r if sy<0 else d-r)
        shift([r*r,0,0,r**4/12,r**4/12,0],x+sx*r/2,y+sy*r/2,-1)
        shift([math.pi*r*r/4,sx*r**3/3,sy*r**3/3,math.pi*r**4/16,
               math.pi*r**4/16,sx*sy*r**4/8],x,y)
    if hole:
        shift([9*math.pi,0,0,81*math.pi/4,81*math.pi/4,0],30,20,-1)
    a,mx,my,xx,yy,xy=[sum(p[i] for p in pieces) for i in range(6)]
    v=a*h;cx=mx/a;cy=my/a
    ix=h*(yy-a*cy*cy)+v*h*h/12
    iy=h*(xx-a*cx*cx)+v*h*h/12
    iz=h*(xx+yy-a*(cx*cx+cy*cy));ixy=-h*(xy-a*cx*cy)
    return v,[cx,cy,h/2],[[ix,ixy,0],[ixy,iy,0],[0,0,iz]]


def recipe(points, h, radius, corners, pose, hole):
    q, offset, _ = k.pose_values(pose)
    lines = [f'stock = extrude(id="stock", profile=polygon(id="outline", points={points}), height={h})', 'body = stock']
    for i, corner in enumerate(corners):
        roles = [f'wall_{(corner-1)%len(points)}', f'wall_{corner}']
        faces = '[{"from_feature":stock,"role":"'+roles[0]+'"},{"from_feature":stock,"role":"'+roles[1]+'"}]'
        lines.append(f'body = fillet(id="round_{i}", body=body, faces={faces}, radius={radius})')
    if hole:
        lines += ['drill = extrude(id="drill", profile=circle(id="drill_profile", radius=3), height='+str(h)+')',
                  'drill_at = translate(id="drill_at", body=drill, offset=[30,20,0])',
                  'body = cut(id="bored", left=body, right=drill_at)']
    lines += [f'finished = transform(id="finished", body=body, rotation={q}, offset={offset})', 'output(name="finished", feature=finished)']
    return '\n'.join(lines)+'\n'


def check_case(cad, root, tag, points, h, radius, corners, pose=0, hole=False):
    source, output = root/(tag+'.star'), root/tag
    source.write_text(recipe(points, h, radius, corners, pose, hole))
    built = run(cad, 'build', source, '--out-dir', output)
    run(cad, 'verify', output)
    native = run(cad, 'properties', source, '--output', 'finished')
    expected = filleted(points, h, radius, corners)
    if hole:
        tool = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(30,20,0),gp_Dir(0,0,1)),3,h).Shape()
        expected = k.boolean(BRepAlgoAPI_Cut, expected, tool)
    if pose:
        expected = BRepBuilderAPI_Transform(expected, k.pose_values(pose)[2], True).Shape()
    expected_values = k.properties(expected)
    if points == [[0,0],[60,0],[60,40],[0,40]]:
        closed = k.transformed(rectangle_moments(60,40,h,radius,corners,hole),k.pose_values(pose)[2])
        k.close(expected_values[0],closed[0])
        for i in range(3):
            k.close(expected_values[1][i],closed[1][i])
            for j in range(3):
                k.close(expected_values[2][i][j],closed[2][i][j])
    actual = k.load(str(output/'finished.step'))
    for values in [k.properties(actual), (native['volume'], native['volume_centroid'], native['central_volume_inertia'])]:
        k.close(values[0], expected_values[0])
        for i in range(3):
            k.close(values[1][i], expected_values[1][i])
            for j in range(3):
                k.close(values[2][i][j], expected_values[2][i][j])
    diffs = [k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(actual,expected),(expected,actual)]]
    for d in diffs:
        k.close(d,0)
    radii = []
    e = TopExp_Explorer(actual, TopAbs_FACE)
    while e.More():
        surface = BRepAdaptor_Surface(TopoDS.Face(e.Current()))
        if surface.GetType() == GeomAbs_Cylinder:
            radii.append(surface.Cylinder().Radius())
        e.Next()
    k.require(len([r for r in radii if abs(r-radius)<1e-7]) == len(corners), ('fillet cylindrical patches',radii))
    k.mesh(output/'finished.stl', expected_values[0])
    replay = root/(tag+'-replay')
    run(cad,'build',output/'recipe.star','--out-dir',replay)
    k.require((replay/'finished.step').read_bytes() == (output/'finished.step').read_bytes(), 'captured recipe STEP replay')
    return {'case':tag,'volume_mm3':expected_values[0], 'centroid_mm':expected_values[1],
            'central_inertia_mm5':expected_values[2], 'directed_difference_mm3':diffs,
            'cylindrical_radii_mm':radii, 'kernel_build_id':built['kernel_build_id'], 'executor_build_id':built['executor_build_id']}



def authored_case(cad, root, radius, bore, h):
    tag=f'cover-r{radius}-b{bore}-h{h}';output=root/tag
    source=k.ROOT/'examples/rounded_bearing_cover.star'
    params=['--param',f'corner_radius={radius}','--param',f'bore_radius={bore}','--param',f'height={h}']
    built=run(cad,'build',source,'--out-dir',output,*params)
    run(cad,'verify',output)
    native=run(cad,'properties',source,'--output','bearing_cover',*params)
    expected=filleted([[0,0],[60,0],[60,40],[0,40]],h,radius,[0,1,2,3])
    for x,y,r in [(30,20,bore),(10,10,2.5),(50,10,2.5),(50,30,2.5),(10,30,2.5)]:
        tool=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x,y,-1),gp_Dir(0,0,1)),r,h+2).Shape()
        expected=k.boolean(BRepAlgoAPI_Cut,expected,tool)
    values=k.properties(expected)
    exact=(2400-(4-math.pi)*radius*radius-math.pi*(bore*bore+25))*h
    k.close(values[0],exact)
    actual=k.load(str(output/'bearing_cover.step'))
    for p in [k.properties(actual),(native['volume'],native['volume_centroid'],native['central_volume_inertia'])]:
        k.close(p[0],values[0])
        for i in range(3):
            k.close(p[1][i],values[1][i])
            for j in range(3): k.close(p[2][i][j],values[2][i][j])
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(actual,expected),(expected,actual)]]
    for d in differences:k.close(d,0)
    k.mesh(output/'bearing_cover.stl',exact)
    return {'case':tag,'volume_mm3':exact,'centroid_mm':values[1],
            'central_inertia_mm5':values[2],'directed_difference_mm3':differences,
            'kernel_build_id':built['kernel_build_id'],'executor_build_id':built['executor_build_id']}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cad',type=Path,default=k.ROOT/'target/release/cad')
    p.add_argument('--out-dir',type=Path,required=True)
    args=p.parse_args()
    root=args.out_dir.resolve();root.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve()
    box=[[0,0],[60,0],[60,40],[0,40]]
    cases=[]
    for radius in [1,4,8]:
        cases.append(check_case(cad,root,f'corner-r{radius}',box,8,radius,[0]))
    for pose in [0,1,2]:
        cases.append(check_case(cad,root,f'four-corners-p{pose}',box,8,4,[0,1,2,3],pose,True))
    cases.append(check_case(cad,root,'triangle',[[0,0],[60,0],[15,40]],8,3,[0]))
    for params in [(4,10,8),(2,8,4),(6,12,12)]:
        cases.append(authored_case(cad,root,*params))
    # Failed topology-event edits must leave an accepted generation byte intact.
    accepted=root/'corner-r4'; before=k.inventory(accepted)
    invalid=root/'radius-event.star'
    invalid.write_text(recipe(box,8,50,[0],0,False))
    failure=run(cad,'build',invalid,'--out-dir',accepted,success=False)
    k.require(k.inventory(accepted)==before,'failed fillet preserves accepted generation')
    report={'valid':True,'oracle':'OCCT BRepFilletAPI_MakeFillet; test dependency only',
            'cases':cases,'radius_event_failure':failure}
    (root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__=='__main__':
    main()
