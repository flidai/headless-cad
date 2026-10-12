# A covered serpentine passage built from native profiles and reusable parts.
# Openings, seating and ports express design intent, not a pressure or seal rating.
channel_width = param(name = "channel_width", default = 8)
channel_depth = param(name = "channel_depth", default = 10)
def validate_parameters():
    if channel_width < 6 or channel_width > 10:
        fail("channel_width must be 6..10 mm")
    if channel_depth < 6 or channel_depth > 18:
        fail("channel_depth must be 6..18 mm")
validate_parameters()
w = channel_width
h = channel_depth
stack_height = h + 6
outline = [[0,0],[96,0],[96,64],[0,64]]
# Clockwise inner ring; its two reentrant turns retain actual material webs.
channel = [[12,12+w],[84-w,12+w],[84-w,32],[12,32],
           [12,44+w],[84,44+w],[84,44],[12+w,44],
           [12+w,32+w],[84,32+w],[84,12],[12,12]]
bolt_positions = [[6,6],[90,6],[90,58],[6,58]]
bolt_holes = [{"center":p,"radius":2.5} for p in bolt_positions]
ports = [[12+w/2,12+w/2],[84-w/2,44+w/2]]

floor_body = extrude(id = "floor_body", profile = polygon(id = "floor_profile", points = outline, circular_holes = bolt_holes), height = 3)
channel_body = extrude(id = "channel_body", profile = polygon(id = "channel_profile", points = outline, holes = [channel], circular_holes = bolt_holes), height = h)
cover_body = extrude(id = "cover_body", profile = polygon(id = "cover_profile", points = outline, circular_holes = bolt_holes + [{"center":p,"radius":2.5} for p in ports]), height = 3)
port_body = extrude(id = "port_body", profile = circle(id = "port_profile", radius = 4, inner_radius = 2.5), height = 10)
bolt_body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [[0,0],[2,0],[2,stack_height],[4,stack_height],[4,stack_height+3],[0,stack_height+3]]))
assert_faces(id = "channel_walls", from_feature = channel_body, role = "hole_walls", in_feature = channel_body, count = 12)

floor = part(id = "floor", body = floor_body, part_number = "SM-FLOOR", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"channel_seat": datum(offset = [0,0,3])})
core = part(id = "channel_core", body = channel_body, part_number = "SM-CORE", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"base": datum(), "cover_seat": datum(offset = [0,0,h])})
cover = part(id = "cover", body = cover_body, part_number = "SM-COVER", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"base": datum(), "inlet": datum(offset = [ports[0][0],ports[0][1],3]), "outlet": datum(offset = [ports[1][0],ports[1][1],3])})
port = part(id = "port", body = port_body, part_number = "SM-PORT", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"base": datum()})
bolt = part(id = "bolt", body = bolt_body, part_number = "SM-BOLT", material = "Steel", density_kg_per_m3 = 7800)
assembly_output(name = "serpentine_manifold", assembly = assembly(id = "serpentine_manifold", instances = [
    instance(id = "floor", part = floor),
    instance(id = "core", part = core, attach_to = "floor", datum = "base", target_datum = "channel_seat"),
    instance(id = "cover", part = cover, attach_to = "core", datum = "base", target_datum = "cover_seat"),
    instance(id = "inlet", part = port, attach_to = "cover", datum = "base", target_datum = "inlet"),
    instance(id = "outlet", part = port, attach_to = "cover", datum = "base", target_datum = "outlet"),
] + [instance(id = "bolt_"+str(i), part = bolt, offset = [bolt_positions[i][0],bolt_positions[i][1],0]) for i in range(4)]))
# An inspection configuration exposes the passage without changing its geometry.
assembly_output(name = "open_channel", assembly = assembly(id = "open_channel", instances = [
    instance(id = "open_floor", part = floor),
    instance(id = "open_core", part = core, attach_to = "open_floor", datum = "base", target_datum = "channel_seat"),
]))
output(name = "channel_core", feature = channel_body)

# Three horizontal legs and two vertical connectors give an independent area.
pi = 3.141592653589793
channel_area = 3*72*w + w*(20-w) + w*(12-w)
core_volume = (96*64 - channel_area - 25*pi)*h
floor_volume = (96*64 - 25*pi)*3
cover_volume = (96*64 - 37.5*pi)*3
port_volume = (16-6.25)*pi*10
bolt_volume = pi*(4*stack_height+48)
material_volume = core_volume + floor_volume + cover_volume + 2*port_volume + 4*bolt_volume
assert_property(id = "core_material", output = "channel_core", property = "volume_mm3", min = core_volume-0.0001, max = core_volume+0.0001)
assert_property(id = "assembly_material", output = "serpentine_manifold", property = "volume_mm3", min = material_volume-0.0001, max = material_volume+0.0001)
assert_property(id = "mass_budget", output = "serpentine_manifold", property = "mass_kg", max = 0.4)
