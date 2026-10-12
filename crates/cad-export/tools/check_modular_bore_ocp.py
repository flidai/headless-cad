"""Independent thirty-bore constraint-island STEP oracle; optional OCP required."""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_constrained_fixture_ocp import body_properties, close, json_output, require
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/modular_bore_plate.star'


def check(cad,output,pitch,r,t):
    params={'pitch':pitch,'bore_radius':r,'thickness':t}
    flags=[item for key,value in params.items() for item in ['--param',f'{key}={value}']]
    result=json_output([str(cad),'build',str(RECIPE),'--out-dir',str(output),*flags])
    report=result['sketch_reports']['modular_layout']
    require(report['schema_version']==3 and report['status']=='fully_constrained','unsolved layout')
    require(len(report['points'])==64 and len(report['islands'])==34,'incorrect decomposition')
    require(report['rank']==128 and sum(i['rank'] for i in report['islands'])==128,'incorrect rank aggregation')
    require(all(i['status']=='fully_constrained' for i in report['islands']),'failed island hidden')
    points=[p for i in report['islands'] for p in i['point_ids']]
    require(len(points)==len(set(points))==64 and set(points)==set(report['points']),'invalid island point partition')
    require((output/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    pi=math.pi;v=(27000-30*pi*r*r)*t
    ix=180*150**3/12-30*pi*r**4/4-60*pi*r*r*pitch*pitch
    iy=150*180**3/12-30*pi*r**4/4-87.5*pi*r*r*pitch*pitch
    tensor=[[t*ix+v*t*t/12,0,0],[0,t*iy+v*t*t/12,0],[0,0,t*(ix+iy)]]
    actual,center,inertia=body_properties(output/'plate.step');close(actual,v,.01,'STEP plate volume')
    for a,b in zip(center,[90,75,t/2]):close(a,b,1e-5,'STEP plate centroid')
    for i in range(3):
        for j in range(3):close(inertia[i][j],tensor[i][j],2e-4*v,'STEP plate central inertia')
    native=json.loads((output/'modular_bore_plate.assembly.json').read_text())
    occurrences={i['id']:i for i in native['instances']}
    for row in range(5):
        for col in range(6):
            placement=occurrences[f'guide_{row*6+col}']['transform']
            expected=[90+(col-2.5)*pitch,75+(row-2)*pitch,12]
            require(placement['translation']==expected,'guide placement mismatch')
            require(placement['rotation']==[0,0,0,1],'guide orientation mismatch')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(output/'modular_bore_plate.step'),'--instances','33','--definitions','3','--solids','33','--assembly-json',str(output/'modular_bore_plate.assembly.json')])
    close(imported['material_volume_mm3'],v+34560+30*pi*(2*r-1)*(t+4),.01,'STEP assembly volume')
    for a,b in zip(imported['bounds_mm'],[0,0,0,180,150,16+t]):close(a,b,1e-5,'STEP assembly bounds')
    return dict(parameters=params,islands=len(report['islands']),volume_mm3=imported['material_volume_mm3'],hierarchy_checked=imported['hierarchy_checked'],placements_checked=imported['placements_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='modular-bore-ocp-') as directory:
        results=[check(args.cad.resolve(),pathlib.Path(directory)/str(i),*p) for i,p in enumerate([(25,4,8),(22,3,6),(28,5,10)])]
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
