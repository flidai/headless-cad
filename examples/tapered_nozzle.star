# Native conical transitions, circular bores, and chamfered shoulders.
# This is an exploded geometry example, not a validated fluid-flow design.
neck_length = param(name = "neck_length", default = 28)
def validate_parameters():
    if neck_length < 16 or neck_length > 48:
        fail("neck_length must be 16..48 mm")
validate_parameters()
z = 16 + neck_length
profile = polygon(id = "nozzle_profile", points = [[6,0],[20,0],[24,4],[24,10],[18,16],[14,z],[12,z+2],[8,z+2],[6,z-2]])
body = revolve(id = "nozzle_body", profile = profile)
assert_faces(id = "outer_taper", from_feature = body, role = "segment_4", in_feature = body)
pin_profile = polygon(id = "pin_profile", points = [[0,0],[5,0],[5,20],[3,40],[3,44],[0,44]])
pin_body = revolve(id = "pin_body", profile = pin_profile)
nozzle = part(id = "nozzle", body = body, material = "Demo aluminum", density_kg_per_m3 = 2700)
pin = part(id = "pin", body = pin_body, material = "Demo steel", density_kg_per_m3 = 7800)
assembly_output(name = "tapered_nozzle", assembly = assembly(id = "tapered_nozzle", instances = [instance(id = "nozzle", part = nozzle),instance(id = "pin", part = pin, offset = [60,0,0])]))
output(name = "nozzle", feature = body)
output(name = "pin", feature = pin_body)
