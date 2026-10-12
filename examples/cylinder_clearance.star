# Analytic roller-to-roller and roller-to-guard minimum separation.
# Curved support is native; display tessellation never supplies the distance.
gap = param(name = "gap", default = 10)
lateral = param(name = "lateral", default = 0)
axial = param(name = "axial", default = 0)
turn = param(name = "turn", default = 1)
guard_turn = param(name = "guard_turn", default = 1)
pose = param(name = "pose", default = 2)
tube = param(name = "tube", default = 0)
required_clearance = param(name = "required_clearance", default = 2)

def validate_parameters():
    if gap < -10 or gap > 30 or lateral < 0 or lateral > 30 or axial < 0 or axial > 40:
        fail("roller offsets exceed the declared example range")
    if turn != 0 and turn != 1:
        fail("turn must be 0 or 1")
    if guard_turn != 0 and guard_turn != 1:
        fail("guard_turn must be 0 or 1")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
    if tube != 0 and tube != 1:
        fail("tube must be 0 or 1")
validate_parameters()

def roller_profile():
    if tube == 1:
        return circle(id = "fixed_profile", radius = 6, inner_radius = 2)
    return circle(id = "fixed_profile", radius = 6)
fixed_body = extrude(id = "fixed_body", profile = roller_profile(), height = 20)
moving_body = extrude(id = "moving_body", profile = circle(id = "moving_profile", radius = 4), height = 16)
guard_body = extrude(id = "guard_body", profile = rectangle(id = "guard_profile", width = 16, depth = 10), height = 24)
fixed = part(id = "fixed_roller", body = fixed_body)
moving = part(id = "moving_roller", body = moving_body)
guard = part(id = "guard", body = guard_body)
roller_rotation = [0,0,0,1] if turn == 0 else [0,0.25881904510252074,0,0.9659258262890683]
guard_rotation = [0,0,0,1] if guard_turn == 0 else [0.25881904510252074,0,0,0.9659258262890683]
cell = assembly(id = "cell", instances = [
    instance(id = "fixed", part = fixed),
    instance(id = "moving", part = moving, offset = [10+gap,lateral,axial], rotation = roller_rotation),
    instance(id = "guard", part = guard, offset = [-8,24,0], rotation = guard_rotation),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell,
    offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "cylinder_clearance", assembly = station)
output(name = "roller", feature = fixed_body)
clearance_check(id = "roller_pair", output = "cylinder_clearance",
    left_instance_path = ["cell","fixed"], right_instance_path = ["cell","moving"],
    min_clearance = required_clearance)
clearance_check(id = "roller_guard", output = "cylinder_clearance",
    left_instance_path = ["cell","fixed"], right_instance_path = ["cell","guard"],
    min_clearance = required_clearance)
