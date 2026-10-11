# Keep the common material of two overlapping boxes.
profile = rectangle(id = "profile", width = 20, depth = 20)
first = extrude(id = "first", profile = profile, height = 20)
second = translate(id = "second", body = first, offset = [10, 5, 5])
common = intersect(id = "common", left = first, right = second)
output(name = "intersection", feature = common)
