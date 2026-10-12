"""Independent analytic expectations for export_cones apex STEP fixtures.

Run export_cones into a directory, then pass that directory here using the
optional OCP development environment. No production kernel dependency on OCP.
"""
import json
import math
import pathlib
import sys

from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader


def close(a, b):
    return math.isfinite(a) and abs(a - b) <= max(1e-7, abs(b) * 1e-10)


def main():
    directory = pathlib.Path(sys.argv[1])
    pi = math.pi
    cone_volume = 256 * pi
    blind_volume = 1856 * pi
    blind_z = 17552 / 1856
    blind_ix = (2000 * (25 + 400 / 3) - 144 * (10.8 + 17**2)) * pi - blind_volume * blind_z**2
    fixtures = {
        "upper_apex": (cone_volume, 3, 15 * cone_volume, 19.2 * cone_volume),
        "lower_apex": (cone_volume, 9, 15 * cone_volume, 19.2 * cone_volume),
        "double_apex": (cone_volume, 6, 13.2 * cone_volume, 19.2 * cone_volume),
        "blind_apex": (blind_volume, blind_z, blind_ix, (2000 * 50 - 144 * 10.8) * pi),
    }
    for name, (volume, z, ix, iz) in fixtures.items():
        reader = STEPControl_Reader()
        assert reader.ReadFile(str(directory / f"{name}.step")) == IFSelect_RetDone
        assert reader.TransferRoots() == 1
        shape = reader.OneShape()
        assert not shape.IsNull() and BRepCheck_Analyzer(shape).IsValid(), name
        p = GProp_GProps()
        BRepGProp.VolumeProperties_s(shape, p)
        center = p.CentreOfMass()
        inertia = p.MatrixOfInertia()
        expected_center = [0, 0, z]
        expected_inertia = [[ix, 0, 0], [0, ix, 0], [0, 0, iz]]
        assert close(p.Mass(), volume), (name, p.Mass(), volume)
        assert all(close(a, b) for a, b in zip([center.X(), center.Y(), center.Z()], expected_center)), name
        native = json.loads((directory / f"{name}.properties.json").read_text())
        assert close(native["volume"], volume), name
        assert all(close(a, b) for a, b in zip(native["volume_centroid"], expected_center)), name
        for i in range(3):
            for j in range(3):
                assert close(inertia.Value(i + 1, j + 1), expected_inertia[i][j]), (name, i, j)
                assert close(native["central_volume_inertia"][i][j], expected_inertia[i][j]), (name, i, j)
        print(json.dumps({"fixture": name, "valid": True, "volume_mm3": volume, "centroid_z_mm": z}))


if __name__ == "__main__":
    main()
