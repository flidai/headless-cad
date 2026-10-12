# Source-authored section sheet for the configurable two-station bearing fixture.
# Native dimensions are inspection/design checks, not manufacturing tolerances.
load("datum_section_fixture.star", "spacing")
sheet_scale = param(name = "sheet_scale", default = 0.8)
drawing_sheet(
    id = "bearing_sections",
    title = "Bearing fixture — datum-linked sections",
    revision = "A",
    width = 420,
    height = 297,
    views = [
        {"view":"left_seat", "x":10, "y":10, "width":195, "height":240, "scale":sheet_scale},
        {"view":"right_seat", "x":215, "y":10, "width":195, "height":240, "scale":sheet_scale},
    ],
)
