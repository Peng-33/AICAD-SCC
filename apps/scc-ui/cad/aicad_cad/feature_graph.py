import math

import cadquery as cq

from .model import BuildResult


AXIS_VECTORS = {
    "x": cq.Vector(1, 0, 0),
    "y": cq.Vector(0, 1, 0),
    "z": cq.Vector(0, 0, 1),
}


def build_feature_graph(document: dict) -> BuildResult:
    """Rebuild one ordered, data-only FeatureGraph as a single valid B-Rep."""
    parameter_values = {
        parameter_id: definition["value"]
        for parameter_id, definition in document["parameters"].items()
    }
    shape = None
    feature_ids: list[str] = []
    feature_records: list[dict] = []

    for feature in document["features"]:
        if feature.get("suppressed", False):
            feature_records.append({
                "id": feature["id"],
                "type": feature["type"],
                "operation": feature["operation"],
                "status": "suppressed",
            })
            continue

        tool = _build_tool(feature, parameter_values)
        operation = feature["operation"]
        before_volume = shape.Volume() if shape is not None else 0.0
        if operation == "add":
            candidate = tool if shape is None else shape.fuse(tool)
            if len(candidate.Solids()) != 1:
                raise ValueError(f"Feature {feature['id']} created disconnected material")
        elif operation == "cut":
            if shape is None:
                raise ValueError(f"Feature {feature['id']} cannot cut before material exists")
            candidate = shape.cut(tool)
            changed = before_volume - candidate.Volume()
            if changed <= max(1e-7, before_volume * 1e-12):
                raise ValueError(f"Feature {feature['id']} did not change the solid")
        else:
            raise ValueError(f"Unsupported feature operation: {operation}")

        candidate = candidate.clean()
        if not candidate.isValid() or len(candidate.Solids()) != 1:
            raise ValueError(f"Feature {feature['id']} produced an invalid B-Rep")
        shape = candidate
        feature_ids.append(feature["id"])
        feature_records.append({
            "id": feature["id"],
            "type": feature["type"],
            "operation": operation,
            "status": "built",
            "volume_after_mm3": round(shape.Volume(), 6),
        })

    if shape is None or not shape.isValid() or len(shape.Solids()) != 1:
        raise ValueError("FeatureGraph produced no valid single solid")
    return BuildResult(
        shape=cq.Workplane("XY").newObject([shape]),
        feature_ids=tuple(feature_ids),
        feature_records=tuple(feature_records),
    )


def _build_tool(feature: dict, parameters: dict[str, float]) -> cq.Shape:
    feature_type = feature["type"]
    origin = _origin(feature["origin"], parameters)
    if feature_type == "box":
        size = feature["size"]
        return cq.Solid.makeBox(
            _resolve(size["x"], parameters),
            _resolve(size["y"], parameters),
            _resolve(size["z"], parameters),
            cq.Vector(*origin),
        )
    if feature_type == "cylinder":
        return cq.Solid.makeCylinder(
            _resolve(feature["diameter"], parameters) / 2,
            _resolve(feature["length"], parameters),
            cq.Vector(*origin),
            AXIS_VECTORS[feature["axis"]],
        )
    if feature_type == "circular_hole_pattern":
        return _circular_pattern(feature, parameters, origin)
    raise ValueError(f"Unsupported feature type: {feature_type}")


def _circular_pattern(
    feature: dict,
    parameters: dict[str, float],
    origin: tuple[float, float, float],
) -> cq.Shape:
    radius = _resolve(feature["pitch_diameter"], parameters) / 2
    hole_radius = _resolve(feature["diameter"], parameters) / 2
    length = _resolve(feature["length"], parameters)
    start = math.radians(feature.get("start_angle", 0))
    axis = feature["axis"]
    result = None
    for index in range(feature["count"]):
        angle = start + (2 * math.pi * index / feature["count"])
        radial_a = radius * math.cos(angle)
        radial_b = radius * math.sin(angle)
        if axis == "z":
            point = (origin[0] + radial_a, origin[1] + radial_b, origin[2])
        elif axis == "y":
            point = (origin[0] + radial_a, origin[1], origin[2] + radial_b)
        else:
            point = (origin[0], origin[1] + radial_a, origin[2] + radial_b)
        cylinder = cq.Solid.makeCylinder(
            hole_radius,
            length,
            cq.Vector(*point),
            AXIS_VECTORS[axis],
        )
        result = cylinder if result is None else result.fuse(cylinder)
    return result


def _origin(value: dict, parameters: dict[str, float]) -> tuple[float, float, float]:
    return tuple(_resolve(value[axis], parameters) for axis in ("x", "y", "z"))


def _resolve(value, parameters: dict[str, float]) -> float:
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    return float(parameters[value["parameter"]])
