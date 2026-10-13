#!/usr/bin/env python3
"""Independent OCCT acceptance for native line/arc blind profile pockets.

The reference cutters below are built from OCCT wires and prisms, independently
of the Rust kernel's profile and B-rep implementation. This script deliberately
does not alter the retained acceptance helpers from earlier increments.
"""

import argparse
from collections import defaultdict
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import shutil


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "mixed_slot_oracle", ROOT / "tools/check-mixed-slots.py"
)
mixed = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mixed)
k = mixed.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut  # noqa: E402
from OCP.BRepBuilderAPI import (  # noqa: E402
    BRepBuilderAPI_MakeEdge,
    BRepBuilderAPI_MakeFace,
    BRepBuilderAPI_MakeWire,
    BRepBuilderAPI_Transform,
)
from OCP.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder, BRepPrimAPI_MakePrism  # noqa: E402
from OCP.GC import GC_MakeArcOfCircle  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Surface  # noqa: E402
from OCP.BRepCheck import BRepCheck_Analyzer  # noqa: E402
from OCP.BRepGProp import BRepGProp  # noqa: E402
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane  # noqa: E402
from OCP.GProp import GProp_GProps  # noqa: E402
from OCP.gp import gp_Ax2, gp_Dir, gp_Pnt, gp_Vec  # noqa: E402
from OCP.TopAbs import TopAbs_FACE, TopAbs_SOLID  # noqa: E402
from OCP.TopExp import TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def close(actual, expected, message, absolute=2e-6):
    require(math.isfinite(actual) and abs(actual - expected) <= max(absolute, abs(expected) * 2e-11),
            f"{message}: {actual} != {expected}")


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_inputs():
    paths = list((ROOT / "tools").glob("*.py"))
    paths += list((ROOT / "crates/cad-export/tools").glob("*.py"))
    paths += [ROOT / "examples/motor_mount_housing.star"]
    return {str(path.relative_to(ROOT)): sha(path) for path in sorted(paths)}


def occt_profile_prism(start, segments, z0, z1):
    """Make an exact reference cutter from independent OCCT line/circle edges."""
    require(z1 > z0, "profile cutter has positive axial span")
    wire = BRepBuilderAPI_MakeWire()
    current = tuple(start)
    for index, segment in enumerate(segments):
        end = tuple(segment["end"])
        a = gp_Pnt(*current, z0)
        b = gp_Pnt(*end, z0)
        if segment["kind"] == "line_to":
            edge = BRepBuilderAPI_MakeEdge(a, b).Edge()
        else:
            require(segment["kind"] == "arc_to", f"segment {index} has supported analytic kind")
            center = tuple(segment["center"])
            r0 = math.dist(current, center)
            close(math.dist(end, center), r0, "arc endpoint radius", 1e-10)
            angle0 = math.atan2(current[1] - center[1], current[0] - center[0])
            angle1 = math.atan2(end[1] - center[1], end[0] - center[0])
            if segment.get("clockwise", False):
                sweep = -((angle0 - angle1) % (2 * math.pi))
            else:
                sweep = (angle1 - angle0) % (2 * math.pi)
            require(1e-9 < abs(sweep) < 2 * math.pi - 1e-9, "nondegenerate circular arc")
            mid = angle0 + sweep / 2
            middle = gp_Pnt(center[0] + r0 * math.cos(mid),
                            center[1] + r0 * math.sin(mid), z0)
            edge = BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(a, middle, b).Value()).Edge()
        wire.Add(edge)
        current = end
    require(math.dist(current, start) < 1e-9 and wire.IsDone(), "closed exact profile wire")
    face = BRepBuilderAPI_MakeFace(wire.Wire()).Face()
    return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, z1 - z0)).Shape()


def planar_path_area(start, segments):
    """Independent Green's-theorem line/arc integral, including concave arcs."""
    current = tuple(start)
    twice_area = 0.
    for segment in segments:
        end = tuple(segment["end"])
        if segment["kind"] == "line_to":
            twice_area += current[0] * end[1] - end[0] * current[1]
        else:
            center = tuple(segment["center"])
            a0 = math.atan2(current[1] - center[1], current[0] - center[0])
            a1 = math.atan2(end[1] - center[1], end[0] - center[0])
            sweep = (-((a0 - a1) % (2 * math.pi)) if segment.get("clockwise", False)
                     else ((a1 - a0) % (2 * math.pi)))
            radius = math.dist(current, center)
            twice_area += (center[0] * (end[1] - current[1])
                           - center[1] * (end[0] - current[0])
                           + radius * radius * sweep)
        current = end
    require(math.dist(current, start) < 1e-9, "area path closes")
    return twice_area / 2


def rounded_rectangle(x, y, width, height, radius):
    """Independent exact finite line/arc contour for OCCT cutter construction."""
    start = [x + radius, y]
    segments = [
        {"kind": "line_to", "end": [x + width - radius, y]},
        {"kind": "arc_to", "center": [x + width - radius, y + radius],
         "end": [x + width, y + radius]},
        {"kind": "line_to", "end": [x + width, y + height - radius]},
        {"kind": "arc_to", "center": [x + width - radius, y + height - radius],
         "end": [x + width - radius, y + height]},
        {"kind": "line_to", "end": [x + radius, y + height]},
        {"kind": "arc_to", "center": [x + radius, y + height - radius],
         "end": [x, y + height - radius]},
        {"kind": "line_to", "end": [x, y + radius]},
        {"kind": "arc_to", "center": [x + radius, y + radius], "end": start},
    ]
    return start, segments


def housing_params(**changes):
    values = dict(height=24, recess_width=52, recess_depth=12, cable_depth=18, pocket_side=1)
    values.update(changes)
    return values


def housing_moments_formula(p):
    outer_area = 120 * 100 - (4 - math.pi) * 6**2 - 4 * math.pi * 2.5**2
    through_area = 2 * (2 * 3 * 60 + math.pi * 3**2)
    motor_area = p["recess_width"] * 60 - (4 - math.pi) * 5**2
    cable_area = 12 * 32 - (4 - math.pi) * 3**2
    return (outer_area - through_area) * p["height"] - motor_area * p["recess_depth"] - cable_area * p["cable_depth"]


def through_capsule(solid, x, y, length, radius, height):
    z0 = -2
    cutter_height = height + 4
    solid = k.boolean(BRepAlgoAPI_Cut, solid, BRepPrimAPI_MakeBox(
        gp_Pnt(x - radius, y - length / 2, z0), 2 * radius, length, cutter_height).Shape())
    for sign in (-1, 1):
        solid = k.boolean(BRepAlgoAPI_Cut, solid, BRepPrimAPI_MakeCylinder(
            gp_Ax2(gp_Pnt(x, y + sign * length / 2, z0), gp_Dir(0, 0, 1)),
            radius, cutter_height).Shape())
    return solid


def reference_housing(p):
    """OCCT stock fillets, drilled bores, through slots, and exact wire cutters."""
    height = p["height"]
    solid = mixed.o.f.filleted([(0, 0), (120, 0), (120, 100), (0, 100)],
                               height, 6, [0, 1, 2, 3])
    for x in (12, 108):
        for y in (10, 90):
            solid = k.boolean(BRepAlgoAPI_Cut, solid, BRepPrimAPI_MakeCylinder(
                gp_Ax2(gp_Pnt(x, y, -2), gp_Dir(0, 0, 1)), 2.5, height + 4).Shape())
    for x in (12, 108):
        solid = through_capsule(solid, x, 50, 60, 3, height)
    top = p["pocket_side"] == 1
    for start, segments, depth in (
        (*rounded_rectangle(50 - p["recess_width"] / 2, 20,
                            p["recess_width"], 60, 5), p["recess_depth"]),
        (*rounded_rectangle(86, 34, 12, 32, 3), p["cable_depth"]),
    ):
        z0, z1 = (height - depth, height + 2) if top else (-2, depth)
        solid = k.boolean(BRepAlgoAPI_Cut, solid,
                          occt_profile_prism(start, segments, z0, z1))
    return solid


def reference_cover():
    solid = mixed.o.f.filleted([(0, 0), (120, 0), (120, 100), (0, 100)],
                               3, 6, [0, 1, 2, 3])
    for x in (12, 108):
        for y in (10, 90):
            solid = k.boolean(BRepAlgoAPI_Cut, solid, BRepPrimAPI_MakeCylinder(
                gp_Ax2(gp_Pnt(x, y, -2), gp_Dir(0, 0, 1)), 2.5, 7).Shape())
    return solid


def compare_properties(actual, expected, description):
    close(actual[0], expected[0], description + " volume", 2e-5)
    for i in range(3):
        close(actual[1][i], expected[1][i], description + " centroid")
        for j in range(3):
            close(actual[2][i][j], expected[2][i][j],
                  description + " central inertia", 2e-4)


def assert_raw_oriented_closed(path, description):
    """Check physical native B-rep edge incidence, independent of STEP healing."""
    model = json.loads(Path(path).read_text())["model"]
    uses = defaultdict(list)
    for face_id, face in enumerate(model["faces"]):
        for loop_id in face["loops"]:
            for use in model["loops"][loop_id]["edges"]:
                uses[use["edge"]].append((face_id, use["reversed"]))
    require(len(uses) == len(model["edges"]),
            description + " all native edges belong to oriented face loops")
    bad = [(edge_id, witnesses) for edge_id, witnesses in uses.items()
           if len(witnesses) != 2 or witnesses[0][1] == witnesses[1][1]]
    require(not bad, description + f" each native edge has two opposite face uses: {bad[:5]}")
    return len(uses)


def directed_difference(left, right):
    return k.properties(k.boolean(BRepAlgoAPI_Cut, left, right))[0]


def exact_step_match(actual, reference, description):
    require(BRepCheck_Analyzer(actual).IsValid(), description + " OCCT-valid STEP")
    solids = TopExp_Explorer(actual, TopAbs_SOLID)
    count = 0
    while solids.More():
        count += 1
        solids.Next()
    require(count == 1, description + " contains one solid")
    compare_properties(k.properties(actual), k.properties(reference), description + " STEP")
    differences = [directed_difference(a, b) for a, b in
                   ((actual, reference), (reference, actual))]
    for difference in differences:
        close(difference, 0, description + " directed OCCT solid difference", 3e-6)
    return differences


def planar_floor_faces(shape, thickness):
    result = []
    faces = TopExp_Explorer(shape, TopAbs_FACE)
    while faces.More():
        face = TopoDS.Face(faces.Current())
        support = BRepAdaptor_Surface(face)
        if support.GetType() == GeomAbs_Plane:
            normal = support.Plane().Axis().Direction()
            if abs(abs(normal.Z()) - 1) < 1e-8:
                properties = GProp_GProps()
                BRepGProp.SurfaceProperties_s(face, properties)
                center = properties.CentreOfMass()
                if 1e-6 < center.Z() < thickness - 1e-6:
                    result.append((properties.Mass(), [center.X(), center.Y(), center.Z()]))
        faces.Next()
    return result


def arc_wall_topology(shape, profiles, pose=0):
    """Bind every authored circular segment to one actual trimmed STEP cylinder."""
    transform = k.pose_values(pose)[2]
    axis = [transform.Value(i, 3) for i in range(1, 4)]
    surfaces = []
    faces = TopExp_Explorer(shape, TopAbs_FACE)
    while faces.More():
        face = TopoDS.Face(faces.Current())
        support = BRepAdaptor_Surface(face)
        if support.GetType() == GeomAbs_Cylinder:
            cylinder = support.Cylinder()
            location = cylinder.Axis().Location()
            direction = cylinder.Axis().Direction()
            surfaces.append((
                cylinder.Radius(),
                [location.X(), location.Y(), location.Z()],
                [direction.X(), direction.Y(), direction.Z()],
                support.LastUParameter() - support.FirstUParameter(),
                support.LastVParameter() - support.FirstVParameter(),
            ))
        faces.Next()
    count = 0
    for start, segments, depth in profiles:
        current = start
        for segment in segments:
            if segment["kind"] == "arc_to":
                center = segment["center"]
                radius = math.dist(current, center)
                first = math.atan2(current[1] - center[1], current[0] - center[0])
                last = math.atan2(segment["end"][1] - center[1],
                                  segment["end"][0] - center[0])
                sweep = ((first - last) % (2 * math.pi) if segment.get("clockwise", False)
                         else (last - first) % (2 * math.pi))
                point = [sum(transform.Value(i + 1, j + 1) * [*center, 0][j]
                             for j in range(3)) + transform.Value(i + 1, 4)
                         for i in range(3)]
                matches = 0
                for r, location, direction, angular, axial in surfaces:
                    if abs(r - radius) > 2e-6:
                        continue
                    along = sum((point[i] - location[i]) * direction[i] for i in range(3))
                    transverse = math.sqrt(sum((point[i] - location[i] - along * direction[i])**2
                                               for i in range(3)))
                    if transverse < 2e-6:
                        close(abs(sum(axis[i] * direction[i] for i in range(3))), 1,
                              "STEP arc wall follows stock axis")
                        close(angular, sweep, "STEP arc wall true circular trim angle")
                        close(axial, depth, "STEP arc wall exact finite pocket depth")
                        matches += 1
                require(matches == 1,
                        "each circular source edge has exactly one physical trimmed STEP wall")
                count += 1
            current = segment["end"]
    return count


def parameter_args(parameters):
    return [item for key, value in sorted(parameters.items())
            for item in ("--param", f"{key}={value}")]


def check_authored_housing(cad, root, label, parameters):
    source = ROOT / "examples/motor_mount_housing.star"
    output = root / label
    values = housing_params(**parameters)
    args = parameter_args(parameters)
    manifest = mixed.o.command(cad, "build", source, "--out-dir", output, *args)
    mixed.o.command(cad, "verify", output)
    native = mixed.o.command(cad, "properties", source, "--output", "housing", *args)
    reference = reference_housing(values)
    imported = k.load(str(output / "housing.step"))
    raw_edges = assert_raw_oriented_closed(output / "housing.brep.json", "authored housing")
    assert_raw_oriented_closed(output / "first_stage.brep.json", "authored first profile cut")
    assert_raw_oriented_closed(output / "cover.brep.json", "authored cover")
    exact = k.properties(reference)
    close(exact[0], housing_moments_formula(values),
          "authored closed-form housing volume", 2e-5)
    compare_properties((native["volume"], native["volume_centroid"],
                        native["central_volume_inertia"]), exact, "authored native")
    differences = exact_step_match(imported, reference, "authored housing")
    cover = k.load(str(output / "cover.step"))
    cover_differences = exact_step_match(cover, reference_cover(), "authored cover")
    require(BRepCheck_Analyzer(imported).IsValid(), "housing valid closed native STEP")
    floors = planar_floor_faces(imported, values["height"])
    require(len(floors) == 2, "two actual planar interior STEP floors")
    side = values["pocket_side"]
    expected_floors = [
        (values["recess_width"] * 60 - (4 - math.pi) * 25,
         values["recess_depth"] if side == 0 else values["height"] - values["recess_depth"]),
        (12 * 32 - (4 - math.pi) * 9,
         values["cable_depth"] if side == 0 else values["height"] - values["cable_depth"]),
    ]
    for area, z in expected_floors:
        require(sum(abs(value - area) < 2e-5 and abs(point[2] - z) < 2e-6
                    for value, point in floors) == 1,
                "each STEP floor has independent exact area and axial depth")
    wall_arc_count = arc_wall_topology(imported, [
        (*rounded_rectangle(50 - values["recess_width"] / 2, 20,
                            values["recess_width"], 60, 5), values["recess_depth"]),
        (*rounded_rectangle(86, 34, 12, 32, 3), values["cable_depth"]),
    ])
    require(wall_arc_count == 8, "eight actual quarter-cylinder pocket wall trims")
    faces = manifest["face_assertions"]
    for name, count in (("motor_floor_survives", 1), ("motor_walls_survive", 8),
                        ("left_slot_survives", 4), ("right_slot_survives", 4)):
        require(faces[name]["status"] == "resolved" and len(faces[name]["witnesses"]) == count,
                name + " source-bound native face lineage")
    witness = faces["motor_floor_survives"]["witnesses"][0]
    require(witness["anchor_faces"][0] != witness["target_face"],
            "first profile floor FaceId remaps after second pocket walls")
    final_model = json.loads((output / "housing.brep.json").read_text())["model"]
    native_floor = final_model["faces"][witness["target_face"]]["surface"]
    require(native_floor["kind"] == "plane", "remapped first floor has planar native support")
    close(native_floor["geometry"]["origin"][2], expected_floors[0][1],
          "remapped first floor physical axial support")
    stock_area = (120 * 100 - (4 - math.pi) * 36 - 4 * math.pi * 2.5**2
                  - 2 * (360 + math.pi * 9))
    section_expected = {
        "both_cavities": (stock_area - expected_floors[0][0] - expected_floors[1][0], 9),
        "cable_only": (stock_area - expected_floors[1][0], 8),
        "remaining_stock": (stock_area, 7),
    }
    for name, (area, loops) in section_expected.items():
        report = json.loads((output / f"section-{name}.json").read_text())
        section = report["section"]
        close(section["area"], area, name + " independently integrated area", 2e-5)
        require(len(section["loops"]) == loops, name + " exact material loop count")
    dimensions = manifest["datum_dimensions"]
    for name, length in (("motor_depth", values["recess_depth"]),
                         ("cable_depth", values["cable_depth"]),
                         ("remaining_web", values["height"] - values["cable_depth"])):
        require(dimensions[name]["status"] == "passed", name + " native datum proof")
        close(dimensions[name]["value_mm"], length, name + " actual floor spacing")
    drawing = (output / "drawing-motor_housing_machining.svg").read_text()
    for name in ("motor_depth", "cable_depth", "remaining_web"):
        require(f'data-dimension="{name}"' in drawing, name + " drawing table cross-reference")
    k.mesh(output / "housing.stl", exact[0])
    k.mesh(output / "cover.stl", k.properties(cover)[0])
    capture = root / (label + "-captured")
    capture.mkdir()
    captured_source = capture / source.name
    shutil.copyfile(output / "recipe.star", captured_source)
    require(sha(captured_source) == sha(source), "captured authored source bytes match original")
    replay = root / (label + "-replay")
    repeated = mixed.o.command(cad, "build", captured_source, "--out-dir", replay, *args)
    require(manifest["feature_hashes"] == repeated["feature_hashes"],
            "captured profile source reproduces native feature hashes")
    require(manifest["recipe_context_hash"] == repeated["recipe_context_hash"],
            "captured original filename preserves exact source-context binding")
    for artifact in ("housing.step", "housing.stl", "housing.brep.json", "cover.step",
                     "first_stage.step", "drawing-motor_housing_machining.svg"):
        require((output / artifact).read_bytes() == (replay / artifact).read_bytes(),
                "captured profile source reproduces " + artifact)
    return {"case": label, "parameters": parameters, "volume_mm3": exact[0],
            "centroid_mm": exact[1], "central_inertia_mm5": exact[2],
            "directed_difference_mm3": differences,
            "cover_directed_difference_mm3": cover_differences,
            "trimmed_arc_walls": wall_arc_count,
            "raw_oriented_edges": raw_edges,
            "floor_face_lineage": witness,
            "floor_area_and_z": expected_floors,
            "native_dimensions_mm": {name: dimensions[name]["value_mm"]
                                     for name in ("motor_depth", "cable_depth", "remaining_web")}}


def generated_config(**changes):
    values = dict(cap="end_cap", second="rounded", pose=0, posed_input=False,
                  reverse=False, first_depth=6, second_depth=13)
    values.update(changes)
    return values


def generated_profiles(p):
    rectangle = ([30, 30], [
        {"kind": "line_to", "end": [50, 30]},
        {"kind": "line_to", "end": [50, 50]},
        {"kind": "line_to", "end": [30, 50]},
        {"kind": "line_to", "end": [30, 30]},
    ])
    if p["second"] == "rounded":
        second = rounded_rectangle(90, 29, 24, 22, 4)
    elif p["second"] == "polygon32":
        second = extra_polygon((102, 40), 32)
        second = ([102 + (second[0][0] - 102) * 3,
                   40 + (second[0][1] - 40) * 3],
                  [{**segment, "end": [102 + (segment["end"][0] - 102) * 3,
                                          40 + (segment["end"][1] - 40) * 3]}
                   for segment in second[1]])
    else:
        require(p["second"] == "concave", "known generated profile family")
        # Counterclockwise outline with a genuine inward, clockwise circular arc.
        second = ([90, 28], [
            {"kind": "line_to", "end": [116, 28]},
            {"kind": "line_to", "end": [116, 52]},
            {"kind": "line_to", "end": [108, 52]},
            {"kind": "line_to", "end": [108, 44]},
            {"kind": "arc_to", "center": [104, 44], "end": [104, 40],
             "clockwise": True},
            {"kind": "line_to", "end": [90, 40]},
            {"kind": "line_to", "end": [90, 28]},
        ])
    return {"rectangle": (*rectangle, p["first_depth"]),
            "second": (*second, p["second_depth"])}


def extra_polygon(center, count):
    vertices = [[center[0] + 3 * math.cos(2 * math.pi * j / count),
                 center[1] + 3 * math.sin(2 * math.pi * j / count)]
                for j in range(count)]
    return (vertices[0], [{"kind": "line_to", "end": vertex}
                          for vertex in vertices[1:] + vertices[:1]])


def generated_source(p, invalid=None):
    profiles = generated_profiles(p)
    if invalid in ("contact", "overlap"):
        x = 50 if invalid == "contact" else 49
        start = [x, 35]
        segments = [{"kind": "line_to", "end": end} for end in
                    ([x + 16, 35], [x + 16, 45], [x, 45], start)]
        profiles["second"] = (start, segments, p["second_depth"])
    if invalid == "self_intersection":
        start = [90, 28]
        segments = [{"kind": "line_to", "end": end} for end in
                    ([112, 52], [90, 52], [112, 28], start)]
        profiles["second"] = (start, segments, p["second_depth"])
    if invalid == "too_many_segments":
        vertices = [[100 + 8 * math.cos(2 * math.pi * i / 33),
                     40 + 8 * math.sin(2 * math.pi * i / 33)] for i in range(33)]
        profiles["second"] = (vertices[0],
                              [{"kind": "line_to", "end": vertex}
                               for vertex in vertices[1:] + vertices[:1]],
                              p["second_depth"])
    order = ("second", "rectangle") if p["reverse"] else ("rectangle", "second")
    q, offset, transform = k.pose_values(p["pose"])
    cap = p["cap"]
    opening = 24 if cap == "end_cap" else 0
    origin = [0, 0, opening]
    frame_x = [1, 0, 0]
    frame_y = [0, 1, 0]
    if p["posed_input"]:
        frame_x = [transform.Value(i, 1) for i in range(1, 4)]
        frame_y = [transform.Value(i, 2) for i in range(1, 4)]
        origin = [sum(transform.Value(i + 1, j + 1) * origin[j]
                      for j in range(3)) + transform.Value(i + 1, 4)
                  for i in range(3)]
    lines = [
        'stock = extrude(id="stock", profile=polygon(id="stock_profile", '
        'points=[[0,0],[160,0],[160,80],[0,80]], circular_holes=['
        '{"center":[10,10],"radius":3},{"center":[150,10],"radius":3},'
        '{"center":[10,70],"radius":3},{"center":[150,70],"radius":3}]), height=24)',
        'through = slot_pattern(id="through", body=stock, frame_origin=[0,0,0], '
        'slots=[{"id":"left","center":[16,40],"direction":[0,1],'
        '"centerline_length":36,"radius":3},'
        '{"id":"right","center":[144,40],"direction":[0,1],'
        '"centerline_length":36,"radius":3}])',
    ]
    for name, (start, segments, _) in profiles.items():
        if name == "rectangle":
            lines.append('rectangle_profile = polygon(id="rectangle_profile", '
                         'points=[[30,30],[50,30],[50,50],[30,50]])')
        else:
            lines.append(f'second_profile = path(id="second_profile", '
                         f'start={start!r}, segments={segments!r})')
    body = "through"
    if p["posed_input"]:
        lines.append(f'posed = transform(id="posed", body=through, '
                     f'rotation={q}, offset={offset})')
        body = "posed"
    for index, name in enumerate(order):
        profile = "rectangle_profile" if name == "rectangle" else "second_profile"
        depth = profiles[name][2]
        if index == 1 and invalid == "full_web":
            depth = 24
        local_cap = cap
        local_origin = origin[:]
        if index == 1 and invalid == "opposite_cap":
            local_cap = "start_cap" if cap == "end_cap" else "end_cap"
            local_origin[2] = 0 if cap == "end_cap" else 24
        if index == 1 and invalid == "wrong_frame":
            local_origin[2] += .2
        if index == 1 and invalid == "stale_cap":
            local_cap = "missing_cap"
        lines.append(f'cut_{index} = blind_profile(id="cut_{index}", body={body}, '
                     f'profile={profile}, from_feature=stock, cap="{local_cap}", '
                     f'depth={depth}, frame_origin={local_origin}, '
                     f'frame_x_direction={frame_x}, frame_y_direction={frame_y})')
        body = f"cut_{index}"
        if index == 0:
            lines.append('output(name="first_stage", feature=cut_0)')
    if invalid in ("fifth", "aggregate_segments", "aggregate_at_limit"):
        # Extra noncontacting profiles isolate the count/segment budget from
        # geometry validity. Four 17+ segment pockets can exceed 64 overall.
        count = 3 if invalid == "fifth" else 2
        for extra in range(count):
            center = (65 + 10 * (extra % 2), 30 + 20 * (extra // 2))
            if invalid in ("aggregate_segments", "aggregate_at_limit"):
                start, segments = extra_polygon(
                    center, 27 if invalid == "aggregate_segments" else 26)
            else:
                start = [center[0] - 3, center[1] - 3]
                segments = [{"kind": "line_to", "end": point} for point in
                            ([center[0] + 3, center[1] - 3],
                             [center[0] + 3, center[1] + 3],
                             [center[0] - 3, center[1] + 3], start)]
            lines.append(f'extra_profile_{extra} = path(id="extra_profile_{extra}", '
                         f'start={start!r}, segments={segments!r})')
            lines.append(f'extra_cut_{extra} = blind_profile(id="extra_cut_{extra}", '
                         f'body={body}, profile=extra_profile_{extra}, from_feature=stock, '
                         f'cap="{cap}", depth=8, frame_origin={origin}, '
                         f'frame_x_direction={frame_x}, frame_y_direction={frame_y})')
            body = f"extra_cut_{extra}"
    lines.append('assert_faces(id="first_floor_survives", from_feature=cut_0, '
                 'role="pocket_floor", in_feature=cut_1, count=1)')
    lines.append('assert_faces(id="first_walls_survive", from_feature=cut_0, '
                 f'role="pocket_walls", in_feature=cut_1, count={len(profiles[order[0]][1])})')
    lines.append('assert_faces(id="second_walls", from_feature=cut_1, '
                 f'role="pocket_walls", in_feature=cut_1, count={len(profiles[order[1]][1])})')
    lines.append(f'assert_faces(id="through_walls_survive", from_feature=through, '
                 f'role="slot_left_walls", in_feature={body}, count=4)')
    lines.append(f'assert_faces(id="first_floor_final", from_feature=cut_0, '
                 f'role="pocket_floor", in_feature={body}, count=1)')
    if not p["posed_input"]:
        lines.append(f'finished = transform(id="finished", body={body}, '
                     f'rotation={q}, offset={offset})')
        body = "finished"
    lines.append(f'output(name="finished", feature={body})')
    return "\n".join(lines) + "\n"


def generated_reference(p):
    solid = generated_stock()
    for start, segments, depth in generated_profiles(p).values():
        z0, z1 = (24 - depth, 26) if p["cap"] == "end_cap" else (-2, depth)
        solid = k.boolean(BRepAlgoAPI_Cut, solid,
                          occt_profile_prism(start, segments, z0, z1))
    return BRepBuilderAPI_Transform(solid, k.pose_values(p["pose"])[2], True).Shape()


def generated_stock():
    solid = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 160, 80, 24).Shape()
    for x in (10, 150):
        for y in (10, 70):
            solid = k.boolean(BRepAlgoAPI_Cut, solid, BRepPrimAPI_MakeCylinder(
                gp_Ax2(gp_Pnt(x, y, -2), gp_Dir(0, 0, 1)), 3, 28).Shape())
    for x in (16, 144):
        solid = through_capsule(solid, x, 40, 36, 3, 24)
    return solid


def generated_first_stage_reference(p):
    name = "second" if p["reverse"] else "rectangle"
    start, segments, depth = generated_profiles(p)[name]
    solid = generated_stock()
    z0, z1 = (24 - depth, 26) if p["cap"] == "end_cap" else (-2, depth)
    solid = k.boolean(BRepAlgoAPI_Cut, solid,
                      occt_profile_prism(start, segments, z0, z1))
    return (BRepBuilderAPI_Transform(solid, k.pose_values(p["pose"])[2], True).Shape()
            if p["posed_input"] else solid)


def reference_at_segment_limit():
    p = generated_config()
    solid = generated_reference(p)
    for center in ((65, 30), (75, 30)):
        start, segments = extra_polygon(center, 26)
        solid = k.boolean(BRepAlgoAPI_Cut, solid,
                          occt_profile_prism(start, segments, 16, 26))
    return solid


def check_segment_budget(cad, root):
    source = root / "segment_budget.star"
    source.write_text(generated_source(generated_config(), "aggregate_at_limit"))
    output = root / "segment_budget"
    manifest = mixed.o.command(cad, "build", source, "--out-dir", output)
    mixed.o.command(cad, "verify", output)
    actual = k.load(str(output / "finished.step"))
    raw_edges = assert_raw_oriented_closed(output / "finished.brep.json",
                                           "64-segment four-pocket final")
    reference = reference_at_segment_limit()
    differences = exact_step_match(actual, reference, "exact 64-segment budget")
    exact = k.properties(reference)
    native = mixed.o.command(cad, "properties", source, "--output", "finished")
    compare_properties((native["volume"], native["volume_centroid"],
                        native["central_volume_inertia"]), exact,
                       "64-segment native four-pocket moments")
    through = manifest["face_assertions"]["through_walls_survive"]
    require(through["status"] == "resolved" and len(through["witnesses"]) == 4,
            "full-budget model retains true through walls")
    floor = manifest["face_assertions"]["first_floor_final"]
    require(floor["status"] == "resolved" and len(floor["witnesses"]) == 1,
            "first floor survives all four serial profile cuts")
    first_floor = floor["witnesses"][0]
    require(first_floor["anchor_faces"][0] != first_floor["target_face"],
            "first floor remaps through two later 26-wall pockets")
    p = generated_config()
    rectangle_area = planar_path_area(*generated_profiles(p)["rectangle"][:2])
    rounded_area = planar_path_area(*generated_profiles(p)["second"][:2])
    extra_area = planar_path_area(*extra_polygon((65, 30), 26))
    base_area = 160 * 80 - 4 * math.pi * 9 - 2 * (216 + math.pi * 9)
    strata = [(3, base_area - rectangle_area - rounded_area - 2 * extra_area, 11),
              (7, base_area - rounded_area - 2 * extra_area, 10),
              (10, base_area - rounded_area, 8),
              (18, base_area, 7)]
    for depth, area, loops in strata:
        report = mixed.o.command(cad, "section", source, "--output", "finished",
                                 "--origin", f"0,0,{24-depth}")
        close(report["section"]["area"], area,
              "64-segment exact section material area", 2e-5)
        require(len(report["section"]["loops"]) == loops,
                "64-segment section has each active native pocket loop")
    k.mesh(output / "finished.stl", exact[0])
    capture = root / "segment_budget-captured"
    capture.mkdir()
    captured_source = capture / source.name
    shutil.copyfile(output / "recipe.star", captured_source)
    replay = root / "segment_budget-replay"
    repeated = mixed.o.command(cad, "build", captured_source, "--out-dir", replay)
    require(manifest["feature_hashes"] == repeated["feature_hashes"] and
            manifest["recipe_context_hash"] == repeated["recipe_context_hash"],
            "64-segment captured source and feature identity replay exactly")
    for artifact in ("finished.step", "finished.stl", "finished.brep.json"):
        require((output / artifact).read_bytes() == (replay / artifact).read_bytes(),
                "64-segment captured solid replay " + artifact)
    return {"segments_per_profile": [4, 8, 26, 26],
            "profiles": 4, "total_segments": 64,
            "raw_oriented_edges": raw_edges,
            "volume_mm3": exact[0], "directed_difference_mm3": differences,
            "first_floor_remap": first_floor}


def pure_rectangle_source(cap, depth):
    opening = 12 if cap == "end_cap" else 0
    return "\n".join((
        'stock = extrude(id="stock", profile=rectangle(id="stock_profile", width=50, depth=40), height=12)',
        'pocket_profile = polygon(id="pocket_profile", '
        'points=[[10,10],[30,10],[30,25],[10,25]])',
        f'finished = blind_profile(id="rectangular_pocket", body=stock, '
        f'profile=pocket_profile, from_feature=stock, cap="{cap}", '
        f'depth={depth}, frame_origin=[0,0,{opening}])',
        'output(name="finished", feature=finished)',
        'assert_faces(id="floor", from_feature=finished, role="pocket_floor", '
        'in_feature=finished, count=1)',
        'assert_faces(id="walls", from_feature=finished, role="pocket_walls", '
        'in_feature=finished, count=4)',
    )) + "\n"


def check_pure_rectangle(cad, root, cap, depth):
    label = "pure_rectangle_top" if cap == "end_cap" else "pure_rectangle_bottom"
    source = root / (label + ".star")
    source.write_text(pure_rectangle_source(cap, depth))
    output = root / label
    manifest = mixed.o.command(cad, "build", source, "--out-dir", output)
    mixed.o.command(cad, "verify", output)
    z0, z1 = (12 - depth, 14) if cap == "end_cap" else (-2, depth)
    reference = k.boolean(BRepAlgoAPI_Cut,
                          BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 50, 40, 12).Shape(),
                          BRepPrimAPI_MakeBox(gp_Pnt(10, 10, z0), 20, 15,
                                              z1 - z0).Shape())
    actual = k.load(str(output / "finished.step"))
    differences = exact_step_match(actual, reference, label)
    native = mixed.o.command(cad, "properties", source, "--output", "finished")
    exact = k.properties(reference)
    close(exact[0], 50 * 40 * 12 - 20 * 15 * depth,
          label + " closed-form volume")
    compare_properties((native["volume"], native["volume_centroid"],
                        native["central_volume_inertia"]), exact,
                       label + " native moments")
    raw_edges = assert_raw_oriented_closed(output / "finished.brep.json", label)
    floor = manifest["face_assertions"]["floor"]
    walls = manifest["face_assertions"]["walls"]
    require(floor["status"] == "resolved" and len(floor["witnesses"]) == 1,
            label + " exact native floor role")
    require(walls["status"] == "resolved" and len(walls["witnesses"]) == 4,
            label + " exact four native planar wall roles")
    model = json.loads((output / "finished.brep.json").read_text())["model"]
    face_id = floor["witnesses"][0]["target_face"]
    floor_surface = model["faces"][face_id]["surface"]
    require(floor_surface["kind"] == "plane", label + " actual analytic floor plane")
    close(floor_surface["geometry"]["origin"][2],
          depth if cap == "start_cap" else 12 - depth,
          label + " physical floor depth")
    close(floor_surface["geometry"]["normal"][2],
          -1 if cap == "start_cap" else 1,
          label + " outward floor support orientation")
    floors = planar_floor_faces(actual, 12)
    require(len(floors) == 1 and abs(floors[0][0] - 300) < 2e-6,
            label + " real STEP floor area")
    for active, axial, expected_area, loops in (
        (True, depth / 2, 50 * 40 - 300, 2),
        (False, (depth + 12) / 2, 50 * 40, 1),
    ):
        z = axial if cap == "start_cap" else 12 - axial
        section = mixed.o.command(cad, "section", source, "--output", "finished",
                                  "--origin", f"0,0,{z}")["section"]
        close(section["area"], expected_area, label + " section material area")
        require(len(section["loops"]) == loops,
                label + " section pocket active=" + str(active))
    k.mesh(output / "finished.stl", exact[0])
    capture = root / (label + "-captured")
    capture.mkdir()
    captured_source = capture / source.name
    shutil.copyfile(output / "recipe.star", captured_source)
    replay = root / (label + "-replay")
    repeated = mixed.o.command(cad, "build", captured_source, "--out-dir", replay)
    require(manifest["feature_hashes"] == repeated["feature_hashes"] and
            manifest["recipe_context_hash"] == repeated["recipe_context_hash"],
            label + " captured source identity replay")
    for artifact in ("finished.step", "finished.stl", "finished.brep.json"):
        require((output / artifact).read_bytes() == (replay / artifact).read_bytes(),
                label + " exact captured solid replay " + artifact)
    return {"case": label, "cap": cap, "depth_mm": depth,
            "volume_mm3": exact[0], "raw_oriented_edges": raw_edges,
            "directed_difference_mm3": differences}


def check_generated(cad, root, label, p):
    source = root / (label + ".star")
    source.write_text(generated_source(p))
    output = root / label
    manifest = mixed.o.command(cad, "build", source, "--out-dir", output)
    mixed.o.command(cad, "verify", output)
    reference = generated_reference(p)
    imported = k.load(str(output / "finished.step"))
    raw_edges = assert_raw_oriented_closed(output / "finished.brep.json", label + " final")
    assert_raw_oriented_closed(output / "first_stage.brep.json", label + " first stage")
    native = mixed.o.command(cad, "properties", source, "--output", "finished")
    exact = k.properties(reference)
    stage = k.load(str(output / "first_stage.step"))
    stage_reference = generated_first_stage_reference(p)
    stage_differences = exact_step_match(stage, stage_reference,
                                         label + " intermediate first pocket")
    k.mesh(output / "first_stage.stl", k.properties(stage_reference)[0])
    compare_properties((native["volume"], native["volume_centroid"],
                        native["central_volume_inertia"]), exact,
                       label + " native moments")
    differences = exact_step_match(imported, reference, label)
    arc_walls = arc_wall_topology(imported, list(generated_profiles(p).values()), p["pose"])
    require(arc_walls == {"rounded": 4, "concave": 1, "polygon32": 0}[p["second"]],
            "generated rounded or concave profile has exact circular STEP wall count")
    # True through wall IDs survive both profile operations without surrogate remap.
    through = manifest["face_assertions"]["through_walls_survive"]
    require(through["status"] == "resolved" and len(through["witnesses"]) == 4,
            "actual four through-slot wall supports survive")
    require(all(witness["anchor_faces"] == [witness["target_face"]]
                for witness in through["witnesses"]),
            "through-slot walls preserve original physical FaceIds")
    floor = manifest["face_assertions"]["first_floor_survives"]
    require(floor["status"] == "resolved" and len(floor["witnesses"]) == 1,
            "first profile pocket floor survives later cut")
    first_floor = floor["witnesses"][0]
    require(first_floor["anchor_faces"][0] != first_floor["target_face"],
            "first floor FaceId remapped after new walls")
    for assertion, name in (("first_walls_survive", "second" if p["reverse"] else "rectangle"),
                            ("second_walls", "rectangle" if p["reverse"] else "second")):
        selected = manifest["face_assertions"][assertion]
        require(selected["status"] == "resolved" and
                len(selected["witnesses"]) == len(generated_profiles(p)[name][1]),
                assertion + " has exact physical line/arc segment wall count")
    if p["pose"] == 0:
        model = json.loads((output / "finished.brep.json").read_text())["model"]
        surface = model["faces"][first_floor["target_face"]]["surface"]
        require(surface["kind"] == "plane", "remapped profile floor stays planar")
        name = "second" if p["reverse"] else "rectangle"
        depth = generated_profiles(p)[name][2]
        expected_z = depth if p["cap"] == "start_cap" else 24 - depth
        close(surface["geometry"]["origin"][2], expected_z,
              "first profile floor remains at exact original depth")
        profile_areas = {name: planar_path_area(start, segments)
                         for name, (start, segments, _) in generated_profiles(p).items()}
        stock_area = 160 * 80 - 4 * math.pi * 3**2 - 2 * (2 * 3 * 36 + math.pi * 3**2)
        depths = sorted({0, p["first_depth"], p["second_depth"], 24})
        for lo, hi in zip(depths, depths[1:]):
            axial = (lo + hi) / 2
            z = axial if p["cap"] == "start_cap" else 24 - axial
            active = [name for name, (_, _, depth) in generated_profiles(p).items()
                      if depth > axial]
            expected_area = stock_area - sum(profile_areas[name] for name in active)
            query = mixed.o.command(cad, "section", source, "--output", "finished",
                                    "--origin", f"0,0,{z}")
            close(query["section"]["area"], expected_area,
                  "exact native profile section stratum", 2e-5)
            require(len(query["section"]["loops"]) == 7 + len(active),
                    "native section retains through and active pocket loops")
        floors = planar_floor_faces(imported, 24)
        require(len(floors) == 2, "actual STEP has two finite-depth profile floors")
        for name, (_, _, depth) in generated_profiles(p).items():
            z = depth if p["cap"] == "start_cap" else 24 - depth
            require(sum(abs(area - profile_areas[name]) < 2e-5 and
                        abs(point[2] - z) < 2e-6 for area, point in floors) == 1,
                    "actual STEP floor area and depth match independently integrated profile")
    k.mesh(output / "finished.stl", exact[0])
    capture = root / (label + "-captured")
    capture.mkdir()
    captured_source = capture / source.name
    shutil.copyfile(output / "recipe.star", captured_source)
    require(sha(captured_source) == sha(source), "generated captured source bytes match original")
    replay = root / (label + "-replay")
    repeated = mixed.o.command(cad, "build", captured_source, "--out-dir", replay)
    require(manifest["feature_hashes"] == repeated["feature_hashes"],
            "generated captured profile source reproduces semantic hashes")
    require(manifest["recipe_context_hash"] == repeated["recipe_context_hash"],
            "generated captured source context remains exact")
    for artifact in ("finished.step", "finished.stl", "finished.brep.json",
                     "first_stage.step"):
        require((output / artifact).read_bytes() == (replay / artifact).read_bytes(),
                label + " byte-identical captured replay " + artifact)
    return {"case": label, "parameters": p, "volume_mm3": exact[0],
            "centroid_mm": exact[1], "central_inertia_mm5": exact[2],
            "directed_difference_mm3": differences,
            "first_stage_directed_difference_mm3": stage_differences,
            "trimmed_arc_walls": arc_walls,
            "raw_oriented_edges": raw_edges,
            "first_floor_remap": first_floor}


def check_refusals(cad, root):
    baseline = generated_config()
    source = root / "revisions.star"
    output = root / "preserved"
    cache = root / "revision-cache"
    source.write_text(generated_source(baseline))
    first = mixed.o.command(cad, "build", source, "--out-dir", output,
                            "--cache-dir", cache)
    initial_first_hash = first["feature_hashes"]["cut_0"]
    first_stage_step = sha(output / "first_stage.step")
    final_step = sha(output / "finished.step")
    revised = generated_config(second_depth=14)
    source.write_text(generated_source(revised))
    changed = mixed.o.command(cad, "build", source, "--out-dir", output,
                              "--cache-dir", cache)
    require(changed["feature_hashes"]["cut_0"] == initial_first_hash,
            "later profile depth edit reuses first native cut")
    require(changed["feature_hashes"]["cut_1"] != first["feature_hashes"]["cut_1"],
            "later profile depth edit invalidates second native cut")
    require(sha(output / "first_stage.step") == first_stage_step and
            sha(output / "finished.step") != final_step,
            "later profile depth edit preserves first STEP and changes final STEP")
    source.write_text(generated_source(baseline))
    restored = mixed.o.command(cad, "build", source, "--out-dir", output,
                               "--cache-dir", cache)
    require(restored["feature_hashes"] == first["feature_hashes"],
            "restored source reproduces original semantic hashes")
    preserved = mixed.o.inventory(output)
    refusals = []
    for invalid in ("contact", "overlap", "self_intersection",
                    "too_many_segments", "aggregate_segments", "fifth",
                    "stale_cap", "wrong_frame", "opposite_cap", "full_web"):
        source.write_text(generated_source(baseline, invalid))
        failure = mixed.o.command(cad, "build", source, "--out-dir", output,
                                  "--cache-dir", cache, success=False)
        require(failure["code"] in ("kernel_operation_failed", "invalid_polygon_profile"),
                invalid + " refuses by native profile or parse validity")
        require(mixed.o.inventory(output) == preserved,
                invalid + " failed source revision preserves entire publication")
        refusals.append({"case": invalid, "diagnostic": failure})
    mixed.o.command(cad, "verify", output)
    return refusals


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cad", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=False)
    cad = args.cad.resolve()
    binary_hash = sha(cad)
    script_hash = sha(__file__)
    source_inventory = pinned_inputs()
    generated = [check_generated(cad, args.out_dir, name, p) for name, p in (
        ("rectangle_rounded_top", generated_config()),
        ("rectangle_rounded_top_reversed", generated_config(reverse=True)),
        ("concave_top", generated_config(second="concave")),
        ("polygon32_top", generated_config(second="polygon32")),
        ("concave_bottom", generated_config(second="concave", cap="start_cap")),
        ("concave_oblique_after", generated_config(second="concave", pose=2)),
        ("concave_oblique_posed_input", generated_config(
            second="concave", pose=2, posed_input=True)),
    )]
    a = k.load(str(args.out_dir / "rectangle_rounded_top/finished.step"))
    b = k.load(str(args.out_dir / "rectangle_rounded_top_reversed/finished.step"))
    for left, right in ((a, b), (b, a)):
        close(directed_difference(left, right), 0,
              "reverse serial profile order preserves exact physical solid", 3e-6)
    authored = [check_authored_housing(cad, args.out_dir, name, p) for name, p in (
        ("housing_default", {}),
        ("housing_bottom", {"pocket_side": 0}),
        ("housing_wide_deep", {"height": 28, "recess_width": 58,
                               "recess_depth": 14, "cable_depth": 19}),
        ("housing_narrow_shallow", {"height": 22, "recess_width": 46,
                                    "recess_depth": 10, "cable_depth": 16}),
    )]
    pure_rectangles = [check_pure_rectangle(cad, args.out_dir, cap, depth)
                       for cap, depth in (("end_cap", 4), ("start_cap", 7))]
    budget = check_segment_budget(cad, args.out_dir)
    refusals = check_refusals(cad, args.out_dir)
    require(binary_hash == sha(cad) and script_hash == sha(__file__) and
            source_inventory == pinned_inputs(),
            "binary, oracle and all source inputs remain immutable during run")
    report = {"valid": True, "binary_sha256": binary_hash,
              "oracle_script_sha256": script_hash,
              "source_inventory_sha256": source_inventory,
              "python": platform.python_version(),
              "ocp": importlib.metadata.version("cadquery-ocp"),
              "generated_cases": generated, "authored_housing_cases": authored,
              "pure_rectangular_cases": pure_rectangles,
              "four_pocket_segment_budget": budget,
              "refusals": refusals,
              "scope": "Independent exact OCCT line/arc profile prisms, serial native pockets, source-bound floors/walls, STEP/STL/section/moment/replay/refusal verification."}
    (args.out_dir / "report.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(json.dumps({"valid": True, "generated_cases": len(generated),
                      "authored_housing_cases": len(authored),
                      "pure_rectangular_cases": len(pure_rectangles),
                      "segment_budget": budget["total_segments"],
                      "refusals": len(refusals),
                      "report": str(args.out_dir / "report.json")}))


if __name__ == "__main__":
    main()
