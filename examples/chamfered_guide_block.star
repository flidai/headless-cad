# Six native planar chamfers form a guide block with two inclined wear-pad seats.
# Top bevels follow original stock faces through four preceding topology edits.
# Only bounded convex edge chamfers are supported; no fit or loading is certified.
corner = param(name = "corner", default = 4)
bevel = param(name = "bevel", default = 1.5)
height = param(name = "height", default = 20)
def validate_parameters():
    if corner < 3 or corner > 5:
        fail("corner must be 3..5 mm")
    if bevel < 0.75 or bevel > 2.25:
        fail("bevel must be 0.75..2.25 mm")
    if height < 16 or height > 24:
        fail("height must be 16..24 mm")
validate_parameters()
stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 60, depth = 40), height = height)
front_left = chamfer(id = "front_left", body = stock, face_normals = [[-1,0,0],[0,-1,0]], distance = corner)
front_right = chamfer(id = "front_right", body = front_left, face_normals = [[1,0,0],[0,-1,0]], distance = corner)
back_right = chamfer(id = "back_right", body = front_right, face_normals = [[1,0,0],[0,1,0]], distance = corner)
back_left = chamfer(id = "back_left", body = back_right, face_normals = [[-1,0,0],[0,1,0]], distance = corner)
front_bevel = chamfer_faces(id = "front_bevel", body = back_left, faces = [{"from_feature":stock,"role":"wall_y_min"},{"from_feature":stock,"role":"end_cap"}], distance = bevel)
finished = chamfer_faces(id = "back_bevel", body = front_bevel, faces = [{"from_feature":stock,"role":"wall_y_max"},{"from_feature":stock,"role":"end_cap"}], distance = bevel)
output(name = "stock", feature = stock)
output(name = "block", feature = finished)
assert_faces(id = "front_bevel_survives", from_feature = front_bevel, role = "chamfer", in_feature = finished, count = 1)
assert_faces(id = "front_corner_survives", from_feature = front_left, role = "chamfer", in_feature = finished, count = 1)
expected_volume = (2400-2*corner*corner)*height - (60-2*corner)*bevel*bevel - 2*bevel*bevel*bevel/3
assert_property(id = "block_volume", output = "block", property = "volume_mm3", min = expected_volume-0.00001, max = expected_volume+0.00001)
mid_area = 2400-2*corner*corner
bevel_area = mid_area-(60-2*corner)*bevel-bevel*bevel/2
section_view(id = "mid", output = "block", origin = [0,0,height/2], min_area = mid_area-0.000001, max_area = mid_area+0.000001)
section_view(id = "bevel", output = "block", origin = [0,0,height-bevel/2], min_area = bevel_area-0.000001, max_area = bevel_area+0.000001)
block = part(id = "block", body = finished, part_number = "GUIDE-BLOCK", revision = "A", material = "Aluminum", density_kg_per_m3 = 2700, datums = {"wear_seat":face_datum(from_feature = front_bevel, role = "chamfer", x_direction = [1,0,0])})
pad = part(id = "pad", body = extrude(id = "pad_body", profile = rectangle(id = "pad_outline", width = 40, depth = 0.5), height = 2), part_number = "GUIDE-PAD", revision = "A", material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum(offset = [20,0.25,0])})
module = assembly(id = "module", instances = [instance(id = "block", part = block), instance(id = "pad", part = pad, attach_to = "block", datum = "base", target_datum = "wear_seat")])
assembly_output(name = "chamfered_guide_block", assembly = assembly(id = "guide", instances = [assembly_instance(id = "left", assembly = module), assembly_instance(id = "right", assembly = module, offset = [90,0,0])]))
