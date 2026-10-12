# Two annular machined flanges locate bearings and three spacer rods around a
# common shaft. Fasteners are analytic envelopes: no threads, fits or loads are certified.
bolt_pitch = param(name = "bolt_pitch", default = 42)
seat_depth = param(name = "seat_depth", default = 6)
flange_height = param(name = "flange_height", default = 21)
def validate_parameters():
    if bolt_pitch < 40 or bolt_pitch > 44:
        fail("bolt_pitch must be 40..44 mm")
    if seat_depth < 4 or seat_depth > 8:
        fail("seat_depth must be 4..8 mm")
    if flange_height < 18 or flange_height > 24:
        fail("flange_height must be 18..24 mm")
validate_parameters()
p = bolt_pitch
h = seat_depth
d = flange_height
gap = 40
s = 0.8660254037844386
bolt_centers = [[p,0],[p/2,p*s],[-p/2,p*s],[-p,0],[-p/2,-p*s],[p/2,-p*s]]
rod_centers = [[30,0],[-15,30*s],[-15,-30*s]]
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
seat_tool = extrude(id = "seat_tool", profile = circle(id = "seat_profile", radius = 6), height = h+2)
finished = drill_pattern(piloted, "seat", seat_tool, bolt_centers, d-h)
output(name = "stock", feature = stock)
output(name = "sockets", feature = sockets)
output(name = "piloted", feature = piloted)
output(name = "flange", feature = finished)
assert_faces(id = "original_outer_wall", from_feature = stock, role = "wall", in_feature = finished, count = 2)
assert_faces(id = "original_center_bore", from_feature = stock, role = "inner_wall", in_feature = finished, count = 1)
assert_faces(id = "original_end_cap", from_feature = stock, role = "end_cap", in_feature = finished, count = 1)
def seat_checks():
    for prefix,count in [["socket",3],["seat",6]]:
        for i in range(count):
            assert_faces(id = prefix+"_floor_"+str(i), from_feature = prefix+"_cut_"+str(i), role = "bore_floor", in_feature = finished, count = 1)
seat_checks()
flange = part(id = "flange", body = finished, material = "Aluminum", density_kg_per_m3 = 2700, datums = {
    prefix+"_"+str(i):face_datum(from_feature = prefix+"_cut_"+str(i), role = "bore_floor", x_direction = [1,0,0]) for prefix,count in [["socket",3],["seat",6]] for i in range(count)
})
assembly_output(name = "flange_inspection", assembly = assembly(id = "flange_inspection", instances = [instance(id = "inspection_flange", part = flange)]))
def ring(id, outer, inner, height):
    return extrude(id = id, profile = circle(id = id+"_profile", radius = outer, inner_radius = inner), height = height)
washer = part(id = "washer", body = ring("washer_body",5.7,3.1,1), material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum(), "end":datum(offset = [0,0,1])})
length = d-h+7
bolt = part(id = "bolt", body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [[0,0],[3,0],[3,length],[5.3,length],[5.3,length+3],[0,length+3]])), material = "Steel", density_kg_per_m3 = 7800, datums = {"head_seat":datum(offset = [0,0,length])})
bearing = part(id = "bearing", body = ring("bearing_body",19.8,10,d-2), material = "Steel", density_kg_per_m3 = 7800)
rod = part(id = "rod", body = extrude(id = "rod_body", profile = circle(id = "rod_profile", radius = 2.8), height = gap+10), material = "Steel", density_kg_per_m3 = 7800, datums = {"base":datum()})
shaft = part(id = "shaft", body = extrude(id = "shaft_body", profile = circle(id = "shaft_profile", radius = 9.8), height = 2*d+gap+20), material = "Steel", density_kg_per_m3 = 7800)
def hardware():
    result = []
    for side in ["lower","upper"]:
        for i in range(6):
            washer_id = side+"_washer_"+str(i)
            result.append(instance(id = washer_id, part = washer, attach_to = side+"_flange", datum = "base", target_datum = "seat_"+str(i)))
            result.append(instance(id = side+"_bolt_"+str(i), part = bolt, attach_to = washer_id, datum = "head_seat", target_datum = "end"))
    for i in range(3):
        result.append(instance(id = "rod_"+str(i), part = rod, attach_to = "lower_flange", datum = "base", target_datum = "socket_"+str(i)))
    return result
assembly_output(name = "round_bearing_cartridge", assembly = assembly(id = "round_bearing_cartridge", instances = [
    instance(id = "lower_flange", part = flange),
    instance(id = "upper_flange", part = flange, offset = [0,0,2*d+gap], rotation = [1,0,0,0]),
    instance(id = "lower_bearing", part = bearing, offset = [0,0,1]),
    instance(id = "upper_bearing", part = bearing, offset = [0,0,d+gap+1]),
    instance(id = "shaft", part = shaft, offset = [0,0,-10]),
]+hardware()))
pi = 3.141592653589793
flange_v = pi*((3600-400)*d-6*(3.2*3.2*(d-h)+36*h)-3*9*5)
steel_v = pi*(12*(5.7*5.7-3.1*3.1)+12*(9*length+5.3*5.3*3)+2*(19.8*19.8-100)*(d-2)+3*2.8*2.8*(gap+10)+9.8*9.8*(2*d+gap+20))
assert_property(id = "flange_volume", output = "flange", property = "volume_mm3", min = flange_v-0.0001, max = flange_v+0.0001)
assert_property(id = "assembly_volume", output = "round_bearing_cartridge", property = "volume_mm3", min = 2*flange_v+steel_v-0.0001, max = 2*flange_v+steel_v+0.0001)
assert_property(id = "assembly_mass", output = "round_bearing_cartridge", property = "mass_kg", min = 2*flange_v*2700e-9+steel_v*7800e-9-0.000001, max = 2*flange_v*2700e-9+steel_v*7800e-9+0.000001)

# Engineering handoff contract: these semantics must survive STEP publication.
# This does not claim drawings, PMI, fits, loads, or downstream importer fidelity.
def require_handoff():
    for aspect in ["geometry", "units", "modeling_tolerance"]:
        require_exchange(id = "flange_step_" + aspect, output = "flange", format = "step", aspect = aspect)
    for aspect in ["geometry", "units", "assembly_definitions", "assembly_hierarchy", "occurrence_paths", "occurrence_placements"]:
        require_exchange(id = "cartridge_step_" + aspect, output = "round_bearing_cartridge", format = "step", aspect = aspect)

require_handoff()
