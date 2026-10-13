# A separate loosening/slot-travel study of the released drive. The loaded
# recipe remains authoritative and unchanged; this source adds a new assembly
# output with its upper 29 leaves moving as one rigid translation.
load("drive_module_rotation.star", "base", "bolt", "nut", "bolt_centers", "base_slot_length")

# The new fasteners are nominal loose envelopes: head lifted 1 mm and nut
# lowered 1 mm. This study makes no clamp preload or thread-engagement claim.
adjustment_bolts = [instance(id = "adjustment_bolt_"+str(i), part = bolt,
    offset = [xy[0],xy[1],-4]) for i,xy in enumerate(bolt_centers)]
adjustment_nuts = [instance(id = "adjustment_nut_"+str(i), part = nut,
    offset = [xy[0],xy[1],-6]) for i,xy in enumerate(bolt_centers)]

# Existing instance declarations can be embedded in another assembly without
# copying their parts or rewriting the released output's occurrence frames.
upper_instance_ids = [
    "left_clamp", "right_clamp", "left_support", "right_support",
    "shaft", "drive_disc", "parallel_key", "hub_left_washer", "hub_left_ring",
    "hub_right_washer", "hub_right_ring", "shaft_washer", "shaft_ring",
] + adjustment_bolts + adjustment_nuts
upper = assembly(id = "drive_adjustment_upper", instances = upper_instance_ids)
adjusted = assembly(id = "drive_module_adjustment", instances = [
    instance(id = "adjustment_base", part = base),
    assembly_instance(id = "moving", assembly = upper,
        offset = [-base_slot_length/2,0,0]),
])
assembly_output(name = "drive_module_adjustment", assembly = adjusted)

def upper_leaf_paths():
    paths = [["moving","left_clamp"], ["moving","right_clamp"]]
    for side in ["left_support", "right_support"]:
        parts = ["housing", "sleeve", "left_cover", "left_cover_screw_minus", "left_cover_screw_plus"] if side == "left_support" else ["right_housing", "right_sleeve", "right_cover", "right_cover_screw_minus", "right_cover_screw_plus"]
        for name in parts:
            paths.append(["moving",side,name])
    for name in ["shaft", "drive_disc", "parallel_key", "hub_left_washer", "hub_left_ring",
                 "hub_right_washer", "hub_right_ring", "shaft_washer", "shaft_ring"]:
        paths.append(["moving",name])
    for i in range(4):
        paths.append(["moving","adjustment_bolt_"+str(i)])
        paths.append(["moving","adjustment_nut_"+str(i)])
    return paths

rigid_translation_group(id = "upper_slot_travel", output = "drive_module_adjustment",
    instance_paths = upper_leaf_paths(), reference_instance_path = ["moving","left_clamp"],
    translation = [base_slot_length,0,0])
def check_mounting_slot_motion():
    for i in range(4):
        clearance_check(id = "mounting_slot_"+str(i)+"_shank_gap",
            output = "drive_module_adjustment",
            left_instance_path = ["adjustment_base"],
            right_instance_path = ["moving","adjustment_bolt_"+str(i)],
            min_clearance = 0.49, translation_group = "upper_slot_travel")
check_mounting_slot_motion()
