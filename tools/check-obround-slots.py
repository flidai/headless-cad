#!/usr/bin/env python3
"""Independent OCCT acceptance for bounded native through obround slots.

The reference uses an OCCT edge fillet and three OCCT cutters per capsule.
Closed form mass moments are derived from the capsule area, independently of
the native boundary representation. OCP is a test dependency only.
"""

import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


f = module('obround_fillet_oracle', 'check-longitudinal-fillet.py')
k = f.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut  # noqa: E402
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform  # noqa: E402
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Surface  # noqa: E402
from OCP.BRepGProp import BRepGProp  # noqa: E402
from OCP.GProp import GProp_GProps  # noqa: E402
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane  # noqa: E402
from OCP.gp import gp_Ax2, gp_Dir, gp_Pnt  # noqa: E402
from OCP.TopAbs import TopAbs_FACE  # noqa: E402
from OCP.TopExp import TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def close(actual, expected, label, absolute=2e-6):
    require(math.isfinite(actual) and abs(actual-expected) <= max(absolute, abs(expected)*2e-12),
            f'{label}: {actual} differs from {expected}')


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inventory(directory):
    return {p.relative_to(directory).as_posix(): sha256(p)
            for p in directory.rglob('*') if p.is_file()}


def semantic_manifest(directory):
    manifest=json.loads((directory/'manifest.json').read_text())
    return ({key:manifest[key] for key in ('kernel_build_id','executor_build_id',
            'recipe_source_hash','recipe_context_hash','feature_hashes')},
            manifest['outputs']['finished']['content_hash'])


def command(cad, *args, success=True):
    process = subprocess.run([str(cad), *map(str, args)], capture_output=True,
                             text=True, timeout=180)
    require((process.returncode == 0) == success,
            f'cad {args[0]} exit={process.returncode}: {process.stderr[:1200]}')
    return json.loads(process.stdout if process.returncode == 0 else process.stderr)


def parameters(scale=1, pose=0, slots=4, length=6, radius=2,
               variant='standard', posed_input=False):
    return dict(scale=scale, pose=pose, slots=slots, length=length,
                radius=radius, variant=variant, posed_input=posed_input)


def geometry(p):
    scale = p['scale']
    w, d, h, corner = (50*scale, 36*scale, 8*scale, 4*scale)
    centers = [(-14*scale, -8*scale), (14*scale, -8*scale),
               (-14*scale, 8*scale), (14*scale, 8*scale)][:p['slots']]
    # Two axis directions exercise placement and cap-wall orientation.
    axes = [(1, 0), (0, 1), (0, 1), (1, 0)][:p['slots']]
    slots = [dict(id=f'slot_{i+1}', center=[x, y], direction=list(axis),
                  centerline_length=p['length']*scale, radius=p['radius']*scale)
             for i, ((x, y), axis) in enumerate(zip(centers, axes))]
    if p['variant'] == 'outer_clearance':
        slots[0]['center']=[-w/2+slots[0]['centerline_length']/2+slots[0]['radius']+.1*scale,0]
    elif p['variant'] == 'circle_clearance':
        slots[0]['center']=[-3*scale-slots[0]['radius']-slots[0]['centerline_length']/2-.1*scale,0]
    elif p['variant'] == 'slot_clearance':
        require(len(slots)>=2,'slot clearance requires two slots')
        slots[1]['center']=[slots[0]['center'][0]+slots[0]['centerline_length']+
                            2*slots[0]['radius']+.1*scale,slots[0]['center'][1]]
        slots[1]['direction']=slots[0]['direction'][:]
    return w, d, h, corner, slots


def recipe(p, invalid=None):
    w, d, h, corner, slots = geometry(p)
    bore = 3*p['scale']
    if invalid == 'outer_contact':
        slots[0]['center'] = [-w/2+slots[0]['centerline_length']/2+slots[0]['radius'], 0]
    elif invalid == 'near_outer_penetration':
        slots[0]['center'] = [-w/2+slots[0]['centerline_length']/2+slots[0]['radius']-.05*p['scale'], 0]
    elif invalid == 'slot_contact':
        slots[1]['center'] = [slots[0]['center'][0]+slots[0]['centerline_length']+2*slots[0]['radius'], slots[0]['center'][1]]
        slots[1]['direction'] = slots[0]['direction']
    elif invalid == 'slot_overlap':
        slots[1]['center'] = [slots[0]['center'][0]+slots[0]['centerline_length']+2*slots[0]['radius']-.05*p['scale'], slots[0]['center'][1]]
        slots[1]['direction'] = slots[0]['direction']
    elif invalid == 'circle_contact':
        slots[0]['center'] = [-bore-slots[0]['radius']-slots[0]['centerline_length']/2,0]
    elif invalid == 'circle_overlap':
        slots[0]['center'] = [-bore-slots[0]['radius']-slots[0]['centerline_length']/2+.05*p['scale'],0]
    elif invalid == 'zero_length':
        slots[0]['centerline_length'] = 0
    elif invalid == 'nonunit_direction':
        slots[0]['direction'] = [2, 0]
    elif invalid == 'too_many':
        slots += [dict(slots[0], id=f'slot_{index}', center=[0, 0]) for index in range(len(slots)+1,6)]
    q, offset, trsf = k.pose_values(p['pose'])
    lines = [
        f'stock = extrude(id="stock", profile=polygon(id="outline", points={[[x,y] for x,y in [(-w/2,-d/2),(w/2,-d/2),(w/2,d/2),(-w/2,d/2)]]}, circular_holes={[{"center":[0,0],"radius":bore}]}), height={h})',
        'body = stock',
    ]
    for i in range(4):
        faces = [{'from_feature': 'stock', 'role': f'wall_{(i-1)%4}'},
                 {'from_feature': 'stock', 'role': f'wall_{i}'}]
        # The graph reference is the variable, not its source identifier.
        lines.append(f'body = fillet(id="round_{i}", body=body, faces={str(faces).replace(chr(39), chr(34))}, radius={corner})')
    # The source frame origin lies on actual retained cap material, clear of
    # the circular bore. Slot coordinates are 2D offsets in that frame.
    authored_slots=[dict(slot,center=[slot['center'][0],slot['center'][1]-12*p['scale']])
                    for slot in slots]
    slot_literal = json.dumps(authored_slots, separators=(',', ':'))
    if p['posed_input']:
        frame_x=[trsf.Value(i,1) for i in range(1,4)]
        frame_y=[trsf.Value(i,2) for i in range(1,4)]
        lines.append(f'body = transform(id="posed_stock", body=body, rotation={q}, offset={offset})')
        frame_origin=[sum(trsf.Value(i+1,j+1)*[0,12*p['scale'],0][j]
                          for j in range(3))+trsf.Value(i+1,4) for i in range(3)]
    else:
        frame_x=[1,0,0];frame_y=[0,1,0];frame_origin=[0,12*p['scale'],0]
    lines.append(f'body = slot_pattern(id="finished_local", body=body, slots={slot_literal}, frame_origin={frame_origin}, frame_x_direction={frame_x}, frame_y_direction={frame_y})')
    if not p['posed_input']:
        lines.append(f'body = transform(id="finished", body=body, rotation={q}, offset={offset})')
    lines.append('output(name="finished", feature=body)')
    if p['pose']==0:
        expected_area=closed_moments(p)[0]/h
        lines.append(f'section_view(id="slot_mid", output="finished", origin={[0,0,h/2]}, min_area={expected_area-1e-5}, max_area={expected_area+1e-5})')
    return '\n'.join(lines)+'\n'


def reference(p):
    w, d, h, corner, slots = geometry(p)
    solid = f.filleted([(-w/2,-d/2),(w/2,-d/2),(w/2,d/2),(-w/2,d/2)],
                       h, corner, [0,1,2,3])
    bore=3*p['scale']
    solid=k.boolean(BRepAlgoAPI_Cut,solid,BRepPrimAPI_MakeCylinder(
        gp_Ax2(gp_Pnt(0,0,-h/4),gp_Dir(0,0,1)),bore,1.5*h).Shape())
    for slot in slots:
        x, y = slot['center']; ux, uy = slot['direction']
        half = slot['centerline_length']/2; r = slot['radius']
        z0, cutter_height = -h/4, 1.5*h
        # Axis-aligned reference cutters are intentionally independent of the
        # native capsule edge construction; two semicircular ends are full
        # cylinders, joined to the middle by three separate Boolean cuts.
        if ux:
            box = BRepPrimAPI_MakeBox(gp_Pnt(x-half,y-r,z0),2*half,2*r,cutter_height).Shape()
        else:
            box = BRepPrimAPI_MakeBox(gp_Pnt(x-r,y-half,z0),2*r,2*half,cutter_height).Shape()
        solid = k.boolean(BRepAlgoAPI_Cut, solid, box)
        for sign in (-1,1):
            cylinder = BRepPrimAPI_MakeCylinder(
                gp_Ax2(gp_Pnt(x+sign*half*ux,y+sign*half*uy,z0),gp_Dir(0,0,1)),
                r,cutter_height).Shape()
            solid = k.boolean(BRepAlgoAPI_Cut, solid, cylinder)
    return BRepBuilderAPI_Transform(solid,k.pose_values(p['pose'])[2],True).Shape()


def closed_moments(p):
    w,d,h,corner,slots = geometry(p)
    missing = corner**2*(1-math.pi/4)
    first = corner**3/6
    second = corner**4*(1/3-math.pi/16)
    ax, ay = w/2-corner, d/2-corner
    bore=3*p['scale']
    area = w*d-4*missing-math.pi*bore*bore
    qxx = d*w**3/12-4*(ax*ax*missing+2*ax*first+second)-math.pi*bore**4/4
    qyy = w*d**3/12-4*(ay*ay*missing+2*ay*first+second)-math.pi*bore**4/4
    qxy = 0.0
    mx = my = 0.0
    for slot in slots:
        x,y = slot['center']; ux,uy = slot['direction']
        a = slot['centerline_length']/2; r = slot['radius']
        capsule_area = 4*a*r+math.pi*r*r
        along = 4*r*a**3/3+8*a*r**3/3+math.pi*r*r*a*a+math.pi*r**4/4
        across = 4*a*r**3/3+math.pi*r**4/4
        area -= capsule_area
        mx -= capsule_area*x; my -= capsule_area*y
        qxx -= along*ux*ux+across*uy*uy+capsule_area*x*x
        qyy -= along*uy*uy+across*ux*ux+capsule_area*y*y
        qxy -= (along-across)*ux*uy+capsule_area*x*y
    cx,cy = mx/area,my/area
    central_xx=qxx-area*cx*cx;central_yy=qyy-area*cy*cy
    central_xy=qxy-area*cx*cy
    volume=area*h; z_moment=volume*h*h/12
    inertia=[[h*central_yy+z_moment,-h*central_xy,0],
             [-h*central_xy,h*central_xx+z_moment,0],
             [0,0,h*(central_xx+central_yy)]]
    return k.transformed((volume,[cx,cy,h/2],inertia),k.pose_values(p['pose'])[2])


def surfaces(shape,p):
    _,_,_,corner,slots=geometry(p)
    radii=[];planes=0;unexpected=[];slot_axes=[]
    explorer=TopExp_Explorer(shape,TopAbs_FACE)
    while explorer.More():
        surface=BRepAdaptor_Surface(TopoDS.Face(explorer.Current()))
        if surface.GetType()==GeomAbs_Cylinder:
            cylinder=surface.Cylinder();radius=cylinder.Radius()
            radii.append(radius)
            if abs(radius-slots[0]['radius'])<2e-7:
                axis=cylinder.Axis();location=axis.Location();direction=axis.Direction()
                slot_axes.append(([location.X(),location.Y(),location.Z()],
                                  [direction.X(),direction.Y(),direction.Z()]))
        elif surface.GetType()==GeomAbs_Plane:
            planes+=1
        else:
            unexpected.append(str(surface.GetType()))
        explorer.Next()
    require(not unexpected,f'unexpected STEP supports {unexpected}')
    slot_walls=len(slot_axes)
    require(slot_walls>=2*len(slots),f'slot cylindrical wall coverage {radii}')
    require(sum(abs(radius-corner)<2e-7 for radius in radii)==4,
            f'four rounded outer corners {radii}')
    require(sum(abs(radius-3*p['scale'])<2e-7 for radius in radii)==1,
            f'one existing circular bore {radii}')
    trsf=k.pose_values(p['pose'])[2]
    for slot in slots:
        x,y=slot['center'];ux,uy=slot['direction'];half=slot['centerline_length']/2
        for sign in (-1,1):
            local=[x+sign*half*ux,y+sign*half*uy,0]
            point=[sum(trsf.Value(i+1,j+1)*local[j] for j in range(3))+
                   trsf.Value(i+1,4) for i in range(3)]
            matching=0
            for location,direction in slot_axes:
                dx=[point[i]-location[i] for i in range(3)]
                projection=sum(dx[i]*direction[i] for i in range(3))
                transverse=math.sqrt(sum((dx[i]-projection*direction[i])**2 for i in range(3)))
                matching+=transverse<2e-6
            require(matching>=1,f'exact semicylinder axis at {point}, found {matching}')
    return dict(plane_faces=planes,cylinder_radii_mm=radii,
                slot_cylindrical_faces=slot_walls,
                endpoint_axis_locations_verified=2*len(slots))


def check_case(cad,root,tag,p):
    source,output=root/(tag+'.star'),root/tag
    source.write_text(recipe(p))
    built=command(cad,'build',source,'--out-dir',output)
    command(cad,'verify',output)
    section_evidence=None
    if p['pose']==0:
        section=built['section_views']['slot_mid']['query']['section']
        area=section['area'];expected_area=closed_moments(p)[0]/(8*p['scale'])
        close(area,expected_area,'native midsection area',1e-5)
        require(len(section['loops'])==p['slots']+2,'outer, circular bore and slot loops')
        section_evidence=dict(area_mm2=area,loop_count=len(section['loops']))
    native=command(cad,'properties',source,'--output','finished')
    expected=closed_moments(p)
    oracle=reference(p)
    imported=k.load(str(output/'finished.step'))
    comparisons=[]
    for label,values in [('OCCT',k.properties(oracle)),('STEP',k.properties(imported)),
                         ('native',(native['volume'],native['volume_centroid'],native['central_volume_inertia']))]:
        close(values[0],expected[0],label+' volume',2e-5)
        for i in range(3):
            close(values[1][i],expected[1][i],label+' centroid',2e-6)
            for j in range(3):
                close(values[2][i][j],expected[2][i][j],label+' inertia',2e-4)
        comparisons.append(dict(source=label,volume_mm3=values[0],centroid_mm=values[1]))
    differences=[k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
                 for a,b in [(oracle,imported),(imported,oracle)]]
    for difference in differences:
        close(difference,0,'OCCT/STEP directed difference',2e-6)
    surface_evidence=surfaces(imported,p)
    replay=root/(tag+'-replay')
    require(sha256(source)==sha256(output/'recipe.star'),'published recipe must capture exact authored source')
    command(cad,'build',output/'recipe.star','--out-dir',replay)
    require((replay/'finished.step').read_bytes()==(output/'finished.step').read_bytes(),
            'captured recipe must replay byte-identical STEP')
    return dict(case=tag,parameters=p,volume_mm3=expected[0],centroid_mm=expected[1],
                central_inertia_mm5=expected[2],comparisons=comparisons,
                directed_difference_mm3=differences,surfaces=surface_evidence,
                section=section_evidence,
                source_sha256=sha256(source),captured_source_sha256=sha256(output/'recipe.star'),
                kernel_build_id=built['kernel_build_id'],executor_build_id=built['executor_build_id'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=ROOT/'target/release/cad')
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args();args.out_dir.mkdir(parents=True,exist_ok=False)
    cad=args.cad.resolve();binary_hash=sha256(cad);script_hash=sha256(__file__)
    configs=[('four-identity',parameters()),('four-oblique',parameters(pose=2)),
             ('four-oblique-posed-input',parameters(pose=2,posed_input=True)),
             ('one-halfscale',parameters(scale=.5,pose=1,slots=1)),
             ('four-double',parameters(scale=2,pose=1)),
             ('short-slots',parameters(length=2,radius=1.5)),
             ('outer-clearance',parameters(slots=1,variant='outer_clearance')),
             ('circle-clearance',parameters(slots=1,variant='circle_clearance')),
             ('slot-clearance',parameters(slots=2,variant='slot_clearance'))]
    cases=[check_case(cad,args.out_dir,tag,p) for tag,p in configs]
    source=args.out_dir/'refusals.star';out=args.out_dir/'preserved';cache=args.out_dir/'cache'
    baseline=parameters(slots=2)
    source.write_text(recipe(baseline));command(cad,'build',source,'--out-dir',out,'--cache-dir',cache)
    before=inventory(out);original_step_sha=sha256(out/'finished.step')
    original_semantic=semantic_manifest(out)
    revised=parameters(slots=2,length=7)
    source.write_text(recipe(revised));command(cad,'build',source,'--out-dir',out,'--cache-dir',cache)
    revised_step_sha=sha256(out/'finished.step')
    require(revised_step_sha!=original_step_sha,'changed source must invalidate cached STEP')
    revised_semantic=semantic_manifest(out)
    require(revised_semantic[0]['feature_hashes']['finished_local']!=
            original_semantic[0]['feature_hashes']['finished_local'],
            'changed slot parameters must invalidate native feature hash')
    source.write_text(recipe(baseline));command(cad,'build',source,'--out-dir',out,'--cache-dir',cache)
    restored=inventory(out)
    stable=lambda files:{name:digest for name,digest in files.items() if name!='manifest.json'}
    require(stable(restored)==stable(before),'restored source must reproduce native and engineering artifact bytes')
    require(semantic_manifest(out)==original_semantic,'restored source semantic manifest identity')
    before=restored
    cache_evidence=dict(original_step_sha256=original_step_sha,revised_step_sha256=revised_step_sha,
                        restored_publication_files=len(before),
                        original_slot_feature_hash=original_semantic[0]['feature_hashes']['finished_local'],
                        revised_slot_feature_hash=revised_semantic[0]['feature_hashes']['finished_local'])
    rejections=[]
    for invalid in ['outer_contact','near_outer_penetration','circle_contact',
                    'circle_overlap','slot_contact','slot_overlap','zero_length',
                    'nonunit_direction','too_many']:
        source.write_text(recipe(baseline,invalid));failure=command(cad,'build',source,'--out-dir',out,'--cache-dir',cache,success=False)
        require(inventory(out)==before,f'{invalid}: failed build changed published bytes')
        rejections.append(dict(case=invalid,diagnostic=failure,preserved_files=len(before)))
    command(cad,'verify',out)
    require(sha256(cad)==binary_hash,'binary changed during oracle')
    require(sha256(__file__)==script_hash,'oracle script changed during oracle')
    report=dict(valid=True,executable_sha256=binary_hash,oracle_script_sha256=script_hash,
                cases=cases,rejections=rejections,
                cache_source_mutation=cache_evidence,
                python=platform.python_version(),ocp=importlib.metadata.version('cadquery-ocp'),
                scope='bounded positive-length through capsules on four-corner filleted convex plate with existing circular bore; analytic nominal moments and independent OCCT STEP comparison, no general inner-arc Boolean')
    (args.out_dir/'report.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    print(json.dumps(dict(valid=True,cases=len(cases),rejections=len(rejections),report=str(args.out_dir/'report.json'))))


if __name__=='__main__':
    main()
