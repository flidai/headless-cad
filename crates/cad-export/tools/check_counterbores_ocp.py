"""Independent disjoint cylinder-slice oracle for export_counterbores fixtures.

STEP validity, full volume moments, annular shoulder topology/orientation and
closed oriented STL are checked against nominal dimensions, not native geometry.
OCP is a development-only dependency.
"""
import json
import math
import pathlib
import sys
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED, TopAbs_WIRE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from check_arc_prisms_ocp import mesh
from check_concave_holes_ocp import box, transform
from check_constrained_fixture_ocp import body_properties, require

CASES = ['top', 'top_reverse', 'bottom', 'bottom_reverse', 'opposed']


def slices(name):
    if name.startswith('top'):
        return [(3, 0, 8), (7, 8, 12)]
    if name.startswith('bottom'):
        return [(6, 0, 4), (3, 4, 12)]
    return [(6, 0, 4), (3, 4, 8), (7, 8, 12)]


def expected(name):
    pieces = [box([0, 0, 0], [80, 40, 12], 1)]
    # Disjoint slices avoid counting the central through-hole twice.
    for radius, low, high in slices(name):
        h = high - low
        v = -math.pi * radius**2 * h
        pieces.append((v, [23, 17, (low+high)/2],
                       [v*(3*radius**2+h*h)/12]*2+[v*radius**2/2]))
    volume = sum(p[0] for p in pieces)
    center = [sum(p[0]*p[1][i] for p in pieces)/volume for i in range(3)]
    tensor = [[0.]*3 for _ in range(3)]
    for v, c, diag in pieces:
        d = [c[i]-center[i] for i in range(3)]
        for i in range(3):
            for j in range(3):
                tensor[i][j] += (diag[i]+v*sum(d[k]**2 for k in range(3) if k != i)) if i == j else -v*d[i]*d[j]
    return volume, center, tensor


def verify(actual, nominal, label):
    av, ac, ai = actual
    v, c, tensor = nominal
    def close(a, b):
        return math.isfinite(a) and abs(a-b) <= max(2e-7, abs(b)*2e-10)
    require(close(av, v), label+' volume')
    require(all(close(a, b) for a, b in zip(ac, c)), label+' centroid')
    tensor_error = max(2e-7, max(abs(tensor[i][i]) for i in range(3))*2e-10)
    require(all(math.isfinite(ai[i][j]) and abs(ai[i][j]-tensor[i][j]) <= tensor_error for i in range(3) for j in range(3)), label+' full inertia')


def surfaces(path, name, oblique):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone, 'STEP import')
    require(reader.TransferRoots() == 1, 'STEP root count')
    explorer = TopExp_Explorer(reader.OneShape(), TopAbs_FACE)
    walls, shoulders = [], []
    while explorer.More():
        face = TopoDS.Face(explorer.Current())
        surface = BRepAdaptor_Surface(face)
        sign = -1 if face.Orientation() == TopAbs_REVERSED else 1
        if surface.GetType() == GeomAbs_Cylinder:
            require(sign == -1, 'bore wall faces inward')
            walls.append(surface.Cylinder().Radius())
        elif surface.GetType() == GeomAbs_Plane:
            plane = surface.Plane()
            p, n = plane.Location(), plane.Axis().Direction()
            wires = TopExp_Explorer(face, TopAbs_WIRE)
            count = 0
            while wires.More():
                count += 1
                wires.Next()
            shoulders.append(([p.X(), p.Y(), p.Z()], [sign*n.X(), sign*n.Y(), sign*n.Z()], count))
        explorer.Next()
    radii = sorted(s[0] for s in slices(name))
    require(len(walls) == len(radii) and all(abs(a-b) < 1e-8 for a, b in zip(sorted(walls), radii)), 'wall radii/count')
    require(len(shoulders) == 5+len(slices(name)), 'plane count')
    for lower, upper in zip(slices(name), slices(name)[1:]):
        radius_lo, _, z = lower
        radius_hi, _, _ = upper
        point = [23, 17, z]
        normal = [0, 0, 1 if radius_hi > radius_lo else -1]
        if oblique:
            point, _ = transform(point, [[0.]*3 for _ in range(3)])
            q = math.sqrt(.5)
            normal = [q*normal[2], -q*normal[2], 0]
        require(any(count == 2 and abs(sum((point[i]-p[i])*n[i] for i in range(3))) < 1e-7 and
                    sum(normal[i]*n[i] for i in range(3)) > 1-1e-10 for p, n, count in shoulders),
                'missing annular shoulder or incorrect outward normal')


def main():
    directory = pathlib.Path(sys.argv[1])
    results = []
    for name in CASES:
        for oblique in [False, True]:
            stem = name+('_oblique' if oblique else '')
            p = expected(name)
            if oblique:
                c, tensor = transform(p[1], p[2])
                p = (p[0], c, tensor)
            native = json.loads((directory/f'{stem}.properties.json').read_text())
            verify((native['volume'], native['centroid'], native['inertia']), p, stem+' native')
            verify(body_properties(directory/f'{stem}.step'), p, stem+' STEP')
            surfaces(directory/f'{stem}.step', name, oblique)
            mesh(directory/f'{stem}.stl', p[0])
            results.append(dict(fixture=stem, volume=p[0], closed_stl=True))
    print(json.dumps(dict(valid=True, cases=results), indent=2))


if __name__ == '__main__':
    main()
