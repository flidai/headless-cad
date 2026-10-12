# A source-library example: two rails and two drilled carriages.
# Placements are prescribed; this example does not certify bearings or motion.
load("recipe_modules/drive_parts.star", "rail_part", "carriage_part")
length = param(name = "length", default = 160)
setback = param(name = "setback", default = 2)
bore = param(name = "bore", default = 3)
def validate_parameters():
    if length < 140 or length > 180:
        fail("length must be 140..180 mm")
    if setback < 1 or setback > 3:
        fail("setback must be 1..3 mm")
    if bore < 2 or bore > 4:
        fail("bore must be 2..4 mm")
validate_parameters()
rail = rail_part("rail", length, setback)
carriage = carriage_part("carriage", bore)
stage = assembly(id = "stage", instances = [instance(id = "rail_left", part = rail), instance(id = "rail_right", part = rail, offset = [0,70,0]), instance(id = "carriage_front", part = carriage, offset = [20,0,20]), instance(id = "carriage_back", part = carriage, offset = [length-60,0,20])])
assembly_output(name = "modular_linear_stage", assembly = stage)
pi = 3.141592653589793
rail_volume = length*(400-setback*setback)
carriage_area = 3600-4*pi*bore*bore
mass = 2*rail_volume*7800/1000000000+2*carriage_area*8*2700/1000000000
assert_property(id = "material_mass", output = "modular_linear_stage", property = "mass_kg", min = mass-0.0000001, max = mass+0.0000001)
section_view(id = "carriages", output = "modular_linear_stage", origin = [0,0,24], min_area = 2*carriage_area-0.000001, max_area = 2*carriage_area+0.000001)
section_view(id = "rails", output = "modular_linear_stage", origin = [0,0,10], min_area = 40*length-0.000001, max_area = 40*length+0.000001)
