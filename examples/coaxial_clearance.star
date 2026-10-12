# Nominal radial/axial clearance inside a bearing, using actual hollow geometry.
# Manufacturing tolerances and continuous shaft travel require separate checks.
gap = param(name = "gap", default = 1)
shape = param(name = "shape", default = 1)
slide = param(name = "slide", default = 0)
axial_gap = param(name = "axial_gap", default = 6)
opposed = param(name = "opposed", default = 0)
pose = param(name = "pose", default = 2)
eccentric = param(name = "eccentric", default = 0)
tilt = param(name = "tilt", default = 0)
oversize = param(name = "oversize", default = 0)
required_clearance = param(name = "required_clearance", default = 0.5)

def validate_parameters():
    if gap < -1 or gap > 3 or slide < -60 or slide > 60 or axial_gap < -4 or axial_gap > 30:
        fail("bearing dimensions exceed the example range")
    if shape != 0 and shape != 1 and shape != 2 and shape != 3:
        fail("shape must be 0 (plain), 1 (stepped shaft), 2 (tapered), or 3 (stepped bore)")
    if opposed != 0 and opposed != 1:
        fail("opposed must be 0 or 1")
    if opposed == 1 and tilt == 1:
        fail("opposed and tilt are separate configurations")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
    if tilt != 0 and tilt != 1 or oversize != 0 and oversize != 1:
        fail("tilt and oversize must be 0 or 1")
validate_parameters()

bore = 6 + gap

def bearing_body():
    if shape == 2:
        return revolve(id = "bearing_body", profile = polygon(id = "bearing_profile",
            points = [[bore,0],[12,0],[12,20],[bore+2,20]]))
    if shape == 3:
        return revolve(id = "bearing_body", profile = polygon(id = "bearing_profile",
            points = [[bore,0],[12,0],[12,20],[bore,20],[bore,14],[bore+1,14],[bore+1,6],[bore,6]]))
    return extrude(id = "bearing_body", profile = circle(id = "bearing_profile", radius = 12, inner_radius = bore), height = 20)

def shaft_body():
    if oversize == 1:
        return extrude(id = "shaft_body", profile = circle(id = "shaft_profile", radius = 14), height = 32)
    if shape == 1:
        return revolve(id = "shaft_body", profile = polygon(id = "shaft_profile",
            points = [[0,0],[5,0],[5,4],[6,4],[6,28],[5,28],[5,32],[0,32]]))
    if shape == 2:
        return revolve(id = "shaft_body", profile = polygon(id = "shaft_profile", points = [[0,0],[6,0],[8,20],[0,20]]))
    return extrude(id = "shaft_body", profile = circle(id = "shaft_profile", radius = 6), height = 32)

bearing_native = bearing_body()
shaft_native = shaft_body()
spacer_native = extrude(id = "spacer_body", profile = circle(id = "spacer_profile", radius = 12, inner_radius = 10), height = 4)
bearing = part(id = "bearing", body = bearing_native, part_number = "BC-BEARING", material = "Steel", density_kg_per_m3 = 7800)
shaft = part(id = "shaft", body = shaft_native, part_number = "BC-SHAFT", material = "Steel", density_kg_per_m3 = 7800)
spacer = part(id = "spacer", body = spacer_native, part_number = "BC-SPACER", material = "Steel", density_kg_per_m3 = 7800)
shaft_height = 20 if shape == 2 and oversize == 0 else 32
shaft_base = 0 if shape == 2 and oversize == 0 else -6
shaft_rotation = [1,0,0,0] if opposed == 1 else ([0,0.043619387365336,0,0.9990482215818578] if tilt == 1 else [0,0,0,1])
cell = assembly(id = "cell", instances = [
    instance(id = "bearing", part = bearing),
    instance(id = "shaft", part = shaft, offset = [eccentric,0,shaft_base+slide+(shaft_height if opposed == 1 else 0)], rotation = shaft_rotation),
    instance(id = "spacer", part = spacer, offset = [0,0,20+axial_gap]),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell, offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "coaxial_clearance", assembly = station)
output(name = "bearing", feature = bearing_native)
clearance_check(id = "shaft_bore", output = "coaxial_clearance", left_instance_path = ["cell","shaft"], right_instance_path = ["cell","bearing"], min_clearance = required_clearance)
clearance_check(id = "bearing_spacer", output = "coaxial_clearance", left_instance_path = ["cell","bearing"], right_instance_path = ["cell","spacer"], min_clearance = required_clearance)
