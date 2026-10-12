# Four machining features drill a native concave jaw; the stock has no holes.
# Prescribed assembly positions are not a fit, contact-load or bearing-life proof.
seat_radius = param(name = "seat_radius", default = 18)
bore_radius = param(name = "bore_radius", default = 3)
depth = param(name = "depth", default = 18)
def validate_parameters():
    if seat_radius < 14 or seat_radius > 22:
        fail("seat_radius must be 14..22 mm")
    if bore_radius < 2 or bore_radius > 4:
        fail("bore_radius must be 2..4 mm")
    if depth < 12 or depth > 24:
        fail("depth must be 12..24 mm")
validate_parameters()
r = seat_radius
b = bore_radius
d = depth
centers = [[12,12],[88,12],[12,40],[88,40]]
def line(end):
    return {"kind":"line_to", "end":end}
stock = extrude(id = "stock", profile = path(id = "stock_profile", start = [0,0], segments = [
    line([100,0]), line([100,50]), line([50+r,50]),
    {"kind":"arc_to", "center":[50,50], "end":[50-r,50], "clockwise":True},
    line([0,50]), line([0,0]),
]), height = d)
tool_blank = extrude(id = "tool_blank", profile = circle(id = "tool_profile", radius = b), height = d+4)
def drill_all(body):
    for i,p in enumerate(centers):
        tool = translate(id = "drill_tool_"+str(i), body = tool_blank, offset = [p[0],p[1],-2])
        body = cut(id = "drill_"+str(i), left = body, right = tool)
    return body
jaw_body = drill_all(stock)
output(name = "stock", feature = stock)
output(name = "drilled_jaw", feature = jaw_body)
assert_faces(id = "retained_seat", from_feature = stock, role = "segment_3", in_feature = jaw_body, count = 1)
assert_faces(id = "retained_cap", from_feature = stock, role = "end_cap", in_feature = jaw_body, count = 1)
assert_faces(id = "all_drilled_walls", from_feature = tool_blank, role = "wall", in_feature = jaw_body, count = 4)
def check_drill_references():
    for i in range(4):
        assert_faces(id = "drilled_wall_"+str(i), from_feature = "drill_tool_"+str(i), role = "translated", in_feature = jaw_body, count = 1)
check_drill_references()
jaw = part(id = "jaw", body = jaw_body, part_number = "MB-JAW", material = "Aluminum", density_kg_per_m3 = 2700)
shaft = part(id = "shaft", body = revolve(id = "shaft_body", profile = polygon(id = "shaft_profile", points = [
    [0,0],[r+3,0],[r+3,4],[r-2,4],[r-2,d+20],[r+3,d+20],[r+3,d+24],[0,d+24],
])), part_number = "MB-SHAFT", material = "Steel", density_kg_per_m3 = 7800)
sleeve = part(id = "sleeve", body = extrude(id = "sleeve_body", profile = circle(id = "sleeve_profile", radius = b, inner_radius = b-1), height = d+4), part_number = "MB-SLEEVE", material = "Steel", density_kg_per_m3 = 7800)
assembly_output(name = "machined_bearing_support", assembly = assembly(id = "machined_bearing_support", instances = [
    instance(id = "lower_jaw", part = jaw),
    instance(id = "upper_jaw", part = jaw, offset = [100,102,0], rotation = [0,0,1,0]),
    instance(id = "shaft", part = shaft, offset = [50,51,-12]),
]+[instance(id = "sleeve_"+str(i), part = sleeve, offset = [p[0],p[1],-2]) for i,p in enumerate(centers+[[p[0],102-p[1]] for p in centers])]))
pi = 3.141592653589793
jaw_volume = (5000-pi*r*r/2-4*pi*b*b)*d
shaft_volume = pi*(8*(r+3)*(r+3)+(d+16)*(r-2)*(r-2))
sleeve_volume = (2*b-1)*pi*(d+4)
assert_property(id = "jaw_material", output = "drilled_jaw", property = "volume_mm3", min = jaw_volume-0.0001, max = jaw_volume+0.0001)
assert_property(id = "assembly_material", output = "machined_bearing_support", property = "volume_mm3", min = 2*jaw_volume+shaft_volume+8*sleeve_volume-0.0001, max = 2*jaw_volume+shaft_volume+8*sleeve_volume+0.0001)
assert_property(id = "material_mass", output = "machined_bearing_support", property = "mass_kg", max = 1.5)
