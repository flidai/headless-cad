# Certified full angular travel of an orbiting tool, not pose-only sampling.
# Angles are radians; the pivot and axis are in the moving part's local frame.
angle = param(name = "angle", default = 6.283185307179586)
guard_radius = param(name = "guard_radius", default = 30)
axial_gap = param(name = "axial_gap", default = 6)
pose = param(name = "pose", default = 2)
required_clearance = param(name = "required_clearance", default = 0.5)

def validate_parameters():
    if guard_radius < 10 or guard_radius > 50 or axial_gap < 0 or axial_gap > 20:
        fail("fixture dimensions exceed the declared example range")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
validate_parameters()

tool_body = extrude(id = "tool_body", profile = circle(id = "tool_profile", radius = 2), height = 4)
guard_body = extrude(id = "guard_body", profile = circle(id = "guard_profile", radius = 3), height = 4)
hub_body = extrude(id = "hub_body", profile = circle(id = "hub_profile", radius = 6, inner_radius = 3), height = 4)
tool = part(id = "tool", body = tool_body, part_number = "RC-TOOL", material = "Steel", density_kg_per_m3 = 7800)
guard = part(id = "guard", body = guard_body, part_number = "RC-GUARD")
hub = part(id = "hub", body = hub_body, part_number = "RC-HUB")
cell = assembly(id = "cell", instances = [
    instance(id = "hub", part = hub),
    instance(id = "tool", part = tool, offset = [20,0,0]),
    instance(id = "guard", part = guard, offset = [0,guard_radius,0]),
    instance(id = "overhead", part = guard, offset = [0,20,4+axial_gap]),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell, offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "rotation_clearance", assembly = station)
output(name = "tool", feature = tool_body)
path = {"pivot": [-20,0,0], "axis": [0,0,1], "angle_radians": angle}
clearance_check(id = "tool_guard", output = "rotation_clearance", left_instance_path = ["cell","guard"], right_instance_path = ["cell","tool"], min_clearance = required_clearance, right_rotation = path)
clearance_check(id = "tool_overhead", output = "rotation_clearance", left_instance_path = ["cell","overhead"], right_instance_path = ["cell","tool"], min_clearance = required_clearance, right_rotation = path)
