# An octagonal mounting plate with a rectangular slot and triangular opening.
# Real inner face loops survive STEP export; the locator follows the remaining
# top face's area centroid as slot_width changes.
slot_width = param(name = "slot_width", default = 15)
thickness = param(name = "thickness", default = 6)
def check_parameters():
    if slot_width < 10 or slot_width > 20 or thickness < 3 or thickness > 10:
        fail("slot_width must be 10..20 and thickness 3..10 millimetres")
check_parameters()
outline = polygon(id = "plate_profile", points = [
    [0, 10], [10, 0], [70, 0], [80, 10],
    [80, 40], [70, 50], [10, 50], [0, 40],
], holes = [
    [[15, 15], [15, 35], [15 + slot_width, 35], [15 + slot_width, 15]],
    [[50, 15], [50, 35], [65, 25]],
])
plate_body = extrude(id = "plate_body", profile = outline, height = thickness)
assert_faces(id = "top_face", from_feature = plate_body, role = "end_cap", in_feature = plate_body, count = 1)
assert_faces(id = "opening_walls", from_feature = plate_body, role = "hole_walls", in_feature = plate_body, count = 7)
plate = part(id = "plate", body = plate_body, datums = {
    "centroid": face_datum(from_feature = plate_body, role = "end_cap", x_direction = [1, 0, 0]),
})
def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)
base = part(id = "base", body = box("base_body", 80, 50, 3))
foot = part(id = "foot", body = box("foot_body", 5, 5, 12))
locator = part(id = "locator", body = box("locator_body", 4, 4, 8), datums = {
    "foot": datum(offset = [2, 2, 0]),
})
mount = assembly(id = "perforated_mount", instances = [
    instance(id = "base", part = base),
    instance(id = "south_west", part = foot, offset = [5, 5, 3]),
    instance(id = "south_east", part = foot, offset = [70, 5, 3]),
    instance(id = "north_west", part = foot, offset = [5, 40, 3]),
    instance(id = "north_east", part = foot, offset = [70, 40, 3]),
    instance(id = "plate", part = plate, offset = [0, 0, 15]),
    instance(id = "locator", part = locator, attach_to = "plate", datum = "foot", target_datum = "centroid"),
])
output(name = "plate", feature = plate_body)
assembly_output(name = "perforated_mount", assembly = mount)
