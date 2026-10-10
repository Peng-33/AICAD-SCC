"""Stateless CadQuery executor for SolidBuildSpec v1.

The worker consumes only the resolved physical JSON contract. It does not
interpret expressions, regenerate geometry, or reinterpret provenance IDs.
"""

from __future__ import annotations

import json
import math
import sys
from typing import Any, Mapping

import cadquery as cq
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeWire

from .solid_build_spec import (
    ArcProfileSegment,
    CircleBodySpec,
    ClosedProfileBodySpec,
    LineProfileSegment,
    RectangleBodySpec,
    RoundedRectangleBodySpec,
    SlotCutSpec,
    SolidBuildSpecError,
    SolidBuildSpecV1,
    parse_solid_build_spec,
)


def build_solid_from_spec(spec_json: Mapping[str, Any]) -> dict[str, Any]:
    """Build one ephemeral solid and return only measurable JSON metadata."""

    try:
        spec = parse_solid_build_spec(spec_json)
        shape = _build_body(spec)
        _validate_bbox(shape.BoundingBox(), spec.body.bounds)
        operation_results: list[dict[str, str]] = []
        for operation in spec.operations:
            try:
                before_volume = float(shape.Volume())
                candidate = _apply_cut(shape, operation)
                after_volume = float(candidate.Volume())
            except SolidBuildSpecError:
                raise
            except Exception as error:
                raise SolidBuildSpecError(
                    "CAD-BACKEND-006",
                    _safe_exception_message(error),
                    operation_id=operation.operation_id,
                ) from error
            delta = before_volume - after_volume
            status = "applied" if delta > max(1e-7, abs(before_volume) * 1e-12) else "no_material_change"
            shape = candidate
            operation_results.append({
                "operation_id": operation.operation_id,
                "source_id": operation.source_id,
                "status": status,
            })

        _validate_solid(shape)
        bounds = shape.BoundingBox()
        _validate_bbox(bounds, spec.body.bounds)
        volume = float(shape.Volume())
        return {
            "ok": True,
            "schema_version": 1,
            "valid": True,
            "body_count": 1,
            "solid_count": len(shape.Solids()),
            "operation_count": len(spec.operations),
            "bbox": {
                "x_min": _rounded(bounds.xmin),
                "x_max": _rounded(bounds.xmax),
                "y_min": _rounded(bounds.ymin),
                "y_max": _rounded(bounds.ymax),
                "z_min": _rounded(bounds.zmin),
                "z_max": _rounded(bounds.zmax),
            },
            "volume_mm3": _rounded(volume),
            "operation_results": operation_results,
            "body_source": dict(spec.body_source),
        }
    except SolidBuildSpecError as error:
        return {"ok": False, "error": error.as_error()}
    except Exception as error:  # CadQuery/OCP errors are protocol errors, never stdout tracebacks.
        return {
            "ok": False,
            "error": {
                "stage": "cad_backend",
                "code": "CAD-BACKEND-006",
                "message": _safe_exception_message(error),
            },
        }


def _build_body(spec: SolidBuildSpecV1) -> cq.Shape:
    body = spec.body
    height = body.z.maximum - body.z.minimum
    if isinstance(body, RectangleBodySpec):
        shape = cq.Solid.makeBox(
            body.width,
            body.height,
            height,
            cq.Vector(body.origin.x, body.origin.y, body.z.minimum),
        )
    elif isinstance(body, CircleBodySpec):
        shape = (
            cq.Workplane(
                "XY",
                origin=(body.center.x, body.center.y, body.z.minimum),
            )
            .circle(body.radius)
            .extrude(height)
            .val()
        )
    elif isinstance(body, RoundedRectangleBodySpec):
        shape = (
            cq.Workplane(
                "XY",
                origin=(body.center.x, body.center.y, body.z.minimum),
            )
            .rect(body.width, body.height)
            .extrude(height)
            .edges("|Z")
            .fillet(body.corner_radius)
            .val()
        )
    elif isinstance(body, ClosedProfileBodySpec):
        shape = _build_closed_profile(body)
    else:  # pragma: no cover - the DTO parser constructs only known variants.
        raise SolidBuildSpecError("CAD-BACKEND-002", "unsupported body DTO")
    _validate_solid(shape)
    return shape


def _build_closed_profile(body: ClosedProfileBodySpec) -> cq.Shape:
    return _build_profile_tool(body.segments, body.z)


def _build_profile_tool(segments: Any, z_interval: Any) -> cq.Shape:
    edges: list[cq.Edge] = []
    z = z_interval.minimum
    for segment in segments:
        if isinstance(segment, LineProfileSegment):
            edges.append(
                cq.Edge.makeLine(
                    cq.Vector(segment.start.x, segment.start.y, z),
                    cq.Vector(segment.end.x, segment.end.y, z),
                )
            )
        elif isinstance(segment, ArcProfileSegment):
            start_angle = math.atan2(
                segment.start.y - segment.center.y,
                segment.start.x - segment.center.x,
            )
            end_angle = math.atan2(
                segment.end.y - segment.center.y,
                segment.end.x - segment.center.x,
            )
            sweep = (
                (end_angle - start_angle) % math.tau
                if segment.direction == "counter_clockwise"
                else (start_angle - end_angle) % math.tau
            )
            midpoint_angle = start_angle + (
                sweep / 2.0 if segment.direction == "counter_clockwise" else -sweep / 2.0
            )
            midpoint = cq.Vector(
                segment.center.x + segment.radius * math.cos(midpoint_angle),
                segment.center.y + segment.radius * math.sin(midpoint_angle),
                z,
            )
            edges.append(
                cq.Edge.makeThreePointArc(
                    cq.Vector(segment.start.x, segment.start.y, z),
                    midpoint,
                    cq.Vector(segment.end.x, segment.end.y, z),
                )
            )
        else:  # pragma: no cover - the DTO parser constructs only known variants.
            raise SolidBuildSpecError("CAD-BACKEND-002", "unsupported closed profile segment")
    wire_builder = BRepBuilderAPI_MakeWire()
    for edge in edges:
        wire_builder.Add(edge.wrapped)
    if not wire_builder.IsDone():
        raise SolidBuildSpecError("CAD-BACKEND-007", "CadQuery could not assemble the closed profile wire")
    wire = cq.Wire(wire_builder.Wire())
    face = cq.Face.makeFromWires(wire)
    return cq.Solid.extrudeLinear(face, cq.Vector(0, 0, z_interval.maximum - z_interval.minimum))


def _apply_cut(shape: cq.Shape, operation: Any) -> cq.Shape:
    if isinstance(operation, SlotCutSpec):
        tool = _build_profile_tool(operation.profile.segments, operation.z)
    else:
        tool = cq.Solid.makeCylinder(
            operation.radius,
            operation.z.maximum - operation.z.minimum,
            cq.Vector(operation.center.x, operation.center.y, operation.z.minimum),
            cq.Vector(0, 0, 1),
        )
    candidate = shape.cut(tool).clean()
    _validate_solid(candidate, operation_id=operation.operation_id)
    return candidate


def _validate_solid(shape: cq.Shape, *, operation_id: str | None = None) -> None:
    if shape is None or not shape.isValid() or len(shape.Solids()) != 1:
        raise SolidBuildSpecError(
            "CAD-BACKEND-007",
            "CadQuery produced an invalid single solid",
            operation_id=operation_id,
        )
    bounds = shape.BoundingBox()
    values = (bounds.xmin, bounds.xmax, bounds.ymin, bounds.ymax, bounds.zmin, bounds.zmax, shape.Volume())
    if not all(_is_finite(value) for value in values) or shape.Volume() <= 0.0:
        raise SolidBuildSpecError(
            "CAD-BACKEND-007",
            "CadQuery produced non-finite or non-positive solid measurements",
            operation_id=operation_id,
        )


def _validate_bbox(bounds: Any, expected: Any) -> None:
    actual = (bounds.xmin, bounds.xmax, bounds.ymin, bounds.ymax, bounds.zmin, bounds.zmax)
    target = (expected.x_min, expected.x_max, expected.y_min, expected.y_max, expected.z_min, expected.z_max)
    if not all(abs(left - right) <= 1e-7 for left, right in zip(actual, target)):
        raise SolidBuildSpecError(
            "CAD-BACKEND-007",
            "CadQuery bbox does not match the SolidBuildSpec body bounds",
        )


def _is_finite(value: float) -> bool:
    return value == value and abs(value) != float("inf")


def _rounded(value: float) -> float:
    return round(float(value), 9)


def _safe_exception_message(error: Exception) -> str:
    message = str(error).replace("\n", " ").strip()
    return message[:512] or error.__class__.__name__


def main() -> int:
    try:
        payload = json.load(sys.stdin)
        response = build_solid_from_spec(payload)
    except Exception:
        response = {
            "ok": False,
            "error": {
                "stage": "cad_backend",
                "code": "CAD-BACKEND-004",
                "message": "request must be a valid JSON object",
            },
        }
    sys.stdout.write(json.dumps(response, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
