# Countersunk annular bearing covers with twelve flush screw envelopes and three
# blind locating sockets per cover. No threads, fits, preload or loads are certified.
bolt_pitch = param(name = "bolt_pitch", default = 42)
seat_depth = param(name = "seat_depth", default = 3)
flange_height = param(name = "flange_height", default = 21)
def validate_parameters():
    if bolt_pitch < 40 or bolt_pitch > 44:
        fail("bolt_pitch must be 40..44 mm")
    if seat_depth < 2 or seat_depth > 4:
        fail("seat_depth must be 2..4 mm")
    if flange_height < 18 or flange_height > 24:
        fail("flange_height must be 18..24 mm")
validate_parameters()
p = bolt_pitch
h = seat_depth
d = flange_height
gap = 40
s = 0.8660254037844386
bolt_centers = [[p,0],[p/2,p*s],[-p/2,p*s],[-p,0],[-p/2,-p*s],[p/2,-p*s]]
rod_centers = [[28,0],[-14,28*s],[-14,-28*s]]
stock = extrude(id = "stock", profile = circle(id = "flange_profile", radius = 60, inner_radius = 20), height = d)
def drill_pattern(body, prefix, tool, centers, z):
    for i,xy in enumerate(centers):
        positioned = translate(id = prefix+"_tool_"+str(i), body = tool, offset = [xy[0],xy[1],z])
        body = cut(id = prefix+"_cut_"+str(i), left = body, right = positioned)
    return body
socket_tool = extrude(id = "socket_tool", profile = circle(id = "socket_profile", radius = 3), height = 7)
sockets = drill_pattern(stock, "socket", socket_tool, rod_centers, d-5)
pilot_tool = extrude(id = "pilot_tool", profile = circle(id = "pilot_profile", radius = 3.2), height = d+4)
piloted = drill_pattern(sockets, "pilot", pilot_tool, bolt_centers, -2)
seat_tool = revolve(id = "seat_tool", profile = polygon(id = "seat_profile", points = [[0,0],[3.2,0],[5.2+h,h+2],[0,h+2]]))
finished = drill_pattern(piloted, "seat", seat_tool, bolt_centers, d-h)
output(name = "stock", feature = stock)
output(name = "sockets", feature = sockets)
output(name = "piloted", feature = piloted)
output(name = "flange", feature = finished)
assert_faces(id = "original_outer_wall", from_feature = stock, role = "wall", in_feature = finished, count = 2)
assert_faces(id = "original_center_bore", from_feature = stock, role = "inner_wall", in_feature = finished, count = 1)
assert_faces(id = "original_end_cap", from_feature = stock, role = "end_cap", in_feature = finished, count = 1)
def seat_checks():
    for prefix,count in [["socket",3]]:
        for i in range(count):
            assert_faces(id = prefix+"_floor_"+str(i), from_feature = prefix+"_cut_"+str(i), role = "bore_floor", in_feature = finished, count = 1)
seat_checks()
def cover_datums():
    result = {"socket_"+str(i):face_datum(from_feature = "socket_cut_"+str(i), role = "bore_floor", x_direction = [1,0,0]) for i in range(3)}
    for i,xy in enumerate(bolt_centers):
        result["seat_"+str(i)] = datum(offset = [xy[0],xy[1],d])
    return result
flange = part(id = "flange", part_number = "CBC-COVER", revision = "A", description = "Countersunk annular bearing cover", body = finished, material = "Aluminum", density_kg_per_m3 = 2700, datums = cover_datums())
assembly_output(name = "flange_inspection", assembly = assembly(id = "flange_inspection", instances = [instance(id = "inspection_flange", part = flange)]))
def ring(id, outer, inner, height):
    return extrude(id = id, profile = circle(id = id+"_profile", radius = outer, inner_radius = inner), height = height)
length = d-h+7
bolt = part(id = "bolt", part_number = "CBC-SCREW", revision = "A", description = "Flush conical-head screw envelope", body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [[0,0],[3,0],[3,length],[3+h,length+h],[0,length+h]])), material = "Steel", density_kg_per_m3 = 7800, datums = {"head_top":datum(offset = [0,0,length+h])})
bearing = part(id = "bearing", part_number = "CBC-BEARING", revision = "A", description = "Bearing envelope", body = ring("bearing_body",19.8,10,d-2), material = "Steel", density_kg_per_m3 = 7800)
rod = part(id = "rod", part_number = "CBC-ROD", revision = "A", description = "Socket spacer rod", body = extrude(id = "rod_body", profile = circle(id = "rod_profile", radius = 2.8), height = gap+10), material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum()})
shaft = part(id = "shaft", part_number = "CBC-SHAFT", revision = "A", description = "Central shaft", body = extrude(id = "shaft_body", profile = circle(id = "shaft_profile", radius = 9.8), height = 2*d+gap+20), material = "Steel", density_kg_per_m3 = 7800)
def hardware():
    result = []
    for side in ["lower","upper"]:
        for i in range(6):
            result.append(instance(id = side+"_bolt_"+str(i), part = bolt, attach_to = side+"_flange", datum = "head_top", target_datum = "seat_"+str(i)))
    for i in range(3):
        result.append(instance(id = "rod_"+str(i), part = rod, attach_to = "lower_flange", datum = "base", target_datum = "socket_"+str(i)))
    return result
assembly_output(name = "countersunk_bearing_cover", assembly = assembly(id = "countersunk_bearing_cover", instances = [
    instance(id = "lower_flange", part = flange),
    instance(id = "upper_flange", part = flange, offset = [0,0,2*d+gap], rotation = [1,0,0,0]),
    instance(id = "lower_bearing", part = bearing, offset = [0,0,1]),
    instance(id = "upper_bearing", part = bearing, offset = [0,0,d+gap+1]),
    instance(id = "shaft", part = shaft, offset = [0,0,-10]),
]+hardware()))
pi = 3.141592653589793
flange_v = pi*((3600-400)*d-6*(3.2*3.2*(d-h)+h*(3.2*3.2+3.2*(3.2+h)+(3.2+h)*(3.2+h))/3)-3*9*5)
steel_v = pi*(12*(9*length+h*(9+3*(3+h)+(3+h)*(3+h))/3)+2*(19.8*19.8-100)*(d-2)+3*2.8*2.8*(gap+10)+9.8*9.8*(2*d+gap+20))
assert_property(id = "flange_volume", output = "flange", property = "volume_mm3", min = flange_v-0.0001, max = flange_v+0.0001)
assert_property(id = "assembly_volume", output = "countersunk_bearing_cover", property = "volume_mm3", min = 2*flange_v+steel_v-0.0001, max = 2*flange_v+steel_v+0.0001)
assert_property(id = "assembly_mass", output = "countersunk_bearing_cover", property = "mass_kg", min = 2*flange_v*2700e-9+steel_v*7800e-9-0.000001, max = 2*flange_v*2700e-9+steel_v*7800e-9+0.000001)

# Engineering handoff contract: these semantics must survive STEP publication.
# This does not claim drawings, PMI, fits, loads, or downstream importer fidelity.
def require_handoff():
    for aspect in ["geometry", "units", "modeling_tolerance"]:
        require_exchange(id = "flange_step_" + aspect, output = "flange", format = "step", aspect = aspect)
    for aspect in ["geometry", "units", "assembly_definitions", "assembly_hierarchy", "occurrence_paths", "occurrence_placements", "part_numbers", "part_revisions", "part_descriptions", "material_names", "material_density"]:
        require_exchange(id = "cartridge_step_" + aspect, output = "countersunk_bearing_cover", format = "step", aspect = aspect)

require_handoff()

# Source-owned inspection sections follow depth edits. Each area is derived
# independently from circular stock, bores and hardware envelopes; overlapping
# assembly occurrences are summed, not fused. No fits or GD&T are certified.
pilot_area = pi*(3600-400-6*3.2*3.2)
socket_area = pilot_area-3*pi*9
seat_radius = 3.2+h/2
seat_area = pi*(3600-400-6*seat_radius*seat_radius-3*9)
assembly_bore_radius = 3.2+h-1.5
assembly_screw_radius = 3+h-1.5
assembly_area = pi*(3600-400-6*assembly_bore_radius*assembly_bore_radius-3*9+19.8*19.8-100+9.8*9.8+3*2.8*2.8+6*assembly_screw_radius*assembly_screw_radius)
section_view(id = "pilot", output = "flange", origin = [0,0,d-6], min_area = pilot_area-0.000001, max_area = pilot_area+0.000001)
section_view(id = "socket", output = "flange", origin = [0,0,d-(5+h)/2], min_area = socket_area-0.000001, max_area = socket_area+0.000001)
section_view(id = "seat", output = "flange", origin = [0,0,d-h/2], min_area = seat_area-0.000001, max_area = seat_area+0.000001)
section_view(id = "assembly", output = "countersunk_bearing_cover", origin = [0,0,d-1.5], min_area = assembly_area-0.000001, max_area = assembly_area+0.000001)
