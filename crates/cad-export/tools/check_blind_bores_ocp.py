"""Independent signed box/cylinder oracle for export_blind_bores fixtures.

Checks imported B-rep validity, full native/STEP mass tensors, oriented cylinder
walls and blind floor planes, and closed consistently oriented STL meshes.
OCP is a development oracle only; nominal dimensions derive no data from CAD.
"""
import json
import math
import pathlib
import sys
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE, TopAbs_REVERSED
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS
from check_arc_prisms_ocp import mesh
from check_concave_holes_ocp import box, transform
from check_constrained_fixture_ocp import body_properties, require
from check_twin_lobe_ocp import verify_properties


TOOLS = {
    'arc': [(0, 0, 3, 8, 12)],
    'top': [(20, 13, 3, 8, 12)],
    'bottom': [(57, 26, 4, 0, 5)],
    'mixed': [(20, 13, 3, 8, 12), (57, 26, 4, 0, 5), (40, 20, 2, 0, 12)],
}


def expected(name):
    pieces = [box([0, 0, 0], [80, 40, 12], 1)]
    if name == 'arc':
        v = math.pi*400*12
        pieces = [(v, [0, 0, 6], [v*(1200+144)/12]*2+[v*200])]
    for x, y, r, low, high in TOOLS[name]:
        h = high - low
        v = -math.pi * r * r * h
        pieces.append((v, [x, y, (low + high) / 2],
                       [v * (3*r*r + h*h) / 12] * 2 + [v*r*r/2]))
    volume = sum(p[0] for p in pieces)
    center = [sum(p[0] * p[1][i] for p in pieces)/volume for i in range(3)]
    inertia = [[0.]*3 for _ in range(3)]
    for v, c, diagonal in pieces:
        d = [c[i] - center[i] for i in range(3)]
        for i in range(3):
            for j in range(3):
                inertia[i][j] += (diagonal[i] + v * sum(d[k]**2 for k in range(3) if k != i)) if i == j else -v*d[i]*d[j]
    return volume, center, inertia


def check_surfaces(path, name, oblique):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone, 'STEP import failed')
    require(reader.TransferRoots() > 0, 'STEP transfer failed')
    explorer = TopExp_Explorer(reader.OneShape(), TopAbs_FACE)
    floors = []
    inward = 0
    outward = 0
    planes = 0
    while explorer.More():
        face = TopoDS.Face(explorer.Current())
        surface = BRepAdaptor_Surface(face)
        orientation = -1 if face.Orientation() == TopAbs_REVERSED else 1
        if surface.GetType() == GeomAbs_Cylinder:
            if orientation == -1:
                inward += 1
            else:
                require(name == 'arc' and abs(surface.Cylinder().Radius()-20) < 1e-8, 'unexpected outward cylindrical wall')
                outward += 1
        elif surface.GetType() == GeomAbs_Plane:
            planes += 1
            plane = surface.Plane()
            loc = plane.Location()
            axis = plane.Axis().Direction()
            floors.append(([loc.X(), loc.Y(), loc.Z()], [orientation * axis.X(), orientation * axis.Y(), orientation * axis.Z()]))
        explorer.Next()
    blind = sum(low != 0 or high != 12 for _, _, _, low, high in TOOLS[name])
    require(inward == len(TOOLS[name]), 'missing inward walls')
    require(outward == (2 if name == 'arc' else 0), 'missing outer arc walls')
    require(planes == (2 if name == 'arc' else 6) + blind, 'missing blind floor plane')
    for x, y, _, low, high in TOOLS[name]:
        if low == 0 and high == 12:
            continue
        floor = low if low != 0 else high
        point = [x, y, floor]
        normal = [0, 0, 1 if low != 0 else -1]
        if oblique:
            point, _ = transform(point, [[0.]*3 for _ in range(3)])
            q = math.sqrt(.5)
            normal = [q*normal[2], -q*normal[2], 0]
        require(any(abs(sum((point[i] - p[i])*n[i] for i in range(3))) < 1e-7 and
                    sum(normal[i]*n[i] for i in range(3)) > 1-1e-10 for p, n in floors), 'floor plane or outward normal is wrong')


def main():
    directory = pathlib.Path(sys.argv[1])
    results = []
    for name in TOOLS:
        for oblique in [False, True]:
            stem = name + ('_oblique' if oblique else '')
            properties = expected(name)
            if oblique:
                c, tensor = transform(properties[1], properties[2])
                properties = (properties[0], c, tensor)
            native = json.loads((directory/f'{stem}.properties.json').read_text())
            verify_properties((native['volume'], native['centroid'], native['inertia']), properties, stem+' native')
            verify_properties(body_properties(directory/f'{stem}.step'), properties, stem+' STEP')
            check_surfaces(directory/f'{stem}.step', name, oblique)
            mesh(directory/f'{stem}.stl', properties[0])
            results.append(dict(fixture=stem, volume=properties[0], closed_stl=True))
    print(json.dumps(dict(valid=True, cases=results), indent=2))


if __name__ == '__main__':
    main()
