# A source-controlled export contract. Change format to "stl" to see unit loss
# block publication while retaining the previous successful output directory.
blank = extrude(id = "blank", profile = rectangle(id = "outline", width = 40, depth = 25), height = 8)
slot_tool = extrude(id = "slot_tool", profile = rectangle(id = "slot_profile", width = 20, depth = 5), height = 10)
slot = translate(id = "slot", body = slot_tool, offset = [10, 10, -1])
plate = cut(id = "plate", left = blank, right = slot)
output(name = "plate", feature = plate)
require_exchange(id = "plate_units", output = "plate", format = "step", aspect = "units")
require_exchange(id = "plate_geometry", output = "plate", format = "step", aspect = "geometry")
