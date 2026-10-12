"""Independent wedge-tangency STEP oracle. Requires optional OCP environment."""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_constrained_fixture_ocp import body_properties, close, json_output, require
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/tangent_bore_fixture.star'


def check(cad, output, width, radius, t):
    params={'half_width':width,'bore_radius':radius,'thickness':t}
    flags=[item for name,value in params.items() for item in ['--param',f'{name}={value}']]
    built=json_output([str(cad),'build',str(RECIPE),'--out-dir',str(output),*flags])
    require((output/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    report=built['sketch_reports']['bore_layout']
    require(report['status']=='fully_constrained' and report['degrees_of_freedom']==0,'unsolved fixture')
    d=radius*math.hypot(width,60)/width
    for name,y in [('lower_center',12+d),('upper_center',68-d)]:
        p=report['points'][name]
        close(p[0],50,1e-5,'nominal center x');close(p[1],y,1e-5,'nominal center y')
    # Recompute every tangency witness from returned world coordinates.
    recipe=json_output([str(cad),'compile',str(RECIPE),*flags])
    # CLI compile has a recipe wrapper; support the documented direct IR too.
    ir=recipe.get('recipe',recipe)
    feature=next(f for f in ir['features'] if f['id']=='bore_layout')
    rows={row['id']:row for row in report['constraint_residuals']}
    for c in feature['sketch']['constraints']:
        if c['kind']!='circle_line_tangent':continue
        center,rim=[report['points'][name] for name in c['circle']]
        a,b=[report['points'][name] for name in c['line']]
        u=[b[i]-a[i] for i in range(2)];v=[center[i]-a[i] for i in range(2)]
        signed=(u[0]*v[1]-u[1]*v[0])/math.hypot(*u)
        residual=abs(signed-(1 if c['side']=='left' else -1)*math.dist(center,rim))
        require(residual<=report['tolerance_mm'],'tangent residual exceeds policy')
        close(rows[c['id']]['residual_mm'],residual,1e-11,'tangent witness')
        require('residual_radians' not in rows[c['id']],'tangency is a dimensional equation')
    v=(8000-2*math.pi*radius**2)*t
    ix=100*80**3/12-2*(math.pi*radius**4/4+math.pi*radius**2*(28-d)**2)
    iy=80*100**3/12-2*math.pi*radius**4/4
    tensor=[[t*ix+v*t*t/12,0,0],[0,t*iy+v*t*t/12,0],[0,0,t*(ix+iy)]]
    actual,center,inertia=body_properties(output/'plate.step')
    close(actual,v,.003,'STEP plate volume')
    for a,b in zip(center,[50,40,t/2]):close(a,b,1e-5,'STEP centroid')
    for i in range(3):
        for j in range(3):close(inertia[i][j],tensor[i][j],2e-4*v,'STEP central inertia')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(output/'tangent_bore_fixture.step'),'--instances','7','--definitions','3','--solids','7','--assembly-json',str(output/'tangent_bore_fixture.assembly.json')])
    close(imported['material_volume_mm3'],v+19200+768,.003,'STEP assembly volume')
    for a,b in zip(imported['bounds_mm'],[0,0,0,100,80,16+t]):close(a,b,1e-5,'STEP assembly bounds')
    return dict(parameters=params,volume_mm3=imported['material_volume_mm3'],placements_checked=imported['placements_checked'],hierarchy_checked=imported['hierarchy_checked'])


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='tangent-ocp-') as directory:
        results=[check(args.cad.resolve(),pathlib.Path(directory)/str(i),*p) for i,p in enumerate([(30,4,8),(20,6,6),(35,3,10)])]
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
