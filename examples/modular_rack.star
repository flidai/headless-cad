# Three levels of reusable assemblies: rack -> row -> carriage -> parts.
# Four carriages share one definition; eight guides share one exact solid.
# The rear row turns 90 degrees; each carriage's right guide turns 180 degrees.
# All dimensions are millimetres. This is placement, not a kinematic solver.

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile",
                   width = width, depth = depth), height = height)

spacing = param(name = "spacing", default = 48)
rear_x = param(name = "rear_x", default = 100)
rear_y = param(name = "rear_y", default = 32)

def validate_parameters():
    if spacing < 44 or rear_x < 96 or rear_y < 28:
        fail("spacing >= 44, rear_x >= 96, and rear_y >= 28 are required")

validate_parameters()
plate_blank = box("carriage_blank", 40, 24, 4)
slot_tool = box("slot_tool", 8, 12, 6)
slot = translate(id = "slot_placed", body = slot_tool, offset = [16, 6, -1])
plate_body = cut(id = "carriage_plate", left = plate_blank, right = slot)
guide_body = box("guide", 4, 24, 10)
slider_body = box("slider", 12, 12, 6)
# These illustrative material inputs demonstrate mass accounting; substitute
# the density and specification for the material used in a manufactured part.
plate = part(id = "plate", body = plate_body, part_number = "RACK-PLATE", revision = "A",
             description = "Slotted carriage plate", material = "Demo alloy", density_kg_per_m3 = 2700)
guide = part(id = "guide", body = guide_body, part_number = "RACK-GUIDE", revision = "A",
             description = "Reversible guide", material = "Demo steel", density_kg_per_m3 = 7800)
slider = part(id = "slider", body = slider_body, part_number = "RACK-SLIDER", revision = "A",
              description = "Sliding block", material = "Demo polymer", density_kg_per_m3 = 1200)

carriage = assembly(id = "carriage", instances = [
    instance(id = "plate", part = plate),
    instance(id = "guide_left", part = guide, offset = [0, 0, 4]),
    instance(id = "guide_right", part = guide, offset = [40, 24, 4],
             rotation = [0, 0, 1, 0]),
    instance(id = "slider", part = slider, offset = [14, 6, 4]),
])
row = assembly(id = "row", instances = [
    assembly_instance(id = "carriage_a", assembly = carriage),
    assembly_instance(id = "carriage_b", assembly = carriage, offset = [spacing, 0, 0]),
])
rack_width = max(spacing + 44, rear_x + 4)
rack_depth = rear_y + spacing + 44
rack_body = box("rack_base", rack_width, rack_depth, 3)
rack_base = part(id = "rack_base", body = rack_body, part_number = "RACK-BASE", revision = "A",
                 material = "Demo alloy", density_kg_per_m3 = 2700)
rack = assembly(id = "rack", instances = [
    instance(id = "base", part = rack_base),
    assembly_instance(id = "front", assembly = row, offset = [0, 0, 3]),
    assembly_instance(id = "rear", assembly = row, offset = [rear_x, rear_y, 3],
                      rotation = [0, 0, 0.7071067811865476, 0.7071067811865476]),
])
assert_faces(id = "guide_top", from_feature = guide_body, role = "end_cap",
             in_feature = guide_body)
assembly_output(name = "modular_rack", assembly = rack)
