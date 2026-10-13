#!/usr/bin/env python3
"""Independent OCCT acceptance for native finite-depth obround pockets.

OCCT constructs three separate cutters per pocket for an independent solid.
The production kernel and exporters are not linked into this oracle.
"""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import shutil
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('through_slot_oracle', ROOT/'tools/check-obround-slots.py')
o = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(o)
k = o.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane
from OCP.gp import gp_Ax2, gp_Dir, gp_Pnt
from OCP.TopAbs import TopAbs_FACE, TopAbs_SOLID
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS


def require(condition, reason):
    if not condition:
        raise AssertionError(reason)


def close(actual, expected, reason, absolute=2e-6):
    require(math.isfinite(actual) and abs(actual-expected)<=max(absolute,abs(expected)*2e-11),
            f'{reason}: {actual} != {expected}')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [ROOT/'examples/blind_slot_fixture.star']
    return {str(path.relative_to(ROOT)):sha(path) for path in sorted(paths)}


def config(**changes):
    result=dict(cap='end_cap', depth=3., slots=4, pose=0, posed_input=False,
                length=6., radius=2., scale=1.)
    result.update(changes)
    return result


def geometry(p):
    basis=dict(scale=p['scale'], pose=p['pose'], slots=p['slots'],
               length=p['length'], radius=p['radius'],variant='standard',posed_input=p['posed_input'])
    width, height, thickness, corner, slots=o.geometry(basis)
    return width,height,thickness,corner,slots


def source(p, invalid=None):
    width,height,thickness,corner,slots=geometry(p)
    q,offset,transform=k.pose_values(p['pose'])
    depth=p['depth']*p['scale']
    cap=p['cap']; anchor='stock'
    if invalid=='depth_beyond_web':depth=thickness
    if invalid=='stale_cap':cap='missing_cap'
    if invalid=='slot_contact':
        a,b=slots[:2];b['center']=[a['center'][0]+a['centerline_length']+2*a['radius'],a['center'][1]]
        b['direction']=a['direction'][:]
    if invalid=='slot_overlap':
        a,b=slots[:2];b['center']=[a['center'][0]+a['centerline_length']+2*a['radius']-.05*p['scale'],a['center'][1]]
        b['direction']=a['direction'][:]
    local_origin=[0,12*p['scale'],thickness if p['cap']=='end_cap' else 0]
    frame_x=[1,0,0];frame_y=[0,1,0]
    if p['posed_input']:
        frame_x=[transform.Value(i,1) for i in range(1,4)]
        frame_y=[transform.Value(i,2) for i in range(1,4)]
        local_origin=[sum(transform.Value(i+1,j+1)*local_origin[j] for j in range(3))+
                      transform.Value(i+1,4) for i in range(3)]
    if invalid=='wrong_frame':local_origin[2]+=0.125*p['scale']
    authored=[dict(slot,center=[slot['center'][0],slot['center'][1]-12*p['scale']]) for slot in slots]
    lines=[
        f'stock = extrude(id="stock", profile=polygon(id="outline", points={json.dumps([[-width/2,-height/2],[width/2,-height/2],[width/2,height/2],[-width/2,height/2]])}, circular_holes={[{"center":[0,0],"radius":3*p["scale"]}]}), height={thickness})',
        'body = stock',
    ]
    for i in range(4):
        faces=[{'from_feature':'stock','role':f'wall_{(i-1)%4}'},
               {'from_feature':'stock','role':f'wall_{i}'}]
        lines.append(f'body = fillet(id="round_{i}", body=body, faces={json.dumps(faces)}, radius={corner})')
    if p['posed_input']:
        lines.append(f'body = transform(id="posed_stock", body=body, rotation={q}, offset={offset})')
    lines.append(f'body = blind_slot_pattern(id="pockets", body=body, from_feature={anchor}, cap="{cap}", depth={depth}, slots={json.dumps(authored)}, frame_origin={local_origin}, frame_x_direction={frame_x}, frame_y_direction={frame_y})')
    if not p['posed_input']:
        lines.append(f'body = transform(id="finished", body=body, rotation={q}, offset={offset})')
    lines.append('output(name="finished", feature=body)')
    return '\n'.join(lines)+'\n'


def reference(p):
    width,height,thickness,corner,slots=geometry(p)
    solid=o.f.filleted([(-width/2,-height/2),(width/2,-height/2),
                        (width/2,height/2),(-width/2,height/2)],thickness,corner,[0,1,2,3])
    bore=3*p['scale']
    solid=k.boolean(BRepAlgoAPI_Cut,solid,BRepPrimAPI_MakeCylinder(
        gp_Ax2(gp_Pnt(0,0,-thickness/4),gp_Dir(0,0,1)),bore,1.5*thickness).Shape())
    bottom = p['cap']=='start_cap'
    z0 = -thickness/4 if bottom else thickness-p['depth']*p['scale']
    cut_height = p['depth']*p['scale']+thickness/4
    for slot in slots:
        x,y=slot['center'];ux,uy=slot['direction']
        half=slot['centerline_length']/2;r=slot['radius']
        if ux:
            box=BRepPrimAPI_MakeBox(gp_Pnt(x-half,y-r,z0),2*half,2*r,cut_height).Shape()
        else:
            box=BRepPrimAPI_MakeBox(gp_Pnt(x-r,y-half,z0),2*r,2*half,cut_height).Shape()
        solid=k.boolean(BRepAlgoAPI_Cut,solid,box)
        for sign in (-1,1):
            cylinder=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(
                x+sign*half*ux,y+sign*half*uy,z0),gp_Dir(0,0,1)),r,cut_height).Shape()
            solid=k.boolean(BRepAlgoAPI_Cut,solid,cylinder)
    return BRepBuilderAPI_Transform(solid,k.pose_values(p['pose'])[2],True).Shape()


def exact_moments(p):
    width,height,thickness,corner,slots=geometry(p)
    base=dict(scale=p['scale'],pose=0,slots=0,length=p['length'],radius=p['radius'],
              variant='standard',posed_input=False)
    volume0,_,inertia0=o.closed_moments(base)
    area0=volume0/thickness
    qxx=(inertia0[1][1]-volume0*thickness**2/12)/thickness
    qyy=(inertia0[0][0]-volume0*thickness**2/12)/thickness
    qxy=-inertia0[0][1]/thickness
    mass=volume0;first=[0.,0.,volume0*thickness/2]
    raw=[[thickness*qxx,thickness*qxy,0.],
         [thickness*qxy,thickness*qyy,0.],
         [0.,0.,volume0*thickness**2/3]]
    depth=p['depth']*p['scale']
    z0,z1=(0,depth) if p['cap']=='start_cap' else (thickness-depth,thickness)
    zfirst=(z1*z1-z0*z0)/2
    zsecond=(z1**3-z0**3)/3
    for slot in slots:
        x,y=slot['center'];ux,uy=slot['direction'];r=slot['radius']
        half=slot['centerline_length']/2
        area=4*half*r+math.pi*r*r
        along=4*r*half**3/3+8*half*r**3/3+math.pi*r*r*half*half+math.pi*r**4/4
        across=4*half*r**3/3+math.pi*r**4/4
        mass-=area*depth
        for i,v in enumerate((x,y)):first[i]-=area*depth*v
        first[2]-=area*zfirst
        raw[0][0]-=depth*(along*ux*ux+across*uy*uy+area*x*x)
        raw[1][1]-=depth*(along*uy*uy+across*ux*ux+area*y*y)
        raw[0][1]-=depth*((along-across)*ux*uy+area*x*y)
        raw[0][2]-=area*x*zfirst
        raw[1][2]-=area*y*zfirst
        raw[2][2]-=area*zsecond
    for i in range(3):
        for j in range(i):raw[i][j]=raw[j][i]
    center=[v/mass for v in first]
    covariance=[[raw[i][j]-first[i]*first[j]/mass for j in range(3)] for i in range(3)]
    trace=sum(covariance[i][i] for i in range(3))
    inertia=[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]
    return k.transformed((mass,center,inertia),k.pose_values(p['pose'])[2])


def compare_moments(actual,expected,label):
    close(actual[0],expected[0],label+' volume',2e-5)
    for i in range(3):
        close(actual[1][i],expected[1][i],label+' centroid',2e-6)
        for j in range(3):close(actual[2][i][j],expected[2][i][j],label+' inertia',2e-4)


def fixture_expected(width,depth,length,side):
    height=12.;width_y=80.
    mass=width*width_y*height
    first=[mass*width/2,mass*width_y/2,mass*height/2]
    raw=[[mass*width*width/3,mass*width*width_y/4,mass*width*height/4],
         [mass*width*width_y/4,mass*width_y*width_y/3,mass*width_y*height/4],
         [mass*width*height/4,mass*width_y*height/4,mass*height*height/3]]
    def remove(center,area,qxx,qyy,length_z,z0,z1):
        nonlocal mass
        x,y=center;volume=area*length_z
        mass-=volume;first[0]-=volume*x;first[1]-=volume*y
        zfirst=area*(z1*z1-z0*z0)/2
        first[2]-=zfirst
        raw[0][0]-=length_z*(qxx+area*x*x)
        raw[1][1]-=length_z*(qyy+area*y*y)
        raw[0][1]-=volume*x*y
        raw[0][2]-=zfirst*x;raw[1][2]-=zfirst*y
        raw[2][2]-=area*(z1**3-z0**3)/3
    for x in (10,width-10):
        for y in (10,70):
            radius=3.;area=math.pi*radius*radius;q=math.pi*radius**4/4
            remove((x,y),area,q,q,height,0,height)
    radius=6.;half=length/2
    area=4*half*radius+math.pi*radius*radius
    along=4*radius*half**3/3+8*half*radius**3/3+math.pi*radius*radius*half*half+math.pi*radius**4/4
    across=4*half*radius**3/3+math.pi*radius**4/4
    z0,z1=(0,depth) if side==0 else (height-depth,height)
    for x in (width/3,2*width/3):
        remove((x,40),area,across,along,depth,z0,z1)
    for i in range(3):
        for j in range(i):raw[i][j]=raw[j][i]
    center=[value/mass for value in first]
    covariance=[[raw[i][j]-first[i]*first[j]/mass for j in range(3)] for i in range(3)]
    trace=sum(covariance[i][i] for i in range(3))
    inertia=[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]
    return mass,center,inertia


def fixture_reference(width,depth,length,side):
    solid=BRepPrimAPI_MakeBox(gp_Pnt(0,0,0),width,80,12).Shape()
    for x in (10,width-10):
        for y in (10,70):
            solid=k.boolean(BRepAlgoAPI_Cut,solid,BRepPrimAPI_MakeCylinder(
                gp_Ax2(gp_Pnt(x,y,-2),gp_Dir(0,0,1)),3,16).Shape())
    z0=-2 if side==0 else 12-depth
    cutter_depth=depth+2
    for x in (width/3,2*width/3):
        cutter=BRepPrimAPI_MakeBox(gp_Pnt(x-6,40-length/2,z0),12,length,cutter_depth).Shape()
        solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
        for sign in (-1,1):
            cutter=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(
                x,40+sign*length/2,z0),gp_Dir(0,0,1)),6,cutter_depth).Shape()
            solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
    return solid


def check_fixture(cad,root,tag,parameters):
    source_path=ROOT/'examples/blind_slot_fixture.star'
    output=root/tag
    args=[word for key,value in parameters.items() for word in ('--param',f'{key}={value}')]
    manifest=o.command(cad,'build',source_path,'--out-dir',output,*args)
    o.command(cad,'verify',output)
    width=parameters.get('plate_width',120);depth=parameters.get('pocket_depth',4)
    length=parameters.get('pocket_length',24);side=parameters.get('pocket_side',1)
    expected=fixture_expected(width,depth,length,side)
    reference_shape=fixture_reference(width,depth,length,side)
    imported=k.load(str(output/'fixture.step'))
    native=o.command(cad,'properties',source_path,'--output','fixture',*args)
    for label,values in [('fixture OCCT',k.properties(reference_shape)),
                         ('fixture STEP',k.properties(imported)),
                         ('fixture native',(native['volume'],native['volume_centroid'],native['central_volume_inertia']))]:
        compare_moments(values,expected,label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
                 for a,b in ((reference_shape,imported),(imported,reference_shape))]
    for value in differences:close(value,0,'fixture directed STEP difference',2e-6)
    require(BRepCheck_Analyzer(imported).IsValid(),'fixture imported STEP is OCCT valid')
    floor_z=depth if side==0 else 12-depth
    floor_area=12*length+math.pi*36
    actual_floor=[]
    face_it=TopExp_Explorer(imported,TopAbs_FACE)
    while face_it.More():
        face=TopoDS.Face(face_it.Current());surface=BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Plane:
            props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
            centroid=props.CentreOfMass()
            if abs(centroid.Z()-floor_z)<2e-6 and abs(props.Mass()-floor_area)<2e-6:
                actual_floor.append([centroid.X(),centroid.Y(),centroid.Z()])
        face_it.Next()
    require(len(actual_floor)==2,'two actual planar STEP pocket floors')
    for x in (width/3,2*width/3):
        require(any(math.dist(point,[x,40,floor_z])<2e-6 for point in actual_floor),
                'STEP floor centroid at independent pocket axis')
    dimension_ids={'left_pocket_depth':depth,'right_pocket_depth':depth,
                   'remaining_web':12-depth}
    dimensions=manifest['datum_dimensions']
    for dimension_id,value in dimension_ids.items():
        d=dimensions[dimension_id]
        require(d['status']=='passed' and d['from']['instance_path']==['fixture_leaf']
                and d['to']['instance_path']==['fixture_leaf'],
                'source-bound single native fixture occurrence dimension')
        close(d['value_mm'],value,dimension_id+' independent STEP axial distance')
        from_z=d['from']['world_frame']['translation'][2]
        to_z=d['to']['world_frame']['translation'][2]
        close(abs(to_z-from_z),value,dimension_id+' witness axial separation')
        if dimension_id=='remaining_web':
            close(from_z,floor_z,'web begins at actual STEP floor')
            close(to_z,12 if side==0 else 0,'web ends at opposite STEP cap')
        else:
            close(from_z,0 if side==0 else 12,'depth begins at actual STEP opening')
            close(to_z,floor_z,'depth ends at actual STEP floor')
            role='slot_'+dimension_id.split('_')[0]+'_floor'
            witness=d['to']['semantic_datum']
            require(witness['definition']['role']==role
                    and len(witness['selection']['witnesses'])==1,
                    'native source-bound named floor face witness')
    drawing=ET.parse(output/'drawing-blind_fixture_machining.svg').getroot()
    rows={node.attrib['data-dimension']:node for node in drawing.iter()
          if node.attrib.get('data-dimension') in dimension_ids}
    require(set(rows)==set(dimension_ids),'three exact printed native floor/table crossrefs')
    for dimension_id,value in dimension_ids.items():
        texts=[node.text or '' for node in rows[dimension_id].iter()
               if node.tag.rsplit('}',1)[-1]=='text']
        require(any(f'{value:.3f} mm' in text for text in texts),
                'printed table value agrees with independent STEP plane spacing')
    for name,area,loops in (('inside_pockets',width*80-4*math.pi*9-
                           2*(12*length+math.pi*36),7),
                            ('intact_web',width*80-4*math.pi*9,5)):
        report=json.loads((output/f'section-{name}.json').read_text())
        close(report['section']['area'],area,'fixture '+name+' analytic section area',2e-5)
        section=report['section'] if 'loops' in report['section'] else report['occurrences'][0]['section']
        require(len(section['loops'])==loops,
                'fixture '+name+' exact pocket/bore loop inventory')
    k.mesh(output/'fixture.stl',expected[0])
    captured=root/(tag+'-captured')
    captured.mkdir()
    captured_source=captured/'blind_slot_fixture.star'
    shutil.copyfile(output/'recipe.star',captured_source)
    require(sha(captured_source)==sha(source_path),'authored source captured byte-identically')
    replay=root/(tag+'-replay')
    repeated=o.command(cad,'build',captured_source,'--out-dir',replay,*args)
    for artifact in ('fixture.step','fixture.stl','fixture.brep.json',
                     'section-inside_pockets.json','section-intact_web.json',
                     'drawing-blind_fixture_machining.json','drawing-blind_fixture_machining.svg'):
        require((output/artifact).read_bytes()==(replay/artifact).read_bytes(),
                f'{tag} captured replay {artifact}')
    require(manifest['feature_hashes']==repeated['feature_hashes'],'fixture feature hash replay')
    return {'case':tag,'parameters':parameters,'volume_mm3':expected[0],
            'centroid_mm':expected[1],'central_inertia_mm5':expected[2],
            'directed_difference_mm3':differences,'replayed_artifacts':7,
            'native_floor_dimensions_mm':dimension_ids}


def topology(shape,p):
    require(BRepCheck_Analyzer(shape).IsValid(),'imported STEP is valid OCCT BRep')
    solids=TopExp_Explorer(shape,TopAbs_SOLID);number=0
    while solids.More():number+=1;solids.Next()
    require(number==1,'one imported closed solid')
    _,_,thickness,_,slots=geometry(p)
    transform=k.pose_values(p['pose'])[2]
    posed=lambda point:[sum(transform.Value(i+1,j+1)*point[j] for j in range(3))+
                        transform.Value(i+1,4) for i in range(3)]
    extrusion_axis=[transform.Value(i,3) for i in range(1,4)]
    offset=[transform.Value(i,4) for i in range(1,4)]
    zfloor=p['depth']*p['scale'] if p['cap']=='start_cap' else thickness-p['depth']*p['scale']
    floor_points=[];cylinder_data=[];cap_areas=[];other=[]
    iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():
        face=TopoDS.Face(iterator.Current());support=BRepAdaptor_Surface(face)
        props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
        center=props.CentreOfMass()
        if support.GetType()==GeomAbs_Plane:
            plane=support.Plane();axis=plane.Axis().Direction()
            alignment=sum(a*b for a,b in zip((axis.X(),axis.Y(),axis.Z()),extrusion_axis))
            if abs(abs(alignment)-1)<1e-8:
                world=[center.X(),center.Y(),center.Z()]
                axial=sum((world[i]-offset[i])*extrusion_axis[i] for i in range(3))
                if abs(axial-zfloor)<2e-6:floor_points.append((props.Mass(),world))
                if abs(axial)<2e-6 or abs(axial-thickness)<2e-6:
                    cap_areas.append((axial,props.Mass()))
        elif support.GetType()==GeomAbs_Cylinder:
            cylinder=support.Cylinder();radius=cylinder.Radius()
            if abs(radius-p['radius']*p['scale'])<2e-6:
                cylinder_data.append((support.FirstUParameter(),support.LastUParameter(),
                                      support.FirstVParameter(),support.LastVParameter(),
                                      cylinder.Axis().Location(),cylinder.Axis().Direction()))
        else:other.append(str(support.GetType()))
        iterator.Next()
    require(not other,f'only native planar and cylindrical supports: {other}')
    require(len(cylinder_data)==2*len(slots),'two actual semicylindrical walls per pocket')
    for first,last,lo,hi,point,axis in cylinder_data:
        close(last-first,math.pi,'actual semicylindrical trim angle')
        close(hi-lo,p['depth']*p['scale'],'actual pocket wall depth')
    require(len(floor_points)==len(slots),'one actual planar floor face per pocket')
    used=[]
    for slot in slots:
        area=2*slot['radius']*slot['centerline_length']+math.pi*slot['radius']**2
        expected=posed([*slot['center'],zfloor])
        matches=[i for i,(surface_area,center) in enumerate(floor_points)
                 if abs(surface_area-area)<2e-6 and math.dist(center,expected)<2e-6]
        require(len(matches)==1 and matches[0] not in used,
                'independent STEP floor area and transformed centroid')
        used.extend(matches)
        half=slot['centerline_length']/2
        for sign in (-1,1):
            endpoint=posed([slot['center'][0]+sign*half*slot['direction'][0],
                            slot['center'][1]+sign*half*slot['direction'][1],0])
            matching=0
            for _,_,_,_,point,axis in cylinder_data:
                location=[point.X(),point.Y(),point.Z()]
                direction=[axis.X(),axis.Y(),axis.Z()]
                delta=[endpoint[i]-location[i] for i in range(3)]
                along=sum(delta[i]*direction[i] for i in range(3))
                transverse=math.sqrt(sum((delta[i]-along*direction[i])**2 for i in range(3)))
                matching+=transverse<2e-6
            require(matching==1,'one trimmed STEP half-cylinder on each transformed slot endpoint axis')
    require(len(cap_areas)==2,'both stock cap planes persist in STEP')
    return {'floor_faces':len(floor_points),
            'semicylinder_faces':len(cylinder_data),'cap_planes':len(cap_areas)}


def sections(cad,recipe,p):
    if p['pose']!=0:return None
    _,_,thickness,_,slots=geometry(p)
    base=dict(scale=p['scale'],pose=0,slots=0,length=p['length'],radius=p['radius'],
              variant='standard',posed_input=False)
    full=o.closed_moments(base)[0]/thickness
    pocket_area=sum(2*s['radius']*s['centerline_length']+math.pi*s['radius']**2 for s in slots)
    floor=p['depth']*p['scale'] if p['cap']=='start_cap' else thickness-p['depth']*p['scale']
    inside=floor/2 if p['cap']=='start_cap' else (floor+thickness)/2
    web=(floor+thickness)/2 if p['cap']=='start_cap' else floor/2
    evidence=[]
    for label,z,area,loop_count in (('inside',inside,full-pocket_area,len(slots)+2),
                                    ('web',web,full,2)):
        report=o.command(cad,'section',recipe,'--output','finished','--origin',f'0,0,{z}')
        section=report['section']
        close(section['area'],area,label+' native section area',2e-5)
        require(len(section['loops'])==loop_count,f'{label} native section loop count')
        evidence.append({'plane_z':z,'area_mm2':section['area'],'loops':len(section['loops'])})
    return evidence


def check_case(cad,root,tag,p):
    recipe=root/(tag+'.star');output=root/tag
    recipe.write_text(source(p))
    built=o.command(cad,'build',recipe,'--out-dir',output)
    o.command(cad,'verify',output)
    native=o.command(cad,'properties',recipe,'--output','finished')
    expected=exact_moments(p)
    oracle=reference(p)
    imported=k.load(str(output/'finished.step'))
    for label,properties in [('reference OCCT',k.properties(oracle)),('native STEP',k.properties(imported)),
                             ('native report',(native['volume'],native['volume_centroid'],native['central_volume_inertia']))]:
        compare_moments(properties,expected,label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
                 for a,b in ((oracle,imported),(imported,oracle))]
    for value in differences:close(value,0,'directed OCCT/STEP difference',2e-6)
    surfaces=topology(imported,p)
    section=sections(cad,recipe,p)
    k.mesh(output/'finished.stl',expected[0])
    replay=root/(tag+'-replay')
    repeated=o.command(cad,'build',output/'recipe.star','--out-dir',replay)
    for artifact in ('finished.step','finished.stl','finished.brep.json','recipe.star'):
        require((output/artifact).read_bytes()==(replay/artifact).read_bytes(),
                f'{tag} byte-identical captured replay {artifact}')
    require(built['feature_hashes']==repeated['feature_hashes'],'replay native feature hashes')
    return {'case':tag,'parameters':p,'volume_mm3':expected[0],'centroid_mm':expected[1],
            'central_inertia_mm5':expected[2],'directed_difference_mm3':differences,
            'surfaces':surfaces,'sections':section,'replayed_artifacts':4,
            'finished_content_hash':built['outputs']['finished']['content_hash']}


def refusals(cad,root):
    baseline=config(slots=2)
    recipe=root/'revisions.star';output=root/'preserved';cache=root/'cache'
    recipe.write_text(source(baseline));accepted=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    before=o.inventory(output)
    revised=config(slots=2,depth=4)
    recipe.write_text(source(revised));changed=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    require(changed['feature_hashes']['pockets']!=accepted['feature_hashes']['pockets'],
            'changed pocket depth invalidates native cache key')
    recipe.write_text(source(baseline));restored=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    require(restored['feature_hashes']==accepted['feature_hashes'],'restored native feature hashes')
    before=o.inventory(output)
    result=[]
    for invalid in ('depth_beyond_web','stale_cap','slot_contact','slot_overlap','wrong_frame'):
        recipe.write_text(source(baseline,invalid))
        failure=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache,success=False)
        require(o.inventory(output)==before,f'{invalid} preserves complete accepted publication')
        result.append({'case':invalid,'diagnostic':failure})
    o.command(cad,'verify',output)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary=sha(cad);script=sha(__file__);sources=inputs()
    cases=[check_case(cad,args.out_dir,tag,p) for tag,p in (
        ('four_top',config()),('four_bottom',config(cap='start_cap')),
        ('one_top',config(slots=1,depth=5)),
        ('four_oblique',config(pose=2)),
        ('four_oblique_posed_input',config(pose=2,posed_input=True)),
        ('scaled',config(scale=.5,depth=2.5,slots=2)),
    )]
    fixtures=[check_fixture(cad,args.out_dir,tag,parameters) for tag,parameters in (
        ('fixture_default',{}),
        ('fixture_bottom_wide',{'plate_width':144,'pocket_depth':6,
                                'pocket_length':30,'pocket_side':0}),
    )]
    rejected=refusals(cad,args.out_dir)
    require(binary==sha(cad) and script==sha(__file__) and sources==inputs(),
            'binary, oracle and source inputs remain byte-identical')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'input_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'authored_fixture_cases':fixtures,'refusals':rejected,
            'scope':'Native blind obround pockets in a convex rounded right prism with one circular throughbore; independent OCCT Boolean reference and exact mass moments.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'authored_fixture_cases':len(fixtures),
                      'refusals':len(rejected),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
