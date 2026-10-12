"""Independent OCCT plane intersections for exact native sections.

OCCT builds section edges from imported STEP, connects closed wires, constructs
planar faces and integrates their areas. These fixtures each have one outer
region with disjoint holes; nesting signs follow the independently specified
fixture, never native section orientation. No display triangles are queried.
Reference: https://dev.opencascade.org/doc/refman/html/class_b_rep_algo_a_p_i___section.html
"""
import argparse
import importlib.metadata
import json
import math
import pathlib
import platform
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import OCP
from OCP.BRepAlgoAPI import BRepAlgoAPI_Section
from OCP.BRepAdaptor import BRepAdaptor_Curve
from OCP.GeomAbs import GeomAbs_Circle, GeomAbs_Line
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeFace
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.ShapeAnalysis import ShapeAnalysis_FreeBounds
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_EDGE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from OCP.collections import HSequence_TopoDS_Shape
from OCP.gp import gp_Dir, gp_Pln, gp_Pnt
from check_constrained_fixture_ocp import require
from check_step_metadata_ocp import command

ROOT=pathlib.Path(__file__).resolve().parents[3]
APPEND='''
# Independent section fixture additions; retained together with the authored source.
output(name = "section_oblique", feature = transform(id = "section_oblique", body = finished, offset = [12,-7,4], rotation = [0.5,0.5,0,0.7071067811865476]))
output(name = "section_box", feature = extrude(id = "section_box", profile = rectangle(id = "section_box_profile", width = 10, depth = 10), height = 10))
'''


def close(a,b,label):
    require(math.isfinite(a) and abs(a-b)<=max(2e-7,abs(b)*2e-10),label+f': {a} != {b}')


def dot(a,b):return sum(x*y for x,y in zip(a,b))
def cross(a,b):return [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]
def project(p,plane):
    delta=[a-b for a,b in zip(p,plane['origin'])]
    return [dot(delta,plane['x_direction']),dot(delta,cross(plane['normal'],plane['x_direction']))]

def rigid(p,translation=True):
    q=math.sqrt(.5)
    return [dot(row,p)+(offset if translation else 0) for row,offset in zip([[.5,.5,q],[.5,.5,-q],[-q,q,0]],[12,-7,4])]


def imported_section(path,plane):
    reader=STEPControl_Reader()
    require(reader.ReadFile(str(path))==IFSelect_RetDone and reader.TransferRoots()==1,'STEP import failed')
    return shape_section(reader.OneShape(),plane)


def shape_section(shape,plane):
    """Fixture contract: at most one outer material loop and disjoint holes."""
    support=gp_Pln(gp_Pnt(*plane['origin']),gp_Dir(*plane['normal']))
    operation=BRepAlgoAPI_Section(shape,support)
    operation.Build();require(operation.IsDone(),'OCCT section failed')
    edges=HSequence_TopoDS_Shape()
    explorer=TopExp_Explorer(operation.Shape(),TopAbs_EDGE)
    curves=[]
    while explorer.More():
        edge=TopoDS.Edge(explorer.Current());edges.Append(edge)
        curve=BRepAdaptor_Curve(edge)
        if curve.GetType()==GeomAbs_Circle:
            circle=curve.Circle();c=circle.Location()
            curves.append(dict(kind='arc',center=project([c.X(),c.Y(),c.Z()],plane),radius=circle.Radius(),sweep=curve.LastParameter()-curve.FirstParameter()))
        elif curve.GetType()==GeomAbs_Line:
            a=curve.Value(curve.FirstParameter());b=curve.Value(curve.LastParameter())
            curves.append(dict(kind='line',start=project([a.X(),a.Y(),a.Z()],plane),end=project([b.X(),b.Y(),b.Z()],plane)))
        else:raise RuntimeError('OCCT section introduced a non-line/non-circle intersection')
        explorer.Next()
    wires=ShapeAnalysis_FreeBounds.ConnectEdgesToWires_s(edges,1e-7,False)
    loops=[]
    for i in range(1,wires.Length()+1):
        wire=TopoDS.Wire(wires.Value(i));require(wire.Closed(),'OCCT section has an open wire')
        face=BRepBuilderAPI_MakeFace(support,wire,True)
        require(face.IsDone() and BRepCheck_Analyzer(face.Face()).IsValid(),'OCCT section face invalid')
        properties=GProp_GProps();BRepGProp.SurfaceProperties_s(face.Face(),properties)
        c=properties.CentreOfMass()
        loops.append(dict(area=abs(properties.Mass()),centroid=project([c.X(),c.Y(),c.Z()],plane)))
    loops.sort(key=lambda loop:loop['area'],reverse=True)
    area=sum(loop['area']*(1 if i==0 else -1) for i,loop in enumerate(loops))
    centroid=None if not loops else [sum(loop['area']*loop['centroid'][j]*(1 if i==0 else -1) for i,loop in enumerate(loops))/area for j in range(2)]
    return dict(area=area,centroid=centroid,loops=loops,edge_count=edges.Length(),curves=curves)


def expected_rings(z):
    holes=[(0.,0.,20.)]
    holes += [(42*math.cos(i*math.pi/3),42*math.sin(i*math.pi/3),3.2+max(0,z-18)) for i in range(6)]
    if z>16:holes += [(28*math.cos(i*2*math.pi/3),28*math.sin(i*2*math.pi/3),3.) for i in range(3)]
    # Plane origin is shifted [3,-4,z] without changing section geometry.
    return [(x-3,y+4,r) for x,y,r in [(0.,0.,60.)]+holes]


def match_rings(section,rings):
    require(len(section['loops'])==len(rings),'native ring count')
    unused=list(rings)
    for boundary in section['loops']:
        curves=[segment['curve'] for segment in boundary['segments']]
        require(curves and all(c['kind']=='arc' for c in curves),'circle section must stay analytic')
        first=curves[0];center=first['center'];radius=first['radius']
        for curve in curves:
            close(curve['radius'],radius,'ring radius continuity')
            for a,b in zip(curve['center'],center):close(a,b,'ring center continuity')
        angle=sum(c['sweep_angle'] for c in curves)
        close(abs(angle),2*math.pi,'complete native circle')
        require((angle>0)==(radius==60.),'outer/hole winding')
        found=next((i for i,(x,y,r) in enumerate(unused) if abs(x-center[0])<1e-7 and abs(y-center[1])<1e-7 and abs(r-radius)<1e-7),None)
        require(found is not None,'unexpected native circular contour');unused.pop(found)


def section_argv(cad,recipe,output,plane,format='json'):
    return [str(cad),'section',str(recipe),'--output',output,'--origin',','.join(map(str,plane['origin'])),'--normal',','.join(map(str,plane['normal'])),'--x-direction',','.join(map(str,plane['x_direction'])),'--format',format]


def check(cad,directory):
    recipe=directory/'source.star'
    recipe.write_text((ROOT/'examples/countersunk_bearing_cover.star').read_text()+APPEND)
    generation=directory/'generation'
    built=command([cad,'build',recipe,'--out-dir',generation])
    cases=[]
    queries=[]
    for oblique in [False,True]:
        for z in [8.,17.,19.5]:
            plane=dict(origin=[3.,-4.,z],normal=[0.,0.,1.],x_direction=[1.,0.,0.])
            if oblique:plane={key:rigid(value,key=='origin') for key,value in plane.items()}
            queries.append((('section_oblique' if oblique else 'flange'),plane,z))
    q=math.sqrt(.5);n=1/math.sqrt(3)
    queries.append(('section_box',dict(origin=[6.,0.,0.],normal=[n,n,n],x_direction=[q,-q,0.]),None))
    queries.append(('section_box',dict(origin=[0.,0.,15.],normal=[0.,0.,1.],x_direction=[1.,0.,0.]),'empty'))
    for i,(output,plane,z) in enumerate(queries):
        args=section_argv(cad,recipe,output,plane)
        native=command(args)
        section=native['section']
        require(native['snapshot']['content_hash']==built['outputs'][output]['content_hash'],'section source content binding')
        (directory/f'section-{i}.json').write_text(json.dumps(native,indent=2,allow_nan=False)+'\n')
        imported=imported_section(generation/f'{output}.step',plane)
        if isinstance(z,float):
            rings=expected_rings(z);area=math.pi*(3600-sum(r*r for _,_,r in rings[1:]));centroid=[-3.,4.]
            match_rings(section,rings)
            require(len(imported['loops'])==len(rings),'OCCT/native ring count')
            sweeps=[0.]*len(rings)
            for curve in imported['curves']:
                require(curve['kind']=='arc','OCCT circular intersection type')
                found=next((j for j,(x,y,r) in enumerate(rings) if abs(x-curve['center'][0])<1e-7 and abs(y-curve['center'][1])<1e-7 and abs(r-curve['radius'])<1e-7),None)
                require(found is not None,'OCCT actual circular support mismatch');sweeps[found]+=abs(curve['sweep'])
            for sweep in sweeps:close(sweep,2*math.pi,'OCCT full ring intersection length')
            actual=sorted((loop['area'],*loop['centroid']) for loop in imported['loops'])
            expected=sorted((math.pi*r*r,x,y) for x,y,r in rings)
            # Equal-size circles need position-based matching, independent of OCCT edge ordering.
            for a,x,y in actual:
                found=next((j for j,(b,u,v) in enumerate(expected) if abs(a-b)<1e-6 and abs(x-u)<1e-6 and abs(y-v)<1e-6),None)
                require(found is not None,'OCCT circle area/center mismatch');expected.pop(found)
        elif z=='empty':
            area=0.;centroid=None
            require(not section['loops'] and not imported['loops'],'empty plane returned geometry')
        else:
            area=18*math.sqrt(3);centroid=[-3*math.sqrt(2),-math.sqrt(6)]
            require(len(section['loops'])==len(imported['loops'])==1,'triangle loop count')
            curves=[s['curve'] for s in section['loops'][0]['segments']]
            require(len(curves)==3 and all(c['kind']=='line' for c in curves),'oblique planar section must be a triangle')
            expected=[project(p,plane) for p in [[6.,0.,0.],[0.,6.,0.],[0.,0.,6.]]]
            require(len(imported['curves'])==3,'OCCT triangle edge count')
            for curve in curves+imported['curves']:
                require(curve['kind']=='line','triangle curve type')
                for key in ['start','end']:
                    require(any(all(abs(a-b)<1e-7 for a,b in zip(curve[key],point)) for point in expected),'triangle endpoint')
        close(section['area'],area,'native area');close(imported['area'],area,'OCCT area')
        if centroid is None:require(section['centroid'] is None,'empty centroid must be unknown')
        else:
            for a,b in zip(section['centroid'],centroid):close(a,b,'native centroid')
            for a,b in zip(imported['centroid'],centroid):close(a,b,'OCCT centroid')
        svg=subprocess.check_output(section_argv(cad,recipe,output,plane,'svg'))
        root=ET.fromstring(svg)
        require(json.loads(root.find('{http://www.w3.org/2000/svg}metadata').text)==native,'SVG query metadata differs from native report')
        (directory/f'section-{i}.svg').write_bytes(svg)
        require(command(args)==native,'section query is not deterministic')
        cases.append(dict(output=output,plane=plane,z=z,native=native,imported=imported,expected=dict(area=area,centroid=centroid)))
    small_recipe=directory/'source_small.star'
    small_recipe.write_bytes((ROOT/'examples/countersink.star').read_bytes())
    small_generation=directory/'generation_small'
    small_build=command([cad,'build',small_recipe,'--out-dir',small_generation])
    for z,radius in [(4.,3.),(10.,5.)]:
        plane=dict(origin=[0.,0.,z],normal=[0.,0.,1.],x_direction=[1.,0.,0.])
        native=command(section_argv(cad,small_recipe,'countersunk',plane))
        section=native['section'];imported=imported_section(small_generation/'countersunk.step',plane)
        area=2400-math.pi*radius**2;centroid=[30.,20.]
        require(native['snapshot']['content_hash']==small_build['outputs']['countersunk']['content_hash'],'small section source binding')
        require(len(section['loops'])==len(imported['loops'])==2,'small contours count')
        lines=[s['curve'] for loop in section['loops'] for s in loop['segments'] if s['curve']['kind']=='line']
        arcs=[s['curve'] for loop in section['loops'] for s in loop['segments'] if s['curve']['kind']=='arc']
        require(len(lines)==4 and len(arcs)==1,'small exact line/arc types')
        for line in lines:
            require(line['start'] in [[0.,0.],[60.,0.],[60.,40.],[0.,40.]],'small rectangular endpoint')
        close(arcs[0]['radius'],radius,'small circular radius');close(arcs[0]['sweep_angle'],-2*math.pi,'small hole sweep')
        for a,b in zip(arcs[0]['center'],centroid):close(a,b,'small bore center')
        for actual in [section,imported]:
            close(actual['area'],area,'small area')
            for a,b in zip(actual['centroid'],centroid):close(a,b,'small centroid')
        close(imported['loops'][0]['area'],2400.,'small OCCT rectangle area')
        close(imported['loops'][1]['area'],math.pi*radius**2,'small OCCT circular area')
        stem=f'small-section-{int(z)}'
        (directory/(stem+'.json')).write_text(json.dumps(native,indent=2,allow_nan=False)+'\n')
        svg=subprocess.check_output(section_argv(cad,small_recipe,'countersunk',plane,'svg'))
        root=ET.fromstring(svg)
        require(json.loads(root.find('{http://www.w3.org/2000/svg}metadata').text)==native,'small SVG query metadata differs from native report')
        (directory/(stem+'.svg')).write_bytes(svg)
        cases.append(dict(output='countersunk',plane=plane,z=z,native=native,imported=imported,expected=dict(area=area,centroid=centroid)))
    rejected=[]
    for output,plane in [('flange',dict(origin=[0.,0.,z],normal=[0.,0.,1.],x_direction=[1.,0.,0.])) for z in [0.,16.,18.,21.]] + [('section_box',dict(origin=[0.,0.,0.],normal=[0.,0.,1.],x_direction=[1.,0.,0.])),('flange',dict(origin=[60.,0.,5.],normal=[1.,0.,0.],x_direction=[0.,1.,0.]))]:
        result=subprocess.run(section_argv(cad,recipe,output,plane),capture_output=True)
        require(result.returncode!=0 and not result.stdout,'event/coplanar section must fail without output')
        rejected.append(dict(output=output,plane=plane,error=json.loads(result.stderr)))
    require(command([cad,'verify',generation])['valid'],'section queries modified published generation')
    require(command([cad,'verify',small_generation])['valid'],'small section query modified generation')
    result=dict(valid=True,cases=cases,rejected=rejected,build=built,small_build=small_build)
    (directory/'sections.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);parser.add_argument('--out-dir','--retain-dir',dest='retain_dir',type=pathlib.Path)
    args=parser.parse_args()
    if args.retain_dir:
        args.retain_dir.mkdir(parents=True,exist_ok=False);result=check(args.cad.resolve(),args.retain_dir)
    else:
        with tempfile.TemporaryDirectory(prefix='cad-sections-ocp-') as temporary:result=check(args.cad.resolve(),pathlib.Path(temporary))
    result['retained_generations']=['generation','generation_small'] if args.retain_dir else []
    result['retained_reports']=['sections.json'] if args.retain_dir else []
    result['oracle']=dict(python_version=platform.python_version(),ocp_version=str(OCP.__version__),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'))
    print(json.dumps(result,indent=2,allow_nan=False))

if __name__=='__main__':main()
