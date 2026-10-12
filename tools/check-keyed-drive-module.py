#!/usr/bin/env python3
"""Independent keyed drive assembly geometry and nominal engagement acceptance."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys
import importlib.util
def oracle(name,file):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file));value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
keyed=oracle("keyed_coupling_oracle","check-keyed-coupling.py")
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'crates/cad-export/tools'))
from check_constrained_fixture_ocp import body_properties
from check_assembly_ocp import require


def expected(diameter,pitch):
    r=diameter/2
    parts={
        'base':(1,6*(80*(pitch+90)-36*math.pi),2700),
        'housing':(2,16*(2048-math.pi*(r+4.025)**2),2700),
        'sleeve':(2,16*math.pi*((r+4)**2-(r+.15)**2),8800),
        'shaft':(1,keyed.k.expected(r,pitch+70,r-2,2.02,15+pitch/2,28.5+pitch/2)[0],7800),
        'drive_disc':(1,keyed.h.expected(24,r+.15,14,r+2.05,2.02)[0],7800),
        'parallel_key':(1,4*4*13,7800),
        'spacer':(2,5*math.pi*((r+4)**2-(r+.1)**2),7800),
        'clamp_bar':(2,6*(12*68-2*math.pi*3**2),2700),
        'clamp_bolt_envelope':(4,math.pi*(2.5**2*65+4.5**2*3),7800),
        'nut_envelope':(4,5*math.pi*(5**2-2.6**2),7800),
    }
    return dict(volume_mm3=sum(count*v for count,v,_ in parts.values()),mass_kg=sum(count*v*density/1e9 for count,v,density in parts.values()),parts=parts)


def close(actual,expected,name,relative=1e-9):
    require(math.isfinite(actual) and abs(actual-expected)<=max(1e-8,abs(expected)*relative),f'{name}: {actual} != {expected}')


def case(cad,directory,diameter,pitch):
    output=directory/f'd{diameter}-p{pitch}'
    result=subprocess.run([str(cad),'build',str(ROOT/'examples/drive_module_keyed.star'),'--out-dir',str(output),'--param',f'shaft_diameter={diameter}','--param',f'mounting_pitch={pitch}'],capture_output=True,text=True)
    require(result.returncode==0,'build failed: '+result.stderr)
    manifest=json.loads((output/'manifest.json').read_text())
    assemblies=[(name,o) for name,o in manifest['outputs'].items() if 'bom' in o and name=='drive_module_release']
    require(len(assemblies)==1,'one final module assembly required')
    name,native=assemblies[0];expect=expected(diameter,pitch)
    require(native['instances']==20 and native['definitions']==10,'20 occurrences and10definitions')
    close(native['volume_mm3'],expect['volume_mm3'],'native material volume')
    close(native['bom']['total_mass_kg'],expect['mass_kg'],'native total mass')
    require(native['bom']['missing_material_occurrences']==0,'complete source material accounting')
    observed={item['part_id']:item for item in native['bom']['items']}
    require(set(observed)==set(expect['parts']),'expected part identities')
    for part,(count,volume,density) in expect['parts'].items():
        item=observed[part];require(item['quantity']==count,'part count '+part)
        close(item['unit_volume_mm3'],volume,part+' volume')
        close(item['unit_mass_kg'],volume*density/1e9,part+' mass')
    section=json.loads((output/'section-bearing_section.json').read_text())['section_view']
    circles={d['declaration']['id']:d for d in section['circle_dimensions']}
    for key,value in {'shaft_diameter':diameter,'sleeve_bore':diameter+.3,'sleeve_outer':diameter+8,'housing_bore':diameter+8.05}.items():
        require(circles[key]['status']=='passed','circle assertion '+key)
        close(circles[key]['value_mm'],value,'native circle '+key)
    plan=json.loads((output/'section-mounting_plan.json').read_text())['section_view']
    spacing=next(d for d in plan['dimensions'] if d['declaration']['id']=='support_pitch')
    close(spacing['value_mm'],pitch,'datum mounting pitch')
    clearance=manifest['clearance_checks']
    for key,value in {'left_support_running_gap':.15,'right_support_running_gap':.15,'disc_shaft_gap':.15,'left_spacer_shaft_gap':.1,'right_spacer_shaft_gap':.1,'left_support_disc_gap':pitch/2-16,'right_support_disc_gap':pitch/2-14}.items():
        check=clearance[key]
        require(check['lower_bound_mm']<=value<=check['upper_bound_mm'],'independent exact nominal clearance '+key)
    imported=body_properties(output/f'{name}.step')
    close(imported[0],expect['volume_mm3'],'independent STEP volume',1e-8)
    hierarchy=subprocess.run([sys.executable,str(ROOT/'crates/cad-export/tools/check_assembly_ocp.py'),str(output/f'{name}.step'),'--instances','20','--definitions','10','--solids','20','--assembly-json',str(output/f'{name}.assembly.json')],capture_output=True,text=True)
    require(hierarchy.returncode==0,'independent assembly hierarchy: '+hierarchy.stderr)
    engagement=check_engagement(output,diameter,pitch)
    verified=subprocess.run([str(cad),'verify',str(output)],capture_output=True,text=True)
    require(verified.returncode==0 and json.loads(verified.stdout)['valid'],'generation integrity')
    return dict(engagement=engagement,parameters=dict(shaft_diameter=diameter,mounting_pitch=pitch),expected=expect,imported_volume_mm3=imported[0],imported_centroid_mm=imported[1],hierarchy=json.loads(hierarchy.stdout),native_volume_mm3=native['volume_mm3'],native_mass_kg=native['bom']['total_mass_kg'],output=str(output))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cases=[case(args.cad,args.out_dir,d,p) for d,p in [(12,60),(14,72)]]
    report=dict(valid=True,cases=cases,scope='independent nominal component solids and source-density mass; not manufactured-part certification')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


def check_engagement(output,diameter,pitch):
    from OCP.gp import gp_Trsf,gp_Quaternion,gp_Vec,gp_Pnt
    from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
    from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder,BRepPrimAPI_MakeBox
    from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut,BRepAlgoAPI_Common
    r=diameter/2;floor=r-2;start=15+pitch/2
    stock=BRepPrimAPI_MakeCylinder(r,pitch+70).Shape()
    shaft=keyed.k.boolean(BRepAlgoAPI_Cut,stock,BRepPrimAPI_MakeBox(gp_Pnt(floor,-2.02,start),4,4.04,13.5).Shape())
    tube=keyed.k.boolean(BRepAlgoAPI_Cut,BRepPrimAPI_MakeCylinder(24,14).Shape(),BRepPrimAPI_MakeCylinder(r+.15,14).Shape())
    disc=keyed.k.boolean(BRepAlgoAPI_Cut,tube,BRepPrimAPI_MakeBox(gp_Pnt(0,-2.02,-1),r+2.05,4.04,16).Shape())
    key=BRepPrimAPI_MakeBox(4,4,13).Shape()
    def pose(shape,offset):
        tr=gp_Trsf();tr.SetRotation(gp_Quaternion(.5,.5,.5,.5));tr.SetTranslationPart(gp_Vec(*offset));return BRepBuilderAPI_Transform(shape,tr,True).Shape()
    expected={'shaft':pose(shaft,[10,0,32]),'drive_disc':pose(disc,[25+pitch/2,0,32]),'parallel_key':pose(key,[25+pitch/2+.25,floor,30])}
    leaves=keyed.convex.imported_leaves(output/'drive_module_release.step');actual={name:leaves[(name,)]['shape'] for name in expected}
    differences={}
    for name,shape in expected.items():
        differences[name]=[keyed.k.properties(keyed.k.boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(shape,actual[name]),(actual[name],shape)]]
        for value in differences[name]:close(value,0,'actual keyed solid '+name)
    overlaps={}
    for a,b in [('shaft','drive_disc'),('shaft','parallel_key'),('drive_disc','parallel_key')]:
        value=keyed.k.properties(keyed.k.boolean(BRepAlgoAPI_Common,actual[a],actual[b]))[0];close(value,0,'engagement overlap');overlaps[a+'-'+b]=value
    # The common quaternion maps local radial X→worldY and tangentY→worldZ.
    gaps={'roof':keyed.convex.distance(keyed.plane(actual['drive_disc'],1,r+2.05),keyed.plane(actual['parallel_key'],1,r+2)),
          'positive_side':keyed.convex.distance(keyed.plane(actual['drive_disc'],2,34.02),keyed.plane(actual['parallel_key'],2,34)),
          'negative_side':keyed.convex.distance(keyed.plane(actual['drive_disc'],2,29.98),keyed.plane(actual['parallel_key'],2,30)),
          'floor_contact':keyed.convex.distance(keyed.plane(actual['shaft'],1,floor),keyed.plane(actual['parallel_key'],1,floor))}
    for name,value in gaps.items():close(value,.05 if name=='roof' else (0 if name=='floor_contact' else .02),'physical face '+name)
    return dict(directed_difference_mm3=differences,pairwise_overlap_mm3=overlaps,physical_face_distances_mm=gaps)


if __name__=='__main__':main()
