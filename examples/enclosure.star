# Electronics enclosure: open top, four vents on each long side, four internal
# supports attached to the floor, and a separate lid in an exploded position.
# cad build examples/enclosure.star --out-dir out/enclosure
# Default body: shell 68,160 - vents 2,048 + supports 1,728 = 67,840 mm³.
# Lid 20,400 mm³; complete assembly 88,240 mm³. No enclosed cavity operation.

def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

def placed_box(id, width, depth, height, position):
    blank = box(id + "/blank", width, depth, height)
    return translate(id = id, body = blank, offset = position)

def vented_body(width, depth, height, wall):
    outer = box("outer", width, depth, height)
    opening = placed_box("opening", width - 2 * wall, depth - 2 * wall,
                         height, [wall, wall, wall])
    result = cut(id = "open_shell", left = outer, right = opening)
    for row in range(2):
        y = -1 if row == 0 else depth - wall - 1
        for i in range(4):
            label = "vent_" + str(row + 1) + "_" + str(i + 1)
            cutter = placed_box(label + "/tool", 8, wall + 2, 8,
                                 [width * (i + 1) / 5 - 4, y, 16])
            result = cut(id = label + "/cut", left = result, right = cutter)
    # Supports meet the floor over full faces, so they are part of the body.
    for row in range(2):
        y = wall + 4 if row == 0 else depth - wall - 10
        for column in range(2):
            x = wall + 4 if column == 0 else width - wall - 10
            label = "support_" + str(row + 1) + "_" + str(column + 1)
            support = placed_box(label, 6, 6, 12, [x, y, wall])
            result = union(id = label + "/attached", left = result, right = support)
    return result

width = param(name = "width", default = 100)
depth = param(name = "depth", default = 68)
height = param(name = "height", default = 36)
wall = param(name = "wall", default = 4)
lid_gap = param(name = "lid_gap", default = 8)

body = vented_body(width, depth, height, wall)
lid = box("lid_shape", width, depth, 3)
body_definition = part(id = "body_definition", body = body)
lid_definition = part(id = "lid_definition", body = lid)
body_instance = instance(id = "body", part = body_definition)
lid_instance = instance(id = "lid", part = lid_definition, offset = [0, 0, height + lid_gap])
complete = assembly(id = "enclosure", instances = [body_instance, lid_instance])
placed_lid = assembly(id = "placed_lid", instances = [lid_instance])

assembly_output(name = "enclosure", assembly = complete)
output(name = "enclosure_body", feature = body)
assembly_output(name = "enclosure_lid", assembly = placed_lid)
