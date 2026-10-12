# Three precision guide bores driven by solved center points and radius handles.
# Each horizontal radius handle removes a construction-point rotational freedom;
# it does not represent circle tangency or a general circular sketch entity.
pitch = param(name = "pitch", default = 25)
bore_radius = param(name = "bore_radius", default = 4)
thickness = param(name = "thickness", default = 8)
def validate_parameters():
    if pitch < 20 or pitch > 30:
        fail("pitch must be 20..30 mm")
    if bore_radius < 3 or bore_radius > 5:
        fail("bore_radius must be 3..5 mm")
    if thickness < 6 or thickness > 10:
        fail("thickness must be 6..10 mm")
validate_parameters()
points = [
    {"id":"a","position":[0,0]}, {"id":"b","position":[100,0]},
    {"id":"c","position":[100,60]}, {"id":"d","position":[0,60]},
    {"id":"left","position":[24,29]}, {"id":"middle","position":[50,30]},
    {"id":"right","position":[76,31]},
    {"id":"left_rim","position":[28,29]}, {"id":"middle_rim","position":[54,30]},
    {"id":"right_rim","position":[80,31]},
]
constraints = [
    {"id":"corner_a","kind":"fixed","point":"a","position":[0,0]},
    {"id":"corner_b","kind":"fixed","point":"b","position":[100,0]},
    {"id":"corner_c","kind":"fixed","point":"c","position":[100,60]},
    {"id":"corner_d","kind":"fixed","point":"d","position":[0,60]},
    {"id":"datum_center","kind":"fixed","point":"middle","position":[50,30]},
    {"id":"left_level","kind":"horizontal","a":"left","b":"middle"},
    {"id":"left_pitch","kind":"distance","a":"left","b":"middle","value":pitch},
    {"id":"right_level","kind":"horizontal","a":"middle","b":"right"},
    {"id":"right_pitch","kind":"distance","a":"middle","b":"right","value":pitch},
]
constraints += [{"id":center+"_radius_gauge","kind":"horizontal","a":center,"b":center+"_rim"} for center in ["left","middle","right"]]
constraints += [{"id":center+"_radius","kind":"distance","a":center,"b":center+"_rim","value":bore_radius} for center in ["left","middle","right"]]
profile = sketch(id = "guide_layout", points = points, constraints = constraints,
    outline = ["a","b","c","d"],
    circular_holes = [{"center":center,"rim":center+"_rim"} for center in ["left","middle","right"]])
plate = extrude(id = "guide_plate", profile = profile, height = thickness)
output(name = "plate", feature = plate)
assert_faces(id = "bore_0", from_feature = plate, role = "circular_hole_0_wall", in_feature = plate, count = 1)
assert_faces(id = "bore_1", from_feature = plate, role = "circular_hole_1_wall", in_feature = plate, count = 1)
assert_faces(id = "bore_2", from_feature = plate, role = "circular_hole_2_wall", in_feature = plate, count = 1)
assert_property(id = "plate_material", output = "plate", property = "volume_mm3", min = 34500, max = 59200)
plate_part = part(id = "plate", body = plate, part_number = "DJ-PLATE", material = "Aluminum", density_kg_per_m3 = 2700)
guide = part(id = "guide", body = extrude(id = "guide_body", profile = circle(id = "guide_profile", radius = bore_radius, inner_radius = bore_radius-1), height = thickness+6), part_number = "DJ-GUIDE", material = "Steel", density_kg_per_m3 = 7800)
foot = part(id = "foot", body = extrude(id = "foot_body", profile = rectangle(id = "foot_profile", width = 100, depth = 8), height = 12), part_number = "DJ-FOOT", material = "Aluminum", density_kg_per_m3 = 2700)
# Guide placements reuse the declared pitch. They are prescribed transforms,
# not an association to solved circular faces or a fit/contact certification.
assembly_output(name = "constrained_drill_jig", assembly = assembly(id = "constrained_drill_jig", instances = [
    instance(id = "plate", part = plate_part, offset = [0,0,12]),
    instance(id = "front_foot", part = foot),
    instance(id = "rear_foot", part = foot, offset = [0,52,0]),
]+[instance(id = "guide_"+str(i), part = guide, offset = [x,30,12]) for i,x in enumerate([50-pitch,50,50+pitch])]))
# Distances admit mirrored branches; the captured starting guesses select the
# intended left-to-right layout, and acceptance checks verify the solved centers.
