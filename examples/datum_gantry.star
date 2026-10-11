# A source-driven inspection gantry with reusable posts, named mounting frames,
# a hanging triangular tool, and nested repetition. Datum attachment is rigid
# placement, not a multi-body constraint or clearance solver.
def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile",
                   width = width, depth = depth), height = height)

span = param(name = "span", default = 80)
post_height = param(name = "post_height", default = 45)
def validate_parameters():
    if span < 60 or post_height < 25:
        fail("span >= 60 and post_height >= 25 are required")
validate_parameters()
width = span + 40

def base_geometry():
    body = box("base_blank", width, 80, 6)
    slot = box("slot_tool", 6, 12, 8)
    for i in range(4):
        x = 10 if i % 2 == 0 else width - 10
        y = 10 if i < 2 else 70
        tool = translate(id = "slot_" + str(i), body = slot, offset = [x - 3, y - 6, -1])
        body = cut(id = "base_cut_" + str(i), left = body, right = tool)
    return body

base = part(id = "base", body = base_geometry(), part_number = "GANTRY-BASE",
    material = "Demo alloy", density_kg_per_m3 = 2700, datums = {
        "left_post": datum(offset = [15, 25, 6]),
        "right_post": datum(offset = [15 + span, 25, 6]),
    })
post = part(id = "post", body = box("post_body", 10, 14, post_height), part_number = "GANTRY-POST",
    material = "Demo alloy", density_kg_per_m3 = 2700, datums = {
        "foot": datum(), "top": datum(offset = [0, 0, post_height]),
    })
bridge = part(id = "bridge", body = box("bridge_body", span + 10, 14, 8), part_number = "GANTRY-BRIDGE",
    material = "Demo alloy", density_kg_per_m3 = 2700, datums = {
        "left_mount": datum(),
        "tool": datum(offset = [(span + 10) / 2, 14, 4]),
        "left_clamp": datum(offset = [20, 7, 8]),
        "right_clamp": datum(offset = [span - 10, 7, 8]),
    })
profile = polygon(id = "tool_profile", points = [[0, 0], [16, 0], [0, 12]])
tool = part(id = "tool", body = extrude(id = "tool_body", profile = profile, height = 8),
    part_number = "GANTRY-TOOL", material = "Demo steel", density_kg_per_m3 = 7800, datums = {
        "mount": datum(offset = [8, 0, 0], rotation = [0.7071067811865476, 0, 0, 0.7071067811865476]),
    })
clamp = part(id = "clamp", body = box("clamp_body", 14, 10, 4), part_number = "GANTRY-CLAMP",
    material = "Demo steel", density_kg_per_m3 = 7800, datums = {"foot": datum(offset = [7, 5, 0])})
base_i = instance(id = "base", part = base)
left = instance(id = "left_post", part = post, attach_to = "base", datum = "foot", target_datum = "left_post")
right = instance(id = "right_post", part = post, attach_to = "base", datum = "foot", target_datum = "right_post")
beam = instance(id = "bridge", part = bridge, attach_to = "left_post", datum = "left_mount", target_datum = "top")
head = instance(id = "tool", part = tool, attach_to = "bridge", datum = "mount", target_datum = "tool", offset = [0, 3, 0])
left_clamp = instance(id = "left_clamp", part = clamp, attach_to = "bridge", datum = "foot", target_datum = "left_clamp")
right_clamp = instance(id = "right_clamp", part = clamp, attach_to = "bridge", datum = "foot", target_datum = "right_clamp")
gantry = assembly(id = "gantry", instances = [head, left_clamp, right_clamp, beam, right, left, base_i])
assembly_output(name = "gantry", assembly = gantry)
line = assembly(id = "inspection_line", instances = [
    assembly_instance(id = "station_a", assembly = gantry),
    assembly_instance(id = "station_b", assembly = gantry, offset = [0, 100, 0]),
])
assembly_output(name = "inspection_line", assembly = line)
