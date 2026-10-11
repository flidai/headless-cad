# A mixed-material balance arm. Move the steel counterweight along the beam,
# then query headless properties to observe the assembly's center of mass.
# All components are rigid datum attachments; no motion solver is implied.
counterweight_distance = param(name = "counterweight_distance", default = 30)

def validate_parameters():
    if counterweight_distance < 15 or counterweight_distance > 45:
        fail("counterweight_distance must be 15..45 mm")
validate_parameters()

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)

base = part(id = "base", body = box("base_body", 40, 40, 6), part_number = "BAL-BASE",
            material = "Demo aluminum", density_kg_per_m3 = 2700,
            datums = {"seat": datum(offset = [20, 20, 6])})
post = part(id = "post", body = box("post_body", 8, 8, 30), part_number = "BAL-POST",
            material = "Demo steel", density_kg_per_m3 = 7800,
            datums = {"foot": datum(offset = [4, 4, 0]), "pivot": datum(offset = [4, 4, 30])})
beam_body = box("beam_body", 120, 12, 6)
beam = part(id = "beam", body = beam_body, part_number = "BAL-BEAM",
            material = "Demo aluminum", density_kg_per_m3 = 2700,
            datums = {"pivot": datum(offset = [60, 6, 0]),
                      "payload": datum(offset = [110, 6, 6]),
                      "counterweight": datum(offset = [60 - counterweight_distance, 6, 6])})
weight = part(id = "counterweight", body = box("counterweight_body", 20, 16, 12), part_number = "BAL-WEIGHT",
              material = "Demo steel", density_kg_per_m3 = 7800,
              datums = {"foot": datum(offset = [10, 8, 0])})
payload = part(id = "payload", body = box("payload_body", 20, 20, 20), part_number = "BAL-PAYLOAD",
               material = "Demo aluminum", density_kg_per_m3 = 2700,
               datums = {"foot": datum(offset = [10, 10, 0])})
assembly_output(name = "balance_arm", assembly = assembly(id = "balance_arm", instances = [
    instance(id = "base", part = base, offset = [-20, -20, 0]),
    instance(id = "post", part = post, attach_to = "base", datum = "foot", target_datum = "seat"),
    instance(id = "beam", part = beam, attach_to = "post", datum = "pivot", target_datum = "pivot",
             rotation = [0, 0, 0.25881904510252074, 0.9659258262890683]),
    instance(id = "counterweight", part = weight, attach_to = "beam", datum = "foot", target_datum = "counterweight"),
    instance(id = "payload", part = payload, attach_to = "beam", datum = "foot", target_datum = "payload"),
]))
output(name = "beam", feature = beam_body)
