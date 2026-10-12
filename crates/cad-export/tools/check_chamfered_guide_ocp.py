"""Independent plane, moment, section and assembly checks for native chamfer chains."""
import argparse
import importlib.metadata
import json
import math
import pathlib
import platform
import sys
import tempfile
import OCP
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Plane
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from check_constrained_fixture_ocp import body_properties, require, json_output
from check_sections_ocp import imported_section, close
from check_step_metadata_ocp import command
from check_assembly_ocp import placement_matrix
from check_arc_prisms_ocp import mesh

ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/chamfered_guide_block.star'


def gauss(low,high):
    return [((low+high)/2+n*(high-low)/2,w*(high-low)/2) for n,w in [(-math.sqrt(3/5),5/9),(0,8/9),(math.sqrt(3/5),5/9)]]


def expected(d,e,h):
    moments=[0.]*5
    for low,high in [(0,h-e),(h-e,h)]:
        for z,wz in gauss(low,high):
            s=max(0,z-h+e)
            for a,b in [(s,d),(d,40-d),(40-d,40-s)]:
                for y,wy in gauss(a,b):
                    width=60-2*max(d-y,0,y-40+d)
                    for i,v in enumerate([width,width*z,width*z*z,width**3/12,width*(y-20)**2]):moments[i]+=wz*wy*v
    v,z1,z2,x2,y2=moments;cz=z1/v;zz=z2-v*cz*cz
    return v,[30,20,cz],[[y2+zz,0,0],[0,x2+zz,0],[0,0,x2+y2]]


def compare(actual,wanted,label):
    close(actual[0],wanted[0],label+' volume')
    for a,b in zip(actual[1],wanted[1]):close(a,b,label+' centroid')
    for ar,br in zip(actual[2],wanted[2]):
        for a,b in zip(ar,br):close(a,b,label+' central inertia')


def plane_checks(path,d,e,h):
    q=math.sqrt(.5)
    wanted=[([-1,0,0],0),([1,0,0],60),([0,-1,0],0),([0,1,0],40),([0,0,-1],0),([0,0,1],h),([-q,-q,0],-d*q),([q,-q,0],(60-d)*q),([q,q,0],(100-d)*q),([-q,q,0],(40-d)*q),([0,-q,q],(h-e)*q),([0,q,q],(h+40-e)*q)]
    reader=STEPControl_Reader();require(reader.ReadFile(str(path))==IFSelect_RetDone and reader.TransferRoots()==1,'STEP import')
    explorer=TopExp_Explorer(reader.OneShape(),TopAbs_FACE);actual=[]
    while explorer.More():
        face=TopoDS.Face(explorer.Current());surface=BRepAdaptor_Surface(face)
        require(surface.GetType()==GeomAbs_Plane,'chamfer exported nonplanar surface')
        plane=surface.Plane();n=plane.Axis().Direction();p=plane.Location()
        sign=-1 if face.Orientation()==TopAbs_REVERSED else 1
        normal=[sign*n.X(),sign*n.Y(),sign*n.Z()];offset=sum(a*b for a,b in zip(normal,[p.X(),p.Y(),p.Z()]))
        match=next((i for i,(other,value) in enumerate(wanted) if max(abs(a-b) for a,b in zip(normal,other))<1e-8 and abs(offset-value)<1e-7),None)
        require(match is not None,'unexpected support plane or orientation');wanted.pop(match)
        actual.append(dict(normal=normal,offset=offset));explorer.Next()
    require(not wanted and len(actual)==12,'missing original/chamfer planes')
    return actual


def case(cad,directory,d,e,h):
    params=dict(corner=d,bevel=e,height=h);flags=[item for k,v in params.items() for item in ['--param',f'{k}={v}']]
    generation=directory/'generation';build=command([cad,'build',RECIPE,'--out-dir',generation,*flags]);manifest=json.loads((generation/'manifest.json').read_text())
    require((generation/'recipe.star').read_bytes()==RECIPE.read_bytes(),'captured source')
    require(manifest['recipe_source']['parameter_overrides']==params,'captured parameters')
    wanted=expected(d,e,h);close(wanted[0],(2400-2*d*d)*h-(60-2*d)*e*e-2*e**3/3,'closed-form volume')
    native=command([cad,'properties',RECIPE,'--output','block',*flags]);compare((native['volume'],native['volume_centroid'],native['central_volume_inertia']),wanted,'native')
    actual=body_properties(generation/'block.step');compare(actual,wanted,'STEP');planes=plane_checks(generation/'block.step',d,e,h)
    mesh(generation/'block.stl',wanted[0])
    section_results={}
    for view,area in [('mid',2400-2*d*d),('bevel',2400-2*d*d-(60-2*d)*e-e*e/2)]:
        report=json.loads((generation/f'section-{view}.json').read_text());section=imported_section(generation/'block.step',report['section']['plane']);close(section['area'],area,'STEP section area')
        for a,b in zip(section['centroid'],[30,20]):close(a,b,'STEP section centroid')
        require(report['snapshot']==manifest['section_views'][view]['snapshot'],'section publication identity');section_results[view]=dict(native=report,imported=section)
    assembly=json.loads((generation/'chamfered_guide_block.assembly.json').read_text())
    a=60-2*d;cy=e*(a/2+2*e/3)/(a+e);normal=[0,-math.sqrt(.5),math.sqrt(.5)]
    # Native placement is checked against an independently integrated trapezoid centroid.
    for occurrence in assembly['instances']:
        if occurrence['part']=='pad':
            matrix=placement_matrix(occurrence['transform']);shift=90 if assembly['instance_paths'][occurrence['id']][0]=='right' else 0
            center=[sum(matrix[i][j]*[20,.25,0][j] for j in range(3))+matrix[i][3] for i in range(3)]
            for v,w in zip(center,[30+shift,cy,h-e+cy]):close(v,w,'wear-pad seat')
            for i in range(3):close(matrix[i][2],normal[i],'wear-pad normal')
    imported=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(generation/'chamfered_guide_block.step'),'--instances','4','--definitions','2','--solids','4','--assembly-json',str(generation/'chamfered_guide_block.assembly.json')])
    close(imported['material_volume_mm3'],2*(wanted[0]+40), 'assembly material volume')
    require(command([cad,'verify',generation])['valid'],'generation integrity')
    report=dict(parameters=params,expected=wanted,native=native,imported=actual,planes=planes,sections=section_results,assembly_import=imported)
    (directory/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');return report


def wedge_case(cad,directory,obtuse,rotated):
    q=math.sqrt(.5);d=2;h=8
    def rotate(p):
        return [(p[0]+p[1])/2+q*p[2],(p[0]+p[1])/2-q*p[2],q*(p[1]-p[0])] if rotated else list(p)
    source='profile=polygon(id="profile",points=[[0,0],[20,0],[30,10],[10,10]])\nstock=extrude(id="stock",profile=profile,height=8)\n'
    if rotated:source+='placed=transform(id="placed",body=stock,offset=[7,-4,2],rotation=[0.5,0.5,0,0.7071067811865476])\n'
    body='placed' if rotated else 'stock'
    wall='wall_1' if obtuse else 'wall_3'
    source+=f'edge=chamfer_faces(id="edge",body={body},faces=[{{"from_feature":stock,"role":"wall_0"}},{{"from_feature":stock,"role":"{wall}"}}],distance=2)\noutput(name="body",feature=edge)\nassert_faces(id="new_face",from_feature=edge,role="chamfer",in_feature=edge,count=1)\n'
    recipe=directory/'source.star';recipe.write_text(source);generation=directory/'generation'
    command([cad,'build',recipe,'--out-dir',generation])
    removed=d*d*q*h/2;volume=1600-removed
    removed_center=[20+(q-1)*d/3 if obtuse else (q+1)*d/3,q*d/3,h/2]
    expected_center=[(1600*a-removed*b)/volume for a,b in zip([15,5,h/2],removed_center)]
    expected_center=rotate(expected_center)
    if rotated:expected_center=[a+b for a,b in zip(expected_center,[7,-4,2])]
    actual=body_properties(generation/'body.step');close(actual[0],volume,'nonright STEP chamfer volume')
    for a,b in zip(actual[1],expected_center):close(a,b,'nonright STEP chamfer centroid')
    mesh(generation/'body.stl',volume)
    require(command([cad,'verify',generation])['valid'],'nonright generation integrity')
    report=dict(obtuse=obtuse,rotated=rotated,expected_volume=volume,expected_centroid=expected_center,imported=actual)
    (directory/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');return report


def check(cad,directory):
    result=[];labels=['low','default','high']
    for label,values in zip(labels,[(3,.75,16),(4,1.5,20),(5,2.25,24)]):
        target=directory/label;target.mkdir();result.append(case(cad,target,*values))
    wedges=[]
    for obtuse in [False,True]:
        for rotated in [False,True]:
            label=('obtuse' if obtuse else 'acute')+('-rotated' if rotated else '-identity');labels.append(label)
            target=directory/label;target.mkdir();wedges.append(wedge_case(cad,target,obtuse,rotated))
    return dict(valid=True,cases=result,wedge_cases=wedges,retained_generations=[f'{label}/generation' for label in labels],retained_reports=[f'{label}/report.json' for label in labels],oracle=dict(python_version=platform.python_version(),ocp_version=str(OCP.__version__),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp')))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=pathlib.Path,required=True);parser.add_argument('--out-dir',type=pathlib.Path);args=parser.parse_args()
    if args.out_dir:args.out_dir.mkdir(parents=True,exist_ok=False);result=check(args.cad.resolve(),args.out_dir)
    else:
        with tempfile.TemporaryDirectory(prefix='cad-chamfer-guide-') as temporary:result=check(args.cad.resolve(),pathlib.Path(temporary))
    print(json.dumps(result,indent=2,allow_nan=False))


if __name__=='__main__':main()
