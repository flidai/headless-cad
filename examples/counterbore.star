# A larger top drill opens an annular shoulder around an existing through-hole.
stock = extrude(id = "stock", profile = rectangle(id = "plate", width = 40, depth = 30), height = 12)
pilot = translate(id = "pilot", body = extrude(id = "pilot_blank", profile = circle(id = "pilot_circle", radius = 3), height = 16), offset = [20,15,-2])
drilled = cut(id = "drilled", left = stock, right = pilot)
tool = translate(id = "counterbore_tool", body = extrude(id = "tool_blank", profile = circle(id = "tool_circle", radius = 6), height = 6), offset = [20,15,8])
finished = cut(id = "counterbore", left = drilled, right = tool)
output(name = "counterbore", feature = finished)
assert_faces(id = "annular_seat", from_feature = finished, role = "bore_floor", in_feature = finished, count = 1)
assert_property(id = "removed_union", output = "counterbore", property = "volume_mm3", min = 13721.4158, max = 13721.4162)
