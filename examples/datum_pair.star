# Minimal rigid attachment: the upper block's center datum follows a turned seat.
def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile",
                   width = width, depth = depth), height = height)
base = part(id = "base", body = box("base_body", 10, 10, 2), datums = {
    "seat": datum(offset = [5, 5, 2], rotation = [0, 0, 0.7071067811865476, 0.7071067811865476]),
})
block = part(id = "block", body = box("block_body", 4, 2, 3), datums = {
    "mount": datum(offset = [2, 1, 0]),
})
# Forward references within this assembly are resolved independently of list order.
upper = instance(id = "upper", part = block, attach_to = "base", datum = "mount", target_datum = "seat")
base_instance = instance(id = "base", part = base)
pair = assembly(id = "pair", instances = [upper, base_instance])
assembly_output(name = "datum_pair", assembly = pair)
