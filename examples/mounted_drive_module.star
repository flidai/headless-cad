# Whole rigid drive modules attach by a housing datum inside their nested
# cartridge. The external sensor follows the second attached module.
module_spacing = param(name = "module_spacing", default = 100)
def validate_parameters():
    if module_spacing < 80 or module_spacing > 120:
        fail("module_spacing must be 80..120 mm")
validate_parameters()

def turned(id, radius, height, inner_radius = 0):
    return extrude(id = id, profile = circle(id = id + "/profile", radius = radius, inner_radius = inner_radius), height = height)

housing_body = revolve(id = "housing_body", profile = polygon(id = "housing_profile", points = [[8,0],[22,0],[22,8],[18,8],[18,30],[12,30],[12,10],[8,10]]))
sleeve_body = turned("sleeve_body", 12, 20, 8)
shaft_body = turned("shaft_body", 7, 55)
retainer_body = turned("retainer_body", 22, 4, 12)
mount_body = extrude(id = "mount_body", profile = rectangle(id = "mount_profile", width = 12, depth = 10), height = 4)
sensor_body = turned("sensor_body", 3, 16)
base_blank = extrude(id = "base_blank", profile = rectangle(id = "base_profile", width = 190, depth = 80), height = 6)
# Fixed through-slots clear both shaft extensions across the allowed spacing.
# Their dimensions stay fixed so spacing edits only update placements.
first_slot = translate(id = "first_slot", body = extrude(id = "first_slot_blank", profile = rectangle(id = "first_slot_profile", width = 16, depth = 16), height = 8), offset = [27,32,-1])
second_slot = translate(id = "second_slot", body = extrude(id = "second_slot_blank", profile = rectangle(id = "second_slot_profile", width = 56, depth = 16), height = 8), offset = [107,32,-1])
base_first_cut = cut(id = "base_first_cut", left = base_blank, right = first_slot)
base_body = cut(id = "base_body", left = base_first_cut, right = second_slot)
housing = part(id = "housing", body = housing_body, part_number = "IC-HOUSING", material = "Aluminum", density_kg_per_m3 = 2700,
               datums = {"mount": datum(), "sensor_seat": datum(offset = [18,0,20], rotation = [0,0.7071067811865475,0,0.7071067811865476])})
sleeve = part(id = "sleeve", body = sleeve_body, part_number = "IC-BUSHING", material = "Bronze", density_kg_per_m3 = 8800)
shaft = part(id = "shaft", body = shaft_body, part_number = "IC-SHAFT", material = "Steel", density_kg_per_m3 = 7800)
retainer = part(id = "retainer", body = retainer_body, part_number = "IC-RETAINER", material = "Steel", density_kg_per_m3 = 7800)
mount = part(id = "sensor_mount", body = mount_body, part_number = "IC-SENSOR-MOUNT", material = "Aluminum", density_kg_per_m3 = 2700,
             datums = {"foot": datum(offset = [6,5,0]), "sensor": datum(offset = [6,5,4])})
sensor = part(id = "sensor", body = sensor_body, part_number = "IC-SENSOR", material = "Demo sensor envelope", density_kg_per_m3 = 3000,
              datums = {"base": datum()})
base = part(id = "base", body = base_body, part_number = "MD-BASE", material = "Aluminum", density_kg_per_m3 = 2700,
            datums = {"dock_a": datum(offset = [35,40,6]), "dock_b": datum(offset = [35+module_spacing,40,6], rotation = [0,0,0.7071067811865475,0.7071067811865476])})
cartridge = assembly(id = "cartridge", instances = [instance(id = "housing", part = housing), instance(id = "sleeve", part = sleeve, offset = [0,0,10]), instance(id = "shaft", part = shaft, offset = [0,0,-10]), instance(id = "retainer", part = retainer, offset = [0,0,30])])
module = assembly(id = "module", instances = [assembly_instance(id = "cartridge", assembly = cartridge, offset = [5,-3,2], rotation = [0.7071067811865475,0,0,0.7071067811865476])])
assembly_output(name = "mounted_drive_module", assembly = assembly(id = "mounted_drive_module", instances = [
    instance(id = "base", part = base, offset = [-35,-40,-6]),
    assembly_instance(id = "module_a", assembly = module, attach_to = "base", source_path = ["cartridge","housing"], datum = "mount", target_datum = "dock_a"),
    assembly_instance(id = "module_b", assembly = module, attach_to = "base", source_path = ["cartridge","housing"], datum = "mount", target_datum = "dock_b"),
    instance(id = "sensor_mount", part = mount, attach_to = "module_b", target_path = ["cartridge","housing"], datum = "foot", target_datum = "sensor_seat"),
    instance(id = "sensor", part = sensor, attach_to = "sensor_mount", datum = "base", target_datum = "sensor"),
]))
output(name = "housing", feature = housing_body)
output(name = "sensor_mount", feature = mount_body)

# Engineering release gates: retain the intended material envelope, stay below
# the 650 g mass budget, and keep the assembly balance inside the support span.
# Native properties are checked even when only an individual output is built.
assert_property(id = "material_envelope", output = "mounted_drive_module", property = "volume_mm3", min = 167000, max = 168000)
assert_property(id = "mass_budget", output = "mounted_drive_module", property = "mass_kg", max = 0.65)
assert_property(id = "balance_envelope", output = "mounted_drive_module", property = "mass_centroid_x_mm", min = 45, max = 61)
