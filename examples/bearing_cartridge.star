# Reusable hollow sleeve and two retainer rings around a separate shaft.
# One millimetre radial clearance is prescribed by the recipe; no mate or
# interference solver is implied. The two retainers share one native definition.
bore_radius = param(name = "bore_radius", default = 6)
sleeve_length = param(name = "sleeve_length", default = 40)
def check_parameters():
    if bore_radius < 4 or bore_radius > 8 or sleeve_length < 20 or sleeve_length > 80:
        fail("bore_radius must be 4..8 and sleeve_length 20..80 mm")
check_parameters()

def turned(id, radius, height, inner_radius = 0):
    return extrude(id = id, profile = circle(id = id + "/profile", radius = radius, inner_radius = inner_radius), height = height)

sleeve_body = turned("sleeve_body", 10, sleeve_length, bore_radius)
retainer_body = turned("retainer_body", 16, 6, 10)
shaft_body = turned("shaft_body", bore_radius - 1, sleeve_length + 30)
assert_faces(id = "sleeve_bore", from_feature = sleeve_body, role = "inner_wall", in_feature = sleeve_body)
sleeve = part(id = "sleeve", body = sleeve_body, part_number = "BC-SLEEVE", material = "Demo bronze", density_kg_per_m3 = 8800,
              datums = {"base": face_datum(from_feature = sleeve_body, role = "start_cap", x_direction = [1, 0, 0]),
                        "end": face_datum(from_feature = sleeve_body, role = "end_cap", x_direction = [1, 0, 0]),
                        "center": datum(offset = [0, 0, sleeve_length / 2])})
retainer = part(id = "retainer", body = retainer_body, part_number = "BC-RETAINER", material = "Demo steel", density_kg_per_m3 = 7800,
                datums = {"base": face_datum(from_feature = retainer_body, role = "start_cap", x_direction = [1, 0, 0]),
                          "end": face_datum(from_feature = retainer_body, role = "end_cap", x_direction = [1, 0, 0])})
shaft = part(id = "shaft", body = shaft_body, part_number = "BC-SHAFT", material = "Demo steel", density_kg_per_m3 = 7800,
             datums = {"center": datum(offset = [0, 0, (sleeve_length + 30) / 2])})
assembly_output(name = "bearing_cartridge", assembly = assembly(id = "bearing_cartridge", instances = [
    instance(id = "sleeve", part = sleeve),
    instance(id = "front_retainer", part = retainer, attach_to = "sleeve", datum = "base", target_datum = "base"),
    instance(id = "rear_retainer", part = retainer, attach_to = "sleeve", datum = "end", target_datum = "end"),
    instance(id = "shaft", part = shaft, attach_to = "sleeve", datum = "center", target_datum = "center"),
]))
output(name = "sleeve", feature = sleeve_body)
output(name = "retainer", feature = retainer_body)
output(name = "shaft", feature = shaft_body)
