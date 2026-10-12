"""Independent rectangle/capsule minus disks oracle for export_through_bores."""
import json
import math
import pathlib
import sys
from check_arc_prisms_ocp import rectangle,circle,shifted,mesh
from check_concave_holes_ocp import transform
from check_constrained_fixture_ocp import body_properties,require
from check_twin_lobe_ocp import verify_properties


def expected(name):
    if name=='rectangle':pieces=[rectangle(80,40,40,20)]
    else:
        radius=20;pieces=[rectangle(40,40,40,20)]
        for sign,x in [(-1,20),(1,60)]:
            pieces.append(shifted([math.pi*radius**2/2,sign*2*radius**3/3,0,math.pi*radius**4/8,math.pi*radius**4/8,0],x,20))
    pieces.extend([circle(3,20,20),circle(4,60,20)])
    area,mx,my,xx,yy,xy=[sum(p[i] for p in pieces) for i in range(6)]
    cx,cy=mx/area,my/area;h=8;v=area*h;xx-=area*cx*cx;yy-=area*cy*cy;xy-=area*cx*cy
    return v,[cx,cy,4],[[h*yy+v*h*h/12,-h*xy,0],[-h*xy,h*xx+v*h*h/12,0],[0,0,h*(xx+yy)]]


def main():
    directory=pathlib.Path(sys.argv[1]);results=[]
    for name in ['rectangle','capsule']:
        for oblique in [False,True]:
            stem=name+('_oblique' if oblique else '');properties=expected(name)
            if oblique:
                c,t=transform(properties[1],properties[2]);properties=(properties[0],c,t)
            native=json.loads((directory/f'{stem}.properties.json').read_text())
            verify_properties((native['volume'],native['centroid'],native['inertia']),properties,stem+' native')
            verify_properties(body_properties(directory/f'{stem}.step'),properties,stem+' STEP')
            step=(directory/f'{stem}.step').read_text();require(step.count('SEAM_CURVE(')==2,'two bores need exactly two seams')
            require(step.count('PCURVE(')==4,'two bores need four seam pcurves')
            require(sum('ADVANCED_FACE(' in l and l.endswith(',.F.);') for l in step.splitlines())==2,'two inward bore faces expected')
            mesh(directory/f'{stem}.stl',properties[0]);results.append(dict(fixture=stem,volume=properties[0],closed_stl=True))
    print(json.dumps(dict(valid=True,cases=results),indent=2))


if __name__=='__main__':main()
