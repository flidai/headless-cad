# Shared engineering part numbers do not merge distinct definition revisions.
# STEP carries these three metadata fields; materials and datums remain sidecars.
revision_edit = param(name = "revision_edit", default = 0)
if_invalid = revision_edit != 0 and revision_edit != 1
def validate_parameters():
    if if_invalid:
        fail("revision_edit must be 0 or 1")
validate_parameters()
body = extrude(id = "body", profile = circle(id = "profile", radius = 3), height = 8)
a = part(id = "revision_a", body = body, part_number = "SHARED-α'42", revision = "A" if revision_edit == 0 else "A2", description = "Bearing's café — α")
b = part(id = "revision_b", body = body, part_number = "SHARED-α'42", revision = "B", description = "Bearing's café — β")
plain = part(id = "plain", body = body)
module = assembly(id = "plain", instances = [instance(id = "a", part = a), instance(id = "b", part = b, offset = [10,0,0]), instance(id = "plain", part = plain, offset = [20,0,0])])
assembly_output(name = "step_metadata", assembly = assembly(id = "step_metadata", instances = [assembly_instance(id = "left", assembly = module), assembly_instance(id = "right", assembly = module, offset = [0,20,0], rotation = [0,0,0.7071067811865475,0.7071067811865476])]))
output(name = "body", feature = body)
require_exchange(id = "numbers", output = "step_metadata", format = "step", aspect = "part_numbers")
require_exchange(id = "revisions", output = "step_metadata", format = "step", aspect = "part_revisions")
require_exchange(id = "descriptions", output = "step_metadata", format = "step", aspect = "part_descriptions")
