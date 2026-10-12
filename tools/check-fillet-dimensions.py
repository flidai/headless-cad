#!/usr/bin/env python3
"""Independent native partial-arc radius and SVG witness acceptance.

OCP is only an exchange oracle. This checks nominal analytic support dimensions,
not manufactured tolerances, fit or a certified physical uncertainty interval.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import xml.etree.ElementTree as ET

SPEC=importlib.util.spec_from_file_location('fillet',Path(__file__).with_name('check-longitudinal-fillet.py'))
f=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(f)
k=f.k


def case(cad,root,radius,bore,h):
    tag=f'r{radius}-b{bore}-h{h}';output=root/tag
    source=k.ROOT/'examples/rounded_bearing_cover.star'
    params=['--param',f'corner_radius={radius}','--param',f'bore_radius={bore}','--param',f'height={h}']
    built=f.run(cad,'build',source,'--out-dir',output,*params)
    f.run(cad,'verify',output)
    report=json.loads((output/'section-cover_midsection.json').read_text())
    dimensions=report['section_view']['circle_dimensions']
    arcs=[d for d in dimensions if d['declaration'].get('coverage')=='single_arc']
    k.require(len(arcs)==4,'four explicit partial arc radius dimensions')
    for dimension in arcs: k.close(dimension['value_mm'],radius)
    d=next(d for d in arcs if d['declaration']['id']=='front_left_radius');arc=d['arc'];face=arc['target_face']
    segment=report['section']['loops'][arc['loop_index']]['segments'][arc['segment_index']]
    curve=segment['curve']
    k.require(segment['face']==face and curve['kind']=='arc','actual selected native arc')
    matches=[s for loop in report['section']['loops'] for s in loop['segments'] if s['face']==face]
    k.require(len(matches)==1,'unique matching native segment')
    witnesses=d['selection']['witnesses']
    k.require(len(witnesses)==1 and witnesses[0]['target_face']==face,'semantic face witness')
    k.require(d['declaration']['kind']=='radius' and d['status']=='passed','nominal radius passed')
    for observed in [d['value_mm'],d['radius'],curve['radius']]:k.close(observed,radius)
    k.close(d['center'][0],radius);k.close(d['center'][1],radius)
    for key in ['start_angle','sweep_angle']:k.close(arc[key],curve[key])
    k.close(abs(arc['sweep_angle']),math.pi/2)
    tip=[radius*(1-1/math.sqrt(2))]*2
    for i in range(2):k.close(arc['tip'][i],tip[i])
    k.require(math.isfinite(arc['arithmetic_uncertainty_mm']) and arc['arithmetic_uncertainty_mm']>=0,'finite arithmetic bound')
    # Independently imported exact STEP must contain this corner's cylindrical
    # support. Do not identify it by snapshot-local face number or mesh samples.
    solid=k.load(str(output/'bearing_cover.step'));e=f.TopExp_Explorer(solid,f.TopAbs_FACE);found=[]
    while e.More():
        surface=f.BRepAdaptor_Surface(f.TopoDS.Face(e.Current()))
        if surface.GetType()==f.GeomAbs_Cylinder:
            c=surface.Cylinder();p=c.Location()
            if abs(p.X()-radius)<1e-7 and abs(p.Y()-radius)<1e-7:
                found.append(c.Radius())
        e.Next()
    k.require(len(found)==1,('unique independent cylinder',found));k.close(found[0],radius)
    svg=ET.parse(output/'section-cover_midsection.svg').getroot()
    groups=[g for g in svg.iter() if g.attrib.get('data-id')==d['declaration']['id']]
    k.require(len(groups)==1,'one SVG radius annotation')
    polygons=[p for p in groups[0] if p.tag.endswith('polygon')]
    k.require(len(polygons)==1,'single radius arrow')
    coords=[float(v) for v in polygons[0].attrib['points'].replace(',',' ').split()]
    k.close(coords[0],tip[0]);k.close(coords[1],-tip[1])
    corner_witnesses=[]
    placements={'front_left_radius':(radius,radius,-1,-1),
                'front_right_radius':(60-radius,radius,1,-1),
                'back_right_radius':(60-radius,40-radius,1,1),
                'back_left_radius':(radius,40-radius,-1,1)}
    for dimension in arcs:
        identity=dimension['declaration']['id'];w=dimension['arc']
        cx,cy,sx,sy=placements[identity]
        expected_tip=[cx+sx*radius/math.sqrt(2),cy+sy*radius/math.sqrt(2)]
        native=report['section']['loops'][w['loop_index']]['segments'][w['segment_index']]
        k.require(native['face']==w['target_face'],'each dimension binds its actual native face')
        selected=[s for loop in report['section']['loops'] for s in loop['segments'] if s['face']==w['target_face']]
        k.require(len(selected)==1,'each dimension has unique native segment')
        k.require(len(dimension['selection']['witnesses'])==1 and dimension['selection']['witnesses'][0]['target_face']==w['target_face'],'each dimension binds semantic lineage')
        for key in ['start_angle','sweep_angle']:k.close(w[key],native['curve'][key])
        k.close(native['curve']['radius'],radius);k.close(abs(w['sweep_angle']),math.pi/2)
        for i in range(2):k.close(w['tip'][i],expected_tip[i]);k.close(dimension['center'][i],[cx,cy][i])
        g=next(g for g in svg.iter() if g.attrib.get('data-id')==identity)
        arrow=next(p for p in g if p.tag.endswith('polygon'))
        xy=[float(v) for v in arrow.attrib['points'].replace(',',' ').split()]
        k.close(xy[0],expected_tip[0]);k.close(xy[1],-expected_tip[1])
        ex=f.TopExp_Explorer(solid,f.TopAbs_FACE);radii=[]
        while ex.More():
            surface=f.BRepAdaptor_Surface(f.TopoDS.Face(ex.Current()))
            if surface.GetType()==f.GeomAbs_Cylinder:
                cylinder=surface.Cylinder();location=cylinder.Location()
                if abs(location.X()-cx)<1e-7 and abs(location.Y()-cy)<1e-7:radii.append(cylinder.Radius())
            ex.Next()
        k.require(len(radii)==1,'each corner has unique independent cylindrical support');k.close(radii[0],radius)
        corner_witnesses.append({'id':identity,'native_arc':w,'step_radius_mm':radii[0],'svg_tip_mm':[xy[0],-xy[1]]})
    circles=[d for d in dimensions if d['declaration']['kind']=='diameter']
    k.require(len(circles)==1 and 'arc' not in circles[0],'legacy full circle evidence unchanged')
    k.close(circles[0]['value_mm'],2*bore)
    replay=root/(tag+'-replay');f.run(cad,'build',output/'recipe.star','--out-dir',replay,*params)
    replay_report=json.loads((replay/'section-cover_midsection.json').read_text())
    k.require(replay_report['section_view']['circle_dimensions']==dimensions,'captured recipe dimension replay')
    return {'case':tag,'parameter_overrides':{'corner_radius':radius,'bore_radius':bore,'height':h},'radius_mm':radius,'native_arc':arc,'independent_step_radius_mm':found[0],
            'svg_arrow_tip_mm':[coords[0],-coords[1]],'corners':corner_witnesses,'kernel_build_id':built['kernel_build_id'],
            'executor_build_id':built['executor_build_id']}


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cad',type=Path,default=k.ROOT/'target/release/cad');p.add_argument('--out-dir',type=Path,required=True)
    args=p.parse_args();root=args.out_dir.resolve();root.mkdir(parents=True,exist_ok=False)
    cases=[case(args.cad.resolve(),root,*params) for params in [(4,10,8),(2,8,4),(6,12,12)]]
    accepted=root/'r4-b10-h8';before=k.inventory(accepted)
    source=(k.ROOT/'examples/rounded_bearing_cover.star').read_text()
    needle='max = corner_radius+0.000001'
    k.require(needle in source,'explicit native radius design bound')
    invalid=root/'bad-radius-bound.star';invalid.write_text(source.replace(needle,'max = corner_radius-0.25').replace('min = corner_radius-0.000001','min = corner_radius-0.5'))
    failure=f.run(args.cad.resolve(),'build',invalid,'--output','rounded_blank','--out-dir',accepted,success=False)
    k.require(k.inventory(accepted)==before,'global radius obligation preserves accepted generation')
    k.require('dimension' in failure.get('code','') or 'radius' in failure.get('diagnostic',''),'native dimension failure')
    report={'valid':True,'measurement':'nominal native analytic support radius','cases':cases,'global_bound_failure':failure}
    (root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))

if __name__=='__main__':main()
