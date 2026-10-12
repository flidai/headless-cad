# A triangulated support dimensioned by three lengths. Positions below are only
# initial guesses; the headless solver determines the vertex coordinates.
span = param(name = "span", default = 80)
left_length = param(name = "left_length", default = 60)
right_length = param(name = "right_length", default = 70)
def validate_parameters():
    if span < 70 or span > 100:
        fail("span must be 70..100 mm")
    if left_length < 55 or left_length > 80:
        fail("left_length must be 55..80 mm")
    if right_length < 60 or right_length > 85:
        fail("right_length must be 60..85 mm")
validate_parameters()
profile = sketch(id = "support_sketch", points = [
    {"id":"a", "position":[0,0]},
    {"id":"b", "position":[span*0.9,2]},
    {"id":"c", "position":[span*0.35,30]},
], constraints = [
    {"id":"ground", "kind":"fixed", "point":"a", "position":[0,0]},
    {"id":"level_base", "kind":"horizontal", "a":"a", "b":"b"},
    {"id":"base_length", "kind":"distance", "a":"a", "b":"b", "value":span},
    {"id":"left_length", "kind":"distance", "a":"a", "b":"c", "value":left_length},
    {"id":"right_length", "kind":"distance", "a":"b", "b":"c", "value":right_length},
], outline = ["a","b","c"])
prism = extrude(id = "support_prism", profile = profile, height = 40)
support_body = transform(id = "support_body", body = prism, offset = [0,40,0],
    rotation = [0.7071067811865476,0,0,0.7071067811865476])
support = part(id = "support", body = support_body, part_number = "CS-SUPPORT", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"seat": face_datum(from_feature = prism, role = "wall_1", x_direction = [1,0,0])})
def line(end):
    return {"kind":"line_to", "end":end}
def arc(center,end):
    return {"kind":"arc_to", "center":center, "end":end}
w = span+20
base_profile = path(id = "base_profile", start = [6,0], segments = [
    line([w-6,0]), arc([w-6,6],[w,6]), line([w,54]), arc([w-6,54],[w-6,60]),
    line([6,60]), arc([6,54],[0,54]), line([0,6]), arc([6,6],[6,0]),
], circular_holes = [{"center":p,"radius":3} for p in [[6,6],[w-6,6],[w-6,54],[6,54]]])
base_body = extrude(id = "base_body", profile = base_profile, height = 6)
base = part(id = "base", body = base_body, part_number = "CS-BASE", material = "Aluminum", density_kg_per_m3 = 2700)
def box(id,width,depth,height):
    return extrude(id = id, profile = rectangle(id = id+"/profile", width = width, depth = depth), height = height)
deck_body = box("deck_body",24,48,3)
deck = part(id = "deck", body = deck_body, part_number = "CS-DECK", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"foot":datum(offset = [12,24,0]), "left_rail":datum(offset = [0,0,3]), "right_rail":datum(offset = [0,45,3])})
rail = part(id = "rail", body = box("rail_body",24,3,5), part_number = "CS-RAIL", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"foot":datum()})
bolt_body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [[0,0],[2.5,0],[2.5,6],[4,6],[4,9],[0,9]]))
bolt = part(id = "bolt", body = bolt_body, part_number = "CS-BOLT", material = "Steel", density_kg_per_m3 = 7800)
assembly_output(name = "constrained_slope_fixture", assembly = assembly(id = "fixture", instances = [
    instance(id = "base", part = base),
    instance(id = "support", part = support, offset = [10,10,6]),
    instance(id = "deck", part = deck, attach_to = "support", datum = "foot", target_datum = "seat"),
    instance(id = "left_rail", part = rail, attach_to = "deck", datum = "foot", target_datum = "left_rail"),
    instance(id = "right_rail", part = rail, attach_to = "deck", datum = "foot", target_datum = "right_rail"),
] + [instance(id = "bolt_"+str(i), part = bolt, offset = p) for i,p in enumerate([[6,6,0],[w-6,6,0],[w-6,54,0],[6,54,0]])]))
output(name = "support", feature = support_body)
assert_faces(id = "inclined_seat", from_feature = prism, role = "wall_1", in_feature = support_body, count = 1)
assert_property(id = "support_material", output = "support", property = "volume_mm3", min = 40000, max = 160000)
assert_property(id = "mass_budget", output = "constrained_slope_fixture", property = "mass_kg", max = 0.65)
# This is a constrained sketch and datum-prescribed assembly, not a load or
# nonlinear assembly-joint simulation. The positive initial branch is intentional.
