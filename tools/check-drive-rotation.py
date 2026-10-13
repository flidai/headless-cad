#!/usr/bin/env python3
"""Independent sampled STEP evidence for a native continuous rotation certificate.

The native proof covers the entire path. OCCT independently checks declared
geometry, sampled distances and all pair overlaps; sampling alone is not proof.
"""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

SPEC=importlib.util.spec_from_file_location('retained_drive',Path(__file__).with_name('check-retained-drive-module.py'))
d=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(d)
k=d.k;c=d.coupling.convex
f=d.oracle('rounded_drive_fillet_oracle','check-longitudinal-fillet.py')
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common,BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder
from OCP.gp import gp_Trsf,gp_Ax1,gp_Pnt,gp_Dir
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_FACE
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Plane
from OCP.TopoDS import TopoDS
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps

MOVING={('shaft',),('drive_disc',),('parallel_key',),('shaft_washer',),('shaft_ring',),
        ('hub_left_washer',),('hub_left_ring',),('hub_right_washer',),('hub_right_ring',)}
COVER_SHAFT_CHECKS={
    'left_cover_shaft_gap':(('left_support','left_cover'),2.5,2.0),
    'right_cover_shaft_gap':(('right_support','right_cover'),4.5,4.0),
}

HOUSING_OUTLINE=[(-22,-26),(22,-26),(22,14),(14,22),(-14,22),(-22,14)]
COVER_CENTERS=(-17,17)

def cover_references(diameter,pitch,depth,thickness):
    """Construct the new stationary solids without reading native BReps/meshes."""
    r=diameter/2;p=pitch;q=[.5,.5,.5,.5]
    housing=d.bored_polygon(HOUSING_OUTLINE,16,[(0,0,r+4.025)])
    left,right=housing,housing
    for x in COVER_CENTERS:
        left=k.boolean(BRepAlgoAPI_Cut,left,BRepPrimAPI_MakeCylinder(
            f.gp_Ax2(gp_Pnt(x,0,-1),gp_Dir(0,0,1)),1.75,depth+1).Shape())
        right=k.boolean(BRepAlgoAPI_Cut,right,BRepPrimAPI_MakeCylinder(
            f.gp_Ax2(gp_Pnt(x,0,16-depth),gp_Dir(0,0,1)),1.75,depth+1).Shape())
    cover=d.bored_polygon(HOUSING_OUTLINE,thickness,[(0,0,r+4.5)]+[(x,0,2.05) for x in COVER_CENTERS])
    shank=depth-0.5+2+thickness
    screw=d.retained.revolve([[0,0],[1.65,0],[1.65,shank],[3,shank],[3,shank+2],[0,shank+2]])
    definitions={'housing_left':left,'housing_right':right,'end_cover':cover,
                 'cover_screw_envelope':screw}
    left_support=[25,0,32];right_support=[25+p,0,32]
    references={
        ('left_support','housing'):d.placed(left,left_support,q),
        ('right_support','right_housing'):d.placed(right,right_support,q),
        ('left_support','left_cover'):d.placed(d.placed(cover,[0,0,-2-thickness]),left_support,q),
        ('right_support','right_cover'):d.placed(d.placed(cover,[0,0,18]),right_support,q),
    }
    for side,support,origin,rotation in (
            ('left',left_support,depth-0.5,[1,0,0,0]),
            ('right',right_support,16-depth+0.5,None)):
        for label,x in (('minus',-17),('plus',17)):
            body=d.placed(screw,[x,0,origin],rotation)
            references[(side+'_support',side+'_cover_screw_'+label)]=d.placed(body,support,q)
    part_ids={path:('housing_left' if path==('left_support','housing') else
                    'housing_right' if path==('right_support','right_housing') else
                    'end_cover' if path[-1].endswith('_cover') else
                    'cover_screw_envelope') for path in references}
    expected={
        'housing_left':16*(2048-math.pi*(r+4.025)**2)-2*math.pi*1.75**2*depth,
        'housing_right':16*(2048-math.pi*(r+4.025)**2)-2*math.pi*1.75**2*depth,
        'end_cover':thickness*(2048-math.pi*(r+4.5)**2-2*math.pi*2.05**2),
        'cover_screw_envelope':math.pi*(1.65**2*shank+3**2*2),
    }
    return definitions,references,part_ids,expected

def rotor_envelopes(diameter,pitch):
    """Conservative axial/radial bounds invariant under the authored X rotation."""
    r=diameter/2;p=pitch;d0=25+p/2
    return {
        ('shaft',):(10,p+80,r+2),
        ('drive_disc',):(d0,d0+14,24),
        ('parallel_key',):(d0+.5,d0+13.5,math.hypot(r+2,2)),
        ('hub_left_washer',):(d0-1.6,d0-.1,r+2),
        ('hub_left_ring',):(d0-2.8,d0-1.6,r+1.5),
        ('hub_right_washer',):(d0+14.1,d0+15.6,r+2),
        ('hub_right_ring',):(d0+15.6,d0+16.8,r+1.5),
        ('shaft_washer',):(p+43.1,p+44.6,r+2),
        ('shaft_ring',):(p+44.6,p+45.8,r+1.5),
    }

def stationary_envelopes(diameter,pitch,depth,thickness):
    """Each fixed cover/screw lies outside a coaxial radial exclusion cylinder."""
    r=diameter/2;p=pitch
    envelopes={
        ('left_support','left_cover'):(23-thickness,23,r+4.5),
        ('right_support','right_cover'):(p+43,p+43+thickness,r+4.5),
    }
    for side,first,last in [('left',21-thickness,25+depth-0.5),
                            ('right',p+41.5-depth,p+45+thickness)]:
        for label in ['minus','plus']:
            envelopes[(side+'_support',side+'_cover_screw_'+label)]=(first,last,17-3)
    return envelopes

def all_phase_new_pair_certificates(diameter,pitch,depth,thickness):
    """Positive separation bounds for every new fixed/rotor pair, not samples."""
    fixed=stationary_envelopes(diameter,pitch,depth,thickness)
    moving=rotor_envelopes(diameter,pitch)
    d.require(set(moving)==MOVING and len(fixed)==6,'complete moving/new fixed envelope inventory')
    certificates={}
    # Far larger than floating-point subtraction error at this assembly scale;
    # reserves 0.1 micron for transformed/exported nominal surfaces as well.
    guard=max(1e-4,256*sys.float_info.epsilon*(pitch+100))
    for fixed_path,(fmin,fmax,radial_floor) in fixed.items():
        for moving_path,(mmin,mmax,radial_ceiling) in moving.items():
            # The shaft's only r+2 shoulder ends at world X=10+12.9;
            # later grooves/keyseat remove material from the r journal.
            if moving_path==('shaft',) and fmin>=10+12.9:
                radial_ceiling=diameter/2
            axial_gap=max(fmin-mmax,mmin-fmax,0)
            radial_gap=radial_floor-radial_ceiling
            raw_bound=max(axial_gap,radial_gap)
            bound=raw_bound-guard
            d.require(bound>0,'uncertified fixed/rotor pair '+str((fixed_path,moving_path)))
            certificates['/'.join(fixed_path)+' | '+'/'.join(moving_path)]={
                'minimum_distance_lower_bound_mm':bound,
                'raw_geometric_bound_mm':raw_bound,
                'arithmetic_and_export_guard_mm':guard,
                'rotor_radial_ceiling_mm':radial_ceiling,
                'method':'invariant_axial_interval' if axial_gap>=radial_gap else 'invariant_radial_exclusion',
            }
    d.require(len(certificates)==54,'all 6 new fixed × 9 rotor pairs certified')
    return certificates

def posed(shape,angle):
    frame=gp_Trsf();frame.SetRotation(gp_Ax1(gp_Pnt(10,0,32),gp_Dir(1,0,0)),angle)
    return BRepBuilderAPI_Transform(shape,frame,True).Shape()

def boundary_distance(point,shape):
    faces=[];iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():faces.append(iterator.Current());iterator.Next()
    return min(c.witness_distance(point,face) for face in faces)

def planar_face_at(shape,x,y,area,label):
    """Select an actual STEP face by independent geometric support and area."""
    matches=[];iterator=TopExp_Explorer(shape,TopAbs_FACE)
    while iterator.More():
        face=TopoDS.Face(iterator.Current());surface=BRepAdaptor_Surface(face)
        if surface.GetType()==GeomAbs_Plane:
            props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
            p=props.CentreOfMass();normal=surface.Plane().Axis().Direction()
            if (abs(surface.Plane().Location().X()-x)<2e-6 and
                abs(p.Y()-y)<2e-6 and abs(props.Mass()-area)<2e-5 and
                abs(abs(normal.X())-1)<1e-9):
                matches.append((face,surface.Plane().Location().X(),props.Mass()))
        iterator.Next()
    d.require(len(matches)==1,'one exact STEP planar face '+label+': '+str(len(matches)))
    return matches[0]

def cover_axial_faces(shapes,pitch,depth,thickness):
    """Measure both blind floors, screw tips and cover caps from STEP topology."""
    evidence={}
    for side,prefix,floor,tip,cover_inner,cover_outer in [
            ('left',25,depth,depth-0.5,-2,-2-thickness),
            ('right',25+pitch,16-depth,16-depth+0.5,18,18+thickness)]:
        housing=shapes[(side+'_support','housing' if side=='left' else 'right_housing')]
        cover=shapes[(side+'_support',side+'_cover')]
        for tag,y in [('minus',-17),('plus',17)]:
            screw=shapes[(side+'_support',side+'_cover_screw_'+tag)]
            floor_face=planar_face_at(housing,prefix+floor,y,math.pi*1.75**2,side+' '+tag+' floor')
            tip_face=planar_face_at(screw,prefix+tip,y,math.pi*1.65**2,side+' '+tag+' tip')
            gap=abs(floor_face[1]-tip_face[1]);d.close(gap,.5,'physical screw tip-to-floor gap',absolute=2e-6)
            evidence[side+'_'+tag+'_tip_floor']={'floor_x_mm':floor_face[1],
                'tip_x_mm':tip_face[1],'gap_mm':gap,
                'floor_area_mm2':floor_face[2],'tip_area_mm2':tip_face[2]}
        # The cover's broad end faces remain planar after all three through bores.
        for label,z in [('inner',cover_inner),('outer',cover_outer)]:
            candidates=[];it=TopExp_Explorer(cover,TopAbs_FACE)
            while it.More():
                face=TopoDS.Face(it.Current());surface=BRepAdaptor_Surface(face)
                if surface.GetType()==GeomAbs_Plane:
                    props=GProp_GProps();BRepGProp.SurfaceProperties_s(face,props)
                    if abs(surface.Plane().Location().X()-(prefix+z))<2e-6 and props.Mass()>1000:
                        candidates.append((surface.Plane().Location().X(),props.Mass()))
                it.Next()
            d.require(len(candidates)==1,'one broad cover '+side+' '+label+' face')
            evidence[side+'_cover_'+label]={'x_mm':candidates[0][0],'area_mm2':candidates[0][1]}
        d.close(abs(evidence[side+'_cover_inner']['x_mm']-evidence[side+'_cover_outer']['x_mm']),
                thickness,'physical cover thickness',absolute=2e-6)
    return evidence

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def native_cover_dimensions(out,manifest,pitch,depth,thickness,physical_faces):
    """Bind native measurement witnesses to independently identified STEP planes."""
    report=json.loads((out/'datum-dimensions.json').read_text())
    d.require(report['recipe_context_hash']==manifest['recipe_context_hash'],
              'datum report uses current source revision')
    d.require(report['dimensions']==manifest['datum_dimensions'],
              'published native dimensions equal current manifest')
    facts={}
    for side,prefix,outboard,inboard in [
            ('left',25,0,16),('right',25+pitch,16,0)]:
        housing_path=[side+'_support','housing' if side=='left' else 'right_housing']
        cover_path=[side+'_support',side+'_cover']
        for tag in ['minus','plus']:
            floor=physical_faces[side+'_'+tag+'_tip_floor']['floor_x_mm']
            for suffix,value,coords in [
                    ('depth',depth,[prefix+outboard,floor]),
                    ('remaining_floor',16-depth,[prefix+inboard,floor])]:
                name=side+'_cover_'+tag+'_'+suffix
                measurement=report['dimensions'][name]
                d.require(measurement['status']=='passed' and
                          measurement['declaration']['output']=='drive_module_release',
                          'current passed native '+name)
                d.close(measurement['value_mm'],value,'native measured '+name,absolute=2e-6)
                d.require(measurement['lower_bound_mm']<=value<=measurement['upper_bound_mm'],
                          'native arithmetic interval encloses STEP '+name)
                endpoints=[measurement['from'],measurement['to']]
                d.require(all(e['instance_path']==housing_path for e in endpoints),
                          'housing face-bound occurrence '+name)
                for endpoint in endpoints:
                    semantic=endpoint['semantic_datum']
                    d.require(semantic['selection']['status']=='resolved' and
                              len(semantic['selection']['witnesses'])==1 and
                              semantic['definition']['offset']==[0,0,0] and
                              semantic['target_content_hash']==endpoint['part_content_hash'],
                              'single current native face witness '+name)
                actual=sorted(e['world_plane']['origin'][0] for e in endpoints)
                for got,want in zip(actual,sorted(coords)):
                    d.close(got,want,'physical STEP support plane '+name,absolute=2e-6)
                roles={e['semantic_datum']['definition']['role'] for e in endpoints}
                d.require('hole_cover_'+tag+'_floor' in roles,
                          'named blind floor anchors '+name)
                facts[name]={'value_mm':measurement['value_mm'],
                             'lower_bound_mm':measurement['lower_bound_mm'],
                             'upper_bound_mm':measurement['upper_bound_mm'],
                             'world_support_x_mm':actual}
        name=side+'_cover_thickness';measurement=report['dimensions'][name]
        d.require(measurement['status']=='passed' and
                  measurement['declaration']['output']=='drive_module_release',
                  'current passed cover thickness '+side)
        d.close(measurement['value_mm'],thickness,'native cover thickness '+side,absolute=2e-6)
        d.require(measurement['lower_bound_mm']<=thickness<=measurement['upper_bound_mm'],
                  'native cover thickness interval '+side)
        endpoints=[measurement['from'],measurement['to']]
        d.require(all(e['instance_path']==cover_path for e in endpoints),
                  'cover face-bound occurrence '+side)
        for endpoint in endpoints:
            semantic=endpoint['semantic_datum']
            d.require(semantic['selection']['status']=='resolved' and
                      len(semantic['selection']['witnesses'])==1 and
                      semantic['definition']['offset']==[0,0,0] and
                      semantic['target_content_hash']==endpoint['part_content_hash'],
                      'single current native cover face '+side)
        actual=sorted(e['world_plane']['origin'][0] for e in endpoints)
        expected=sorted([physical_faces[side+'_cover_inner']['x_mm'],
                         physical_faces[side+'_cover_outer']['x_mm']])
        for got,want in zip(actual,expected):
            d.close(got,want,'physical STEP cover cap '+side,absolute=2e-6)
        d.require({e['semantic_datum']['definition']['role'] for e in endpoints}==
                  {'start_cap','end_cap'},'both native cover caps '+side)
        facts[name]={'value_mm':measurement['value_mm'],
                     'lower_bound_mm':measurement['lower_bound_mm'],
                     'upper_bound_mm':measurement['upper_bound_mm'],
                     'world_support_x_mm':actual}
    d.require(len(facts)==10,'ten sourced machining dimensions')
    return facts

def axial_size_allowances(shapes,manifest,diameter,pitch,depth,thickness,native_dimensions):
    """Independently identify flange/shank STEP planes and combine size bands."""
    from decimal import Decimal
    dimensions=manifest['datum_dimensions'];facts={};stacks={};r=diameter/2
    def measured(name,path,coordinates,expected):
        report=dimensions[name]
        d.require(report['status']=='passed' and report['declaration']['output']=='drive_module_release',name)
        d.close(report['value_mm'],expected,'STEP measured '+name,absolute=2e-6)
        d.require(report['lower_bound_mm']<=expected<=report['upper_bound_mm'],'enclosing interval '+name)
        endpoints=[report['from'],report['to']]
        for endpoint in endpoints:
            witness=endpoint['semantic_datum']
            d.require(endpoint['instance_path']==list(path) and witness['selection']['status']=='resolved'
                      and len(witness['selection']['witnesses'])==1
                      and witness['target_content_hash']==endpoint['part_content_hash'],'face-bound '+name)
        actual=sorted(e['world_plane']['origin'][0] for e in endpoints)
        for a,b in zip(actual,sorted(coordinates)):d.close(a,b,'actual STEP support '+name,absolute=2e-6)
        facts[name]={'value_mm':expected,'world_support_x_mm':actual}
    for side,prefix,flange_x in [('left',25,[23,25]),('right',25+pitch,[43+pitch,41+pitch])]:
        sleeve_path=(side+'_support','sleeve' if side=='left' else 'right_sleeve')
        areas=[math.pi*((r+6)**2-(r+.15)**2),math.pi*((r+6)**2-(r+4)**2)]
        planes=[planar_face_at(shapes[sleeve_path],x,0,area,side+' flange')[1]
                for x,area in zip(flange_x,areas)]
        flange=side+'_flange_thickness';measured(flange,sleeve_path,planes,2.)
        native_dimensions[flange]=facts[flange]
        for tag,y in [('minus',-17),('plus',17)]:
            path=(side+'_support',side+'_cover_screw_'+tag)
            tip=prefix+(depth-.5 if side=='left' else 16-depth+.5)
            underhead=prefix+(-2-thickness if side=='left' else 18+thickness)
            planes=[planar_face_at(shapes[path],tip,y,math.pi*1.65**2,side+' screw tip')[1],
                    planar_face_at(shapes[path],underhead,y,math.pi*(3**2-1.65**2),side+' underhead')[1]]
            shank=side+'_cover_'+tag+'_shank';length=depth-.5+2+thickness
            measured(shank,path,planes,length)
            if tag=='minus':native_dimensions[shank]=facts[shank]
            terms=[(side+'_cover_'+tag+'_depth',depth,.1,1),
                   (flange,2.,.05,1),(side+'_cover_thickness',thickness,.05,1),
                   (shank,length,.1,-1)]
            lower=Decimal(0);upper=Decimal(0);nominal=Decimal(0)
            for name,value,band,sign in terms:
                tol=dimensions[name]['tolerance']
                for key,want in [('nominal_mm',value),('lower_deviation_mm',-band),
                                 ('upper_deviation_mm',band),('lower_limit_mm',value-band),
                                 ('upper_limit_mm',value+band)]:
                    d.close(tol[key],want,'authored '+name+' '+key,absolute=1e-12)
                v,b=Decimal(str(value)),Decimal(str(band));nominal+=sign*v
                lower+=v-b if sign==1 else -v-b
                upper+=v+b if sign==1 else -v+b
            name=side+'_cover_'+tag+'_tip_allowance';stack=manifest['tolerance_stacks'][name]
            d.require(stack['status']=='passed' and stack['measurement_interval_certified'],'certified native stack '+name)
            d.require([(t['datum_dimension'],t['coefficient']) for t in stack['terms']]==
                      [(n,s) for n,v,b,s in terms],'exact independent signed terms '+name)
            for key,want in [('nominal_mm',float(nominal)),('lower_limit_mm',float(lower)),
                             ('upper_limit_mm',float(upper))]:d.close(stack[key],want,'independent decimal stack '+name,absolute=1e-10)
            d.require(stack['lower_limit_mm']<=float(lower) and stack['upper_limit_mm']>=float(upper),
                      'outward design interval '+name)
            d.require(stack['lower_limit_mm']>=.15 and stack['upper_limit_mm']<=.85,'positive conditional allowance')
            stacks[name]={'nominal_mm':float(nominal),'independent_limits_mm':[str(lower),str(upper)],
                          'native_limits_mm':[stack['lower_limit_mm'],stack['upper_limit_mm']]}
    d.require(len(dimensions)==26 and len(facts)==6 and len(stacks)==4,'complete axial allowance inventory')
    return {'native_step_measurements':facts,'authored_independent_stacks':stacks,
            'scope':'Independent size bands conditional on nominal seating; no thread, preload or manufactured conformance proof'}

def machining_sheet_binding(out,manifest,native_dimensions):
    """Verify every published table row embeds its exact current native report."""
    sheet=json.loads((out/'drawing-end_cover_machining.json').read_text())
    datum=json.loads((out/'datum-dimensions.json').read_text())
    source=sheet['datum_dimension_report']
    d.require(sheet['schema_version']==2 and sheet['status']=='published' and
              sheet['recipe_context_hash']==manifest['recipe_context_hash'] and
              sheet['recipe_source']==manifest['recipe_source'],
              'drawing table tied to this source revision')
    d.require(source['file']=='datum-dimensions.json' and source['algorithm']=='blake3' and
              source['digest']==manifest['artifact_integrity']['files']['datum-dimensions.json']['digest'],
              'drawing binds staged native dimension report bytes')
    d.require(manifest['drawing_sheets']['end_cover_machining']['status']=='published',
              'current machining sheet is published')
    tables=sheet['dimension_tables']
    d.require({t['id'] for t in tables}=={'left_pilots','right_pilots'} and len(tables)==2,
              'two exact pilot tables')
    xml=ET.fromstring((out/'drawing-end_cover_machining.svg').read_text())
    groups={e.attrib['id']:e for e in xml.iter() if e.tag.endswith('g') and 'id' in e.attrib}
    seen=set();rows=[]
    for table in tables:
        d.require(table['output']=='drive_module_release' and len(table['rows'])==7 and table['show_size_limits'],
                  'seven current assembly rows per side with authored limits')
        d.require('drawing-table-'+table['id'] in groups,'SVG table '+table['id'])
        box=table['paper_box_mm']
        d.require(box['x']>=10 and box['y']>=10 and box['x']+box['width']<=410 and
                  box['y']+box['height']<=262,'table inside A3 printable region')
        for row in table['rows']:
            name=row['dimension'];d.require(name in native_dimensions and name not in seen,
                                           'one exact native dimension per row '+name)
            seen.add(name)
            measurement=datum['dimensions'][name]
            d.require(row['native_dimension']==measurement and
                      measurement==manifest['datum_dimensions'][name],
                      'full native face/interval witness embedded '+name)
            group=groups['drawing-table-'+table['id']+'-'+name]
            d.require(group.attrib.get('data-dimension')==name,
                      'SVG row identity '+name)
            visible=' '.join(''.join(group.itertext()).split())
            expected=f"≈ {measurement['value_mm']:.3f} mm"
            d.require(expected in visible,'displayed native millimetres '+name+': '+visible)
            limits=row['authored_size_limits_mm'];tol=measurement.get('tolerance')
            if tol:
                d.require(limits==[tol['lower_limit_mm'],tol['upper_limit_mm']],'exact report size bands '+name)
                printed=re.search(r'([-+0-9.eE]+)\.\.([-+0-9.eE]+) mm',visible)
                d.require(printed is not None and [float(v) for v in printed.groups()]==limits,
                          'round-trip authored size limits '+name)
            else:
                d.require(limits is None and 'not specified' in visible,'no invented size band '+name)
            rows.append({'id':name,'value_mm':measurement['value_mm'],
                         'native_report_digest':source['digest'],'svg_text':visible})
    d.require(seen==set(native_dimensions),'every expected machining dimension displayed')
    all_text=' '.join(xml.itertext())
    d.require('size limits authored, or not specified' in all_text and
              sheet['declaration']['revision'] in all_text and
              manifest['recipe_context_hash'][:12] in all_text,
              'visible native scope, revision and source context')
    return rows

def section_svg_geometry(path):
    """Standalone native curves/annotations, excluding source-context metadata."""
    root=ET.fromstring(path.read_text())
    tags={'path','line','polygon','circle','ellipse','text'}
    return [(element.tag.rsplit('}',1)[-1],sorted(element.attrib.items()),element.text)
            for element in root.iter() if element.tag.rsplit('}',1)[-1] in tags]

def drawing_annotation_style(out,expected_scale):
    """Check paper font, short tag, original ID and native value from SVG + reports."""
    sheet=json.loads((out/'drawing-end_cover_machining.json').read_text())
    root=ET.fromstring((out/'drawing-end_cover_machining.svg').read_text())
    ids={element.attrib.get('id'):element for element in root.iter() if element.attrib.get('id')}
    evidence={}
    for view in sheet['views']:
        viewport=view['viewport'];view_id=viewport['view'];scale=viewport['scale']
        d.close(scale,expected_scale[view_id],'authored paper view scale '+view_id)
        d.close(viewport['paper_annotation_font_mm'],2.8,'authored printed font '+view_id)
        nested=ids['drawing-view-'+view_id]
        d.close(float(nested.attrib['data-scale']),scale,'SVG view scale '+view_id)
        d.close(float(nested.attrib['data-paper-annotation-font-mm']),2.8,
                'SVG paper font provenance '+view_id)
        caption=ids['drawing-caption-'+view_id]
        d.require(caption.text==viewport['paper_caption'],'source caption '+view_id)
        d.close(float(caption.attrib['font-size']),2.8,'caption printed font '+view_id)
        report=json.loads((out/view['section_report']).read_text())
        native={r['declaration']['id']:r for r in report['section_view']['circle_dimensions']}
        source_tags={entry['dimension']:entry['tag'] for entry in viewport['annotation_labels']}
        found={}
        for group in nested.iter():
            if group.attrib.get('class') not in {'section-dimension','section-circle-dimension'}:
                continue
            name=group.attrib['data-id'];d.require(name in source_tags and name in native,
                                                   'styled native dimension ID '+name)
            value=float(group.attrib['data-value-mm'])
            d.close(value,native[name]['value_mm'],'styled native value '+name,absolute=1e-8)
            labels=[element for element in group.iter() if element.tag.rsplit('}',1)[-1]=='text']
            d.require(len(labels)==1,'one printed label '+name)
            label=labels[0]
            printed=re.fullmatch(r'([A-Za-z][A-Za-z0-9]*):⌀([0-9.eE+-]+)mm',label.text or '')
            d.require(printed is not None and printed.group(1)==source_tags[name],
                      'short source diameter tag with original ID '+name)
            d.close(float(printed.group(2)),native[name]['value_mm'],
                    'printed native diameter '+name,absolute=1e-5)
            d.require('textLength' not in label.attrib and label.attrib.get('font-family')=='monospace',
                      'uncompressed native paper text '+name)
            d.close(float(label.attrib['font-size'])*scale,2.8,
                    'physical printed label height '+name,absolute=1e-9)
            found[name]={'tag':source_tags[name],'value_mm':value,
                         'paper_font_mm':float(label.attrib['font-size'])*scale}
        d.require(set(found)==set(source_tags),'every and only native dimensions styled '+view_id)
        evidence[view_id]={'scale':scale,'caption':caption.text,'dimensions':found}
    d.require(sum(len(v['dimensions']) for v in evidence.values())==7,
              'seven native short labels across three views')
    return evidence

def annotation_style_variants(cad,directory,baseline):
    """Source-only sheet edits retain exact solids and standalone native curves."""
    original=(d.ROOT/'examples/drive_module_rotation.star').read_text()
    params=dict(shaft_diameter=12,mounting_pitch=60,turn_radians=math.tau,
                base_corner_radius=6,pilot_depth=5,cover_thickness=2)
    expected_scale={'left_cover_pilot_section':.8,'right_cover_pilot_section':.8,
                    'end_cover_section':.8}
    baseline_manifest=json.loads((baseline/'manifest.json').read_text())
    baseline_style=drawing_annotation_style(baseline,expected_scale)
    results={}
    for label,before,after in [
            ('half_scale','"scale":0.8','"scale":0.5'),
            ('short_tag_edit','"tag":"L1"','"tag":"L3"')]:
        d.require(original.count(before)>=1,'source annotation edit target '+label)
        source=directory/(label+'.star');source.write_text(original.replace(before,after,1))
        out=directory/(label+'-generation')
        result=c.run(cad,source,out,params);d.require(result.returncode==0,result.stderr)
        verified=k.command(cad,'verify',out);d.require(verified.returncode==0,verified.stderr)
        manifest=json.loads((out/'manifest.json').read_text())
        d.require(manifest['recipe_source_hash']!=baseline_manifest['recipe_source_hash'] and
                  manifest['recipe_context_hash']!=baseline_manifest['recipe_context_hash'],
                  'annotation edit changes captured source revision '+label)
        d.require(manifest['feature_hashes']==baseline_manifest['feature_hashes'],
                  'annotation edit does not alter feature graph '+label)
        for output,record in baseline_manifest['outputs'].items():
            d.require(manifest['outputs'][output]['content_hash']==record['content_hash'],
                      'identical native output content '+output+' '+label)
        geometry_files=[path.name for path in baseline.iterdir()
                        if path.name.endswith(('.step','.brep.json'))]
        for name in geometry_files:
            d.require(digest(out/name)==digest(baseline/name),
                      'byte-identical STEP/BRep '+name+' '+label)
        for name in ['left_cover_pilot_section','right_cover_pilot_section','end_cover_section']:
            d.require(section_svg_geometry(out/('section-'+name+'.svg'))==
                      section_svg_geometry(baseline/('section-'+name+'.svg')),
                      'standalone native section curves/labels unchanged '+name+' '+label)
        scales=dict(expected_scale)
        if label=='half_scale':scales['left_cover_pilot_section']=.5
        style=drawing_annotation_style(out,scales)
        if label=='short_tag_edit':
            d.require(style['left_cover_pilot_section']['dimensions']['left_cover_minus_diameter']['tag']=='L3' and
                      baseline_style['left_cover_pilot_section']['dimensions']['left_cover_minus_diameter']['tag']=='L1',
                      'short tag source edit visible without measurement drift')
        d.require(digest(out/'drawing-end_cover_machining.svg')!=
                  digest(baseline/'drawing-end_cover_machining.svg'),
                  'drawing revision digest changes with annotation '+label)
        results[label]={'source_sha256':digest(source),'drawing_sha256':digest(out/'drawing-end_cover_machining.svg'),
                        'source_context_hash':manifest['recipe_context_hash'],
                        'unchanged_geometry_file_count':len(geometry_files),'style':style}
    return results

def run_case(cad,directory,diameter,pitch,sign,radius,depth=5,thickness=2):
    tag=f'd{diameter}-p{pitch}-turn{sign}-r{radius}-depth{depth}-cover{thickness}';out=directory/tag
    parameters=dict(shaft_diameter=diameter,mounting_pitch=pitch,
                    turn_radians=sign*math.tau,base_corner_radius=radius,
                    pilot_depth=depth,cover_thickness=thickness)
    result=c.run(cad,d.ROOT/'examples/drive_module_rotation.star',out,parameters)
    d.require(result.returncode==0,result.stderr);manifest=json.loads(result.stdout)
    report=json.loads((out/'rigid-rotation-groups.json').read_text());group=report['groups']['rotor']
    d.require(group['status']=='named_pairs_passed' and group['coverage']=='named_pairs_only','explicit named-pair scope')
    d.require({tuple(member['instance_path']) for member in group['members']}==MOVING,'nine exact moving members')
    d.require(group['assembly_content_hash']==manifest['outputs']['drive_module_release']['content_hash'],'source-bound assembly')
    for actual,wanted in zip(group['rotation_world']['pivot'],[10,0,32]):d.close(actual,wanted,'world pivot')
    for actual,wanted in zip(group['rotation_world']['axis'],[1,0,0]):d.close(actual,wanted,'world axis')
    d.close(group['rotation_world']['angle_radians'],sign*math.tau,'signed full turn')
    native=manifest['outputs']['drive_module_release']
    d.require(native['instances']==30 and native['definitions']==14,
              'thirty placed solids and fourteen shared definitions')
    leaves=c.imported_leaves(out/'drive_module_release.step')
    _,references,part_ids=d.geometries(diameter,pitch)
    additions,new_refs,new_parts,expected_new=cover_references(diameter,pitch,depth,thickness)
    references.update(new_refs);part_ids.update(new_parts)
    expected_paths=set(references)
    d.require(len(references)==30 and len(set(part_ids.values()))==14,
              'independent exact occurrence/definition inventory')
    d.require(set(leaves)==expected_paths,'all thirty STEP occurrence paths')
    expected_quantities={part:list(part_ids.values()).count(part) for part in set(part_ids.values())}
    bom={item['part_id']:item for item in native['bom']['items']}
    d.require(set(bom)==set(expected_quantities) and native['bom']['missing_material_occurrences']==0,
              'complete fourteen-definition material BOM')
    for part,shape in additions.items():
        d.close(k.properties(shape)[0],expected_new[part],'independent '+part+' volume',absolute=3e-6)
        d.require(bom[part]['quantity']==expected_quantities[part],
                  'BOM quantity '+part)
        d.close(bom[part]['unit_volume_mm3'],expected_new[part],
                'native BOM unit volume '+part,absolute=3e-6)
    points=[(0,-40),(pitch+90,-40),(pitch+90,40),(0,40)]
    base=f.filleted(points,6,radius,[0,1,2,3])
    for x in [33,pitch+33]:
        for y in [-28,28]:
            cutter=f.BRepPrimAPI_MakeCylinder(f.gp_Ax2(f.gp_Pnt(x,y,0),f.gp_Dir(0,0,1)),3,6).Shape()
            base=k.boolean(BRepAlgoAPI_Cut,base,cutter)
    references[('base',)]=base
    d.close(k.properties(base)[0],6*(80*(pitch+90)-36*math.pi-(4-math.pi)*radius**2),'rounded base closed-form volume')
    shapes={path:leaf['shape'] for path,leaf in leaves.items()}
    for path,shape in shapes.items():
        d.require(leaves[path]['part']==part_ids[path],'STEP part identity '+str(path))
        for first,second in [(shape,references[path]),(references[path],shape)]:
            d.close(k.properties(k.boolean(BRepAlgoAPI_Cut,first,second))[0],0,'independent authored geometry',absolute=3e-6)
    new_certificates=all_phase_new_pair_certificates(diameter,pitch,depth,thickness)
    physical_faces=cover_axial_faces(shapes,pitch,depth,thickness)
    native_dimensions=native_cover_dimensions(out,manifest,pitch,depth,thickness,physical_faces)
    axial_allowances=axial_size_allowances(shapes,manifest,diameter,pitch,depth,thickness,native_dimensions)
    sheet_rows=machining_sheet_binding(out,manifest,native_dimensions)
    r=diameter/2
    nominal_contacts={}
    for side,sleeve in [('left','sleeve'),('right','right_sleeve')]:
        cover=(side+'_support',side+'_cover')
        flange=(side+'_support',sleeve)
        d.close(c.distance(shapes[cover],shapes[flange]),0,
                'nominal cover/flange plane contact '+side,absolute=2e-6)
        area=math.pi*((r+6)**2-(r+4.5)**2)
        d.require(area>0,'positive analytic annular cover/flange contact '+side)
        nominal_contacts[side+'_cover_flange']={
            'occt_min_distance_mm':0,'analytic_contact_area_mm2':area,
            'basis':'matched coplanar annular faces; distinct exact STEP solids'}
        for position_tag in ['minus','plus']:
            screw=(side+'_support',side+'_cover_screw_'+position_tag)
            d.close(c.distance(shapes[cover],shapes[screw]),0,
                    'nominal screw-head seating '+side+position_tag,absolute=2e-6)
            seat_area=math.pi*(3**2-2.05**2)
            d.require(seat_area>0,'positive analytic screw-head seat')
            nominal_contacts[side+'_screw_'+position_tag+'_seat']={
                'occt_min_distance_mm':0,'analytic_contact_area_mm2':seat_area,
                'basis':'matched coplanar head/clearance annulus; distinct exact STEP solids'}
    expected={'left_support_running_gap':.1,'right_support_running_gap':.15,
              'left_support_disc_gap':pitch/2-16,'right_support_disc_gap':pitch/2-14,'disc_shaft_gap':.15,
              'hub_left_washer_shaft_gap':.1,'hub_right_washer_shaft_gap':.1,'shaft_washer_shaft_gap':.1,
              'hub_left_washer_hub_gap':.1,'hub_right_washer_hub_gap':.1,'shaft_washer_bush_gap':.1,
              **{name:values[1] for name,values in COVER_SHAFT_CHECKS.items()}}
    checks=manifest['clearance_checks'];d.require(set(checks)==set(expected),'thirteen explicit pairs')
    query_count=0;proofs={}
    for name,check in checks.items():
        d.require(check['rotation_group']=='rotor' and check['status']=='passed','group-bound passed check')
        left=tuple(check['left']['instance_path']);right=tuple(check['right']['instance_path'])
        both=left in MOVING and right in MOVING
        d.require(check['lower_bound_mm']<=expected[name]+1e-7 and check['upper_bound_mm']>=expected[name]-1e-7,'whole-path expected minimum')
        d.require(check['lower_bound_mm']>=check['declaration']['min_clearance'],'native lower bound meets requirement')
        if name in COVER_SHAFT_CHECKS:
            fixed_path,_,required=COVER_SHAFT_CHECKS[name]
            d.require(left==fixed_path and right==('shaft',) and not both,
                      'cover-shaft endpoint orientation '+name)
            d.close(check['declaration']['min_clearance'],required,
                    'cover-shaft authored minimum '+name)
            d.require(check['query']['method']=='bore_trim_witness' and
                      check['motion_proof']=='continuous_rotation' and
                      check['moving_endpoint']=='right',
                      'native circular-bore/shaft witness '+name)
            sweep=check['rotation_sweep']
            d.require(sweep['proof_kind']=='bore_witness_orbit' and
                      sweep['query_count']==3 and sweep['intervals']==[],
                      'native full-turn bore witness orbit '+name)
            orbit=sweep['bore_orbit']
            d.require('symmetry' not in sweep and orbit['start']==0 and orbit['end']==1,
                      'explicit complete bore orbit interval '+name)
            d.close(orbit['bore_radius_mm'],diameter/2+4.5,
                    'selected actual cover bore radius '+name)
            d.require(orbit['guarded_radius_mm']>=orbit['bore_radius_mm'] and
                      orbit['shaft_stock_outer_radius_mm']<=diameter/2+2+1e-8 and
                      orbit['axis_offset_mm']<1e-6 and
                      orbit['axis_direction_departure']<1e-6 and
                      orbit['distance_variation_bound_mm']>0 and
                      orbit['lower_bound_mm']<=expected[name]<=orbit['upper_bound_mm'],
                      'finite source-bound bore/stock full-turn geometry '+name)
            d.require(sorted(sample['time'] for sample in sweep['samples'])==[0,.5,1],
                      'three actual bore orbit samples '+name)
            for sample in sweep['samples']:
                moved=posed(shapes[right],sign*math.tau*sample['time'])
                for side,shape in [('left',shapes[left]),('right',moved)]:
                    d.require(boundary_distance(sample['query'][side+'_point'],shape)<2e-6,
                              'native bore witness on exact STEP boundary '+name)
            query_count+=3;proofs[name]='bore_witness_orbit'
            continue
        if both:
            d.require(check['motion_proof']=='common_rigid_motion' and 'rotation_sweep' not in check,'same rigid transform preserves distance')
            query_count+=1;proofs[name]='common_rigid_motion'
        else:
            sweep=check['rotation_sweep'];proof=sweep['symmetry']
            d.require(sweep['proof_kind']=='rotational_symmetry' and sweep['query_count']==3,'native three-query symmetry proof')
            d.require(sweep['intervals']==[] and proof['start']==0 and proof['end']==1,'explicit full-path certificate, no synthetic zero-displacement interval')
            d.require(sweep['speed_bound_mm']>0 and sweep['radius_bound_mm']>0,'physical motion preserved')
            d.require(proof['distance_variation_bound_mm']>0,'arithmetic allowance retained')
            stationary=left if right in MOVING else right;moving=right if right in MOVING else left
            d.require(check['moving_endpoint']==('right' if right in MOVING else 'left'),'authored endpoint orientation')
            d.require(sorted(s['time'] for s in sweep['samples'])==[0,.5,1],'three actual native samples')
            for sample in sweep['samples']:
                moved=posed(shapes[moving],sign*math.tau*sample['time'])
                for side,shape in [('left',shapes[stationary]),('right',moved)]:
                    d.require(boundary_distance(sample['query'][side+'_point'],shape)<2e-6,'native sample witness on actual STEP boundary')
            query_count+=3;proofs[name]='rotational_symmetry'
    d.require(query_count==27,'twenty-seven actual native group queries')
    sampled=[]
    for phase in [0,.125,.25,.375,.5,.625,.75,.875,1]:
        actual={path:posed(shape,sign*math.tau*phase) if path in MOVING else shape for path,shape in shapes.items()}
        distances={}
        for name,check in checks.items():
            left=actual[tuple(check['left']['instance_path'])];right=actual[tuple(check['right']['instance_path'])]
            value=c.distance(left,right);d.close(value,expected[name],'independent rotated distance '+name,absolute=2e-6)
            distances[name]=value
        boxes={path:d.bounds(shape) for path,shape in actual.items()};paths=list(actual);maximum_overlap=0.;pairs=0
        for i,left in enumerate(paths):
            for right in paths[i+1:]:
                volume=0. if d.separated_boxes(boxes[left],boxes[right]) else k.properties(k.boolean(BRepAlgoAPI_Common,actual[left],actual[right]))[0]
                d.close(volume,0,'sampled assembly overlap',absolute=3e-6);maximum_overlap=max(maximum_overlap,volume);pairs+=1
        d.require(pairs==435,'all 30 choose 2 occurrence pairs independently checked')
        sampled.append(dict(phase=phase,distances_mm=distances,pairs=pairs,maximum_overlap_mm3=maximum_overlap))
    for name in ['shaft_axial_endplay','hub_axial_endplay']:
        stack=manifest['tolerance_stacks'][name]
        d.require(stack['measurement_interval_certified'],'retained native endplay obligation')
        d.close(stack['nominal_mm'],.2,'nominal endplay')
    verified=k.command(cad,'verify',out);d.require(verified.returncode==0,verified.stderr)
    replay_source=directory/(tag+'-replay-input')
    replay_source.mkdir()
    captured=replay_source/'drive_module_rotation.star'
    shutil.copyfile(out/'recipe.star',captured)
    replay=directory/(tag+'-captured-replay')
    rebuilt=c.run(cad,captured,replay,parameters)
    d.require(rebuilt.returncode==0,'captured source replay: '+rebuilt.stderr)
    replay_verify=k.command(cad,'verify',replay)
    d.require(replay_verify.returncode==0,'captured source replay verify: '+replay_verify.stderr)
    compared=[]
    for name in ['drive_module_release.step','housing.step','right_housing.step',
                 'end_cover.step','cover_screw_envelope.step',
                 'rigid-rotation-groups.json',
                 'section-left_cover_pilot_section.json',
                 'section-right_cover_pilot_section.json',
                 'section-end_cover_section.json',
                 'datum-dimensions.json','drawing-end_cover_machining.json',
                 'drawing-end_cover_machining.svg']:
        d.require(digest(out/name)==digest(replay/name),'captured replay differs: '+name)
        compared.append(name)
    rebuilt_manifest=json.loads((replay/'manifest.json').read_text())
    for field in ['recipe_source_hash','feature_hashes','outputs','face_assertions',
                  'section_views','clearance_checks','rigid_rotation_groups',
                  'datum_dimensions','drawing_sheets']:
        d.require(manifest[field]==rebuilt_manifest[field],
                  'captured replay semantics differ: '+field)
    return dict(case=tag,parameters=parameters,occurrences=30,definitions=14,
                fixed_rotor_certificates=new_certificates,
                actual_step_axial_faces=physical_faces,
                native_machining_dimensions=native_dimensions,
                conditional_axial_size_allowances=axial_allowances,
                published_machining_rows=sheet_rows,
                nominal_contacts=nominal_contacts,
                actual_native_queries=query_count,proofs=proofs,
                sampled_step_evidence=sampled,
                captured_source_replay={'valid':True,'compared_files':compared})

def rejected_edits(cad,directory):
    source=d.ROOT/'examples/drive_module_rotation.star'
    compiled=k.command(cad,'compile',source);d.require(compiled.returncode==0,compiled.stderr)
    original=json.loads(compiled.stdout);path=directory/'failure-source.json';out=directory/'failure-publication';cache=directory/'cache'
    path.write_text(json.dumps(original))
    def build(extra=()):return k.command(cad,'build',path,'--out-dir',out,'--cache-dir',cache,*extra)
    first=build();d.require(first.returncode==0,first.stderr);baseline=c.inventory(out);failures=[]
    for label in ['offset_axis','missing_reference','excessive_gap',
                  'unknown_table_dimension','wrong_table_output','table_overflow',
                  'stale_floor_datum','pilot_reaches_opposite_cap',
                  'unknown_paper_annotation','impossible_paper_annotation_layout',
                  'shank_band_loses_axial_allowance']:
        changed=json.loads(json.dumps(original))
        if label=='offset_axis':changed['rigid_rotation_groups'][0]['pivot']=[1,0,0]
        elif label=='missing_reference':changed['rigid_rotation_groups'][0]['reference_instance_path']=['base']
        elif label=='excessive_gap':changed['clearance_checks'][0]['min_clearance']=.3
        elif label=='unknown_table_dimension':
            table=next(s for s in changed['drawing_sheets'] if s['id']=='end_cover_machining')['dimension_tables'][0]
            table['rows'][0]['dimension']='missing_native_dimension'
        elif label=='wrong_table_output':
            table=next(s for s in changed['drawing_sheets'] if s['id']=='end_cover_machining')['dimension_tables'][0]
            table['output']='bearing_fit_inspection'
        elif label=='table_overflow':
            table=next(s for s in changed['drawing_sheets'] if s['id']=='end_cover_machining')['dimension_tables'][0]
            table['height']=100
        elif label=='stale_floor_datum':
            datum=next(x for x in changed['datum_dimensions'] if x['id']=='left_cover_minus_depth')
            datum['to']['datum']='axis'
        elif label=='pilot_reaches_opposite_cap':
            feature=next(x for x in changed['features'] if x['id']=='housing_left_pilots')
            feature['pattern_holes'][0]['depth']=16
        elif label=='unknown_paper_annotation':
            sheet=next(x for x in changed['drawing_sheets'] if x['id']=='end_cover_machining')
            sheet['views'][0]['annotation_labels'][0]['dimension']='missing_native_dimension'
        elif label=='shank_band_loses_axial_allowance':
            dimension=next(x for x in changed['datum_dimensions'] if x['id']=='left_cover_minus_shank')
            dimension['tolerance']['upper']=.4
        else:
            sheet=next(x for x in changed['drawing_sheets'] if x['id']=='end_cover_machining')
            sheet['views'][0]['width']=20
        path.write_text(json.dumps(changed));result=build(['--output','sleeve'])
        d.require(result.returncode!=0,'invalid source must fail '+label)
        diagnostic=json.loads(result.stderr)
        d.require(diagnostic.get('code') in ['invalid_rotation_group','rotation_group_failed',
                  'clearance_check_failed','invalid_drawing_sheet','invalid_graph',
                  'datum_dimension_failed','tolerance_stack_failed'] or
                  label in {'pilot_reaches_opposite_cap','impossible_paper_annotation_layout'}
                  and isinstance(diagnostic.get('code'),str),
                  'structured failure '+label)
        d.require(c.inventory(out)==baseline,'invalid source preserves every published byte')
        failures.append(dict(case=label,diagnostic=diagnostic))
    source_text=source.read_text()
    for label,before,after in [
            ('out_of_range_pilot_depth','pilot_depth = param(name = "pilot_depth", default = 5)',
             'pilot_depth = param(name = "pilot_depth", default = 16)'),
            ('pilot_breaks_outer_wall',
             'end_cover_pilots("housing_left_pilots", "start_cap", cap_centroid_y)',
             'end_cover_pilots("housing_left_pilots", "start_cap", cap_centroid_y+30)')]:
        d.require(source_text.count(before)==1,'unique source edit '+label)
        edited=directory/(label+'.star')
        edited.write_text(source_text.replace(before,after))
        result=c.run(cad,edited,out,{})
        d.require(result.returncode!=0,'invalid drilled housing must fail '+label)
        d.require(c.inventory(out)==baseline,'invalid housing preserves every published byte '+label)
        failures.append(dict(case=label,diagnostic=json.loads(result.stderr)))
    return failures

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=d.ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();source=d.ROOT/'examples/drive_module_rotation.star'
    executable_sha256,source_sha256,script_sha256=digest(cad),digest(source),digest(__file__)
    cases=[run_case(cad,args.out_dir,*configuration) for configuration in
           [(12,60,1,6,5,2),
            (14,72,-1,2,4.5,2),
            (10,90,1,10,5,2.5),
            (16,90,1,10,4,1.5),
            (16,90,-1,10,6,3)]]
    annotation_variants=annotation_style_variants(cad,args.out_dir,
                                                   args.out_dir/cases[0]['case'])
    failures=rejected_edits(cad,args.out_dir)
    d.require(digest(cad)==executable_sha256 and digest(source)==source_sha256
              and digest(__file__)==script_sha256,
              'pinned executable/source/oracle changed during oracle run')
    report=dict(valid=True,executable_sha256=executable_sha256,
                source_sha256=source_sha256,oracle_script_sha256=script_sha256,
                python_version=platform.python_version(),
                ocp_distribution=importlib.metadata.version('cadquery-ocp'),
                cases=cases,annotation_variants=annotation_variants,rejected_edits=failures,
                scope='Native continuous named-pair proofs, conservative full-turn bounds for 54 fixed/rotor pairs, independent STEP face measurements bound to fourteen drawing rows, and four authored independent-interval axial allowances conditional on nominal seating. No general assembly collision, thread, preload, installation or load certification')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))

if __name__=='__main__':main()
