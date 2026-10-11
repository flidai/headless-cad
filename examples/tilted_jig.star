# Exact transformed tooling: X rotation stands the triangular profile upright,
# then Z rotations turn two tools in opposite directions on a slotted base.
# These are kernel operations; the exported part coordinates contain the turns.
# Assembly instances only place the already-built parts at their local origins.
length = param(name = "tool_length", default = 30)
rise = param(name = "tool_rise", default = 12)
span = param(name = "tool_span", default = 8)

def validate_parameters():
    if length < 10 or length > 30 or rise < 3 or rise > 20 or span < 3 or span > 8:
        fail("tool_length must be 10..30, tool_rise 3..20, tool_span 3..8 mm")
validate_parameters()

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)

def base_shape():
    body = box("base_blank", 80, 50, 4)
    slot = box("slot", 4, 10, 6)
    for i in range(2):
        cutter = translate(id = "slot_at_" + str(i), body = slot, offset = [4 + i * 68, 20, -1])
        body = cut(id = "base_cut_" + str(i), left = body, right = cutter)
    return body

profile = polygon(id = "tool_profile", points = [[0, 0], [length, 0], [0, rise]])
blank = extrude(id = "tool_blank", profile = profile, height = span)
upright = transform(id = "tool_upright", body = blank,
                    rotation = [0.7071067811865476, 0, 0, 0.7071067811865476])
left_body = transform(id = "left_tool", body = upright, offset = [10, 20, 4],
                      rotation = [0, 0, 0.25881904510252074, 0.9659258262890683])
right_body = transform(id = "right_tool", body = upright, offset = [40, 30, 4],
                       rotation = [0, 0, -0.25881904510252074, 0.9659258262890683])
assert_faces(id = "left_cap", from_feature = blank, role = "end_cap", in_feature = left_body)
assert_faces(id = "right_walls", from_feature = blank, role = "walls", in_feature = right_body, count = 3)
base = part(id = "base", body = base_shape(), part_number = "JIG-BASE")
left = part(id = "left_tool", body = left_body, part_number = "JIG-LEFT")
right = part(id = "right_tool", body = right_body, part_number = "JIG-RIGHT")
assembly_output(name = "tilted_jig", assembly = assembly(id = "tilted_jig", instances = [
    instance(id = "base", part = base), instance(id = "left", part = left), instance(id = "right", part = right),
]))
output(name = "left_tool", feature = left_body)
output(name = "right_tool", feature = right_body)
