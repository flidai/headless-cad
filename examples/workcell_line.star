# A spaced line of tooling stations: common frames, distinct triangular tools.
# Zooming into one station should load its four mesh definitions, not every tool.
# Datums describe rigid placement; no collision or mechanism solver is implied.
count_value = param(name = "stations", default = 12)
columns_value = param(name = "columns", default = 4)
variants_value = param(name = "variants", default = 12)
count = int(count_value)
columns = int(columns_value)
variants = int(variants_value)
def check_parameters():
    if count != count_value or columns != columns_value or variants != variants_value:
        fail("stations, columns, and variants must be integers")
    if count < 1 or count > 1000 or columns < 1 or columns > count:
        fail("stations must be 1..1000 and columns must be 1..stations")
    if variants < 1 or variants > 40 or variants > count:
        fail("variants must be 1..40 and no greater than stations")
check_parameters()

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)
base = part(id = "base", body = box("base_body", 100, 60, 4), part_number = "CELL-BASE", datums = {
    "left": datum(offset = [12, 20, 4]), "right": datum(offset = [80, 20, 4]),
})
post = part(id = "post", body = box("post_body", 8, 10, 32), part_number = "CELL-POST", datums = {
    "foot": datum(), "top": datum(offset = [0, 0, 32]),
})
beam = part(id = "beam", body = box("beam_body", 76, 10, 6), part_number = "CELL-BEAM", datums = {
    "foot": datum(), "tool": datum(offset = [38, 10, 3]),
})

def make_station(index):
    prefix = "variant_" + str(index)
    width = 12 + index
    rise = 8 + index / 2
    profile = polygon(id = prefix + "/tool_profile", points = [[0, 0], [width, 0], [0, rise]])
    tool = part(id = prefix + "/tool", body = extrude(id = prefix + "/tool_body", profile = profile, height = 6),
                part_number = "CELL-TOOL-" + str(index), datums = {
        "mount": datum(offset = [width / 2, 0, 0], rotation = [0.7071067811865476, 0, 0, 0.7071067811865476]),
    })
    root_id = prefix + "/base"
    left_id = prefix + "/left"
    beam_id = prefix + "/beam"
    return assembly(id = prefix, instances = [
        instance(id = root_id, part = base),
        instance(id = left_id, part = post, attach_to = root_id, datum = "foot", target_datum = "left"),
        instance(id = prefix + "/right", part = post, attach_to = root_id, datum = "foot", target_datum = "right"),
        instance(id = beam_id, part = beam, attach_to = left_id, datum = "foot", target_datum = "top"),
        instance(id = prefix + "/tool_instance", part = tool, attach_to = beam_id, datum = "mount", target_datum = "tool", offset = [0, 2, 0]),
    ])
def station_definitions():
    result = []
    for i in range(variants):
        result.append(make_station(i))
    return result
station_types = station_definitions()
def cells():
    result = []
    for i in range(count):
        result.append(assembly_instance(id = "station_" + str(i), assembly = station_types[i % variants],
            offset = [200 * (i % columns), 150 * (i // columns), 0]))
    return result
line = assembly(id = "workcell_line", instances = cells())
assembly_output(name = "workcell_line", assembly = line)
