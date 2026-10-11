# A U-shaped tooling plate with arm openings and a recessed central bay.
# The centroid of the cap may lie in empty space: an explicit local datum
# offset locates the small registration block on the solid front web.
notch_width = param(name = "notch_width", default = 30)
notch_depth = param(name = "notch_depth", default = 35)
thickness = param(name = "thickness", default = 6)
def check_parameters():
    if notch_width < 20 or notch_width > 40 or notch_depth < 25 or notch_depth > 40:
        fail("notch_width must be 20..40 and notch_depth 25..40 millimetres")
    if thickness < 3 or thickness > 10:
        fail("thickness must be 3..10 millimetres")
check_parameters()
left = 40 - notch_width / 2
right = 40 + notch_width / 2
web = 60 - notch_depth
outline = [[0, 0], [80, 0], [80, 60], [right, 60],
           [right, web], [left, web], [left, 60], [0, 60]]
blank_profile = polygon(id = "blank_profile", points = outline)
blank_body = extrude(id = "blank_body", profile = blank_profile, height = thickness)
profile = polygon(id = "fork_profile", points = outline, holes = [
    [[8, 35], [8, 48], [17, 48], [17, 35]],
    [[63, 35], [63, 48], [72, 48], [72, 35]],
])
fork_body = extrude(id = "fork_body", profile = profile, height = thickness)
assert_faces(id = "top_face", from_feature = fork_body, role = "end_cap", in_feature = fork_body)
assert_faces(id = "opening_walls", from_feature = fork_body, role = "hole_walls", in_feature = fork_body, count = 8)
fork = part(id = "fork", body = fork_body, datums = {
    "register": face_datum(from_feature = fork_body, role = "end_cap", x_direction = [1, 0, 0], offset = [0, -12, 0]),
})
def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)
base = part(id = "base", body = box("base_body", 90, 70, 3))
foot = part(id = "foot", body = box("foot_body", 5, 5, 12))
locator = part(id = "locator", body = box("locator_body", 4, 4, 8), datums = {
    "foot": datum(offset = [2, 2, 0]),
})
mount = assembly(id = "fork_mount", instances = [
    instance(id = "base", part = base, offset = [-5, -5, 0]),
    instance(id = "south_west", part = foot, offset = [3, 3, 3]),
    instance(id = "south_east", part = foot, offset = [72, 3, 3]),
    instance(id = "north_west", part = foot, offset = [3, 52, 3]),
    instance(id = "north_east", part = foot, offset = [72, 52, 3]),
    instance(id = "fork", part = fork, offset = [0, 0, 15]),
    instance(id = "locator", part = locator, attach_to = "fork", datum = "foot", target_datum = "register"),
])
output(name = "blank", feature = blank_body)
output(name = "fork", feature = fork_body)
assembly_output(name = "fork_mount", assembly = mount)
