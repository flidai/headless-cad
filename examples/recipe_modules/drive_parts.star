# Imports use labels relative to the root recipe's locked module set.
load("recipe_modules/stock.star", "rectangular_stock", "through_bore")

def rail_part(id, length, setback):
    stock = rectangular_stock(id+"_stock", length, 20, 20)
    front = chamfer_faces(id = id+"_front", body = stock, faces = [{"from_feature":stock,"role":"wall_y_min"},{"from_feature":stock,"role":"end_cap"}], distance = setback)
    body = chamfer_faces(id = id+"_back", body = front, faces = [{"from_feature":stock,"role":"wall_y_max"},{"from_feature":stock,"role":"end_cap"}], distance = setback)
    return part(id = id, body = body, part_number = "STAGE-RAIL", revision = "A", material = "Steel", density_kg_per_m3 = 7800)

def carriage_part(id, bore_radius):
    stock = rectangular_stock(id+"_stock", 40, 90, 8)
    body = stock
    for i in range(4):
        x = 8 if i < 2 else 32
        y = 8 if i % 2 == 0 else 82
        body = through_bore(id+"_bore_"+str(i), body, x, y, bore_radius, 8)
    return part(id = id, body = body, part_number = "STAGE-CARRIAGE", revision = "A", material = "Aluminum", density_kg_per_m3 = 2700, datums = {"top":face_datum(from_feature = stock, role = "end_cap", x_direction = [1,0,0])})
