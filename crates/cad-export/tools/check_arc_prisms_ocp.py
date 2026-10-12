"""Independent moments for export_arc_prisms: rectangles and circular sectors.

Run with the optional OCP development environment and the fixture directory.
Checks native and imported STEP properties, analytic surfaces, STL closure and
winding. Expectations do not integrate or triangulate the implementation's path.
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
from check_concave_holes_ocp import close, transform


def shifted(m,x,y,sign=1):
    a,mx,my,xx,yy,xy=m
    return [sign*v for v in [a,mx+x*a,my+y*a,xx+2*x*mx+x*x*a,yy+2*y*my+y*y*a,xy+x*my+y*mx+x*y*a]]


def rectangle(w,h,x=0,y=0,sign=1):
    a=w*h
    return shifted([a,0,0,a*w*w/12,a*h*h/12,0],x,y,sign)


def circle(r,x,y,sign=-1):
    return shifted([math.pi*r*r,0,0,math.pi*r**4/4,math.pi*r**4/4,0],x,y,sign)


def expected(name):
    pi=math.pi
    if name=='capsule':
        r=5
        pieces=[rectangle(20,10)]
        for s in [-1,1]:
            pieces.append(shifted([pi*r*r/2,s*2*r**3/3,0,pi*r**4/8,pi*r**4/8,0],s*10,0))
        pieces.append(circle(2,0,0))
    elif name=='rounded':
        r=4
        pieces=[rectangle(40,30)]
        for sx in [-1,1]:
            for sy in [-1,1]:
                pieces.append(rectangle(4,4,sx*18,sy*13,-1))
                pieces.append(shifted([pi*r*r/4,sx*r**3/3,sy*r**3/3,pi*r**4/16,pi*r**4/16,sx*sy*r**4/8],sx*16,sy*11))
        pieces.append(circle(2,7,3))
    else:
        r=10
        pieces=[rectangle(6,20,-3,0),[pi*r*r/2,2*r**3/3,0,pi*r**4/8,pi*r**4/8,0],circle(1.5,-3,2)]
    a,mx,my,xx,yy,xy=[sum(p[i] for p in pieces) for i in range(6)]
    cx,cy=mx/a,my/a
    h=8;v=a*h
    ix=h*(yy-a*cy*cy)+v*h*h/12
    iy=h*(xx-a*cx*cx)+v*h*h/12
    iz=h*(xx+yy-a*(cx*cx+cy*cy))
    ixy=-h*(xy-a*cx*cy)
    return v,[cx,cy,h/2],[[ix,ixy,0],[ixy,iy,0],[0,0,iz]]


def mesh(path,exact,absolute_error=None):
    points=[];normals=[]
    for line in path.read_text().splitlines():
        f=line.split()
        if f[:1]==['vertex']:points.append(tuple(map(float,f[1:])))
        if f[:2]==['facet','normal']:normals.append(tuple(map(float,f[2:])))
    assert len(points)==3*len(normals) and normals
    cross=lambda a,b:(a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0])
    dot=lambda a,b:sum(x*y for x,y in zip(a,b))
    incidence={};v=0
    for k,normal in enumerate(normals):
        p=points[3*k:3*k+3]
        n=cross([p[1][i]-p[0][i] for i in range(3)],[p[2][i]-p[0][i] for i in range(3)])
        length=math.sqrt(dot(n,n));assert length>0
        assert all(abs(n[i]/length-normal[i])<1e-9 for i in range(3))
        v+=dot(p[0],cross(p[1],p[2]))/6
        for i in range(3):
            a,b=p[i],p[(i+1)%3];key=tuple(sorted((a,b)))
            count,sign=incidence.get(key,(0,0));incidence[key]=(count+1,sign+(1 if a<b else -1))
    assert all(value==(2,0) for value in incidence.values())
    assert abs(v-exact) < (.002*exact if absolute_error is None else absolute_error),(path,v,exact)


def main():
    directory=pathlib.Path(sys.argv[1])
    for name in ['capsule','rounded','asymmetric']:
        for oblique in [False,True]:
            fixture=name+('_oblique' if oblique else '')
            v,c,inertia=expected(name)
            if oblique:c,inertia=transform(c,inertia)
            step=directory/f'{fixture}.step'
            text=step.read_text()
            assert text.count('SEAM_CURVE(')==1,fixture
            assert text.count('CYLINDRICAL_SURFACE(')=={'capsule':3,'rounded':5,'asymmetric':2}[name]
            reader=STEPControl_Reader()
            assert reader.ReadFile(str(step))==IFSelect_RetDone
            assert reader.TransferRoots()==1
            shape=reader.OneShape();assert not shape.IsNull() and BRepCheck_Analyzer(shape).IsValid(),fixture
            p=GProp_GProps();BRepGProp.VolumeProperties_s(shape,p)
            center=p.CentreOfMass();matrix=p.MatrixOfInertia()
            native=json.loads((directory/f'{fixture}.properties.json').read_text())
            assert close(p.Mass(),v) and close(native['volume'],v),(fixture,p.Mass(),native['volume'],v)
            for i in range(3):
                assert close([center.X(),center.Y(),center.Z()][i],c[i]),(fixture,'centroid',i)
                assert close(native['volume_centroid'][i],c[i]),(fixture,'nativecentroid',i)
                for j in range(3):
                    assert close(matrix.Value(i+1,j+1),inertia[i][j]),(fixture,i,j,matrix.Value(i+1,j+1),inertia[i][j])
                    assert close(native['central_volume_inertia'][i][j],inertia[i][j]),(fixture,'nativeinertia',i,j)
            mesh(directory/f'{fixture}.stl',v)
            print(json.dumps({'fixture':fixture,'valid':True,'volume_mm3':v,'centroid_mm':c,'closed_stl':True}))


if __name__=='__main__':main()
