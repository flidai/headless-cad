# Two removable height-setting inserts sit on actual, independently machined floors.
# Local seating certifies contact disks, not retention, preload or rated loads.
load("mixed_slot_fixture.star", "fixture_part", "first_depth", "second_depth", "pocket_side", "plate_width", "height")

insert_clearance = param(name = "insert_clearance", default = 0.2)
insert_lift = param(name = "insert_lift", default = 0)
required_seat_radius = param(name = "required_seat_radius", default = 1)
first_insert_shift = param(name = "first_insert_shift", default = 0)
required_side_clearance = param(name = "required_side_clearance", default = 0.04)
proud_height = 2

def validate_inserts():
    if first_insert_shift < -1 or first_insert_shift > 1:
        fail("first_insert_shift must be -1..1 mm")
    if required_side_clearance <= 0 or required_side_clearance > 1:
        fail("required_side_clearance must be positive and at most 1 mm")
    if insert_clearance < 0.05 or insert_clearance > 1:
        fail("insert_clearance must be 0.05..1 mm")
    if insert_lift < 0 or insert_lift > 1:
        fail("insert_lift must be 0..1 mm")
    if required_seat_radius < 0.1 or required_seat_radius > 10:
        fail("required_seat_radius must be 0.1..10 mm")
validate_inserts()

def make_insert(name, length, pocket_radius, depth):
    r = pocket_radius-insert_clearance
    half = length/2
    profile = path(id = name+"_profile", start = [r,-half], segments = [
        {"kind":"line_to","end":[r,half]},
        {"kind":"arc_to","center":[0,half],"end":[-r,half]},
        {"kind":"line_to","end":[-r,-half]},
        {"kind":"arc_to","center":[0,-half],"end":[r,-half]},
    ])
    body = extrude(id = name+"_body", profile = profile, height = depth+proud_height)
    output(name = name, feature = body)
    return part(id = name, body = body, part_number = "FIX-"+name, revision = "A",
        description = "Removable height-setting insert; nominal side clearance, no retention model",
        material = "Steel", density_kg_per_m3 = 7800,
        datums = {
            "mount":datum(),
            "seat":face_datum(from_feature = body, role = "start_cap", x_direction = [1,0,0]),
            "top":face_datum(from_feature = body, role = "end_cap", x_direction = [1,0,0]),
        })

first_insert = make_insert("first_insert", 20, 5, first_depth)
second_insert = make_insert("second_insert", 22, 6, second_depth)
seated = assembly(id = "seated_pocket_inserts", instances = [
    instance(id = "fixture", part = fixture_part),
    instance(id = "first_insert", part = first_insert, attach_to = "fixture",
        datum = "mount", target_datum = "first_floor", offset = [first_insert_shift,0,insert_lift]),
    instance(id = "second_insert", part = second_insert, attach_to = "fixture",
        datum = "mount", target_datum = "second_floor", offset = [0,0,insert_lift]),
])
assembly_output(name = "seated_pocket_inserts", assembly = seated)

def check_insert(name, floor):
    nominal_seating_check(id = name+"_floor_seat", output = "seated_pocket_inserts",
        left_instance_path = [name], left_datum = "seat",
        right_instance_path = ["fixture"], right_datum = floor,
        min_shared_disk_radius_mm = required_seat_radius)
    pocket_fit_check(id = name+"_pocket_fit", output = "seated_pocket_inserts",
        pocket_instance_path = ["fixture"], pocket_datum = floor,
        insert_instance_path = [name], insert_datum = "seat",
        min_side_clearance_mm = required_side_clearance)
    datum_dimension(id = name+"_proud_height", output = "seated_pocket_inserts",
        from_instance_path = ["fixture"], from_datum = "opening",
        to_instance_path = [name], to_datum = "top", direction = 1,
        min = proud_height-0.000001, max = proud_height+0.000001)
check_insert("first_insert", "first_floor")
check_insert("second_insert", "second_floor")

# Section through the two protruding inserts, independent of pocket depths.
section_view(id = "insert_tops", output = "seated_pocket_inserts",
    origin = [0,0,height+1 if pocket_side == 1 else -1])
drawing_sheet(id = "seated_insert_heights", title = "Height-setting inserts — nominal floor seating",
    revision = "A", width = 420, height = 297,
    views = [{"view":"insert_tops","x":10,"y":10,"width":400,"height":170,"scale":1,
        "paper_annotation_font_mm":2.8,"paper_caption":"INSERT TOP SECTION","annotation_labels":[]}],
    dimension_tables = [{"id":"insert_heights","title":"NATIVE PROTRUSION MEASUREMENTS","output":"seated_pocket_inserts",
        "x":10,"y":190,"width":400,"height":45,"rows":[
            {"dimension":"first_insert_proud_height","label":"First insert above opening"},
            {"dimension":"second_insert_proud_height","label":"Second insert above opening"},
        ]}])
