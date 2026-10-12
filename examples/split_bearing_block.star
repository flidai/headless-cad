# A split support with a native concave cylindrical seat and shared jaw geometry.
# Placements prescribe an inspection pose; this is not a bearing-fit or load certificate.
seat_radius = param(name = "seat_radius", default = 14)
depth = param(name = "depth", default = 16)
jaw_gap = param(name = "jaw_gap", default = 2)
def validate_parameters():
    if seat_radius < 10 or seat_radius > 18:
        fail("seat_radius must be 10..18 mm")
    if depth < 12 or depth > 24:
        fail("depth must be 12..24 mm")
    if jaw_gap < 0 or jaw_gap > 3:
        fail("jaw_gap must be 0..3 mm")
validate_parameters()
r = seat_radius
d = depth
g = jaw_gap
def line(end):
    return {"kind":"line_to", "end":end}
profile = path(id = "jaw_profile", start = [0,0], segments = [
    line([80,0]), line([80,40]), line([40+r,40]),
    {"kind":"arc_to", "center":[40,40], "end":[40-r,40], "clockwise":True},
    line([0,40]), line([0,0]),
], circular_holes = [{"center":[x,12],"radius":3} for x in [10,70]])
jaw_body = extrude(id = "jaw_body", profile = profile, height = d)
output(name = "jaw", feature = jaw_body)
assert_faces(id = "concave_seat", from_feature = jaw_body, role = "segment_3", in_feature = jaw_body, count = 1)
jaw = part(id = "jaw", body = jaw_body, part_number = "SB-JAW", material = "Aluminum", density_kg_per_m3 = 2700)
shaft = part(id = "shaft", body = revolve(id = "shaft_body", profile = polygon(id = "shaft_profile", points = [
    [0,0],[r+3,0],[r+3,4],[r-2,4],[r-2,d+20],[r+3,d+20],[r+3,d+24],[0,d+24],
])), part_number = "SB-SHAFT", material = "Steel", density_kg_per_m3 = 7800)
sleeve = part(id = "sleeve", body = extrude(id = "sleeve_body", profile = circle(id = "sleeve_profile", radius = 3, inner_radius = 2), height = d+4), part_number = "SB-SLEEVE", material = "Steel", density_kg_per_m3 = 7800)
assembly_output(name = "split_bearing_block", assembly = assembly(id = "split_bearing_block", instances = [
    instance(id = "lower_jaw", part = jaw),
    instance(id = "upper_jaw", part = jaw, offset = [80,80+g,0], rotation = [0,0,1,0]),
    instance(id = "shaft", part = shaft, offset = [40,40+g/2,-12]),
]+[instance(id = "sleeve_"+str(i), part = sleeve, offset = [p[0],p[1],-2]) for i,p in enumerate([[10,12],[70,12],[10,68+g],[70,68+g]])]))
pi = 3.141592653589793
jaw_volume = (3200-pi*r*r/2-18*pi)*d
shaft_volume = pi*(8*(r+3)*(r+3)+(d+16)*(r-2)*(r-2))
sleeve_volume = 5*pi*(d+4)
assert_property(id = "jaw_material", output = "jaw", property = "volume_mm3", min = jaw_volume-0.0001, max = jaw_volume+0.0001)
assert_property(id = "assembly_material", output = "split_bearing_block", property = "volume_mm3", min = 2*jaw_volume+shaft_volume+4*sleeve_volume-0.0001, max = 2*jaw_volume+shaft_volume+4*sleeve_volume+0.0001)
assert_property(id = "material_mass", output = "split_bearing_block", property = "mass_kg", max = 0.7)
