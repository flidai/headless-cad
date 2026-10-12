# Nominal design with explicit manufacturing size limits; no fit-class or GD&T claim.
# The native measurement must match nominal even if a wide manufacturing band is authored.
load("datum_section_fixture.star", "depth", "spacing")

bore_allowance = param(name = "bore_allowance", default = 0.02)
pin_allowance = param(name = "pin_allowance", default = 0.02)
def validate_limits():
    if bore_allowance <= 0 or bore_allowance > 0.1:
        fail("bore_allowance must be above 0 and at most 0.1 mm")
    if pin_allowance <= 0 or pin_allowance > 0.1:
        fail("pin_allowance must be above 0 and at most 0.1 mm")
validate_limits()

section_view(id = "pilot_limits", output = "datum_section_fixture",
    instance_path = ["carrier","left","housing"], datum = "top",
    origin = [0,0,-depth-1])
section_dimension(view = "pilot_limits", id = "station_pitch",
    from_instance_path = ["carrier","left","housing"], from_datum = "top",
    to_instance_path = ["carrier","right","housing"], to_datum = "top",
    offset = -35, tolerance = {"nominal":spacing,"lower":-0.05,"upper":0.05})
section_circle_dimension(view = "pilot_limits", id = "bore_size",
    from_feature = "pilot_body", role = "wall",
    instance_path = ["carrier","left","housing"], kind = "diameter",
    label_offset = [0,35], tolerance = {"nominal":6,"lower":0,"upper":bore_allowance})
section_circle_dimension(view = "pilot_limits", id = "pin_size",
    from_feature = "pin_blank", role = "wall",
    instance_path = ["carrier","right","pin"], kind = "diameter",
    label_offset = [0,35], tolerance = {"nominal":5.6,"lower":-pin_allowance,"upper":0})

drawing_sheet(id = "bearing_limits", title = "Bearing fixture — nominal sizes and manufacturing limits",
    revision = "A", width = 297, height = 210,
    views = [{"view":"pilot_limits","x":10,"y":10,"width":277,"height":165,"scale":1}])
