# Two machining stations with sections attached to each housing's semantic top.
# The recipes and named datums author these views; the viewer only inspects them.
height = param(name = "height", default = 15)
depth = param(name = "depth", default = 3)
spacing = param(name = "spacing", default = 90)
pose = param(name = "pose", default = 2)
shift = param(name = "shift", default = 25)

def validate_parameters():
    if height < 12 or height > 18:
        fail("height must be 12..18 mm")
    if depth < 2 or depth > 4:
        fail("depth must be 2..4 mm")
    if spacing < 80 or spacing > 100:
        fail("spacing must be 80..100 mm")
    if pose != 0 and pose != 1 and pose != 2:
        fail("pose must be 0, 1 or 2")
validate_parameters()

stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 60, depth = 40), height = height)
pilot = translate(id = "pilot", body = extrude(id = "pilot_body", profile = circle(id = "pilot_profile", radius = 3), height = height+4), offset = [30,20,-2])
drilled = cut(id = "drilled", left = stock, right = pilot)
sink_body = revolve(id = "sink_body", profile = polygon(id = "sink_profile", points = [[0,0],[3,0],[4+depth,depth+1],[0,depth+1]]))
sink = translate(id = "sink", body = sink_body, offset = [30,20,height-depth])
housing_body = cut(id = "housing_body", left = drilled, right = sink)
pin_body = translate(id = "pin_body", body = extrude(id = "pin_blank", profile = circle(id = "pin_profile", radius = 2.8), height = height+4), offset = [30,20,-2])
foot_body = translate(id = "foot_body", body = extrude(id = "foot_blank", profile = rectangle(id = "foot_profile", width = 80, depth = 60), height = 5), offset = [-10,-10,-5])

housing = part(id = "housing", body = housing_body, datums = {"top":face_datum(from_feature = stock, role = "end_cap", x_direction = [1,0,0])})
pin = part(id = "pin", body = pin_body)
foot = part(id = "foot", body = foot_body)
station = assembly(id = "station", instances = [instance(id = "housing", part = housing), instance(id = "pin", part = pin), instance(id = "foot", part = foot)])
carrier = assembly(id = "carrier", instances = [assembly_instance(id = "left", assembly = station), assembly_instance(id = "right", assembly = station, offset = [spacing,0,0])])
rotation = [0,0,0,1] if pose == 0 else ([0.7071067811865475,0,0,0.7071067811865476] if pose == 1 else [0.5,0.5,0,0.7071067811865476])
fixture = assembly(id = "fixture", instances = [assembly_instance(id = "carrier", assembly = carrier, offset = [shift,-12,7], rotation = rotation)])
assembly_output(name = "datum_section_fixture", assembly = fixture)
output(name = "housing", feature = housing_body)

pi = 3.141592653589793
section_radius = 3+depth/2
expected_area = 2*(60*40-pi*section_radius*section_radius+pi*2.8*2.8)
# Each view sections the entire assembly in the selected datum's plane.
# Areas sum occurrences; these bounds are validation checks, not tolerances.
section_view(id = "left_seat", output = "datum_section_fixture", instance_path = ["carrier","left","housing"], datum = "top", origin = [0,0,-depth/2], min_area = expected_area-0.000001, max_area = expected_area+0.000001)
section_view(id = "right_seat", output = "datum_section_fixture", instance_path = ["carrier","right","housing"], datum = "top", origin = [0,0,-depth/2], min_area = expected_area-0.000001, max_area = expected_area+0.000001)
