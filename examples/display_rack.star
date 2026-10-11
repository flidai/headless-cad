# Display workload: vary repeated occurrences independently from unique geometry.
# This measures inspection scale, not advanced modeling or constraint solving.
# Example: cad build examples/display_rack.star --display-only --param rows=100 \
#          --param columns=100 --param variants=1 --out-dir out/display_benchmark
rows_value = param(name = "rows", default = 20)
columns_value = param(name = "columns", default = 20)
variants_value = param(name = "variants", default = 4)
rows = int(rows_value)
columns = int(columns_value)
variants = int(variants_value)

def validate_parameters():
    if rows != rows_value or columns != columns_value or variants != variants_value:
        fail("rows, columns, and variants must be integers")
    if rows < 1 or columns < 1 or rows * columns > 100000:
        fail("positive rows and columns with at most 100000 occurrences are required")
    if variants < 1 or variants > 1000 or variants > rows * columns:
        fail("variants must be 1..1000 and no greater than the occurrence count")
validate_parameters()

def definitions():
    result = []
    for i in range(variants):
        name = "block_" + str(i)
        profile = rectangle(id = name + "/profile", width = 8, depth = 8)
        body = extrude(id = name + "/body", profile = profile, height = 8 + i)
        result.append(part(id = name, body = body, part_number = "DISPLAY-" + str(i),
                           material = "Demo alloy", density_kg_per_m3 = 2700))
    return result
parts = definitions()

def occurrences():
    result = []
    for row in range(rows):
        for column in range(columns):
            index = row * columns + column
            result.append(instance(id = "cell_" + str(row) + "_" + str(column),
                                   part = parts[index % variants], offset = [12 * column, 12 * row, 0]))
    return result
rack = assembly(id = "display_rack", instances = occurrences())
assembly_output(name = "display_rack", assembly = rack)
