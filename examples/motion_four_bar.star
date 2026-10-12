# Reuse the complete source-owned mechanism; this study does not change its nominal pose.
load("solved_four_bar.star", "layout", "angle")
high_angle = param(name = "high_angle", default = 1.3)
low_angle = param(name = "low_angle", default = 0.5)
max_pin_step = param(name = "max_pin_step", default = 2)
motion_study(id = "crank_cycle", mechanism = layout,
    targets = [{"input_angle":angle}, {"input_angle":high_angle},
               {"input_angle":low_angle}, {"input_angle":angle}],
    max_drive_step_radians = 0.08, max_pin_step_mm = max_pin_step)
# Every accepted sample is solved and branch-checked. Intervals between samples
# are not certified for collision, dynamics or continuous branch correctness.
