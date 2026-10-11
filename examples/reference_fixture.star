# A grooved tooling plate with raised stops. Dimensions are millimetres.
# The original plate's top face splits into several patches as grooves are cut.
# Semantic selection follows its descendants; it must not jump to the stop tops.
# cad select examples/reference_fixture.star --from blank --role end_cap --in finished --all

def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

width = param(name = "width", default = 80)
depth = param(name = "depth", default = 40)
thickness = param(name = "thickness", default = 8)
groove_depth = param(name = "groove_depth", default = 4)
groove_width = param(name = "groove_width", default = 8)

def validate_parameters():
    if width < 64 or depth < 20 or thickness <= 0:
        fail("width must be at least 64, depth at least 20, thickness positive")
    if groove_depth <= 0 or groove_depth >= thickness:
        fail("groove_depth must be between zero and thickness")
    if groove_width <= 0 or groove_width >= width / 4:
        fail("groove_width must be positive and less than width / 4")

validate_parameters()

blank = box("blank", width, depth, thickness)
def add_grooves(blank):
    current = blank
    for index in range(3):
        name = "groove_" + str(index)
        tool = box(name + "/tool", groove_width, depth - 8, groove_depth + 1)
        placed = translate(id = name + "/placed", body = tool,
                           offset = [(index + 1) * width / 4 - groove_width / 2,
                                     4, thickness - groove_depth])
        current = cut(id = name, left = current, right = placed)
    return current

current = add_grooves(blank)

stop = box("stop", 8, 4, 12)
left_stop = translate(id = "stop_left", body = stop, offset = [0, 0, thickness])
right_stop = translate(id = "stop_right", body = stop,
                       offset = [width - 8, 0, thickness])
with_left = union(id = "with_left", left = current, right = left_stop)
finished = union(id = "finished", left = with_left, right = right_stop)
# Design intent is checked before any generation is published.
assert_faces(id = "plate_top_survives", from_feature = blank, role = "end_cap",
             in_feature = finished, all = True)
assert_faces(id = "two_stop_tops", from_feature = stop, role = "end_cap",
             in_feature = finished, count = 2)
output(name = "reference_fixture", feature = finished)
