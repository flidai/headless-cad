# Rounded bored housing and keyed shaft with a complete signed rotation proof.
# Only the named pair is certified. Nominal clearance is not a manufactured fit,
# bearing load rating, shaft retention design, or all-pairs collision proof.
pi = 3.141592653589793
shaft_radius = param(name = "shaft_radius", default = 6)
bore_gap = param(name = "bore_gap", default = 0.15)
required_clearance = param(name = "required_clearance", default = 0.1)
pose = param(name = "pose", default = 0)
turn_radians = param(name = "turn_radians", default = 2*pi)
def model():
    if shaft_radius < 5 or shaft_radius > 7 or bore_gap < 0.01 or bore_gap > 0.4 or required_clearance <= 0 or pose not in [0,1]:
        fail("shaft_radius 5..7, bore_gap .01...4, positive clearance and pose 0/1 required")
    stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 40, depth = 30), height = 12)
    front_left = fillet(id = "front_left", body = stock, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_min"}], radius = 4)
    front_right = fillet(id = "front_right", body = front_left, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_min"}], radius = 4)
    back_right = fillet(id = "back_right", body = front_right, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_max"}], radius = 4)
    rounded = fillet(id = "back_left", body = back_right, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_max"}], radius = 4)
    centered = translate(id = "centered", body = rounded, offset = [-20,-15,0])
    bore_tool = translate(id = "bore_tool", body = extrude(id = "bore_cylinder", profile = circle(id = "bore_profile", radius = shaft_radius+bore_gap), height = 14), offset = [0,0,-1])
    bored = cut(id = "central_bore", left = centered, right = bore_tool)
    drill = extrude(id = "mounting_drill", profile = circle(id = "mounting_profile", radius = 2), height = 14)
    housing_body = bored
    for index, x in enumerate([-14,14]):
        tool = translate(id = "mounting_tool_"+str(index), body = drill, offset = [x,0,-1])
        housing_body = cut(id = "mounting_hole_"+str(index), left = housing_body, right = tool)
    shaft_stock = extrude(id = "shaft_stock", profile = circle(id = "shaft_profile", radius = shaft_radius), height = 40)
    seat_blank = extrude(id = "seat_blank", profile = rectangle(id = "seat_profile", width = 4, depth = 4), height = 8)
    seat_tool = translate(id = "seat_tool", body = seat_blank, offset = [shaft_radius-2,-2,16])
    shaft_body = cut(id = "shaft_body", left = shaft_stock, right = seat_tool)
    housing = part(id = "housing_part", body = housing_body, part_number = "ROUNDED-HOUSING", revision = "A", description = "Rounded housing with one running bore and two actual mounting holes", material = "Aluminum", density_kg_per_m3 = 2700)
    shaft = part(id = "shaft_part", body = shaft_body, part_number = "KEYED-SHAFT", revision = "A", description = "Keyed shaft with intact journals; no retention or rated fit", material = "Steel", density_kg_per_m3 = 7800)
    pair = assembly(id = "fit_definition", instances = [instance(id = "housing", part = housing), instance(id = "shaft", part = shaft, offset = [0,0,-10])])
    frame = [0,0,0,1] if pose == 0 else [0.5,0.5,0.5,0.5]
    offset = [0,0,0] if pose == 0 else [13,-7,15]
    scene = assembly(id = "fit_scene", instances = [assembly_instance(id = "fixture", assembly = pair, offset = offset, rotation = frame)])
    assembly_output(name = "rounded_bearing_rotation", assembly = scene)
    output(name = "housing", feature = housing_body)
    output(name = "shaft", feature = shaft_body)
    clearance_check(id = "running_gap", output = "rounded_bearing_rotation", left_instance_path = ["fixture","housing"], right_instance_path = ["fixture","shaft"], min_clearance = required_clearance, rotation_group = "shaft_rotation")
    rigid_rotation_group(id = "shaft_rotation", output = "rounded_bearing_rotation", instance_paths = [["fixture","shaft"]], reference_instance_path = ["fixture","shaft"], pivot = [0,0,20], axis = [0,0,1], angle_radians = turn_radians)
    area = 1200-(4-pi)*16-pi*((shaft_radius+bore_gap)*(shaft_radius+bore_gap)+8)
    assert_property(id = "housing_volume", output = "housing", property = "volume_mm3", min = area*12-0.000001, max = area*12+0.000001)
    section_view(id = "housing_midsection", output = "housing", origin = [0,0,6], min_area = area-0.000001, max_area = area+0.000001)
    section_circle_dimension(view = "housing_midsection", id = "running_bore_diameter", from_feature = bored, role = "bore_wall", kind = "diameter", label_offset = [0,12], min = 2*(shaft_radius+bore_gap)-0.000001, max = 2*(shaft_radius+bore_gap)+0.000001)
    section_view(id = "shaft_journal", output = "shaft", origin = [0,0,5])
    section_circle_dimension(view = "shaft_journal", id = "journal_diameter", from_feature = shaft_stock, role = "wall", kind = "diameter", label_offset = [10,0], min = 2*shaft_radius-0.000001, max = 2*shaft_radius+0.000001)
    assert_faces(id = "keyseat_floor", from_feature = shaft_body, role = "keyseat_floor", in_feature = shaft_body, count = 1)
    for feature, label in [(front_left,[-8,-8]),(front_right,[8,-8]),(back_right,[8,8]),(rounded,[-8,8])]:
        assert_faces(id = feature+"_survives", from_feature = feature, role = "fillet_wall", in_feature = housing_body, count = 1)
        section_circle_dimension(view = "housing_midsection", id = feature+"_radius", from_feature = feature, role = "fillet_wall", kind = "radius", coverage = "single_arc", label_offset = label, min = 3.999999, max = 4.000001)
model()
