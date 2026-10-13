# A separate seated mounting state of the released drive at either slot end.
# The imported recipe owns all exact solids and its original release output.
# This fixture certifies only nominal face seating and four shank/slot gaps;
# threads, preload, torque and rated mounting loads remain unmodeled.
load("drive_module_rotation.star", "base", "bolt", "nut", "bolt_centers", "base_slot_length")

mounting_endpoint = param(name = "mounting_endpoint", default = -1)
bolt_lift = param(name = "bolt_lift", default = 0)
nut_drop = param(name = "nut_drop", default = 0)
def validate_mounting_parameters():
    if mounting_endpoint != -1 and mounting_endpoint != 1:
        fail("mounting_endpoint must be -1 or 1")
    if bolt_lift < 0 or bolt_lift > 1:
        fail("bolt_lift must be 0..1 mm")
    if nut_drop < 0 or nut_drop > 1:
        fail("nut_drop must be 0..1 mm")
validate_mounting_parameters()

# The upper 29 leaves move together; only the base remains fixed. Fasteners
# have their original seated Z at zero lift/drop, and follow the upper module.
tight_bolts = [instance(id = "tight_bolt_"+str(i), part = bolt,
    offset = [xy[0],xy[1],-5+bolt_lift]) for i,xy in enumerate(bolt_centers)]
tight_nuts = [instance(id = "tight_nut_"+str(i), part = nut,
    offset = [xy[0],xy[1],-5-nut_drop]) for i,xy in enumerate(bolt_centers)]
upper = assembly(id = "tightened_upper", instances = [
    "left_clamp", "right_clamp", "left_support", "right_support",
    "shaft", "drive_disc", "parallel_key", "hub_left_washer", "hub_left_ring",
    "hub_right_washer", "hub_right_ring", "shaft_washer", "shaft_ring",
] + tight_bolts + tight_nuts)
tightened = assembly(id = "drive_module_tightened", instances = [
    instance(id = "fixed_base", part = base),
    assembly_instance(id = "moving", assembly = upper,
        offset = [mounting_endpoint*base_slot_length/2,0,0]),
])
assembly_output(name = "drive_module_tightened", assembly = tightened)

def check_tightened_mounting():
    for i in range(4):
        clamp = ["moving","left_clamp" if i < 2 else "right_clamp"]
        bolt_path = ["moving","tight_bolt_"+str(i)]
        nut_path = ["moving","tight_nut_"+str(i)]
        nominal_seating_check(id = "tight_bolt_"+str(i)+"_head_seat",
            output = "drive_module_tightened",
            left_instance_path = bolt_path, left_datum = "underhead",
            right_instance_path = clamp, right_datum = "end",
            min_shared_disk_radius_mm = 0.4)
        nominal_seating_check(id = "tight_nut_"+str(i)+"_base_seat",
            output = "drive_module_tightened",
            left_instance_path = nut_path, left_datum = "end",
            right_instance_path = ["fixed_base"], right_datum = "start",
            min_shared_disk_radius_mm = 0.4)
        clearance_check(id = "tight_slot_"+str(i)+"_shank_gap",
            output = "drive_module_tightened",
            left_instance_path = ["fixed_base"], right_instance_path = bolt_path,
            min_clearance = 0.49)
check_tightened_mounting()
