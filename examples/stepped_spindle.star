# Connected turned parts: the housing's two coaxial bore diameters are native
# cylindrical surfaces. Prescribed fits are design intent, not a tolerance analysis.
length = param(name = "length", default = 50)
flange_radius = param(name = "flange_radius", default = 15)
def validate_parameters():
    if length < 40 or length > 80 or flange_radius < 12 or flange_radius > 18:
        fail("length must be 40..80 and flange_radius 12..18 mm")
validate_parameters()
shaft_profile = polygon(id = "shaft_profile", points = [[0,0],[8,0],[8,20],[flange_radius,20],[flange_radius,26],[6,26],[6,length],[0,length]])
shaft_body = revolve(id = "shaft_body", profile = shaft_profile)
# A flanged housing with an 8 mm bore below the 12 mm counterbore.
housing_profile = polygon(id = "housing_profile", points = [[8,0],[25,0],[25,8],[20,8],[20,32],[12,32],[12,12],[8,12]])
housing_body = revolve(id = "housing_body", profile = housing_profile)
# A separate blind-bored cap demonstrates an axis contact and a bore floor.
cap_profile = polygon(id = "cap_profile", points = [[0,0],[20,0],[20,30],[8,30],[8,10],[0,10]])
cap_body = revolve(id = "cap_body", profile = cap_profile)
shaft = part(id = "shaft", body = shaft_body, material = "Demo steel", density_kg_per_m3 = 7800)
housing = part(id = "housing", body = housing_body, material = "Demo aluminum", density_kg_per_m3 = 2700)
cap = part(id = "blind_cap", body = cap_body, material = "Demo aluminum", density_kg_per_m3 = 2700)
# Exploded inspection layout keeps shoulder and bore geometry visible.
assembly_output(name = "stepped_spindle", assembly = assembly(id = "stepped_spindle", instances = [instance(id = "housing", part = housing), instance(id = "shaft", part = shaft, offset = [65,0,0]), instance(id = "blind_cap", part = cap, offset = [120,0,0])]))
output(name = "shaft", feature = shaft_body)
output(name = "housing", feature = housing_body)
output(name = "blind_cap", feature = cap_body)
