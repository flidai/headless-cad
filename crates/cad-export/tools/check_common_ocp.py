"""Compare native convex intersection STEP with independently computed OCCT Common.

Usage: python check_common_ocp.py left.step right.step common.step
Optional disposable cadquery-ocp environment only; never a kernel dependency.
Checks volume, area, bounds and both directed geometric-difference volumes.
"""
import argparse
import json
import math
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common, BRepAlgoAPI_Cut
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.BRepBndLib import BRepBndLib
from OCP.Bnd import Bnd_Box
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader


def load(filename):
    reader = STEPControl_Reader()
    assert reader.ReadFile(filename) == IFSelect_RetDone, "STEP parse failed"
    assert reader.TransferRoots() == 1, "expected one root"
    shape = reader.OneShape()
    assert not shape.IsNull() and BRepCheck_Analyzer(shape).IsValid(), "invalid input shape"
    return shape


def properties(shape):
    volume, area = GProp_GProps(), GProp_GProps()
    BRepGProp.VolumeProperties_s(shape, volume)
    BRepGProp.SurfaceProperties_s(shape, area)
    return volume.Mass(), area.Mass()


def bounds(shape):
    box = Bnd_Box()
    BRepBndLib.Add_s(shape, box)
    assert not box.IsVoid(), "expected nonempty intersection"
    lo, hi = box.CornerMin(), box.CornerMax()
    return [lo.X(), lo.Y(), lo.Z(), hi.X(), hi.Y(), hi.Z()]


def boolean(operator, left, right):
    operation = operator(left, right)
    operation.Build()
    assert operation.IsDone(), "independent OCCT boolean failed"
    result = operation.Shape()
    assert not result.IsNull() and BRepCheck_Analyzer(result).IsValid(), "invalid OCCT result"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("left")
    parser.add_argument("right")
    parser.add_argument("result")
    args = parser.parse_args()
    left, right, actual = map(load, (args.left, args.right, args.result))
    expected = boolean(BRepAlgoAPI_Common, left, right)
    av, aa = properties(actual)
    ev, ea = properties(expected)
    assert math.isfinite(av) and abs(av-ev) <= max(1e-7, abs(ev)*1e-9), (av, ev)
    assert math.isfinite(aa) and abs(aa-ea) <= max(1e-6, abs(ea)*1e-9), (aa, ea)
    ab, eb = bounds(actual), bounds(expected)
    assert all(abs(a-b) < 1e-6 for a,b in zip(ab,eb)), (ab,eb)
    extras = [properties(boolean(BRepAlgoAPI_Cut,a,b))[0] for a,b in [(actual,expected),(expected,actual)]]
    assert all(abs(v) <= max(1e-7, abs(ev)*1e-9) for v in extras), extras
    print(json.dumps({"valid": True, "native_volume_mm3": av, "occt_volume_mm3": ev,
                      "native_area_mm2": aa, "occt_area_mm2": ea,
                      "bounds_mm": ab, "directed_difference_volumes_mm3": extras}, indent=2))


if __name__ == "__main__":
    main()
