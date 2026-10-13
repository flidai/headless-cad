# A machining section of the native slotted base. The imported recipe retains
# ownership of its geometry and engineering obligations. All slot sizes below
# are measured from complete native section loops, not repeated recipe inputs.
load("drive_module_rotation.star", "base_slotted", "base_slot_length", "design_revision")

slot_drawing_scale = param(name = "slot_drawing_scale", default = 1)

section_view(id = "base_slot_plan", output = "base", origin = [0,0,3])
def dimension_base_slots():
    names = ["front_minus", "front_plus", "rear_minus", "rear_plus"]
    for i,name in enumerate(names):
        y_sign = -1 if i % 2 == 0 else 1
        width_offset = -60 if i < 2 else 85
        center_offset = -50 if i < 2 else 50
        role = "slot_"+name+"_walls"
        section_slot_dimension(view = "base_slot_plan", id = name+"_width",
            from_feature = base_slotted, role = role, kind = "width",
            label_offset = [width_offset,0],
            tolerance = {"nominal":6,"lower":0,"upper":0.1})
        section_slot_dimension(view = "base_slot_plan", id = name+"_overall",
            from_feature = base_slotted, role = role, kind = "overall_length",
            label_offset = [0,y_sign*24],
            tolerance = {"nominal":base_slot_length+6,"lower":0,"upper":0.1})
        section_slot_dimension(view = "base_slot_plan", id = name+"_centerline",
            from_feature = base_slotted, role = role, kind = "centerline_length",
            label_offset = [center_offset,y_sign*40],
            min = base_slot_length-0.000001, max = base_slot_length+0.000001)
dimension_base_slots()

slot_tags = [{"dimension":name+"_"+kind,"tag":tag+suffix}
    for name,tag in [("front_minus","FM"),("front_plus","FP"),("rear_minus","RM"),("rear_plus","RP")]
    for kind,suffix in [("width","W"),("overall","O"),("centerline","C")]]
drawing_sheet(id = "base_slot_machining", title = "Slotted base — native slot dimensions",
    revision = design_revision, width = 420, height = 297,
    views = [{"view":"base_slot_plan","x":10,"y":10,"width":400,"height":235,"scale":slot_drawing_scale,"paper_annotation_font_mm":2.8,"paper_caption":"BASE SLOT SECTION","annotation_labels":slot_tags}])
