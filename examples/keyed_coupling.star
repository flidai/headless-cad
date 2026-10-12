# Native shaft seat, hub keyway and separate key. This fixture establishes
# geometric engagement only: axial retention and rated torque remain unverified.
shaft_radius = param(name = "shaft_radius", default = 6)
key_width = param(name = "key_width", default = 4)
side_gap = param(name = "side_gap", default = 0.02)
roof_gap = param(name = "roof_gap", default = 0.05)
bore_gap = param(name = "bore_gap", default = 0.15)
def check_parameters():
    if shaft_radius < 5 or shaft_radius > 8:
        fail("shaft_radius must be 5..8")
    if key_width < 3 or key_width > 4:
        fail("key_width must be 3..4")
    if side_gap < 0.01 or side_gap > 0.1:
        fail("side_gap must be 0.01..0.1")
    if roof_gap < 0.01 or roof_gap > 0.2:
        fail("roof_gap must be 0.01..0.2")
    if bore_gap < 0.05 or bore_gap > 0.3:
        fail("bore_gap must be 0.05..0.3")
check_parameters()
r = shaft_radius
w = key_width+2*side_gap
floor = r-2
shaft_stock = extrude(id = "shaft_stock", profile = circle(id = "shaft_profile", radius = r), height = 40)
seat_box = extrude(id = "seat_box", profile = rectangle(id = "seat_profile", width = 4, depth = w), height = 13.5)
seat_tool = translate(id = "seat_tool", body = seat_box, offset = [floor,-w/2,10])
shaft_body = cut(id = "shaft_body", left = shaft_stock, right = seat_tool)
hub_stock = extrude(id = "hub_stock", profile = circle(id = "hub_profile", radius = 16, inner_radius = r+bore_gap), height = 14)
hub_box = extrude(id = "hub_box", profile = rectangle(id = "hub_slot_profile", width = r+2+roof_gap, depth = w), height = 16)
hub_tool = translate(id = "hub_tool", body = hub_box, offset = [0,-w/2,-1])
hub_body = cut(id = "hub_body", left = hub_stock, right = hub_tool)
key_body = extrude(id = "key_body", profile = rectangle(id = "key_profile", width = 4, depth = key_width), height = 13)
shaft = part(id = "shaft", body = shaft_body, part_number = "KEYED-SHAFT", revision = "A", material = "Steel", density_kg_per_m3 = 7800)
hub = part(id = "hub", body = hub_body, part_number = "KEYED-HUB", revision = "A", material = "Steel", density_kg_per_m3 = 7800)
key = part(id = "key", body = key_body, part_number = "PARALLEL-KEY", revision = "A", material = "Steel", density_kg_per_m3 = 7800)
coupling = assembly(id = "coupling", instances = [
    instance(id = "shaft", part = shaft),
    instance(id = "hub", part = hub, offset = [0,0,10]),
    instance(id = "key", part = key, offset = [floor,-key_width/2,10.25]),
])
assembly_output(name = "keyed_coupling", assembly = coupling)
output(name = "shaft", feature = shaft_body)
output(name = "hub", feature = hub_body)
output(name = "key", feature = key_body)
assert_faces(id = "seat_floor", from_feature = shaft_body, role = "keyseat_floor", in_feature = shaft_body, count = 1)
# Native keyed-body clearance queries are not yet supported. Nominal source
# gaps are not presented as independently certified whole-assembly clearances.
assert_property(id = "mass_budget", output = "keyed_coupling", property = "mass_kg", min = 0.001, max = 0.2)
section_view(id = "engagement", output = "keyed_coupling", origin = [0,0,17])
drawing_sheet(id = "key_engagement", title = "Keyed coupling — nominal engagement, retention not established", revision = "A", width = 210, height = 148,
    views = [{"view":"engagement","x":10,"y":10,"width":190,"height":100,"scale":2}])
