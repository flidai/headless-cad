# Native slot centers and surviving stock sidewalls own these machining datums.
# The imported drive remains the geometry source; no center coordinates are
# repeated as authored datum locations.
load("drive_module_rotation.star", "base_stock", "base_slotted", "mounting_pitch", "design_revision")

position_drawing_scale = param(name = "position_drawing_scale", default = 1)

def machining_datums():
    result = {
        "left_edge": face_datum(from_feature = base_stock, role = "wall_3", x_direction = [0,1,0]),
        "lower_edge": face_datum(from_feature = base_stock, role = "wall_0", x_direction = [1,0,0]),
    }
    for name in ["front_minus", "front_plus", "rear_minus", "rear_plus"]:
        result[name] = slot_center_datum(from_feature = base_slotted, role = "slot_"+name+"_walls")
    return result

machining_base = part(id = "machining_base", body = base_slotted,
    part_number = "DM-BASE", revision = design_revision,
    description = "Native slot center machining references", datums = machining_datums())
machining_layout = assembly(id = "base_machining_layout", instances = [instance(id = "machining_leaf", part = machining_base)])
assembly_output(name = "base_machining_layout", assembly = machining_layout)

section_view(id = "slot_positions", output = "base_machining_layout", origin = [0,0,3])
section_view(id = "slot_pitches", output = "base_machining_layout", origin = [0,0,3])

def position_dimension(view, name, start, end, axis, offset, nominal):
    section_dimension(view = view, id = name,
        from_instance_path = ["machining_leaf"], from_datum = start,
        to_instance_path = ["machining_leaf"], to_datum = end,
        axis = axis, offset = offset,
        min = nominal-0.000001, max = nominal+0.000001)

position_dimension("slot_positions", "front_x", "left_edge", "front_minus", "x", -55, 33)
position_dimension("slot_positions", "rear_x", "left_edge", "rear_minus", "x", -70, mounting_pitch+33)
position_dimension("slot_positions", "lower_y", "lower_edge", "front_minus", "y", mounting_pitch+110, 12)
position_dimension("slot_positions", "upper_y", "lower_edge", "front_plus", "y", -40, 68)
position_dimension("slot_pitches", "pitch_x", "front_plus", "rear_plus", "x", 60, mounting_pitch)
position_dimension("slot_pitches", "pitch_y", "rear_minus", "rear_plus", "y", mounting_pitch+110, 56)

drawing_sheet(id = "base_slot_positions", title = "Slotted base — native machining positions",
    revision = design_revision, width = 594, height = 420,
    views = [
        {"view":"slot_positions","x":10,"y":10,"width":305,"height":340,"scale":position_drawing_scale,"paper_annotation_font_mm":2.8,"paper_caption":"EDGE REFERENCES","annotation_labels":[{"dimension":"front_x","tag":"X1"},{"dimension":"rear_x","tag":"X2"},{"dimension":"lower_y","tag":"Y1"},{"dimension":"upper_y","tag":"Y2"}]},
        {"view":"slot_pitches","x":325,"y":10,"width":259,"height":340,"scale":position_drawing_scale,"paper_annotation_font_mm":2.8,"paper_caption":"CENTER PITCHES","annotation_labels":[{"dimension":"pitch_x","tag":"PX"},{"dimension":"pitch_y","tag":"PY"}]},
    ])
