# Analytic circular holes and polygonal relief slots in one native tooling plate.
# Four nested bolt assemblies reuse stem/head definitions. Clearances are recipe
# intent; no interference or preload calculation is implied.
plate_thickness = param(name = "plate_thickness", default = 8)
bearing_bore = param(name = "bearing_bore", default = 12)
bolt_radius = param(name = "bolt_radius", default = 3)
def check_parameters():
    if plate_thickness < 4 or plate_thickness > 12 or bearing_bore < 10 or bearing_bore > 14 or bolt_radius < 2 or bolt_radius > 4:
        fail("plate_thickness must be 4..12, bearing_bore 10..14, bolt_radius 2..4 mm")
check_parameters()
bolt_positions = [[15,15],[85,15],[85,55],[15,55]]
round_holes = [{"center":[50,35],"radius":bearing_bore}] + [{"center":position,"radius":bolt_radius} for position in bolt_positions]
profile = polygon(id = "plate_profile", points = [[8,0],[92,0],[100,8],[100,62],[92,70],[8,70],[0,62],[0,8]],
    holes = [[[35,8],[35,12],[65,12],[65,8]],[[35,58],[35,62],[65,62],[65,58]]],
    circular_holes = round_holes)
plate_body = extrude(id = "plate_body", profile = profile, height = plate_thickness)
assert_faces(id = "bearing_seat", from_feature = plate_body, role = "circular_hole_0_wall", in_feature = plate_body)
plate = part(id = "plate", body = plate_body, material = "Demo aluminum", density_kg_per_m3 = 2700,
    datums = {"center":face_datum(from_feature = plate_body, role = "end_cap", x_direction = [1,0,0], offset = [0,0,-plate_thickness/2])})

def turned(id, radius, height, inner_radius = 0):
    return extrude(id = id, profile = circle(id = id+"/profile", radius = radius, inner_radius = inner_radius), height = height)
sleeve_body = turned("sleeve_body", bearing_bore, plate_thickness+8, bearing_bore-4)
sleeve = part(id = "sleeve", body = sleeve_body, material = "Demo bronze", density_kg_per_m3 = 8800,
    datums = {"center":datum(offset = [0,0,(plate_thickness+8)/2])})
shaft_body = turned("shaft_body", bearing_bore-5, plate_thickness+22)
shaft = part(id = "shaft", body = shaft_body, material = "Demo steel", density_kg_per_m3 = 7800,
    datums = {"center":datum(offset = [0,0,(plate_thickness+22)/2])})
stem = part(id = "bolt_stem", body = turned("stem_body", bolt_radius-0.5, plate_thickness+4), material = "Demo steel", density_kg_per_m3 = 7800)
head = part(id = "bolt_head", body = turned("head_body", bolt_radius+2, 3), material = "Demo steel", density_kg_per_m3 = 7800)
bolt = assembly(id = "bolt", instances = [instance(id = "stem", part = stem),instance(id = "head", part = head, offset = [0,0,plate_thickness+4])])
components = [instance(id = "plate", part = plate),
    instance(id = "sleeve", part = sleeve, attach_to = "plate", datum = "center", target_datum = "center"),
    instance(id = "shaft", part = shaft, attach_to = "plate", datum = "center", target_datum = "center")]
components += [assembly_instance(id = "bolt_"+str(i), assembly = bolt, offset = [bolt_positions[i][0],bolt_positions[i][1],-2]) for i in range(len(bolt_positions))]
assembly_output(name = "drilled_bearing_mount", assembly = assembly(id = "drilled_bearing_mount", instances = components))
output(name = "plate", feature = plate_body)
output(name = "sleeve", feature = sleeve_body)
output(name = "shaft", feature = shaft_body)
