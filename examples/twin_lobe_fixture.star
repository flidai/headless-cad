# Native curved Booleans partition two overlapping circular blanks into three inserts.
# The fixture is a prescribed tooling layout, not a fit or contact-load certificate.
left_radius = param(name = "left_radius", default = 22)
spacing = param(name = "spacing", default = 30)
depth = param(name = "depth", default = 8)
def validate_parameters():
    if left_radius < 20 or left_radius > 24:
        fail("left_radius must be 20..24 mm")
    if spacing < 26 or spacing > 34:
        fail("spacing must be 26..34 mm")
    if depth < 6 or depth > 10:
        fail("depth must be 6..10 mm")
validate_parameters()
def disk(id, radius, height):
    return extrude(id = id, profile = circle(id = id+"/profile", radius = radius), height = height)
left_blank = disk("left_blank", left_radius, depth)
right_blank = disk("right_blank", 18, depth)
right_placed = translate(id = "right_placed", body = right_blank, offset = [spacing,0,0])
left_lobe = cut(id = "left_lobe", left = left_blank, right = right_placed)
right_lobe = cut(id = "right_lobe", left = right_placed, right = left_blank)
overlap = intersect(id = "overlap", left = left_blank, right = right_placed)
base_left = disk("base_left", left_radius, 4)
base_right = translate(id = "base_right", body = disk("base_right_blank", 18, 4), offset = [spacing,0,0])
union_base = union(id = "union_base", left = base_left, right = base_right)
output(name = "left_lobe", feature = left_lobe)
output(name = "overlap", feature = overlap)
output(name = "union_base", feature = union_base)
# Source walls survive trimming; major exterior portions become two native faces.
assert_faces(id = "left_outer", from_feature = left_blank, role = "wall", in_feature = left_lobe, count = 2)
assert_faces(id = "cut_wall", from_feature = right_blank, role = "wall", in_feature = left_lobe, count = 1)
assert_faces(id = "lens_left", from_feature = left_blank, role = "wall", in_feature = overlap, count = 1)
assert_faces(id = "lens_right", from_feature = right_blank, role = "wall", in_feature = overlap, count = 1)
assert_faces(id = "union_left", from_feature = base_left, role = "wall", in_feature = union_base, count = 2)
assert_faces(id = "union_right", from_feature = "base_right_blank", role = "wall", in_feature = union_base, count = 2)
base = part(id = "base", body = union_base, part_number = "TL-BASE", material = "Aluminum", density_kg_per_m3 = 2700)
left_part = part(id = "left_lobe", body = left_lobe, part_number = "TL-LEFT", material = "Aluminum", density_kg_per_m3 = 2700)
right_part = part(id = "right_lobe", body = right_lobe, part_number = "TL-RIGHT", material = "Aluminum", density_kg_per_m3 = 2700)
lens = part(id = "overlap", body = overlap, part_number = "TL-INSERT", material = "Steel", density_kg_per_m3 = 7800)
boss = part(id = "boss", body = extrude(id = "boss_body", profile = circle(id = "boss_profile", radius = 4, inner_radius = 2), height = 4), part_number = "TL-BOSS", material = "Steel", density_kg_per_m3 = 7800)
assembly_output(name = "twin_lobe_fixture", assembly = assembly(id = "twin_lobe_fixture", instances = [
    instance(id = "base", part = base),
    instance(id = "left_lobe", part = left_part, offset = [0,0,4]),
    instance(id = "right_lobe", part = right_part, offset = [0,0,4]),
    instance(id = "overlap", part = lens, offset = [0,0,4]),
    instance(id = "left_boss", part = boss, offset = [0,0,4+depth]),
    instance(id = "right_boss", part = boss, offset = [spacing,0,4+depth]),
]))
assert_property(id = "left_material", output = "left_lobe", property = "volume_mm3", min = 3000, max = 20000)
assert_property(id = "lens_material", output = "overlap", property = "volume_mm3", min = 100, max = 8000)
assert_property(id = "fixture_mass", output = "twin_lobe_fixture", property = "mass_kg", max = 0.2)
