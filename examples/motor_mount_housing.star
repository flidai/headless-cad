# Machined motor-mount housing and removable cover, displayed side by side.
# Native geometry, floor dimensions and material checks; no load/retention rating.
height = param(name = "height", default = 24)
recess_width = param(name = "recess_width", default = 52)
recess_depth = param(name = "recess_depth", default = 12)
cable_depth = param(name = "cable_depth", default = 18)
pocket_side = param(name = "pocket_side", default = 1)

def validate_parameters():
    if height < 22 or height > 28:
        fail("height must be 22..28 mm")
    if recess_width < 46 or recess_width > 58:
        fail("recess_width must be 46..58 mm")
    if recess_depth < 10 or recess_depth > 14:
        fail("recess_depth must be 10..14 mm")
    if cable_depth < 16 or cable_depth > 19:
        fail("cable_depth must be 16..19 mm")
    if pocket_side != 0 and pocket_side != 1:
        fail("pocket_side must be 0 or 1")
validate_parameters()

# Explicit tangent line/arc contour: segment names remain source-owned.
def rounded_rectangle(name, x, y, w, h, r, holes = []):
    return path(id = name, start = [x+r,y], circular_holes = holes, segments = [
        {"kind":"line_to","end":[x+w-r,y]},
        {"kind":"arc_to","center":[x+w-r,y+r],"end":[x+w,y+r]},
        {"kind":"line_to","end":[x+w,y+h-r]},
        {"kind":"arc_to","center":[x+w-r,y+h-r],"end":[x+w-r,y+h]},
        {"kind":"line_to","end":[x+r,y+h]},
        {"kind":"arc_to","center":[x+r,y+h-r],"end":[x,y+h-r]},
        {"kind":"line_to","end":[x,y+r]},
        {"kind":"arc_to","center":[x+r,y+r],"end":[x+r,y]},
    ])

cover_holes = [{"center":[x,y],"radius":2.5} for x in [12,108] for y in [10,90]]
outer = rounded_rectangle("outer_profile", 0, 0, 120, 100, 6, cover_holes)
stock = extrude(id = "housing_stock", profile = outer, height = height)
mounting = slot_pattern(id = "mounting_slots", body = stock, frame_origin = [0,0,0],
    slots = [{"id":name,"center":[x,50],"direction":[0,1],"centerline_length":60,"radius":3}
        for name,x in [("left",12),("right",108)]])
cap = "end_cap" if pocket_side == 1 else "start_cap"
back = "start_cap" if pocket_side == 1 else "end_cap"
opening_z = height if pocket_side == 1 else 0
motor_profile = rounded_rectangle("motor_profile", 50-recess_width/2, 20, recess_width, 60, 5)
motor_recess = blind_profile(id = "motor_recess", body = mounting, profile = motor_profile,
    from_feature = stock, cap = cap, depth = recess_depth, frame_origin = [0,0,opening_z])
cable_profile = rounded_rectangle("cable_profile", 86, 34, 12, 32, 3)
housing = blind_profile(id = "cable_pocket", body = motor_recess, profile = cable_profile,
    from_feature = stock, cap = cap, depth = cable_depth, frame_origin = [0,0,opening_z])
cover = extrude(id = "cover_body", profile = outer, height = 3)
output(name = "stock", feature = mounting)
output(name = "first_stage", feature = motor_recess)
output(name = "housing", feature = housing)
output(name = "cover", feature = cover)
assert_faces(id = "motor_floor_survives", from_feature = motor_recess, role = "pocket_floor", in_feature = housing, count = 1)
assert_faces(id = "motor_walls_survive", from_feature = motor_recess, role = "pocket_walls", in_feature = housing, count = 8)
assert_faces(id = "left_slot_survives", from_feature = mounting, role = "slot_left_walls", in_feature = housing, count = 4)
assert_faces(id = "right_slot_survives", from_feature = mounting, role = "slot_right_walls", in_feature = housing, count = 4)

housing_part = part(id = "housing_part", body = housing, part_number = "MOTOR-HOUSING", revision = "A",
    description = "Two separately machined line/arc cavities with adjustable mounting slots",
    material = "Aluminium", density_kg_per_m3 = 2700, datums = {
        "opening":face_datum(from_feature = stock, role = cap, x_direction = [1,0,0]),
        "back":face_datum(from_feature = stock, role = back, x_direction = [1,0,0]),
        "motor_floor":face_datum(from_feature = motor_recess, role = "pocket_floor", x_direction = [1,0,0]),
        "cable_floor":face_datum(from_feature = housing, role = "pocket_floor", x_direction = [1,0,0]),
    })
cover_part = part(id = "cover_part", body = cover, part_number = "MOTOR-COVER", revision = "A",
    description = "Removable cover shown beside housing for inspection",
    material = "Aluminium", density_kg_per_m3 = 2700)
layout = assembly(id = "motor_mount_housing", instances = [
    instance(id = "housing", part = housing_part),
    instance(id = "cover", part = cover_part, offset = [0,120,0]),
])
assembly_output(name = "motor_mount_housing", assembly = layout)

def measure_depth(name, floor, depth):
    datum_dimension(id = name, output = "motor_mount_housing",
        from_instance_path = ["housing"], from_datum = "opening",
        to_instance_path = ["housing"], to_datum = floor, direction = -1,
        min = depth-0.000001, max = depth+0.000001)
measure_depth("motor_depth", "motor_floor", recess_depth)
measure_depth("cable_depth", "cable_floor", cable_depth)
datum_dimension(id = "remaining_web", output = "motor_mount_housing",
    from_instance_path = ["housing"], from_datum = "cable_floor",
    to_instance_path = ["housing"], to_datum = "back", direction = -1,
    min = height-cable_depth-0.000001, max = height-cable_depth+0.000001)

pi = 3.141592653589793
outer_area = 120*100-(4-pi)*36-4*pi*2.5*2.5
stock_area = outer_area-2*(2*3*60+pi*9)
motor_area = recess_width*60-(4-pi)*25
cable_area = 12*32-(4-pi)*9
housing_volume = stock_area*height-motor_area*recess_depth-cable_area*cable_depth
assert_property(id = "machined_volume", output = "housing", property = "volume_mm3",
    min = housing_volume-0.00001, max = housing_volume+0.00001)
assert_property(id = "layout_material_mass", output = "motor_mount_housing", property = "mass_kg",
    min = (housing_volume+outer_area*3)*2700/1000000000-0.00000001,
    max = (housing_volume+outer_area*3)*2700/1000000000+0.00000001)

def depth_section(name, depth, area):
    section_view(id = name, output = "housing", origin = [0,0,height-depth if pocket_side == 1 else depth],
        min_area = area-0.000001, max_area = area+0.000001)
depth_section("both_cavities", recess_depth/2, stock_area-motor_area-cable_area)
depth_section("cable_only", (recess_depth+cable_depth)/2, stock_area-cable_area)
depth_section("remaining_stock", (height+cable_depth)/2, stock_area)
drawing_sheet(id = "motor_housing_machining", title = "Motor mount — native line/arc cavities", revision = "A",
    width = 420, height = 297, views = [
        {"view":name,"x":x,"y":10,"width":130,"height":170,"scale":0.8,
         "paper_annotation_font_mm":2.8,"paper_caption":caption,"annotation_labels":[]}
        for name,x,caption in [("both_cavities",10,"BOTH CAVITIES"),("cable_only",145,"DEEP CABLE POCKET"),("remaining_stock",280,"REMAINING STOCK")]
    ], dimension_tables = [{"id":"depths","title":"NATIVE FLOOR MEASUREMENTS","output":"motor_mount_housing",
        "x":10,"y":190,"width":400,"height":55,"rows":[
            {"dimension":"motor_depth","label":"Motor recess depth"},
            {"dimension":"cable_depth","label":"Cable pocket depth"},
            {"dimension":"remaining_web","label":"Remaining web below deeper floor"},
        ]}])
