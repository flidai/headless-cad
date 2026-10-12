"""Independent signed-cylinder and nested assembly acceptance for blind_bore_pallet."""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_arc_prisms_ocp import mesh
from check_concave_holes_ocp import box
from check_constrained_fixture_ocp import body_properties, close, json_output, require
from check_machined_bearing_ocp import inward_cylinders
from check_twin_lobe_ocp import verify_properties
ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT/'examples/blind_bore_pallet.star'
TOP = [(x,y) for y in [20,60] for x in [20,60,100]]
BOTTOM = [(40,40),(80,40)]
THROUGH = [(x,y) for y in [8,72] for x in [8,112]]


def cylinder(r,h,c,sign=1):
    v=sign*math.pi*r*r*h
    return v,c,[v*(3*r*r+h*h)/12]*2+[v*r*r/2]


def combine(pieces):
    v=sum(p[0] for p in pieces)
    c=[sum(p[0]*p[1][i] for p in pieces)/v for i in range(3)]
    tensor=[[0.]*3 for _ in range(3)]
    for mass,center,diagonal in pieces:
        delta=[center[i]-c[i] for i in range(3)]
        for i in range(3):
            for j in range(3):
                tensor[i][j]+=(diagonal[i]+mass*sum(delta[k]**2 for k in range(3) if k!=i)) if i==j else -mass*delta[i]*delta[j]
    return v,c,tensor


def pieces(r,h,d):
    plate=[box([0,0,0],[120,80,d],1)]
    plate += [cylinder(r,h,[x,y,d-h/2],-1) for x,y in TOP]
    plate += [cylinder(4,6,[x,y,3],-1) for x,y in BOTTOM]
    plate += [cylinder(2.5,d,[x,y,d/2],-1) for x,y in THROUGH]
    steel=[]
    for x,y in TOP:
        steel += [cylinder(r-.5,h+10,[x,y,d-h+(h+10)/2]),cylinder(r+2,4,[x,y,d+12])]
    steel += [cylinder(3.5,16,[x,y,-2]) for x,y in BOTTOM]
    for x,y in THROUGH:
        steel += [cylinder(2.5,d+2,[x,y,d/2]),cylinder(1.5,d+2,[x,y,d/2],-1)]
    return plate,steel


def check(cad,out,r,h,d):
    params={'socket_radius':r,'socket_depth':h,'plate_height':d}
    flags=[a for key,value in params.items() for a in ['--param',f'{key}={value}']]
    json_output([str(cad),'build',str(RECIPE),'--out-dir',str(out),*flags])
    require((out/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    plate,steel=pieces(r,h,d)
    for name,components in [('stock',[box([0,0,0],[120,80,d],1)]),('pallet',plate)]:
        expected=combine(components)
        native=json_output([str(cad),'properties',str(RECIPE),'--output',name,*flags])
        verify_properties((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,name+' native')
        verify_properties(body_properties(out/f'{name}.step'),expected,name+' STEP')
        mesh(out/f'{name}.stl',expected[0])
    require(inward_cylinders(out/'pallet.step')==sorted([float(r)]*6+[4.]*2+[2.5]*4),'wrong imported bore radii/orientations')
    step=(out/'pallet.step').read_text()
    require(step.count('PLANE(')==14,'eight blind floors missing')
    require(step.count('SEAM_CURVE(')==12,'twelve periodic bore seams missing')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(out/'blind_bore_pallet.step'),'--instances','26','--definitions','4','--solids','26','--assembly-json',str(out/'blind_bore_pallet.assembly.json')])
    pair=[];weighted=[]
    for offset in [0,150]:
        for components,density in [(plate,2700e-9),(steel,7800e-9)]:
            for v,c,i in components:
                c=[c[0]+offset,c[1],c[2]]
                pair.append((v,c,i))
                weighted.append((v*density,c,[a*density for a in i]))
    expected=combine(pair)
    verify_properties(body_properties(out/'blind_bore_pallet.step'),expected,'assembly STEP')
    close(imported['material_volume_mm3'],expected[0],.003,'assembly material volume')
    native=json_output([str(cad),'properties',str(RECIPE),'--output','blind_bore_pallet',*flags])
    verify_properties((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,'assembly native')
    mass,center,tensor=combine(weighted)
    close(native['mass_kg'],mass,1e-8,'material mass')
    for a,b in zip(native['center_of_mass'],center):close(a,b,1e-6,'material center')
    for i in range(3):
        for j in range(3):close(native['central_mass_inertia'][i][j],tensor[i][j],max(1e-7,abs(tensor[i][j])*1e-9),'material inertia')
    for a,b in zip(imported['bounds_mm'],[0,0,-10,270,80,d+14]):close(a,b,1e-5,'assembly bounds')
    return dict(parameters=params,volume_mm3=expected[0],closed_stl=True,inward_bores=12,blind_floors=8,hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=pathlib.Path,required=True)
    args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='blind-pallet-ocp-') as directory:
        results=[check(args.cad.resolve(),pathlib.Path(directory)/str(i),*p) for i,p in enumerate([(8,12,28),(6,8,24),(10,16,32)])]
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
