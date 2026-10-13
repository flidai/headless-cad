#!/usr/bin/env python3
"""Independent OCCT acceptance for a solved, face-attached blind-hole pattern.

The reference solid is constructed with OCCT edge fillets and cylinder cuts,
independently of the Rust kernel and of the recipe's solved point coordinates.
This establishes the bounded nominal domain only, not a general pocket tool.
"""
import argparse
import hashlib
import importlib.metadata
import importlib.util
import json
import math
from pathlib import Path
import platform
import sys


ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


f = module("independent_fillet", "check-longitudinal-fillet.py")
k = f.k
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut  # noqa: E402
from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform  # noqa: E402
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder  # noqa: E402
from OCP.gp import gp_Ax2, gp_Dir, gp_Pnt, gp_Quaternion, gp_Trsf, gp_Vec  # noqa: E402


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inventory(directory):
    return {p.relative_to(directory).as_posix(): digest(p)
            for p in directory.rglob("*") if p.is_file()}


def close(left, right, label, absolute=2e-6):
    require(math.isfinite(left) and abs(left-right) <= max(absolute, abs(right)*2e-9),
            f"{label}: {left} versus {right}")


def reference(parameters):
    radius = parameters.get("hole_radius", 2)
    pitch = parameters.get("half_pitch", 16)
    depth = parameters.get("hole_depth", 5)
    pattern_y = parameters.get("pattern_y", 0)
    underside = parameters.get("underside", 0)
    pose = parameters.get("pose", 0)
    solid = f.filleted([(-25,-18),(25,-18),(25,18),(-25,18)], 12, 4, [0,1,2,3])
    through = BRepPrimAPI_MakeCylinder(
        gp_Ax2(gp_Pnt(0,0,-1), gp_Dir(0,0,1)), 7, 14).Shape()
    solid = k.boolean(BRepAlgoAPI_Cut, solid, through)
    for x in (-pitch, pitch):
        z = -1 if underside else 12-depth
        # With a fixed +X tangent, the lower outward -Z cap has local Y=-Y.
        y = -pattern_y if underside else pattern_y
        cutter = BRepPrimAPI_MakeCylinder(
            gp_Ax2(gp_Pnt(x,y,z), gp_Dir(0,0,1)), radius, depth+1).Shape()
        solid = k.boolean(BRepAlgoAPI_Cut, solid, cutter)
    if pose:
        transform = gp_Trsf()
        quaternion = (.5,.5,.5,.5) if pose == 1 else (.1,.2,.3,math.sqrt(.86))
        offset = (13,-7,15) if pose == 1 else (11,-9,17)
        transform.SetRotation(gp_Quaternion(*quaternion))
        transform.SetTranslationPart(gp_Vec(*offset))
        solid = BRepBuilderAPI_Transform(solid, transform, True).Shape()
    return solid


def formula(parameters):
    r = parameters.get("hole_radius", 2)
    d = parameters.get("hole_depth", 5)
    material_area = 50*36-(4-math.pi)*4*4-math.pi*7*7
    return material_area*12-2*math.pi*r*r*d


def params_argv(parameters):
    result = []
    for name, value in sorted(parameters.items()):
        result += ["--param", f"{name}={value}"]
    return result


def check_case(cad, source, root, tag, parameters):
    out = root / tag
    manifest = f.run(cad, "build", source, "--out-dir", out, *params_argv(parameters))
    verified = f.run(cad, "verify", out)
    require(verified["valid"], "native verified generation")
    native = f.run(cad, "properties", source, "--output", "housing", *params_argv(parameters))
    independent = reference(parameters)
    actual = k.load(str(out / "housing.step"))
    expected_props = k.properties(independent)
    actual_props = k.properties(actual)
    native_props = (native["volume"], native["volume_centroid"], native["central_volume_inertia"])
    close(expected_props[0], formula(parameters), "closed-form OCCT volume", 1e-5)
    for label, props in (("STEP", actual_props), ("native", native_props)):
        close(props[0], expected_props[0], label+" volume", 1e-5)
        for i in range(3):
            close(props[1][i], expected_props[1][i], label+" centroid", 1e-6)
            for j in range(3):
                close(props[2][i][j], expected_props[2][i][j], label+" inertia", 2e-4)
    differences = [k.properties(k.boolean(BRepAlgoAPI_Cut,a,b))[0]
                   for a,b in ((actual, independent),(independent, actual))]
    for value in differences:
        require(abs(value) < 2e-6, f"STEP/OCCT directed solid difference {value}")
    required_roles = {f"{side}_{kind}" for side in ("clamp_left", "clamp_right")
                      for kind in ("wall", "floor")}
    require(required_roles.issubset(manifest["face_assertions"]),
            "four authored named wall/floor assertions")
    require(all(manifest["face_assertions"][name]["status"] == "resolved"
                for name in required_roles), "named wall/floor roles resolve")
    require(manifest["face_assertions"]["central_bore_survives"]["status"] == "resolved",
            "central through-bore lineage survives")
    require("housing" in manifest["sketch_reports"], "pattern sketch solution published")
    if parameters.get("pose", 0) == 0:
        require("clamp_holes" in manifest["section_views"], "source-authored section")
        dimensions = manifest["section_views"]["clamp_holes"]["circle_dimensions"]
        require({dimension["declaration"]["id"] for dimension in dimensions} ==
                {"left_clamp_diameter", "right_clamp_diameter"},
                "source-authored diameter dimension")
    replay = root / (tag+"-replay")
    f.run(cad, "build", out / "recipe.star", "--out-dir", replay, *params_argv(parameters))
    require((out / "housing.step").read_bytes() == (replay / "housing.step").read_bytes(),
            "captured source reproduces byte-identical STEP")
    return {"case": tag, "parameters": parameters, "volume_mm3": expected_props[0],
            "centroid_mm": expected_props[1], "central_inertia_mm5": expected_props[2],
            "directed_difference_mm3": differences,
            "kernel_build_id": manifest["kernel_build_id"],
            "executor_build_id": manifest["executor_build_id"]}


def reject(cad, source, published, parameters, label):
    before = inventory(published)
    failure = f.run(cad, "build", source, "--out-dir", published,
                    *params_argv(parameters), success=False)
    require(inventory(published) == before,
            f"{label}: failed generation changed previously published bytes")
    return {"case": label, "parameters": parameters, "error": failure}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cad", type=Path, default=ROOT / "target/release/cad")
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    cad, root = args.cad.resolve(), args.out_dir.resolve()
    source = ROOT / "examples/sketched_blind_housing.star"
    executable_sha256, source_sha256 = digest(cad), digest(source)
    root.mkdir(parents=True, exist_ok=False)
    cases = [
        ("top-default", {}),
        ("bottom-default", {"underside":1}),
        ("top-posed", {"pose":1}),
        ("bottom-posed", {"underside":1,"pose":1}),
        ("top-oblique", {"pose":2,"pattern_y":3}),
        ("bottom-oblique", {"underside":1,"pose":2,"pattern_y":3}),
        ("top-different", {"half_pitch":15,"hole_radius":3,"hole_depth":7}),
        ("bottom-different", {"half_pitch":17,"hole_radius":1.5,"hole_depth":4,"underside":1,"pose":1}),
    ]
    results = [check_case(cad, source, root, name, parameters)
               for name, parameters in cases]
    published = root / "top-default"
    rejected = []
    for label, parameters in [
        ("touch-central-bore", {"half_pitch":9}),
        ("touch-outer-wall", {"half_pitch":23}),
        ("floor-breakthrough", {"hole_depth":12}),
    ]:
        rejected.append(reject(cad, source, published, parameters, label))
    original = source.read_text()
    source_edits = [
        ("underconstrained", original.replace(
            '        {"id":"center_axis","kind":"horizontal","a":"center_left","b":"center_right"},\n', "")),
        ("overconstrained", original.replace("    housing = hole_pattern(",
            '    constraints += [{"id":"wrong_right","kind":"fixed","point":"center_right","position":[half_pitch+1,0]}]\n' + "    housing = hole_pattern(")),
        ("nonplanar-face", original.replace(
            'face = {"from_feature":stock,"role":cap_role}',
            'face = {"from_feature":bored,"role":"bore_wall"}')),
        ("posed-parallel-hint", original.replace(
            'hint = [1,0,0] if pose == 0 else ([0,1,0] if pose == 1 else oblique_hint)',
            'hint = [1,0,0]')),
    ]
    for label, text in source_edits:
        require(text != original, f"{label}: source edit matched")
        edited = root / (label+".star")
        edited.write_text(text)
        rejected.append(reject(cad, edited, published,
                               {"pose":1} if label == "posed-parallel-hint" else {}, label))
    require(digest(cad) == executable_sha256, "CAD executable changed during oracle run")
    require(digest(source) == source_sha256, "authored recipe changed during oracle run")
    report = {"valid": True, "cases": results, "rejected": rejected,
              "executable": {"path": str(cad), "sha256": executable_sha256},
              "source": {"path": str(source), "sha256": source_sha256},
              "oracle": {"python": platform.python_version(), "python_executable": sys.executable,
                         "ocp_distribution": "cadquery-ocp",
                         "ocp_version": importlib.metadata.version("cadquery-ocp")},
              "scope": "One solved planar face-attached pattern of contained, disjoint, flat-bottom blind bores on a rounded right prism; OCCT is a test-only independent oracle. No threads, drill tips, arbitrary pockets, or load rating."}
    (root / "report.json").write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps({"valid":True,"cases":len(results),"rejected":len(rejected),
                      "report":str(root / "report.json")}))


if __name__ == "__main__":
    main()
