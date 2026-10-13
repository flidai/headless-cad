#!/usr/bin/env python3
"""Independent OCCT acceptance of the optional drive-base relief pockets."""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import shutil

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT/'examples/drive_module_rotation.star'
# Frozen original drive base STEP, built from the pre-relief source with the
# same release exporter. This pins the zero-depth geometry exchange exactly.
LEGACY_BASE_STEP_SHA256 = '0a5593aea2fda2953025ed3afbed5abd5c495d7b2841efcd219b4df8c1e9330e'
spec = importlib.util.spec_from_file_location('mixed_oracle', ROOT/'tools/check-mixed-slots.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
o, b, k = m.o, m.b, m.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut

LEGACY = {
    'face_assertions': 'base_back_right_survives base_body_survives base_front_left_survives base_front_right_survives left_cover_minus_floor left_cover_minus_wall left_cover_plus_floor left_cover_plus_wall left_housing_bore_survives left_outboard_cap_survives right_cover_minus_floor right_cover_minus_wall right_cover_plus_floor right_cover_plus_wall right_housing_bore_survives right_outboard_cap_survives',
    'property_assertions': 'cover_screw_envelope_volume end_cover_volume left_housing_machined_volume mass_budget material_mass material_volume right_housing_machined_volume',
    'clearance_checks': 'disc_shaft_gap hub_left_washer_hub_gap hub_left_washer_shaft_gap hub_right_washer_hub_gap hub_right_washer_shaft_gap left_cover_shaft_gap left_support_disc_gap left_support_running_gap right_cover_shaft_gap right_support_disc_gap right_support_running_gap shaft_washer_bush_gap shaft_washer_shaft_gap',
    'nominal_seating_checks': 'left_cover_flange_seat left_cover_minus_head_seat left_cover_plus_head_seat right_cover_flange_seat right_cover_minus_head_seat right_cover_plus_head_seat',
    'datum_dimensions': 'bush_outer_span hub_left_ring_thickness hub_left_washer_thickness hub_right_ring_thickness hub_right_washer_thickness hub_stop_span hub_width left_cover_minus_depth left_cover_minus_remaining_floor left_cover_minus_shank left_cover_plus_depth left_cover_plus_remaining_floor left_cover_plus_shank left_cover_thickness left_flange_thickness right_cover_minus_depth right_cover_minus_remaining_floor right_cover_minus_shank right_cover_plus_depth right_cover_plus_remaining_floor right_cover_plus_shank right_cover_thickness right_flange_thickness shaft_ring_thickness shaft_stop_span shaft_washer_thickness',
    'section_views': 'bearing_section end_cover_section left_cover_pilot_section mounting_plan right_cover_pilot_section',
    'drawing_sheets': 'drive_module_sections end_cover_machining',
    'tolerance_stacks': 'housing_sleeve_diametral_limits hub_axial_endplay left_cover_minus_tip_allowance left_cover_plus_tip_allowance right_cover_minus_tip_allowance right_cover_plus_tip_allowance running_diametral_limits shaft_axial_endplay',
    'rigid_rotation_groups': 'rotor',
}
LEGACY = {key: set(value.split()) for key,value in LEGACY.items()}

def require(ok, message):
    if not ok: raise AssertionError(message)

def close(actual, expected, message, tolerance=2e-6):
    b.close(actual, expected, message, tolerance)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def inputs():
    paths=list((ROOT/'tools').glob('*.py'))+list((ROOT/'crates/cad-export/tools').glob('*.py'))
    for name in ('drive_module_rotation','drive_module_slot_positions',
                 'drive_module_slot_drawing','drive_module_adjustment',
                 'drive_module_tightened'):
        paths.append(ROOT/'examples'/f'{name}.star')
        lock=ROOT/'examples'/f'{name}.star.lock.json'
        if lock.exists():paths.append(lock)
    return {str(path.relative_to(ROOT)):sha(path) for path in sorted(paths)}

def slots(pitch,length,depth):
    through=[dict(id=name,center=[x,y],direction=[1,0],centerline_length=length,radius=3.)
             for name,x,y in [('front_minus',33,-28),('front_plus',33,28),
                              ('rear_minus',pitch+33,-28),('rear_plus',pitch+33,28)]]
    blind=[] if depth==0 else [dict(id=name,center=[pitch/2+x,0],direction=[0,1],
                                   centerline_length=18.,radius=3.,depth=depth+extra)
                               for name,x,extra in [('shallow',23,0),('deep',43,1)]]
    return through,blind

def exact(pitch,length,corner,depth):
    mass,center,inertia=m.stock_moments(pitch+90,80.,6.,corner,0.)
    center[0]=(pitch+90)/2
    through,blind=slots(pitch,length,depth)
    return m.subtract_capsules(m.subtract_capsules((mass,center,inertia),through,6.,'start_cap'),blind,6.,'end_cap')

def reference(pitch,length,corner,depth):
    width=pitch+90
    solid=o.f.filleted([(0,-40),(width,-40),(width,40),(0,40)],6.,corner,[0,1,2,3])
    through,blind=slots(pitch,length,depth)
    for slot in through: solid=m.cut_capsule(solid,slot,-1.5,9.)
    for slot in blind: solid=m.cut_capsule(solid,slot,6-slot['depth'],slot['depth']+1.5)
    return solid

def params(pitch,length,corner,depth):
    return [word for key,val in [('mounting_pitch',pitch),('base_slot_length',length),
                                ('base_corner_radius',corner),('base_pocket_depth',depth)]
            for word in ('--param',f'{key}={val}')]

def obligations(manifest,depth):
    inventory={}
    for category,baseline in LEGACY.items():
        reports=manifest[category]
        require(baseline<=set(reports),f'original {category} obligation missing')
        for identifier in baseline:
            require(reports[identifier]['status'] in
                    ('passed','resolved','published','named_pairs_passed'),
                    f'original {category}/{identifier} failed')
        inventory[category]=sorted(reports)
    if depth>0:
        additions={'face_assertions':{'base_shallow_floor_survives'}|
                   {f'base_{name}_survives' for name in ('front_minus','front_plus','rear_minus','rear_plus')},
                   'datum_dimensions':{'base_shallow_depth','base_deep_depth','base_remaining_web'},
                   'section_views':{'base_relief_section','base_web_section'},
                   'drawing_sheets':{'base_relief_machining'}}
        for category,ids in additions.items():
            require(ids<=set(manifest[category]),f'new {category} obligation missing')
            require(all(manifest[category][identifier]['status'] in ('passed','resolved','published')
                        for identifier in ids),f'new {category} obligation failed')
    return inventory

def native_witnesses(manifest,output,depth):
    if depth==0:
        require(manifest['outputs']['base']['feature_id']=='base_slotted',
                'zero-depth base aliases original through-slot feature')
        require(manifest['outputs']['base']['content_hash']==manifest['feature_hashes']['base_slotted'],
                'zero-depth base has original native geometry hash')
        return None
    values={'base_shallow_depth':depth,'base_deep_depth':depth+1,
            'base_remaining_web':5-depth}
    for identifier,value in values.items():
        report=manifest['datum_dimensions'][identifier]
        close(report['value_mm'],value,identifier+' native floor depth')
        require(report['from']['part_id']==report['to']['part_id']=='base',
                identifier+' bound to actual base part')
        require(report['from']['instance_path']==report['to']['instance_path']==['base'],
                identifier+' bound to exact base occurrence')
    first=manifest['face_assertions']['base_shallow_floor_survives']
    require(first['status']=='resolved' and len(first['witnesses'])==1,
            'first native floor survives second relief')
    witness=first['witnesses'][0]
    require(witness['anchor_faces'][0]!=witness['target_face'],
            'first floor native FaceId remapped by second relief')
    for name in ('front_minus','front_plus','rear_minus','rear_plus'):
        report=manifest['face_assertions'][f'base_{name}_survives']
        require(report['status']=='resolved' and len(report['witnesses'])==4,
                'original four native through-slot walls survive')
        require(all(w['anchor_faces']==[w['target_face']] for w in report['witnesses']),
                'through-slot wall native FaceIds remain unchanged')
    drawing=(output/'drawing-base_relief_machining.svg').read_text()
    for identifier,value in values.items():
        require(f'data-dimension="{identifier}"' in drawing and f'{value:.3f} mm' in drawing,
                'native depth drawing table prints measured floor value')
    return {'depths_mm':values,'first_floor_witness':witness}

def check_case(cad,root,tag,pitch,depth,length=6.,corner=6.):
    out=root/tag;arguments=params(pitch,length,corner,depth)
    manifest=o.command(cad,'build',SOURCE,'--out-dir',out,*arguments)
    o.command(cad,'verify',out)
    if pitch==60 and length==6 and corner==6 and depth==0:
        require(sha(out/'base.step')==LEGACY_BASE_STEP_SHA256,
                'zero-depth STEP byte-identical to frozen pre-relief drive base')
    inventory=obligations(manifest,depth)
    witnesses=native_witnesses(manifest,out,depth)
    expected=exact(pitch,length,corner,depth)
    independent=reference(pitch,length,corner,depth)
    imported=k.load(str(out/'base.step'))
    native=o.command(cad,'properties',SOURCE,'--output','base',*arguments)
    for label,properties in [('OCCT',k.properties(independent)),('STEP',k.properties(imported)),
                             ('native',(native['volume'],native['volume_centroid'],
                                        native['central_volume_inertia']))]:
        b.compare_moments(properties,expected,tag+' '+label)
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,z))[0]
                 for a,z in ((independent,imported),(imported,independent))]
    for value in differences:close(value,0,'bidirectional exact solid difference')
    through,blind=slots(pitch,length,depth)
    topology=m.actual_topology(imported,{'pose':0,'cap':'end_cap'},through,blind,6.)
    stock_area=(pitch+90)*80-(4-math.pi)*corner*corner
    strata=m.section_strata(cad,SOURCE,'base',{'pose':0,'cap':'end_cap'},through,blind,
                            6.,stock_area,base_loops=1,parameters=arguments)
    k.mesh(out/'base.stl',expected[0])
    if depth>0:
        for identifier,z,count in [('base_relief_section',5.75,2),('base_web_section',3.,0)]:
            active=sum(slot['depth']>6-z for slot in blind)
            require(active==count,identifier+' authored plane lies in intended pocket stratum')
            section=manifest['section_views'][identifier]
            area=stock_area-sum(2*s['radius']*s['centerline_length']+math.pi*s['radius']**2
                                for s in through+blind[:count])
            close(section['query']['section']['area'],area,
                  identifier+' authored native section area',2e-5)
    return {'case':tag,'parameters':{'mounting_pitch':pitch,'base_slot_length':length,
            'base_corner_radius':corner,'base_pocket_depth':depth},
            'volume_mm3':expected[0],'centroid_mm':expected[1],
            'central_inertia_mm5':expected[2],
            'directed_difference_mm3':differences,'topology':topology,
            'section_strata':strata,'native_witnesses':witnesses,
            'obligations':inventory,'base_content_hash':manifest['outputs']['base']['content_hash'],
            'assembly_material_volume_mm3':manifest['property_assertions']['material_volume']['actual'],
            'assembly_material_mass_kg':manifest['property_assertions']['material_mass']['actual'],
            'output_hashes':{name:value['content_hash'] for name,value in manifest['outputs'].items()},
            'feature_hashes':manifest['feature_hashes']}

def compare_depth_revisions(cases):
    baseline=cases[0]
    for case in cases[1:4]:
        for name,value in baseline['output_hashes'].items():
            if name in ('base','drive_module_release','bearing_fit_inspection'):continue
            require(case['output_hashes'][name]==value,
                    name+' geometry changed from base-pocket depth alone')
        for category,ids in baseline['obligations'].items():
            require(set(ids)<=set(case['obligations'][category]),
                    category+' engineering obligation narrowed')
        require(case['feature_hashes']['base_slotted']==baseline['feature_hashes']['base_slotted'],
                'base pocket edit changed original through-slot geometry')
        removed=baseline['volume_mm3']-case['volume_mm3']
        close(baseline['assembly_material_volume_mm3']-
              case['assembly_material_volume_mm3'],removed,
              'assembly material volume changes by exactly native removed base volume')
        close(baseline['assembly_material_mass_kg']-
              case['assembly_material_mass_kg'],removed*2700/1e9,
              'assembly material mass changes by aluminum density times removed volume',1e-9)

def imported_finished_base(cad,root,case):
    expected=case['base_content_hash'];checks={}
    for name in ('drive_module_slot_positions','drive_module_slot_drawing'):
        source=ROOT/'examples'/f'{name}.star';out=root/name
        manifest=o.command(cad,'build',source,'--out-dir',out,
                           '--param','base_pocket_depth=1')
        o.command(cad,'verify',out)
        require(manifest['outputs']['base']['content_hash']==expected,
                name+' imports actual finished pocketed base')
        require((out/'base.step').read_bytes()==(root/case['case']/'base.step').read_bytes(),
                name+' finished base STEP byte-identical to engineering drive')
        if name.endswith('slot_positions'):
            sections=('slot_positions','slot_pitches')
            values={'front_x':33,'rear_x':93,'lower_y':12,'upper_y':68,
                    'pitch_x':60,'pitch_y':56}
            reports={d['declaration']['id']:d for view in sections
                     for d in manifest['section_views'][view]['dimensions']}
            require(set(reports)==set(values),'all six actual-base slot positions retained')
            for identifier,value in values.items():
                report=reports[identifier]
                close(report['value_mm'],value,identifier+' imported native position')
                for side in ('from_witness','to_witness'):
                    require(report[side]['part_content_hash']==expected,
                            identifier+' position witness targets finished base')
            drawing='base_slot_positions'
        else:
            report=manifest['section_views']['base_slot_plan']
            require(report['query']['content_hash']==expected,
                    'slot-size section query targets actual finished base')
            sizes=report['slot_dimensions']
            require(len(sizes)==12,'all twelve native through-slot sizes retained')
            for size in sizes:
                require(size['target_content_hash']==expected and size['status']=='passed',
                        'through-slot size report targets actual finished base')
            drawing='base_slot_machining'
        require(manifest['drawing_sheets'][drawing]['status']=='published',
                name+' actual-base drawing published')
        checks[name]={'base_content_hash':expected,'drawing':drawing}
    return checks

def replay(cad,root,case):
    original=root/case['case'];captured=root/(case['case']+'-captured');captured.mkdir()
    recipe=captured/SOURCE.name;shutil.copyfile(original/'recipe.star',recipe)
    require(sha(recipe)==sha(SOURCE),'captured source byte-identical to authored source')
    again=root/(case['case']+'-replay');p=case['parameters']
    repeated=o.command(cad,'build',recipe,'--out-dir',again,
                       *params(p['mounting_pitch'],p['base_slot_length'],
                               p['base_corner_radius'],p['base_pocket_depth']))
    for artifact in ('base.step','base.stl','base.brep.json','drive_module_release.step'):
        require((original/artifact).read_bytes()==(again/artifact).read_bytes(),
                case['case']+' exact captured replay '+artifact)
    require(repeated['feature_hashes']==case['feature_hashes'],
            'captured replay native feature hashes')

def refusals(cad,root):
    recipe=root/'revisions.star';out=root/'preserved';cache=root/'cache'
    source=SOURCE.read_text();recipe.write_text(source)
    o.command(cad,'build',recipe,'--out-dir',out,'--cache-dir',cache,*params(60,6,6,1.))
    stable=o.inventory(out);failures=[]
    mutations=[('low_depth',source,params(60,6,6,.25)),
               ('high_depth',source,params(60,6,6,1.75)),
               ('wrong_cap',source.replace('cap = "end_cap", depth = base_pocket_depth,',
                                           'cap = "start_cap", depth = base_pocket_depth,'),params(60,6,6,1.)),
               ('through_overlap',source.replace('"center":[p/2+23,0]',
                                                 '"center":[33,-28]'),params(60,6,6,1.)),
               ('impossible_web',source.replace(
                   'min = 5-base_pocket_depth-0.000001, max = 5-base_pocket_depth+0.000001',
                   'min = 4-base_pocket_depth-0.000001, max = 4-base_pocket_depth+0.000001'),
                params(60,6,6,1.))]
    for tag,modified,arguments in mutations:
        require(modified!=source or tag in ('low_depth','high_depth'),
                tag+' refusal mutation applied')
        recipe.write_text(modified)
        failure=o.command(cad,'build',recipe,'--out-dir',out,'--cache-dir',cache,
                          *arguments,success=False)
        require(o.inventory(out)==stable,tag+' refusal preserved complete publication')
        failures.append({'case':tag,'diagnostic':failure})
    recipe.write_text(source);o.command(cad,'verify',out)
    for name in ('drive_module_adjustment','drive_module_tightened'):
        authored=ROOT/'examples'/f'{name}.star';published=root/(name+'-preserved')
        o.command(cad,'build',authored,'--out-dir',published,
                  '--param','base_pocket_depth=0')
        unchanged=o.inventory(published)
        failure=o.command(cad,'build',authored,'--out-dir',published,
                          '--param','base_pocket_depth=1',success=False)
        require(failure['code']=='clearance_check_failed',
                name+' refuses currently unsupported pocketed motion or seating')
        require(o.inventory(published)==unchanged,
                name+' pocketed refusal preserves full earlier publication')
        o.command(cad,'verify',published)
        failures.append({'case':name+'_pocketed','diagnostic':failure})
    return failures

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    binary=sha(args.cad);script=sha(__file__);sources=inputs()
    cases=[check_case(args.cad,args.out_dir,tag,pitch,depth)
           for tag,pitch,depth in [('legacy_zero',60.,0.),('relief_half',60.,.5),
                                   ('relief_one',60.,1.),('relief_one_half',60.,1.5),
                                   ('wide_pitch',90.,1.)]]
    compare_depth_revisions(cases)
    imports=imported_finished_base(args.cad,args.out_dir,cases[2])
    for case in (cases[0],cases[2]):replay(args.cad,args.out_dir,case)
    failed=refusals(args.cad,args.out_dir)
    require(binary==sha(args.cad) and script==sha(__file__) and sources==inputs(),
            'binary, oracle and source inventory unchanged during run')
    report={'valid':True,'binary_sha256':binary,'oracle_script_sha256':script,
            'legacy_base_step_sha256':LEGACY_BASE_STEP_SHA256,
            'source_inventory_sha256':sources,'python':platform.python_version(),
            'ocp':importlib.metadata.version('cadquery-ocp'),
            'cases':cases,'imported_finished_base':imports,'refusals':failed,
            'scope':'Actual drive base with four full-height mounting slots and optional two serial end-cap relief pockets; independent OCCT Boolean, closed-form moments, native floor witnesses and retained assembly obligations.'}
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'valid':True,'cases':len(cases),'refusals':len(failed),
                      'report':str(args.out_dir/'report.json')}))

if __name__=='__main__':main()
