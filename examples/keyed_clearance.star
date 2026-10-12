# Five separated native clearance inspection pairs, not a complete drive.
# Queries certify actual surviving material witnesses, not untrimmed surrogates.
shaft_radius = param(name = "shaft_radius", default = 6)
bore_gap = param(name = "bore_gap", default = 0.15)
required_clearance = param(name = "required_clearance", default = 0.1)
hub_turn = param(name = "hub_turn", default = 0)
pose = param(name = "pose", default = 0)
scale = param(name = "scale", default = 1)
def check_parameters():
    if scale < 0.001 or scale > 100:
        fail("scale must be 0.001..100")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
    if hub_turn != 0 and hub_turn != 1 and hub_turn != 2:
        fail("hub_turn must be 0, 1 or 2")
check_parameters()
r = shaft_radius*scale
w = 4.04*scale
floor = r-2*scale
shaft_stock = extrude(id = "shaft_stock", profile = circle(id = "shaft_profile", radius = r), height = 40*scale)
seat_box = extrude(id = "seat_box", profile = rectangle(id = "seat_profile", width = 4*scale, depth = w), height = 13.5*scale)
seat_tool = translate(id = "seat_tool", body = seat_box, offset = [floor,-w/2,10*scale])
shaft_body = cut(id = "shaft_body", left = shaft_stock, right = seat_tool)
hub_stock = extrude(id = "hub_stock", profile = circle(id = "hub_profile", radius = 16*scale, inner_radius = r+bore_gap*scale), height = 14*scale)
hub_box = extrude(id = "hub_box", profile = rectangle(id = "hub_slot_profile", width = r+2.05*scale, depth = w), height = 16*scale)
hub_tool = translate(id = "hub_tool", body = hub_box, offset = [0,-w/2,-scale])
hub_body = cut(id = "hub_body", left = hub_stock, right = hub_tool)
sleeve_body = extrude(id = "sleeve_body", profile = circle(id = "sleeve_profile", radius = r+3*scale, inner_radius = r+bore_gap*scale), height = 4*scale)
shaft = part(id = "shaft", body = shaft_body)
hub = part(id = "hub", body = hub_body)
sleeve = part(id = "sleeve", body = sleeve_body)
hub_rotation = [0,0,0,1] if hub_turn == 0 else ([0,0,0.7071067811865475,0.7071067811865476] if hub_turn == 1 else [0,0,1,0])
pairs = assembly(id = "pairs", instances = [
    instance(id = "journal_shaft", part = shaft, offset = [0*scale,0,0]),
    instance(id = "journal_sleeve", part = sleeve, offset = [0*scale,0,2*scale]),
    instance(id = "seat_shaft", part = shaft, offset = [60*scale,0,0]),
    instance(id = "seat_sleeve", part = sleeve, offset = [60*scale,0,12*scale]),
    instance(id = "boundary_shaft", part = shaft, offset = [120*scale,0,0]),
    instance(id = "boundary_sleeve", part = sleeve, offset = [120*scale,0,8*scale]),
    instance(id = "hub_shaft", part = shaft, offset = [180*scale,0,0]),
    instance(id = "hub_mate", part = hub, offset = [180*scale,0,10*scale], rotation = hub_rotation),
    instance(id = "axial_hub", part = hub, offset = [240*scale,0,0], rotation = hub_rotation),
    instance(id = "axial_sleeve", part = sleeve, offset = [240*scale,0,16*scale]),
])
rotation = [0,0,0,1] if pose == 0 else ([0,0.7071067811865475,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
station = assembly(id = "station", instances = [assembly_instance(id = "cell", assembly = pairs, offset = [25*scale,-12*scale,7*scale], rotation = rotation)])
assembly_output(name = "keyed_clearance", assembly = station)
output(name = "shaft", feature = shaft_body)
output(name = "hub", feature = hub_body)
output(name = "sleeve", feature = sleeve_body)
clearance_check(id = "journal_gap", output = "keyed_clearance", left_instance_path = ["cell","journal_shaft"], right_instance_path = ["cell","journal_sleeve"], min_clearance = required_clearance*scale)
clearance_check(id = "seat_gap", output = "keyed_clearance", left_instance_path = ["cell","seat_shaft"], right_instance_path = ["cell","seat_sleeve"], min_clearance = required_clearance*scale)
clearance_check(id = "boundary_gap", output = "keyed_clearance", left_instance_path = ["cell","boundary_shaft"], right_instance_path = ["cell","boundary_sleeve"], min_clearance = required_clearance*scale)
clearance_check(id = "hub_gap", output = "keyed_clearance", left_instance_path = ["cell","hub_shaft"], right_instance_path = ["cell","hub_mate"], min_clearance = required_clearance*scale)
clearance_check(id = "axial_gap", output = "keyed_clearance", left_instance_path = ["cell","axial_hub"], right_instance_path = ["cell","axial_sleeve"], min_clearance = required_clearance*scale)
