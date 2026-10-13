# Two separate machining operations. The first floor remains a semantic
# reference after the second operation inserts new walls and remaps floor IDs.
plate_width = param(name = "plate_width", default = 120)
first_depth = param(name = "first_depth", default = 3)
second_depth = param(name = "second_depth", default = 7)
pocket_side = param(name = "pocket_side", default = 1)
height = 12

def validate_parameters():
    if plate_width < 96 or plate_width > 144:
        fail("plate_width must be 96..144 mm")
    if first_depth < 2 or first_depth > 5:
        fail("first_depth must be 2..5 mm")
    if second_depth < 6 or second_depth > 9:
        fail("second_depth must be 6..9 mm")
    if pocket_side != 0 and pocket_side != 1:
        fail("pocket_side must be 0 or 1")
validate_parameters()

cap = "end_cap" if pocket_side == 1 else "start_cap"
back_cap = "start_cap" if pocket_side == 1 else "end_cap"
opening_z = height if pocket_side == 1 else 0
stock = extrude(id = "serial_stock", profile = polygon(id = "serial_profile",
    points = [[0,0],[plate_width,0],[plate_width,80],[0,80]],
    circular_holes = [{"center":[x,y],"radius":3} for x in [10,plate_width-10] for y in [10,70]]), height = height)
first_cut = blind_slot_pattern(id = "locating_pocket", body = stock,
    from_feature = stock, cap = cap, depth = first_depth,
    frame_origin = [0,0,opening_z], frame_x_direction = [1,0,0], frame_y_direction = [0,1,0],
    slots = [{"id":"locating","center":[plate_width/3,40],"direction":[0,1],"centerline_length":20,"radius":5}])
second_cut = blind_slot_pattern(id = "relief_pocket", body = first_cut,
    from_feature = stock, cap = cap, depth = second_depth,
    frame_origin = [0,0,opening_z], frame_x_direction = [1,0,0], frame_y_direction = [0,1,0],
    slots = [{"id":"relief","center":[plate_width*2/3,40],"direction":[0,1],"centerline_length":22,"radius":6}])
output(name = "first_stage", feature = first_cut)
output(name = "fixture", feature = second_cut)
assert_faces(id = "first_floor_survives", from_feature = first_cut, role = "slot_locating_floor", in_feature = second_cut, count = 1)
assert_faces(id = "first_walls_survive", from_feature = first_cut, role = "slot_locating_walls", in_feature = second_cut, count = 4)

fixture_part = part(id = "serial_fixture_part", body = second_cut,
    part_number = "FIX-SERIAL", revision = "A", description = "Separate locating and relief pocket operations",
    datums = {
        "opening":face_datum(from_feature = stock, role = cap, x_direction = [1,0,0]),
        "back":face_datum(from_feature = stock, role = back_cap, x_direction = [1,0,0]),
        "first_floor":face_datum(from_feature = first_cut, role = "slot_locating_floor", x_direction = [1,0,0]),
        "second_floor":face_datum(from_feature = second_cut, role = "slot_relief_floor", x_direction = [1,0,0]),
    })
layout = assembly(id = "serial_fixture_layout", instances = [instance(id = "fixture_leaf", part = fixture_part)])
assembly_output(name = "serial_fixture_layout", assembly = layout)

def measure_depth(name, floor, depth):
    datum_dimension(id = name, output = "serial_fixture_layout",
        from_instance_path = ["fixture_leaf"], from_datum = "opening",
        to_instance_path = ["fixture_leaf"], to_datum = floor, direction = -1,
        min = depth-0.000001, max = depth+0.000001)
measure_depth("locating_depth", "first_floor", first_depth)
measure_depth("relief_depth", "second_floor", second_depth)
datum_dimension(id = "remaining_web", output = "serial_fixture_layout",
    from_instance_path = ["fixture_leaf"], from_datum = "second_floor",
    to_instance_path = ["fixture_leaf"], to_datum = "back", direction = -1,
    min = height-second_depth-0.000001, max = height-second_depth+0.000001)

pi = 3.141592653589793
stock_area = plate_width*80-4*pi*9
first_area = 2*5*20+pi*25
second_area = 2*6*22+pi*36

def depth_section(name, depth, expected_area):
    z = height-depth if pocket_side == 1 else depth
    section_view(id = name, output = "fixture", origin = [0,0,z],
        min_area = expected_area-0.000001, max_area = expected_area+0.000001)
depth_section("both_pockets", first_depth/2, stock_area-first_area-second_area)
depth_section("deep_pocket_only", (first_depth+second_depth)/2, stock_area-second_area)
depth_section("intact_web", second_depth+1, stock_area)

drawing_sheet(id = "serial_fixture_machining", title = "Serial blind machining — preserved floor references",
    revision = "A", width = 420, height = 297,
    views = [
        {"view":name,"x":x,"y":10,"width":130,"height":170,"scale":0.8,"paper_annotation_font_mm":2.8,"paper_caption":caption,"annotation_labels":[]}
        for name,x,caption in [("both_pockets",10,"BOTH POCKETS"),("deep_pocket_only",145,"DEEP POCKET ONLY"),("intact_web",280,"INTACT WEB")]
    ],
    dimension_tables = [{"id":"depths","title":"NATIVE FLOOR MEASUREMENTS","output":"serial_fixture_layout","x":10,"y":190,"width":400,"height":55,
        "rows":[{"dimension":"locating_depth","label":"First operation: locating depth"},{"dimension":"relief_depth","label":"Second operation: relief depth"},{"dimension":"remaining_web","label":"Material below deeper floor"}]}])
