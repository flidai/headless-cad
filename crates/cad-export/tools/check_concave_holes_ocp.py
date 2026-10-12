"""Check export_concave_holes fixtures against signed box/cylinder moments.

Usage: python check_concave_holes_ocp.py DIRECTORY
OCP is an optional development oracle, never a production dependency.
"""
import json
import math
import pathlib
import sys
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader


def close(a,b):
    return math.isfinite(a) and abs(a-b) <= max(1e-7,abs(b)*1e-10)


def box(lo, sizes, sign):
    v=sign*math.prod(sizes)
    c=[lo[i]+sizes[i]/2 for i in range(3)]
    inertia=[v*sum(sizes[j]**2 for j in range(3) if i!=j)/12 for i in range(3)]
    return v,c,inertia


def expected(circle):
    pieces=[box([0,0,0],[40,30,6],1),box([5,5,0],[4,18,6],-1),box([9,5,0],[16,4,6],-1),box([25,5,0],[4,18,6],-1)]
    if circle:
        v=-math.pi*9*6
        pieces.append((v,[17,17,3],[v*(27+36)/12,v*(27+36)/12,v*9/2]))
    else:
        pieces.append(box([13,13,0],[8,8,6],-1))
    v=sum(p[0] for p in pieces)
    c=[sum(p[0]*p[1][i] for p in pieces)/v for i in range(3)]
    inertia=[[0.]*3 for _ in range(3)]
    for mass,center,diagonal in pieces:
        d=[center[i]-c[i] for i in range(3)]
        for i in range(3):
            for j in range(3):
                inertia[i][j]+=(diagonal[i]+mass*sum(d[k]**2 for k in range(3) if k!=i)) if i==j else -mass*d[i]*d[j]
    return v,c,inertia


def transform(c,inertia):
    q=math.sqrt(.5)
    r=[[.5,.5,q],[.5,.5,-q],[-q,q,0]]
    c=[sum(r[i][j]*c[j] for j in range(3))+[12,-7,4][i] for i in range(3)]
    inertia=[[sum(r[i][a]*inertia[a][b]*r[j][b] for a in range(3) for b in range(3)) for j in range(3)] for i in range(3)]
    return c,inertia


def mesh_check(path,expected_volume,circle):
    vertices=[];normals=[]
    for line in path.read_text().splitlines():
        fields=line.split()
        if fields[:1]==['vertex']: vertices.append(tuple(map(float,fields[1:])))
        if fields[:2]==['facet','normal']: normals.append(tuple(map(float,fields[2:])))
    assert len(vertices)==3*len(normals)
    edges={};volume=0
    cross=lambda a,b:(a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0])
    dot=lambda a,b:sum(x*y for x,y in zip(a,b))
    for i,declared in enumerate(normals):
        p=vertices[3*i:3*i+3]
        n=cross([p[1][j]-p[0][j] for j in range(3)],[p[2][j]-p[0][j] for j in range(3)])
        length=math.sqrt(dot(n,n));assert length>0
        assert all(abs(n[j]/length-declared[j])<1e-9 for j in range(3))
        volume+=dot(p[0],cross(p[1],p[2]))/6
        for j in range(3):
            a,b=p[j],p[(j+1)%3]; key=tuple(sorted((a,b)))
            count,sign=edges.get(key,(0,0));edges[key]=(count+1,sign+(1 if a<b else -1))
    assert all(pair==(2,0) for pair in edges.values())
    if circle: assert 0<volume-expected_volume<expected_volume*.002
    else: assert close(volume,expected_volume)


def main():
    directory=pathlib.Path(sys.argv[1])
    for circle,name in [(True,'circle_bay'),(False,'rectangle_bay')]:
        for oblique in [False,True]:
            fixture=name+('_oblique' if oblique else '')
            v,c,inertia=expected(circle)
            if oblique:c,inertia=transform(c,inertia)
            reader=STEPControl_Reader()
            assert reader.ReadFile(str(directory/f'{fixture}.step'))==IFSelect_RetDone
            assert reader.TransferRoots()==1
            shape=reader.OneShape();assert not shape.IsNull() and BRepCheck_Analyzer(shape).IsValid()
            p=GProp_GProps();BRepGProp.VolumeProperties_s(shape,p)
            center=p.CentreOfMass();matrix=p.MatrixOfInertia()
            native=json.loads((directory/f'{fixture}.properties.json').read_text())
            assert close(p.Mass(),v) and close(native['volume'],v),fixture
            for i in range(3):
                assert close([center.X(),center.Y(),center.Z()][i],c[i]),fixture
                assert close(native['volume_centroid'][i],c[i]),fixture
                for j in range(3):
                    assert close(matrix.Value(i+1,j+1),inertia[i][j]),(fixture,i,j,matrix.Value(i+1,j+1),inertia[i][j])
                    assert close(native['central_volume_inertia'][i][j],inertia[i][j]),(fixture,i,j)
            mesh_check(directory/f'{fixture}.stl',v,circle)
            print(json.dumps({'fixture':fixture,'valid':True,'volume_mm3':v,'centroid_mm':c,'closed_stl':True}))


if __name__=='__main__':main()
