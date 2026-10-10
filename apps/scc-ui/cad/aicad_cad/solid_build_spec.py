"""Strict transport DTOs for the Phase 10.2 SolidBuildSpec contract.

This module deliberately contains no CadQuery calls and no engineering
semantics. It validates only the serialized v1 contract before handing
resolved physical geometry to the backend executor.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Mapping


SCHEMA_VERSION = 1
_TOLERANCE = 1e-9


class SolidBuildSpecError(ValueError):
    """Stable, structured transport validation failure."""

    def __init__(self, code: str, message: str, *, path: str | None = None,
                 operation_id: str | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.path = path
        self.operation_id = operation_id

    def as_error(self) -> dict[str, Any]:
        error: dict[str, Any] = {
            "stage": "cad_backend",
            "code": self.code,
            "message": self.message,
        }
        if self.path is not None:
            error["path"] = self.path
        if self.operation_id is not None:
            error["operation_id"] = self.operation_id
        return error


@dataclass(frozen=True)
class Point2:
    x: float
    y: float


@dataclass(frozen=True)
class ZInterval:
    minimum: float
    maximum: float


@dataclass(frozen=True)
class Bounds3D:
    x_min: float
    x_max: float
    y_min: float
    y_max: float
    z_min: float
    z_max: float


@dataclass(frozen=True)
class RectangleBodySpec:
    origin: Point2
    width: float
    height: float
    z: ZInterval
    bounds: Bounds3D


@dataclass(frozen=True)
class CircleBodySpec:
    center: Point2
    radius: float
    z: ZInterval
    bounds: Bounds3D


@dataclass(frozen=True)
class RoundedRectangleBodySpec:
    center: Point2
    width: float
    height: float
    corner_radius: float
    z: ZInterval
    bounds: Bounds3D


@dataclass(frozen=True)
class LineProfileSegment:
    start: Point2
    end: Point2


@dataclass(frozen=True)
class ArcProfileSegment:
    start: Point2
    end: Point2
    center: Point2
    radius: float
    direction: str


ProfileSegment = LineProfileSegment | ArcProfileSegment


@dataclass(frozen=True)
class ClosedProfileSpec:
    segments: tuple[ProfileSegment, ...]


@dataclass(frozen=True)
class ClosedProfileBodySpec:
    segments: tuple[ProfileSegment, ...]
    z: ZInterval
    bounds: Bounds3D


BodySpec = RectangleBodySpec | CircleBodySpec | RoundedRectangleBodySpec | ClosedProfileBodySpec

@dataclass(frozen=True)
class CylindricalCutSpec:
    operation_id: str
    source_id: str
    center: Point2
    radius: float
    z: ZInterval


@dataclass(frozen=True)
class SlotCutSpec:
    operation_id: str
    source_id: str
    profile: ClosedProfileSpec
    z: ZInterval


OperationSpec = CylindricalCutSpec | SlotCutSpec


@dataclass(frozen=True)
class SolidBuildSpecV1:
    body: BodySpec
    operations: tuple[OperationSpec, ...]
    body_source: dict[str, str]


def parse_solid_build_spec(value: Mapping[str, Any]) -> SolidBuildSpecV1:
    """Validate and decode a serialized v1 SolidBuildSpec.

    No field is defaulted. Operation IDs and source IDs are retained as
    opaque strings exactly as transported; they are never split or inferred.
    """

    root = _object(value, "SolidBuildSpec")
    expected_root_keys = {"schema_version", "part", "operations", "source_map"}
    if "schema_version" not in root:
        raise SolidBuildSpecError(
            "CAD-BACKEND-001",
            f"missing schema_version; expected {SCHEMA_VERSION}",
            path="$.schema_version",
        )
    _keys(root, expected_root_keys, "$")
    version = root["schema_version"]
    if isinstance(version, bool) or not isinstance(version, int) or version != SCHEMA_VERSION:
        raise SolidBuildSpecError(
            "CAD-BACKEND-001",
            f"unsupported schema_version; expected {SCHEMA_VERSION}",
            path="$.schema_version",
        )

    part = _object(root["part"], "part")
    _keys(part, {"body", "bounds"}, "$.part")
    body = _object(part["body"], "part.body")
    _keys(body, {"footprint", "z"}, "$.part.body")
    footprint = _object(body["footprint"], "part.body.footprint")
    body_z = _interval(body["z"], "$.part.body.z", require_base=True)
    bounds = _bounds(part["bounds"], "$.part.bounds")
    kind = footprint.get("kind")
    if kind == "rectangle":
        _keys(footprint, {"kind", "origin", "width", "height"}, "$.part.body.footprint")
        origin_data = _object(footprint["origin"], "part.body.footprint.origin")
        _keys(origin_data, {"x", "y"}, "$.part.body.footprint.origin")
        origin = Point2(
            _number(origin_data["x"], "$.part.body.footprint.origin.x"),
            _number(origin_data["y"], "$.part.body.footprint.origin.y"),
        )
        width = _positive(footprint["width"], "$.part.body.footprint.width")
        height = _positive(footprint["height"], "$.part.body.footprint.height")
        expected = Bounds3D(
            origin.x,
            origin.x + width,
            origin.y,
            origin.y + height,
            body_z.minimum,
            body_z.maximum,
        )
        if not _bounds_equal(expected, bounds):
            raise SolidBuildSpecError(
                "CAD-BACKEND-003",
                "body bounds do not match the resolved rectangle",
                path="$.part.bounds",
            )
        body_spec: BodySpec = RectangleBodySpec(origin, width, height, body_z, bounds)
    elif kind == "circle":
        _keys(footprint, {"kind", "center", "radius"}, "$.part.body.footprint")
        center_data = _object(footprint["center"], "part.body.footprint.center")
        _keys(center_data, {"x", "y"}, "$.part.body.footprint.center")
        center = Point2(
            _number(center_data["x"], "$.part.body.footprint.center.x"),
            _number(center_data["y"], "$.part.body.footprint.center.y"),
        )
        radius = _positive(footprint["radius"], "$.part.body.footprint.radius")
        expected = Bounds3D(
            center.x - radius,
            center.x + radius,
            center.y - radius,
            center.y + radius,
            body_z.minimum,
            body_z.maximum,
        )
        if not _bounds_equal(expected, bounds):
            raise SolidBuildSpecError(
                "CAD-BACKEND-003",
                "body bounds do not match the resolved circle",
                path="$.part.bounds",
            )
        body_spec = CircleBodySpec(center, radius, body_z, bounds)
    elif kind == "rounded_rectangle":
        _keys(footprint, {"kind", "center", "width", "height", "corner_radius"}, "$.part.body.footprint")
        center_data = _object(footprint["center"], "part.body.footprint.center")
        _keys(center_data, {"x", "y"}, "$.part.body.footprint.center")
        center = Point2(
            _number(center_data["x"], "$.part.body.footprint.center.x"),
            _number(center_data["y"], "$.part.body.footprint.center.y"),
        )
        width = _positive(footprint["width"], "$.part.body.footprint.width")
        height = _positive(footprint["height"], "$.part.body.footprint.height")
        corner_radius = _positive(footprint["corner_radius"], "$.part.body.footprint.corner_radius")
        if corner_radius > min(width, height) / 2.0 + _TOLERANCE:
            raise SolidBuildSpecError(
                "CAD-BACKEND-004",
                "rounded rectangle corner radius exceeds half the minimum extent",
                path="$.part.body.footprint.corner_radius",
            )
        expected = Bounds3D(
            center.x - width / 2.0,
            center.x + width / 2.0,
            center.y - height / 2.0,
            center.y + height / 2.0,
            body_z.minimum,
            body_z.maximum,
        )
        if not _bounds_equal(expected, bounds):
            raise SolidBuildSpecError(
                "CAD-BACKEND-003",
                "body bounds do not match the resolved rounded rectangle",
                path="$.part.bounds",
            )
        body_spec = RoundedRectangleBodySpec(center, width, height, corner_radius, body_z, bounds)
    elif kind == "closed_profile":
        footprint_path = "$.part.body.footprint"
        _keys(footprint, {"kind", "segments"}, footprint_path)
        segments = _closed_profile_segments(footprint["segments"], footprint_path)
        expected = _closed_profile_bounds(segments, body_z, footprint_path)
        if not _bounds_equal(expected, bounds):
            raise SolidBuildSpecError(
                "CAD-BACKEND-003",
                "body bounds do not match the resolved closed profile",
                path="$.part.bounds",
            )
        body_spec = ClosedProfileBodySpec(segments, body_z, bounds)
    else:
        raise SolidBuildSpecError(
            "CAD-BACKEND-002",
            "unsupported body footprint kind",
            path="$.part.body.footprint.kind",
        )


    operations_data = root["operations"]
    if not isinstance(operations_data, dict):
        raise SolidBuildSpecError("CAD-BACKEND-004", "operations must be an object", path="$.operations")
    source_map = _object(root["source_map"], "source_map")
    _keys(source_map, {"body", "operations"}, "$.source_map")
    body_source = _object(source_map["body"], "source_map.body")
    _keys(body_source, {"feature", "entity"}, "$.source_map.body")
    body_source_decoded = {
        "feature": _source(body_source["feature"], "$.source_map.body.feature"),
        "entity": _source(body_source["entity"], "$.source_map.body.entity"),
    }
    source_operations = source_map["operations"]
    if not isinstance(source_operations, dict):
        raise SolidBuildSpecError(
            "CAD-BACKEND-004",
            "source_map.operations must be an object",
            path="$.source_map.operations",
        )
    if set(operations_data) != set(source_operations):
        raise SolidBuildSpecError(
            "CAD-BACKEND-005",
            "operations and source_map.operations keys must match",
            path="$.source_map.operations",
        )

    operations: list[OperationSpec] = []
    source_ids: set[str] = set()
    for operation_id, operation_data in operations_data.items():
        if not isinstance(operation_id, str) or not operation_id:
            raise SolidBuildSpecError(
                "CAD-BACKEND-005", "operation ID must be a non-empty opaque key", path="$.operations"
            )
        path = f"$.operations[{operation_id!r}]"
        operation = _object(operation_data, path)
        kind = operation.get("type")
        source_id = _source(source_operations[operation_id], f"$.source_map.operations[{operation_id!r}]")
        if source_id in source_ids:
            raise SolidBuildSpecError(
                "CAD-BACKEND-005", "source_map operation IDs must be unique",
                path="$.source_map.operations", operation_id=operation_id,
            )
        source_ids.add(source_id)
        if kind == "cylindrical_cut":
            _keys(operation, {"type", "center", "radius", "z", "axis"}, path)
            if operation["axis"] != "positive_z":
                raise SolidBuildSpecError(
                    "CAD-BACKEND-002", "unsupported cylindrical cut axis", path=f"{path}.axis", operation_id=operation_id
                )
            center_data = _object(operation["center"], f"{path}.center")
            _keys(center_data, {"x", "y"}, f"{path}.center")
            center = Point2(
                _number(center_data["x"], f"{path}.center.x"),
                _number(center_data["y"], f"{path}.center.y"),
            )
            operations.append(CylindricalCutSpec(
                operation_id, source_id, center,
                _positive(operation["radius"], f"{path}.radius"),
                _interval(operation["z"], f"{path}.z"),
            ))
        elif kind == "slot_cut":
            _keys(operation, {"type", "profile", "z", "axis"}, path)
            if operation["axis"] != "positive_z":
                raise SolidBuildSpecError(
                    "CAD-BACKEND-002", "unsupported slot cut axis", path=f"{path}.axis", operation_id=operation_id
                )
            z = _interval(operation["z"], f"{path}.z")
            if z != body_z:
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004", "through slot interval must match the resolved body Z interval", path=f"{path}.z"
                )
            profile_data = _object(operation["profile"], f"{path}.profile")
            _keys(profile_data, {"segments"}, f"{path}.profile")
            segments = _closed_profile_segments(profile_data["segments"], f"{path}.profile")
            profile_bounds = _closed_profile_bounds(segments, z, f"{path}.profile")
            if (profile_bounds.x_min <= bounds.x_min or profile_bounds.x_max >= bounds.x_max
                    or profile_bounds.y_min <= bounds.y_min or profile_bounds.y_max >= bounds.y_max):
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004", "slot profile must lie strictly inside the body bounds", path=f"{path}.profile"
                )
            operations.append(SlotCutSpec(operation_id, source_id, ClosedProfileSpec(segments), z))
        else:
            raise SolidBuildSpecError(
                "CAD-BACKEND-002", "unsupported operation variant", path=f"{path}.type", operation_id=operation_id
            )

    return SolidBuildSpecV1(
        body=body_spec,
        operations=tuple(operations),
        body_source=body_source_decoded,
    )


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SolidBuildSpecError("CAD-BACKEND-004", f"{label} must be an object", path=label)
    return value


def _keys(value: Mapping[str, Any], expected: set[str], path: str) -> None:
    if set(value) != expected:
        raise SolidBuildSpecError("CAD-BACKEND-004", f"{path} has missing or unknown fields", path=path)


def _number(value: Any, path: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise SolidBuildSpecError("CAD-BACKEND-004", "number must be finite", path=path)
    return float(value)


def _positive(value: Any, path: str) -> float:
    number = _number(value, path)
    if number <= 0.0:
        raise SolidBuildSpecError("CAD-BACKEND-004", "number must be positive", path=path)
    return number


def _interval(value: Any, path: str, *, require_base: bool = False) -> ZInterval:
    data = _object(value, path)
    _keys(data, {"min", "max"}, path)
    minimum = _number(data["min"], f"{path}.min")
    maximum = _number(data["max"], f"{path}.max")
    if maximum <= minimum or (require_base and minimum != 0.0):
        raise SolidBuildSpecError("CAD-BACKEND-004", "interval is invalid", path=path)
    return ZInterval(minimum, maximum)


def _bounds(value: Any, path: str) -> Bounds3D:
    data = _object(value, path)
    _keys(data, {"min_x", "max_x", "min_y", "max_y", "min_z", "max_z"}, path)
    bounds = Bounds3D(
        _number(data["min_x"], f"{path}.min_x"),
        _number(data["max_x"], f"{path}.max_x"),
        _number(data["min_y"], f"{path}.min_y"),
        _number(data["max_y"], f"{path}.max_y"),
        _number(data["min_z"], f"{path}.min_z"),
        _number(data["max_z"], f"{path}.max_z"),
    )
    if bounds.x_max <= bounds.x_min or bounds.y_max <= bounds.y_min or bounds.z_max <= bounds.z_min:
        raise SolidBuildSpecError("CAD-BACKEND-004", "bounds must be strictly increasing", path=path)
    return bounds


def _point(value: Any, path: str) -> Point2:
    data = _object(value, path)
    _keys(data, {"x", "y"}, path)
    return Point2(_number(data["x"], f"{path}.x"), _number(data["y"], f"{path}.y"))


def _closed_profile_segments(value: Any, path: str) -> tuple[ProfileSegment, ...]:
    if not isinstance(value, list) or not value:
        raise SolidBuildSpecError(
            "CAD-BACKEND-004",
            "closed profile requires a non-empty segments array",
            path=f"{path}.segments",
        )
    segments: list[ProfileSegment] = []
    for index, raw_segment in enumerate(value):
        segment_path = f"{path}.segments[{index}]"
        segment = _object(raw_segment, segment_path)
        kind = segment.get("kind")
        if kind == "line":
            _keys(segment, {"kind", "start", "end"}, segment_path)
            segments.append(
                LineProfileSegment(
                    _point(segment["start"], f"{segment_path}.start"),
                    _point(segment["end"], f"{segment_path}.end"),
                )
            )
        elif kind == "arc":
            _keys(segment, {"kind", "start", "end", "center", "radius", "direction"}, segment_path)
            direction = segment["direction"]
            if direction not in {"clockwise", "counter_clockwise"}:
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004",
                    "closed profile arc direction is unsupported",
                    path=f"{segment_path}.direction",
                )
            segments.append(
                ArcProfileSegment(
                    _point(segment["start"], f"{segment_path}.start"),
                    _point(segment["end"], f"{segment_path}.end"),
                    _point(segment["center"], f"{segment_path}.center"),
                    _positive(segment["radius"], f"{segment_path}.radius"),
                    direction,
                )
            )
        else:
            raise SolidBuildSpecError(
                "CAD-BACKEND-002",
                "unsupported closed profile segment kind",
                path=f"{segment_path}.kind",
            )
    return tuple(segments)


def _closed_profile_bounds(
    segments: tuple[ProfileSegment, ...], z: ZInterval, path: str
) -> Bounds3D:
    min_x = math.inf
    max_x = -math.inf
    min_y = math.inf
    max_y = -math.inf
    area = 0.0
    previous_end: Point2 | None = None
    for index, segment in enumerate(segments):
        if isinstance(segment, LineProfileSegment):
            start, end = segment.start, segment.end
            if _distance(start, end) <= _TOLERANCE:
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004", "closed profile line segment is degenerate", path=f"{path}.segments[{index}]"
                )
            area += (start.x * end.y - start.y * end.x) / 2.0
            points = (start, end)
        else:
            start, end, center = segment.start, segment.end, segment.center
            if _distance(start, end) <= _TOLERANCE:
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004", "closed profile arc segment is degenerate", path=f"{path}.segments[{index}]"
                )
            if not _scale_close(_distance(center, start), segment.radius) or not _scale_close(
                _distance(center, end), segment.radius
            ):
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004", "closed profile arc endpoints do not match radius", path=f"{path}.segments[{index}]"
                )
            start_angle = math.atan2(start.y - center.y, start.x - center.x)
            end_angle = math.atan2(end.y - center.y, end.x - center.x)
            sweep = _directed_sweep(start_angle, end_angle, segment.direction)
            if sweep <= _TOLERANCE or sweep >= math.tau - _TOLERANCE:
                raise SolidBuildSpecError(
                    "CAD-BACKEND-004", "closed profile arc sweep is invalid", path=f"{path}.segments[{index}]"
                )
            signed_sweep = sweep if segment.direction == "counter_clockwise" else -sweep
            area += 0.5 * (
                center.x * (end.y - start.y)
                - center.y * (end.x - start.x)
                + segment.radius * segment.radius * signed_sweep
            )
            points = [start, end]
            for cardinal in (0.0, math.pi / 2.0, math.pi, 3.0 * math.pi / 2.0):
                if _angle_on_directed_arc(start_angle, end_angle, segment.direction, cardinal):
                    points.append(
                        Point2(
                            center.x + segment.radius * math.cos(cardinal),
                            center.y + segment.radius * math.sin(cardinal),
                        )
                    )
        if previous_end is not None and _distance(previous_end, start) > _TOLERANCE:
            raise SolidBuildSpecError(
                "CAD-BACKEND-004", "closed profile segments are disconnected", path=f"{path}.segments[{index}]"
            )
        previous_end = end
        for point in points:
            min_x = min(min_x, point.x)
            max_x = max(max_x, point.x)
            min_y = min(min_y, point.y)
            max_y = max(max_y, point.y)
    if previous_end is None or _distance(previous_end, segments[0].start) > _TOLERANCE:
        raise SolidBuildSpecError("CAD-BACKEND-004", "closed profile loop is not closed", path=path)
    if not math.isfinite(area) or abs(area) <= _TOLERANCE:
        raise SolidBuildSpecError("CAD-BACKEND-004", "closed profile loop has zero signed area", path=path)
    return Bounds3D(min_x, max_x, min_y, max_y, z.minimum, z.maximum)


def _distance(left: Point2, right: Point2) -> float:
    return math.hypot(left.x - right.x, left.y - right.y)


def _scale_close(left: float, right: float) -> bool:
    return math.isclose(left, right, rel_tol=1e-12, abs_tol=_TOLERANCE)


def _directed_sweep(start: float, end: float, direction: str) -> float:
    if direction == "counter_clockwise":
        return (end - start) % math.tau
    return (start - end) % math.tau


def _angle_on_directed_arc(start: float, end: float, direction: str, angle: float) -> bool:
    return _directed_sweep(start, angle, direction) <= _directed_sweep(start, end, direction) + _TOLERANCE


def _source(value: Any, path: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise SolidBuildSpecError("CAD-BACKEND-005", "source ID must be non-empty", path=path)
    return value


def _bounds_equal(left: Bounds3D, right: Bounds3D) -> bool:
    left_values = (left.x_min, left.x_max, left.y_min, left.y_max, left.z_min, left.z_max)
    right_values = (right.x_min, right.x_max, right.y_min, right.y_max, right.z_min, right.z_max)
    return all(
        math.isclose(a, b, rel_tol=1e-12, abs_tol=_TOLERANCE)
        for a, b in zip(left_values, right_values)
    )
