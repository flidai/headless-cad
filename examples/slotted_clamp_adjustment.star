# An exact, source-owned adjustment path for one unthreaded clamp envelope.
# The whole 6 mm stroke is certified against the native plate slot wall.
# The head and thread engagement are nominal geometry only.
slot_length = param(name = "slot_length", default = 6)
slot_radius = param(name = "slot_radius", default = 3)
bolt_radius = param(name = "bolt_radius", default = 2.5)
slide_length = param(name = "slide_length", default = 6)
def validate_adjustment():
    if slot_length < 2 or slot_length > 8:
        fail("slot_length must be 2..8 mm")
    if slot_radius < 2.6 or slot_radius > 4:
        fail("slot_radius must be 2.6..4 mm")
    if bolt_radius < 1.5 or bolt_radius >= slot_radius:
        fail("bolt_radius must be positive and smaller than the slot radius")
    if slide_length <= 0 or slide_length > slot_length:
        fail("slide_length must be within the slot centerline length")
validate_adjustment()

plate_stock = extrude(id = "adjustment_plate_stock", profile = polygon(
    id = "adjustment_plate_profile", points = [[0,0],[50,0],[50,30],[0,30]]), height = 6)
plate_body = slot_pattern(id = "adjustment_plate_body", body = plate_stock,
    frame_origin = [0,0,0], frame_x_direction = [1,0,0], frame_y_direction = [0,1,0],
    slots = [{"id":"travel", "center":[25,15], "direction":[1,0],
              "centerline_length":slot_length, "radius":slot_radius}])
bolt_body = revolve(id = "adjustment_bolt_body", profile = polygon(
    id = "adjustment_bolt_profile", points = [
        [0,0],[bolt_radius,0],[bolt_radius,8],[4.5,8],[4.5,10],[0,10]]))
plate = part(id = "adjustment_plate", body = plate_body, part_number = "SLOT-PLATE", revision = "A")
bolt = part(id = "adjustment_bolt", body = bolt_body, part_number = "SLOT-BOLT-ENVELOPE", revision = "A")
fixture = assembly(id = "slotted_clamp_adjustment", instances = [
    instance(id = "plate", part = plate),
    instance(id = "bolt", part = bolt, offset = [25-slot_length/2,15,-1]),
])
assembly_output(name = "slotted_clamp_adjustment", assembly = fixture)
rigid_translation_group(id = "adjustment", output = "slotted_clamp_adjustment",
    instance_paths = [["bolt"]], reference_instance_path = ["bolt"],
    translation = [slide_length,0,0])
clearance_check(id = "slot_shank_running_gap", output = "slotted_clamp_adjustment",
    left_instance_path = ["plate"], right_instance_path = ["bolt"],
    min_clearance = 0.49, translation_group = "adjustment")
output(name = "adjustment_plate", feature = plate_body)
output(name = "adjustment_bolt", feature = bolt_body)
