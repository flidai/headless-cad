# Native drilled rounded rotor / stationary external roller fixture.
# The whole signed turn is certified for ONE named pair. This is not a contacting
# cam drive, shaft fit, fastener fit, bearing design, load rating, or all-pairs proof.
pi = 3.141592653589793
corner_radius = param(name = "corner_radius", default = 2)
roller_offset = param(name = "roller_offset", default = 18)
required_clearance = param(name = "required_clearance", default = 5)
turn_radians = param(name = "turn_radians", default = 2*pi)
pose = param(name = "pose", default = 0)
def model():
    if corner_radius < 1 or corner_radius > 3 or roller_offset < 18 or roller_offset > 22 or required_clearance <= 0 or required_clearance > 10 or pose not in [0,1]:
        fail("corner_radius 1..3, roller_offset 18..22, clearance (0,10], pose 0 or 1 required")
    stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 20, depth = 12), height = 6)
    front_left = fillet(id = "front_left", body = stock, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_min"}], radius = corner_radius)
    front_right = fillet(id = "front_right", body = front_left, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_min"}], radius = corner_radius)
    back_right = fillet(id = "back_right", body = front_right, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_max"}], radius = corner_radius)
    rounded = fillet(id = "back_left", body = back_right, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_max"}], radius = corner_radius)
    centered = translate(id = "centered", body = rounded, offset = [-10,-6,0])
    bore_tool = translate(id = "bore_tool", body = extrude(id = "bore_cylinder", profile = circle(id = "bore_profile", radius = 2), height = 8), offset = [0,0,-1])
    bored = cut(id = "central_bore", left = centered, right = bore_tool)
    drill = extrude(id = "mounting_drill", profile = circle(id = "mounting_profile", radius = 1), height = 8)
    rotor_body = bored
    for index, x in enumerate([-5,5]):
        tool = translate(id = "mounting_tool_"+str(index), body = drill, offset = [x,0,-1])
        rotor_body = cut(id = "mounting_hole_"+str(index), left = rotor_body, right = tool)
    roller_body = extrude(id = "roller_body", profile = circle(id = "roller_profile", radius = 2), height = 6)
    rotor = part(id = "rotor_part", body = rotor_body, part_number = "DRILLED-ROTOR", revision = "A", description = "Drilled rounded inspection rotor; bore and mounting holes have no fit or load certification", material = "Aluminum", density_kg_per_m3 = 2700)
    roller = part(id = "roller_part", body = roller_body, part_number = "REFERENCE-ROLLER", revision = "A", description = "Stationary analytic clearance reference roller", material = "Steel", density_kg_per_m3 = 7800)
    rig = assembly(id = "rig_definition", instances = [instance(id = "rotor", part = rotor), instance(id = "roller", part = roller, offset = [roller_offset,0,0])])
    frame = [0,0,0,1] if pose == 0 else [0.5,0.5,0.5,0.5]
    offset = [0,0,0] if pose == 0 else [13,-7,15]
    scene = assembly(id = "fixture", instances = [assembly_instance(id = "rig", assembly = rig, offset = offset, rotation = frame)])
    output(name = "rotor_inspection", feature = rotor_body)
    assembly_output(name = "drilled_rotor_fixture", assembly = scene)
    rigid_rotation_group(id = "rotor_rotation", output = "drilled_rotor_fixture", instance_paths = [["rig","rotor"]], reference_instance_path = ["rig","rotor"], pivot = [0,0,3], axis = [0,0,1], angle_radians = turn_radians)
    clearance_check(id = "rotor_roller_path", output = "drilled_rotor_fixture", left_instance_path = ["rig","roller"], right_instance_path = ["rig","rotor"], min_clearance = required_clearance, rotation_group = "rotor_rotation")
    area = 240-(4-pi)*corner_radius*corner_radius-6*pi
    assert_property(id = "rotor_volume", output = "rotor_inspection", property = "volume_mm3", min = area*6-0.000001, max = area*6+0.000001)
    section_view(id = "rotor_midsection", output = "rotor_inspection", origin = [0,0,3], min_area = area-0.000001, max_area = area+0.000001)
    section_circle_dimension(view = "rotor_midsection", id = "central_bore_diameter", from_feature = bored, role = "bore_wall", kind = "diameter", label_offset = [0,8], min = 3.999999, max = 4.000001)
    assert_faces(id = "central_bore_survives", from_feature = bored, role = "bore_wall", in_feature = rotor_body, count = 1)
    for index in [0,1]:
        assert_faces(id = "mounting_bore_"+str(index)+"_survives", from_feature = "mounting_hole_"+str(index), role = "bore_wall", in_feature = rotor_body, count = 1)
    for feature, label in [(front_left,[-5,-5]),(front_right,[5,-5]),(back_right,[5,5]),(rounded,[-5,5])]:
        assert_faces(id = feature+"_survives", from_feature = feature, role = "fillet_wall", in_feature = rotor_body, count = 1)
        section_circle_dimension(view = "rotor_midsection", id = feature+"_radius", from_feature = feature, role = "fillet_wall", kind = "radius", coverage = "single_arc", label_offset = label, min = corner_radius-0.000001, max = corner_radius+0.000001)
model()
