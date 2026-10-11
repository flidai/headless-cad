# All dimensions are millimetres. mm() is an annotation in the initial API.
# Functions can generate reusable feature graphs; namespace their logical IDs.
def plate(prefix, width, depth, thickness):
    outline = rectangle(
        id = prefix + "/outline",
        width = width,
        depth = depth,
    )
    return extrude(
        id = prefix + "/blank",
        profile = outline,
        height = thickness,
    )

part = plate("plate", mm(80), mm(40), mm(6))
output(name = "plate", feature = part)
