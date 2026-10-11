# Convex intersection keeps oblique planes as exact planar faces.
# First tool: crop a rotated square into an octagonal prism.
# Second tool: clip a box with a tilted solid to form two sloping shoulders.
# The cutters are modeling operands, not tessellated display approximations.
root_two = 1.4142135623730951
crop_height = param(name = "crop_height", default = 6)
shoulder_height = param(name = "shoulder_height", default = 15)

def validate_parameters():
    if crop_height < 2 or crop_height > 6:
        fail("crop_height must be 2..6 mm")
    if shoulder_height < 11 or shoulder_height > 18:
        fail("shoulder_height must be 11..18 mm")
validate_parameters()

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)

square = box("square_blank", 10 * root_two, 10 * root_two, 8)
diamond = transform(id = "diamond", body = square, offset = [10, 0, 0],
                    rotation = [0, 0, 0.3826834323650898, 0.9238795325112867])
crop = translate(id = "crop", body = box("crop_blank", 12, 14, crop_height), offset = [4, 3, 1])
octagonal = intersect(id = "octagonal", left = diamond, right = crop)

large = box("shoulder_operand", 20 * root_two, 20, 20 * root_two)
tilted = transform(id = "shoulder_tilt", body = large, offset = [-10, -5, 0],
                   rotation = [0, 0.3826834323650898, 0, 0.9238795325112867])
shoulder_crop = box("shoulder_crop", 20, 10, shoulder_height)
shouldered = intersect(id = "shouldered", left = tilted, right = shoulder_crop)

assert_faces(id = "octagonal_top", from_feature = "crop_blank", role = "end_cap", in_feature = octagonal)
assert_faces(id = "oblique_shoulder", from_feature = large, role = "end_cap", in_feature = shouldered)
assert_faces(id = "shoulder_top", from_feature = shoulder_crop, role = "end_cap", in_feature = shouldered)

base = part(id = "base", body = box("base", 50, 35, 3), part_number = "CLIP-BASE")
oct_part = part(id = "octagonal", body = octagonal, part_number = "CLIP-OCTAGON")
shoulder_part = part(id = "shouldered", body = shouldered, part_number = "CLIP-SHOULDER")
assembly_output(name = "clipped_tools", assembly = assembly(id = "clipped_tools", instances = [
    instance(id = "base", part = base),
    instance(id = "octagonal", part = oct_part, offset = [0, 5, 2]),
    instance(id = "shouldered", part = shoulder_part, offset = [25, 12, 3]),
]))
output(name = "octagonal", feature = octagonal)
output(name = "shouldered", feature = shouldered)
