"""Compare native unit-density volume properties with independent OCCT STEP import.

Usage: python check_properties_ocp.py body.step properties.json
The JSON comes from `cad properties recipe --output name`. No OCP production
runtime dependency. All inertia entries use the central, world-aligned frame.
Reference: https://occt3d.com/dev/doc/refman/html/class_g_prop___g_props.html
"""
import argparse
import json
import math
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader


def close(actual, expected):
    return math.isfinite(actual) and abs(actual-expected) <= max(1e-7, abs(expected)*1e-10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("step")
    parser.add_argument("properties")
    args = parser.parse_args()
    with open(args.properties, encoding="utf-8") as source:
        actual = json.load(source)
    reader = STEPControl_Reader()
    assert reader.ReadFile(args.step) == IFSelect_RetDone, "STEP parse failed"
    assert reader.TransferRoots() == 1, "expected one transferable root"
    shape = reader.OneShape()
    assert not shape.IsNull() and BRepCheck_Analyzer(shape).IsValid(), "invalid imported B-rep"
    properties = GProp_GProps()
    BRepGProp.VolumeProperties_s(shape, properties)
    volume = properties.Mass()
    point = properties.CentreOfMass()
    centroid = [point.X(), point.Y(), point.Z()]
    matrix = properties.MatrixOfInertia()
    inertia = [[matrix.Value(i+1,j+1) for j in range(3)] for i in range(3)]
    assert close(actual["volume"],volume), (actual["volume"],volume)
    assert all(close(a,b) for a,b in zip(actual["volume_centroid"],centroid)), (actual["volume_centroid"],centroid)
    errors = []
    for i in range(3):
        for j in range(3):
            assert close(actual["central_volume_inertia"][i][j],inertia[i][j]), (i,j,actual["central_volume_inertia"],inertia)
            errors.append(abs(actual["central_volume_inertia"][i][j]-inertia[i][j]))
    print(json.dumps({"valid":True,"occt_volume_mm3":volume,"occt_centroid_mm":centroid,
                      "occt_central_inertia_mm5":inertia,"max_inertia_absolute_error_mm5":max(errors)},indent=2))


if __name__ == "__main__":
    main()
