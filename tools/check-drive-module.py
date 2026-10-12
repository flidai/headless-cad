#!/usr/bin/env python3
"""Independent dimensional/volume/mass and STEP acceptance of a drive module."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys
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
        'shaft':(1,math.pi*r*r*(pitch+70),7800),
        'drive_disc_blank':(1,14*math.pi*(576-(r+.15)**2),7800),
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
    result=subprocess.run([str(cad),'build',str(ROOT/'examples/drive_module_release.star'),'--out-dir',str(output),'--param',f'shaft_diameter={diameter}','--param',f'mounting_pitch={pitch}'],capture_output=True,text=True)
    require(result.returncode==0,'build failed: '+result.stderr)
    manifest=json.loads((output/'manifest.json').read_text())
    assemblies=[(name,o) for name,o in manifest['outputs'].items() if 'bom' in o and name=='drive_module_release']
    require(len(assemblies)==1,'one final module assembly required')
    name,native=assemblies[0];expect=expected(diameter,pitch)
    require(native['instances']==19 and native['definitions']==9,'19 occurrences and9definitions')
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
    hierarchy=subprocess.run([sys.executable,str(ROOT/'crates/cad-export/tools/check_assembly_ocp.py'),str(output/f'{name}.step'),'--instances','19','--definitions','9','--solids','19','--assembly-json',str(output/f'{name}.assembly.json')],capture_output=True,text=True)
    require(hierarchy.returncode==0,'independent assembly hierarchy: '+hierarchy.stderr)
    verified=subprocess.run([str(cad),'verify',str(output)],capture_output=True,text=True)
    require(verified.returncode==0 and json.loads(verified.stdout)['valid'],'generation integrity')
    return dict(parameters=dict(shaft_diameter=diameter,mounting_pitch=pitch),expected=expect,imported_volume_mm3=imported[0],imported_centroid_mm=imported[1],hierarchy=json.loads(hierarchy.stdout),native_volume_mm3=native['volume_mm3'],native_mass_kg=native['bom']['total_mass_kg'],output=str(output))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad');parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cases=[case(args.cad,args.out_dir,d,p) for d,p in [(12,60),(14,72)]]
    report=dict(valid=True,cases=cases,scope='independent nominal component solids and source-density mass; not manufactured-part certification')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n');print(json.dumps(report,indent=2,allow_nan=False))


if __name__=='__main__':main()
