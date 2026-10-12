# Two reusable tooling pallets: six top blind sockets, two underside sockets,
# and four through-bores each. Flat-bottom cuts are analytic cylinders.
# Prescribed assembly placement does not certify fits, threads or load capacity.
socket_radius = param(name = "socket_radius", default = 8)
socket_depth = param(name = "socket_depth", default = 12)
plate_height = param(name = "plate_height", default = 28)
def check_parameters():
    if socket_radius < 6 or socket_radius > 10:
        fail("socket_radius must be 6..10 mm")
    if socket_depth < 8 or socket_depth > 16:
        fail("socket_depth must be 8..16 mm")
    if plate_height < 24 or plate_height > 32:
        fail("plate_height must be 24..32 mm")
check_parameters()
r = socket_radius
h = socket_depth
d = plate_height
top_centers = [[20,20],[60,20],[100,20],[20,60],[60,60],[100,60]]
bottom_centers = [[40,40],[80,40]]
through_centers = [[8,8],[112,8],[8,72],[112,72]]
stock = extrude(id = "stock", profile = rectangle(id = "plate_profile", width = 120, depth = 80), height = d)
top_tool = extrude(id = "top_tool", profile = circle(id = "top_circle", radius = r), height = h+2)
bottom_tool = extrude(id = "bottom_tool", profile = circle(id = "bottom_circle", radius = 4), height = 8)
through_tool = extrude(id = "through_tool", profile = circle(id = "through_circle", radius = 2.5), height = d+4)
def drill_pattern(body, prefix, tool, centers, z):
    for i,xy in enumerate(centers):
        cutter = translate(id = prefix+"_tool_"+str(i), body = tool, offset = [xy[0],xy[1],z])
        body = cut(id = prefix+"_cut_"+str(i), left = body, right = cutter)
    return body
top_drilled = drill_pattern(stock, "top", top_tool, top_centers, d-h)
both_sides = drill_pattern(top_drilled, "bottom", bottom_tool, bottom_centers, -2)
finished = drill_pattern(both_sides, "through", through_tool, through_centers, -2)
output(name = "stock", feature = stock)
output(name = "pallet", feature = finished)
assert_faces(id = "top_faces_retained", from_feature = stock, role = "end_cap", in_feature = finished, count = 1)
assert_faces(id = "all_top_walls", from_feature = top_tool, role = "wall", in_feature = finished, count = 6)
assert_faces(id = "all_bottom_walls", from_feature = bottom_tool, role = "wall", in_feature = finished, count = 2)
def check_floors():
    for prefix,count in [["top",6],["bottom",2]]:
        for i in range(count):
            assert_faces(id = prefix+"_floor_"+str(i), from_feature = prefix+"_cut_"+str(i), role = "bore_floor", in_feature = finished, count = 1)
check_floors()
pallet = part(id = "pallet", body = finished, part_number = "BP-PALLET", material = "Aluminum", density_kg_per_m3 = 2700, datums = {
    "socket_"+str(i): face_datum(from_feature = "top_cut_"+str(i), role = "bore_floor", x_direction = [1,0,0]) for i in range(6)
})
assembly_output(name = "pallet_inspection", assembly = assembly(id = "pallet_inspection", instances = [instance(id = "inspection_pallet", part = pallet)]))
pin = part(id = "pin", body = revolve(id = "pin_body", profile = polygon(id = "pin_profile", points = [
    [0,0],[r-0.5,0],[r-0.5,h+10],[r+2,h+10],[r+2,h+14],[0,h+14],
])), part_number = "BP-PIN", material = "Steel", density_kg_per_m3 = 7800, datums = {"tip": datum()})
foot = part(id = "foot", body = extrude(id = "foot_body", profile = circle(id = "foot_profile", radius = 3.5), height = 16), part_number = "BP-FOOT", material = "Steel", density_kg_per_m3 = 7800)
sleeve = part(id = "sleeve", body = extrude(id = "sleeve_body", profile = circle(id = "sleeve_profile", radius = 2.5, inner_radius = 1.5), height = d+2), part_number = "BP-SLEEVE", material = "Steel", density_kg_per_m3 = 7800)
station = assembly(id = "station", instances = [instance(id = "pallet", part = pallet)]+[
    instance(id = "pin_"+str(i), part = pin, attach_to = "pallet", datum = "tip", target_datum = "socket_"+str(i)) for i,xy in enumerate(top_centers)
]+[instance(id = "foot_"+str(i), part = foot, offset = [xy[0],xy[1],-10]) for i,xy in enumerate(bottom_centers)
]+[instance(id = "sleeve_"+str(i), part = sleeve, offset = [xy[0],xy[1],-1]) for i,xy in enumerate(through_centers)])
assembly_output(name = "blind_bore_pallet", assembly = assembly(id = "pallet_pair", instances = [
    assembly_instance(id = "station_a", assembly = station),
    assembly_instance(id = "station_b", assembly = station, offset = [150,0,0]),
]))
pi = 3.141592653589793
pallet_v = 9600*d-pi*(6*r*r*h+192+25*d)
steel_v = 6*pi*((r-0.5)*(r-0.5)*(h+10)+4*(r+2)*(r+2))+392*pi+16*pi*(d+2)
assert_property(id = "pallet_volume", output = "pallet", property = "volume_mm3", min = pallet_v-0.0001, max = pallet_v+0.0001)
assert_property(id = "pair_volume", output = "blind_bore_pallet", property = "volume_mm3", min = 2*(pallet_v+steel_v)-0.0001, max = 2*(pallet_v+steel_v)+0.0001)
assert_property(id = "pair_mass", output = "blind_bore_pallet", property = "mass_kg", min = 2*(pallet_v*2700e-9+steel_v*7800e-9)-0.000001, max = 2*(pallet_v*2700e-9+steel_v*7800e-9)+0.000001)
