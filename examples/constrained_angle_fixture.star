# Angle-driven support and an equal-leg trapezoid deck. These coordinates are
# starting guesses; no sine, cosine or tangent formula constructs either profile.
span = param(name = "span", default = 60)
tilt_degrees = param(name = "tilt_degrees", default = 30)
deck_top_width = param(name = "deck_top_width", default = 24)
def validate_parameters():
    if span < 50 or span > 80:
        fail("span must be 50..80 mm")
    if tilt_degrees < 15 or tilt_degrees > 45:
        fail("tilt_degrees must be 15..45 degrees")
    if deck_top_width < 18 or deck_top_width > 30:
        fail("deck_top_width must be 18..30 mm")
validate_parameters()
support_profile = sketch(id = "support_sketch", points = [
    {"id":"a","position":[0,0]}, {"id":"b","position":[span,1]}, {"id":"c","position":[span,30]},
], constraints = [
    {"id":"ground","kind":"fixed","point":"a","position":[0,0]},
    {"id":"base_level","kind":"horizontal","a":"a","b":"b"},
    {"id":"base_length","kind":"distance","a":"a","b":"b","value":span},
    {"id":"upright","kind":"perpendicular","a":["a","b"],"b":["b","c"]},
    {"id":"incline","kind":"angle","a":["a","b"],"b":["a","c"],"degrees":tilt_degrees},
], outline = ["a","b","c"])
support_prism = extrude(id = "support_prism", profile = support_profile, height = 60)
support_body = transform(id = "support_body", body = support_prism, offset = [0,60,0],
    rotation = [0.7071067811865476,0,0,0.7071067811865476])
support = part(id = "support", body = support_body, part_number = "CA-SUPPORT", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"inclined_seat":face_datum(from_feature = support_prism,role = "wall_2",x_direction = [1,0,0])})
deck_profile = sketch(id = "deck_sketch", points = [
    {"id":"a","position":[0,0]}, {"id":"b","position":[36,0]},
    {"id":"c","position":[32,40]}, {"id":"d","position":[4,40]},
], constraints = [
    {"id":"ground","kind":"fixed","point":"a","position":[0,0]},
    {"id":"base_level","kind":"horizontal","a":"a","b":"b"},
    {"id":"base_width","kind":"distance","a":"a","b":"b","value":36},
    {"id":"top_parallel","kind":"parallel","a":["a","b"],"b":["d","c"]},
    {"id":"top_width","kind":"distance","a":"d","b":"c","value":deck_top_width},
    {"id":"equal_legs","kind":"equal_length","a":["a","d"],"b":["b","c"]},
    {"id":"leg_length","kind":"distance","a":"a","b":"d","value":44},
], outline = ["a","b","c","d"])
deck_body = extrude(id = "deck_body", profile = deck_profile, height = 4)
deck = part(id = "deck",body = deck_body,part_number = "CA-DECK",material = "Steel",density_kg_per_m3 = 7800,
    datums = {"top_center":face_datum(from_feature = deck_body,role = "end_cap",x_direction = [1,0,0]),
              "left_pad":face_datum(from_feature = deck_body,role = "wall_3",x_direction = [0,0,1]),
              "right_pad":face_datum(from_feature = deck_body,role = "wall_1",x_direction = [0,0,1])})
def line(end):
    return {"kind":"line_to","end":end}
def arc(center,end):
    return {"kind":"arc_to","center":center,"end":end}
w = span+20
base_profile = path(id = "base_profile",start = [6,0],segments = [
    line([w-6,0]),arc([w-6,6],[w,6]),line([w,74]),arc([w-6,74],[w-6,80]),
    line([6,80]),arc([6,74],[0,74]),line([0,6]),arc([6,6],[6,0]),
],circular_holes = [{"center":p,"radius":3} for p in [[6,6],[w-6,6],[w-6,74],[6,74]]])
base = part(id = "base",body = extrude(id = "base_body",profile = base_profile,height = 6),part_number = "CA-BASE",material = "Aluminum",density_kg_per_m3 = 2700)
pad = part(id = "pad",body = extrude(id = "pad_body",profile = rectangle(id = "pad_profile",width = 2,depth = 8),height = 5),part_number = "CA-PAD",material = "Steel",density_kg_per_m3 = 7800,datums = {"foot":datum(offset = [1,4,0])})
bolt = part(id = "bolt",body = revolve(id = "bolt_body",profile = polygon(id = "bolt_profile",points = [[0,0],[2.5,0],[2.5,6],[4,6],[4,9],[0,9]])),part_number = "CA-BOLT",material = "Steel",density_kg_per_m3 = 7800)
assembly_output(name = "constrained_angle_fixture",assembly = assembly(id = "constrained_angle_fixture",instances = [
    instance(id = "base",part = base),instance(id = "support",part = support,offset = [10,10,6]),
    # Attach the deck's top center 4 mm above the seat so its underside is flush.
    instance(id = "deck",part = deck,attach_to = "support",datum = "top_center",target_datum = "inclined_seat",offset = [0,0,4]),
    instance(id = "left_pad",part = pad,attach_to = "deck",datum = "foot",target_datum = "left_pad"),
    instance(id = "right_pad",part = pad,attach_to = "deck",datum = "foot",target_datum = "right_pad"),
]+[instance(id = "bolt_"+str(i),part = bolt,offset = p) for i,p in enumerate([[6,6,0],[w-6,6,0],[w-6,74,0],[6,74,0]])]))
output(name = "support",feature = support_body)
output(name = "deck",feature = deck_body)
assert_faces(id = "inclined_seat",from_feature = support_prism,role = "wall_2",in_feature = support_body,count = 1)
assert_property(id = "support_material",output = "support",property = "volume_mm3",min = 18000,max = 200000)
assert_property(id = "deck_material",output = "deck",property = "volume_mm3",min = 4500,max = 6000)
assert_property(id = "mass_budget",output = "constrained_angle_fixture",property = "mass_kg",max = 0.75)
# This is a prescribed engineering fixture, not an assembly-joint or load solve.
