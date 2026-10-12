# Installed nominal shaft-retention geometry. The shoulder is integral and the
# open ring occupies a real groove. This is not hub retention, a standard ring
# specification, spring-installation proof or load-capacity certification.
# Bushes are shown at their intended housing seats; supporting housings are
# outside this isolated fixture. Shaft retention assumes those seats are fixed.
shaft_radius = param(name = "shaft_radius", default = 6)
mounting_pitch = param(name = "mounting_pitch", default = 60)
required_clearance = param(name = "required_clearance", default = 0.09)
r = shaft_radius
p = mounting_pitch
def validate_parameters():
    if r < 5 or r > 8 or p < 48 or p > 90:
        fail("shaft_radius must be 5..8 and mounting_pitch 48..90")
    if required_clearance <= 0:
        fail("required_clearance must be positive")
validate_parameters()
shaft_stock = revolve(id = "shaft_stock", profile = polygon(id = "shaft_profile", points = [
    [0,0],[r+2,0],[r+2,12.9],[r,12.9],[r,p+34.55],
    [r-0.6,p+34.55],[r-0.6,p+35.8],[r,p+35.8],[r,p+70],[0,p+70],
]))
seat_box = extrude(id = "seat_box", profile = rectangle(id = "seat_profile", width = 4, depth = 4.04), height = 13.5)
seat_tool = translate(id = "seat_tool", body = seat_box, offset = [r-2,-2.02,15+p/2])
shaft_body = cut(id = "shaft_body", left = shaft_stock, right = seat_tool)
bush_body = revolve(id = "bush_body", profile = polygon(id = "bush_profile", points = [
    [r+0.15,0],[r+6,0],[r+6,2],[r+4,2],[r+4,18],[r+0.15,18],
]))
washer_body = extrude(id = "washer_body", profile = circle(id = "washer_profile", radius = r+2, inner_radius = r+0.1), height = 1.5)
# A genuine 60-degree opening, constructed from native arcs (no complete ring
# hidden behind an envelope). Installed-state dimensions need a spring design.
ri = r-0.5
ro = r+1.5
cos30 = sqrt(3)/2
ring_body = extrude(id = "ring_body", profile = path(id = "ring_profile", start = [ro*cos30,ro/2], segments = [
    {"kind":"arc_to","center":[0,0],"end":[-ro,0],"clockwise":False},
    {"kind":"arc_to","center":[0,0],"end":[ro*cos30,-ro/2],"clockwise":False},
    {"kind":"line_to","end":[ri*cos30,-ri/2]},
    {"kind":"arc_to","center":[0,0],"end":[-ri,0],"clockwise":True},
    {"kind":"arc_to","center":[0,0],"end":[ri*cos30,ri/2],"clockwise":True},
    {"kind":"line_to","end":[ro*cos30,ro/2]},
]), height = 1.2)
shaft = part(id = "shaft", body = shaft_body, material = "Steel", density_kg_per_m3 = 7800, datums = {
    "shoulder":face_datum(from_feature = shaft_stock, role = "segment_2", x_direction = [1,0,0]),
    "loaded_groove_wall":face_datum(from_feature = shaft_stock, role = "segment_6", x_direction = [1,0,0]),
})
bush = part(id = "bush", body = bush_body, material = "Bronze", density_kg_per_m3 = 8800, datums = {
    "outer_thrust":face_datum(from_feature = bush_body, role = "segment_0", x_direction = [1,0,0]),
})
washer = part(id = "washer", body = washer_body, material = "Steel", density_kg_per_m3 = 7800, datums = {
    "start":face_datum(from_feature = washer_body, role = "start_cap", x_direction = [1,0,0]),
    "end":face_datum(from_feature = washer_body, role = "end_cap", x_direction = [1,0,0]),
})
ring = part(id = "retaining_ring", body = ring_body, material = "Spring steel (installed geometry only)", density_kg_per_m3 = 7800, datums = {
    "start":face_datum(from_feature = ring_body, role = "start_cap", x_direction = [1,0,0]),
    "end":face_datum(from_feature = ring_body, role = "end_cap", x_direction = [1,0,0]),
})
retention = assembly(id = "retention", instances = [
    instance(id = "shaft", part = shaft, offset = [0,0,10]),
    instance(id = "left_bush", part = bush, offset = [0,0,23]),
    instance(id = "right_bush", part = bush, offset = [0,0,p+43], rotation = [1,0,0,0]),
    instance(id = "washer", part = washer, offset = [0,0,p+43.1]),
    instance(id = "retaining_ring", part = ring, offset = [0,0,p+44.6]),
])
assembly_output(name = "retained_shaft", assembly = retention)
output(name = "shaft", feature = shaft_body)
output(name = "bush", feature = bush_body)
output(name = "washer", feature = washer_body)
output(name = "retaining_ring", feature = ring_body)
clearance_check(id = "left_bearing", output = "retained_shaft", left_instance_path = ["shaft"], right_instance_path = ["left_bush"], min_clearance = required_clearance)
clearance_check(id = "right_bearing", output = "retained_shaft", left_instance_path = ["shaft"], right_instance_path = ["right_bush"], min_clearance = required_clearance)
clearance_check(id = "washer_bearing", output = "retained_shaft", left_instance_path = ["washer"], right_instance_path = ["right_bush"], min_clearance = required_clearance)
# Intentional washer/ring and ring/groove-wall contacts are not positive-gap
# queries. Native semantic face datums retain the stop planes for independent
# measurement. The directed native dimensions and stack below certify the
# 0.20 mm nominal endplay and separate authored worst-case design interval.
pi = 3.141592653589793
b = 2.02
removed_area = b*sqrt(r*r-b*b)+r*r*asin(b/r)-2*(r-2)*b
shaft_volume = pi*((r+2)*(r+2)*12.9+r*r*(p+70-12.9)-1.25*(r*r-(r-0.6)*(r-0.6)))-13.5*removed_area
assert_property(id = "native_shaft_volume", output = "shaft", property = "volume_mm3", min = shaft_volume-0.000001, max = shaft_volume+0.000001)
assert_faces(id = "seat_floor", from_feature = shaft_body, role = "keyseat_floor", in_feature = shaft_body, count = 1)

# Native directed support-plane dimensions, independent of section projection.
# Bounds certify nominal geometry; authored +/-0.01 deviations describe design
# manufacturing limits, not manufactured-part conformance or GD&T.
datum_dimension(id = "shaft_stop_span", output = "retained_shaft",
    from_instance_path = ["shaft"], from_datum = "shoulder",
    to_instance_path = ["shaft"], to_datum = "loaded_groove_wall", direction = 1,
    tolerance = {"nominal":p+22.9,"lower":-0.01,"upper":0.01})
datum_dimension(id = "bush_stop_span", output = "retained_shaft",
    from_instance_path = ["left_bush"], from_datum = "outer_thrust",
    to_instance_path = ["right_bush"], to_datum = "outer_thrust", direction = -1,
    tolerance = {"nominal":p+20,"lower":-0.01,"upper":0.01})
datum_dimension(id = "washer_thickness", output = "retained_shaft",
    from_instance_path = ["washer"], from_datum = "start",
    to_instance_path = ["washer"], to_datum = "end", direction = -1,
    tolerance = {"nominal":1.5,"lower":-0.01,"upper":0.01})
datum_dimension(id = "ring_thickness", output = "retained_shaft",
    from_instance_path = ["retaining_ring"], from_datum = "start",
    to_instance_path = ["retaining_ring"], to_datum = "end", direction = -1,
    tolerance = {"nominal":1.2,"lower":-0.01,"upper":0.01})
tolerance_stack(id = "shaft_endplay", terms = [
    {"datum_dimension":"shaft_stop_span","coefficient":1},
    {"datum_dimension":"bush_stop_span","coefficient":-1},
    {"datum_dimension":"washer_thickness","coefficient":-1},
    {"datum_dimension":"ring_thickness","coefficient":-1},
], min = 0.1, max = 0.3)
