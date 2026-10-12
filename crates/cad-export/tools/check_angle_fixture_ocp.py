"""Independent angular-sketch/body/assembly STEP acceptance; optional OCP only.

Usage: python check_angle_fixture_ocp.py --cad target/debug/cad --out-dir /tmp/angle-oracle
Three configurations use trigonometry and closed-form triangle/trapezoid moments,
not solver output, for nominal geometry. Captured solver residuals are independently
recomputed from returned world coordinates with separate linear/angular units.
"""
import argparse
import itertools
import json
import math
import pathlib
import sys
import tempfile
import subprocess
from check_constrained_fixture_ocp import body_properties, close, json_output, require
from check_assembly_ocp import placement_matrix
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/constrained_angle_fixture.star'


def vector(a,b):return [b[i]-a[i] for i in range(2)]
def angle(a,b):return math.atan2(a[0]*b[1]-a[1]*b[0],a[0]*b[0]+a[1]*b[1])
def wrap(value,period):return (value+period/2)%period-period/2


def evidence(report,linear,angular,tolerance):
    require(report['schema_version']==2 and report['status']=='fully_constrained','invalid solve status/schema')
    require(report['degrees_of_freedom']==0 and report['condition_bound']<=1e8,'uncertified rank/conditioning')
    rows={r['id']:r for r in report['constraint_residuals']}
    require(rows.keys()==linear.keys()|angular.keys(),'constraint evidence set mismatch')
    for name,value in linear.items():
        require(value<=tolerance,f'{name} actual linear residual exceeds tolerance')
        require('residual_radians' not in rows[name],'linear constraint labeled angular')
        close(rows[name]['residual_mm'],value,1e-11,f'{name} linear residual')
    for name,value in angular.items():
        require(value<=1e-9,f'{name} actual angular residual exceeds tolerance')
        require('residual_mm' not in rows[name],'angular constraint labeled linear')
        close(rows[name]['residual_radians'],value,1e-13,f'{name} angular residual')
    close(report['max_residual_mm'],max(linear.values()),1e-11,'maximum linear residual')
    close(report['max_angular_residual_radians'],max(angular.values()),1e-13,'maximum angular residual')
    require(report['angular_tolerance_radians']==1e-9,'angular policy changed')


def check(cad,root,name,span,degrees,top):
    directory=root/name
    params={'span':span,'tilt_degrees':degrees,'deck_top_width':top}
    flags=sum((['--param',f'{k}={v}'] for k,v in params.items()),[])
    subprocess.run([str(cad),'build',str(RECIPE),'--out-dir',str(directory),*flags],check=True,capture_output=True,text=True)
    manifest=json.loads((directory/'manifest.json').read_text())
    require((directory/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source mismatch')
    require(manifest['recipe_source']['parameter_overrides']==params,'captured parameter mismatch')
    reports=manifest['sketch_reports'];tolerance=manifest['tolerance']['linear']
    support=reports['support_sketch'];deck=reports['deck_sketch']
    require(support['rank']==6 and deck['rank']==8,'unexpected sketch rank')
    for r in reports.values():require(r['solver_build_id']==manifest['solver_build_id'],'solver identity mismatch')
    a,b,c=[support['points'][k] for k in 'abc'];ab=vector(a,b);bc=vector(b,c);ac=vector(a,c)
    evidence(support,{'ground':math.hypot(*a),'base_level':abs(a[1]-b[1]),'base_length':abs(math.dist(a,b)-span)},
             {'upright':abs(wrap(angle(ab,bc)-math.pi/2,math.pi)),'incline':abs(wrap(angle(ab,ac)-math.radians(degrees),2*math.pi))},tolerance)
    a,b,c,d=[deck['points'][k] for k in 'abcd']
    evidence(deck,{'ground':math.hypot(*a),'base_level':abs(a[1]-b[1]),'base_width':abs(math.dist(a,b)-36),'top_width':abs(math.dist(d,c)-top),'equal_legs':abs(math.dist(a,d)-math.dist(b,c)),'leg_length':abs(math.dist(a,d)-44)},
             {'top_parallel':abs(wrap(angle(vector(a,b),vector(d,c)),math.pi))},tolerance)
    theta=math.radians(degrees);height=span*math.tan(theta)
    k=(36-top)/2;h=math.sqrt(44**2-k*k);area=(36+top)*h/2;cy=h*(36+2*top)/(3*(36+top))
    sv=30*span*height;dv=4*area
    sx=span*span/18;sz=height*height/18;cov=span*height/36
    si=[[sv*(sz+300),0,-sv*cov],[0,sv*(sx+sz),0],[-sv*cov,0,sv*(sx+300)]]
    dx=h*(36**3+36**2*top+36*top**2+top**3)/(48*area)
    dy=h**3*(36+3*top)/(12*area)-cy*cy
    di=[[dv*(dy+16/12),0,0],[0,dv*(dx+16/12),0],[0,0,dv*(dx+dy)]]
    # Fixed corpus accuracy budgets, independent of the solver's mixed-unit
    # Jacobian condition estimate (which is not a coordinate-error bound).
    linear_error=1e-5
    volume_error=0.003
    for body,volume,center,inertia in [('support',sv,[2*span/3,30,height/3],si),('deck',dv,[18,cy,2],di)]:
        native=json_output([str(cad),'properties',str(RECIPE),'--output',body,*flags])
        imported=body_properties(directory/f'{body}.step')
        actuals=[(native['volume'],native['volume_centroid'],native['central_volume_inertia']),imported]
        for actual_volume,actual_center,actual_inertia in actuals:
            close(actual_volume,volume,volume_error,f'{body} volume')
            for actual,expected in zip(actual_center,center):close(actual,expected,linear_error,f'{body} centroid')
            for i in range(3):
                for j in range(3):close(actual_inertia[i][j],inertia[i][j],2e-4*volume,f'{body} inertia {i},{j}')
    co,si=math.cos(theta),math.sin(theta)
    rotation=[[co,0,-si],[0,1,0],[si,0,co]]
    def rotate(p):return [sum(rotation[i][j]*p[j] for j in range(3)) for i in range(3)]
    seat=[10+span/2,40,6+height/2];shift=rotate([18,cy,0]);origin=[seat[i]-shift[i] for i in range(3)]
    def placed(p):return [v+origin[i] for i,v in enumerate(rotate(p))]
    assembly_path=directory/'constrained_angle_fixture.assembly.json';assembly=json.loads(assembly_path.read_text())
    instances={item['id']:item for item in assembly['instances']}
    matrix=placement_matrix(instances['deck']['transform'])
    for i in range(3):
        close(matrix[i][3],origin[i],linear_error,'deck placement origin')
        for j in range(3):close(matrix[i][j],rotation[i][j],linear_error/36,'deck placement rotation')
    vertices=[[0,0,0],[span+20,80,6]]
    vertices.extend([[10+x,10+y,6+z] for x,z in [(0,0),(span,0),(span,height)] for y in [0,60]])
    vertices.extend(placed([x,y,z]) for x,y in [(0,0),(36,0),(36-k,h),(k,h)] for z in [0,4])
    for pad_name,cx,nx in [('left_pad',k/2,-h/44),('right_pad',36-k/2,h/44)]:
        normal=[nx,k/44,0];center=[cx,h/2,2];local_rotation=[[0,normal[1],normal[0]],[0,-normal[0],normal[1]],[1,0,0]]
        def padpoint(p):return placed([center[i]+local_rotation[i][0]*(p[0]-1)+local_rotation[i][1]*(p[1]-4)+local_rotation[i][2]*p[2] for i in range(3)])
        expected_origin=padpoint([0,0,0]);matrix=placement_matrix(instances[pad_name]['transform'])
        for i in range(3):
            close(matrix[i][3],expected_origin[i],linear_error,'pad placement origin')
            for j in range(3):close(matrix[i][j],sum(rotation[i][q]*local_rotation[q][j] for q in range(3)),linear_error/36,'pad placement rotation')
        vertices.extend(padpoint(p) for p in itertools.product([0,2],[0,8],[0,5]))
    vertices.extend([[6-4,6-4,0],[span+14+4,74+4,9]])
    bounds=[min(p[i] for p in vertices) for i in range(3)]+[max(p[i] for p in vertices) for i in range(3)]
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(directory/'constrained_angle_fixture.step'),'--instances','9','--definitions','5','--solids','9','--assembly-json',str(assembly_path)])
    total=480*span+8736+sv+dv+160+342*math.pi
    close(imported['material_volume_mm3'],total,volume_error,'assembly volume')
    for a,b in zip(imported['bounds_mm'],bounds):close(a,b,linear_error,'assembly bounds')
    return {'case':name,'parameters':params,'volume_mm3':imported['material_volume_mm3'],'bounds_mm':imported['bounds_mm'],'linear_tolerance_mm':linear_error,'solver_build_id':manifest['solver_build_id'],'support_sketch_hash':manifest['feature_hashes']['support_sketch'],'deck_sketch_hash':manifest['feature_hashes']['deck_sketch'],'hierarchy_checked':imported['hierarchy_checked'],'placements_checked':imported['placements_checked']}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);parser.add_argument('--out-dir',type=pathlib.Path);args=parser.parse_args()
    temporary=tempfile.TemporaryDirectory(prefix='angle-fixture-') if args.out_dir is None else None
    root=args.out_dir.resolve() if args.out_dir else pathlib.Path(temporary.name);root.mkdir(parents=True,exist_ok=True)
    results=[check(args.cad.resolve(),root,*case) for case in [('default',60,30,24),('low',50,15,18),('high',80,45,30)]]
    require(len({r['solver_build_id'] for r in results})==1,'solver changed during check')
    require(len({r['support_sketch_hash'] for r in results})==3 and len({r['deck_sketch_hash'] for r in results})==3,'edits did not change profile identity')
    print(json.dumps({'valid':True,'cases':results},indent=2))
    if temporary:temporary.cleanup()


if __name__=='__main__':main()
