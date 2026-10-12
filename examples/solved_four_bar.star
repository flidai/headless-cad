# A closed planar pin/link graph. Initial coordinates are guesses, not placements.
# Rigid parts are placed from the numerical solution; the viewer only inspects it.
angle = param(name = "angle", default = 0.9)
coupler_length = param(name = "coupler_length", default = 70)
pose = param(name = "pose", default = 0)
def validate_parameters():
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
validate_parameters()
rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
layout = mechanism(id = "four_bar", offset = [10,-5,3], rotation = rotation,
    pins = [
        {"id":"a", "initial":[0,0], "ground":[0,0]},
        {"id":"b", "initial":[26,15]},
        {"id":"c", "initial":[80,50]},
        {"id":"d", "initial":[80,0], "ground":[80,0]},
    ],
    links = [
        {"id":"ground", "from":"a", "to":"d", "length":80},
        {"id":"crank", "from":"a", "to":"b", "length":30},
        {"id":"coupler", "from":"b", "to":"c", "length":coupler_length},
        {"id":"rocker", "from":"d", "to":"c", "length":50},
    ],
    drives = [{"id":"input_angle", "link":"crank", "angle_radians":angle}],
    branches = [{"id":"upper_closure", "pins":["b","d","c"], "side":"left"}])

def link_part(id, length, height):
    profile = path(id = id+"_profile", start = [0,-6], segments = [
        {"kind":"line_to", "end":[length,-6]},
        {"kind":"arc_to", "center":[length,0], "end":[length,6]},
        {"kind":"line_to", "end":[0,6]},
        {"kind":"arc_to", "center":[0,0], "end":[0,-6]},
    ], circular_holes = [{"center":[x,0], "radius":2} for x in [0,length]])
    body = extrude(id = id+"_body", profile = profile, height = height)
    return part(id = id, body = body, part_number = "FB-"+id,
        material = "Aluminum", density_kg_per_m3 = 2700,
        datums = {"start":datum(), "end":datum(offset = [length,0,0]),
                  "top":face_datum(from_feature = body, role = "end_cap", x_direction = [1,0,0])})

ground = link_part("ground", 80, 6)
crank = link_part("crank", 30, 4)
coupler = link_part("coupler", coupler_length, 4)
rocker = link_part("rocker", 50, 4)
pin = part(id = "pin", body = extrude(id = "pin_body", profile = circle(id = "pin_profile", radius = 1.8), height = 20),
    part_number = "FB-PIN", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"base":datum()})
# A solved rigid child and direct solved parts use the same binding contract.
crank_unit = assembly(id = "crank_unit", instances = [
    instance(id = "link", part = crank, offset = [0,0,8]),
    instance(id = "pin_b", part = pin, attach_to = "link", datum = "base", target_datum = "end", offset = [0,0,-8]),
])
linkage = assembly(id = "linkage", instances = [
    instance(id = "ground", part = ground),
    assembly_instance(id = "crank_unit", assembly = crank_unit),
    instance(id = "coupler", part = coupler, offset = [0,0,14]),
    instance(id = "rocker", part = rocker, offset = [0,0,8]),
    instance(id = "pin_a", part = pin, attach_to = "ground", datum = "base", target_datum = "start"),
    instance(id = "pin_d", part = pin, attach_to = "ground", datum = "base", target_datum = "end"),
    instance(id = "pin_c", part = pin, attach_to = "rocker", datum = "base", target_datum = "end", offset = [0,0,-8]),
])
mechanism_bind(assembly = linkage, instance = "ground", mechanism = layout, link = "ground")
mechanism_bind(assembly = linkage, instance = "crank_unit", mechanism = layout, link = "crank")
mechanism_bind(assembly = linkage, instance = "coupler", mechanism = layout, link = "coupler")
mechanism_bind(assembly = linkage, instance = "rocker", mechanism = layout, link = "rocker")
assembly_output(name = "solved_four_bar", assembly = linkage)
output(name = "coupler", feature = "coupler_body")
pi = 3.141592653589793
volume = (12*80+28*pi)*6 + (12*(30+coupler_length+50)+84*pi)*4
mass = volume*2700/1000000000 + 4*pi*1.8*1.8*20*7800/1000000000
assert_property(id = "material_mass", output = "solved_four_bar", property = "mass_kg", min = mass-0.0000001, max = mass+0.0000001)
section_view(id = "moving_coupler", output = "solved_four_bar", instance_path = ["coupler"], datum = "top", origin = [0,0,-2], min_area = 12*coupler_length+(28+4*1.8*1.8)*pi-0.000001, max_area = 12*coupler_length+(28+4*1.8*1.8)*pi+0.000001)
