# Preliminary bearing-supported drive layout: source, engineering evidence and
# section-sheet release together. Shaft and disc have real mating key features.
# Axial retention, threads and load ratings remain design work; nominal keyed
# engagement does not establish rated torque or manufacturing completeness.
design_revision = "A"
shaft_diameter = param(name = "shaft_diameter", default = 12)
mounting_pitch = param(name = "mounting_pitch", default = 60)
required_clearance = param(name = "required_clearance", default = 0.1)

def validate_parameters():
    if shaft_diameter < 10 or shaft_diameter > 18:
        fail("shaft_diameter must be 10..18 mm")
    if mounting_pitch < 60 or mounting_pitch > 90:
        fail("mounting_pitch must be 60..90 mm")
    if required_clearance <= 0:
        fail("required_clearance must be positive")
validate_parameters()

r = shaft_diameter / 2
p = mounting_pitch
pi = 3.141592653589793
# Cyclic frame: local X -> world Y, local Y -> world Z, local Z -> world X.
shaft_frame = [0.5,0.5,0.5,0.5]
bolt_centers = [[x,y] for x in [33,p+33] for y in [-28,28]]
base_body = extrude(id = "base_body", profile = polygon(id = "base_profile",
    points = [[0,-40],[p+90,-40],[p+90,40],[0,40]],
    circular_holes = [{"center":xy,"radius":3} for xy in bolt_centers]), height = 6)
housing_body = extrude(id = "housing_body", profile = polygon(id = "housing_profile",
    points = [[-22,-26],[22,-26],[22,14],[14,22],[-14,22],[-22,14]],
    circular_holes = [{"center":[0,0],"radius":r+4.025}]), height = 16)
sleeve_body = extrude(id = "sleeve_body", profile = circle(id = "sleeve_profile", radius = r+4, inner_radius = r+0.15), height = 16)
shaft_stock = extrude(id = "shaft_stock", profile = circle(id = "shaft_profile", radius = r), height = p+70)
disc_stock = extrude(id = "disc_stock", profile = circle(id = "disc_profile", radius = 24, inner_radius = r+0.15), height = 14)
seat_box = extrude(id = "seat_box", profile = rectangle(id = "seat_profile", width = 4, depth = 4.04), height = 13.5)
seat_tool = translate(id = "seat_tool", body = seat_box, offset = [r-2,-2.02,15+p/2])
shaft_body = cut(id = "shaft_body", left = shaft_stock, right = seat_tool)
keyway_box = extrude(id = "keyway_box", profile = rectangle(id = "keyway_profile", width = r+2.05, depth = 4.04), height = 16)
keyway_tool = translate(id = "keyway_tool", body = keyway_box, offset = [0,-2.02,-1])
disc_body = cut(id = "disc_body", left = disc_stock, right = keyway_tool)
key_body = extrude(id = "key_body", profile = rectangle(id = "key_profile", width = 4, depth = 4), height = 13)
spacer_body = extrude(id = "spacer_body", profile = circle(id = "spacer_profile", radius = r+4, inner_radius = r+0.1), height = 5)
bolt_body = revolve(id = "bolt_body", profile = polygon(id = "bolt_profile", points = [[0,0],[2.5,0],[2.5,65],[4.5,65],[4.5,68],[0,68]]))

clamp_body = extrude(id = "clamp_body", profile = polygon(id = "clamp_profile", points = [[0,-34],[12,-34],[12,34],[0,34]], circular_holes = [{"center":[6,y],"radius":3} for y in [-28,28]]), height = 6)
nut_body = extrude(id = "nut_body", profile = circle(id = "nut_profile", radius = 5, inner_radius = 2.6), height = 5)
clamp = part(id = "clamp_bar", body = clamp_body, part_number = "DM-CLAMP", revision = design_revision, description = "Bridge clamp seated on bearing housing", material = "Aluminum", density_kg_per_m3 = 2700)
nut = part(id = "nut_envelope", body = nut_body, part_number = "DM-NUT-ENVELOPE", revision = design_revision, description = "Underside clamp nut envelope; no thread or preload model", material = "Steel", density_kg_per_m3 = 7800)
base = part(id = "base", body = base_body, part_number = "DM-BASE", revision = design_revision, description = "Four-hole mounting plate", material = "Aluminum", density_kg_per_m3 = 2700)
housing = part(id = "housing", body = housing_body, part_number = "DM-HOUSING", revision = design_revision, description = "Bearing housing seated under bridge clamp", material = "Aluminum", density_kg_per_m3 = 2700,
    datums = {"axis":datum(), "seat":datum(offset = [0,-26,0])})
sleeve = part(id = "sleeve", body = sleeve_body, part_number = "DM-SLEEVE", revision = design_revision, description = "Plain bronze bearing sleeve", material = "Bronze", density_kg_per_m3 = 8800)
shaft = part(id = "shaft", body = shaft_body, part_number = "DM-SHAFT", revision = design_revision, description = "Shaft with finite keyseat; axial retention pending", material = "Steel", density_kg_per_m3 = 7800)
disc = part(id = "drive_disc", body = disc_body, part_number = "DM-KEYED-DISC", revision = design_revision, description = "Keyed drive disc; no teeth, belt groove or rated load evidence", material = "Steel", density_kg_per_m3 = 7800)
key = part(id = "parallel_key", body = key_body, part_number = "DM-PARALLEL-KEY", revision = design_revision, description = "4 x 4 x 13 mm key; nominal engagement, load capacity unverified", material = "Steel", density_kg_per_m3 = 7800)
spacer = part(id = "spacer", body = spacer_body, part_number = "DM-SPACER", revision = design_revision, description = "Loose spacer envelope; does not provide axial retention", material = "Steel", density_kg_per_m3 = 7800)
bolt = part(id = "clamp_bolt_envelope", body = bolt_body, part_number = "DM-CLAMP-BOLT-ENVELOPE", revision = design_revision, description = "Through-clamp head and shank envelope; no thread or preload model", material = "Steel", density_kg_per_m3 = 7800)
support = assembly(id = "support", instances = [instance(id = "housing", part = housing), instance(id = "sleeve", part = sleeve)])
module = assembly(id = "drive_module_release", instances = [
    instance(id = "base", part = base),
    instance(id = "left_clamp", part = clamp, offset = [27,0,54]),
    instance(id = "right_clamp", part = clamp, offset = [p+27,0,54]),
    assembly_instance(id = "left_support", assembly = support, offset = [25,0,32], rotation = shaft_frame),
    assembly_instance(id = "right_support", assembly = support, offset = [25+p,0,32], rotation = shaft_frame),
    instance(id = "shaft", part = shaft, offset = [10,0,32], rotation = shaft_frame),
    instance(id = "drive_disc", part = disc, offset = [25+p/2,0,32], rotation = shaft_frame),
    instance(id = "parallel_key", part = key, offset = [25+p/2+0.25,r-2,30], rotation = shaft_frame),
    instance(id = "left_spacer", part = spacer, offset = [18,0,32], rotation = shaft_frame),
    instance(id = "right_spacer", part = spacer, offset = [p+43,0,32], rotation = shaft_frame),
] + [instance(id = "clamp_bolt_"+str(i), part = bolt, offset = [xy[0],xy[1],-5]) for i,xy in enumerate(bolt_centers)]
  + [instance(id = "clamp_nut_"+str(i), part = nut, offset = [xy[0],xy[1],-5]) for i,xy in enumerate(bolt_centers)])
assembly_output(name = "drive_module_release", assembly = module)
# A focused section target excludes perpendicular clamp fasteners, whose oblique
# circular sections are outside the current native section-query family.
fit_inspection = assembly(id = "fit_inspection", instances = [
    assembly_instance(id = "inspect_support", assembly = support, offset = [25,0,32], rotation = shaft_frame),
    instance(id = "inspect_shaft", part = shaft, offset = [10,0,32], rotation = shaft_frame),
])
assembly_output(name = "bearing_fit_inspection", assembly = fit_inspection)
output(name = "base", feature = base_body)
output(name = "housing", feature = housing_body)
output(name = "sleeve", feature = sleeve_body)
output(name = "shaft", feature = shaft_body)
output(name = "drive_disc", feature = disc_body)
output(name = "parallel_key", feature = key_body)

# Independent material-volume formulas include actual holes, not bounding boxes.
base_volume = (80*(p+90)-4*pi*3*3)*6
housing_volume = (44*48-64-pi*(r+4.025)*(r+4.025))*16
sleeve_volume = pi*((r+4)*(r+4)-(r+0.15)*(r+0.15))*16
half_width = 2.02
bore = r+0.15
seat_area = half_width*sqrt(r*r-half_width*half_width)+r*r*asin(half_width/r)-2*(r-2)*half_width
keyway_area = 2*(r+2.05)*half_width-half_width*sqrt(bore*bore-half_width*half_width)-bore*bore*asin(half_width/bore)
shaft_volume = pi*r*r*(p+70)-seat_area*13.5
disc_volume = (pi*(24*24-bore*bore)-keyway_area)*14
key_volume = 4*4*13
spacer_volume = pi*((r+4)*(r+4)-(r+0.1)*(r+0.1))*5
bolt_volume = pi*(2.5*2.5*65+4.5*4.5*3)
clamp_volume = (12*68-2*pi*3*3)*6
nut_volume = pi*(5*5-2.6*2.6)*5
expected_volume = base_volume+2*housing_volume+2*sleeve_volume+shaft_volume+disc_volume+key_volume+2*spacer_volume+4*bolt_volume+2*clamp_volume+4*nut_volume
expected_mass = (base_volume+2*housing_volume+2*clamp_volume)*0.0000027+2*sleeve_volume*0.0000088+(shaft_volume+disc_volume+key_volume+2*spacer_volume+4*bolt_volume+4*nut_volume)*0.0000078
assert_property(id = "material_volume", output = "drive_module_release", property = "volume_mm3", min = expected_volume-0.00001, max = expected_volume+0.00001)
assert_property(id = "material_mass", output = "drive_module_release", property = "mass_kg", min = expected_mass-0.00000001, max = expected_mass+0.00000001)
assert_property(id = "mass_budget", output = "drive_module_release", property = "mass_kg", max = 2)

# Named static obligations only; this is not a whole-assembly collision certificate.
def support_checks():
    for side in ["left_support","right_support"]:
        clearance_check(id = side+"_running_gap", output = "drive_module_release", left_instance_path = [side,"sleeve"], right_instance_path = ["shaft"], min_clearance = required_clearance)
        clearance_check(id = side+"_disc_gap", output = "drive_module_release", left_instance_path = [side,"sleeve"], right_instance_path = ["drive_disc"], min_clearance = 2)
support_checks()
clearance_check(id = "disc_shaft_gap", output = "drive_module_release", left_instance_path = ["drive_disc"], right_instance_path = ["shaft"], min_clearance = required_clearance)
clearance_check(id = "left_spacer_shaft_gap", output = "drive_module_release", left_instance_path = ["left_spacer"], right_instance_path = ["shaft"], min_clearance = 0.09)
clearance_check(id = "right_spacer_shaft_gap", output = "drive_module_release", left_instance_path = ["right_spacer"], right_instance_path = ["shaft"], min_clearance = 0.09)

# Plan section through mounting plate and transverse section through left bearing.
section_view(id = "mounting_plan", output = "drive_module_release", origin = [0,0,3])
section_dimension(view = "mounting_plan", id = "support_pitch", from_instance_path = ["left_support","housing"], from_datum = "axis", to_instance_path = ["right_support","housing"], to_datum = "axis", axis = "x", offset = -50,
    tolerance = {"nominal":p,"lower":-0.05,"upper":0.05})
section_view(id = "bearing_section", output = "bearing_fit_inspection", instance_path = ["inspect_support","housing"], datum = "axis", origin = [0,0,8])
section_circle_dimension(view = "bearing_section", id = "shaft_diameter", from_feature = "shaft_stock", role = "wall", instance_path = ["inspect_shaft"], kind = "diameter", label_offset = [30,30], tolerance = {"nominal":shaft_diameter,"lower":-0.02,"upper":0})
section_circle_dimension(view = "bearing_section", id = "sleeve_bore", from_feature = "sleeve_body", role = "inner_wall", instance_path = ["inspect_support","sleeve"], kind = "diameter", label_offset = [34,15], tolerance = {"nominal":shaft_diameter+0.3,"lower":0,"upper":0.02})
section_circle_dimension(view = "bearing_section", id = "sleeve_outer", from_feature = "sleeve_body", role = "wall", instance_path = ["inspect_support","sleeve"], kind = "diameter", label_offset = [-34,15], tolerance = {"nominal":shaft_diameter+8,"lower":-0.015,"upper":0})
section_circle_dimension(view = "bearing_section", id = "housing_bore", from_feature = "housing_body", role = "circular_hole_0_wall", instance_path = ["inspect_support","housing"], kind = "diameter", label_offset = [-34,30], tolerance = {"nominal":shaft_diameter+8.05,"lower":0,"upper":0.02})
tolerance_stack(id = "running_diametral_limits", terms = [{"view":"bearing_section","dimension":"sleeve_bore","coefficient":1},{"view":"bearing_section","dimension":"shaft_diameter","coefficient":-1}], min = 0.29, max = 0.35)
tolerance_stack(id = "housing_sleeve_diametral_limits", terms = [{"view":"bearing_section","dimension":"housing_bore","coefficient":1},{"view":"bearing_section","dimension":"sleeve_outer","coefficient":-1}], min = 0.04, max = 0.09)
drawing_sheet(id = "drive_module_sections", title = "Bearing drive layout — mounting and nominal size limits", revision = design_revision, width = 420, height = 297,
    views = [{"view":"mounting_plan","x":10,"y":10,"width":240,"height":240,"scale":1},{"view":"bearing_section","x":260,"y":10,"width":150,"height":240,"scale":1}])
