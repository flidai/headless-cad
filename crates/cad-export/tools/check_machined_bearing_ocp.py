"""Independent sequential through-bore and concave-seat STEP acceptance.

The nominal jaw is a rectangular prism minus a half cylinder and four full
cylinders. OCP is solely an optional independent import/measurement oracle.
"""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_constrained_fixture_ocp import body_properties, close, json_output, require
from check_arc_prisms_ocp import mesh
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder
from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.STEPControl import STEPControl_Reader
from OCP.IFSelect import IFSelect_RetDone
from OCP.gp import gp_Pnt, gp_Vec
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/machined_bearing_support.star'


def jaw_properties(r,b,d,drilled=True):
    pi=math.pi
    # Signed planar components: area, center, central second x/y moments.
    half_area=pi*r*r/2
    offset=4*r/(3*pi)
    pieces=[(5000,50,25,50*100**3/12,100*50**3/12),
            (-half_area,50,50-offset,-pi*r**4/8,-(pi*r**4/8-half_area*offset**2))]
    if drilled:
        for x in [12,88]:
            for y in [12,40]:pieces.append((-b*b*pi,x,y,-pi*b**4/4,-pi*b**4/4))
    area=sum(p[0] for p in pieces)
    cx=sum(a*x for a,x,y,xx,yy in pieces)/area
    cy=sum(a*y for a,x,y,xx,yy in pieces)/area
    xx=sum(ix+a*(x-cx)**2 for a,x,y,ix,iy in pieces)
    yy=sum(iy+a*(y-cy)**2 for a,x,y,ix,iy in pieces)
    xy=sum(a*(x-cx)*(y-cy) for a,x,y,ix,iy in pieces)
    v=area*d
    return v,[cx,cy,d/2],[[d*yy+v*d*d/12,-d*xy,0],[-d*xy,d*xx+v*d*d/12,0],[0,0,d*(xx+yy)]]


def inward_cylinders(step):
    reader=STEPControl_Reader();require(reader.ReadFile(str(step))==IFSelect_RetDone,'STEP read failed')
    require(reader.TransferRoots()>0,'STEP transfer failed')
    explorer=TopExp_Explorer(reader.OneShape(),TopAbs_FACE);radii=[]
    while explorer.More():
        face=TopoDS.Face(explorer.Current());surface=BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Cylinder:
            point=gp_Pnt();du=gp_Vec();dv=gp_Vec()
            surface.D1((surface.FirstUParameter()+surface.LastUParameter())/2,
                       (surface.FirstVParameter()+surface.LastVParameter())/2,point,du,dv)
            normal=du.Crossed(dv)
            if face.Orientation()==TopAbs_REVERSED:normal.Reverse()
            axis=surface.Cylinder().Axis();origin=axis.Location();direction=axis.Direction()
            radial=gp_Vec(origin,point);axial=radial.Dot(gp_Vec(direction))
            radial.Subtract(gp_Vec(direction).Multiplied(axial))
            require(normal.Dot(radial)<0,'imported cylinder wall points out of the cutout')
            radii.append(surface.Cylinder().Radius())
        explorer.Next()
    return sorted(radii)


def check(cad,out,r,b,d):
    params={'seat_radius':r,'bore_radius':b,'depth':d}
    flags=[item for key,value in params.items() for item in ['--param',f'{key}={value}']]
    json_output([str(cad),'build',str(RECIPE),'--out-dir',str(out),*flags])
    require((out/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    for name,drilled in [('stock',False),('drilled_jaw',True)]:
        expected,center,tensor=jaw_properties(r,b,d,drilled)
        native=json_output([str(cad),'properties',str(RECIPE),'--output',name,*flags])
        for label,actual in [('native',(native['volume'],native['volume_centroid'],native['central_volume_inertia'])),('STEP',body_properties(out/f'{name}.step'))]:
            volume,c,inertia=actual;close(volume,expected,.003,label+' '+name+' volume')
            for a,value in zip(c,center):close(a,value,1e-5,label+' '+name+' centroid')
            for i in range(3):
                for j in range(3):close(inertia[i][j],tensor[i][j],2e-4*expected,label+' '+name+' inertia')
        step=(out/f'{name}.step').read_text();count=5 if drilled else 1
        require(step.count('CYLINDRICAL_SURFACE(')==count,'analytic cylinder supports lost')
        require(step.count('SEAM_CURVE(')==(4 if drilled else 0),'wrong periodic seam count')
        require(inward_cylinders(out/f'{name}.step')==sorted([float(r)]+([float(b)]*4 if drilled else [])),'imported inward cylinder radii differ')
        mesh(out/f'{name}.stl',expected)
    expected=jaw_properties(r,b,d)[0]
    manifest=json.loads((out/'machined_bearing_support.assembly.json').read_text())
    occurrences={i['id']:i for i in manifest['instances']}
    expected_placements={'lower_jaw':([0,0,0],[0,0,0,1]),'upper_jaw':([100,102,0],[0,0,1,0]),'shaft':([50,51,-12],[0,0,0,1])}
    for name,(p,q) in expected_placements.items():
        require(occurrences[name]['transform']['translation']==p,'incorrect '+name+' placement')
        require(occurrences[name]['transform']['rotation']==q,'incorrect '+name+' rotation')
    sleeves=[i for name,i in occurrences.items() if name not in expected_placements]
    require(sorted(tuple(i['transform']['translation']) for i in sleeves)==sorted((x,y,-2) for x in [12,88] for y in [12,40,62,90]),'incorrect sleeve placements')
    require(all(i['transform']['rotation']==[0,0,0,1] for i in sleeves),'incorrect sleeve rotation')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(out/'machined_bearing_support.step'),'--instances','11','--definitions','3','--solids','11','--assembly-json',str(out/'machined_bearing_support.assembly.json')])
    shaft=math.pi*(8*(r+3)**2+(d+16)*(r-2)**2)
    total=2*expected+shaft+8*math.pi*(2*b-1)*(d+4)
    close(imported['material_volume_mm3'],total,.003,'assembly volume')
    assembly_properties=json_output([str(cad),'properties',str(RECIPE),'--output','machined_bearing_support',*flags])
    mass=(2*expected*2700+(shaft+8*math.pi*(2*b-1)*(d+4))*7800)*1e-9
    close(assembly_properties['mass_kg'],mass,1e-8,'native material mass')
    for a,b in zip(assembly_properties['center_of_mass'],[50,51,d/2]):close(a,b,1e-5,'native material center')
    for a,b in zip(imported['bounds_mm'],[0,0,-12,100,102,d+12]):close(a,b,1e-5,'assembly bounds')
    volume,centroid,_=body_properties(out/'machined_bearing_support.step')
    close(volume,total,.003,'flattened assembly volume')
    for a,b in zip(centroid,[50,51,d/2]):close(a,b,1e-5,'assembly centroid')
    return dict(parameters=params,volume_mm3=total,closed_stl=True,inward_cylinders=5,hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='machined-bearing-ocp-') as directory:
        results=[check(args.cad.resolve(),pathlib.Path(directory)/str(i),*p) for i,p in enumerate([(18,3,18),(14,2,12),(22,4,24)])]
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
