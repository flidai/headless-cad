#!/usr/bin/env python3
"""Independent OCCT acceptance for mixed through and serial blind capsules."""
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
SPEC=importlib.util.spec_from_file_location('serial_oracle',ROOT/'tools/check-serial-blind-slots.py')
s=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(s)
b=s.b;o=b.o;k=b.k
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


def require(ok,reason):
    if not ok:raise AssertionError(reason)


def close(actual,expected,reason,absolute=2e-6):
    b.close(actual,expected,reason,absolute)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    paths += [ROOT/'examples/mixed_slot_fixture.star']
    return {str(path.relative_to(ROOT)):sha(path) for path in sorted(paths)}


def config(**changes):
    p=dict(expanded=False,through_count=2,blind_count=2,cap='end_cap',
           depths=(3.,5.),pose=0,posed_input=False,reverse_blind=False)
    p.update(changes)
    return p


def geometry(p):
    if p['expanded']:
        w,d,h,corner,bore=80.,60.,12.,5.,3.
        through_centers=[(-28,-16),(28,-16),(-28,16),(28,16),(0,25)]
        blind_centers=[(-9,-16),(9,-16),(-9,16),(9,16),(0,25)]
        through=[dict(id=f'through_{i+1}',center=list(center),direction=[1,0],
                      centerline_length=8.,radius=2.)
                 for i,center in enumerate(through_centers[:p['through_count']])]
        blind=[dict(id=f'blind_{i+1}',center=list(center),direction=[1,0],
                    centerline_length=8.,radius=2.5,depth=p['depths'][i]
                    if i<len(p['depths']) else 4.)
               for i,center in enumerate(blind_centers[:p['blind_count']])]
    else:
        w,d,h,corner,bore=50.,36.,8.,4.,3.
        through_centers=[(-14,8),(14,8),(-14,-8),(14,-8),(0,14)]
        blind_centers=[(-14,-8),(14,-8),(-14,8),(14,8),(0,14)]
        through=[dict(id=f'through_{i+1}',center=list(center),direction=[1,0],
                      centerline_length=6.,radius=2.)
                 for i,center in enumerate(through_centers[:p['through_count']])]
        blind=[dict(id=f'blind_{i+1}',center=list(center),direction=[1,0],
                    centerline_length=6.,radius=2.,depth=p['depths'][i]
                    if i<len(p['depths']) else 4.)
               for i,center in enumerate(blind_centers[:p['blind_count']])]
    return w,d,h,corner,bore,through,blind


def generated_source(p,invalid=None):
    w,d,h,corner,bore,through,blind=geometry(p)
    if invalid=='through_blind_contact':
        a=through[0];c=blind[0]
        c['center']=[a['center'][0]+a['centerline_length']/2+a['radius']+
                     c['centerline_length']/2+c['radius'],a['center'][1]]
    if invalid=='through_blind_overlap':
        a=through[0];c=blind[0]
        c['center']=[a['center'][0]+a['centerline_length']/2+a['radius']+
                     c['centerline_length']/2+c['radius']-.05,a['center'][1]]
    q,offset,trsf=k.pose_values(p['pose'])
    pose=lambda xyz:[sum(trsf.Value(i+1,j+1)*xyz[j] for j in range(3))+
                     trsf.Value(i+1,4) for i in range(3)]
    axis_x=[trsf.Value(i,1) for i in range(1,4)] if p['posed_input'] else [1,0,0]
    axis_y=[trsf.Value(i,2) for i in range(1,4)] if p['posed_input'] else [0,1,0]
    through_origin=pose([0,0,0]) if p['posed_input'] else [0,0,0]
    blind_origin=pose([0,0,h if p['cap']=='end_cap' else 0]) if p['posed_input'] else [0,0,h if p['cap']=='end_cap' else 0]
    lines=[f'stock = extrude(id="stock", profile=polygon(id="outline", points={json.dumps([[-w/2,-d/2],[w/2,-d/2],[w/2,d/2],[-w/2,d/2]])}, circular_holes={[{"center":[0,0],"radius":bore}]}), height={h})',
           'body = stock']
    for i in range(4):
        faces=[{'from_feature':'stock','role':f'wall_{(i-1)%4}'},
               {'from_feature':'stock','role':f'wall_{i}'}]
        lines.append(f'body = fillet(id="round_{i}",body=body,faces={json.dumps(faces)},radius={corner})')
    if p['posed_input']:
        lines.append(f'body = transform(id="posed_stock",body=body,rotation={q},offset={offset})')
    lines.append(f'through_cut = slot_pattern(id="through_cut",body=body,slots={json.dumps(through)},frame_origin={through_origin},frame_x_direction={axis_x},frame_y_direction={axis_y})')
    lines.append('output(name="through_stage",feature=through_cut)')
    order=blind[::-1] if p['reverse_blind'] else blind
    previous='through_cut'
    for i,slot in enumerate(order):
        feature=f'blind_cut_{i+1}'
        cap=p['cap']
        origin=blind_origin[:]
        if invalid=='opposite_cap' and i==1:
            cap='start_cap' if p['cap']=='end_cap' else 'end_cap'
            local=[0,0,0 if p['cap']=='end_cap' else h]
            origin=pose(local) if p['posed_input'] else local
        if invalid=='wrong_frame' and i==1:origin[2]+=.125
        depth=h if invalid=='full_web' and i==1 else slot['depth']
        lines.append(f'{feature} = blind_slot_pattern(id="{feature}",body={previous},from_feature=stock,cap="{cap}",depth={depth},slots={json.dumps([{key:value for key,value in slot.items() if key!="depth"}])},frame_origin={origin},frame_x_direction={axis_x},frame_y_direction={axis_y})')
        if i==0:lines.append(f'output(name="first_stage",feature={feature})')
        previous=feature
    lines.append(f'assert_faces(id="through_walls_survive",from_feature=through_cut,role="slot_{through[0]["id"]}_walls",in_feature={previous},count=4)')
    lines.append(f'assert_faces(id="first_floor_survives",from_feature=blind_cut_1,role="slot_{order[0]["id"]}_floor",in_feature={previous},count=1)')
    if not p['posed_input']:
        lines.append(f'finished = transform(id="finished",body={previous},rotation={q},offset={offset})')
    else:lines.append(f'finished = {previous}')
    lines.append('output(name="finished",feature=finished)')
    if invalid=='blind_as_through':
        lines.append(f'section_view(id="false_through",output="finished",origin={[0,0,h/2]})')
        lines.append(f'section_slot_dimension(view="false_through",id="false_width",from_feature=blind_cut_1,role="slot_{order[0]["id"]}_walls",kind="width",label_offset=[0,20],min=0,max=20)')
    return '\n'.join(lines)+'\n'


def stock_moments(w,d,h,corner,bore):
    missing=corner**2*(1-math.pi/4)
    first=corner**3/6
    second=corner**4*(1/3-math.pi/16)
    ax,ay=w/2-corner,d/2-corner
    area=w*d-4*missing-math.pi*bore*bore
    qxx=d*w**3/12-4*(ax*ax*missing+2*ax*first+second)-math.pi*bore**4/4
    qyy=w*d**3/12-4*(ay*ay*missing+2*ay*first+second)-math.pi*bore**4/4
    volume=area*h;zterm=volume*h*h/12
    inertia=[[h*qyy+zterm,0,0],[0,h*qxx+zterm,0],[0,0,h*(qxx+qyy)]]
    return volume,[0.,0.,h/2],inertia


def subtract_capsules(base,slots,h,cap):
    mass,center,inertia=base
    trace=sum(inertia[i][i] for i in range(3))/2
    covariance=[[(trace if i==j else 0)-inertia[i][j] for j in range(3)] for i in range(3)]
    first=[mass*x for x in center]
    raw=[[covariance[i][j]+mass*center[i]*center[j] for j in range(3)] for i in range(3)]
    for slot in slots:
        x,y=slot['center'];ux,uy=slot['direction'];radius=slot['radius']
        half=slot['centerline_length']/2
        area=4*half*radius+math.pi*radius*radius
        along=4*radius*half**3/3+8*half*radius**3/3+math.pi*radius*radius*half*half+math.pi*radius**4/4
        across=4*half*radius**3/3+math.pi*radius**4/4
        depth=slot.get('depth',h)
        z0,z1=(0,depth) if cap=='start_cap' else (h-depth,h)
        volume=area*depth;zfirst=area*(z1*z1-z0*z0)/2
        mass-=volume
        first[0]-=volume*x;first[1]-=volume*y;first[2]-=zfirst
        raw[0][0]-=depth*(along*ux*ux+across*uy*uy+area*x*x)
        raw[1][1]-=depth*(along*uy*uy+across*ux*ux+area*y*y)
        raw[0][1]-=depth*((along-across)*ux*uy+area*x*y)
        raw[0][2]-=zfirst*x;raw[1][2]-=zfirst*y
        raw[2][2]-=area*(z1**3-z0**3)/3
    for i in range(3):
        for j in range(i):raw[i][j]=raw[j][i]
    center=[value/mass for value in first]
    covariance=[[raw[i][j]-first[i]*first[j]/mass for j in range(3)] for i in range(3)]
    trace=sum(covariance[i][i] for i in range(3))
    inertia=[[(trace if i==j else 0)-covariance[i][j] for j in range(3)] for i in range(3)]
    return mass,center,inertia


def generated_expected(p):
    w,d,h,corner,bore,through,blind=geometry(p)
    base=stock_moments(w,d,h,corner,bore)
    after_through=subtract_capsules(base,through,h,'start_cap')
    result=subtract_capsules(after_through,blind,h,p['cap'])
    return k.transformed(result,k.pose_values(p['pose'])[2])


def cut_capsule(solid,slot,z0,height):
    x,y=slot['center'];ux,uy=slot['direction'];half=slot['centerline_length']/2
    radius=slot['radius']
    if ux:
        cutter=BRepPrimAPI_MakeBox(gp_Pnt(x-half,y-radius,z0),2*half,2*radius,height).Shape()
    else:
        cutter=BRepPrimAPI_MakeBox(gp_Pnt(x-radius,y-half,z0),2*radius,2*half,height).Shape()
    solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
    for sign in (-1,1):
        cutter=BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x+sign*half*ux,y+sign*half*uy,z0),
                                               gp_Dir(0,0,1)),radius,height).Shape()
        solid=k.boolean(BRepAlgoAPI_Cut,solid,cutter)
    return solid


def generated_reference(p):
    w,d,h,corner,bore,through,blind=geometry(p)
    solid=o.f.filleted([(-w/2,-d/2),(w/2,-d/2),(w/2,d/2),(-w/2,d/2)],h,corner,[0,1,2,3])
    solid=k.boolean(BRepAlgoAPI_Cut,solid,BRepPrimAPI_MakeCylinder(
        gp_Ax2(gp_Pnt(0,0,-h/4),gp_Dir(0,0,1)),bore,1.5*h).Shape())
    for slot in through:solid=cut_capsule(solid,slot,-h/4,1.5*h)
    for slot in blind:
        depth=slot['depth'];z0=-h/4 if p['cap']=='start_cap' else h-depth
        solid=cut_capsule(solid,slot,z0,depth+h/4)
    return BRepBuilderAPI_Transform(solid,k.pose_values(p['pose'])[2],True).Shape()


def actual_topology(shape,p,through,blind,h):
    require(BRepCheck_Analyzer(shape).IsValid(),'imported mixed STEP OCCT-valid')
    solids=TopExp_Explorer(shape,TopAbs_SOLID);count=0
    while solids.More():count+=1;solids.Next()
    require(count==1,'one closed mixed STEP solid')
    trsf=k.pose_values(p['pose'])[2]
    posed=lambda xyz:[sum(trsf.Value(i+1,j+1)*xyz[j] for j in range(3))+
                      trsf.Value(i+1,4) for i in range(3)]
    axis=[trsf.Value(i,3) for i in range(1,4)]
    origin=[trsf.Value(i,4) for i in range(1,4)]
    radii=[];floors=[];caps=[]
    iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():
        face=TopoDS.Face(iterator.Current());surface=BRepAdaptor_Surface(face)
        props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
        center=props.CentreOfMass();world=[center.X(),center.Y(),center.Z()]
        if surface.GetType()==GeomAbs_Plane:
            normal=surface.Plane().Axis().Direction()
            alignment=sum(a*z for a,z in zip(axis,(normal.X(),normal.Y(),normal.Z())))
            if abs(abs(alignment)-1)<1e-8:
                axial=sum((world[i]-origin[i])*axis[i] for i in range(3))
                if abs(axial)<2e-6 or abs(axial-h)<2e-6:caps.append(axial)
                else:floors.append((props.Mass(),world))
        elif surface.GetType()==GeomAbs_Cylinder:
            cylinder=surface.Cylinder()
            radii.append((cylinder.Radius(),surface.FirstUParameter(),surface.LastUParameter(),
                          surface.FirstVParameter(),surface.LastVParameter(),cylinder.Axis()))
        iterator.Next()
    require(len(caps)==2 and len(floors)==len(blind),'two stock caps and each blind floor persist')
    for slot in blind:
        depth=slot['depth'];floor_z=depth if p['cap']=='start_cap' else h-depth
        area=2*slot['radius']*slot['centerline_length']+math.pi*slot['radius']**2
        world=posed([*slot['center'],floor_z])
        require(sum(abs(a-area)<2e-6 and math.dist(point,world)<2e-6 for a,point in floors)==1,
                'actual STEP floor has independent depth/area/centroid')
    for slot in through+blind:
        depth=slot.get('depth',h);half=slot['centerline_length']/2
        for sign in (-1,1):
            world=posed([slot['center'][0]+sign*half*slot['direction'][0],
                         slot['center'][1]+sign*half*slot['direction'][1],0])
            matches=0
            for radius,u0,u1,v0,v1,axis in radii:
                if abs(radius-slot['radius'])>2e-6:continue
                point=axis.Location();direction=axis.Direction()
                location=[point.X(),point.Y(),point.Z()]
                vector=[direction.X(),direction.Y(),direction.Z()]
                delta=[world[i]-location[i] for i in range(3)]
                along=sum(delta[i]*vector[i] for i in range(3))
                transverse=math.sqrt(sum((delta[i]-along*vector[i])**2 for i in range(3)))
                if transverse<2e-6:
                    close(u1-u0,math.pi,'mixed STEP semicylinder angular trim')
                    close(v1-v0,depth,'through or blind actual STEP wall depth')
                    matches+=1
            require(matches==1,'one true cylindrical wall at each capsule endpoint axis')
    return {'through_slots':len(through),'blind_slots':len(blind),'floor_faces':len(floors),
            'semicylinder_faces':2*(len(through)+len(blind))}


def section_strata(cad,recipe,output,p,through,blind,h,base_area,base_loops=2,parameters=()):
    if p['pose']!=0:return None
    values=sorted({0,*[slot['depth'] for slot in blind],h})
    through_area=sum(2*slot['radius']*slot['centerline_length']+
                     math.pi*slot['radius']**2 for slot in through)
    result=[]
    for lo,hi in zip(values,values[1:]):
        depth=(lo+hi)/2
        z=depth if p['cap']=='start_cap' else h-depth
        active=[slot for slot in blind if slot['depth']>depth]
        blind_area=sum(2*slot['radius']*slot['centerline_length']+
                       math.pi*slot['radius']**2 for slot in active)
        query=o.command(cad,'section',recipe,'--output',output,'--origin',f'0,0,{z}',*parameters)
        section=query['section']
        close(section['area'],base_area-through_area-blind_area,
              'mixed section material area at individual depth stratum',2e-5)
        require(len(section['loops'])==base_loops+len(through)+len(active),
                'all actual through loops persist through each blind depth stratum')
        result.append({'z':z,'active_blind':len(active),'through_loops':len(through),
                       'area_mm2':section['area'],'loops':len(section['loops'])})
    return result


def check_generated(cad,root,tag,p):
    recipe=root/(tag+'.star');output=root/tag
    recipe.write_text(generated_source(p))
    manifest=o.command(cad,'build',recipe,'--out-dir',output)
    o.command(cad,'verify',output)
    through_witness=manifest['face_assertions']['through_walls_survive']
    first_witness=manifest['face_assertions']['first_floor_survives']
    require(through_witness['status']=='resolved' and len(through_witness['witnesses'])==4,
            'true through walls survive serial blind cuts by native lineage')
    require(all(w['anchor_faces']==[w['target_face']] for w in through_witness['witnesses']),
            'actual through wall native FaceIds remain unchanged by blind cuts')
    require(first_witness['status']=='resolved' and len(first_witness['witnesses'])==1,
            'first native blind floor survives later blind cut')
    floor_face=first_witness['witnesses'][0]
    if p['blind_count']>1:
        require(floor_face['anchor_faces'][0]!=floor_face['target_face'],
                'first floor FaceId remapped after later blind walls inserted')
    expected=generated_expected(p)
    oracle=generated_reference(p)
    imported=k.load(str(output/'finished.step'))
    native=o.command(cad,'properties',recipe,'--output','finished')
    for label,properties in [('independent OCCT',k.properties(oracle)),
                             ('imported STEP',k.properties(imported)),
                             ('native',(native['volume'],native['volume_centroid'],
                                        native['central_volume_inertia']))]:
        b.compare_moments(properties,expected,label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,z))[0]
                 for a,z in ((oracle,imported),(imported,oracle))]
    for value in differences:close(value,0,'mixed OCCT/STEP directed solid difference')
    w,d,h,corner,bore,through,blind=geometry(p)
    surfaces=actual_topology(imported,p,through,blind,h)
    base_area=stock_moments(w,d,h,corner,bore)[0]/h
    strata=section_strata(cad,recipe,'finished',p,through,blind,h,base_area)
    k.mesh(output/'finished.stl',expected[0])
    replay=root/(tag+'-replay')
    repeated=o.command(cad,'build',output/'recipe.star','--out-dir',replay)
    for artifact in ('through_stage.step','first_stage.step','finished.step',
                     'finished.stl','finished.brep.json','recipe.star'):
        require((output/artifact).read_bytes()==(replay/artifact).read_bytes(),
                tag+' exact captured replay '+artifact)
    require(manifest['feature_hashes']==repeated['feature_hashes'],
            'mixed native feature hashes captured replay')
    return {'case':tag,'parameters':p,'volume_mm3':expected[0],
            'centroid_mm':expected[1],'central_inertia_mm5':expected[2],
            'directed_difference_mm3':differences,'topology':surfaces,'strata':strata,
            'through_witnesses':len(through_witness['witnesses']),
            'first_floor_remap':floor_face}


def authored_slots(width,first_depth,second_depth):
    through=[dict(id=name,center=[x,y],direction=[0,1],centerline_length=8.,radius=3.)
             for name,x,y in [('front_lower',20,15),('front_upper',20,65),
                              ('rear_lower',width-20,15),('rear_upper',width-20,65)]]
    blind=s.fixture_slots(width,first_depth,second_depth)
    return through,blind


def check_authored(cad,root,tag,parameters):
    source=ROOT/'examples/mixed_slot_fixture.star';output=root/tag
    args=[word for key,value in parameters.items() for word in ('--param',f'{key}={value}')]
    manifest=o.command(cad,'build',source,'--out-dir',output,*args)
    o.command(cad,'verify',output)
    width=parameters.get('plate_width',120)
    first_depth=parameters.get('first_depth',3)
    second_depth=parameters.get('second_depth',7)
    side=parameters.get('pocket_side',1)
    through,blind=authored_slots(width,first_depth,second_depth)
    stock=(width*80*12,[width/2,40,6],
           [[width*80*12*(80*80+12*12)/12,0,0],
            [0,width*80*12*(width*width+12*12)/12,0],
            [0,0,width*80*12*(width*width+80*80)/12]])
    bores=[dict(center=[x,y],direction=[1,0],centerline_length=0.,radius=3.)
           for x in (10,width-10) for y in (10,70)]
    exact=subtract_capsules(stock,bores,12,'start_cap')
    exact=subtract_capsules(exact,through,12,'start_cap')
    exact=subtract_capsules(exact,blind,12,'start_cap' if side==0 else 'end_cap')
    oracle=s.fixture_reference(width,first_depth,second_depth,side)
    for slot in through:oracle=cut_capsule(oracle,slot,-3,18)
    imported=k.load(str(output/'fixture.step'))
    native=o.command(cad,'properties',source,'--output','fixture',*args)
    for label,properties in [('authored OCCT',k.properties(oracle)),
                             ('authored STEP',k.properties(imported)),
                             ('authored native',(native['volume'],native['volume_centroid'],
                                                 native['central_volume_inertia']))]:
        b.compare_moments(properties,exact,label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,z))[0]
                 for a,z in ((oracle,imported),(imported,oracle))]
    for value in differences:close(value,0,'authored mixed OCCT/STEP difference')
    p=config(cap='start_cap' if side==0 else 'end_cap')
    actual_topology(imported,p,through,blind,12)
    base_area=width*80-4*math.pi*9
    strata=section_strata(cad,source,'fixture',p,through,blind,12,base_area,
                          base_loops=5,parameters=args)
    require(len(strata)==3,'authored three exact through/blind section strata')
    first=manifest['face_assertions']['first_floor_survives']
    require(first['status']=='resolved' and len(first['witnesses'])==1
            and first['witnesses'][0]['anchor_faces'][0]!=first['witnesses'][0]['target_face'],
            'authored first floor is remapped through final mixed body')
    for name in ('front_lower','front_upper','rear_lower','rear_upper'):
        report=manifest['face_assertions'][name+'_walls_survive']
        require(report['status']=='resolved' and len(report['witnesses'])==4,
                'authored true through wall lineage remains exact')
        require(all(w['anchor_faces']==[w['target_face']] for w in report['witnesses']),
                'authored through wall FaceIds survive both blind operations')
    dimensions=manifest['datum_dimensions']
    for name,value in {'locating_depth':first_depth,'relief_depth':second_depth,
                       'remaining_web':12-second_depth}.items():
        close(dimensions[name]['value_mm'],value,'authored native floor depth')
    section_sizes=json.loads((output/'section-through_sizes.json').read_text())
    sizes={d['declaration']['id']:d for d in section_sizes['section_view']['slot_dimensions']}
    require(set(sizes)=={'mounting_width','mounting_length'},'exact through-size report inventory')
    close(sizes['mounting_width']['value_mm'],6,'native through width report')
    close(sizes['mounting_length']['value_mm'],14,'native through overall length report')
    position=json.loads((output/'section-through_positions.json').read_text())
    close(position['section_view']['dimensions'][0]['value_mm'],width-40,
          'surviving through center datum pitch')
    drawing=(output/'drawing-mixed_fixture_machining.svg').read_text()
    for name,value in {'locating_depth':first_depth,'relief_depth':second_depth,
                       'remaining_web':12-second_depth}.items():
        require(f'data-dimension="{name}"' in drawing and f'{value:.3f} mm' in drawing,
                'authored printed depth table binds actual floor datum')
    k.mesh(output/'fixture.stl',exact[0])
    captured=root/(tag+'-captured');captured.mkdir()
    captured_source=captured/'mixed_slot_fixture.star'
    shutil.copyfile(output/'recipe.star',captured_source)
    require(sha(captured_source)==sha(source),'authored mixed source byte-identical capture')
    replay=root/(tag+'-replay')
    repeated=o.command(cad,'build',captured_source,'--out-dir',replay,*args)
    for artifact in ('through_stage.step','first_stage.step','fixture.step','fixture.stl',
                     'drawing-mixed_fixture_machining.svg','drawing-mixed_mounting_references.svg'):
        require((output/artifact).read_bytes()==(replay/artifact).read_bytes(),
                tag+' exact authored captured replay '+artifact)
    require(manifest['feature_hashes']==repeated['feature_hashes'],
            'authored mixed feature hashes exact replay')
    return {'case':tag,'parameters':parameters,'volume_mm3':exact[0],
            'centroid_mm':exact[1],'central_inertia_mm5':exact[2],
            'directed_difference_mm3':differences,'strata':strata,
            'through_center_pitch_mm':width-40}


def refusals(cad,root):
    recipe=root/'revisions.star';output=root/'preserved';cache=root/'cache'
    initial=config();recipe.write_text(generated_source(initial))
    first=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    through_hash=sha(output/'through_stage.step')
    first_hash=sha(output/'first_stage.step')
    original_hash=sha(output/'finished.step')
    modified=config(depths=(3.,6.));recipe.write_text(generated_source(modified))
    second=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    require(sha(output/'through_stage.step')==through_hash
            and sha(output/'first_stage.step')==first_hash
            and sha(output/'finished.step')!=original_hash,
            'second blind depth edit preserves through and first cut geometry bytes')
    require(second['feature_hashes']['through_cut']==first['feature_hashes']['through_cut']
            and second['feature_hashes']['blind_cut_1']==first['feature_hashes']['blind_cut_1']
            and second['feature_hashes']['blind_cut_2']!=first['feature_hashes']['blind_cut_2'],
            'native cache invalidation limited to second blind feature')
    recipe.write_text(generated_source(initial))
    restored=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache)
    require(restored['feature_hashes']==first['feature_hashes'],
            'source restoration preserves semantic feature hashes')
    preserved=o.inventory(output)
    failures=[]
    cases=[('opposite_cap',initial),('full_web',initial),
           ('through_blind_contact',initial),('through_blind_overlap',initial),
           ('wrong_frame',initial),('blind_as_through',initial),
           ('fifth_through',config(expanded=True,through_count=5,blind_count=2,
                                  depths=(3.,7.))),
           ('fifth_blind',config(expanded=True,through_count=4,blind_count=5,
                                depths=(2.,3.,5.,7.,8.)))]
    for invalid,p in cases:
        recipe.write_text(generated_source(p,None if invalid in ('fifth_through','fifth_blind') else invalid))
        failure=o.command(cad,'build',recipe,'--out-dir',output,'--cache-dir',cache,success=False)
        require(o.inventory(output)==preserved,
                invalid+' rejected without modifying complete published artifact inventory')
        failures.append({'case':invalid,'diagnostic':failure})
    o.command(cad,'verify',output)
    return failures


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary=sha(cad);script=sha(__file__);sources=pinned_inputs()
    cases=[check_generated(cad,args.out_dir,tag,p) for tag,p in (
        ('top',config()),('top_reversed',config(reverse_blind=True)),
        ('bottom',config(cap='start_cap')),
        ('bottom_reversed',config(cap='start_cap',reverse_blind=True)),
        ('top_oblique',config(pose=2)),
        ('top_oblique_posed_input',config(pose=2,posed_input=True)),
        ('four_plus_four',config(expanded=True,through_count=4,blind_count=4,
                                depths=(2.,3.,5.,7.))),
    )]
    for i,j in ((0,1),(2,3)):
        a=k.load(str(args.out_dir/cases[i]['case']/'finished.step'))
        z=k.load(str(args.out_dir/cases[j]['case']/'finished.step'))
        for left,right in ((a,z),(z,a)):
            close(k.properties(k.boolean(BRepAlgoAPI_Cut,left,right))[0],0,
                  'reversed blind operation order same physical mixed solid')
    authored=[check_authored(cad,args.out_dir,tag,parameters) for tag,parameters in (
        ('fixture_default',{}),
        ('fixture_bottom_wide',{'plate_width':132,'first_depth':4,'second_depth':8,
                                'pocket_side':0}),
    )]
    failed=refusals(cad,args.out_dir)
    require(binary==sha(cad) and script==sha(__file__) and sources==pinned_inputs(),
            'frozen binary, oracle and supporting source inventory unchanged')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'source_inventory_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'authored_fixture_cases':authored,'refusals':failed,
            'scope':'One closed analytic solid with actual through obrounds and serial same-cap blind obround pockets; independent OCCT Boolean and closed-form mass moments.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'authored_fixture_cases':len(authored),
                      'refusals':len(failed),'report':str(args.out_dir/'report.json')}))


if __name__=='__main__':main()
