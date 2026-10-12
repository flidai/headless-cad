"""Independent stepped-cylinder moments and assembly exchange acceptance."""
import argparse
import importlib.metadata
import json
import pathlib
import platform
import sys
import tempfile
import OCP
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


def volume_moments(values):
    volume,center,tensor=values
    return dict(volume_mm3=volume,centroid_mm=center,central_volume_inertia_mm5=tensor)


def check(cad,out,r,h,d,report_path=None):
    params={'counterbore_radius':r,'counterbore_depth':h,'plate_height':d}
    flags=[a for key,value in params.items() for a in ['--param',f'{key}={value}']]
    build=json_output([str(cad),'build',str(RECIPE),'--out-dir',str(out),*flags])
    queries={}
    require((out/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    plate=plate_pieces(r,h,d)
    pilot=[box([0,0,0],[120,80,d],1)]+[cylinder(3.2,d,[x,y,d/2],-1) for x,y in CENTERS]
    for name,components in [('stock',[box([0,0,0],[120,80,d],1)]),('piloted',pilot),('plate',plate)]:
        expected=combine(components)
        native=json_output([str(cad),'properties',str(RECIPE),'--output',name,*flags])
        verify((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,name+' native')
        step_properties=body_properties(out/f'{name}.step')
        verify(step_properties,expected,name+' STEP')
        queries[name]=dict(native=native,expected=volume_moments(expected),step=volume_moments(step_properties))
        mesh(out/f'{name}.stl',expected[0])
    require(inward_cylinders(out/'plate.step')==sorted([float(r)]*6+[4.5]*6+[3.2]*6),'wrong stepped bore radii/orientations')
    step=(out/'plate.step').read_text()
    require(step.count('PLANE(')==18,'twelve shoulder planes missing')
    require(step.count('SEAM_CURVE(')==18,'eighteen periodic bore seams missing')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(out/'counterbored_fixture.step'),'--instances','32','--definitions','5','--solids','32','--assembly-json',str(out/'counterbored_fixture.assembly.json')])
    aluminum,steel=assembly_pieces(r,h,d)
    expected=combine(aluminum+steel)
    assembly_step=body_properties(out/'counterbored_fixture.step')
    verify(assembly_step,expected,'assembly STEP')
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
    queries['counterbored_fixture']=dict(
        native=native,expected=volume_moments(expected),step=volume_moments(assembly_step),
        expected_material=dict(mass_kg=mass,center_of_mass_mm=center,central_mass_inertia_kg_mm2=tensor))
    if report_path is not None:
        report=dict(valid=True,parameters=params,build=build,queries=queries,assembly_importer=imported)
        with report_path.open('x') as destination:
            json.dump(report,destination,indent=2,allow_nan=False)
            destination.write('\n')
    return dict(parameters=params,volume_mm3=expected[0],closed_stl=True,inward_bores=18,annular_shoulders=12,hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=pathlib.Path,required=True)
    parser.add_argument('--retain-dir',type=pathlib.Path,
                        help='Retain generations and reports in a new directory; never overwrite an existing path')
    args=parser.parse_args()
    oracle=dict(python_version=platform.python_version(),
                cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'),
                ocp_version=str(OCP.__version__))

    def run(directory,retain):
        if retain:
            (directory/'reports').mkdir()
        results=[]
        for i,parameters in enumerate([(7,6,21),(6,4,18),(8,8,24)]):
            report=directory/'reports'/f'{i}.json' if retain else None
            result=check(args.cad.resolve(),directory/str(i),*parameters,report_path=report)
            if retain:
                result.update(generation=str(i),report=f'reports/{i}.json')
            results.append(result)
        return results

    if args.retain_dir is not None:
        # Atomic mkdir rejects directories, files, and dangling symlinks alike.
        # Failures intentionally leave diagnostics but never print valid:true.
        args.retain_dir.mkdir(parents=True,exist_ok=False)
        results=run(args.retain_dir.resolve(),True)
    else:
        with tempfile.TemporaryDirectory(prefix='counterbored-fixture-ocp-') as directory:
            results=run(pathlib.Path(directory),False)
    print(json.dumps(dict(valid=True,cases=results,oracle=oracle,
                          retained_generations=[str(i) for i in range(3)] if args.retain_dir else [],
                          retained_reports=[f'reports/{i}.json' for i in range(3)] if args.retain_dir else []),
                     indent=2,allow_nan=False))


if __name__=='__main__':main()
