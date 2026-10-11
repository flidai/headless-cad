# Three coaxial, separate turned components. True circles/cylinders are native;
# display and STL meshes are derived. No curved Boolean or hollow collar implied.
shaft_radius = param(name = "shaft_radius", default = 8)
shaft_length = param(name = "shaft_length", default = 60)
def check_parameters():
    if shaft_radius < 4 or shaft_radius > 12 or shaft_length < 30 or shaft_length > 100:
        fail("shaft_radius must be 4..12 and shaft_length 30..100 mm")
check_parameters()

def turned(id, radius, length):
    return extrude(id = id, profile = circle(id = id + "/profile", radius = radius), height = length)

shaft_body = turned("shaft_body", shaft_radius, shaft_length)
flange_body = turned("flange_body", 18, 6)
cap_body = turned("cap_body", 12, 4)
shaft = part(id = "shaft", body = shaft_body, material = "Demo steel", density_kg_per_m3 = 7800,
             datums = {"base": datum(), "end": face_datum(from_feature = shaft_body, role = "end_cap", x_direction = [1, 0, 0])})
flange = part(id = "flange", body = flange_body,
              datums = {"end": face_datum(from_feature = flange_body, role = "end_cap", x_direction = [1, 0, 0])})
cap = part(id = "cap", body = cap_body, datums = {"base": datum()})
assembly_output(name = "shaft_stack", assembly = assembly(id = "shaft_stack", instances = [
    instance(id = "flange", part = flange),
    instance(id = "shaft", part = shaft, attach_to = "flange", datum = "base", target_datum = "end"),
    instance(id = "cap", part = cap, attach_to = "shaft", datum = "base", target_datum = "end"),
]))
output(name = "shaft", feature = shaft_body)
output(name = "flange", feature = flange_body)
output(name = "cap", feature = cap_body)
# Native geometry rotation, separately exported to exercise analytic frame handling.
output(name = "tilted_shaft", feature = transform(id = "tilted_shaft", body = shaft_body,
       offset = [30, -10, 5], rotation = [0, 0.25881904510252074, 0, 0.9659258262890683]))
