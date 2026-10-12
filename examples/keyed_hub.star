# Native annular hub with a through keyway. The intended mating key is 4 x 4 mm:
# a radius-6 shaft seat at radius 4 leaves its top at radius 8, below this roof
# at 8.05. Width 4.04 leaves nominal 0.02 mm per side. These are authored sizes,
# not a standardized fit, torque rating, native clearance certificate or axial
# retention design. No faceted approximation replaces the circular bore/wall.
hub_radius = param(name = "hub_radius", default = 24)
bore_radius = param(name = "bore_radius", default = 6.15)
hub_length = param(name = "hub_length", default = 14)
key_width = param(name = "key_width", default = 4.04)
key_roof = param(name = "key_roof", default = 8.05)
tool_start = param(name = "tool_start", default = 0)
tool_margin = param(name = "tool_margin", default = 1)
pose = param(name = "pose", default = 0)
scale = param(name = "scale", default = 1)
def check_parameters():
    if scale < 0.001 or scale > 100:
        fail("scale must be 0.001..100")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
check_parameters()
r = bore_radius*scale
outer = hub_radius*scale
h = hub_length*scale
w = key_width*scale
c = key_roof*scale
a = tool_start*scale
margin = tool_margin*scale
stock_blank = extrude(id = "stock_blank", profile = circle(id = "stock_profile", radius = outer, inner_radius = r), height = h)
tool_blank = extrude(id = "tool_blank", profile = rectangle(id = "tool_profile", width = c-a, depth = w), height = h+2*margin)
positioned_tool = translate(id = "positioned_tool", body = tool_blank, offset = [a,-w/2,-margin])
rotation = [0,0,0,1] if pose == 0 else ([0,0.7071067811865475,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
offset = [0,0,0] if pose == 0 else [20*scale,-8*scale,3*scale]
stock = transform(id = "stock_body", body = stock_blank, offset = offset, rotation = rotation)
tool = transform(id = "key_tool", body = positioned_tool, offset = offset, rotation = rotation)
keyed = cut(id = "keyed_body", left = stock, right = tool)
output(name = "keyed_hub", feature = keyed)
output(name = "stock", feature = stock)
assert_faces(id = "keyway_roof", from_feature = keyed, role = "hub_keyway_roof", in_feature = keyed, count = 1)
assert_faces(id = "keyway_side_negative", from_feature = keyed, role = "hub_keyway_side_negative", in_feature = keyed, count = 1)
assert_faces(id = "keyway_side_positive", from_feature = keyed, role = "hub_keyway_side_positive", in_feature = keyed, count = 1)
assert_faces(id = "remaining_bore", from_feature = stock_blank, role = "inner_wall", in_feature = keyed, count = 2)
assert_faces(id = "first_cap", from_feature = stock_blank, role = "start_cap", in_feature = keyed, count = 1)
assert_faces(id = "last_cap", from_feature = stock_blank, role = "end_cap", in_feature = keyed, count = 1)
assert_property(id = "material_removed", output = "keyed_hub", property = "volume_mm3", min = 0.01*scale*scale*scale, max = 3.141592653589793*(outer*outer-r*r)*h-0.000001*scale*scale*scale)
