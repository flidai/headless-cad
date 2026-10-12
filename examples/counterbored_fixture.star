# Six coaxial hole groups, each machined with a through drill and two counterbores.
# Hardware follows native annular shoulder datums. Threads, fits and preload are
# not simulated: these are prescribed analytic envelopes of the fastening stack.
counterbore_radius = param(name = "counterbore_radius", default = 7)
counterbore_depth = param(name = "counterbore_depth", default = 6)
plate_height = param(name = "plate_height", default = 21)
def check_parameters():
    if counterbore_radius < 6 or counterbore_radius > 8:
        fail("counterbore_radius must be 6..8 mm")
    if counterbore_depth < 4 or counterbore_depth > 8:
        fail("counterbore_depth must be 4..8 mm")
    if plate_height < 18 or plate_height > 24:
        fail("plate_height must be 18..24 mm")
check_parameters()
r = counterbore_radius
h = counterbore_depth
d = plate_height
gap = 30
centers = [[20,20],[60,20],[100,20],[20,60],[60,60],[100,60]]
stock = extrude(id = "stock", profile = rectangle(id = "plate_profile", width = 120, depth = 80), height = d)
pilot_tool = extrude(id = "pilot_tool", profile = circle(id = "pilot_circle", radius = 3.2), height = d+4)
top_tool = extrude(id = "top_tool", profile = circle(id = "top_circle", radius = r), height = h+2)
bottom_tool = extrude(id = "bottom_tool", profile = circle(id = "bottom_circle", radius = 4.5), height = 5)
def drill_pattern(body, prefix, tool, z):
    for i,xy in enumerate(centers):
        cutter = translate(id = prefix+"_tool_"+str(i), body = tool, offset = [xy[0],xy[1],z])
        body = cut(id = prefix+"_cut_"+str(i), left = body, right = cutter)
    return body
piloted = drill_pattern(stock, "pilot", pilot_tool, -2)
top_seated = drill_pattern(piloted, "top", top_tool, d-h)
finished = drill_pattern(top_seated, "bottom", bottom_tool, -2)
output(name = "stock", feature = stock)
output(name = "piloted", feature = piloted)
output(name = "plate", feature = finished)
assert_faces(id = "original_cap", from_feature = stock, role = "end_cap", in_feature = finished, count = 1)
assert_faces(id = "shortened_pilot_walls", from_feature = pilot_tool, role = "wall", in_feature = finished, count = 6)
def shoulder_checks():
    for side in ["top","bottom"]:
        for i in range(6):
            assert_faces(id = side+"_seat_"+str(i), from_feature = side+"_cut_"+str(i), role = "bore_floor", in_feature = finished, count = 1)
shoulder_checks()
plate = part(id = "plate", body = finished, part_number = "CB-PLATE", material = "Aluminum", density_kg_per_m3 = 2700, datums = {
    side+"_seat_"+str(i): face_datum(from_feature = side+"_cut_"+str(i), role = "bore_floor", x_direction = [1,0,0]) for side in ["top","bottom"] for i in range(6)
})
assembly_output(name = "plate_inspection", assembly = assembly(id = "plate_inspection", instances = [instance(id = "inspection_plate", part = plate)]))
def ring(id, outer, inner, length):
    return extrude(id = id, profile = circle(id = id+"_profile", radius = outer, inner_radius = inner), height = length)
washer = part(id = "washer", body = ring("washer_body", r-0.3, 3.1, 1), part_number = "CB-WASHER", material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum(), "end":datum(offset = [0,0,1])})
nut = part(id = "nut", body = ring("nut_body", r-1, 3.1, 4), part_number = "CB-NUT-ENVELOPE", material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum()})
spacer = part(id = "spacer", body = ring("spacer_body", 4.2, 3.2, gap+6), part_number = "CB-SPACER", material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum()})
shaft_length = 2*d+gap-2*h+7
bolt = part(id = "bolt", body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [
    [0,0],[3,0],[3,shaft_length],[r-1,shaft_length],[r-1,shaft_length+3],[0,shaft_length+3],
])), part_number = "CB-BOLT-ENVELOPE", material = "Steel", density_kg_per_m3 = 7800, datums = {"head_seat":datum(offset = [0,0,shaft_length])})
def opposite(i):
    return i+3 if i < 3 else i-3
def hardware():
    result = []
    for i in range(6):
        result.append(instance(id = "top_washer_"+str(i), part = washer, attach_to = "top_plate", datum = "base", target_datum = "top_seat_"+str(i)))
        result.append(instance(id = "bottom_washer_"+str(i), part = washer, attach_to = "bottom_plate", datum = "base", target_datum = "top_seat_"+str(opposite(i))))
        result.append(instance(id = "nut_"+str(i), part = nut, attach_to = "bottom_washer_"+str(i), datum = "base", target_datum = "end"))
        result.append(instance(id = "bolt_"+str(i), part = bolt, attach_to = "top_washer_"+str(i), datum = "head_seat", target_datum = "end"))
        result.append(instance(id = "spacer_"+str(i), part = spacer, attach_to = "bottom_plate", datum = "base", target_datum = "bottom_seat_"+str(opposite(i))))
    return result
assembly_output(name = "counterbored_fixture", assembly = assembly(id = "counterbored_fixture", instances = [
    instance(id = "top_plate", part = plate, offset = [0,0,d+gap]),
    instance(id = "bottom_plate", part = plate, offset = [0,80,d], rotation = [1,0,0,0]),
]+hardware()))
pi = 3.141592653589793
plate_v = 9600*d-6*pi*(3.2*3.2*(d-h-3)+r*r*h+4.5*4.5*3)
hardware_v = 6*pi*(2*((r-0.3)*(r-0.3)-3.1*3.1)+4*((r-1)*(r-1)-3.1*3.1)+(4.2*4.2-3.2*3.2)*(gap+6)+9*shaft_length+3*(r-1)*(r-1))
assert_property(id = "plate_volume", output = "plate", property = "volume_mm3", min = plate_v-0.0001, max = plate_v+0.0001)
assert_property(id = "assembly_volume", output = "counterbored_fixture", property = "volume_mm3", min = 2*plate_v+hardware_v-0.0001, max = 2*plate_v+hardware_v+0.0001)
assert_property(id = "assembly_mass", output = "counterbored_fixture", property = "mass_kg", min = 2*plate_v*2700e-9+hardware_v*7800e-9-0.000001, max = 2*plate_v*2700e-9+hardware_v*7800e-9+0.000001)
