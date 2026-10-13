# A solved face-attached clamp-hole pattern on a rounded, through-bored housing.
# The source sketch determines the hole centers and radii; the viewer only
# inspects the resulting validated geometry. Flat floors are nominal and do
# not represent drill tips, threads, bolt preload, or a manufactured fit.
pi = 3.141592653589793
half_pitch = param(name = "half_pitch", default = 16)
hole_radius = param(name = "hole_radius", default = 2)
hole_depth = param(name = "hole_depth", default = 5)
pattern_y = param(name = "pattern_y", default = 0)
underside = param(name = "underside", default = 0)
pose = param(name = "pose", default = 0)
def model():
    if half_pitch < 8 or half_pitch > 24 or hole_radius < 1.5 or hole_radius > 3 or hole_depth < 3 or hole_depth > 12 or pattern_y < -4 or pattern_y > 4 or underside not in [0,1] or pose not in [0,1,2]:
        fail("half_pitch 8..24, hole_radius 1.5..3, hole_depth 3..12, pattern_y -4..4, underside 0/1 and pose 0/1/2 required")

    stock = extrude(id = "stock", profile = rectangle(id = "outline", width = 50, depth = 36), height = 12)
    round_0 = fillet(id = "round_0", body = stock, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_min"}], radius = 4)
    round_1 = fillet(id = "round_1", body = round_0, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_min"}], radius = 4)
    round_2 = fillet(id = "round_2", body = round_1, faces = [{"from_feature":stock,"role":"wall_x_max"},{"from_feature":stock,"role":"wall_y_max"}], radius = 4)
    rounded = fillet(id = "rounded", body = round_2, faces = [{"from_feature":stock,"role":"wall_x_min"},{"from_feature":stock,"role":"wall_y_max"}], radius = 4)
    centered = translate(id = "centered", body = rounded, offset = [-25,-18,0])
    through_tool = translate(id = "through_tool", body = extrude(id = "through_cylinder", profile = circle(id = "through_profile", radius = 7), height = 14), offset = [0,0,-1])
    bored = cut(id = "bored", left = centered, right = through_tool)

    # Drill after the common rigid placement. The face selection and tangent X
    # direction must follow that placement; the solved coordinates remain local to
    # the selected planar cap. The stock center remains the cap area centroid.
    qx = 0.1
    qy = 0.2
    qz = 0.3
    qw = sqrt(0.86)
    frame = [0,0,0,1] if pose == 0 else ([0.5,0.5,0.5,0.5] if pose == 1 else [qx,qy,qz,qw])
    offset = [0,0,0] if pose == 0 else ([13,-7,15] if pose == 1 else [11,-9,17])
    # The second pose is genuinely oblique. Rotate the local X hint analytically
    # with the source quaternion so it remains tangent to the selected cap.
    oblique_hint = [1-2*(qy*qy+qz*qz), 2*(qx*qy+qz*qw), 2*(qx*qz-qy*qw)]
    hint = [1,0,0] if pose == 0 else ([0,1,0] if pose == 1 else oblique_hint)
    placed = transform(id = "placed", body = bored, rotation = frame, offset = offset)
    cap_role = "start_cap" if underside == 1 else "end_cap"
    points = [
        {"id":"center_left","position":[-half_pitch,0]},
        {"id":"center_right","position":[half_pitch,0]},
        {"id":"rim_left","position":[-half_pitch+hole_radius,0]},
        {"id":"rim_right","position":[half_pitch+hole_radius,0]},
    ]
    constraints = [
        {"id":"ground_left","kind":"fixed","point":"center_left","position":[-half_pitch,0]},
        {"id":"center_axis","kind":"horizontal","a":"center_left","b":"center_right"},
        {"id":"center_pitch","kind":"distance","a":"center_left","b":"center_right","value":2*half_pitch},
        {"id":"left_radius_axis","kind":"horizontal","a":"center_left","b":"rim_left"},
        {"id":"left_radius","kind":"distance","a":"center_left","b":"rim_left","value":hole_radius},
        {"id":"right_radius_axis","kind":"horizontal","a":"center_right","b":"rim_right"},
        {"id":"right_radius","kind":"distance","a":"center_right","b":"rim_right","value":hole_radius},
    ]
    housing = hole_pattern(id = "housing", body = placed,
        face = {"from_feature":stock,"role":cap_role}, x_direction_hint = hint,
        origin_offset = [0,pattern_y], points = points, constraints = constraints,
        holes = [
            {"id":"clamp_left","center":"center_left","rim":"rim_left","depth":hole_depth},
            {"id":"clamp_right","center":"center_right","rim":"rim_right","depth":hole_depth},
        ])
    output(name = "housing", feature = housing)

    for side in ["clamp_left","clamp_right"]:
        assert_faces(id = side+"_wall", from_feature = housing, role = "hole_"+side+"_wall", in_feature = housing, count = 1)
        assert_faces(id = side+"_floor", from_feature = housing, role = "hole_"+side+"_floor", in_feature = housing, count = 1)
    assert_faces(id = "central_bore_survives", from_feature = bored, role = "bore_wall", in_feature = housing, count = 1)
    for corner in [round_0,round_1,round_2,rounded]:
        assert_faces(id = corner+"_survives", from_feature = corner, role = "fillet_wall", in_feature = housing, count = 1)

    outer_area = 50*36-(4-pi)*4*4
    expected_volume = (outer_area-pi*7*7)*12-2*pi*hole_radius*hole_radius*hole_depth
    assert_property(id = "exact_housing_volume", output = "housing", property = "volume_mm3", min = expected_volume-0.000001, max = expected_volume+0.000001)
    if pose == 0:
        cut_plane = hole_depth/2 if underside == 1 else 12-hole_depth/2
        cut_area = outer_area-pi*(7*7+2*hole_radius*hole_radius)
        section_view(id = "clamp_holes", output = "housing", origin = [0,0,cut_plane], min_area = cut_area-0.000001, max_area = cut_area+0.000001)
        section_circle_dimension(view = "clamp_holes", id = "left_clamp_diameter", from_feature = housing, role = "hole_clamp_left_wall", kind = "diameter", label_offset = [-10,12], min = 2*hole_radius-0.000001, max = 2*hole_radius+0.000001)
        section_circle_dimension(view = "clamp_holes", id = "right_clamp_diameter", from_feature = housing, role = "hole_clamp_right_wall", kind = "diameter", label_offset = [10,12], min = 2*hole_radius-0.000001, max = 2*hole_radius+0.000001)
model()
