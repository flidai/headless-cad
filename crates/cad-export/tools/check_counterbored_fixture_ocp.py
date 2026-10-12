"""Independent stepped-cylinder moments and assembly exchange acceptance."""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_arc_prisms_ocp import mesh
from check_blind_pallet_ocp import cylinder, combine
from check_concave_holes_ocp import box
from check_constrained_fixture_ocp import body_properties, close, json_output, require
from check_machined_bearing_ocp import inward_cylinders
from check_counterbores_ocp import verify
ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT/'examples/counterbored_fixture.star'
CENTERS = [(x,y) for y in [20,60] for x in [20,60,100]]


def plate_pieces(r,h,d):
    result = [box([0,0,0],[120,80,d],1)]
    for x,y in CENTERS:
        for radius, low, high in [(4.5,0,3),(3.2,3,d-h),(r,d-h,d)]:
            result.append(cylinder(radius,high-low,[x,y,(low+high)/2],-1))
    return result


def assembly_pieces(r,h,d):
    aluminum=[]
    for v,c,t in plate_pieces(r,h,d):
        aluminum.append((v,[c[0],c[1],c[2]+d+30],t))
        aluminum.append((v,[c[0],80-c[1],d-c[2]],t))
    steel=[]
    length=2*d+30-2*h+7
    for x,y in CENTERS:
        # All hardware is rotationally symmetric; reversing the bottom washer
        # and nut keeps their diagonal central tensors unchanged.
        for outer,inner,height,z in [(r-.3,3.1,1,2*d+30-h+.5),
                                     (r-.3,3.1,1,h-.5),
                                     (r-1,3.1,4,h-3),
                                     (4.2,3.2,36,d+15)]:
            steel.append(cylinder(outer,height,[x,y,z]))
            steel.append(cylinder(inner,height,[x,y,z],-1))
        steel.append(cylinder(3,length,[x,y,h-6+length/2]))
        steel.append(cylinder(r-1,3,[x,y,2*d+30-h+2.5]))
    return aluminum,steel


def check(cad,out,r,h,d):
    params={'counterbore_radius':r,'counterbore_depth':h,'plate_height':d}
    flags=[a for key,value in params.items() for a in ['--param',f'{key}={value}']]
    json_output([str(cad),'build',str(RECIPE),'--out-dir',str(out),*flags])
    require((out/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    plate=plate_pieces(r,h,d)
    pilot=[box([0,0,0],[120,80,d],1)]+[cylinder(3.2,d,[x,y,d/2],-1) for x,y in CENTERS]
    for name,components in [('stock',[box([0,0,0],[120,80,d],1)]),('piloted',pilot),('plate',plate)]:
        expected=combine(components)
        native=json_output([str(cad),'properties',str(RECIPE),'--output',name,*flags])
        verify((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,name+' native')
        verify(body_properties(out/f'{name}.step'),expected,name+' STEP')
        mesh(out/f'{name}.stl',expected[0])
    require(inward_cylinders(out/'plate.step')==sorted([float(r)]*6+[4.5]*6+[3.2]*6),'wrong stepped bore radii/orientations')
    step=(out/'plate.step').read_text()
    require(step.count('PLANE(')==18,'twelve shoulder planes missing')
    require(step.count('SEAM_CURVE(')==18,'eighteen periodic bore seams missing')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(out/'counterbored_fixture.step'),'--instances','32','--definitions','5','--solids','32','--assembly-json',str(out/'counterbored_fixture.assembly.json')])
    aluminum,steel=assembly_pieces(r,h,d)
    expected=combine(aluminum+steel)
    verify(body_properties(out/'counterbored_fixture.step'),expected,'assembly STEP')
    close(imported['material_volume_mm3'],expected[0],.003,'assembly material volume')
    native=json_output([str(cad),'properties',str(RECIPE),'--output','counterbored_fixture',*flags])
    verify((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,'assembly native')
    weighted=[(v*density,c,[a*density for a in t]) for components,density in [(aluminum,2700e-9),(steel,7800e-9)] for v,c,t in components]
    mass,center,tensor=combine(weighted)
    close(native['mass_kg'],mass,1e-8,'material mass')
    for a,b in zip(native['center_of_mass'],center):close(a,b,1e-6,'material center')
    for i in range(3):
        for j in range(3):close(native['central_mass_inertia'][i][j],tensor[i][j],max(1e-7,abs(tensor[i][j])*1e-9),'material inertia')
    for a,b in zip(imported['bounds_mm'],[0,0,min(0,h-6),120,80,2*d+30]):close(a,b,1e-5,'assembly bounds')
    return dict(parameters=params,volume_mm3=expected[0],closed_stl=True,inward_bores=18,annular_shoulders=12,hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=pathlib.Path,required=True)
    args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='counterbored-fixture-ocp-') as directory:
        results=[check(args.cad.resolve(),pathlib.Path(directory)/str(i),*p) for i,p in enumerate([(7,6,21),(6,4,18),(8,8,24)])]
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
