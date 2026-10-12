"""Independent cylinder/frustum-integral and STEP assembly acceptance for round bearing cartridges."""
import argparse
import importlib.metadata
import json
import math
import pathlib
import platform
import sys
import tempfile
import OCP
from check_arc_prisms_ocp import mesh
from check_blind_pallet_ocp import cylinder, combine
from check_constrained_fixture_ocp import body_properties, close, json_output, require
from check_round_stock_ocp import cylinder_radii
from check_counterbores_ocp import verify
from check_countersinks_ocp import frustum, cone_faces
from check_step_metadata_ocp import check_metadata
from check_step_materials_ocp import check_materials

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT/'examples/countersunk_bearing_cover.star'


def centers(radius, count):
    return [(radius*math.cos(i*2*math.pi/count), radius*math.sin(i*2*math.pi/count)) for i in range(count)]


def flange_pieces(p,h,d,stage='flange'):
    result = [cylinder(60,d,[0,0,d/2]), cylinder(20,d,[0,0,d/2],-1)]
    if stage == 'stock':
        return result
    for x,y in centers(28,3):
        result.append(cylinder(3,5,[x,y,d-2.5],-1))
    if stage == 'sockets':
        return result
    for x,y in centers(p,6):
        if stage == 'piloted':
            result.append(cylinder(3.2,d,[x,y,d/2],-1))
        else:
            result.extend([cylinder(3.2,d-h,[x,y,(d-h)/2],-1), frustum(3.2,3.2+h,d-h,d,[x,y])])
    return result


def assembly_pieces(p,h,d):
    aluminum=[]
    steel=[]
    for top in [False,True]:
        def place(components):
            return [(v,[c[0],-c[1],2*d+40-c[2]] if top else c,t) for v,c,t in components]
        aluminum.extend(place(flange_pieces(p,h,d)))
        hardware=[cylinder(19.8,d-2,[0,0,d/2]), cylinder(10,d-2,[0,0,d/2],-1)]
        for x,y in centers(p,6):
            hardware.extend([cylinder(3,d-h+7,[x,y,(d-h-7)/2]),
                             frustum(3,3+h,d-h,d,[x,y],1)])
        steel.extend(place(hardware))
    for x,y in centers(28,3):
        steel.append(cylinder(2.8,50,[x,y,d+20]))
    steel.append(cylinder(9.8,2*d+60,[0,0,d+20]))
    return aluminum,steel


def volume_moments(values):
    volume,center,tensor=values
    return dict(volume_mm3=volume,centroid_mm=center,central_volume_inertia_mm5=tensor)


def check(cad,out,p,h,d,report_path=None):
    params=dict(bolt_pitch=p,seat_depth=h,flange_height=d)
    flags=[a for key,value in params.items() for a in ['--param',f'{key}={value}']]
    build=json_output([str(cad),'build',str(RECIPE),'--out-dir',str(out),*flags])
    queries={}
    require((out/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    for name in ['stock','sockets','piloted','flange']:
        expected=combine(flange_pieces(p,h,d,name))
        native=json_output([str(cad),'properties',str(RECIPE),'--output',name,*flags])
        verify((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,name+' native')
        step_properties=body_properties(out/f'{name}.step')
        verify(step_properties,expected,name+' STEP')
        queries[name]=dict(native=native,expected=volume_moments(expected),step=volume_moments(step_properties))
        mesh(out/f'{name}.stl',expected[0])
    inward,outward=cylinder_radii(out/'flange.step')
    wanted=sorted([20.]+[3.]*3+[3.2]*6)
    require(len(inward)==len(wanted) and all(abs(a-b)<1e-8 for a,b in zip(sorted(inward),wanted)), 'wrong stock and machining radii/orientations')
    require(len(outward)==2 and all(abs(r-60)<1e-8 for r in outward), 'wrong outer cylindrical stock wall')
    step=(out/'flange.step').read_text()
    require(step.count('PLANE(')==5,'three blind floors and two end caps')
    cone_faces(out/'flange.step',6)
    require(step.count('SEAM_CURVE(')==16,'periodic stock and bore seams missing')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(out/'countersunk_bearing_cover.step'),'--instances','20','--definitions','5','--solids','20','--assembly-json',str(out/'countersunk_bearing_cover.assembly.json')])
    metadata_report=check_metadata(out/'countersunk_bearing_cover.step',out/'countersunk_bearing_cover.assembly.json')
    material_report=check_materials(out/'countersunk_bearing_cover.step',out/'countersunk_bearing_cover.assembly.json')
    aluminum,steel=assembly_pieces(p,h,d)
    expected=combine(aluminum+steel)
    assembly_step=body_properties(out/'countersunk_bearing_cover.step')
    verify(assembly_step,expected,'assembly STEP')
    close(imported['material_volume_mm3'],expected[0],.003,'assembly volume')
    native=json_output([str(cad),'properties',str(RECIPE),'--output','countersunk_bearing_cover',*flags])
    verify((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,'assembly native')
    weighted=[(v*density,c,[a*density for a in t]) for components,density in [(aluminum,2700e-9),(steel,7800e-9)] for v,c,t in components]
    mass,center,tensor=combine(weighted)
    close(native['mass_kg'],mass,1e-8,'material mass')
    for a,b in zip(native['center_of_mass'],center):close(a,b,1e-6,'material center')
    for i in range(3):
        for j in range(3):close(native['central_mass_inertia'][i][j],tensor[i][j],max(1e-7,abs(tensor[i][j])*1e-9),'material inertia')
    for a,b in zip(imported['bounds_mm'],[-60,-60,-10,60,60,2*d+50]):close(a,b,1e-5,'assembly bounds')
    queries['countersunk_bearing_cover']=dict(
        native=native,expected=volume_moments(expected),step=volume_moments(assembly_step),
        expected_material=dict(mass_kg=mass,center_of_mass_mm=center,central_mass_inertia_kg_mm2=tensor))
    if report_path is not None:
        report=dict(valid=True,parameters=params,build=build,queries=queries,assembly_importer=imported,step_metadata=metadata_report,step_materials=material_report)
        with report_path.open('x') as destination:
            json.dump(report,destination,indent=2,allow_nan=False)
            destination.write('\n')
    return dict(parameters=params,volume_mm3=expected[0],closed_stl=True,inward_cylinders=10,blind_floors=3,inward_cones=6,hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'],metadata_checked=metadata_report['valid'],metadata_definitions=metadata_report['part_definitions_checked'],materials_checked=material_report['valid'],material_definitions=material_report['materials_checked'])


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
        for i,parameters in enumerate([(42,3,21),(40,2,18),(44,4,24)]):
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
        with tempfile.TemporaryDirectory(prefix='countersunk-bearing-cover-ocp-') as directory:
            results=run(pathlib.Path(directory),False)
    print(json.dumps(dict(valid=True,cases=results,oracle=oracle,
                          retained_generations=[str(i) for i in range(3)] if args.retain_dir else [],
                          retained_reports=[f'reports/{i}.json' for i in range(3)] if args.retain_dir else []),
                     indent=2,allow_nan=False))


if __name__=='__main__':main()
