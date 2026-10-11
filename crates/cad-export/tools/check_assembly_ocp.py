"""Optional XCAF assembly interoperability check; OCP is NOT a runtime dependency.

Install cadquery-ocp in a disposable virtual environment, then run, for example:
  python crates/cad-export/tools/check_assembly_ocp.py mounting_array.step \
    --instances 24 --definitions 1 --solids 24 --volume 4992 \
    --bounds -6 0 0 110 52 4 --assembly-json mounting_array.assembly.json

Checks imported product structure and shared definitions as well as placed geometry.
An optional assembly JSON checks every occurrence name, part identity, and rigid
placement against the independently imported STEP document. When hierarchy is
present, checks every direct parent/child relationship, local placement, shared
subassembly definition, and composed leaf placement recursively. --instances and
--definitions count leaf occurrences and part definitions, respectively. This is an
interoperability regression check, not complete STEP schema certification.
"""

import argparse
import json
import math

from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.TCollection import TCollection_AsciiString, TCollection_ExtendedString
from OCP.TDataStd import TDataStd_Name
from OCP.TDF import TDF_Label, TDF_Tool
from OCP.TDocStd import TDocStd_Document
from OCP.TopAbs import TopAbs_SOLID
from OCP.TopExp import TopExp_Explorer
from OCP.TopLoc import TopLoc_Location
from OCP.XCAFDoc import XCAFDoc_DocumentTool

try:
    from OCP.TDF import TDF_LabelSequence as LabelSequence
except ImportError:
    from OCP.collections import Sequence_TDF_Label as LabelSequence


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def label_entry(label):
    entry = TCollection_AsciiString()
    TDF_Tool.Entry_s(label, entry)
    return entry.ToCString()


def label_name(label):
    name = TDataStd_Name()
    require(label.FindAttribute(TDataStd_Name.GetID_s(), name), "missing product name")
    return name.Get().ToExtString()


def close(actual, expected, tolerance=1e-6):
    return math.isfinite(actual) and abs(actual - expected) <= tolerance


def placement_matrix(transform):
    x, y, z, w = transform["rotation"]
    tx, ty, tz = transform["translation"]
    return [
        [1 - 2 * (y*y + z*z), 2 * (x*y - z*w), 2 * (x*z + y*w), tx],
        [2 * (x*y + z*w), 1 - 2 * (x*x + z*z), 2 * (y*z - x*w), ty],
        [2 * (x*z - y*w), 2 * (y*z + x*w), 1 - 2 * (x*x + y*y), tz],
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("step")
    parser.add_argument("--instances", type=int, required=True)
    parser.add_argument("--definitions", type=int, required=True)
    parser.add_argument("--roots", type=int, default=1)
    parser.add_argument("--solids", type=int)
    parser.add_argument("--volume", type=float)
    parser.add_argument("--bounds", nargs=6, type=float)
    parser.add_argument("--assembly-json")
    args = parser.parse_args()
    expected = None
    if args.assembly_json:
        with open(args.assembly_json, encoding="utf-8") as source:
            expected = json.load(source)
        require(args.roots == 1, "assembly JSON comparison supports one root")
    reader = STEPCAFControl_Reader()
    reader.SetNameMode(True)
    require(reader.ReadFile(args.step) == IFSelect_RetDone, "STEP parse failed")
    doc = TDocStd_Document(TCollection_ExtendedString("assembly oracle"))
    require(reader.Transfer(doc), "XCAF transfer failed")
    tool = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
    roots = LabelSequence()
    tool.GetFreeShapes(roots)
    require(roots.Length() == args.roots, f"expected {args.roots} roots, got {roots.Length()}")
    definitions = {}
    assembly_definitions = {}
    occurrences = {}
    definition_edges = {}
    volume = 0.0
    solids = 0
    bounds = Bnd_Box()
    root_names = []

    def matrix(location):
        transform = location.Transformation()
        return [[transform.Value(row, column) for column in range(1, 5)] for row in range(1, 4)]

    def walk(parent, path, placement, active):
        identity = label_entry(parent)
        require(identity not in active, "cyclic imported assembly")
        assembly_definitions[identity] = label_name(parent)
        components = LabelSequence()
        require(tool.GetComponents_s(parent, components), "cannot enumerate assembly components")
        names = set()
        edges = []
        for index in range(1, components.Length() + 1):
            component = components.Value(index)
            reference = TDF_Label()
            require(tool.GetReferredShape_s(component, reference), "component has no definition reference")
            name = label_name(component)
            require(name not in names, f"duplicate sibling occurrence name: {name}")
            names.add(name)
            local = tool.GetLocation_s(component)
            world = placement.Multiplied(local)
            child_path = path + [name]
            child_identity = label_entry(reference)
            nested = tool.IsAssembly_s(reference)
            edges.append({"id": name, "definition": child_identity,
                          "assembly": nested, "placement": matrix(local)})
            if nested:
                walk(reference, child_path, world, active | {identity})
            else:
                definitions[child_identity] = label_name(reference)
                key = tuple(child_path)
                require(key not in occurrences, f"duplicate occurrence path: {child_path}")
                occurrences[key] = {"definition": child_identity, "placement": matrix(world)}
        definition_edges[identity] = edges

    for root_index in range(1, roots.Length() + 1):
        root = roots.Value(root_index)
        require(tool.IsAssembly_s(root), "root is flattened geometry, not an assembly")
        root_names.append(label_name(root))
        walk(root, [], TopLoc_Location(), set())
        shape = tool.GetShape_s(root)
        require(not shape.IsNull(), "empty root shape")
        require(BRepCheck_Analyzer(shape).IsValid(), "invalid assembly geometry")
        properties = GProp_GProps()
        BRepGProp.VolumeProperties_s(shape, properties)
        volume += properties.Mass()
        BRepBndLib.Add_s(shape, bounds)
        explorer = TopExp_Explorer(shape, TopAbs_SOLID)
        while explorer.More():
            solids += 1
            explorer.Next()
    require(len(occurrences) == args.instances, f"instance count: {len(occurrences)}")
    require(len(definitions) == args.definitions, f"shared definition count: {len(definitions)}")
    require(not bounds.IsVoid(), "no placed geometry")
    minimum, maximum = bounds.CornerMin(), bounds.CornerMax()
    coordinates = [minimum.X(), minimum.Y(), minimum.Z(), maximum.X(), maximum.Y(), maximum.Z()]
    if args.solids is not None:
        require(solids == args.solids, f"placed solid count: {solids}")
    expected_volume = args.volume if args.volume is not None else (expected or {}).get("material_volume_mm3")
    if expected_volume is not None:
        require(close(volume, expected_volume, max(1e-6, abs(expected_volume) * 1e-10)), f"material volume: {volume}")
    if args.bounds is not None:
        require(all(close(a, b) for a, b in zip(coordinates, args.bounds)), f"placed bounds: {coordinates}")
    if expected:
        # The manifest transforms definition AABB corners. A rotated non-box can
        # occupy a strictly smaller AABB, so require containment, not equality.
        conservative = expected["bounds_mm"]
        require(all(coordinates[i] >= conservative["min"][i] - 1e-6 and
                    coordinates[i + 3] <= conservative["max"][i] + 1e-6
                    for i in range(3)), f"geometry exceeds conservative bounds: {coordinates}")
    if expected:
        require(root_names == [expected["assembly_id"]], f"assembly names: {root_names}")
        instances = {tuple(expected.get("instance_paths", {}).get(instance["id"], [instance["id"]])): instance for instance in expected["instances"]}
        require(len(instances) == len(expected["instances"]), "duplicate expected instance ID")
        require(instances.keys() == occurrences.keys(), "occurrence names differ from assembly JSON")
        if "hierarchy" in expected:
            by_name = {name: identity for identity, name in assembly_definitions.items()}
            require(len(by_name) == len(assembly_definitions), "duplicated assembly definitions")
            require(by_name.keys() == expected["hierarchy"].keys(), "assembly definition graph differs")
            for name, definition in expected["hierarchy"].items():
                actual_edges = {edge["id"]: edge for edge in definition_edges[by_name[name]]}
                wanted_edges = [(i, False) for i in definition["instances"]] + [(i, True) for i in definition.get("assemblies", [])]
                require(actual_edges.keys() == {i["id"] for i, _ in wanted_edges}, f"direct children differ: {name}")
                for instance, nested in wanted_edges:
                    edge = actual_edges[instance["id"]]
                    require(edge["assembly"] == nested, "assembly occurrence was flattened")
                    if nested:
                        require(edge["definition"] == by_name[instance["assembly"]], "subassembly definition not reused")
                    else:
                        require(definitions[edge["definition"]] == instance["part"], "leaf part identity differs")
                    wanted = placement_matrix(instance["transform"])
                    require(all(close(a, b, 1e-12) for row, target in zip(edge["placement"], wanted)
                                for a, b in zip(row, target)), f"local placement differs: {name}/{instance['id']}")
        hash_to_reference = {}
        reference_to_hash = {}
        for name, instance in instances.items():
            actual = occurrences[name]
            wanted = placement_matrix(instance["transform"])
            require(all(close(a, b, 1e-12) for row, target in zip(actual["placement"], wanted)
                        for a, b in zip(row, target)), f"incorrect placement for {name}: {actual['placement']}")
            content_hash = expected["definitions"][instance["part"]]["content_hash"]
            identity = actual["definition"]
            require(hash_to_reference.setdefault(content_hash, identity) == identity,
                    f"definition duplicated instead of reused for {name}")
            require(reference_to_hash.setdefault(identity, content_hash) == content_hash,
                    f"distinct definitions incorrectly share geometry for {name}")
            aliases = [part for part, data in expected["definitions"].items() if data["content_hash"] == content_hash]
            require(definitions[identity] in aliases, f"incorrect definition product name: {definitions[identity]}")
    print(json.dumps({"valid": True, "roots": root_names, "instances": len(occurrences),
                      "definitions": definitions, "assembly_definitions": assembly_definitions, "solids": solids,
                      "material_volume_mm3": volume, "bounds_mm": coordinates,
                      "definition_relationships": sum(map(len, definition_edges.values())),
                      "placements_checked": bool(expected),
                      "hierarchy_checked": bool(expected and "hierarchy" in expected)}, indent=2))


if __name__ == "__main__":
    main()
