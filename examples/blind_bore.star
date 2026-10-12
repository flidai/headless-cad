# Flat-bottom blind drill from the top face; exact stock and tool remain recipes.
stock = extrude(id = "stock", profile = rectangle(id = "plate", width = 40, depth = 30), height = 12)
tool = translate(id = "positioned", body = extrude(id = "tool", profile = circle(id = "circle", radius = 4), height = 8), offset = [20,15,6])
drilled = cut(id = "drill", left = stock, right = tool)
output(name = "blind_bore", feature = drilled)
assert_faces(id = "floor_survives", from_feature = drilled, role = "bore_floor", in_feature = drilled, count = 1)
assert_property(id = "removed_volume", output = "blind_bore", property = "volume_mm3", min = 14098.4069, max = 14098.4072)
