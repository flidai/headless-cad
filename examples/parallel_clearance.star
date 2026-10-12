# Eccentric shaft and offset sleeve: nominal clearance using actual annular material.
# The spacer footprints intersect away from the line joining their centers.
eccentric = param(name = "eccentric", default = 0.25)
lateral = param(name = "lateral", default = 0)
shaft_radius = param(name = "shaft_radius", default = 6)
shaft_inner = param(name = "shaft_inner", default = 0)
slide = param(name = "slide", default = 0)
spacer_x = param(name = "spacer_x", default = 8)
axial_gap = param(name = "axial_gap", default = 6)
opposed = param(name = "opposed", default = 0)
spacer_opposed = param(name = "spacer_opposed", default = 0)
pose = param(name = "pose", default = 2)
tilt = param(name = "tilt", default = 0)
required_clearance = param(name = "required_clearance", default = 0.2)

def validate_parameters():
    if eccentric < -40 or eccentric > 40 or lateral < -40 or lateral > 40 or slide < -60 or slide > 60:
        fail("shaft placement exceeds the declared range")
    if shaft_radius <= 0 or shaft_radius > 30 or shaft_inner < 0 or shaft_inner >= shaft_radius:
        fail("shaft radii must define a solid disk or annulus")
    if spacer_x < -40 or spacer_x > 40 or axial_gap < -6 or axial_gap > 30:
        fail("spacer placement exceeds the declared range")
    if opposed != 0 and opposed != 1 or spacer_opposed != 0 and spacer_opposed != 1 or tilt != 0 and tilt != 1:
        fail("orientation switches must be 0 or 1")
    if opposed == 1 and tilt == 1:
        fail("opposed and tilt are separate configurations")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
validate_parameters()

bearing_native = extrude(id = "bearing_body", profile = circle(id = "bearing_profile", radius = 12, inner_radius = 7), height = 20)
shaft_native = extrude(id = "shaft_body", profile = circle(id = "shaft_profile", radius = shaft_radius, inner_radius = shaft_inner), height = 32)
spacer_native = extrude(id = "spacer_body", profile = circle(id = "spacer_profile", radius = 12, inner_radius = 10), height = 4)
bearing = part(id = "bearing", body = bearing_native, part_number = "EC-BEARING", material = "Steel", density_kg_per_m3 = 7800)
shaft = part(id = "shaft", body = shaft_native, part_number = "EC-SPINDLE", material = "Steel", density_kg_per_m3 = 7800)
spacer = part(id = "spacer", body = spacer_native, part_number = "EC-SLEEVE", material = "Steel", density_kg_per_m3 = 7800)
shaft_rotation = [1,0,0,0] if opposed == 1 else ([0,0.043619387365336,0,0.9990482215818578] if tilt == 1 else [0,0,0,1])
cell = assembly(id = "cell", instances = [
    instance(id = "bearing", part = bearing),
    instance(id = "shaft", part = shaft, offset = [eccentric,lateral,-6+slide+(32 if opposed == 1 else 0)], rotation = shaft_rotation),
    instance(id = "spacer", part = spacer, offset = [spacer_x,0,20+axial_gap+(4 if spacer_opposed == 1 else 0)], rotation = [1,0,0,0] if spacer_opposed == 1 else [0,0,0,1]),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell, offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "parallel_clearance", assembly = station)
output(name = "bearing", feature = bearing_native)
clearance_check(id = "eccentric_shaft", output = "parallel_clearance", left_instance_path = ["cell","shaft"], right_instance_path = ["cell","bearing"], min_clearance = required_clearance)
clearance_check(id = "offset_sleeve", output = "parallel_clearance", left_instance_path = ["cell","bearing"], right_instance_path = ["cell","spacer"], min_clearance = required_clearance)
