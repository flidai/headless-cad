# Prescribed slider and rotary-head coordinates; no collision or force simulation.
load("recipe_modules/drive_parts.star", "rail_part", "carriage_part")
length = 160
travel = param(name = "travel", default = 50)
angle = param(name = "angle", default = 0.6)
pose = param(name = "pose", default = 0)
def validate_pose():
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
validate_pose()
rail = rail_part("rail", length, 2)
carriage = carriage_part("carriage", 3)
hub_body = extrude(id = "hub_body", profile = circle(id = "hub_profile", radius = 12), height = 6)
hub = part(id = "hub", body = hub_body, part_number = "ROTARY-HUB", material = "Steel", density_kg_per_m3 = 7800, datums = {"axis":datum(),"top":face_datum(from_feature = hub_body, role = "end_cap", x_direction = [1,0,0])})
arm_stock = extrude(id = "arm_stock", profile = rectangle(id = "arm_profile", width = 50, depth = 10), height = 4)
pivot_tool = translate(id = "pivot_tool", body = extrude(id = "pivot_bore", profile = circle(id = "pivot_profile", radius = 2), height = 6), offset = [5,5,-1])
arm_body = cut(id = "arm_body", left = arm_stock, right = pivot_tool)
arm = part(id = "arm", body = arm_body, part_number = "INDEX-ARM", material = "Aluminum", density_kg_per_m3 = 2700, datums = {"pivot":datum(offset = [5,5,0])})
head = assembly(id = "head", instances = [instance(id = "hub", part = hub), instance(id = "arm", part = arm, attach_to = "hub", datum = "pivot", target_datum = "top", offset = [0,0,2])])
stage = assembly(id = "stage", instances = [instance(id = "rail_left", part = rail), instance(id = "rail_right", part = rail, offset = [0,70,0]), instance(id = "carriage", part = carriage, attach_to = "rail_left", datum = "drive", target_datum = "drive", joint = prismatic(position = travel, min = 20, max = 100)), assembly_instance(id = "head", assembly = head, attach_to = "carriage", source_path = ["hub"], datum = "axis", target_datum = "top", joint = revolute(angle = angle, min = -3.141592653589793, max = 3.141592653589793))])
rotation = [0,0,0,1] if pose == 0 else ([0,0,0.7071067811865475,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
assembly_output(name = "jointed_linear_stage", assembly = assembly(id = "fixture", instances = [assembly_instance(id = "stage", assembly = stage, offset = [7,-4,2], rotation = rotation)]))
output(name = "carriage", feature = "carriage_bore_3")
pi = 3.141592653589793
mass = (2*length*(400-4)+pi*144*6)*7800/1000000000 + ((3600-4*pi*9)*8+(500-pi*4)*4)*2700/1000000000
assert_property(id = "material_mass", output = "jointed_linear_stage", property = "mass_kg", min = mass-0.0000001, max = mass+0.0000001)
section_view(id = "moving_carriage", output = "jointed_linear_stage", instance_path = ["stage","carriage"], datum = "top", origin = [0,0,-4], min_area = 3600-4*pi*9-0.000001, max_area = 3600-4*pi*9+0.000001)
