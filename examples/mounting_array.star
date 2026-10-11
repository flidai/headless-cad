# One slotted mounting block, reused as a configurable array of rigid instances.
# Alternate rows rotate the same definition 90 degrees around Z.
# Default: 24 occurrences * (10*6*4 - 4*2*4) = 4,992 mm³.
# cad build examples/mounting_array.star --out-dir out/mounting_array

def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

def pattern(definition, rows, columns, pitch_x, pitch_y):
    if rows != int(rows) or columns != int(columns) or rows < 1 or columns < 1:
        fail("rows and columns must be positive integers")
    instances = []
    for row in range(int(rows)):
        rotation = [0, 0, 0, 1] if row % 2 == 0 else [0, 0, 0.7071067811865476, 0.7071067811865476]
        for column in range(int(columns)):
            id = "mount_" + str(row) + "_" + str(column)
            instances.append(instance(id = id, part = definition,
                                      offset = [column * pitch_x, row * pitch_y, 0],
                                      rotation = rotation))
    return assembly(id = "mounting_array", instances = instances)

rows = param(name = "rows", default = 4)
columns = param(name = "columns", default = 6)
pitch_x = param(name = "pitch_x", default = 20)
pitch_y = param(name = "pitch_y", default = 14)
blank = box("blank", 10, 6, 4)
tool = box("slot_tool", 4, 2, 6)
placed_tool = translate(id = "slot_position", body = tool, offset = [3, 2, -1])
mount = cut(id = "slotted_mount", left = blank, right = placed_tool)
definition = part(id = "mount_definition", body = mount)
mounts = pattern(definition, rows, columns, pitch_x, pitch_y)
assembly_output(name = "mounting_array", assembly = mounts)
