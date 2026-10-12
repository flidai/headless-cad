# Repeated rigid spindle stations exercise shared analytic geometry and display LOD.
# Native volume/mass checks express material totals, not collision or bearing certification.
stations_value = param(name = "stations", default = 12)
columns_value = param(name = "columns", default = 4)
variants_value = param(name = "variants", default = 3)
spacing = param(name = "spacing", default = 100)
stations = int(stations_value)
columns = int(columns_value)
variants = int(variants_value)
def validate_parameters():
    if stations != stations_value or columns != columns_value or variants != variants_value:
        fail("stations, columns and variants must be integers")
    if stations < 1 or stations > 1000 or columns < 1 or columns > stations or variants < 1 or variants > 12 or variants > stations:
        fail("stations: 1..1000, columns: 1..stations, variants: 1..min(12,stations)")
    if spacing < 80 or spacing > 250:
        fail("spacing must be 80..250 mm")
validate_parameters()
base_body = extrude(id = "base_body", profile = polygon(id = "base_profile", points = [[-35,-35],[35,-35],[35,35],[-35,35]], circular_holes = [{"center":[0,0],"radius":9}]), height = 6)
base = part(id = "base", body = base_body, part_number = "SL-BASE", material = "Aluminum", density_kg_per_m3 = 2700, datums = {"seat": datum(offset = [0,0,6])})
shaft_body = revolve(id = "shaft_body", profile = polygon(id = "shaft_profile", points = [[0,0],[7,0],[7,48],[5,48],[5,60],[0,60]]))
shaft = part(id = "shaft", body = shaft_body, part_number = "SL-SHAFT", material = "Steel", density_kg_per_m3 = 7800)
retainer_body = extrude(id = "retainer_body", profile = circle(id = "retainer_profile", radius = 14, inner_radius = 9), height = 3)
retainer = part(id = "retainer", body = retainer_body, part_number = "SL-RETAINER", material = "Steel", density_kg_per_m3 = 7800)
def station_definitions():
    result = []
    for variant in range(variants):
        tag = str(variant)
        radius = 22 + variant * 0.5
        neck = 14 + variant * 0.5
        body = revolve(id = "housing_body_"+tag, profile = polygon(id = "housing_profile_"+tag, points = [[9,0],[radius,0],[radius,8],[neck,20],[neck,36],[9,36]]))
        housing = part(id = "housing_"+tag, body = body, part_number = "SL-HOUSING-"+tag, material = "Aluminum", density_kg_per_m3 = 2700, datums = {"foot": datum(), "retainer": datum(offset = [0,0,36])})
        result.append(assembly(id = "station_"+tag, instances = [
            instance(id = "base_"+tag, part = base),
            instance(id = "housing_"+tag, part = housing, attach_to = "base_"+tag, datum = "foot", target_datum = "seat"),
            instance(id = "shaft_"+tag, part = shaft),
            instance(id = "retainer_"+tag, part = retainer, offset = [0,0,42]),
        ]))
    return result
station_types = station_definitions()
line = assembly(id = "spindle_line", instances = [assembly_instance(id = "station_"+str(i), assembly = station_types[i % variants], offset = [(i % columns)*spacing,(i // columns)*spacing,0]) for i in range(stations)])
assembly_output(name = "spindle_line", assembly = line)
assembly_output(name = "service_station", assembly = station_types[0])
def expected_volume():
    total = 0
    for i in range(stations):
        variant = i % variants
        total += 29400 + 3.141592653589793 * (10555 + 616*variant + 9*variant*variant)
    return total
volume = expected_volume()
assert_property(id = "material_total", output = "spindle_line", property = "volume_mm3", min = volume - 0.0001, max = volume + 0.0001)
