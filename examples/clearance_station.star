# Native geometric minimum separation of two convex tooling jaws.
# Independent root pose and moving-jaw rotation exercise occurrence placement.
gap = param(name = "gap", default = 10)
y_gap = param(name = "y_gap", default = 5)
z_gap = param(name = "z_gap", default = 4)
pose = param(name = "pose", default = 2)
turn = param(name = "turn", default = 1)
required_clearance = param(name = "required_clearance", default = 2)

def validate_parameters():
    if gap < -40 or gap > 30 or y_gap < -20 or y_gap > 20 or z_gap < -10 or z_gap > 15:
        fail("clearance station offsets exceed the declared example range")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
    if turn != 0 and turn != 1:
        fail("turn must be 0 or 1")
validate_parameters()

jaw_body = extrude(id = "jaw_body", profile = polygon(id = "jaw_outline",
    points = [[0,0],[40,0],[40,14],[32,20],[0,20]]), height = 10)
jaw = part(id = "jaw", body = jaw_body)
# 30 degree local-Z rotation, independent of the complete cell's root pose.
moving_rotation = [0,0,0,1] if turn == 0 else [0,0,0.25881904510252074,0.9659258262890683]
cell = assembly(id = "cell", instances = [
    instance(id = "fixed", part = jaw),
    instance(id = "moving", part = jaw, offset = [40+gap,20+y_gap,10+z_gap], rotation = moving_rotation),
])
root_rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = cell,
    offset = [25,-12,7], rotation = root_rotation)])
assembly_output(name = "clearance_station", assembly = station)
output(name = "jaw", feature = jaw_body)
clearance_check(id = "jaw_separation", output = "clearance_station",
    left_instance_path = ["cell","fixed"], right_instance_path = ["cell","moving"],
    min_clearance = required_clearance)
