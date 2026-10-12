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
