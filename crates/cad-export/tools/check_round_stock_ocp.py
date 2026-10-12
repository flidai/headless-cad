"""Independent exact cylinder integrals for drilling disk and annular stock."""
import json
import pathlib
import sys
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from check_arc_prisms_ocp import mesh
from check_blind_pallet_ocp import cylinder, combine
from check_concave_holes_ocp import transform
from check_constrained_fixture_ocp import body_properties, require
from check_counterbores_ocp import verify

CASES = ['disk_through', 'disk_blind', 'disk_stepped',
         'annulus_through', 'annulus_blind', 'annulus_stepped']


def expected(name):
    pieces = [cylinder(30, 16, [0, 0, 8])]
    if name.startswith('annulus'):
        pieces.append(cylinder(8, 16, [0, 0, 8], -1))
    if name.endswith('through'):
        pieces.append(cylinder(3, 16, [18, 0, 8], -1))
    elif name.endswith('blind'):
        pieces.append(cylinder(2, 5, [-18, 0, 13.5], -1))
    else:
        pieces.extend([cylinder(3, 12, [18, 0, 6], -1),
                       cylinder(5, 4, [18, 0, 14], -1),
                       cylinder(2, 5, [-18, 0, 13.5], -1)])
    return combine(pieces)


def cylinder_radii(path):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone, 'STEP import')
    require(reader.TransferRoots() == 1, 'STEP root count')
    explorer = TopExp_Explorer(reader.OneShape(), TopAbs_FACE)
    inward, outward = [], []
    while explorer.More():
        face = TopoDS.Face(explorer.Current())
        surface = BRepAdaptor_Surface(face)
        if surface.GetType() == GeomAbs_Cylinder:
            (inward if face.Orientation() == TopAbs_REVERSED else outward).append(surface.Cylinder().Radius())
        explorer.Next()
    return inward, outward


def cylinder_faces(path, name):
    inward, outward = cylinder_radii(path)
    wanted = [3] if name.endswith('through') else [2] if name.endswith('blind') else [2, 3, 5]
    if name.startswith('annulus'):
        wanted.append(8)
    require(len(outward) == 2 and all(abs(r-30) < 1e-8 for r in outward), 'two outer cylinder patches')
    require(len(inward) == len(wanted) and all(abs(a-b) < 1e-8 for a,b in zip(sorted(inward), sorted(wanted))), 'inward stock and machining cylinders')


def main():
    directory = pathlib.Path(sys.argv[1])
    results = []
    for name in CASES:
        for oblique in [False, True]:
            stem = name+('_oblique' if oblique else '')
            nominal = expected(name)
            if oblique:
                center, tensor = transform(nominal[1], nominal[2])
                nominal = nominal[0], center, tensor
            native = json.loads((directory/f'{stem}.properties.json').read_text())
            verify((native['volume'], native['centroid'], native['inertia']), nominal, stem+' native')
            verify(body_properties(directory/f'{stem}.step'), nominal, stem+' STEP')
            cylinder_faces(directory/f'{stem}.step', name)
            mesh(directory/f'{stem}.stl', nominal[0])
            results.append(dict(fixture=stem, volume=nominal[0], closed_stl=True))
    print(json.dumps(dict(valid=True, cases=results), indent=2))


if __name__ == '__main__':
    main()
