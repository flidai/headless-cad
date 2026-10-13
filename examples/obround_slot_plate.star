# Native full-height obround slots on a bounded plate. The explicit source
# frame is the untransformed lower cap: origin (0,0,0), X and Y of the profile.
plate_profile = polygon(id="plate_profile", points=[[0,0],[80,0],[80,50],[0,50]],
    circular_holes=[{"center":[40,25],"radius":5}])
stock = extrude(id="stock", profile=plate_profile, height=6)
slotted = slot_pattern(id="slotted", body=stock,
    frame_origin=[0,0,0], frame_x_direction=[1,0,0], frame_y_direction=[0,1,0],
    slots=[
        {"id":"left","center":[15,12],"direction":[1,0],"centerline_length":8,"radius":3},
        {"id":"right","center":[65,38],"direction":[0,1],"centerline_length":6,"radius":3},
    ])
assert_faces(id="left_slot_wall", from_feature=slotted, role="slot_left_wall_0", in_feature=slotted, count=1)
assert_faces(id="right_slot_wall", from_feature=slotted, role="slot_right_wall_0", in_feature=slotted, count=1)
assert_faces(id="stock_cap_survives", from_feature=stock, role="end_cap", in_feature=slotted, count=1)
output(name="slotted_plate", feature=slotted)
