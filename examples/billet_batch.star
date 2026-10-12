# A separated rectangular-stock batch, not a fused plate or an assembly BOM.
# Row-wise exact unions preserve each independent native solid and its lineage.
rows = param(name = "rows", default = 8)
columns = param(name = "columns", default = 8)
def make_batch():
    if rows < 1 or rows > 12 or columns < 1 or columns > 12 or rows != int(rows) or columns != int(columns):
        fail("rows and columns must be integers in 1..12")
    stock = extrude(id = "stock", profile = rectangle(id = "profile", width = 10, depth = 6), height = 3)
    batch = None
    for row in range(int(rows)):
        strip = None
        for column in range(int(columns)):
            billet = translate(id = "billet_"+str(row)+"_"+str(column), body = stock, offset = [12*column,8*row,0])
            if strip == None:
                strip = billet
            else:
                strip = union(id = "row_"+str(row)+"_"+str(column), left = strip, right = billet)
        if batch == None:
            batch = strip
        else:
            batch = union(id = "batch_"+str(row), left = batch, right = strip)
    # A genuine later Boolean edits the first stock piece after composition.
    tool = extrude(id = "trim_tool", profile = rectangle(id = "trim_profile", width = 2, depth = 6), height = 3)
    trimmed = cut(id = "trimmed", left = batch, right = tool)
    output(name = "batch", feature = batch)
    output(name = "trimmed", feature = trimmed)
    assert_faces(id = "stock_top_lineage", from_feature = stock, role = "end_cap", in_feature = batch, count = int(rows*columns))
    volume = rows*columns*180
    assert_property(id = "batch_volume", output = "batch", property = "volume_mm3", min = volume-0.000001, max = volume+0.000001)
    assert_property(id = "trim_volume", output = "trimmed", property = "volume_mm3", min = volume-36.000001, max = volume-35.999999)
make_batch()
