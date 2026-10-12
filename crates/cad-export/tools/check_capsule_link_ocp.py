"""Independent solved capsule-link STEP oracle; optional OCP environment."""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_constrained_fixture_ocp import body_properties, close, json_output, require
from OCP.STEPControl import STEPControl_Reader
from OCP.IFSelect import IFSelect_RetDone
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/constrained_capsule_link.star'


def check(cad,output,span,r,bore):
    params={'span':span,'outer_radius':r,'bore_radius':bore}
    flags=[item for key,value in params.items() for item in ['--param',f'{key}={value}']]
    built=json_output([str(cad),'build',str(RECIPE),'--out-dir',str(output),*flags])
    require((output/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source differs')
    report=built['sketch_reports']['capsule_layout'];require(report['status']=='fully_constrained','unsolved layout')
    for name,point in [('a',[0,-r]),('b',[span,-r]),('c',[span,r]),('d',[0,r])]:
        for actual,expected in zip(report['points'][name],point):close(actual,expected,1e-5,'nominal capsule endpoint')
    pi=math.pi;v=8*(2*r*span+pi*r*r-2*pi*bore*bore)
    ix=2*span*r**3/3+pi*r**4/4-pi*bore**4/2
    iy=r*span**3/6+pi*r**4/4+4*span*r**3/3+pi*r*r*span*span/4-pi*bore**4/2-pi*bore*bore*span*span/2
    tensor=[[8*ix+v*64/12,0,0],[0,8*iy+v*64/12,0],[0,0,8*(ix+iy)]]
    actual,center,inertia=body_properties(output/'link.step');close(actual,v,.003,'STEP link volume')
    for a,b in zip(center,[span/2,0,4]):close(a,b,1e-5,'STEP link centroid')
    for i in range(3):
        for j in range(3):close(inertia[i][j],tensor[i][j],2e-4*v,'STEP central inertia')
    # Independently inspect outer arc and bore support types after STEP import.
    reader=STEPControl_Reader()
    require(reader.ReadFile(str(output/'link.step'))==IFSelect_RetDone,'STEP read failed')
    require(reader.TransferRoots()>0,'STEP transfer failed')
    faces=TopExp_Explorer(reader.OneShape(),TopAbs_FACE);radii=[]
    while faces.More():
        surface=BRepAdaptor_Surface(TopoDS.Face(faces.Current()))
        if surface.GetType()==GeomAbs_Cylinder:radii.append(surface.Cylinder().Radius())
        faces.Next()
    require(len(radii)==4,'expected two native arc walls and two bore walls')
    for a,b in zip(sorted(radii),[bore,bore,r,r]):close(a,b,1e-5,'analytic cylindrical support radius')
    total=2*v+20*pi*((r-2)**2-bore*bore)+2*pi*(6*(bore+2)**2+28*(bore-.5)**2)
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(output/'constrained_capsule_link.step'),'--instances','6','--definitions','3','--solids','6','--assembly-json',str(output/'constrained_capsule_link.assembly.json')])
    close(imported['material_volume_mm3'],total,.003,'STEP assembly volume')
    for a,b in zip(imported['bounds_mm'],[-r,-r,0,span+r,r,34]):close(a,b,1e-5,'STEP assembly bounds')
    return dict(parameters=params,volume_mm3=imported['material_volume_mm3'],hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='capsule-link-ocp-') as directory:
        results=[check(args.cad.resolve(),pathlib.Path(directory)/str(i),*p) for i,p in enumerate([(60,10,4),(40,8,5),(80,14,3)])]
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
