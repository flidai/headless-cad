# A plate follows the sloping face of a parametric ramp.
# The seat is derived from face lineage, area centroid and an explicit X hint.
# Changing ramp_rise updates the mounting position AND orientation.
rise = param(name = "ramp_rise", default = 20)

def validate_parameters():
    if rise < 12 or rise > 28:
        fail("ramp_rise must be 12..28 mm")
validate_parameters()

def box(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id + "/profile", width = width, depth = depth), height = height)

profile = polygon(id = "ramp_profile", points = [[0, 0], [40, 0], [0, rise]])
prism = extrude(id = "ramp_prism", profile = profile, height = 30)
ramp_body = transform(id = "ramp_body", body = prism, offset = [0, 30, 0],
                      rotation = [0.7071067811865476, 0, 0, 0.7071067811865476])
ramp = part(id = "ramp", body = ramp_body, part_number = "SURFACE-RAMP",
            datums = {"seat": face_datum(from_feature = prism, role = "wall_1", x_direction = [1, 0, 0])})
base = part(id = "base", body = box("base_body", 60, 50, 4), part_number = "SURFACE-BASE")
plate = part(id = "plate", body = box("plate_body", 12, 20, 2), part_number = "SURFACE-PLATE",
             datums = {"foot": datum(offset = [6, 10, 0])})
assembly_output(name = "surface_mount", assembly = assembly(id = "surface_mount", instances = [
    instance(id = "base", part = base),
    instance(id = "ramp", part = ramp, offset = [10, 10, 4]),
    instance(id = "plate", part = plate, attach_to = "ramp", datum = "foot", target_datum = "seat"),
]))
output(name = "ramp", feature = ramp_body)
