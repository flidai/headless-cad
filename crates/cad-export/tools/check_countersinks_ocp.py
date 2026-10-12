"""Independent conical machining integration, STEP orientation and mesh closure.

Three-point Gauss-Legendre integration exactly integrates the degree <=4 slice
polynomials. The expected dimensions are fixture inputs, never native topology.
"""
import json
import math
import pathlib
import sys
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cone
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from check_arc_prisms_ocp import mesh
from check_blind_pallet_ocp import cylinder, combine
from check_concave_holes_ocp import box, transform
from check_constrained_fixture_ocp import body_properties, require
from check_counterbores_ocp import verify

CASES = [stock+'_'+side for stock in ['box','disk','annulus'] for side in ['top','bottom']]+['box_crossing_top','box_crossing_bottom']


def frustum(r0,r1,low,high,xy,sign=-1):
    h=high-low
    # Slice area, axial first/second moments, and radial fourth moment.
    moments=[0.]*4
    for node,weight in [(-math.sqrt(3/5),5/9),(0,8/9),(math.sqrt(3/5),5/9)]:
        z=(node+1)*h/2
        r=r0+(r1-r0)*z/h
        for i,value in enumerate([r*r,z*r*r,z*z*r*r,r**4]):
            moments[i]+=weight*h/2*value
    a,m,q,radial=moments
    volume=sign*math.pi*a
    axial=sign*math.pi*(q-m*m/a)
    radial=sign*math.pi*radial
    return volume,[*xy,low+m/a],[axial+radial/4,axial+radial/4,radial/2]


def expected(name):
    xy=[23,17] if name.startswith('box') else [18,0]
    pieces=[box([0,0,0],[60,40,12],1)] if name.startswith('box') else [cylinder(30,12,[0,0,6])]
    if name.startswith('annulus'): pieces.append(cylinder(8,12,[0,0,6],-1))
    if name=='box_crossing_top':
        pieces.extend([cylinder(3,6.5,[*xy,3.25],-1),frustum(3,7.4,6.5,12,xy)])
    elif name=='box_crossing_bottom':
        pieces.extend([frustum(7.4,3,0,5.5,xy),cylinder(3,6.5,[*xy,8.75],-1)])
    elif name.endswith('top'):
        pieces.extend([cylinder(3,8,[*xy,4],-1),frustum(3,7,8,12,xy)])
    else:
        pieces.extend([frustum(7,3,0,4,xy),cylinder(3,8,[*xy,8],-1)])
    return combine(pieces)


def cone_faces(path, expected_count=1, slope=1.):
    reader=STEPControl_Reader()
    require(reader.ReadFile(str(path))==IFSelect_RetDone,'STEP read')
    require(reader.TransferRoots()==1,'STEP root')
    explorer=TopExp_Explorer(reader.OneShape(),TopAbs_FACE)
    count=0
    while explorer.More():
        face=TopoDS.Face(explorer.Current())
        surface=BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Cone:
            count+=1
            require(face.Orientation()==TopAbs_REVERSED,'cone wall must face removed material')
            require(abs(abs(surface.Cone().SemiAngle())-math.atan(slope))<1e-10,'cone semiangle')
        explorer.Next()
    require(count==expected_count,'conical countersink count')


def main():
    directory=pathlib.Path(sys.argv[1]);cases=[]
    for name in CASES:
        for oblique in [False,True]:
            stem=name+('_oblique' if oblique else '')
            nominal=expected(name)
            if oblique:
                c,i=transform(nominal[1],nominal[2]);nominal=nominal[0],c,i
            native=json.loads((directory/f'{stem}.properties.json').read_text())
            verify((native['volume'],native['centroid'],native['inertia']),nominal,stem+' native')
            verify(body_properties(directory/f'{stem}.step'),nominal,stem+' STEP')
            cone_faces(directory/f'{stem}.step',slope=.8 if 'crossing' in name else 1.)
            try:
                mesh(directory/f'{stem}.stl',nominal[0])
            except AssertionError as error:
                raise AssertionError(stem+' STL closure, normals, or volume mismatch') from error
            cases.append(dict(fixture=stem,volume_mm3=nominal[0],inward_cone=True,closed_stl=True))
    print(json.dumps(dict(valid=True,cases=cases),indent=2))

if __name__=='__main__':main()
