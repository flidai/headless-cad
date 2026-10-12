# Native tangent fillets on four longitudinal edges, then genuine through bores.
# This cover geometry does not certify bearing load, fatigue or a standard fit.
pi = 3.141592653589793
corner_radius = param(name = "corner_radius", default = 4)
bore_radius = param(name = "bore_radius", default = 10)
height = param(name = "height", default = 8)
def model():
    if corner_radius < 2 or corner_radius > 6 or bore_radius < 8 or bore_radius > 12 or height < 4 or height > 12:
        fail("corner_radius 2..6, bore_radius 8..12, height 4..12 mm required")
    stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 60, depth = 40), height = height)
    front_left = fillet(id = "front_left", body = stock, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_min"}], radius = corner_radius)
    front_right = fillet(id = "front_right", body = front_left, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_min"}], radius = corner_radius)
    back_right = fillet(id = "back_right", body = front_right, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_max"}], radius = corner_radius)
    rounded = fillet(id = "back_left", body = back_right, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_max"}], radius = corner_radius)
    center_tool = translate(id = "center_tool", body = extrude(id = "center_cylinder", profile = circle(id = "center_profile", radius = bore_radius), height = height+2), offset = [30,20,-1])
    cover = cut(id = "bearing_bore", left = rounded, right = center_tool)
    bearing_feature = cover
    screw_tool = extrude(id = "screw_cylinder", profile = circle(id = "screw_profile", radius = 2.5), height = height+2)
    for index, point in enumerate([[10,10],[50,10],[50,30],[10,30]]):
        cutter = translate(id = "mounting_tool_"+str(index), body = screw_tool, offset = [point[0],point[1],-1])
        cover = cut(id = "mounting_hole_"+str(index), left = cover, right = cutter)
    output(name = "rounded_blank", feature = rounded)
    output(name = "bearing_cover", feature = cover)
    for name in [front_left,front_right,back_right,rounded]:
        assert_faces(id = name+"_survives", from_feature = name, role = "fillet_wall", in_feature = cover, count = 1)
    area = 2400-(4-pi)*corner_radius*corner_radius-pi*(bore_radius*bore_radius+25)
    assert_property(id = "cover_volume", output = "bearing_cover", property = "volume_mm3", min = area*height-0.00001, max = area*height+0.00001)
    section_view(id = "cover_midsection", output = "bearing_cover", origin = [0,0,height/2], min_area = area-0.000001, max_area = area+0.000001)
    section_circle_dimension(view = "cover_midsection", id = "bearing_diameter", from_feature = bearing_feature, role = "bore_wall", kind = "diameter", label_offset = [16,0], min = 2*bore_radius-0.000001, max = 2*bore_radius+0.000001)
    for feature, offset in [(front_left,[-12,-12]),(front_right,[12,-12]),(back_right,[12,12]),(rounded,[-12,12])]:
        section_circle_dimension(view = "cover_midsection", id = feature+"_radius", from_feature = feature, role = "fillet_wall", kind = "radius", coverage = "single_arc", label_offset = offset, min = corner_radius-0.000001, max = corner_radius+0.000001)
model()
