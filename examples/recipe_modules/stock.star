# Shared modeling functions; IDs are supplied by the calling design.
def rectangular_stock(id, width, depth, height):
    return extrude(id = id, profile = rectangle(id = id+"_profile", width = width, depth = depth), height = height)

def through_bore(id, body, x, y, radius, height):
    tool = extrude(id = id+"_tool", profile = circle(id = id+"_circle", radius = radius), height = height+2)
    placed = translate(id = id+"_placed", body = tool, offset = [x,y,-1])
    return cut(id = id, left = body, right = placed)
