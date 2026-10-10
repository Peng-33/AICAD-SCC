import math

import cadquery as cq

from .model import BuildResult


def build_gearbox(spec: dict) -> BuildResult:
    """Build the first bounded gearbox-housing B-Rep family."""
    parameters = spec["parameters"]
    envelope = parameters["envelope"]
    shell = parameters["shell"]
    axes = parameters["axes"]
    mounting = parameters.get("mounting", {})
    top_opening = parameters.get("top_opening", {})
    side_openings = parameters.get("side_openings", {})
    internal_cavities = parameters.get("internal_cavities", {})
    locating = parameters.get("locating", {})
    features: list[str] = []

    base = _rounded_prism(
        envelope["base_length"],
        envelope["base_width"],
        envelope["base_thickness"],
        shell["corner_radius"],
        0,
    )
    features.append("base.rounded_plate")

    body = _rounded_prism(
        envelope["body_length"],
        envelope["body_width"],
        envelope["body_height"],
        shell["body_radius"],
        envelope["base_thickness"],
    )
    shape = base.fuse(body)
    features.append("shell.outer_body")

    cavity_bottom = envelope["base_thickness"] + shell["bottom_reinforcement"]
    cavity = _rounded_prism(
        shell["inner_length"],
        shell["inner_width"],
        shell["inner_height"],
        min(shell["body_radius"], 8),
        cavity_bottom,
    )
    shape = shape.cut(cavity)
    features.append("shell.internal_cavity")

    opening_length = top_opening.get("length", 142)
    opening_width = top_opening.get("width", 88)
    opening_radius = top_opening.get("corner_radius", 10)
    opening_bottom = envelope["total_height"] - shell["top_thickness"]
    opening = _rounded_prism(
        opening_length,
        opening_width,
        shell["top_thickness"] + 2,
        opening_radius,
        opening_bottom,
    )
    shape = shape.cut(opening)
    features.append("top.inspection_opening")

    if top_opening:
        shape = _add_top_features(shape, envelope, top_opening, features)

    if mounting:
        shape = _add_mounting_features(shape, envelope, mounting, features)

    for axis_id in ("main", "second", "third"):
        shape = _cut_axis(shape, envelope, axes[axis_id], axis_id, features)

    if internal_cavities:
        shape = _cut_internal_clearances(shape, axes, internal_cavities, features)

    shape = _add_front_cover_patterns(shape, envelope, axes, features)

    if side_openings:
        shape = _cut_side_openings(shape, envelope, shell, side_openings, features)

    if locating:
        shape = _cut_locating_features(shape, envelope, locating, features)

    shape = shape.clean()
    workplane = cq.Workplane("XY").newObject([shape])
    if not shape.isValid() or not shape.Solids():
        raise ValueError("OCCT produced an invalid or empty gearbox solid")
    return BuildResult(shape=workplane, feature_ids=tuple(features))


def _add_top_features(
    shape: cq.Shape,
    envelope: dict,
    opening: dict,
    features: list[str],
) -> cq.Shape:
    groove_width = opening.get("seal_groove_width")
    groove_depth = opening.get("seal_groove_depth")
    groove_offset = opening.get("seal_groove_offset")
    if groove_width and groove_depth and groove_offset:
        half_width = groove_width / 2
        outer_offset = groove_offset + half_width
        inner_offset = groove_offset - half_width
        z = envelope["total_height"] - groove_depth
        outer = _rounded_prism(
            opening["length"] + 2 * outer_offset,
            opening["width"] + 2 * outer_offset,
            groove_depth + 1,
            opening["corner_radius"] + outer_offset,
            z,
        )
        inner = _rounded_prism(
            opening["length"] + 2 * inner_offset,
            opening["width"] + 2 * inner_offset,
            groove_depth + 2,
            opening["corner_radius"] + inner_offset,
            z - 0.5,
        )
        shape = shape.cut(outer.cut(inner))
        features.append("top.seal_groove")

    # M8 x 1.25 tapped holes are represented by their 6.8 mm tap-drill geometry.
    hole_line_y = opening["width"] / 2 + 9
    points = [(x, sy * hole_line_y) for sy in (-1, 1) for x in (-57, -19, 19, 57)]
    points.extend([(0, -hole_line_y), (0, hole_line_y)])
    holes = _vertical_cylinders(points, 3.4, 16, envelope["total_height"] - 15)
    shape = shape.cut(holes)
    features.append("top.ten_cover_tap_holes")
    return shape


def _rounded_prism(length: float, width: float, height: float, radius: float, z: float) -> cq.Shape:
    if radius <= 0 or radius * 2 >= min(length, width):
        raise ValueError("Rounded-prism radius is outside the supported range")
    horizontal = cq.Workplane("XY").rect(length - 2 * radius, width).extrude(height)
    vertical = cq.Workplane("XY").rect(length, width - 2 * radius).extrude(height)
    corners = cq.Workplane("XY").pushPoints([
        (sx * (length / 2 - radius), sy * (width / 2 - radius))
        for sx in (-1, 1)
        for sy in (-1, 1)
    ]).circle(radius).extrude(height)
    return horizontal.union(vertical).union(corners).val().translate((0, 0, z))


def _add_mounting_features(
    shape: cq.Shape,
    envelope: dict,
    mounting: dict,
    features: list[str],
) -> cq.Shape:
    x = envelope["base_length"] / 2 - mounting["edge_offset_x"]
    y = envelope["base_width"] / 2 - mounting["edge_offset_y"]
    points = [(sx * x, sy * y) for sx in (-1, 1) for sy in (-1, 1)]
    ears = cq.Workplane("XY").pushPoints(points).circle(mounting["ear_diameter"] / 2).extrude(envelope["base_thickness"]).val()
    shape = shape.fuse(ears)
    features.append("mounting.four_ears")

    for index, (px, py) in enumerate(points, start=1):
        through = cq.Solid.makeCylinder(
            mounting["hole_diameter"] / 2,
            envelope["base_thickness"] + 2,
            cq.Vector(px, py, -1),
            cq.Vector(0, 0, 1),
        )
        counterbore = cq.Solid.makeCylinder(
            mounting["counterbore_diameter"] / 2,
            mounting["counterbore_depth"] + 1,
            cq.Vector(px, py, envelope["base_thickness"] - mounting["counterbore_depth"]),
            cq.Vector(0, 0, 1),
        )
        shape = shape.cut(through.fuse(counterbore))
        features.append(f"mounting.ear_{index}.counterbored_through_hole")
    return shape


def _cut_axis(
    shape: cq.Shape,
    envelope: dict,
    axis: dict,
    axis_id: str,
    features: list[str],
) -> cq.Shape:
    body_half_width = envelope["body_width"] / 2
    x = axis["x"]
    z = axis["z"]
    through = cq.Solid.makeCylinder(
        axis["through_diameter"] / 2,
        envelope["body_width"] + 4,
        cq.Vector(x, -body_half_width - 2, z),
        cq.Vector(0, 1, 0),
    )
    shape = shape.cut(through)
    features.append(f"axis.{axis_id}.through_bore")

    front_diameter = axis.get("front_bearing_diameter")
    front_depth = axis.get("front_bearing_depth")
    if front_diameter and front_depth:
        front = cq.Solid.makeCylinder(
            front_diameter / 2,
            front_depth + 1,
            cq.Vector(x, -body_half_width - 1, z),
            cq.Vector(0, 1, 0),
        )
        shape = shape.cut(front)
        features.append(f"axis.{axis_id}.front_bearing_seat")

    front_step_diameter = axis.get("front_step_diameter")
    front_step_depth = axis.get("front_step_depth")
    if front_step_diameter and front_step_depth:
        front_step = cq.Solid.makeCylinder(
            front_step_diameter / 2,
            front_step_depth + 1,
            cq.Vector(x, -body_half_width + front_depth - 1, z),
            cq.Vector(0, 1, 0),
        )
        shape = shape.cut(front_step)
        features.append(f"axis.{axis_id}.front_step")

    rear_diameter = axis.get("rear_bearing_diameter")
    rear_depth = axis.get("rear_bearing_depth")
    if rear_diameter and rear_depth:
        rear = cq.Solid.makeCylinder(
            rear_diameter / 2,
            rear_depth + 1,
            cq.Vector(x, body_half_width + 1, z),
            cq.Vector(0, -1, 0),
        )
        shape = shape.cut(rear)
        features.append(f"axis.{axis_id}.rear_bearing_seat")

    rear_step_diameter = axis.get("rear_step_diameter")
    rear_step_depth = axis.get("rear_step_depth")
    if rear_step_diameter and rear_step_depth:
        rear_step = cq.Solid.makeCylinder(
            rear_step_diameter / 2,
            rear_step_depth + 1,
            cq.Vector(x, body_half_width - rear_depth + 1, z),
            cq.Vector(0, -1, 0),
        )
        shape = shape.cut(rear_step)
        features.append(f"axis.{axis_id}.rear_step")
    return shape


def _cut_internal_clearances(
    shape: cq.Shape,
    axes: dict,
    cavities: dict,
    features: list[str],
) -> cq.Shape:
    for axis_id in ("main", "second", "third"):
        cavity = cavities.get(axis_id)
        if not cavity:
            continue
        axis = axes[axis_id]
        cutter = cq.Solid.makeCylinder(
            cavity["diameter"] / 2,
            cavity["width"],
            cq.Vector(axis["x"], -cavity["width"] / 2, axis["z"]),
            cq.Vector(0, 1, 0),
        )
        shape = shape.cut(cutter)
        features.append(f"internal.{axis_id}_clearance")
    return shape


def _add_front_cover_patterns(
    shape: cq.Shape,
    envelope: dict,
    axes: dict,
    features: list[str],
) -> cq.Shape:
    patterns = {
        "main": (6, 138, 6.8, 16),
        "second": (4, 96, 5.0, 12),
        "third": (4, 78, 5.0, 12),
    }
    front_y = -envelope["body_width"] / 2 - 1
    for axis_id, (count, pcd, tap_drill, depth) in patterns.items():
        axis = axes[axis_id]
        cutters = None
        for index in range(count):
            angle = 2 * 3.141592653589793 * index / count
            px = axis["x"] + pcd / 2 * math.cos(angle)
            pz = axis["z"] + pcd / 2 * math.sin(angle)
            cutter = cq.Solid.makeCylinder(
                tap_drill / 2,
                depth + 1,
                cq.Vector(px, front_y, pz),
                cq.Vector(0, 1, 0),
            )
            cutters = cutter if cutters is None else cutters.fuse(cutter)
        shape = shape.cut(cutters)
        features.append(f"axis.{axis_id}.front_cover_pattern")
    return shape


def _cut_side_openings(
    shape: cq.Shape,
    envelope: dict,
    shell: dict,
    openings: dict,
    features: list[str],
) -> cq.Shape:
    half_length = envelope["body_length"] / 2
    half_width = envelope["body_width"] / 2
    wall_depth = shell["wall_thickness"] + 3

    right = openings.get("right_round")
    if right:
        y = -half_width + right["x_from_front"]
        through = cq.Solid.makeCylinder(
            right["diameter"] / 2,
            wall_depth,
            cq.Vector(half_length + 1, y, right["z"]),
            cq.Vector(-1, 0, 0),
        )
        recess = cq.Solid.makeCylinder(
            right["recess_diameter"] / 2,
            right["recess_depth"] + 1,
            cq.Vector(half_length + 1, y, right["z"]),
            cq.Vector(-1, 0, 0),
        )
        shape = shape.cut(through.fuse(recess))
        bolt_cutters = None
        for dy, dz in ((33, 0), (0, 33), (-33, 0), (0, -33)):
            cutter = cq.Solid.makeCylinder(
                2.5,
                13,
                cq.Vector(half_length + 1, y + dy, right["z"] + dz),
                cq.Vector(-1, 0, 0),
            )
            bolt_cutters = cutter if bolt_cutters is None else bolt_cutters.fuse(cutter)
        shape = shape.cut(bolt_cutters)
        features.extend(("side.right_observation_opening", "side.right_observation_bolt_pattern"))

    left = openings.get("left_rectangular")
    if left:
        y = -half_width + left["x_from_front"]
        plane = cq.Plane(
            origin=(-half_length - 1, y, left["z"]),
            xDir=(0, 1, 0),
            normal=(1, 0, 0),
        )
        opening = _rounded_prism_on_plane(
            plane,
            left["length"],
            left["height"],
            wall_depth,
            left["corner_radius"],
        )
        shape = shape.cut(opening)
        bolt_cutters = None
        for dy in (-30, 0, 30):
            for dz in (-35, 35):
                cutter = cq.Solid.makeCylinder(
                    2.5,
                    13,
                    cq.Vector(-half_length - 1, y + dy, left["z"] + dz),
                    cq.Vector(1, 0, 0),
                )
                bolt_cutters = cutter if bolt_cutters is None else bolt_cutters.fuse(cutter)
        shape = shape.cut(bolt_cutters)
        features.extend(("side.left_inspection_opening", "side.left_inspection_bolt_pattern"))
    return shape


def _cut_locating_features(
    shape: cq.Shape,
    envelope: dict,
    locating: dict,
    features: list[str],
) -> cq.Shape:
    y = -envelope["base_width"] / 2 + locating["dowel_front_offset"]
    dowel_points = [(-locating["dowel_spacing"] / 2, y), (locating["dowel_spacing"] / 2, y)]
    dowels = _vertical_cylinders(
        dowel_points,
        locating["dowel_hole_diameter"] / 2,
        locating["dowel_hole_depth"] + 1,
        -1,
    )
    shape = shape.cut(dowels)
    features.append("locating.two_dowel_holes")

    for x in (-locating["keyway_spacing"] / 2, locating["keyway_spacing"] / 2):
        keyway = (
            cq.Workplane("XY")
            .center(x, 0)
            .rect(locating["keyway_length"], locating["keyway_width"])
            .extrude(locating["keyway_depth"] + 1)
            .val()
            .translate((0, 0, -1))
        )
        shape = shape.cut(keyway)
    features.append("locating.two_keyways")
    return shape


def _vertical_cylinders(
    points: list[tuple[float, float]],
    radius: float,
    height: float,
    z: float,
) -> cq.Shape:
    return (
        cq.Workplane("XY")
        .pushPoints(points)
        .circle(radius)
        .extrude(height)
        .val()
        .translate((0, 0, z))
    )


def _rounded_prism_on_plane(
    plane: cq.Plane,
    length: float,
    width: float,
    depth: float,
    radius: float,
) -> cq.Shape:
    horizontal = cq.Workplane(plane).rect(length - 2 * radius, width).extrude(depth)
    vertical = cq.Workplane(plane).rect(length, width - 2 * radius).extrude(depth)
    corners = (
        cq.Workplane(plane)
        .pushPoints([
            (sx * (length / 2 - radius), sy * (width / 2 - radius))
            for sx in (-1, 1)
            for sy in (-1, 1)
        ])
        .circle(radius)
        .extrude(depth)
    )
    return horizontal.union(vertical).union(corners).val()
