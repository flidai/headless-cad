# Worst-case size-only diametral clearance. This does not establish alignment,
# geometric collision clearance, fit class or manufactured-part conformance.
load("bearing_limits.star", "bore_allowance")

tolerance_stack(id = "diametral_clearance", terms = [
    {"view":"pilot_limits", "dimension":"bore_size", "coefficient":1},
    {"view":"pilot_limits", "dimension":"pin_size", "coefficient":-1},
], min = 0.39, max = 0.45)
