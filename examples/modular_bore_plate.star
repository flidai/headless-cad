# Thirty independent center/rim constraint groups exceed the old whole-sketch
# point budget. This demonstrates disconnected-island scale, not a larger
# connected nonlinear mechanism. Grid anchors use declared pitch coordinates.
pitch = param(name = "pitch", default = 25)
bore_radius = param(name = "bore_radius", default = 4)
thickness = param(name = "thickness", default = 8)
def validate_parameters():
    if pitch < 22 or pitch > 28:
        fail("pitch must be 22..28 mm")
    if bore_radius < 3 or bore_radius > 5:
        fail("bore_radius must be 3..5 mm")
    if thickness < 6 or thickness > 10:
        fail("thickness must be 6..10 mm")
validate_parameters()
corners = [["a",[0,0]],["b",[180,0]],["c",[180,150]],["d",[0,150]]]
centers = [[90+(col-2.5)*pitch,75+(row-2)*pitch] for row in range(5) for col in range(6)]
points = [{"id":p[0],"position":p[1]} for p in corners]
points += [{"id":"center_"+str(i),"position":p} for i,p in enumerate(centers)]
points += [{"id":"rim_"+str(i),"position":[p[0]+3.5,p[1]+0.2]} for i,p in enumerate(centers)]
constraints = [{"id":"fix_"+p[0],"kind":"fixed","point":p[0],"position":p[1]} for p in corners]
constraints += [{"id":"fix_center_"+str(i),"kind":"fixed","point":"center_"+str(i),"position":p} for i,p in enumerate(centers)]
constraints += [{"id":"gauge_"+str(i),"kind":"horizontal","a":"center_"+str(i),"b":"rim_"+str(i)} for i in range(30)]
constraints += [{"id":"radius_"+str(i),"kind":"distance","a":"center_"+str(i),"b":"rim_"+str(i),"value":bore_radius} for i in range(30)]
layout = sketch(id = "modular_layout", points = points, constraints = constraints,
    outline = ["a","b","c","d"], circular_holes = [{"center":"center_"+str(i),"rim":"rim_"+str(i)} for i in range(30)])
plate = extrude(id = "modular_plate", profile = layout, height = thickness)
output(name = "plate", feature = plate)
assert_property(id = "plate_material", output = "plate", property = "volume_mm3", min = 147000, max = 262000)
plate_part = part(id = "plate", body = plate, part_number = "MB-PLATE", material = "Aluminum", density_kg_per_m3 = 2700)
foot = part(id = "foot", body = extrude(id = "foot_body", profile = rectangle(id = "foot_profile", width = 180, depth = 8), height = 12), part_number = "MB-FOOT", material = "Aluminum", density_kg_per_m3 = 2700)
guide = part(id = "guide", body = extrude(id = "guide_body", profile = circle(id = "guide_profile", radius = bore_radius, inner_radius = bore_radius-1), height = thickness+4), part_number = "MB-GUIDE", material = "Steel", density_kg_per_m3 = 7800)
# Guide placements share the declared grid anchors; no contact/fit solve occurs.
assembly_output(name = "modular_bore_plate", assembly = assembly(id = "modular_bore_plate", instances = [
    instance(id = "plate", part = plate_part, offset = [0,0,12]),
    instance(id = "front_foot", part = foot),
    instance(id = "rear_foot", part = foot, offset = [0,142,0]),
]+[instance(id = "guide_"+str(i), part = guide, offset = [p[0],p[1],12]) for i,p in enumerate(centers)]))
