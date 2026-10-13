# Minimal native planar seating example. The witness is a positive material
# disk in both projected face trims with an arithmetic support-plane gap bound.
flange_body = revolve(id = "flange_body", profile = polygon(id = "flange_profile",
    points = [[2,0],[6,0],[6,2],[2,2]]))
plate_body = extrude(id = "plate_body", profile = polygon(id = "plate_profile",
    points = [[-10,-10],[10,-10],[10,10],[-10,10]],
    circular_holes = [{"center":[0,0],"radius":3}]), height = 2)
flange = part(id = "flange", body = flange_body, datums = {
    "outside":face_datum(from_feature = flange_body, role = "segment_0", x_direction = [1,0,0]),
})
plate = part(id = "plate", body = plate_body, datums = {
    "inside":face_datum(from_feature = plate_body, role = "end_cap", x_direction = [1,0,0]),
})
pair = assembly(id = "pair", instances = [
    instance(id = "flange", part = flange),
    instance(id = "plate", part = plate, offset = [0,0,-2]),
])
assembly_output(name = "seated_pair", assembly = pair)
output(name = "plate_body", feature = plate_body)
nominal_seating_check(id = "flange_plate_seat", output = "seated_pair",
    left_instance_path = ["flange"], left_datum = "outside",
    right_instance_path = ["plate"], right_datum = "inside",
    min_shared_disk_radius_mm = 0.2)
