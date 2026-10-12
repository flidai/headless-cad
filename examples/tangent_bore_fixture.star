# Two bored circles fit between directed construction lines. These are infinite
# supporting-line constraints, not finite-segment contacts or assembly mates.
half_width = param(name = "half_width", default = 30)
bore_radius = param(name = "bore_radius", default = 4)
thickness = param(name = "thickness", default = 8)
def validate_parameters():
    if half_width < 20 or half_width > 35:
        fail("half_width must be 20..35 mm")
    if bore_radius < 3 or bore_radius > 6:
        fail("bore_radius must be 3..6 mm")
    if thickness < 6 or thickness > 10:
        fail("thickness must be 6..10 mm")
validate_parameters()
anchors = [
    ["a",[0,0]], ["b",[100,0]], ["c",[100,80]], ["d",[0,80]],
    ["lower_vertex",[50,12]], ["lower_left",[50-half_width,72]], ["lower_right",[50+half_width,72]],
    ["upper_vertex",[50,68]], ["upper_left",[50-half_width,8]], ["upper_right",[50+half_width,8]],
]
points = [{"id":p[0],"position":p[1]} for p in anchors]+[
    {"id":"lower_center","position":[50,22]}, {"id":"lower_rim","position":[54,22]},
    {"id":"upper_center","position":[50,58]}, {"id":"upper_rim","position":[54,58]},
]
constraints = [{"id":"fix_"+p[0],"kind":"fixed","point":p[0],"position":p[1]} for p in anchors]
constraints += [{"id":n+"_radius_gauge","kind":"horizontal","a":n+"_center","b":n+"_rim"} for n in ["lower","upper"]]
constraints += [{"id":n+"_radius","kind":"distance","a":n+"_center","b":n+"_rim","value":bore_radius} for n in ["lower","upper"]]
constraints += [
    {"id":"lower_left_contact","kind":"circle_line_tangent","circle":["lower_center","lower_rim"],"line":["lower_vertex","lower_left"],"side":"right"},
    {"id":"lower_right_contact","kind":"circle_line_tangent","circle":["lower_center","lower_rim"],"line":["lower_vertex","lower_right"],"side":"left"},
    {"id":"upper_left_contact","kind":"circle_line_tangent","circle":["upper_center","upper_rim"],"line":["upper_vertex","upper_left"],"side":"left"},
    {"id":"upper_right_contact","kind":"circle_line_tangent","circle":["upper_center","upper_rim"],"line":["upper_vertex","upper_right"],"side":"right"},
]
layout = sketch(id = "bore_layout", points = points, constraints = constraints,
    outline = ["a","b","c","d"], circular_holes = [{"center":n+"_center","rim":n+"_rim"} for n in ["lower","upper"]])
plate = extrude(id = "bore_plate", profile = layout, height = thickness)
output(name = "plate", feature = plate)
assert_faces(id = "lower_bore", from_feature = plate, role = "circular_hole_0_wall", in_feature = plate, count = 1)
assert_faces(id = "upper_bore", from_feature = plate, role = "circular_hole_1_wall", in_feature = plate, count = 1)
assert_property(id = "plate_material", output = "plate", property = "volume_mm3", min = 46000, max = 79500)
plate_part = part(id = "plate", body = plate, part_number = "TB-PLATE", material = "Aluminum", density_kg_per_m3 = 2700)
foot = part(id = "foot", body = extrude(id = "foot_body", profile = rectangle(id = "foot_profile", width = 100, depth = 8), height = 12), part_number = "TB-FOOT", material = "Aluminum", density_kg_per_m3 = 2700)
clamp = part(id = "clamp", body = extrude(id = "clamp_body", profile = rectangle(id = "clamp_profile", width = 6, depth = 8), height = 4), part_number = "TB-CLAMP", material = "Steel", density_kg_per_m3 = 7800)
# The wedge rays are construction keepout lines. Clamps use prescribed corner
# placements; this fixture does not certify holding force, contact, or loads.
assembly_output(name = "tangent_bore_fixture", assembly = assembly(id = "tangent_bore_fixture", instances = [
    instance(id = "plate", part = plate_part, offset = [0,0,12]),
    instance(id = "front_foot", part = foot),
    instance(id = "rear_foot", part = foot, offset = [0,72,0]),
]+[instance(id = "clamp_"+str(i), part = clamp, offset = [p[0],p[1],12+thickness]) for i,p in enumerate([[0,0],[94,0],[0,72],[94,72]])]))
