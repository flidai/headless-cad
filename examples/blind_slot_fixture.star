# A machined fixture with two blind obround pockets and four through bores.
# Pocket floors and the intact back face remain native analytic geometry.
plate_width = param(name = "plate_width", default = 120)
plate_height = 12
pocket_depth = param(name = "pocket_depth", default = 4)
pocket_length = param(name = "pocket_length", default = 24)
pocket_radius = 6
pocket_side = param(name = "pocket_side", default = 1)

def validate_fixture():
    if plate_width < 96 or plate_width > 180:
        fail("plate_width must be 96..180 mm")
    if pocket_depth < 2 or pocket_depth > 8:
        fail("pocket_depth must be 2..8 mm")
    if pocket_length < 12 or pocket_length > 36:
        fail("pocket_length must be 12..36 mm")
    if pocket_side != 0 and pocket_side != 1:
        fail("pocket_side must be 0 (start) or 1 (end)")
validate_fixture()

cap = "end_cap" if pocket_side == 1 else "start_cap"
back_cap = "start_cap" if pocket_side == 1 else "end_cap"
opening_z = plate_height if pocket_side == 1 else 0
inside_z = plate_height-pocket_depth/2 if pocket_side == 1 else pocket_depth/2
web_z = plate_height-pocket_depth-1 if pocket_side == 1 else pocket_depth+1
stock = extrude(id = "fixture_stock", profile = polygon(id = "fixture_profile",
    points = [[0,0],[plate_width,0],[plate_width,80],[0,80]],
    circular_holes = [{"center":[x,y],"radius":3} for x in [10,plate_width-10] for y in [10,70]]), height = plate_height)
fixture = blind_slot_pattern(id = "fixture_pockets", body = stock, from_feature = stock,
    cap = cap, depth = pocket_depth,
    frame_origin = [0,0,opening_z], frame_x_direction = [1,0,0], frame_y_direction = [0,1,0],
    slots = [{"id":name,"center":[x,40],"direction":[0,1],"centerline_length":pocket_length,"radius":pocket_radius}
        for name,x in [("left",plate_width/3),("right",plate_width*2/3)]])
output(name = "fixture", feature = fixture)

def assert_pocket_faces():
    for name in ["left","right"]:
        assert_faces(id = name+"_floor_exists", from_feature = fixture, role = "slot_"+name+"_floor", in_feature = fixture, count = 1)
        assert_faces(id = name+"_walls_exist", from_feature = fixture, role = "slot_"+name+"_walls", in_feature = fixture, count = 4)
assert_pocket_faces()

pi = 3.141592653589793
stock_area = plate_width*80-4*pi*3*3
pocket_area = 2*pocket_radius*pocket_length+pi*pocket_radius*pocket_radius
section_view(id = "inside_pockets", output = "fixture", origin = [0,0,inside_z],
    min_area = stock_area-2*pocket_area-0.000001, max_area = stock_area-2*pocket_area+0.000001)
section_view(id = "intact_web", output = "fixture", origin = [0,0,web_z],
    min_area = stock_area-0.000001, max_area = stock_area+0.000001)

fixture_part = part(id = "blind_fixture_part", body = fixture,
    part_number = "FIX-BLIND", revision = "A", description = "Two blind capsule pockets with intact web",
    datums = {
        "opening":face_datum(from_feature = fixture, role = cap, x_direction = [1,0,0]),
        "back":face_datum(from_feature = fixture, role = back_cap, x_direction = [1,0,0]),
        "left_floor":face_datum(from_feature = fixture, role = "slot_left_floor", x_direction = [1,0,0]),
        "right_floor":face_datum(from_feature = fixture, role = "slot_right_floor", x_direction = [1,0,0]),
    })
fixture_layout = assembly(id = "blind_fixture_layout", instances = [instance(id = "fixture_leaf", part = fixture_part)])
assembly_output(name = "blind_fixture_layout", assembly = fixture_layout)

def measure_pocket_depths():
    for side in ["left","right"]:
        datum_dimension(id = side+"_pocket_depth", output = "blind_fixture_layout",
            from_instance_path = ["fixture_leaf"], from_datum = "opening",
            to_instance_path = ["fixture_leaf"], to_datum = side+"_floor", direction = -1,
            min = pocket_depth-0.000001, max = pocket_depth+0.000001)
measure_pocket_depths()
datum_dimension(id = "remaining_web", output = "blind_fixture_layout",
    from_instance_path = ["fixture_leaf"], from_datum = "left_floor",
    to_instance_path = ["fixture_leaf"], to_datum = "back", direction = -1,
    min = plate_height-pocket_depth-0.000001, max = plate_height-pocket_depth+0.000001)

drawing_sheet(id = "blind_fixture_machining", title = "Blind pocket fixture — sections and native depths",
    revision = "A", width = 420, height = 297,
    views = [
        {"view":"inside_pockets","x":10,"y":10,"width":195,"height":170,"scale":0.8,"paper_annotation_font_mm":2.8,"paper_caption":"INSIDE POCKETS","annotation_labels":[]},
        {"view":"intact_web","x":215,"y":10,"width":195,"height":170,"scale":0.8,"paper_annotation_font_mm":2.8,"paper_caption":"INTACT WEB","annotation_labels":[]},
    ],
    dimension_tables = [{"id":"pocket_depths","title":"NATIVE FLOOR MEASUREMENTS","output":"blind_fixture_layout","x":10,"y":190,"width":400,"height":55,
        "rows":[{"dimension":"left_pocket_depth","label":"Left pocket depth"},{"dimension":"right_pocket_depth","label":"Right pocket depth"},{"dimension":"remaining_web","label":"Remaining material below floor"}]}])
