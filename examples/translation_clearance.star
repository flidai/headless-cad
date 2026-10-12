# Continuous straight-line travel, authored in each moving part's local axes.
# Each named pair covers the entire declared path, not just sampled poses.
travel = param(name = "travel", default = 40)
traverse = param(name = "traverse", default = 20)
pass_y = param(name = "pass_y", default = 10)
bore_gap = param(name = "bore_gap", default = 1)
opposed = param(name = "opposed", default = 0)
pose = param(name = "pose", default = 2)
required_clearance = param(name = "required_clearance", default = 0.5)

def validate_parameters():
    if travel < -60 or travel > 60 or traverse < -60 or traverse > 60:
        fail("travel exceeds the declared example range")
    if pass_y < 0 or pass_y > 30 or bore_gap <= 0 or bore_gap > 3:
        fail("fixture dimensions exceed the declared example range")
    if opposed != 0 and opposed != 1:
        fail("opposed must be 0 or 1")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
validate_parameters()

bearing_body = extrude(id = "bearing_body", profile = circle(id = "bearing_profile", radius = 12, inner_radius = 6+bore_gap), height = 20)
shaft_body = revolve(id = "shaft_body", profile = polygon(id = "shaft_profile", points = [[0,0],[5,0],[5,4],[6,4],[6,28],[5,28],[5,32],[0,32]]))
guard_body = extrude(id = "guard_body", profile = rectangle(id = "guard_profile", width = 4, depth = 4), height = 4)
bearing = part(id = "bearing", body = bearing_body, part_number = "TC-BEARING", material = "Steel", density_kg_per_m3 = 7800)
shaft = part(id = "shaft", body = shaft_body, part_number = "TC-SHAFT", material = "Steel", density_kg_per_m3 = 7800)
guard = part(id = "guard", body = guard_body)
cell = assembly(id = "cell", instances = [
    instance(id = "bearing", part = bearing),
    instance(id = "shaft", part = shaft, offset = [0,0,26 if opposed == 1 else -6], rotation = [1,0,0,0] if opposed == 1 else [0,0,0,1]),
    instance(id = "guard", part = guard, offset = [35,0,0]),
    instance(id = "carriage", part = guard, offset = [25,pass_y,0]),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell, offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "translation_clearance", assembly = station)
output(name = "bearing", feature = bearing_body)
clearance_check(id = "shaft_travel", output = "translation_clearance", left_instance_path = ["cell","bearing"], right_instance_path = ["cell","shaft"], min_clearance = required_clearance, right_translation = [0,0,travel])
clearance_check(id = "carriage_pass", output = "translation_clearance", left_instance_path = ["cell","guard"], right_instance_path = ["cell","carriage"], min_clearance = required_clearance, right_translation = [traverse,0,0])
