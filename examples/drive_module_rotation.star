# Source-owned rigid rotation of the retained rotor. Continuous certificates
# cover only named clearance pairs; sampled inspector animation is illustrative.
# Preliminary bearing-supported drive layout: source, engineering evidence and
# section-sheet release together. Shaft and disc have real mating key features.
# An integral shoulder and three open retaining-ring/groove stacks retain the
# shaft and keyed hub nominally. Two removable end covers bear on the bronze
# sleeve flanges through nominal unthreaded screw envelopes. Ring spring
# installation, threads, preload, material tolerances and rated loads remain
# engineering work.
design_revision = "A"
turn_radians = param(name = "turn_radians", default = 6.283185307179586)
shaft_diameter = param(name = "shaft_diameter", default = 12)
mounting_pitch = param(name = "mounting_pitch", default = 60)
base_corner_radius = param(name = "base_corner_radius", default = 6)
required_clearance = param(name = "required_clearance", default = 0.09)
minimum_endplay = param(name = "minimum_endplay", default = 0.1)

def validate_parameters():
    if shaft_diameter < 10 or shaft_diameter > 16:
        fail("shaft_diameter must be 10..16 mm")
    if mounting_pitch < 60 or mounting_pitch > 90:
        fail("mounting_pitch must be 60..90 mm")
    if base_corner_radius < 2 or base_corner_radius > 10:
        fail("base_corner_radius must be 2..10 mm")
    if minimum_endplay <= 0:
        fail("minimum_endplay must be positive")
    if required_clearance <= 0:
        fail("required_clearance must be positive")
validate_parameters()

r = shaft_diameter / 2
p = mounting_pitch
pi = 3.141592653589793
# Cyclic frame: local X -> world Y, local Y -> world Z, local Z -> world X.
shaft_frame = [0.5,0.5,0.5,0.5]
bolt_centers = [[x,y] for x in [33,p+33] for y in [-28,28]]
base_stock = extrude(id = "base_stock", profile = polygon(id = "base_profile",
    points = [[0,-40],[p+90,-40],[p+90,40],[0,40]],
    circular_holes = [{"center":xy,"radius":3} for xy in bolt_centers]), height = 6)
# Exact tangent edge fillets preserve the four real anchor holes.
base_front_left = fillet(id = "base_front_left", body = base_stock, faces = [{"from_feature":base_stock,"role":"wall_3"},{"from_feature":base_stock,"role":"wall_0"}], radius = base_corner_radius)
base_front_right = fillet(id = "base_front_right", body = base_front_left, faces = [{"from_feature":base_stock,"role":"wall_0"},{"from_feature":base_stock,"role":"wall_1"}], radius = base_corner_radius)
base_back_right = fillet(id = "base_back_right", body = base_front_right, faces = [{"from_feature":base_stock,"role":"wall_1"},{"from_feature":base_stock,"role":"wall_2"}], radius = base_corner_radius)
base_body = fillet(id = "base_body", body = base_back_right, faces = [{"from_feature":base_stock,"role":"wall_2"},{"from_feature":base_stock,"role":"wall_3"}], radius = base_corner_radius)
housing_body = extrude(id = "housing_body", profile = polygon(id = "housing_profile",
    points = [[-22,-26],[22,-26],[22,14],[14,22],[-14,22],[-22,14]],
    circular_holes = [{"center":[0,0],"radius":r+4.025}]), height = 16)
# Opposite outboard faces receive the same two solved pilot locations. Two
# handed housing parts avoid machining unused holes into the inboard faces.
# The cap is asymmetric in Y; its area centroid moves with the bearing bore.
# Offsetting the face-local sketch origin puts both pilots on the shaft axis Y=0.
pilot_radius = 1.75
pilot_depth = 5
pilot_half_pitch = 17
cap_centroid_y = -16384.0/(3*(2048-pi*(r+4.025)*(r+4.025)))
def end_cover_pilots(feature_id, cap_role, offset_y):
    points = [
        {"id":"minus_center","position":[-pilot_half_pitch,0]},
        {"id":"plus_center","position":[pilot_half_pitch,0]},
        {"id":"minus_rim","position":[-pilot_half_pitch+pilot_radius,0]},
        {"id":"plus_rim","position":[pilot_half_pitch+pilot_radius,0]},
    ]
    constraints = [
        {"id":"minus_fixed","kind":"fixed","point":"minus_center","position":[-pilot_half_pitch,0]},
        {"id":"pitch_axis","kind":"horizontal","a":"minus_center","b":"plus_center"},
        {"id":"pitch","kind":"distance","a":"minus_center","b":"plus_center","value":2*pilot_half_pitch},
        {"id":"minus_radius_axis","kind":"horizontal","a":"minus_center","b":"minus_rim"},
        {"id":"minus_radius","kind":"distance","a":"minus_center","b":"minus_rim","value":pilot_radius},
        {"id":"plus_radius_axis","kind":"horizontal","a":"plus_center","b":"plus_rim"},
        {"id":"plus_radius","kind":"distance","a":"plus_center","b":"plus_rim","value":pilot_radius},
    ]
    return hole_pattern(id = feature_id, body = housing_body,
        face = {"from_feature":housing_body,"role":cap_role}, x_direction_hint = [1,0,0],
        origin_offset = [0,offset_y], points = points, constraints = constraints,
        holes = [
            {"id":"cover_minus","center":"minus_center","rim":"minus_rim","depth":pilot_depth},
            {"id":"cover_plus","center":"plus_center","rim":"plus_rim","depth":pilot_depth},
        ])
housing_left_pilots = end_cover_pilots("housing_left_pilots", "start_cap", cap_centroid_y)
housing_right_pilots = end_cover_pilots("housing_right_pilots", "end_cap", -cap_centroid_y)
# A 2 mm removable plate bears on the outer 1.5 mm of the bronze flange.
# Its aperture clears the sleeve barrel and the rotating shoulder/washers.
cover_body = extrude(id = "end_cover_body", profile = polygon(id = "end_cover_profile",
    points = [[-22,-26],[22,-26],[22,14],[14,22],[-14,22],[-22,14]],
    circular_holes = [{"center":[0,0],"radius":r+4.5}]
        + [{"center":[x,0],"radius":2.05} for x in [-pilot_half_pitch,pilot_half_pitch]]), height = 2)
# Nominal unthreaded screw envelopes: 3.3 mm shank, 6 mm head. This models
# engagement and head seating, not threads, preload, torque or rated retention.
cover_screw_body = revolve(id = "cover_screw_body", profile = polygon(id = "cover_screw_profile",
    points = [[0,0],[1.65,0],[1.65,9],[3,9],[3,11],[0,11]]))
sleeve_body = revolve(id = "sleeve_body", profile = polygon(id = "sleeve_profile", points = [
    [r+0.15,0],[r+6,0],[r+6,2],[r+4,2],[r+4,18],[r+0.15,18],
]))
# Axial coordinates are shaft-local Z; world X is local Z + 10.
shaft_stock = revolve(id = "shaft_stock", profile = polygon(id = "shaft_profile", points = [
    [0,0],[r+2,0],[r+2,12.9],[r,12.9],
    [r,12.2+p/2],[r-0.6,12.2+p/2],[r-0.6,13.45+p/2],[r,13.45+p/2],
    [r,30.55+p/2],[r-0.6,30.55+p/2],[r-0.6,31.8+p/2],[r,31.8+p/2],
    [r,p+34.55],[r-0.6,p+34.55],[r-0.6,p+35.8],[r,p+35.8],
    [r,p+70],[0,p+70],
]))
disc_stock = extrude(id = "disc_stock", profile = circle(id = "disc_profile", radius = 24, inner_radius = r+0.15), height = 14)
seat_box = extrude(id = "seat_box", profile = rectangle(id = "seat_profile", width = 4, depth = 4.04), height = 13.5)
seat_tool = translate(id = "seat_tool", body = seat_box, offset = [r-2,-2.02,15.25+p/2])
shaft_body = cut(id = "shaft_body", left = shaft_stock, right = seat_tool)
keyway_box = extrude(id = "keyway_box", profile = rectangle(id = "keyway_profile", width = r+2.05, depth = 4.04), height = 16)
keyway_tool = translate(id = "keyway_tool", body = keyway_box, offset = [0,-2.02,-1])
disc_body = cut(id = "disc_body", left = disc_stock, right = keyway_tool)
key_body = extrude(id = "key_body", profile = rectangle(id = "key_profile", width = 4, depth = 4), height = 13)
washer_body = extrude(id = "washer_body", profile = circle(id = "washer_profile", radius = r+2, inner_radius = r+0.1), height = 1.5)
# The installed ring has a true 60-degree opening; it is not a full annulus proxy.
ri = r-0.5
ro = r+1.5
cos30 = sqrt(3)/2
ring_body = extrude(id = "ring_body", profile = path(id = "ring_profile", start = [ro*cos30,ro/2], segments = [
    {"kind":"arc_to","center":[0,0],"end":[-ro,0],"clockwise":False},
    {"kind":"arc_to","center":[0,0],"end":[ro*cos30,-ro/2],"clockwise":False},
    {"kind":"line_to","end":[ri*cos30,-ri/2]},
    {"kind":"arc_to","center":[0,0],"end":[-ri,0],"clockwise":True},
    {"kind":"arc_to","center":[0,0],"end":[ri*cos30,ri/2],"clockwise":True},
    {"kind":"line_to","end":[ro*cos30,ro/2]},
]), height = 1.2)
bolt_body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [[0,0],[2.5,0],[2.5,65],[4.5,65],[4.5,68],[0,68]]))

clamp_body = extrude(id = "clamp_body", profile = polygon(id = "clamp_profile", points = [[0,-34],[12,-34],[12,34],[0,34]], circular_holes = [{"center":[6,y],"radius":3} for y in [-28,28]]), height = 6)
nut_body = extrude(id = "nut_body", profile = circle(id = "nut_profile", radius = 5, inner_radius = 2.6), height = 5)
clamp = part(id = "clamp_bar", body = clamp_body, part_number = "DM-CLAMP", revision = design_revision, description = "Bridge clamp seated on bearing housing", material = "Aluminum", density_kg_per_m3 = 2700)
nut = part(id = "nut_envelope", body = nut_body, part_number = "DM-NUT-ENVELOPE", revision = design_revision, description = "Underside clamp nut envelope; no thread or preload model", material = "Steel", density_kg_per_m3 = 7800)
base = part(id = "base", body = base_body, part_number = "DM-BASE", revision = design_revision, description = "Four-hole mounting plate with native tangent corner fillets", material = "Aluminum", density_kg_per_m3 = 2700)
housing = part(id = "housing_left", body = housing_left_pilots, part_number = "DM-HOUSING-L", revision = design_revision, description = "Left bearing housing with two outboard blind end-cover pilots", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"axis":datum(), "seat":datum(offset = [0,-26,0])})
right_housing = part(id = "housing_right", body = housing_right_pilots, part_number = "DM-HOUSING-R", revision = design_revision, description = "Right bearing housing with two outboard blind end-cover pilots", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"axis":datum(), "seat":datum(offset = [0,-26,0])})
end_cover = part(id = "end_cover", body = cover_body, part_number = "DM-END-COVER", revision = design_revision, description = "Removable sleeve-flange retainer plate with two screw clearances", material = "Aluminum", density_kg_per_m3 = 2700)
cover_screw = part(id = "cover_screw_envelope", body = cover_screw_body, part_number = "DM-COVER-SCREW-ENVELOPE", revision = design_revision, description = "Nominal unthreaded end-cover screw envelope; no preload or thread model", material = "Steel", density_kg_per_m3 = 7800)
sleeve = part(id = "sleeve", body = sleeve_body, part_number = "DM-SLEEVE", revision = design_revision, description = "Flanged bronze bush seated against the outer housing face", material = "Bronze", density_kg_per_m3 = 8800, datums = {
    "outer_thrust":face_datum(from_feature = sleeve_body, role = "segment_0", x_direction = [1,0,0]),
})
shaft = part(id = "shaft", body = shaft_body, part_number = "DM-SHAFT", revision = design_revision, description = "Finite keyed shaft with integral shoulder and three retaining grooves", material = "Steel", density_kg_per_m3 = 7800, datums = {
    "shoulder":face_datum(from_feature = shaft_stock, role = "segment_2", x_direction = [1,0,0]),
    "hub_left_stop":face_datum(from_feature = shaft_stock, role = "segment_4", x_direction = [1,0,0]),
    "hub_right_stop":face_datum(from_feature = shaft_stock, role = "segment_10", x_direction = [1,0,0]),
    "loaded_groove_wall":face_datum(from_feature = shaft_stock, role = "segment_14", x_direction = [1,0,0]),
})
disc = part(id = "drive_disc", body = disc_body, part_number = "DM-KEYED-DISC", revision = design_revision, description = "Keyed hub retained between washer and open-ring stacks; no rated load evidence", material = "Steel", density_kg_per_m3 = 7800, datums = {
    "start":face_datum(from_feature = disc_body, role = "start_cap", x_direction = [1,0,0]),
    "end":face_datum(from_feature = disc_body, role = "end_cap", x_direction = [1,0,0]),
})
key = part(id = "parallel_key", body = key_body, part_number = "DM-PARALLEL-KEY", revision = design_revision, description = "4 x 4 x 13 mm key; nominal engagement, load capacity unverified", material = "Steel", density_kg_per_m3 = 7800)
washer = part(id = "washer", body = washer_body, part_number = "DM-THRUST-WASHER", revision = design_revision, material = "Steel", density_kg_per_m3 = 7800, datums = {
    "start":face_datum(from_feature = washer_body, role = "start_cap", x_direction = [1,0,0]),
    "end":face_datum(from_feature = washer_body, role = "end_cap", x_direction = [1,0,0]),
})
ring = part(id = "retaining_ring", body = ring_body, part_number = "DM-RING-INSTALLED", revision = design_revision, description = "Open installed-state ring; spring installation and capacity unverified", material = "Spring steel", density_kg_per_m3 = 7800, datums = {
    "start":face_datum(from_feature = ring_body, role = "start_cap", x_direction = [1,0,0]),
    "end":face_datum(from_feature = ring_body, role = "end_cap", x_direction = [1,0,0]),
})
bolt = part(id = "clamp_bolt_envelope", body = bolt_body, part_number = "DM-CLAMP-BOLT-ENVELOPE", revision = design_revision, description = "Through-clamp head and shank envelope; no thread or preload model", material = "Steel", density_kg_per_m3 = 7800)
left_support = assembly(id = "left_support_definition", instances = [
    instance(id = "housing", part = housing),
    instance(id = "sleeve", part = sleeve, offset = [0,0,-2]),
    instance(id = "left_cover", part = end_cover, offset = [0,0,-4]),
] + [instance(id = "left_cover_screw_"+tag, part = cover_screw, offset = [x,0,5], rotation = [1,0,0,0]) for tag,x in [("minus",-pilot_half_pitch),("plus",pilot_half_pitch)]])
right_support = assembly(id = "right_support_definition", instances = [
    instance(id = "right_housing", part = right_housing),
    instance(id = "right_sleeve", part = sleeve, offset = [0,0,18], rotation = [1,0,0,0]),
    instance(id = "right_cover", part = end_cover, offset = [0,0,18]),
] + [instance(id = "right_cover_screw_"+tag, part = cover_screw, offset = [x,0,11]) for tag,x in [("minus",-pilot_half_pitch),("plus",pilot_half_pitch)]])
module = assembly(id = "drive_module_release", instances = [
    instance(id = "base", part = base),
    instance(id = "left_clamp", part = clamp, offset = [27,0,54]),
    instance(id = "right_clamp", part = clamp, offset = [p+27,0,54]),
    assembly_instance(id = "left_support", assembly = left_support, offset = [25,0,32], rotation = shaft_frame),
    assembly_instance(id = "right_support", assembly = right_support, offset = [25+p,0,32], rotation = shaft_frame),
    instance(id = "shaft", part = shaft, offset = [10,0,32], rotation = shaft_frame),
    instance(id = "drive_disc", part = disc, offset = [25+p/2,0,32], rotation = shaft_frame),
    instance(id = "parallel_key", part = key, offset = [25+p/2+0.5,r-2,30], rotation = shaft_frame),
    instance(id = "hub_left_washer", part = washer, offset = [25+p/2-1.6,0,32], rotation = shaft_frame),
    instance(id = "hub_left_ring", part = ring, offset = [25+p/2-2.8,0,32], rotation = shaft_frame),
    instance(id = "hub_right_washer", part = washer, offset = [25+p/2+14.1,0,32], rotation = shaft_frame),
    instance(id = "hub_right_ring", part = ring, offset = [25+p/2+15.6,0,32], rotation = shaft_frame),
    instance(id = "shaft_washer", part = washer, offset = [p+43.1,0,32], rotation = shaft_frame),
    instance(id = "shaft_ring", part = ring, offset = [p+44.6,0,32], rotation = shaft_frame),
] + [instance(id = "clamp_bolt_"+str(i), part = bolt, offset = [xy[0],xy[1],-5]) for i,xy in enumerate(bolt_centers)]
  + [instance(id = "clamp_nut_"+str(i), part = nut, offset = [xy[0],xy[1],-5]) for i,xy in enumerate(bolt_centers)])
assembly_output(name = "drive_module_release", assembly = module)
# A focused section target excludes perpendicular clamp fasteners, whose oblique
# circular sections are outside the current native section-query family.
fit_inspection = assembly(id = "fit_inspection", instances = [
    assembly_instance(id = "inspect_support", assembly = left_support, offset = [25,0,32], rotation = shaft_frame),
    instance(id = "inspect_shaft", part = shaft, offset = [10,0,32], rotation = shaft_frame),
])
assembly_output(name = "bearing_fit_inspection", assembly = fit_inspection)
output(name = "base", feature = base_body)
output(name = "housing", feature = housing_left_pilots)
output(name = "right_housing", feature = housing_right_pilots)
output(name = "end_cover", feature = cover_body)
output(name = "cover_screw_envelope", feature = cover_screw_body)
output(name = "sleeve", feature = sleeve_body)
output(name = "shaft", feature = shaft_body)
output(name = "drive_disc", feature = disc_body)
output(name = "parallel_key", feature = key_body)
output(name = "washer", feature = washer_body)
output(name = "retaining_ring", feature = ring_body)

# Cap descendants and drilled walls/floors must remain uniquely selectable.
# The local pilot sections inspect actual removed material, not preview meshes.
def cover_engineering_checks():
    for side, body, cap in [("left",housing_left_pilots,"start_cap"),("right",housing_right_pilots,"end_cap")]:
        assert_faces(id = side+"_housing_bore_survives", from_feature = housing_body, role = "circular_hole_0_wall", in_feature = body, count = 1)
        assert_faces(id = side+"_outboard_cap_survives", from_feature = housing_body, role = cap, in_feature = body, count = 1)
        for tag in ["minus","plus"]:
            assert_faces(id = side+"_cover_"+tag+"_wall", from_feature = body, role = "hole_cover_"+tag+"_wall", in_feature = body, count = 1)
            assert_faces(id = side+"_cover_"+tag+"_floor", from_feature = body, role = "hole_cover_"+tag+"_floor", in_feature = body, count = 1)
    pilot_section_area = 2048-pi*(r+4.025)*(r+4.025)-2*pi*pilot_radius*pilot_radius
    for side, name, body, section_z in [("left","housing",housing_left_pilots,pilot_depth/2),("right","right_housing",housing_right_pilots,16-pilot_depth/2)]:
        view_id = side+"_cover_pilot_section"
        section_view(id = view_id, output = name, origin = [0,0,section_z], min_area = pilot_section_area-0.00001, max_area = pilot_section_area+0.00001)
        for tag, label_x in [("minus",-12),("plus",12)]:
            section_circle_dimension(view = view_id, id = side+"_cover_"+tag+"_diameter", from_feature = body, role = "hole_cover_"+tag+"_wall", kind = "diameter", label_offset = [label_x,12], min = 2*pilot_radius-0.000001, max = 2*pilot_radius+0.000001)
    cover_section_area = 2048-pi*(r+4.5)*(r+4.5)-2*pi*2.05*2.05
    section_view(id = "end_cover_section", output = "end_cover", origin = [0,0,1], min_area = cover_section_area-0.00001, max_area = cover_section_area+0.00001)
    section_circle_dimension(view = "end_cover_section", id = "cover_sleeve_aperture", from_feature = cover_body, role = "circular_hole_0_wall", kind = "diameter", label_offset = [0,18], min = 2*(r+4.5)-0.000001, max = 2*(r+4.5)+0.000001)
    for tag, index, label_x in [("minus",1,-12),("plus",2,12)]:
        section_circle_dimension(view = "end_cover_section", id = "cover_"+tag+"_clearance", from_feature = cover_body, role = "circular_hole_"+str(index)+"_wall", kind = "diameter", label_offset = [label_x,-12], min = 4.1-0.000001, max = 4.1+0.000001)
cover_engineering_checks()

# Independent material-volume formulas include actual holes, not bounding boxes.
base_volume = (80*(p+90)-4*pi*3*3-(4-pi)*base_corner_radius*base_corner_radius)*6
housing_volume = (44*48-64-pi*(r+4.025)*(r+4.025))*16-2*pi*pilot_radius*pilot_radius*pilot_depth
cover_volume = (2048-pi*(r+4.5)*(r+4.5)-2*pi*2.05*2.05)*2
cover_screw_volume = pi*(1.65*1.65*9+3*3*2)
sleeve_volume = pi*((r+6)*(r+6)-(r+0.15)*(r+0.15))*2+pi*((r+4)*(r+4)-(r+0.15)*(r+0.15))*16
half_width = 2.02
bore = r+0.15
seat_area = half_width*sqrt(r*r-half_width*half_width)+r*r*asin(half_width/r)-2*(r-2)*half_width
keyway_area = 2*(r+2.05)*half_width-half_width*sqrt(bore*bore-half_width*half_width)-bore*bore*asin(half_width/bore)
shaft_volume = pi*((r+2)*(r+2)*12.9+r*r*(p+70-12.9)-3*1.25*(r*r-(r-0.6)*(r-0.6)))-seat_area*13.5
disc_volume = (pi*(24*24-bore*bore)-keyway_area)*14
key_volume = 4*4*13
washer_volume = pi*((r+2)*(r+2)-(r+0.1)*(r+0.1))*1.5
ring_volume = (5*pi/6)*(ro*ro-ri*ri)*1.2
bolt_volume = pi*(2.5*2.5*65+4.5*4.5*3)
clamp_volume = (12*68-2*pi*3*3)*6
nut_volume = pi*(5*5-2.6*2.6)*5
expected_volume = base_volume+2*housing_volume+2*cover_volume+4*cover_screw_volume+2*sleeve_volume+shaft_volume+disc_volume+key_volume+3*washer_volume+3*ring_volume+4*bolt_volume+2*clamp_volume+4*nut_volume
expected_mass = (base_volume+2*housing_volume+2*cover_volume+2*clamp_volume)*0.0000027+2*sleeve_volume*0.0000088+(shaft_volume+disc_volume+key_volume+3*washer_volume+3*ring_volume+4*bolt_volume+4*nut_volume+4*cover_screw_volume)*0.0000078
assert_property(id = "left_housing_machined_volume", output = "housing", property = "volume_mm3", min = housing_volume-0.00001, max = housing_volume+0.00001)
assert_property(id = "right_housing_machined_volume", output = "right_housing", property = "volume_mm3", min = housing_volume-0.00001, max = housing_volume+0.00001)
assert_property(id = "end_cover_volume", output = "end_cover", property = "volume_mm3", min = cover_volume-0.00001, max = cover_volume+0.00001)
assert_property(id = "cover_screw_envelope_volume", output = "cover_screw_envelope", property = "volume_mm3", min = cover_screw_volume-0.00001, max = cover_screw_volume+0.00001)
assert_property(id = "material_volume", output = "drive_module_release", property = "volume_mm3", min = expected_volume-0.00001, max = expected_volume+0.00001)
assert_property(id = "material_mass", output = "drive_module_release", property = "mass_kg", min = expected_mass-0.00000001, max = expected_mass+0.00000001)
assert_property(id = "mass_budget", output = "drive_module_release", property = "mass_kg", max = 2)

# Named static obligations only; this is not a whole-assembly collision certificate.
def support_checks():
    for side in ["left_support","right_support"]:
        bush_id = "sleeve" if side == "left_support" else "right_sleeve"
        clearance_check(id = side+"_running_gap", output = "drive_module_release", left_instance_path = [side,bush_id], right_instance_path = ["shaft"], min_clearance = required_clearance, rotation_group = "rotor")
        clearance_check(id = side+"_disc_gap", output = "drive_module_release", left_instance_path = [side,bush_id], right_instance_path = ["drive_disc"], min_clearance = 2, rotation_group = "rotor")
support_checks()
clearance_check(id = "disc_shaft_gap", output = "drive_module_release", left_instance_path = ["drive_disc"], right_instance_path = ["shaft"], min_clearance = required_clearance, rotation_group = "rotor")
def washer_checks():
    for name in ["hub_left_washer","hub_right_washer","shaft_washer"]:
        clearance_check(id = name+"_shaft_gap", output = "drive_module_release", left_instance_path = [name], right_instance_path = ["shaft"], min_clearance = 0.09, rotation_group = "rotor")
    for name in ["hub_left_washer","hub_right_washer"]:
        clearance_check(id = name+"_hub_gap", output = "drive_module_release", left_instance_path = [name], right_instance_path = ["drive_disc"], min_clearance = 0.09, rotation_group = "rotor")
washer_checks()
clearance_check(id = "shaft_washer_bush_gap", output = "drive_module_release", left_instance_path = ["shaft_washer"], right_instance_path = ["right_support","right_sleeve"], min_clearance = 0.09, rotation_group = "rotor")
# The actual polygonal end covers are fixed while the keyed shaft turns. Native
# whole-turn bore witnesses certify the shaft's clearance to each cover aperture.
# These two checks do not stand in for all new hardware pair interactions.
clearance_check(id = "left_cover_shaft_gap", output = "drive_module_release", left_instance_path = ["left_support","left_cover"], right_instance_path = ["shaft"], min_clearance = 2, rotation_group = "rotor")
clearance_check(id = "right_cover_shaft_gap", output = "drive_module_release", left_instance_path = ["right_support","right_cover"], right_instance_path = ["shaft"], min_clearance = 4, rotation_group = "rotor")
# Ring/groove, ring/washer and key/floor contacts are intentional. The open ring
# is outside the clearance-query family; no full-annulus surrogate is queried.

# Plan section through mounting plate and transverse section through left bearing.
section_view(id = "mounting_plan", output = "drive_module_release", origin = [0,0,3])
def base_radius_dimensions():
    for feature, offset in [(base_front_left,[-10,-10]),(base_front_right,[10,-10]),(base_back_right,[10,10]),(base_body,[-10,10])]:
        assert_faces(id = feature+"_survives", from_feature = feature, role = "fillet_wall", in_feature = base_body, count = 1)
        section_circle_dimension(view = "mounting_plan", id = feature+"_radius", from_feature = feature, role = "fillet_wall", instance_path = ["base"], kind = "radius", coverage = "single_arc", label_offset = offset, min = base_corner_radius-0.000001, max = base_corner_radius+0.000001)
base_radius_dimensions()
section_dimension(view = "mounting_plan", id = "support_pitch", from_instance_path = ["left_support","housing"], from_datum = "axis", to_instance_path = ["right_support","right_housing"], to_datum = "axis", axis = "x", offset = -50,
    tolerance = {"nominal":p,"lower":-0.05,"upper":0.05})
section_view(id = "bearing_section", output = "bearing_fit_inspection", instance_path = ["inspect_support","housing"], datum = "axis", origin = [0,0,8])
section_circle_dimension(view = "bearing_section", id = "shaft_diameter", from_feature = "shaft_stock", role = "segment_3", instance_path = ["inspect_shaft"], kind = "diameter", label_offset = [30,30], tolerance = {"nominal":shaft_diameter,"lower":-0.02,"upper":0})
section_circle_dimension(view = "bearing_section", id = "sleeve_bore", from_feature = "sleeve_body", role = "segment_5", instance_path = ["inspect_support","sleeve"], kind = "diameter", label_offset = [34,15], tolerance = {"nominal":shaft_diameter+0.3,"lower":0,"upper":0.02})
section_circle_dimension(view = "bearing_section", id = "sleeve_outer", from_feature = "sleeve_body", role = "segment_3", instance_path = ["inspect_support","sleeve"], kind = "diameter", label_offset = [-34,15], tolerance = {"nominal":shaft_diameter+8,"lower":-0.015,"upper":0})
section_circle_dimension(view = "bearing_section", id = "housing_bore", from_feature = "housing_body", role = "circular_hole_0_wall", instance_path = ["inspect_support","housing"], kind = "diameter", label_offset = [-34,30], tolerance = {"nominal":shaft_diameter+8.05,"lower":0,"upper":0.02})
tolerance_stack(id = "running_diametral_limits", terms = [{"view":"bearing_section","dimension":"sleeve_bore","coefficient":1},{"view":"bearing_section","dimension":"shaft_diameter","coefficient":-1}], min = 0.29, max = 0.35)
tolerance_stack(id = "housing_sleeve_diametral_limits", terms = [{"view":"bearing_section","dimension":"housing_bore","coefficient":1},{"view":"bearing_section","dimension":"sleeve_outer","coefficient":-1}], min = 0.04, max = 0.09)
drawing_sheet(id = "drive_module_sections", title = "Bearing drive layout — mounting and nominal size limits", revision = design_revision, width = 420, height = 297,
    views = [{"view":"mounting_plan","x":10,"y":10,"width":240,"height":240,"scale":0.5},{"view":"bearing_section","x":260,"y":10,"width":150,"height":240,"scale":1}])

# Native parallel stop planes, measured from resolved B-rep face datums. These
# authored manufacturing limits are illustrative design requirements, not a
# supplier ring specification or a proof of spring installation/load capacity.
datum_dimension(id = "shaft_stop_span", output = "drive_module_release", from_instance_path = ["shaft"], from_datum = "shoulder", to_instance_path = ["shaft"], to_datum = "loaded_groove_wall", direction = 1,
    tolerance = {"nominal":p+22.9,"lower":-0.025,"upper":0.025})
datum_dimension(id = "bush_outer_span", output = "drive_module_release", from_instance_path = ["left_support","sleeve"], from_datum = "outer_thrust", to_instance_path = ["right_support","right_sleeve"], to_datum = "outer_thrust", direction = -1,
    tolerance = {"nominal":p+20,"lower":-0.025,"upper":0.025})
datum_dimension(id = "hub_stop_span", output = "drive_module_release", from_instance_path = ["shaft"], from_datum = "hub_left_stop", to_instance_path = ["shaft"], to_datum = "hub_right_stop", direction = 1,
    tolerance = {"nominal":19.6,"lower":-0.025,"upper":0.025})
datum_dimension(id = "hub_width", output = "drive_module_release", from_instance_path = ["drive_disc"], from_datum = "start", to_instance_path = ["drive_disc"], to_datum = "end", direction = -1,
    tolerance = {"nominal":14,"lower":-0.02,"upper":0.02})
def retention_thickness_dimensions():
    for name in ["shaft_washer","hub_left_washer","hub_right_washer"]:
        datum_dimension(id = name+"_thickness", output = "drive_module_release", from_instance_path = [name], from_datum = "start", to_instance_path = [name], to_datum = "end", direction = -1,
            tolerance = {"nominal":1.5,"lower":-0.01,"upper":0.01})
    for name in ["shaft_ring","hub_left_ring","hub_right_ring"]:
        datum_dimension(id = name+"_thickness", output = "drive_module_release", from_instance_path = [name], from_datum = "start", to_instance_path = [name], to_datum = "end", direction = -1,
            tolerance = {"nominal":1.2,"lower":-0.01,"upper":0.01})
retention_thickness_dimensions()
tolerance_stack(id = "shaft_axial_endplay", terms = [
    {"datum_dimension":"shaft_stop_span","coefficient":1},
    {"datum_dimension":"bush_outer_span","coefficient":-1},
    {"datum_dimension":"shaft_washer_thickness","coefficient":-1},
    {"datum_dimension":"shaft_ring_thickness","coefficient":-1},
], min = minimum_endplay, max = 0.35)
tolerance_stack(id = "hub_axial_endplay", terms = [
    {"datum_dimension":"hub_stop_span","coefficient":1},
    {"datum_dimension":"hub_width","coefficient":-1},
    {"datum_dimension":"hub_left_washer_thickness","coefficient":-1},
    {"datum_dimension":"hub_right_washer_thickness","coefficient":-1},
    {"datum_dimension":"hub_left_ring_thickness","coefficient":-1},
    {"datum_dimension":"hub_right_ring_thickness","coefficient":-1},
], min = minimum_endplay, max = 0.35)

# ONE common world transform is composed with each initial occurrence frame.
# The key and open rings move with the rotor, not around independent local axes.
# Their membership is not an implicit claim of clearance against every housing.
rigid_rotation_group(id = "rotor", output = "drive_module_release",
    instance_paths = [["shaft"],["drive_disc"],["parallel_key"],
        ["hub_left_washer"],["hub_left_ring"],["hub_right_washer"],
        ["hub_right_ring"],["shaft_washer"],["shaft_ring"]],
    reference_instance_path = ["shaft"], pivot = [0,0,0], axis = [0,0,1],
    angle_radians = turn_radians)
