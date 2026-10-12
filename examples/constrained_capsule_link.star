# A solved line/arc outline and two solved bore radii. The coordinates below
# are starting guesses; native geometry uses the constraint solution.
span = param(name = "span", default = 60)
outer_radius = param(name = "outer_radius", default = 10)
bore_radius = param(name = "bore_radius", default = 4)
def validate_parameters():
    if span < 40 or span > 80:
        fail("span must be 40..80 mm")
    if outer_radius < 8 or outer_radius > 14:
        fail("outer_radius must be 8..14 mm")
    if bore_radius < 3 or bore_radius > 5:
        fail("bore_radius must be 3..5 mm")
validate_parameters()
points = [
    {"id":"left_center","position":[0,0]}, {"id":"right_center","position":[60,0]},
    {"id":"a","position":[0,-10]}, {"id":"b","position":[60,-10]},
    {"id":"c","position":[60,10]}, {"id":"d","position":[0,10]},
    {"id":"left_rim","position":[4,0]}, {"id":"right_rim","position":[64,0]},
]
constraints = [
    {"id":"ground","kind":"fixed","point":"left_center","position":[0,0]},
    {"id":"axis","kind":"horizontal","a":"left_center","b":"right_center"},
    {"id":"span","kind":"distance","a":"left_center","b":"right_center","value":span},
]
endpoints = [["a","left_center"],["b","right_center"],["c","right_center"],["d","left_center"]]
constraints += [{"id":p[0]+"_vertical","kind":"vertical","a":p[1],"b":p[0]} for p in endpoints]
constraints += [{"id":p[0]+"_radius","kind":"distance","a":p[1],"b":p[0],"value":outer_radius} for p in endpoints]
constraints += [{"id":side+"_bore_gauge","kind":"horizontal","a":side+"_center","b":side+"_rim"} for side in ["left","right"]]
constraints += [{"id":side+"_bore_radius","kind":"distance","a":side+"_center","b":side+"_rim","value":bore_radius} for side in ["left","right"]]
layout = sketch(id = "capsule_layout", points = points, constraints = constraints,
    curve_outline = {"start":"a","segments":[
        {"kind":"line_to","end":"b"},
        {"kind":"arc_to","center":"right_center","end":"c"},
        {"kind":"line_to","end":"d"},
        {"kind":"arc_to","center":"left_center","end":"a"},
    ]}, circular_holes = [{"center":side+"_center","rim":side+"_rim"} for side in ["left","right"]])
link = extrude(id = "capsule_body", profile = layout, height = 8)
output(name = "link", feature = link)
# The initial signs choose the top/bottom endpoint branches and positive span;
# independent acceptance verifies that solved branch across parameter changes.
link_part = part(id = "link", body = link, part_number = "CL-LINK", material = "Aluminum", density_kg_per_m3 = 2700)
spacer = part(id = "spacer", body = extrude(id = "spacer_body", profile = circle(id = "spacer_profile", radius = outer_radius-2, inner_radius = bore_radius), height = 10), part_number = "CL-SPACER", material = "Steel", density_kg_per_m3 = 7800)
pin = part(id = "pin", body = revolve(id = "pin_body", profile = polygon(id = "pin_profile", points = [[0,0],[bore_radius+2,0],[bore_radius+2,6],[bore_radius-0.5,6],[bore_radius-0.5,34],[0,34]])), part_number = "CL-PIN", material = "Steel", density_kg_per_m3 = 7800)
# Prescribed coaxial placements share declared span with the solved sketch;
# the pin clearance is design intent, not a fit or motion certification.
assembly_output(name = "constrained_capsule_link", assembly = assembly(id = "constrained_capsule_link", instances = [
    instance(id = "lower_link", part = link_part, offset = [0,0,6]),
    instance(id = "upper_link", part = link_part, offset = [0,0,24]),
]+[instance(id = "spacer_"+str(i), part = spacer, offset = [x,0,14]) for i,x in enumerate([0,span])]
 +[instance(id = "pin_"+str(i), part = pin, offset = [x,0,0]) for i,x in enumerate([0,span])]))
assert_faces(id = "right_arc", from_feature = link, role = "segment_1", in_feature = link, count = 1)
assert_faces(id = "left_arc", from_feature = link, role = "segment_3", in_feature = link, count = 1)
assert_property(id = "link_material", output = "link", property = "volume_mm3", min = 5000, max = 23000)
assert_property(id = "fixture_mass", output = "constrained_capsule_link", property = "mass_kg", max = 0.4)
