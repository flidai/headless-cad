#!/usr/bin/env python3
"""Independent OCCT acceptance for serial same-cap varied-depth capsule pockets."""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import shutil

ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location('blind_slot_oracle',ROOT/'tools/check-blind-slots.py')
b=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(b)
o=b.o;k=b.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox,BRepPrimAPI_MakeCylinder
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.GeomAbs import GeomAbs_Cylinder,GeomAbs_Plane
from OCP.gp import gp_Ax2,gp_Dir,gp_Pnt
from OCP.TopAbs import TopAbs_FACE,TopAbs_SOLID
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS


def require(condition,message):
    if not condition:raise AssertionError(message)


def close(actual,expected,message,tolerance=2e-6):
    b.close(actual,expected,message,tolerance)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [ROOT/'examples/serial_blind_fixture.star']
    return {str(path.relative_to(ROOT)):sha(path) for path in sorted(paths)}


def config(**changes):
    result=dict(cap='end_cap',depths=(3.,5.),pose=0,posed_input=False,
                reverse=False,scale=1.,length=6.,radius=2.)
    result.update(changes)
    return result


def ingredients(p):
    old=b.config(cap=p['cap'],depth=p['depths'][0],slots=2,
                 pose=p['pose'],posed_input=p['posed_input'],
                 scale=p['scale'],length=p['length'],radius=p['radius'])
    width,height,thickness,corner,slots=b.geometry(old)
    return old,width,height,thickness,corner,slots


def source(p,invalid=None):
    old,width,height,thickness,corner,slots=ingredients(p)
    raw=b.source(old).splitlines()
    slot_line=next(i for i,line in enumerate(raw) if line.startswith('body = blind_slot_pattern('))
    q,offset,transform=k.pose_values(p['pose'])
    frame_x=[1,0,0];frame_y=[0,1,0]
    origin=[0,12*p['scale'],thickness if p['cap']=='end_cap' else 0]
    if p['posed_input']:
        frame_x=[transform.Value(i,1) for i in range(1,4)]
        frame_y=[transform.Value(i,2) for i in range(1,4)]
        origin=[sum(transform.Value(i+1,j+1)*origin[j] for j in range(3))+
                transform.Value(i+1,4) for i in range(3)]
    by_id={slot['id']:dict(slot,center=[slot['center'][0],slot['center'][1]-12*p['scale']])
           for slot in slots}
    order=['slot_2','slot_1'] if p['reverse'] else ['slot_1','slot_2']
    depths={name:p['depths'][index]*p['scale'] for index,name in enumerate(('slot_1','slot_2'))}
    if invalid=='contact':
        first=by_id[order[0]];second=by_id[order[1]]
        second['center']=[first['center'][0]+first['centerline_length']+2*first['radius'],
                          first['center'][1]]
        second['direction']=first['direction'][:]
    if invalid=='overlap':
        first=by_id[order[0]];second=by_id[order[1]]
        second['center']=[first['center'][0]+first['centerline_length']+2*first['radius']-.05*p['scale'],
                          first['center'][1]]
        second['direction']=first['direction'][:]
    if invalid=='fifth':
        all_slots=b.geometry(b.config(slots=4,scale=p['scale'],length=p['length'],radius=p['radius']))[-1]
        first_ids=[slot['id'] for slot in all_slots]
        by_id={slot['id']:dict(slot,center=[slot['center'][0],slot['center'][1]-12*p['scale']])
               for slot in all_slots}
        by_id['slot_5']=dict(id='slot_5',center=[0,0],direction=[1,0],
                             centerline_length=2*p['scale'],radius=p['radius']*p['scale'])
        order=['slot_1','slot_5']
    def call(feature_id,body,slot_ids,cap,depth,frame):
        declared=[by_id[name] for name in slot_ids]
        return (f'{feature_id} = blind_slot_pattern(id="{feature_id}", body={body}, '
                f'from_feature=stock, cap="{cap}", depth={depth}, slots={json.dumps(declared)}, '
                f'frame_origin={frame}, frame_x_direction={frame_x}, frame_y_direction={frame_y})')
    first_ids=[order[0]] if invalid!='fifth' else ['slot_1','slot_2','slot_3','slot_4']
    first=call('first_cut','body',first_ids,p['cap'],depths[order[0]],origin)
    second_cap=('start_cap' if p['cap']=='end_cap' else 'end_cap') if invalid=='opposite_cap' else p['cap']
    second_depth=thickness if invalid=='full_web' else depths.get(order[1],p['depths'][1]*p['scale'])
    second_origin=origin[:]
    if invalid=='opposite_cap':
        opposite=[0,12*p['scale'],0 if p['cap']=='end_cap' else thickness]
        if p['posed_input']:
            second_origin=[sum(transform.Value(i+1,j+1)*opposite[j] for j in range(3))+
                           transform.Value(i+1,4) for i in range(3)]
        else:
            second_origin=opposite
    if invalid=='wrong_frame':second_origin[2]+=.125*p['scale']
    second=call('second_cut','first_cut',[order[1]],second_cap,second_depth,second_origin)
    assertion=(f'assert_faces(id="first_floor_survives", from_feature=first_cut, '
               f'role="slot_{order[0]}_floor", in_feature=second_cut, count=1)')
    raw[slot_line:slot_line+1]=[first,'output(name="first_stage", feature=first_cut)',
                               second,assertion,'body = second_cut']
    return '\n'.join(raw)+'\n'


def expected(p):
    old,width,height,thickness,corner,slots=ingredients(p)
    base=b.config(cap=p['cap'],depth=p['depths'][0],slots=0,pose=0,
                  scale=p['scale'],length=p['length'],radius=p['radius'])
    volume0,_,inertia0=b.exact_moments(base)
    qxx=(inertia0[1][1]-volume0*thickness**2/12)/thickness
    qyy=(inertia0[0][0]-volume0*thickness**2/12)/thickness
    qxy=-inertia0[0][1]/thickness
    mass=volume0;first=[0.,0.,volume0*thickness/2]
    raw=[[thickness*qxx,thickness*qxy,0.],
         [thickness*qxy,thickness*qyy,0.],
         [0.,0.,volume0*thickness**2/3]]
    for index,slot in enumerate(slots):
        depth=p['depths'][index]*p['scale']
        z0,z1=(0,depth) if p['cap']=='start_cap' else (thickness-depth,thickness)
        x,y=slot['center'];ux,uy=slot['direction'];r=slot['radius']
        half=slot['centerline_length']/2
        area=4*half*r+math.pi*r*r
        along=4*r*half**3/3+8*half*r**3/3+math.pi*r*r*half*half+math.pi*r**4/4
        across=4*half*r**3/3+math.pi*r**4/4
        zfirst=(z1*z1-z0*z0)/2
        mass-=area*depth
        first[0]-=area*depth*x;first[1]-=area*depth*y;first[2]-=area*zfirst
        raw[0][0]-=depth*(along*ux*ux+across*uy*uy+area*x*x)
        raw[1][1]-=depth*(along*uy*uy+across*ux*ux+area*y*y)
        raw[0][1]-=depth*((along-across)*ux*uy+area*x*y)
        raw[0][2]-=area*x*zfirst;raw[1][2]-=area*y*zfirst
        raw[2][2]-=area*(z1**3-z0**3)/3
    for i in range(3):
        for j in range(i):raw[i][j]=raw[j][i]
    center=[value/mass for value in first]
    covariance=[[raw[i][j]-first[i]*first[j]/mass for j in range(3)] for i in range(3)]
    trace=sum(covariance[i][i] for i in range(3))
    inertia=[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]
    return k.transformed((mass,center,inertia),k.pose_values(p['pose'])[2])


def reference(p):
    old,width,height,thickness,corner,slots=ingredients(p)
    solid=o.f.filleted([(-width/2,-height/2),(width/2,-height/2),
                        (width/2,height/2),(-width/2,height/2)],thickness,corner,[0,1,2,3])
    bore=3*p['scale']
    solid=k.boolean(BRepAlgoAPI_Cut,solid,BRepPrimAPI_MakeCylinder(
        gp_Ax2(gp_Pnt(0,0,-thickness/4),gp_Dir(0,0,1)),bore,1.5*thickness).Shape())
    for index,slot in enumerate(slots):
        depth=p['depths'][index]*p['scale']
        z0=-thickness/4 if p['cap']=='start_cap' else thickness-depth
        cutter_height=depth+thickness/4
        x,y=slot['center'];ux,uy=slot['direction'];half=slot['centerline_length']/2;r=slot['radius']
        if ux:
            box=BRepPrimAPI_MakeBox(gp_Pnt(x-half,y-r,z0),2*half,2*r,cutter_height).Shape()
        else:
            box=BRepPrimAPI_MakeBox(gp_Pnt(x-r,y-half,z0),2*r,2*half,cutter_height).Shape()
        solid=k.boolean(BRepAlgoAPI_Cut,solid,box)
        for sign in (-1,1):
            cylinder=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(
                x+sign*half*ux,y+sign*half*uy,z0),gp_Dir(0,0,1)),r,cutter_height).Shape()
            solid=k.boolean(BRepAlgoAPI_Cut,solid,cylinder)
    return BRepBuilderAPI_Transform(solid,k.pose_values(p['pose'])[2],True).Shape()


def fixture_slots(width,first_depth,second_depth):
    return [dict(id='locating',center=[width/3,40],direction=[0,1],
                 centerline_length=20,radius=5,depth=first_depth),
            dict(id='relief',center=[2*width/3,40],direction=[0,1],
                 centerline_length=22,radius=6,depth=second_depth)]


def fixture_expected(width,first_depth,second_depth,side):
    height=12.;material_width=80.
    mass=width*material_width*height
    first=[mass*width/2,mass*material_width/2,mass*height/2]
    raw=[[mass*width**2/3,mass*width*material_width/4,mass*width*height/4],
         [mass*width*material_width/4,mass*material_width**2/3,mass*material_width*height/4],
         [mass*width*height/4,mass*material_width*height/4,mass*height**2/3]]
    def remove(x,y,area,qxx,qyy,z0,z1):
        nonlocal mass
        depth=z1-z0;volume=area*depth;zfirst=area*(z1*z1-z0*z0)/2
        mass-=volume
        first[0]-=volume*x;first[1]-=volume*y;first[2]-=zfirst
        raw[0][0]-=depth*(qxx+area*x*x)
        raw[1][1]-=depth*(qyy+area*y*y)
        raw[0][1]-=volume*x*y
        raw[0][2]-=zfirst*x;raw[1][2]-=zfirst*y
        raw[2][2]-=area*(z1**3-z0**3)/3
    for x in (10,width-10):
        for y in (10,70):
            area=math.pi*9;q=math.pi*3**4/4
            remove(x,y,area,q,q,0,height)
    for slot in fixture_slots(width,first_depth,second_depth):
        x,y=slot['center'];radius=slot['radius'];half=slot['centerline_length']/2
        area=4*half*radius+math.pi*radius**2
        along=(4*radius*half**3/3+8*half*radius**3/3+
               math.pi*radius**2*half**2+math.pi*radius**4/4)
        across=4*half*radius**3/3+math.pi*radius**4/4
        z0,z1=(0,slot['depth']) if side==0 else (height-slot['depth'],height)
        remove(x,y,area,across,along,z0,z1)
    for i in range(3):
        for j in range(i):raw[i][j]=raw[j][i]
    center=[value/mass for value in first]
    covariance=[[raw[i][j]-first[i]*first[j]/mass for j in range(3)] for i in range(3)]
    trace=sum(covariance[i][i] for i in range(3))
    inertia=[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]
    return mass,center,inertia


def fixture_reference(width,first_depth,second_depth,side):
    solid=BRepPrimAPI_MakeBox(gp_Pnt(0,0,0),width,80,12).Shape()
    for x in (10,width-10):
        for y in (10,70):
            cutter=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x,y,-2),gp_Dir(0,0,1)),3,16).Shape()
            solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
    for slot in fixture_slots(width,first_depth,second_depth):
        x,y=slot['center'];radius=slot['radius'];half=slot['centerline_length']/2
        depth=slot['depth'];z0=-2 if side==0 else 12-depth
        box=BRepPrimAPI_MakeBox(gp_Pnt(x-radius,y-half,z0),2*radius,2*half,depth+2).Shape()
        solid=k.boolean(BRepAlgoAPI_Cut,solid,box)
        for sign in (-1,1):
            cutter=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x,y+sign*half,z0),
                                                gp_Dir(0,0,1)),radius,depth+2).Shape()
            solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
    return solid


def check_fixture(cad,root,tag,parameters):
    path=ROOT/'examples/serial_blind_fixture.star';output=root/tag
    args=[word for key,value in parameters.items() for word in ('--param',f'{key}={value}')]
    manifest=o.command(cad,'build',path,'--out-dir',output,*args)
    o.command(cad,'verify',output)
    width=parameters.get('plate_width',120)
    first_depth=parameters.get('first_depth',3)
    second_depth=parameters.get('second_depth',7)
    side=parameters.get('pocket_side',1)
    expected_values=fixture_expected(width,first_depth,second_depth,side)
    oracle=fixture_reference(width,first_depth,second_depth,side)
    imported=k.load(str(output/'fixture.step'))
    native=o.command(cad,'properties',path,'--output','fixture',*args)
    for label,values in [('authored OCCT',k.properties(oracle)),
                         ('authored STEP',k.properties(imported)),
                         ('authored native',(native['volume'],native['volume_centroid'],
                                             native['central_volume_inertia']))]:
        b.compare_moments(values,expected_values,label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,z))[0]
                 for a,z in ((oracle,imported),(imported,oracle))]
    for value in differences:close(value,0,'authored STEP directed difference')
    require(BRepCheck_Analyzer(imported).IsValid(),'authored serial STEP OCCT-valid')
    assertion=manifest['face_assertions']['first_floor_survives']
    require(assertion['status']=='resolved' and len(assertion['witnesses'])==1,
            'authored first-floor source lineage survives second operation')
    witness=assertion['witnesses'][0]
    require(witness['anchor_faces'][0]!=witness['target_face'],
            'authored first floor native face ID remaps in second operation')
    final_model=json.loads((output/'fixture.brep.json').read_text())['model']
    floor=final_model['faces'][witness['target_face']]['surface']
    floor_z=first_depth if side==0 else 12-first_depth
    require(floor['kind']=='plane','authored first floor remains analytic plane')
    close(floor['geometry']['origin'][2],floor_z,'authored first floor retained its exact depth')
    floor_points=[];iterator=TopExp_Explorer(imported,TopAbs_FACE)
    while iterator.More():
        face=TopoDS.Face(iterator.Current());support=BRepAdaptor_Surface(face)
        if support.GetType()==GeomAbs_Plane:
            props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
            center=props.CentreOfMass()
            floor_points.append((props.Mass(),[center.X(),center.Y(),center.Z()]))
        iterator.Next()
    for slot in fixture_slots(width,first_depth,second_depth):
        radius=slot['radius'];area=2*radius*slot['centerline_length']+math.pi*radius**2
        z=slot['depth'] if side==0 else 12-slot['depth']
        target=[*slot['center'],z]
        require(sum(abs(a-area)<2e-6 and math.dist(point,target)<2e-6
                    for a,point in floor_points)==1,
                'authored STEP retains each floor at its independently cut depth')
    area0=width*80-4*math.pi*9
    areas=[2*s['radius']*s['centerline_length']+math.pi*s['radius']**2
           for s in fixture_slots(width,first_depth,second_depth)]
    for name,area,loops in (('both_pockets',area0-sum(areas),7),
                            ('deep_pocket_only',area0-areas[1],6),
                            ('intact_web',area0,5)):
        report=json.loads((output/f'section-{name}.json').read_text())
        close(report['section']['area'],area,name+' independently integrated section')
        section=report['section'] if 'loops' in report['section'] else report['occurrences'][0]['section']
        require(len(section['loops'])==loops,name+' exact native section loop count')
    dimensions=manifest['datum_dimensions']
    expected_dimensions={'locating_depth':first_depth,'relief_depth':second_depth,
                         'remaining_web':12-second_depth}
    for name,value in expected_dimensions.items():
        d=dimensions[name]
        require(d['status']=='passed' and d['from']['instance_path']==['fixture_leaf']
                and d['to']['instance_path']==['fixture_leaf'],
                'authored depth dimension resolves exact native occurrence')
        close(d['value_mm'],value,name+' STEP floor/cap axial spacing')
        close(abs(d['to']['world_frame']['translation'][2]-
                  d['from']['world_frame']['translation'][2]),value,
              name+' native datum witness spacing')
        if name=='locating_depth':
            require(d['to']['semantic_datum']['definition']['anchor_feature']=='locating_pocket',
                    'first floor datum retains first operation as semantic source')
    drawing=(output/'drawing-serial_fixture_machining.svg').read_text()
    for name,value in expected_dimensions.items():
        require(f'data-dimension="{name}"' in drawing and f'{value:.3f} mm' in drawing,
                'printed native dimension table crossref and exact value')
    k.mesh(output/'fixture.stl',expected_values[0])
    captured=root/(tag+'-captured');captured.mkdir()
    captured_source=captured/'serial_blind_fixture.star'
    shutil.copyfile(output/'recipe.star',captured_source)
    require(sha(captured_source)==sha(path),'authored serial source captured exactly')
    replay=root/(tag+'-replay')
    repeated=o.command(cad,'build',captured_source,'--out-dir',replay,*args)
    for artifact in ('fixture.step','fixture.stl','fixture.brep.json',
                     'first_stage.step','drawing-serial_fixture_machining.svg'):
        require((output/artifact).read_bytes()==(replay/artifact).read_bytes(),
                tag+' captured replay '+artifact)
    require(manifest['feature_hashes']==repeated['feature_hashes'],
            'authored serial feature hashes replay exactly')
    return {'case':tag,'parameters':parameters,'volume_mm3':expected_values[0],
            'centroid_mm':expected_values[1],'central_inertia_mm5':expected_values[2],
            'directed_difference_mm3':differences,'first_floor_remap':witness,
            'native_floor_dimensions_mm':expected_dimensions}


def topology(shape,p):
    require(BRepCheck_Analyzer(shape).IsValid(),'serial STEP is valid OCCT BRep')
    solids=TopExp_Explorer(shape,TopAbs_SOLID);count=0
    while solids.More():count+=1;solids.Next()
    require(count==1,'one serial STEP solid')
    old,width,height,thickness,corner,slots=ingredients(p)
    trsf=k.pose_values(p['pose'])[2]
    posed=lambda xyz:[sum(trsf.Value(i+1,j+1)*xyz[j] for j in range(3))+
                      trsf.Value(i+1,4) for i in range(3)]
    floor_points=[];cylinders=[];caps=[]
    axis=[trsf.Value(i,3) for i in range(1,4)]
    origin=[trsf.Value(i,4) for i in range(1,4)]
    iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():
        face=TopoDS.Face(iterator.Current());support=BRepAdaptor_Surface(face)
        props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
        center=props.CentreOfMass();world=[center.X(),center.Y(),center.Z()]
        if support.GetType()==GeomAbs_Plane:
            normal=support.Plane().Axis().Direction()
            if abs(abs(sum(a*b for a,b in zip(axis,(normal.X(),normal.Y(),normal.Z()))))-1)<1e-8:
                z=sum((world[i]-origin[i])*axis[i] for i in range(3))
                if abs(z)<2e-6 or abs(z-thickness)<2e-6:caps.append(z)
                else:floor_points.append((props.Mass(),world))
        elif support.GetType()==GeomAbs_Cylinder:
            cylinder=support.Cylinder()
            if abs(cylinder.Radius()-p['radius']*p['scale'])<2e-6:
                cylinders.append((support.FirstUParameter(),support.LastUParameter(),
                                  support.FirstVParameter(),support.LastVParameter(),
                                  cylinder.Axis()))
        iterator.Next()
    require(len(caps)==2,'both opening and opposite native cap planes')
    require(len(floor_points)==2,'two distinct native planar pocket floors')
    require(len(cylinders)==4,'four trimmed half-cylinder walls')
    for index,slot in enumerate(slots):
        depth=p['depths'][index]*p['scale']
        zfloor=depth if p['cap']=='start_cap' else thickness-depth
        area=2*slot['radius']*slot['centerline_length']+math.pi*slot['radius']**2
        point=posed([*slot['center'],zfloor])
        require(sum(abs(a-area)<2e-6 and math.dist(c,point)<2e-6 for a,c in floor_points)==1,
                'one STEP floor at exact independent depth, centroid and capsule area')
        half=slot['centerline_length']/2
        for sign in (-1,1):
            point=posed([slot['center'][0]+sign*half*slot['direction'][0],
                         slot['center'][1]+sign*half*slot['direction'][1],0])
            matches=0
            for u0,u1,v0,v1,axis in cylinders:
                location=axis.Location();direction=axis.Direction()
                start=[location.X(),location.Y(),location.Z()]
                along=[direction.X(),direction.Y(),direction.Z()]
                delta=[point[i]-start[i] for i in range(3)]
                axial=sum(delta[i]*along[i] for i in range(3))
                transverse=math.sqrt(sum((delta[i]-axial*along[i])**2 for i in range(3)))
                if transverse<2e-6:
                    close(u1-u0,math.pi,'half-cylinder STEP trim angle')
                    close(v1-v0,depth,'individual native pocket wall depth')
                    matches+=1
            require(matches==1,'one real half-cylinder at each capsule cap axis')
    return {'floor_faces':len(floor_points),'semicylinder_faces':len(cylinders),
            'cap_faces':len(caps)}


def sections(cad,recipe,p):
    if p['pose']!=0:return None
    old,width,height,thickness,corner,slots=ingredients(p)
    base=b.config(slots=0,pose=0,scale=p['scale'],length=p['length'],radius=p['radius'])
    area=b.exact_moments(base)[0]/thickness
    slot_areas=[2*s['radius']*s['centerline_length']+math.pi*s['radius']**2 for s in slots]
    short=min(p['depths'])*p['scale'];long=max(p['depths'])*p['scale']
    levels=[('both',short/2,area-sum(slot_areas),4),
            ('one',(short+long)/2,area-slot_areas[p['depths'].index(max(p['depths']))],3),
            ('web',(long+thickness)/2,area,2)]
    if p['cap']=='end_cap':levels=[(name,thickness-depth,expected,loops)
                                  for name,depth,expected,loops in levels]
    result=[]
    for name,z,expected_area,loops in levels:
        report=o.command(cad,'section',recipe,'--output','finished','--origin',f'0,0,{z}')
        section=report['section']
        close(section['area'],expected_area,name+' native material section area',2e-5)
        require(len(section['loops'])==loops,name+' exact native section loop count')
        result.append({'level':name,'z':z,'area_mm2':section['area'],'loops':loops})
    return result


def check_case(cad,root,tag,p):
    recipe=root/(tag+'.star');output=root/tag
    recipe.write_text(source(p))
    manifest=o.command(cad,'build',recipe,'--out-dir',output)
    o.command(cad,'verify',output)
    selected=manifest['face_assertions']['first_floor_survives']
    require(selected['status']=='resolved' and len(selected['witnesses'])==1,
            'first cut floor survives second cut through exact native lineage')
    witness=selected['witnesses'][0]
    require(witness['anchor_faces'][0]!=witness['target_face'],
            'old floor face ID remaps after second walls are inserted')
    model=json.loads((output/'finished.brep.json').read_text())['model']
    target=model['faces'][witness['target_face']]['surface']
    require(target['kind']=='plane','remapped first floor is an analytic plane')
    old,width,height,thickness,corner,slots=ingredients(p)
    first_name='slot_2' if p['reverse'] else 'slot_1'
    first_depth=p['depths'][int(first_name[-1])-1]*p['scale']
    local_floor=first_depth if p['cap']=='start_cap' else thickness-first_depth
    trsf=k.pose_values(p['pose'])[2]
    expected_world=[sum(trsf.Value(i+1,j+1)*[*slots[int(first_name[-1])-1]['center'],local_floor][j]
                        for j in range(3))+trsf.Value(i+1,4) for i in range(3)]
    for i in range(3):close(target['geometry']['origin'][i],expected_world[i],
                            'remapped first floor native plane origin')
    exact=expected(p);oracle=reference(p);imported=k.load(str(output/'finished.step'))
    native=o.command(cad,'properties',recipe,'--output','finished')
    for label,values in [('OCCT reference',k.properties(oracle)),('STEP',k.properties(imported)),
                         ('native',(native['volume'],native['volume_centroid'],native['central_volume_inertia']))]:
        b.compare_moments(values,exact,label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,bshape))[0]
                 for a,bshape in ((oracle,imported),(imported,oracle))]
    for difference in differences:close(difference,0,'directed OCCT/STEP solid difference')
    surface=topology(imported,p)
    section=sections(cad,recipe,p)
    k.mesh(output/'finished.stl',exact[0])
    replay=root/(tag+'-replay')
    o.command(cad,'build',output/'recipe.star','--out-dir',replay)
    for artifact in ('finished.step','finished.stl','finished.brep.json','recipe.star'):
        require((output/artifact).read_bytes()==(replay/artifact).read_bytes(),
                f'{tag} captured source exact replay {artifact}')
    return {'case':tag,'parameters':p,'volume_mm3':exact[0],
            'centroid_mm':exact[1],'central_inertia_mm5':exact[2],
            'directed_difference_mm3':differences,'surface_topology':surface,
            'sections':section,'first_floor_face_remap':witness}


def check_refusals(cad,root):
    p=config();recipe=root/'revisions.star';output=root/'preserved';cache=root/'cache'
    recipe.write_text(source(p))
    before_manifest=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    baseline=o.inventory(output)
    first_hash=before_manifest['feature_hashes']['first_cut']
    first_step=sha(output/'first_stage.step')
    final_step=sha(output/'finished.step')
    revised=config(depths=(3.,6.))
    recipe.write_text(source(revised))
    revised_manifest=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    require(revised_manifest['feature_hashes']['first_cut']==first_hash,
            'editing second depth reuses unchanged first native feature')
    require(revised_manifest['feature_hashes']['second_cut']!=before_manifest['feature_hashes']['second_cut'],
            'editing second depth invalidates second native feature')
    require(sha(output/'first_stage.step')==first_step and sha(output/'finished.step')!=final_step,
            'second-depth edit retains first-stage STEP bytes and changes final native solid')
    first=before_manifest['face_assertions']['first_floor_survives']['witnesses'][0]
    second=revised_manifest['face_assertions']['first_floor_survives']['witnesses'][0]
    for witness in (first,second):
        require(len(witness['anchor_faces'])==1,'first floor native ancestry remains one-to-one')
    recipe.write_text(source(p));restored=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    require(restored['feature_hashes']==before_manifest['feature_hashes'],
            'restored source reproduces semantic feature hashes')
    preserved=o.inventory(output)
    failures=[]
    for invalid in ('opposite_cap','full_web','contact','overlap','wrong_frame','fifth'):
        recipe.write_text(source(p,invalid))
        failure=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache,success=False)
        require(o.inventory(output)==preserved,invalid+' failed revision preserves complete publication')
        failures.append({'case':invalid,'diagnostic':failure})
    o.command(cad,'verify',output)
    return failures


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary=sha(cad);script=sha(__file__);sources=pinned_inputs()
    cases=[check_case(cad,args.out_dir,tag,p) for tag,p in (
        ('top',config()),('top_reverse',config(reverse=True)),
        ('bottom',config(cap='start_cap')),
        ('bottom_reverse',config(cap='start_cap',reverse=True)),
        ('top_oblique',config(pose=2)),
        ('top_oblique_posed_input',config(pose=2,posed_input=True)),
    )]
    for i,j in ((0,1),(2,3)):
        close(cases[i]['volume_mm3'],cases[j]['volume_mm3'],'reversed cut order same exact volume')
        for dimension in range(3):
            close(cases[i]['centroid_mm'][dimension],cases[j]['centroid_mm'][dimension],
                  'reversed cut order same exact centroid')
        a=k.load(str(args.out_dir/cases[i]['case']/'finished.step'))
        z=k.load(str(args.out_dir/cases[j]['case']/'finished.step'))
        for left,right in ((a,z),(z,a)):
            close(k.properties(k.boolean(BRepAlgoAPI_Cut,left,right))[0],0,
                  'reversed serial cut order same physical OCCT solid')
    fixtures=[check_fixture(cad,args.out_dir,tag,parameters) for tag,parameters in (
        ('fixture_default',{}),
        ('fixture_bottom_revised',{'plate_width':132,'first_depth':4,
                                   'second_depth':8,'pocket_side':0}),
    )]
    failures=check_refusals(cad,args.out_dir)
    require(binary==sha(cad) and script==sha(__file__) and sources==pinned_inputs(),
            'release binary, oracle and source inputs remain immutable')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'source_inventory_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),'cases':cases,
            'authored_fixture_cases':fixtures,
            'refusals':failures,
            'scope':'Serial same-cap native blind capsules at unequal depths; first-floor lineage and source revisions, OCCT solid and analytic moments.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'authored_fixture_cases':len(fixtures),
                      'refusals':len(failures),
                      'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
