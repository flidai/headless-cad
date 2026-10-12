#!/usr/bin/env python3
"""Independent OCCT acceptance for the nominal retained drive module.

OCP is used only by this test. The recipe, kernel and STEP writer have no OCP
runtime dependency. This checks exact exported solids and source certificates;
it does not certify ring installation, strength, or manufactured fit.
"""

import argparse
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'crates/cad-export/tools'))


def oracle(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


drive = oracle('keyed_drive_oracle', 'check-keyed-drive-module.py')
retained = oracle('retained_shaft_oracle', 'check-retained-shaft.py')
coupling = drive.keyed
k = retained.k

from check_assembly_ocp import require
from check_constrained_fixture_ocp import body_properties
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common, BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeFace, BRepBuilderAPI_MakePolygon, BRepBuilderAPI_Transform
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder, BRepPrimAPI_MakePrism
from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.gp import gp_Pnt, gp_Quaternion, gp_Trsf, gp_Vec


def close(actual, expected, label, relative=2e-9, absolute=2e-7):
    require(math.isfinite(actual) and abs(actual-expected) <= max(absolute, abs(expected)*relative),
            f'{label}: {actual} differs from {expected}')


def placed(shape, offset, rotation=None):
    frame = gp_Trsf()
    if rotation is not None:
        frame.SetRotation(gp_Quaternion(*rotation))
    frame.SetTranslationPart(gp_Vec(*offset))
    return BRepBuilderAPI_Transform(shape, frame, True).Shape()


def polygon_prism(points, height):
    polygon = BRepBuilderAPI_MakePolygon()
    for x, y in points:
        polygon.Add(gp_Pnt(x, y, 0))
    polygon.Close()
    face = BRepBuilderAPI_MakeFace(polygon.Wire()).Face()
    return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, height)).Shape()


def annulus(outer, inner, height):
    return k.boolean(BRepAlgoAPI_Cut, BRepPrimAPI_MakeCylinder(outer, height).Shape(),
                     BRepPrimAPI_MakeCylinder(inner, height).Shape())


def bored_polygon(points, height, holes):
    shape = polygon_prism(points, height)
    for x, y, radius in holes:
        shape = k.boolean(BRepAlgoAPI_Cut, shape,
                          placed(BRepPrimAPI_MakeCylinder(radius, height).Shape(), [x, y, 0]))
    return shape


def ring(outer, inner):
    whole = annulus(outer, inner, 1.2)
    wedge = polygon_prism([(0, 0), (4*outer, -4*outer/math.sqrt(3)),
                           (4*outer, 4*outer/math.sqrt(3))], 1.2)
    return k.boolean(BRepAlgoAPI_Cut, whole, wedge)


def geometries(diameter, pitch):
    r = diameter/2
    p = pitch
    d = 25+p/2
    q = [.5, .5, .5, .5]
    holes = [(x, y, 3) for x in [33, p+33] for y in [-28, 28]]
    base = bored_polygon([(0, -40), (p+90, -40), (p+90, 40), (0, 40)], 6, holes)
    housing = bored_polygon([(-22, -26), (22, -26), (22, 14), (14, 22),
                             (-14, 22), (-22, 14)], 16, [(0, 0, r+4.025)])
    bush = retained.revolve([[r+.15, 0], [r+6, 0], [r+6, 2], [r+4, 2],
                             [r+4, 18], [r+.15, 18]])
    profile = [[0, 0], [r+2, 0], [r+2, 12.9], [r, 12.9],
               [r, 12.2+p/2], [r-.6, 12.2+p/2], [r-.6, 13.45+p/2],
               [r, 13.45+p/2], [r, 30.55+p/2], [r-.6, 30.55+p/2],
               [r-.6, 31.8+p/2], [r, 31.8+p/2], [r, p+34.55],
               [r-.6, p+34.55], [r-.6, p+35.8], [r, p+35.8],
               [r, p+70], [0, p+70]]
    shaft = k.boolean(BRepAlgoAPI_Cut, retained.revolve(profile),
                      BRepPrimAPI_MakeBox(gp_Pnt(r-2, -2.02, 15.25+p/2),
                                           4, 4.04, 13.5).Shape())
    disc = k.boolean(BRepAlgoAPI_Cut, annulus(24, r+.15, 14),
                     BRepPrimAPI_MakeBox(gp_Pnt(0, -2.02, -1),
                                          r+2.05, 4.04, 16).Shape())
    key = BRepPrimAPI_MakeBox(4, 4, 13).Shape()
    washer = annulus(r+2, r+.1, 1.5)
    split_ring = ring(r+1.5, r-.5)
    clamp = bored_polygon([(0, -34), (12, -34), (12, 34), (0, 34)],
                           6, [(6, -28, 3), (6, 28, 3)])
    bolt = retained.revolve([[0, 0], [2.5, 0], [2.5, 65], [4.5, 65],
                             [4.5, 68], [0, 68]])
    nut = annulus(5, 2.6, 5)
    definitions = dict(base=base, housing=housing, sleeve=bush, shaft=shaft,
                       drive_disc=disc, parallel_key=key, washer=washer,
                       retaining_ring=split_ring, clamp_bar=clamp,
                       clamp_bolt_envelope=bolt, nut_envelope=nut)
    refs = {
        ('base',): placed(base, [0, 0, 0]),
        ('left_clamp',): placed(clamp, [27, 0, 54]),
        ('right_clamp',): placed(clamp, [p+27, 0, 54]),
        ('left_support', 'housing'): placed(housing, [25, 0, 32], q),
        ('left_support', 'sleeve'): placed(placed(bush, [0, 0, -2]), [25, 0, 32], q),
        ('right_support', 'right_housing'): placed(housing, [25+p, 0, 32], q),
        ('right_support', 'right_sleeve'): placed(placed(bush, [0, 0, 18], [1, 0, 0, 0]), [25+p, 0, 32], q),
        ('shaft',): placed(shaft, [10, 0, 32], q),
        ('drive_disc',): placed(disc, [d, 0, 32], q),
        ('parallel_key',): placed(key, [d+.5, r-2, 30], q),
        ('hub_left_washer',): placed(washer, [d-1.6, 0, 32], q),
        ('hub_left_ring',): placed(split_ring, [d-2.8, 0, 32], q),
        ('hub_right_washer',): placed(washer, [d+14.1, 0, 32], q),
        ('hub_right_ring',): placed(split_ring, [d+15.6, 0, 32], q),
        ('shaft_washer',): placed(washer, [p+43.1, 0, 32], q),
        ('shaft_ring',): placed(split_ring, [p+44.6, 0, 32], q),
    }
    part_ids = {key: key[-1] for key in refs}
    for key in [('left_support', 'housing'), ('right_support', 'right_housing')]:
        part_ids[key] = 'housing'
    for key in [('left_support', 'sleeve'), ('right_support', 'right_sleeve')]:
        part_ids[key] = 'sleeve'
    for key in [('left_clamp',), ('right_clamp',)]:
        part_ids[key] = 'clamp_bar'
    for key in [('hub_left_washer',), ('hub_right_washer',), ('shaft_washer',)]:
        part_ids[key] = 'washer'
    for key in [('hub_left_ring',), ('hub_right_ring',), ('shaft_ring',)]:
        part_ids[key] = 'retaining_ring'
    for i, (x, y) in enumerate([(x, y) for x in [33, p+33] for y in [-28, 28]]):
        refs[(f'clamp_bolt_{i}',)] = placed(bolt, [x, y, -5])
        refs[(f'clamp_nut_{i}',)] = placed(nut, [x, y, -5])
        part_ids[(f'clamp_bolt_{i}',)] = 'clamp_bolt_envelope'
        part_ids[(f'clamp_nut_{i}',)] = 'nut_envelope'
    return definitions, refs, part_ids


def expected_volumes(diameter, pitch):
    r = diameter/2
    b = 2.02
    bore = r+.15
    seat_area = b*math.sqrt(r*r-b*b)+r*r*math.asin(b/r)-2*(r-2)*b
    keyway_area = 2*(r+2.05)*b-b*math.sqrt(bore*bore-b*b)-bore*bore*math.asin(b/bore)
    return {
        'base': 6*(80*(pitch+90)-4*math.pi*3**2),
        'housing': 16*(2048-math.pi*(r+4.025)**2),
        'sleeve': math.pi*((r+6)**2-bore**2)*2+math.pi*((r+4)**2-bore**2)*16,
        'shaft': math.pi*((r+2)**2*12.9+r*r*(pitch+70-12.9)
                         -3*1.25*(r*r-(r-.6)**2))-13.5*seat_area,
        'drive_disc': 14*(math.pi*(24**2-bore**2)-keyway_area),
        'parallel_key': 4*4*13,
        'washer': 1.5*math.pi*((r+2)**2-(r+.1)**2),
        'retaining_ring': 1.2*(5*math.pi/6)*((r+1.5)**2-(r-.5)**2),
        'clamp_bar': 6*(12*68-2*math.pi*3**2),
        'clamp_bolt_envelope': math.pi*(2.5**2*65+4.5**2*3),
        'nut_envelope': 5*math.pi*(5**2-2.6**2),
    }


def density(part_id):
    return (2700 if part_id in ('base', 'housing', 'clamp_bar')
            else 8800 if part_id == 'sleeve' else 7800)/1e9


def bounds(shape):
    box = Bnd_Box()
    BRepBndLib.Add_s(shape, box)
    lo, hi = box.CornerMin(), box.CornerMax()
    return ([lo.X(), lo.Y(), lo.Z()], [hi.X(), hi.Y(), hi.Z()])


def separated_boxes(a, b):
    return any(a[1][i] < b[0][i]-1e-5 or b[1][i] < a[0][i]-1e-5 for i in range(3))


def actual_planes(actual, diameter, pitch):
    r = diameter/2
    p = pitch
    d = 25+p/2
    coords = {
        'shaft_shoulder': (('shaft',), 22.9),
        'left_bush_outer': (('left_support', 'sleeve'), 23),
        'right_bush_outer': (('right_support', 'right_sleeve'), p+43),
        'shaft_loaded_wall': (('shaft',), p+45.8),
        'shaft_hub_left_wall': (('shaft',), d-2.8),
        'shaft_hub_right_wall': (('shaft',), d+16.8),
        'hub_start': (('drive_disc',), d),
        'hub_end': (('drive_disc',), d+14),
    }
    for part, start, length in [
        ('hub_left_ring', d-2.8, 1.2), ('hub_left_washer', d-1.6, 1.5),
        ('hub_right_washer', d+14.1, 1.5), ('hub_right_ring', d+15.6, 1.2),
        ('shaft_washer', p+43.1, 1.5), ('shaft_ring', p+44.6, 1.2)]:
        coords[part+'_start'] = ((part,), start)
        coords[part+'_end'] = ((part,), start+length)
    planes = {name: coupling.plane(actual[path], 0, x) for name, (path, x) in coords.items()}
    x_values = {name: BRepAdaptor_Surface(face).Plane().Location().X()
                for name, face in planes.items()}
    for name, (_, x) in coords.items():
        close(x_values[name], x, 'actual axial plane '+name, absolute=1e-7)
    contacts = {
        'shaft_ring_to_groove': ('shaft_ring_end', 'shaft_loaded_wall'),
        'shaft_ring_to_washer': ('shaft_ring_start', 'shaft_washer_end'),
        'hub_left_ring_to_groove': ('hub_left_ring_start', 'shaft_hub_left_wall'),
        'hub_left_ring_to_washer': ('hub_left_ring_end', 'hub_left_washer_start'),
        'hub_right_ring_to_groove': ('hub_right_ring_end', 'shaft_hub_right_wall'),
        'hub_right_ring_to_washer': ('hub_right_ring_start', 'hub_right_washer_end'),
    }
    distances = {}
    for name, (left, right) in contacts.items():
        distances[name] = coupling.convex.distance(planes[left], planes[right])
        close(distances[name], 0, name+' physical contact', absolute=1e-6)
    gaps = {
        'left_shoulder_bush': x_values['left_bush_outer']-x_values['shaft_shoulder'],
        'right_bush_washer': x_values['shaft_washer_start']-x_values['right_bush_outer'],
        'hub_left_washer_disc': x_values['hub_start']-x_values['hub_left_washer_end'],
        'hub_disc_right_washer': x_values['hub_right_washer_start']-x_values['hub_end'],
    }
    for name, value in gaps.items():
        close(value, .1, 'signed axial gap '+name, absolute=1e-7)
    dimensions = {
        'shaft_stop_span': x_values['shaft_loaded_wall']-x_values['shaft_shoulder'],
        'bush_outer_span': x_values['right_bush_outer']-x_values['left_bush_outer'],
        'hub_stop_span': x_values['shaft_hub_right_wall']-x_values['shaft_hub_left_wall'],
        'hub_width': x_values['hub_end']-x_values['hub_start'],
    }
    for name in ['shaft_washer', 'shaft_ring', 'hub_left_washer', 'hub_right_washer',
                 'hub_left_ring', 'hub_right_ring']:
        dimensions[name+'_thickness'] = x_values[name+'_end']-x_values[name+'_start']
    endplay = {
        'shaft_axial_endplay': dimensions['shaft_stop_span']-dimensions['bush_outer_span']
                               -dimensions['shaft_washer_thickness']-dimensions['shaft_ring_thickness'],
        'hub_axial_endplay': dimensions['hub_stop_span']-dimensions['hub_width']
                             -sum(dimensions[name+'_thickness'] for name in
                                  ['hub_left_washer', 'hub_right_washer', 'hub_left_ring', 'hub_right_ring']),
    }
    for name, value in endplay.items():
        close(value, .2, name+' actual endplay', absolute=1e-7)
    close(gaps['left_shoulder_bush']+gaps['right_bush_washer'], .2,
          'shaft signed actual gap sum', absolute=1e-7)
    close(gaps['hub_left_washer_disc']+gaps['hub_disc_right_washer'], .2,
          'hub signed actual gap sum', absolute=1e-7)
    shaft_floor = coupling.plane(actual[('shaft',)], 1, r-2)
    key_floor = coupling.plane(actual[('parallel_key',)], 1, r-2)
    disc_roof = coupling.plane(actual[('drive_disc',)], 1, r+2.05)
    key_roof = coupling.plane(actual[('parallel_key',)], 1, r+2)
    keyed_gaps = {
        'floor_contact': coupling.convex.distance(shaft_floor, key_floor),
        'roof': coupling.convex.distance(disc_roof, key_roof),
        'positive_side': coupling.convex.distance(
            coupling.plane(actual[('drive_disc',)], 2, 34.02),
            coupling.plane(actual[('parallel_key',)], 2, 34)),
        'negative_side': coupling.convex.distance(
            coupling.plane(actual[('drive_disc',)], 2, 29.98),
            coupling.plane(actual[('parallel_key',)], 2, 30)),
    }
    for name, value in keyed_gaps.items():
        close(value, 0 if name == 'floor_contact' else .05 if name == 'roof' else .02,
              'physical keyed engagement '+name, absolute=1e-6)
    return dict(planes_mm=x_values, contacts_mm=distances, signed_gaps_mm=gaps,
                dimensions_mm=dimensions, endplay_mm=endplay, keyed_engagement_mm=keyed_gaps)


def check_certificates(summary, physical, diameter, pitch):
    dimensions = summary['datum_dimensions']
    expected = physical['dimensions_mm']
    require(set(dimensions) == set(expected), 'all ten actual datum dimensions required')
    for name, value in expected.items():
        report = dimensions[name]
        require(report['status'] == 'passed', 'datum dimension failed: '+name)
        close(report['value_mm'], value, 'native face measurement '+name, absolute=1e-7)
        require(report['lower_bound_mm'] <= value+1e-8 <= report['upper_bound_mm']+1e-8,
                'actual dimension must be inside native numeric interval: '+name)
        for endpoint in ['from', 'to']:
            witness = report[endpoint]
            require('native_plane' in witness and 'world_plane' in witness and
                    'semantic_datum' in witness and 'part_content_hash' in witness,
                    'complete source face witness: '+name)
    stacks = summary['tolerance_stacks']
    expected_limits = {'shaft_axial_endplay': (.13, .27),
                       'hub_axial_endplay': (.115, .285)}
    for name, (low, high) in expected_limits.items():
        report = stacks[name]
        require(report['status'] == 'passed' and report['measurement_interval_certified'],
                'native measured stack certified: '+name)
        close(report['nominal_mm'], .2, name+' nominal', absolute=1e-7)
        close(report['lower_limit_mm'], low, name+' design lower', absolute=1e-7)
        close(report['upper_limit_mm'], high, name+' design upper', absolute=1e-7)
        require(report['measured_lower_bound_mm'] <= physical['endplay_mm'][name]+1e-8
                <= report['measured_upper_bound_mm']+1e-8,
                'actual OCP endplay enclosed: '+name)
    circles = json.loads((summary['_out_dir']/'section-bearing_section.json').read_text())['section_view']
    actual_circles = {item['declaration']['id']: item for item in circles['circle_dimensions']}
    for name, value in {'shaft_diameter': diameter, 'sleeve_bore': diameter+.3,
                        'sleeve_outer': diameter+8, 'housing_bore': diameter+8.05}.items():
        report = actual_circles[name]
        require(report['status'] == 'passed', 'section circle '+name)
        close(report['value_mm'], value, 'section circle '+name)
    plan = json.loads((summary['_out_dir']/'section-mounting_plan.json').read_text())['section_view']
    spacing = next(item for item in plan['dimensions'] if item['declaration']['id'] == 'support_pitch')
    close(spacing['value_mm'], pitch, 'mounted support pitch')
    return dict(datum_dimensions={name: dimensions[name]['value_mm'] for name in dimensions},
                axial_stacks={name: {key: stacks[name][key] for key in
                                     ['nominal_mm', 'lower_limit_mm', 'upper_limit_mm',
                                      'measured_lower_bound_mm', 'measured_upper_bound_mm',
                                      'measurement_interval_certified']}
                              for name in expected_limits})


def aggregate(values):
    """Independent centroid and central inertia from placed OCCT leaf properties.

    values are (volume, centroid, central volume inertia, kg/mm^3 density).
    The mass tensor uses the same geometry with each leaf's specified density.
    """
    def combine(weighted):
        total = math.fsum(weight for weight, _, _ in weighted)
        center = [math.fsum(weight*c[i] for weight, c, _ in weighted)/total for i in range(3)]
        inertia = [[math.fsum(
            tensor[i][j] + weight*((sum((c[a]-center[a])**2 for a in range(3)) if i == j else 0)
                                    -(c[i]-center[i])*(c[j]-center[j]))
            for weight, c, tensor in weighted) for j in range(3)] for i in range(3)]
        return total, center, inertia
    volume = combine([(v, c, tensor) for v, c, tensor, _ in values])
    mass = combine([(v*d, c, [[x*d for x in row] for row in tensor]) for v, c, tensor, d in values])
    return volume, mass


def compare_moments(native, values):
    volume, mass = aggregate(values)
    close(native['volume'], volume[0], 'full assembly volume')
    close(native['mass_kg'], mass[0], 'full assembly mass', absolute=2e-9)
    for name, expected in [('volume_centroid', volume[1]), ('center_of_mass', mass[1])]:
        for i in range(3):
            close(native[name][i], expected[i], name+str(i))
    for name, expected in [('central_volume_inertia', volume[2]), ('central_mass_inertia', mass[2])]:
        for i in range(3):
            for j in range(3):
                close(native[name][i][j], expected[i][j], name+str((i, j)), relative=2e-8,
                      absolute=2e-6 if name.startswith('central_volume') else 2e-9)
    return dict(volume_mm3=volume[0], centroid_mm=volume[1], central_volume_inertia_mm5=volume[2],
                mass_kg=mass[0], center_of_mass_mm=mass[1], central_mass_inertia_kg_mm2=mass[2])


def case(cad, directory, diameter, pitch):
    output = directory/f'd{diameter}-p{pitch}'
    source = ROOT/'examples/drive_module_retained.star'
    build = coupling.convex.run(cad, source, output,
                                dict(shaft_diameter=diameter, mounting_pitch=pitch))
    require(build.returncode == 0, 'retained module build failed: '+build.stderr)
    summary = json.loads(build.stdout)
    summary['_out_dir'] = output
    manifest = json.loads((output/'manifest.json').read_text())
    assembly_name = 'drive_module_release'
    native = manifest['outputs'][assembly_name]
    require(native['instances'] == 24 and native['definitions'] == 11,
            '24 real occurrences and 11 shared part definitions')
    expected = expected_volumes(diameter, pitch)
    definitions, references, part_ids = geometries(diameter, pitch)
    require(len(references) == 24 and set(definitions) == set(expected), 'independent fixture inventory')
    for part, shape in definitions.items():
        close(k.properties(shape)[0], expected[part], 'independent '+part+' volume', relative=1e-8)
    counts = {part: list(part_ids.values()).count(part) for part in definitions}
    bom_items = {item['part_id']: item for item in native['bom']['items']}
    require(set(bom_items) == set(definitions), 'BOM real part identities')
    require(native['bom']['missing_material_occurrences'] == 0,
            'all occurrence densities supplied')
    analytic_volume = math.fsum(counts[name]*volume for name, volume in expected.items())
    analytic_mass = math.fsum(counts[name]*volume*density(name)
                              for name, volume in expected.items())
    close(native['volume_mm3'], analytic_volume, 'assembly analytical volume', relative=1e-9)
    close(native['bom']['total_mass_kg'], analytic_mass, 'assembly analytical mass',
          relative=1e-9, absolute=2e-9)
    for part, item in bom_items.items():
        require(item['quantity'] == counts[part], 'BOM quantity '+part)
        close(item['unit_volume_mm3'], expected[part], 'BOM unit volume '+part, relative=1e-9)
        close(item['unit_mass_kg'], expected[part]*density(part), 'BOM unit mass '+part,
              relative=1e-9, absolute=2e-9)
    leaves = coupling.convex.imported_leaves(output/(assembly_name+'.step'))
    require(set(leaves) == set(references),
            'exact full STEP assembly occurrence paths: '+str(set(references)^set(leaves)))
    differences = {}
    actual = {}
    moments = []
    for path, reference in references.items():
        leaf = leaves[path]
        require(leaf['part'] == part_ids[path], 'STEP part identity '+str(path))
        shape = leaf['shape']
        actual[path] = shape
        ref_volume = expected[part_ids[path]]
        volume, center, inertia = k.properties(shape)
        close(volume, ref_volume, 'imported leaf volume '+str(path), relative=1e-8)
        for i in range(3):
            close(center[i], k.properties(reference)[1][i],
                  'imported centroid '+str(path)+str(i), relative=1e-8)
        differences['/'.join(path)] = [k.properties(k.boolean(BRepAlgoAPI_Cut, a, b))[0]
                                       for a, b in [(shape, reference), (reference, shape)]]
        for value in differences['/'.join(path)]:
            close(value, 0, 'directed STEP shape difference '+str(path), absolute=3e-6)
        moments.append((volume, center, inertia, density(part_ids[path])))
    property_result = k.command(cad, 'properties', source, '--output', assembly_name,
                                '--param', f'shaft_diameter={diameter}',
                                '--param', f'mounting_pitch={pitch}')
    require(property_result.returncode == 0, 'native full moments: '+property_result.stderr)
    native_properties = json.loads(property_result.stdout)
    full_moments = compare_moments(native_properties, moments)
    imported_assembly = body_properties(output/(assembly_name+'.step'))
    close(imported_assembly[0], analytic_volume, 'flattened STEP assembly volume', relative=1e-8)
    for i in range(3):
        close(imported_assembly[1][i], full_moments['centroid_mm'][i],
              'flattened STEP assembly centroid '+str(i), relative=1e-8)
    hierarchy = subprocess.run([
        sys.executable, str(ROOT/'crates/cad-export/tools/check_assembly_ocp.py'),
        str(output/(assembly_name+'.step')), '--instances', '24', '--definitions', '11',
        '--solids', '24', '--assembly-json', str(output/(assembly_name+'.assembly.json')),
    ], capture_output=True, text=True)
    require(hierarchy.returncode == 0, 'independent STEP hierarchy: '+hierarchy.stderr)
    checked_boxes = {path: bounds(shape) for path, shape in actual.items()}
    overlaps = {}
    paths = list(actual)
    for i, left in enumerate(paths):
        for right in paths[i+1:]:
            if separated_boxes(checked_boxes[left], checked_boxes[right]):
                volume = 0.
            else:
                volume = k.properties(k.boolean(BRepAlgoAPI_Common,
                                                 actual[left], actual[right]))[0]
            close(volume, 0, 'unintended placed-solid overlap '+str((left, right)),
                  absolute=2e-6)
            overlaps['/'.join(left)+' | '+'/'.join(right)] = volume
    physical = actual_planes(actual, diameter, pitch)
    certificates = check_certificates(summary, physical, diameter, pitch)
    expected_clearance = {
        'left_support_running_gap': .1,
        'right_support_running_gap': .15,
        'left_support_disc_gap': pitch/2-16,
        'right_support_disc_gap': pitch/2-14,
        'disc_shaft_gap': .15,
        'hub_left_washer_shaft_gap': .1,
        'hub_right_washer_shaft_gap': .1,
        'shaft_washer_shaft_gap': .1,
        'hub_left_washer_hub_gap': .1,
        'hub_right_washer_hub_gap': .1,
        'shaft_washer_bush_gap': .1,
    }
    require(set(summary['clearance_checks']) == set(expected_clearance),
            'all eleven declared static clearance checks')
    for name, value in expected_clearance.items():
        report = summary['clearance_checks'][name]
        require(report['status'] == 'passed' and
                report['lower_bound_mm']-1e-7 <= value <= report['upper_bound_mm']+1e-7,
                'native independent clearance interval '+name)
    verified = k.command(cad, 'verify', output)
    require(verified.returncode == 0 and json.loads(verified.stdout)['valid'],
            'immutable generation integrity')
    return dict(parameters=dict(shaft_diameter=diameter, mounting_pitch=pitch),
                output=str(output), analytical_volume_mm3=analytic_volume,
                analytical_mass_kg=analytic_mass, bom_counts=counts,
                full_moments=full_moments, hierarchy=json.loads(hierarchy.stdout),
                directed_difference_mm3=differences,
                all_pairwise_overlap_mm3=overlaps,
                physical_retention=physical, certificates=certificates,
                native_clearance_mm={name: [summary['clearance_checks'][name]['lower_bound_mm'],
                                            summary['clearance_checks'][name]['upper_bound_mm']]
                                     for name in expected_clearance})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=ROOT/'target/release/cad')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=False)
    cases = [case(args.cad.resolve(), args.out_dir, diameter, pitch)
             for diameter, pitch in [(12, 60), (14, 72), (10, 60)]]
    report = dict(valid=True, cases=cases,
                  scope='Nominal shaft and hub retention geometry and source size certificates; '
                        'ring elasticity, installation, strength and manufactured conformance unverified')
    (args.out_dir/'report.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
