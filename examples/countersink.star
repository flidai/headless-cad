# Native conical countersink around a through-drilled pilot hole.
depth = param(name = "depth", default = 4)
def validate_depth():
    if depth < 2 or depth > 5:
        fail("depth must be 2..5 mm")
validate_depth()
stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 60, depth = 40), height = 12)
drill = translate(id = "drill", body = extrude(id = "pilot", profile = circle(id = "pilot_profile", radius = 3), height = 14), offset = [30,20,-1])
drilled = cut(id = "drilled", left = stock, right = drill)
sink_body = revolve(id = "sink_body", profile = polygon(id = "sink_profile", points = [[0,0],[3,0],[4+depth,depth+1],[0,depth+1]]))
sink = translate(id = "sink", body = sink_body, offset = [30,20,12-depth])
finished = cut(id = "finished", left = drilled, right = sink)
output(name = "stock", feature = stock)
output(name = "countersunk", feature = finished)
assert_faces(id = "cone_survives", from_feature = sink_body, role = "conical_faces", in_feature = finished, count = 1)
assert_faces(id = "pilot_survives", from_feature = "pilot", role = "wall", in_feature = finished, count = 1)
pi = 3.141592653589793
r = 3+depth
expected = 60*40*12-pi*9*(12-depth)-pi*depth*(9+3*r+r*r)/3
assert_property(id = "volume", output = "countersunk", property = "volume_mm3", min = expected-0.00001, max = expected+0.00001)

# Section intent is source: edits rebuild these native curves with the part.
# These area checks are engineering assertions, not manufacturing tolerances.
pilot_area = 60*40-pi*9
seat_area = 60*40-pi*(3+depth/2)*(3+depth/2)
section_view(id = "pilot", output = "countersunk", origin = [0,0,2], min_area = pilot_area-0.000001, max_area = pilot_area+0.000001)
section_view(id = "seat", output = "countersunk", origin = [0,0,12-depth/2], min_area = seat_area-0.000001, max_area = seat_area+0.000001)

section_circle_dimension(view = "pilot", id = "pilot_diameter", from_feature = "pilot", role = "wall", kind = "diameter", label_offset = [0,30], min = 5.999999, max = 6.000001)
section_circle_dimension(view = "seat", id = "seat_diameter", from_feature = sink_body, role = "conical_faces", kind = "diameter", label_offset = [0,30], min = 6+depth-0.000001, max = 6+depth+0.000001)
