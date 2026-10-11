# Modular machining fixture: four through-slots, an attached locating fence,
# and two instances of one jaw definition, shown 2 mm above the plate.
# cad build examples/fixture.star --out-dir out/fixture
# Default material: plate/fence 82,944 mm³ + jaws 6,912 mm³ = 89,856 mm³.

def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

def placed_box(id, width, depth, height, position):
    blank = box(id + "/blank", width, depth, height)
    return translate(id = id, body = blank, offset = position)

def slotted_plate(width, depth, thickness, fence_height):
    plate = box("plate", width, depth, thickness)
    fence = box("fence", width, 8, fence_height)
    result = union(id = "plate_and_fence", left = plate, right = fence)
    # Shared helper plus a loop make the slot pattern a recipe, not copied solids.
    for i in range(4):
        label = "slot_" + str(i + 1)
        cutter = placed_box(label + "/tool", 8, depth - 44, thickness + 2,
                             [width * (i + 1) / 5 - 4, 22, -1])
        result = cut(id = label + "/cut", left = result, right = cutter)
    return result

width = param(name = "width", default = 120)
depth = param(name = "depth", default = 80)
thickness = param(name = "thickness", default = 8)
fence_height = param(name = "fence_height", default = 24)
jaw_clearance = param(name = "jaw_clearance", default = 2)

plate = slotted_plate(width, depth, thickness, fence_height)
jaw_shape = box("jaw_shape", 18, 12, 16)
plate_definition = part(id = "plate_definition", body = plate)
jaw_definition = part(id = "jaw_definition", body = jaw_shape)
base_instance = instance(id = "base", part = plate_definition)
left_jaw = instance(id = "jaw_left", part = jaw_definition,
                    offset = [10, depth - 20, thickness + jaw_clearance])
right_jaw = instance(id = "jaw_right", part = jaw_definition,
                     offset = [width - 28, depth - 20, thickness + jaw_clearance])
jaws = assembly(id = "jaws", instances = [left_jaw, right_jaw])
fixture = assembly(id = "fixture", instances = [base_instance, left_jaw, right_jaw])

assembly_output(name = "fixture", assembly = fixture)
output(name = "fixture_plate", feature = plate)
assembly_output(name = "fixture_jaws", assembly = jaws)
