# Open tray with a rectangular drain and a separate lid.
# Default volumes: tray 53,616 mm³; lid 21,000 mm³; assembly 74,616 mm³.

def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

width = param(name = "width", default = 100)
depth = param(name = "depth", default = 70)
height = param(name = "height", default = 25)
wall = param(name = "wall", default = 4)

outer = box("outer", width, depth, height)
inner_tool = box("inner/tool", width - 2 * wall, depth - 2 * wall, height)
inner = translate(id = "inner/positioned", body = inner_tool, offset = [wall, wall, wall])
hollow = cut(id = "hollow", left = outer, right = inner)

drain_tool = box("drain/tool", 40, 10, wall + 2)
drain = translate(id = "drain/positioned", body = drain_tool,
                  offset = [(width - 40) / 2, (depth - 10) / 2, -1])
tray = cut(id = "tray", left = hollow, right = drain)

lid_blank = box("lid/blank", width, depth, 3)
lid = translate(id = "lid", body = lid_blank, offset = [0, 0, height + 5])
assembly = union(id = "assembly", left = tray, right = lid)

output(name = "tray", feature = tray)
output(name = "lid", feature = lid)
output(name = "assembly", feature = assembly)
