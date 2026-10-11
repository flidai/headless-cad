# A slotted mounting bracket. All dimensions are millimetres.
# cad build examples/bracket.star --param width=100 --out-dir out/bracket

def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

width = param(name = "width", default = 80)
depth = param(name = "depth", default = 40)
thickness = param(name = "thickness", default = 6)
wall_height = param(name = "wall_height", default = 40)
slot_width = param(name = "slot_width", default = 20)

base = box("base", width, depth, thickness)
wall = box("wall", thickness, depth, wall_height)
blank = union(id = "bracket", left = base, right = wall)

slot_tool = box("slot/tool", slot_width, 10, thickness + 2)
slot = translate(id = "slot/positioned", body = slot_tool,
                 offset = [(width - slot_width) / 2, (depth - 10) / 2, -1])
slotted = cut(id = "base_slot", left = blank, right = slot)

window_tool = box("window/tool", thickness + 2, 16, 16)
window = translate(id = "window/positioned", body = window_tool,
                   offset = [-1, (depth - 16) / 2, 14])
finished = cut(id = "wall_window", left = slotted, right = window)

output(name = "bracket", feature = finished)
