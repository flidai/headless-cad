# Analytic tapered-tool clearance, including a pointed and a crowned profile.
# Curved support is native; display tessellation never supplies the distance.
gap = param(name = "gap", default = 10)
lateral = param(name = "lateral", default = 0)
axial = param(name = "axial", default = 0)
turn = param(name = "turn", default = 1)
guard_turn = param(name = "guard_turn", default = 1)
pose = param(name = "pose", default = 2)
shape = param(name = "shape", default = 0)
required_clearance = param(name = "required_clearance", default = 2)

def validate_parameters():
    if gap < -10 or gap > 30 or lateral < 0 or lateral > 30 or axial < 0 or axial > 40:
        fail("tool offsets exceed the declared example range")
    if turn != 0 and turn != 1:
        fail("turn must be 0 or 1")
    if guard_turn != 0 and guard_turn != 1:
        fail("guard_turn must be 0 or 1")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
    if shape != 0 and shape != 1 and shape != 2 and shape != 3 and shape != 4:
        fail("shape must be 0 (frustum), 1 (cone), 2 (crown), 3 (nonconvex), or 4 (hollow)")
validate_parameters()

def tool_points():
    if shape == 1:
        return [[0,0],[6,0],[0,20]]
    if shape == 2:
        return [[0,0],[6,0],[8,10],[3,20],[0,20]]
    if shape == 3:
        return [[0,0],[6,0],[3,10],[6,20],[0,20]]
    if shape == 4:
        return [[2,0],[6,0],[3,20],[2,20]]
    return [[0,0],[6,0],[3,20],[0,20]]
fixed_body = revolve(id = "fixed_body", profile = polygon(id = "fixed_profile", points = tool_points()))
moving_body = revolve(id = "moving_body", profile = polygon(id = "moving_profile", points = [[0,0],[4,0],[0,16]]))
guard_body = extrude(id = "guard_body", profile = rectangle(id = "guard_profile", width = 16, depth = 10), height = 24)
fixed = part(id = "fixed_tool", body = fixed_body)
moving = part(id = "moving_tool", body = moving_body)
guard = part(id = "guard", body = guard_body)
tool_rotation = [0,0,0,1] if turn == 0 else [0,0.25881904510252074,0,0.9659258262890683]
guard_rotation = [0,0,0,1] if guard_turn == 0 else [0.25881904510252074,0,0,0.9659258262890683]
cell = assembly(id = "cell", instances = [
    instance(id = "fixed", part = fixed),
    instance(id = "moving", part = moving, offset = [10+gap,lateral,axial], rotation = tool_rotation),
    instance(id = "guard", part = guard, offset = [-8,24,0], rotation = guard_rotation),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell,
    offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "revolved_clearance", assembly = station)
output(name = "tool", feature = fixed_body)
clearance_check(id = "tool_pair", output = "revolved_clearance",
    left_instance_path = ["cell","fixed"], right_instance_path = ["cell","moving"],
    min_clearance = required_clearance)
clearance_check(id = "tool_guard", output = "revolved_clearance",
    left_instance_path = ["cell","fixed"], right_instance_path = ["cell","guard"],
    min_clearance = required_clearance)
