# Between-centers inspection fixture: two shared center pins seat in native
# conical blind bores. These placements demonstrate design intent; they are
# not a contact, clearance, or collision certification.
work_length = param(name = "work_length", default = 80)
tip_depth = param(name = "tip_depth", default = 12)
bore_clearance = param(name = "bore_clearance", default = 0.2)
def validate_parameters():
    if work_length < 40 or work_length > 120:
        fail("work_length must be 40..120 mm")
    if tip_depth < 6 or tip_depth > 16:
        fail("tip_depth must be 6..16 mm")
    if bore_clearance < 0.05 or bore_clearance > 1:
        fail("bore_clearance must be 0.05..1 mm")
validate_parameters()

workpiece_body = revolve(id = "workpiece_body", profile = polygon(id = "workpiece_profile", points = [[0,tip_depth],[6,0],[16,0],[16,work_length],[6,work_length],[0,work_length-tip_depth]]))
pin_body = revolve(id = "pin_body", profile = polygon(id = "pin_profile", points = [[0,0],[8,0],[8,20],[6,20],[0,20+tip_depth]]))
base_body = extrude(id = "base_body", profile = polygon(id = "base_profile", points = [[-30,-30],[work_length+30,-30],[work_length+30,30],[-30,30]]), height = 8)
holder_body = extrude(id = "holder_body", profile = polygon(id = "holder_profile", points = [[-10,-15],[32,-15],[32,15],[-10,15]], circular_holes = [{"center":[0,0],"radius":8+bore_clearance}]), height = 20)
assert_faces(id = "pin_cone", from_feature = pin_body, role = "conical_faces", in_feature = pin_body, count = 1)
assert_faces(id = "workpiece_centers", from_feature = workpiece_body, role = "inner_walls", in_feature = workpiece_body, count = 2)

axis_x = [0,0.7071067811865475,0,0.7071067811865476]
base = part(id = "base", body = base_body, part_number = "CF-BASE", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"left_holder": datum(offset = [-20,0,40], rotation = axis_x), "right_holder": datum(offset = [work_length,0,40], rotation = axis_x), "workpiece": datum(offset = [0,0,40], rotation = axis_x)})
holder = part(id = "holder", body = holder_body, part_number = "CF-HOLDER", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"bore_start": datum(), "bore_end": datum(offset = [0,0,20])})
pin = part(id = "center_pin", body = pin_body, part_number = "CF-CENTER", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"base": datum()})
workpiece = part(id = "workpiece", body = workpiece_body, part_number = "CF-WORKPIECE", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"origin": datum()})
assembly_output(name = "centering_fixture", assembly = assembly(id = "centering_fixture", instances = [
    instance(id = "base", part = base),
    instance(id = "left_holder", part = holder, attach_to = "base", datum = "bore_start", target_datum = "left_holder"),
    instance(id = "right_holder", part = holder, attach_to = "base", datum = "bore_start", target_datum = "right_holder"),
    instance(id = "left_center", part = pin, attach_to = "left_holder", datum = "base", target_datum = "bore_start"),
    instance(id = "right_center", part = pin, attach_to = "right_holder", datum = "base", target_datum = "bore_end", rotation = [0,1,0,0]),
    instance(id = "workpiece", part = workpiece, attach_to = "base", datum = "origin", target_datum = "workpiece"),
]))
output(name = "center_pin", feature = pin_body)
output(name = "workpiece", feature = workpiece_body)

# Deeper center seats remove exactly the volume added to the two pins.
pi = 3.141592653589793
expected_volume = 480*work_length + 79200 + pi*(2560 + 256*work_length - 40*(8+bore_clearance)*(8+bore_clearance))
assert_property(id = "material_envelope", output = "centering_fixture", property = "volume_mm3", min = expected_volume-0.0001, max = expected_volume+0.0001)
assert_property(id = "mass_budget", output = "centering_fixture", property = "mass_kg", max = 1)
assert_property(id = "balance", output = "centering_fixture", property = "mass_centroid_x_mm", min = work_length/2-0.000001, max = work_length/2+0.000001)
