# Material identity is per engineering definition, independent of shared geometry.
steel_density = param(name = "steel_density", default = 7800)
def validate_parameters():
    if steel_density < 7800 or steel_density > 7900:
        fail("steel_density must be 7800..7900 kg/m3")
validate_parameters()
body = extrude(id = "body", profile = circle(id = "profile", radius = 3), height = 8)
steel = part(id = "steel", body = body, material = "Steel's α", density_kg_per_m3 = steel_density)
alloy = part(id = "alloy", body = body, material = "Aluminium β", density_kg_per_m3 = 2700)
plain = part(id = "plain", body = body)
weighted = assembly(id = "weighted", instances = [instance(id = "steel", part = steel), instance(id = "alloy", part = alloy, offset = [10,0,0])])
assembly_output(name = "weighted", assembly = weighted)
assembly_output(name = "step_materials", assembly = assembly(id = "step_materials", instances = [assembly_instance(id = "left", assembly = weighted), assembly_instance(id = "right", assembly = weighted, offset = [0,20,0]), instance(id = "plain_a", part = plain, offset = [20,0,0]), instance(id = "plain_b", part = plain, offset = [20,20,0])]))
output(name = "body", feature = body)
require_exchange(id = "materials", output = "step_materials", format = "step", aspect = "material_names")
require_exchange(id = "densities", output = "step_materials", format = "step", aspect = "material_density")
