# Angled fixture with exact triangular-prism ramps and hexagonal seating blocks.
# Polygons are genuine planar profiles, not faceted substitutes for circles.
# Rotations place reusable prism definitions; general polygon Booleans remain
# unsupported, so only the orthogonal base uses cut operations.

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile",
                   width = width, depth = depth), height = height)

length = param(name = "ramp_length", default = 40)
rise = param(name = "ramp_rise", default = 20)
span = param(name = "span", default = 30)

def validate_parameters():
    if length < 30 or rise <= 3 or span < 20:
        fail("ramp_length >= 30, ramp_rise > 3, and span >= 20 are required")

validate_parameters()
width = 2 * length + 20
depth = span + 30
positions = [[15, 8], [width - 15, 8], [15, depth - 8], [width - 15, depth - 8]]

def make_base():
    result = box("base_blank", width, depth, 4)
    tool = box("mounting_slot", 6, 12, 6)
    for index in range(4):
        center = positions[index]
        placed = translate(id = "slot_" + str(index), body = tool,
                           offset = [center[0] - 3, center[1] - 6, -1])
        result = cut(id = "base_cut_" + str(index), left = result, right = placed)
    return result

base_body = make_base()
ramp_profile = polygon(id = "ramp_profile", points = [[0, 0], [length, 0], [0, rise]])
ramp_body = extrude(id = "ramp", profile = ramp_profile, height = span)
hex_profile = polygon(id = "hex_profile", points = [
    [4, 0], [2, 3.4641016151377544], [-2, 3.4641016151377544],
    [-4, 0], [-2, -3.4641016151377544], [2, -3.4641016151377544],
])
head_body = extrude(id = "hex_head", profile = hex_profile, height = 3)
base = part(id = "base", body = base_body, part_number = "ANGLE-BASE",
            material = "Demo alloy", density_kg_per_m3 = 2700)
ramp = part(id = "ramp", body = ramp_body, part_number = "ANGLE-RAMP",
            material = "Demo alloy", density_kg_per_m3 = 2700)
head = part(id = "hex_head", body = head_body, part_number = "ANGLE-HEAD",
            material = "Demo steel", density_kg_per_m3 = 7800)

def components():
    result = [instance(id = "base", part = base),
        instance(id = "left_ramp", part = ramp, offset = [5, span + 15, 4],
                 rotation = [0.7071067811865476, 0, 0, 0.7071067811865476]),
        instance(id = "right_ramp", part = ramp, offset = [width - 5, 15, 4],
                 rotation = [0, 0.7071067811865476, 0.7071067811865476, 0])]
    for index in range(4):
        center = positions[index]
        result.append(instance(id = "head_" + str(index), part = head,
                               offset = [center[0], center[1], 4]))
    return result

assert_faces(id = "ramp_topology", from_feature = ramp_body, role = "walls",
             in_feature = ramp_body, count = 3)
fixture = assembly(id = "angled_fixture", instances = components())
assembly_output(name = "angled_fixture", assembly = fixture)
output(name = "ramp_definition", feature = ramp_body)
