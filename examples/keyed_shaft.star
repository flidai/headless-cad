# A finite rectangular keyseat crossing the native cylindrical wall. Both end
# journals remain intact. This is a machining-domain fixture: a matching internal
# hub keyway and axial retention are still required before a complete drive design.
shaft_radius = param(name = "shaft_radius", default = 6)
shaft_length = param(name = "shaft_length", default = 40)
key_width = param(name = "key_width", default = 4)
key_depth = param(name = "key_depth", default = 2)
key_start = param(name = "key_start", default = 10)
key_length = param(name = "key_length", default = 14)
pose = param(name = "pose", default = 0)
scale = param(name = "scale", default = 1)
def check_parameters():
    if scale < 0.001 or scale > 100:
        fail("scale must be 0.001..100")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
check_parameters()
r = shaft_radius*scale
h = shaft_length*scale
w = key_width*scale
d = key_depth*scale
z = key_start*scale
length = key_length*scale
stock = extrude(id = "stock_body", profile = circle(id = "stock_profile", radius = r), height = h)
tool_blank = extrude(id = "tool_blank", profile = rectangle(id = "tool_profile", width = d+2*scale, depth = w), height = length)
tool = translate(id = "key_tool", body = tool_blank, offset = [r-d,-w/2,z])
keyed = cut(id = "keyed_body", left = stock, right = tool)
rotation = [0,0,0,1] if pose == 0 else ([0,0.7071067811865475,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
offset = [0,0,0] if pose == 0 else [20*scale,-8*scale,3*scale]
finished = transform(id = "finished", body = keyed, offset = offset, rotation = rotation)
output(name = "keyed_shaft", feature = finished)
output(name = "stock", feature = stock)
assert_faces(id = "seat_floor", from_feature = keyed, role = "keyseat_floor", in_feature = finished, count = 1)
assert_faces(id = "seat_start", from_feature = keyed, role = "keyseat_start", in_feature = finished, count = 1)
assert_faces(id = "seat_end", from_feature = keyed, role = "keyseat_end", in_feature = finished, count = 1)
assert_faces(id = "first_journal_cap", from_feature = stock, role = "start_cap", in_feature = finished, count = 1)
assert_faces(id = "last_journal_cap", from_feature = stock, role = "end_cap", in_feature = finished, count = 1)
assert_property(id = "material_removed", output = "keyed_shaft", property = "volume_mm3", min = 0.01*scale*scale*scale, max = 3.141592653589793*r*r*h-0.000001*scale*scale*scale)
