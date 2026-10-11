"""Optional independent interoperability check; OCP is NOT a kernel dependency.

Create a disposable venv and install cadquery-ocp, then run:
  cargo run -p cad-export --example export_plate -- /tmp/cad-exports
  python crates/cad-export/tools/check_step_ocp.py /tmp/cad-exports/plate.step

This checks this example's known dimensions, topology, volume, and validity.
It is an interoperability smoke test, not complete STEP schema certification.
"""

import argparse
import json

from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_SOLID, TopAbs_VERTEX
from OCP.TopExp import TopExp_Explorer

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("step")
parser.add_argument("--volume", type=float, default=19200.0)
parser.add_argument("--bounds", nargs=6, type=float, default=[0, 0, 0, 80, 40, 6])
parser.add_argument("--solids", type=int, default=1)
parser.add_argument("--faces", type=int)
parser.add_argument("--edges", type=int)
parser.add_argument("--vertices", type=int)
args = parser.parse_args()
reader = STEPControl_Reader()
assert reader.ReadFile(args.step) == IFSelect_RetDone, "STEP parse failed"
assert reader.TransferRoots() == 1, "expected one transferable root"
shape = reader.OneShape()
assert not shape.IsNull(), "empty imported shape"
assert BRepCheck_Analyzer(shape).IsValid(), "invalid imported B-rep"

counts = {}
for name, kind, expected in [
    ("solids", TopAbs_SOLID, args.solids),
    ("faces", TopAbs_FACE, args.faces),
    ("edges", TopAbs_EDGE, args.edges),
    ("vertices", TopAbs_VERTEX, args.vertices),
]:
    explorer = TopExp_Explorer(shape, kind)
    unique = []
    while explorer.More():
        current = explorer.Current()
        if not any(current.IsSame(previous) for previous in unique):
            unique.append(current)
        explorer.Next()
    counts[name] = len(unique)
    if expected is not None:
        assert counts[name] == expected, (name, counts[name], expected)

properties = GProp_GProps()
BRepGProp.VolumeProperties_s(shape, properties)
assert abs(properties.Mass() - args.volume) < max(1e-6, abs(args.volume) * 1e-10), properties.Mass()
bounds = Bnd_Box()
BRepBndLib.Add_s(shape, bounds)
minimum, maximum = bounds.CornerMin(), bounds.CornerMax()
coordinates = (minimum.X(), minimum.Y(), minimum.Z(), maximum.X(), maximum.Y(), maximum.Z())
assert all(abs(actual - expected) < 1e-6 for actual, expected in zip(coordinates, args.bounds)), coordinates
print(json.dumps({"valid": True, "counts": counts, "volume_mm3": properties.Mass(), "bounds_mm": coordinates}, indent=2))
