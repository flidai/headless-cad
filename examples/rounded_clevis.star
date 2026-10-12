# Analytic rounded outlines, a reusable pair of cheeks, and a pinned link.
# Datum placements prescribe this pose; this is not a joint or load solver.
link_length = param(name = "link_length", default = 45)
clevis_gap = param(name = "clevis_gap", default = 16)
corner_radius = param(name = "corner_radius", default = 8)
def validate_parameters():
    if link_length < 32 or link_length > 64:
        fail("link_length must be 32..64 mm")
    if clevis_gap < 12 or clevis_gap > 24:
        fail("clevis_gap must be 12..24 mm")
    if corner_radius < 4 or corner_radius > 10:
        fail("corner_radius must be 4..10 mm")
validate_parameters()
def line(end):
    return {"kind":"line_to", "end":end}
def arc(center, end):
    return {"kind":"arc_to", "center":center, "end":end}
r = corner_radius
base_profile = path(id = "base_profile", start = [r,0], segments = [
    line([80-r,0]), arc([80-r,r],[80,r]),
    line([80,60-r]), arc([80-r,60-r],[80-r,60]),
    line([r,60]), arc([r,60-r],[0,60-r]),
    line([0,r]), arc([r,r],[r,0]),
], circular_holes = [{"center":p,"radius":3} for p in [[10,10],[70,10],[70,50],[10,50]]])
cheek_profile = path(id = "cheek_profile", start = [-12,0], segments = [
    line([12,0]), line([12,28]), arc([0,28],[-12,28]), line([-12,0]),
], circular_holes = [{"center":[0,28],"radius":5.5}])
link_profile = path(id = "link_profile", start = [0,-8], segments = [
    line([link_length,-8]), arc([link_length,0],[link_length,8]),
    line([0,8]), arc([0,0],[0,-8]),
], circular_holes = [{"center":[0,0],"radius":5.5}, {"center":[link_length,0],"radius":4}])
base_body = extrude(id = "base_body", profile = base_profile, height = 6)
cheek_body = extrude(id = "cheek_body", profile = cheek_profile, height = 6)
link_body = extrude(id = "link_body", profile = link_profile, height = clevis_gap-6)
pin_body = extrude(id = "pin_body", profile = circle(id = "pin_profile", radius = 5), height = clevis_gap+16)
assert_faces(id = "left_link_round", from_feature = link_body, role = "segment_3", in_feature = link_body, count = 1)
assert_faces(id = "link_cap", from_feature = link_body, role = "end_cap", in_feature = link_body, count = 1)

upright = [0.7071067811865475,0,0,0.7071067811865476]
base = part(id = "base", body = base_body, part_number = "RC-BASE", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"left_cheek": datum(offset = [40,30-clevis_gap/2,6], rotation = upright), "right_cheek": datum(offset = [40,36+clevis_gap/2,6], rotation = upright)})
cheek = part(id = "cheek", body = cheek_body, part_number = "RC-CHEEK", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"base": datum(), "pivot": datum(offset = [0,28,0])})
link = part(id = "link", body = link_body, part_number = "RC-LINK", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"pivot": datum()})
pin = part(id = "pin", body = pin_body, part_number = "RC-PIN", material = "Steel", density_kg_per_m3 = 7800,
    datums = {"base": datum()})
assembly_output(name = "rounded_clevis", assembly = assembly(id = "rounded_clevis", instances = [
    instance(id = "base", part = base),
    instance(id = "left_cheek", part = cheek, attach_to = "base", datum = "base", target_datum = "left_cheek"),
    instance(id = "right_cheek", part = cheek, attach_to = "base", datum = "base", target_datum = "right_cheek"),
    instance(id = "link", part = link, attach_to = "right_cheek", datum = "pivot", target_datum = "pivot", offset = [0,0,9]),
    instance(id = "pin", part = pin, attach_to = "right_cheek", datum = "base", target_datum = "pivot", offset = [0,0,-2]),
]))
output(name = "link", feature = link_body)
output(name = "base", feature = base_body)

pi = 3.141592653589793
base_volume = (80*60-(4-pi)*r*r-36*pi)*6
cheek_volume = (24*28+(72-30.25)*pi)*6
link_volume = (16*link_length+(64-30.25-16)*pi)*(clevis_gap-6)
pin_volume = 25*pi*(clevis_gap+16)
material_volume = base_volume+2*cheek_volume+link_volume+pin_volume
assert_property(id = "link_material", output = "link", property = "volume_mm3", min = link_volume-0.0001, max = link_volume+0.0001)
assert_property(id = "assembly_material", output = "rounded_clevis", property = "volume_mm3", min = material_volume-0.0001, max = material_volume+0.0001)
assert_property(id = "mass_budget", output = "rounded_clevis", property = "mass_kg", max = 0.4)
