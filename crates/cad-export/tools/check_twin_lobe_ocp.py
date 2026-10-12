"""Independent circular-segment integrals for analytic cylinder Boolean export."""
import argparse
import json
import math
import pathlib
import sys
import tempfile
from check_constrained_fixture_ocp import body_properties,close,json_output,require
from check_arc_prisms_ocp import mesh
from check_concave_holes_ocp import transform
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/twin_lobe_fixture.star'


def segment(radius,lo,hi,offset=0):
    # Antiderivatives of 2*sqrt(r²-x²), x times that, x² times that,
    # and the slice integral of y². Exact segment integration, no CAD geometry.
    def primitive(x):
        root=math.sqrt(max(0,radius*radius-x*x));angle=math.asin(max(-1,min(1,x/radius)))
        return [x*root+radius*radius*angle,-2*root**3/3,
                (x*root*(2*x*x-radius*radius)+radius**4*angle)/4,
                (x*root*(5*radius*radius-2*x*x)+3*radius**4*angle)/12]
    lower,upper=primitive(lo),primitive(hi)
    a,mx,xx,yy=[b-a for a,b in zip(lower,upper)]
    return [a,mx+offset*a,xx+2*offset*mx+offset*offset*a,yy]


def regions(r,d):
    right=18;radical=(r*r-right*right+d*d)/(2*d)
    left_disk=segment(r,-r,r);right_disk=segment(right,-right,right,d)
    left_cap=segment(r,radical,r);right_cap=segment(right,-right,radical-d,d)
    overlap=[a+b for a,b in zip(left_cap,right_cap)]
    return dict(overlap=overlap,left_lobe=[a-b for a,b in zip(left_disk,overlap)],
                right_lobe=[a-b for a,b in zip(right_disk,overlap)],
                union_base=[a+b-c for a,b,c in zip(left_disk,right_disk,overlap)])


def mesh_error(r,d,h,key):
    radical=(r*r-18**2+d*d)/(2*d)
    alpha=math.acos(radical/r);beta=math.acos((d-radical)/18)
    perimeter={'overlap':2*r*alpha+36*beta,
               'left_lobe':2*r*(math.pi-alpha)+36*beta,
               'right_lobe':2*r*alpha+36*(math.pi-beta),
               'union_base':2*r*(math.pi-alpha)+36*(math.pi-beta)}[key]
    # Each straight tessellation chord lies within the exported 0.01 mm
    # sagitta policy of its circle. The area strip is bounded by arc length
    # times sagitta; extrusion multiplies this absolute area budget by height.
    return perimeter*0.01*h+0.001


def properties(m,h):
    area,mx,xx,yy=m;cx=mx/area;v=area*h;xx-=area*cx*cx
    return v,[cx,0,h/2],[[h*yy+v*h*h/12,0,0],[0,h*xx+v*h*h/12,0],[0,0,h*(xx+yy)]]


def verify_properties(actual,expected,label):
    v,c,t=expected;av,ac,at=actual;close(av,v,.003,label+' volume')
    for a,b in zip(ac,c):close(a,b,1e-5,label+' centroid')
    for i in range(3):
        for j in range(3):close(at[i][j],t[i][j],2e-4*v,label+' inertia')


def check(cad,out,r,d,h):
    params={'left_radius':r,'spacing':d,'depth':h};flags=[item for k,v in params.items() for item in ['--param',f'{k}={v}']]
    json_output([str(cad),'build',str(RECIPE),'--out-dir',str(out),*flags]);require((out/'recipe.star').read_bytes()==RECIPE.read_bytes(),'source mismatch')
    moments=regions(r,d)
    for name in ['left_lobe','overlap','union_base']:
        expected=properties(moments[name],4 if name=='union_base' else h)
        native=json_output([str(cad),'properties',str(RECIPE),'--output',name,*flags])
        verify_properties((native['volume'],native['volume_centroid'],native['central_volume_inertia']),expected,'native '+name)
        verify_properties(body_properties(out/f'{name}.step'),expected,'STEP '+name)
        require('SEAM_CURVE(' not in (out/f'{name}.step').read_text(),'trimmed arc wall incorrectly exported as periodic seam')
        mesh(out/f'{name}.stl',expected[0],mesh_error(r,d,4 if name=='union_base' else h,name))
    assembly=json.loads((out/'twin_lobe_fixture.assembly.json').read_text());instances={i['id']:i for i in assembly['instances']}
    expected_placements={'base':[0,0,0],'left_lobe':[0,0,4],'overlap':[0,0,4],'right_lobe':[0,0,4],'left_boss':[0,0,h+4],'right_boss':[d,0,h+4]}
    for name,p in expected_placements.items():
        require(instances[name]['transform']['translation']==p,'wrong '+name+' placement')
        require(instances[name]['transform']['rotation']==[0,0,0,1],'wrong '+name+' rotation')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(out/'twin_lobe_fixture.step'),'--instances','6','--definitions','5','--solids','6','--assembly-json',str(out/'twin_lobe_fixture.assembly.json')])
    union=properties(moments['union_base'],h+4);boss=48*math.pi;total=union[0]+2*boss
    center=[(union[0]*union[1][0]+boss*d)/total,0,(union[0]*(h+4)/2+2*boss*(h+6))/total]
    volume,centroid,_=body_properties(out/'twin_lobe_fixture.step');close(volume,total,.003,'assembly volume')
    for a,b in zip(centroid,center):close(a,b,1e-5,'assembly centroid')
    for a,b in zip(imported['bounds_mm'],[-r,-r,0,d+18,r,h+8]):close(a,b,1e-5,'assembly bounds')
    aluminum=union[0]-moments['overlap'][0]*h;steel=moments['overlap'][0]*h+2*boss
    native=json_output([str(cad),'properties',str(RECIPE),'--output','twin_lobe_fixture',*flags]);close(native['mass_kg'],(2700*aluminum+7800*steel)*1e-9,1e-8,'assembly material mass')
    return dict(parameters=params,volume_mm3=total,placements_checked=imported['placements_checked'],hierarchy_checked=imported['hierarchy_checked'])


def check_operand_frames(directory):
    for name,key in [('union','union_base'),('intersection','overlap'),('difference','left_lobe')]:
        for oblique in [False,True]:
            stem=name+('_oblique' if oblique else '');expected=properties(regions(22,30)[key],8)
            if oblique:
                c,t=transform(expected[1],expected[2]);expected=(expected[0],c,t)
            verify_properties(body_properties(directory/f'{stem}.step'),expected,stem)
            native=json.loads((directory/f'{stem}.properties.json').read_text());verify_properties((native['volume'],native['centroid'],native['inertia']),expected,'native '+stem)
            require('SEAM_CURVE(' not in (directory/f'{stem}.step').read_text(),'false partial-arc seam')
            mesh(directory/f'{stem}.stl',expected[0],mesh_error(22,30,8,key))


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cad',type=pathlib.Path,required=True);p.add_argument('--boolean-fixtures',type=pathlib.Path);a=p.parse_args()
    with tempfile.TemporaryDirectory(prefix='twin-lobe-ocp-') as directory:
        results=[check(a.cad.resolve(),pathlib.Path(directory)/str(i),*v) for i,v in enumerate([(22,30,8),(20,26,6),(24,34,10)])]
    if a.boolean_fixtures:check_operand_frames(a.boolean_fixtures)
    print(json.dumps(dict(valid=True,cases=results,transformed_operand_cases=6 if a.boolean_fixtures else 0),indent=2))


if __name__=='__main__':main()
