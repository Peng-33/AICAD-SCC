"""Stateless ordered Sketch + Extrude ADD/CUT rebuild using CadQuery/OCP.

The request is lowered from EngineeringModel by Rust. The worker owns shapes
only for this invocation and returns measurable, exact analytic/HLR evidence.
It does not serialize a second editable model or produce export artifacts.
"""

from __future__ import annotations

import hashlib
import json
import math
import sys
import time
from collections import Counter, defaultdict
from itertools import combinations
from typing import Any, Mapping

import cadquery as cq
from OCP.BRepAlgoAPI import BRepAlgoAPI_Section
from OCP.BRepAdaptor import BRepAdaptor_Curve, BRepAdaptor_Surface
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeWire
from OCP.BRepClass import BRepClass_FaceClassifier
from OCP.GCPnts import GCPnts_QuasiUniformDeflection
from OCP.GeomAbs import GeomAbs_BSplineCurve, GeomAbs_BezierCurve, GeomAbs_Circle, GeomAbs_Cone, GeomAbs_Cylinder, GeomAbs_Ellipse, GeomAbs_Line, GeomAbs_Plane, GeomAbs_Torus, GeomAbs_BSplineSurface
from OCP.HLRAlgo import HLRAlgo_Projector
from OCP.HLRBRep import HLRBRep_Algo, HLRBRep_HLRToShape
from OCP.ShapeAnalysis import ShapeAnalysis_FreeBounds
from OCP.TopAbs import TopAbs_EDGE, TopAbs_IN
from OCP.TopExp import TopExp_Explorer
from OCP.TopTools import TopTools_HSequenceOfShape
from OCP.gp import gp_Ax2, gp_Dir, gp_Pln, gp_Pnt

SCHEMA_VERSION = 6
TOLERANCE = 1e-7
SPATIAL_CURVE_MAX_DEVIATION_MM = 0.05
MAX_SPATIAL_CURVE_SAMPLE_POINTS = 512
MAX_SPATIAL_BSPLINE_FACE_SAMPLE_POINTS = 4096
HLR_CURVE_MAX_DEVIATION_MM = 0.01
VIEWS = {
    "front": ((0.0, 0.0, 1.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)),
    "top": ((0.0, 1.0, 0.0), (1.0, 0.0, 0.0), (0.0, 0.0, -1.0)),
    "left": ((-1.0, 0.0, 0.0), (0.0, 0.0, 1.0), (0.0, 1.0, 0.0)),
}


class RebuildError(ValueError):
    def __init__(self, code: str, message: str, feature_id: str | None = None, stage: str = "parametric_rebuild") -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.feature_id = feature_id
        self.stage = stage

    def as_error(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "stage": self.stage,
            "code": self.code,
            "message": self.message,
        }
        if self.feature_id is not None:
            result["operation_id"] = self.feature_id
        return result


def rebuild_parametric_model(value: Mapping[str, Any]) -> dict[str, Any]:
    try:
        request = _parse_request(value)
        result = _rebuild(request)
        return {"ok": True, "result": result}
    except RebuildError as error:
        return {"ok": False, "error": error.as_error()}
    except Exception as error:  # kernel faults are structured and never leak tracebacks
        message = str(error).replace("\n", " ").strip()[:512] or error.__class__.__name__
        return {
            "ok": False,
            "error": {
                "stage": "parametric_rebuild",
                "code": "BOOLEAN_FAILED",
                "message": message,
            },
        }


def section_parametric_model(value: Mapping[str, Any]) -> dict[str, Any]:
    """Return exact-solid section loops alongside ordinary rebuild evidence."""
    try:
        if not isinstance(value, Mapping) or set(value) != {"action", "rebuild", "section_plane"} or value.get("action") != "section":
            raise RebuildError("SECTION_REQUEST_INVALID", "section request fields are invalid", stage="section_geometry")
        request = _parse_request(value["rebuild"])
        plane = _parse_section_plane(value["section_plane"])
        result = _rebuild(request, section_plane=plane)
        return {"ok": True, "result": result}
    except RebuildError as error:
        return {"ok": False, "error": error.as_error()}
    except Exception as error:  # kernel faults remain structured and fail closed
        message = str(error).replace("\n", " ").strip()[:512] or error.__class__.__name__
        return {"ok": False, "error": {"stage": "section_geometry", "code": "SECTION_GEOMETRY_FAILED", "message": message}}


def _parse_section_plane(value: Any) -> dict[str, Any]:
    required = {"axis", "offset_mm", "view_side"}
    if not isinstance(value, Mapping) or set(value) != required:
        raise RebuildError("SECTION_REQUEST_INVALID", "section plane fields are invalid", stage="section_geometry")
    axis, offset, side = value["axis"], value["offset_mm"], value["view_side"]
    if axis not in {"x", "y", "z"} or side not in {"positive", "negative"}:
        raise RebuildError("SECTION_REQUEST_INVALID", "section axis or viewing direction is unsupported", stage="section_geometry")
    if not isinstance(offset, (int, float)) or isinstance(offset, bool) or not math.isfinite(offset):
        raise RebuildError("SECTION_REQUEST_INVALID", "section offset must be finite millimetres", stage="section_geometry")
    sign = 1.0 if side == "positive" else -1.0
    if axis == "x":
        normal, u_axis, v_axis = (sign, 0.0, 0.0), (0.0, 0.0, -sign), (0.0, 1.0, 0.0)
    elif axis == "y":
        normal, u_axis, v_axis = (0.0, sign, 0.0), (1.0, 0.0, 0.0), (0.0, 0.0, -sign)
    else:
        normal, u_axis, v_axis = (0.0, 0.0, sign), (1.0, 0.0, 0.0), (0.0, sign, 0.0)
    origin = [0.0, 0.0, 0.0]
    origin[{"x": 0, "y": 1, "z": 2}[axis]] = float(offset)
    return {"axis": axis, "offset_mm": float(offset), "view_side": side, "origin": origin, "normal": normal, "u_axis": u_axis, "v_axis": v_axis}


def parse_parametric_request(value: Mapping[str, Any]) -> dict[str, Any]:
    """Decode the strict versioned operation contract for shared consumers."""

    return _parse_request(value)


def _parse_request(value: Mapping[str, Any]) -> dict[str, Any]:
    if not isinstance(value, Mapping) or set(value) != {"schema_version", "model_sha256", "operations"}:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "request fields are invalid")
    if value["schema_version"] != SCHEMA_VERSION:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "unsupported rebuild schema version")
    digest = value["model_sha256"]
    if not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "model identity is invalid")
    operations = value["operations"]
    if not isinstance(operations, list) or not operations:
        raise RebuildError("NO_BODY", "feature history contains no active operations")
    ids: set[str] = set()
    available_sources: set[str] = set()
    decoded = []
    for envelope in operations:
        if not isinstance(envelope, Mapping) or set(envelope) != {"kind", "data"} or not isinstance(envelope["data"], Mapping):
            raise RebuildError("REBUILD_DEPENDENCY_ERROR", "operation envelope fields are invalid")
        kind, data = envelope["kind"], envelope["data"]
        if kind == "extrude":
            operation = _parse_extrude(data)
        elif kind == "sweep":
            operation = _parse_sweep(data)
        elif kind == "loft":
            operation = _parse_loft(data)
        elif kind == "countersink":
            operation = _parse_countersink(data)
        elif kind == "revolve":
            operation = _parse_revolve(data)
        elif kind in {"fillet", "chamfer"}:
            required = {"feature_id", "edge_refs", "radius" if kind == "fillet" else "distance"}
            if set(data) != required:
                raise RebuildError("REBUILD_DEPENDENCY_ERROR", "edge treatment fields are invalid")
            feature_id = data["feature_id"]
            if not isinstance(feature_id, str) or not feature_id:
                raise RebuildError("REBUILD_DEPENDENCY_ERROR", "edge treatment feature id is invalid")
            dimension_name = "radius" if kind == "fillet" else "distance"
            dimension = data[dimension_name]
            code = "INVALID_FILLET_RADIUS" if kind == "fillet" else "INVALID_CHAMFER_DISTANCE"
            if not isinstance(dimension, (int, float)) or isinstance(dimension, bool) or not math.isfinite(dimension) or dimension <= 0:
                raise RebuildError(code, f"{dimension_name} must be finite and positive", feature_id)
            edge_values = data["edge_refs"]
            if not isinstance(edge_values, list) or not edge_values:
                raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "at least one semantic edge reference is required", feature_id)
            edge_refs = [_parse_edge_reference(item, feature_id) for item in edge_values]
            referenced = {ref["source_feature_id"] for ref in edge_refs}
            for ref in edge_refs:
                for face_key in ("first", "second"):
                    face = ref["selector"].get(face_key)
                    if face is not None:
                        referenced.add(face["source_feature_id"])
            if not referenced.issubset(available_sources):
                raise RebuildError("FEATURE_DEPENDENCY_ERROR", "edge treatment references a missing or later feature", feature_id)
            if len({json.dumps(ref, sort_keys=True) for ref in edge_refs}) != len(edge_refs):
                raise RebuildError("DUPLICATE_EDGE_REFERENCE", "semantic edge references must be unique", feature_id)
            operation = {"kind": kind, "feature_id": feature_id, "edge_refs": edge_refs, dimension_name: float(dimension), "source_feature_id": feature_id}
        else:
            raise RebuildError("REBUILD_DEPENDENCY_ERROR", "operation kind is unsupported")
        feature_id = operation["feature_id"]
        if operation["kind"] == "extrude" and operation.get("support_face") is not None:
            if operation["support_face"]["source_feature_id"] not in available_sources:
                raise RebuildError("FEATURE_DEPENDENCY_ERROR", "extrude support face references a missing or later source feature", feature_id)
        if operation["kind"] == "countersink":
            if operation["source_feature_id"] not in available_sources:
                raise RebuildError("FEATURE_DEPENDENCY_ERROR", "countersink must follow its source Hole main-bore operation", feature_id)
            support_face = operation["support_face"]
            if support_face is not None and support_face["source_feature_id"] not in available_sources:
                raise RebuildError("FEATURE_DEPENDENCY_ERROR", "countersink references a missing or later support face", feature_id)
        if feature_id in ids:
            raise RebuildError("REBUILD_DEPENDENCY_ERROR", "feature IDs are duplicated", feature_id)
        ids.add(feature_id)
        decoded.append(operation)
        if operation["kind"] in {"extrude", "sweep", "loft", "countersink"}:
            available_sources.add(operation["source_feature_id"])
        else:
            available_sources.add(feature_id)
    first = decoded[0]
    if first["kind"] not in {"extrude", "revolve", "sweep", "loft"} or first["operation"] != "add":
        raise RebuildError("NO_BODY", "the first material operation must be an ADD feature")
    return {"model_sha256": digest, "operations": decoded}


def _parse_extrude(operation: Mapping[str, Any]) -> dict[str, Any]:
    required = {"feature_id", "sketch_id", "operation", "plane", "loops", "profile_entity_ids", "direction", "extent"}
    optional = {"source_feature_id", "source_feature_kind", "support_face", "blind_hole"}
    if not required.issubset(operation) or set(operation) - required - optional:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "extrude operation fields are invalid")
    feature_id, sketch_id = operation["feature_id"], operation["sketch_id"]
    if not isinstance(feature_id, str) or not feature_id or not isinstance(sketch_id, str) or not sketch_id:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "feature or sketch ID is invalid")
    plane = _parse_plane(operation["plane"], feature_id)
    loops = _parse_loops(operation["loops"], feature_id)
    entity_ids = operation["profile_entity_ids"]
    if not isinstance(entity_ids, list) or len(entity_ids) != len(loops) or any(not isinstance(loop, list) or len(loop) != len(curves) for loop, curves in zip(entity_ids, loops)):
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "profile entity identities do not match sketch loops", feature_id)
    flat_ids = [item for loop in entity_ids for item in loop]
    if any(not isinstance(item, str) or not item for item in flat_ids) or len(set(flat_ids)) != len(flat_ids):
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "profile entity identities are empty or duplicated", feature_id)
    direction = _unit_vector(operation["direction"], "extrude direction", feature_id)
    if abs(abs(_dot(direction, plane["normal"])) - 1.0) > 1e-8:
        raise RebuildError("INVALID_EXTRUDE_EXTENT", "extrusion direction must be normal to its sketch plane", feature_id)
    extent = operation["extent"]
    if not isinstance(extent, Mapping) or not isinstance(extent.get("kind"), str):
        raise RebuildError("INVALID_EXTRUDE_EXTENT", "extent must contain a kind", feature_id)
    extent_kind = extent["kind"]
    if extent_kind == "distance" and set(extent) == {"kind", "length"}:
        extent_value = extent["length"]
        if not isinstance(extent_value, (float, int)) or isinstance(extent_value, bool) or not math.isfinite(extent_value) or extent_value <= 0:
            if operation.get("blind_hole") is True:
                raise RebuildError("INVALID_HOLE_DEPTH", "blind hole depth must be finite and positive", feature_id)
            raise RebuildError("INVALID_EXTRUDE_EXTENT", "extrude distance must be finite and positive", feature_id)
        extent_value = {"kind": "distance", "length": float(extent_value)}
    elif extent_kind == "symmetric" and set(extent) == {"kind", "length"}:
        extent_value = extent["length"]
        if not isinstance(extent_value, (float, int)) or isinstance(extent_value, bool) or not math.isfinite(extent_value) or extent_value <= 0:
            raise RebuildError("INVALID_EXTRUDE_EXTENT", "symmetric extent must be finite and positive", feature_id)
        extent_value = {"kind": "symmetric", "length": float(extent_value)}
    elif extent_kind == "through_all" and set(extent) == {"kind"}:
        extent_value = {"kind": "through_all"}
    else:
        raise RebuildError("INVALID_EXTRUDE_EXTENT", "extent kind or fields are unsupported", feature_id)
    operation_kind = operation["operation"]
    if operation_kind not in {"add", "cut"}:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "boolean operation is unsupported", feature_id)
    if extent_value["kind"] == "through_all" and operation_kind != "cut":
        raise RebuildError("INVALID_EXTRUDE_EXTENT", "THROUGH_ALL is supported only for CUT in this phase", feature_id)
    if not isinstance(operation.get("blind_hole", False), bool):
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "blind_hole must be boolean", feature_id)
    return {
        "kind": "extrude", "feature_id": feature_id, "sketch_id": sketch_id,
        "operation": operation_kind, "plane": plane, "loops": loops,
        "profile_entity_ids": entity_ids, "direction": direction, "extent": extent_value,
        "source_feature_id": operation.get("source_feature_id") or feature_id,
        "source_feature_kind": operation.get("source_feature_kind", "extrude"),
        "support_face": _parse_face_reference(operation.get("support_face"), feature_id),
        "blind_hole": operation.get("blind_hole", False),
    }


def _parse_sweep(operation: Mapping[str, Any]) -> dict[str, Any]:
    required = {
        "feature_id", "sketch_id", "path_id", "operation", "plane", "loops", "profile_entity_ids",
        "profile_origin", "path_plane", "path", "path_entity_ids", "direction", "orientation",
    }
    if not isinstance(operation, Mapping) or set(operation) != required:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "Sweep operation fields are invalid")
    feature_id, sketch_id, path_id = operation["feature_id"], operation["sketch_id"], operation["path_id"]
    if not isinstance(feature_id, str) or not feature_id or not isinstance(sketch_id, str) or not sketch_id or not isinstance(path_id, str) or not path_id:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "Sweep feature, Sketch, or Path identity is invalid")
    plane = _parse_plane(operation["plane"], feature_id)
    loops = _parse_loops(operation["loops"], feature_id)
    entity_ids = operation["profile_entity_ids"]
    if not isinstance(entity_ids, list) or len(entity_ids) != len(loops) or any(
        not isinstance(ids, list) or len(ids) != len(curves)
        for ids, curves in zip(entity_ids, loops)
    ):
        raise RebuildError("SWEEP_PROFILE_INVALID", "profile entity identities do not match Sketch loops", feature_id)
    flat_ids = [item for ids in entity_ids for item in ids]
    if any(not isinstance(item, str) or not item for item in flat_ids) or len(set(flat_ids)) != len(flat_ids):
        raise RebuildError("SWEEP_PROFILE_INVALID", "profile entity identities are empty or duplicated", feature_id)
    profile_origin = _vec2(operation["profile_origin"], feature_id)
    path_plane = _parse_plane(operation["path_plane"], feature_id)
    path = _parse_path_curves(operation["path"], feature_id)
    path_entity_ids = operation["path_entity_ids"]
    if not isinstance(path_entity_ids, list) or len(path_entity_ids) != len(path) or any(not isinstance(item, str) or not item for item in path_entity_ids) or len(set(path_entity_ids)) != len(path_entity_ids):
        raise RebuildError("PATH_INVALID", "Path segment identities do not match Path geometry", feature_id)
    boolean_operation = operation["operation"]
    direction = operation["direction"]
    orientation = operation["orientation"]
    if boolean_operation not in {"add", "cut"} or direction not in {"forward", "reverse"}:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "Sweep operation or direction is unsupported", feature_id)
    if orientation != "fixed_binormal":
        raise RebuildError("SWEEP_ORIENTATION_UNSUPPORTED", "only the documented planar fixed-binormal orientation is supported", feature_id)
    _validate_sweep_frame(plane, profile_origin, path_plane, path, direction, feature_id)
    return {
        "kind": "sweep", "feature_id": feature_id, "sketch_id": sketch_id, "path_id": path_id,
        "operation": boolean_operation, "plane": plane, "loops": loops,
        "profile_entity_ids": entity_ids, "profile_origin": profile_origin,
        "path_plane": path_plane, "path": path, "path_entity_ids": path_entity_ids,
        "direction": direction, "orientation": orientation, "source_feature_id": feature_id,
    }


def _parse_loft(operation: Mapping[str, Any]) -> dict[str, Any]:
    required = {"feature_id", "operation", "mode", "orientation_policy", "sections"}
    if not isinstance(operation, Mapping) or set(operation) != required:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "Loft operation fields are invalid")
    feature_id = operation["feature_id"]
    sections_value = operation["sections"]
    if not isinstance(feature_id, str) or not feature_id:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "Loft feature identity is invalid")
    if not isinstance(sections_value, list) or not 2 <= len(sections_value) <= 32:
        raise RebuildError("LOFT_SECTION_COUNT_INVALID", "Loft requires 2 to 32 ordered Sketch sections", feature_id)
    if operation["operation"] not in {"add", "cut"} or operation["mode"] != "ruled":
        raise RebuildError("LOFT_MODE_UNSUPPORTED", "only explicit RULED Loft ADD/CUT is supported", feature_id)
    if operation["orientation_policy"] != "aligned":
        raise RebuildError("LOFT_ORIENTATION_UNSUPPORTED", "only aligned, co-oriented section frames are supported", feature_id)

    sections = []
    sketch_ids = set()
    signature = None
    orientation = 0
    first_plane = None
    previous_offset = 0.0
    for index, value in enumerate(sections_value):
        fields = {"sketch_id", "plane", "loops", "profile_entity_ids"}
        if not isinstance(value, Mapping) or set(value) != fields:
            raise RebuildError("LOFT_PROFILE_INVALID", "Loft section fields are invalid", feature_id)
        sketch_id = value["sketch_id"]
        if not isinstance(sketch_id, str) or not sketch_id or sketch_id in sketch_ids:
            raise RebuildError("LOFT_SECTION_REFERENCE_INVALID", "Loft Sketch references must be unique and nonempty", feature_id)
        sketch_ids.add(sketch_id)
        plane = _parse_plane(value["plane"], feature_id)
        loops = _parse_loops(value["loops"], feature_id)
        if len(loops) != 1:
            raise RebuildError("LOFT_INNER_LOOPS_UNSUPPORTED", "Loft currently requires exactly one outer loop; inner-loop correspondence is deferred", feature_id)
        entity_ids = value["profile_entity_ids"]
        if not isinstance(entity_ids, list) or len(entity_ids) != 1 or not isinstance(entity_ids[0], list) or len(entity_ids[0]) != len(loops[0]):
            raise RebuildError("LOFT_PROFILE_INVALID", "Loft profile entity identities do not match its outer loop", feature_id)
        flat_ids = entity_ids[0]
        if any(not isinstance(item, str) or not item for item in flat_ids) or len(set(flat_ids)) != len(flat_ids):
            raise RebuildError("LOFT_PROFILE_INVALID", "Loft profile entity identities must be unique and nonempty", feature_id)
        current_signature = tuple(curve["kind"] for curve in loops[0])
        if signature is not None and signature != current_signature:
            raise RebuildError("LOFT_PROFILE_INCOMPATIBLE", "ordered section curves do not have one-to-one Line/Arc/Circle correspondence", feature_id)
        signature = current_signature
        area = _signed_loop_area(loops[0])
        if not math.isfinite(area) or abs(area) <= TOLERANCE:
            raise RebuildError("LOFT_PROFILE_DEGENERATE", "Loft section outer loop has degenerate area", feature_id)
        sign = 1 if area > 0 else -1
        if orientation and sign != orientation:
            raise RebuildError("LOFT_PROFILE_ORIENTATION_INVALID", "Loft section outer loops have inconsistent directions", feature_id)
        orientation = sign
        if first_plane is None:
            first_plane = plane
        else:
            close = lambda left, right: all(abs(a-b) <= 1.0e-8 for a,b in zip(left,right))
            if not close(plane["normal"], first_plane["normal"]) or not close(plane["u_axis"], first_plane["u_axis"]) or not close(plane["v_axis"], first_plane["v_axis"]):
                raise RebuildError("LOFT_ORIENTATION_UNSUPPORTED", "Loft sections must be parallel, co-oriented, and use the same local axes", feature_id)
            delta = tuple(plane["origin"][i] - first_plane["origin"][i] for i in range(3))
            offset = _dot(delta, first_plane["normal"])
            lateral = math.sqrt(sum((delta[i] - first_plane["normal"][i]*offset)**2 for i in range(3)))
            if lateral > 1.0e-7 or (index > 0 and offset <= previous_offset + 1.0e-7):
                raise RebuildError("LOFT_SECTION_ORDER_INVALID", "Loft sections must advance strictly along one axis without lateral plane offsets", feature_id)
            previous_offset = offset
        sections.append({
            "sketch_id": sketch_id, "plane": plane, "loops": loops,
            "profile_entity_ids": entity_ids,
        })
    return {
        "kind": "loft", "feature_id": feature_id, "operation": operation["operation"],
        "mode": operation["mode"], "orientation_policy": operation["orientation_policy"],
        "sections": sections, "source_feature_id": feature_id,
    }


def _signed_loop_area(curves: list[dict[str, Any]]) -> float:
    area = 0.0
    for curve in curves:
        if curve["kind"] == "line":
            start, end = curve["start"], curve["end"]
            area += (start[0]*end[1] - start[1]*end[0]) * 0.5
        elif curve["kind"] == "circle":
            area += math.pi * curve["radius"] ** 2
        else:
            start, end, center, radius = curve["start"], curve["end"], curve["center"], curve["radius"]
            a0 = math.atan2(start[1]-center[1], start[0]-center[0])
            a1 = math.atan2(end[1]-center[1], end[0]-center[0])
            delta = (a1-a0) % math.tau if curve["direction"] == "counter_clockwise" else -((a0-a1) % math.tau)
            area += 0.5 * (radius*center[0]*(math.sin(a1)-math.sin(a0))
                - radius*center[1]*(math.cos(a1)-math.cos(a0)) + radius*radius*delta)
    return area


def _parse_path_curves(value: Any, feature_id: str) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not value:
        raise RebuildError("PATH_EMPTY", "Path must contain one or more Line/Arc segments", feature_id)
    result = []
    for curve in value:
        if not isinstance(curve, Mapping) or not isinstance(curve.get("kind"), str):
            raise RebuildError("PATH_INVALID", "Path segment is invalid", feature_id)
        kind = curve["kind"]
        if kind == "line" and set(curve) == {"kind", "start", "end"}:
            start, end = _vec2(curve["start"], feature_id), _vec2(curve["end"], feature_id)
            if _distance2(start, end) <= TOLERANCE:
                raise RebuildError("PATH_DEGENERATE", "Path contains a zero-length Line", feature_id)
            result.append({"kind": "line", "start": start, "end": end})
        elif kind == "arc" and set(curve) == {"kind", "start", "end", "center", "radius", "direction"}:
            start, end, center = _vec2(curve["start"], feature_id), _vec2(curve["end"], feature_id), _vec2(curve["center"], feature_id)
            radius, arc_direction = curve["radius"], curve["direction"]
            if not isinstance(radius, (int, float)) or isinstance(radius, bool) or not math.isfinite(radius) or radius <= 0 or arc_direction not in {"counter_clockwise", "clockwise"}:
                raise RebuildError("PATH_DEGENERATE", "Path Arc radius or direction is invalid", feature_id)
            if abs(_distance2(start, center) - radius) > TOLERANCE or abs(_distance2(end, center) - radius) > TOLERANCE or _distance2(start, end) <= TOLERANCE:
                raise RebuildError("PATH_DEGENERATE", "Path Arc endpoints do not match a non-degenerate radius", feature_id)
            result.append({"kind": "arc", "start": start, "end": end, "center": center, "radius": float(radius), "direction": arc_direction})
        else:
            raise RebuildError("PATH_UNSUPPORTED_CURVE", "Path supports Line and Arc segments only", feature_id)
    for index, curve in enumerate(result):
        if index + 1 >= len(result):
            continue
        following = result[index + 1]
        if _distance2(_curve_end(curve), _curve_start(following)) > 1.0e-8:
            raise RebuildError("PATH_DISCONTINUOUS", "Path segments must connect in order", feature_id)
        if _distance2(_curve_tangent(curve, False), _curve_tangent(following, True)) > 1.0e-8:
            raise RebuildError("PATH_NON_TANGENT_JOIN", "Path joins must be C1 tangent-continuous; sharp corners are unsupported", feature_id)
    if _distance2(_curve_start(result[0]), _curve_end(result[-1])) <= 1.0e-8:
        raise RebuildError("PATH_CLOSED_UNSUPPORTED", "Path must be open", feature_id)
    _validate_path_self_intersection(result, feature_id)
    return result


def _curve_tangent(curve: Mapping[str, Any], at_start: bool) -> tuple[float, float]:
    if curve["kind"] == "line":
        start, end = curve["start"], curve["end"]
        delta = (end[0] - start[0], end[1] - start[1])
    else:
        point = curve["start"] if at_start else curve["end"]
        sign = 1.0 if curve["direction"] == "counter_clockwise" else -1.0
        delta = (-sign * (point[1] - curve["center"][1]), sign * (point[0] - curve["center"][0]))
    length = math.hypot(*delta)
    if length <= TOLERANCE:
        raise RebuildError("PATH_DEGENERATE", "Path tangent is degenerate")
    return delta[0] / length, delta[1] / length


def _point_on_arc(curve: Mapping[str, Any], point: tuple[float, float]) -> bool:
    start_angle = math.atan2(curve["start"][1] - curve["center"][1], curve["start"][0] - curve["center"][0])
    end_angle = math.atan2(curve["end"][1] - curve["center"][1], curve["end"][0] - curve["center"][0])
    angle = math.atan2(point[1] - curve["center"][1], point[0] - curve["center"][0])
    if curve["direction"] == "counter_clockwise":
        sweep, offset = (end_angle - start_angle) % math.tau, (angle - start_angle) % math.tau
    else:
        sweep, offset = (start_angle - end_angle) % math.tau, (start_angle - angle) % math.tau
    return offset <= sweep + 1.0e-10


def _path_curve_intersections(first: Mapping[str, Any], second: Mapping[str, Any]) -> list[tuple[float, float]]:
    a_line, b_line = first["kind"] == "line", second["kind"] == "line"
    if a_line and b_line:
        p, p2, q, q2 = first["start"], first["end"], second["start"], second["end"]
        r, d = (p2[0]-p[0],p2[1]-p[1]), (q2[0]-q[0],q2[1]-q[1])
        qp = (q[0]-p[0],q[1]-p[1]); cross=lambda x,y:x[0]*y[1]-x[1]*y[0]
        den=cross(r,d)
        if abs(den)>1e-12:
            t,u=cross(qp,d)/den,cross(qp,r)/den
            return [(p[0]+t*r[0],p[1]+t*r[1])] if -1e-10<=t<=1+1e-10 and -1e-10<=u<=1+1e-10 else []
        if abs(cross(qp,r))>1e-10: return []
        return [point for point in (p,p2,q,q2) if _point_on_line(point,p,p2) and _point_on_line(point,q,q2)]
    if a_line != b_line:
        line, arc = (first, second) if a_line else (second, first)
        p,p2,c=line["start"],line["end"],arc["center"]
        dx,dy=p2[0]-p[0],p2[1]-p[1]; fx,fy=p[0]-c[0],p[1]-c[1]
        aa=dx*dx+dy*dy; bb=2*(fx*dx+fy*dy); cc=fx*fx+fy*fy-arc["radius"]**2
        disc=bb*bb-4*aa*cc
        if disc < -1e-10:return []
        root=math.sqrt(max(0.0,disc)); out=[]
        for t in ((-bb-root)/(2*aa),(-bb+root)/(2*aa)):
            point=(p[0]+t*dx,p[1]+t*dy)
            if -1e-10<=t<=1+1e-10 and _point_on_arc(arc,point):out.append(point)
        return out
    c1,c2=first["center"],second["center"]; r1,r2=first["radius"],second["radius"]
    dx,dy=c2[0]-c1[0],c2[1]-c1[1]; d=math.hypot(dx,dy)
    if d<=1e-8 and abs(r1-r2)<=1e-8:
        points=[first["start"],first["end"],second["start"],second["end"]]
        return [p for p in points if _point_on_arc(first,p) and _point_on_arc(second,p)]
    if d<=1e-8 or d>r1+r2+1e-8 or d<abs(r1-r2)-1e-8:return []
    along=(r1*r1-r2*r2+d*d)/(2*d); h2=r1*r1-along*along
    if h2 < -1e-8:return []
    h=math.sqrt(max(0.0,h2)); bx,by=c1[0]+along*dx/d,c1[1]+along*dy/d
    points=((bx-h*dy/d,by+h*dx/d),(bx+h*dy/d,by-h*dx/d))
    return [p for p in points if _point_on_arc(first,p) and _point_on_arc(second,p)]


def _point_on_line(point: tuple[float, float], start: tuple[float, float], end: tuple[float, float]) -> bool:
    dx,dy=end[0]-start[0],end[1]-start[1]; length2=dx*dx+dy*dy
    if length2<=TOLERANCE*TOLERANCE:return _distance2(point,start)<=TOLERANCE
    t=((point[0]-start[0])*dx+(point[1]-start[1])*dy)/length2
    return -1e-10<=t<=1+1e-10 and _distance2(point,(start[0]+t*dx,start[1]+t*dy))<=1e-8


def _validate_path_self_intersection(curves: list[dict[str, Any]], feature_id: str) -> None:
    for i, first in enumerate(curves):
        for j in range(i+1,len(curves)):
            allowed = _curve_end(first) if j==i+1 else None
            for point in _path_curve_intersections(first,curves[j]):
                if allowed is None or _distance2(point,allowed)>1e-8:
                    raise RebuildError("PATH_SELF_INTERSECTION", "Path segments cross or overlap away from their adjacent join", feature_id)


def _reverse_path_curves(curves: list[dict[str, Any]]) -> list[dict[str, Any]]:
    result=[]
    for source in reversed(curves):
        curve=dict(source); curve["start"],curve["end"]=source["end"],source["start"]
        if curve["kind"]=="arc":curve["direction"]="clockwise" if source["direction"]=="counter_clockwise" else "counter_clockwise"
        result.append(curve)
    return result


def _validate_sweep_frame(plane: Mapping[str, tuple[float,float,float]], profile_origin: tuple[float,float], path_plane: Mapping[str, tuple[float,float,float]], curves: list[dict[str,Any]], direction: str, feature_id: str) -> None:
    directed = curves if direction=="forward" else _reverse_path_curves(curves)
    start = _curve_start(directed[0]); tangent2=_curve_tangent(directed[0],True)
    tangent=tuple(path_plane["u_axis"][i]*tangent2[0]+path_plane["v_axis"][i]*tangent2[1] for i in range(3))
    binormal=path_plane["normal"]
    cross=lambda a,b:(a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0])
    expected_u=cross(binormal,tangent)
    path_point=tuple(path_plane["origin"][i]+path_plane["u_axis"][i]*start[0]+path_plane["v_axis"][i]*start[1] for i in range(3))
    profile_point=tuple(plane["origin"][i]+plane["u_axis"][i]*profile_origin[0]+plane["v_axis"][i]*profile_origin[1] for i in range(3))
    close=lambda a,b:max(abs(a[i]-b[i]) for i in range(3))<=1e-7
    if not close(path_point,profile_point) or not close(tangent,plane["normal"]) or not close(expected_u,plane["u_axis"]) or not close(binormal,plane["v_axis"]):
        raise RebuildError("SWEEP_PROFILE_ORIENTATION_INVALID", "Sketch anchor and axes must match the directed Path endpoint, tangent, and fixed-binormal frame", feature_id)


def _parse_countersink(operation: Mapping[str, Any]) -> dict[str, Any]:
    required = {
        "feature_id", "source_feature_id", "source_feature_kind", "plane", "direction",
        "main_diameter", "mouth_diameter", "included_angle_degrees",
    }
    optional = {"support_face"}
    if not required.issubset(operation) or set(operation) - required - optional:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "countersink operation fields are invalid")
    feature_id = operation["feature_id"]
    source_feature_id = operation["source_feature_id"]
    source_feature_kind = operation["source_feature_kind"]
    if not isinstance(feature_id, str) or not feature_id or not isinstance(source_feature_id, str) or not source_feature_id:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "countersink operation identity is invalid")
    if not isinstance(source_feature_kind, str) or source_feature_kind not in {"hole", "pattern:linear", "pattern:circular", "pattern:explicit_placement"}:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "countersink source feature kind is invalid", feature_id)
    plane = _parse_plane(operation["plane"], feature_id)
    direction = _unit_vector(operation["direction"], "countersink direction", feature_id)
    if abs(abs(_dot(direction, plane["normal"])) - 1.0) > 1.0e-8:
        raise RebuildError("INVALID_COUNTERSINK", "countersink direction must be normal to its support plane", feature_id)
    main_diameter = operation["main_diameter"]
    mouth_diameter = operation["mouth_diameter"]
    angle = operation["included_angle_degrees"]
    if any(not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) for value in (main_diameter, mouth_diameter, angle)):
        raise RebuildError("INVALID_COUNTERSINK", "countersink dimensions must be finite", feature_id)
    if main_diameter <= 0 or mouth_diameter <= main_diameter or not 0 < angle < 180:
        raise RebuildError("INVALID_COUNTERSINK", "countersink needs a larger mouth diameter and an included angle between 0 and 180 degrees", feature_id)
    depth = (float(mouth_diameter) - float(main_diameter)) / (2.0 * math.tan(math.radians(float(angle)) / 2.0))
    if not math.isfinite(depth) or depth <= 1.0e-7:
        raise RebuildError("INVALID_COUNTERSINK", "derived countersink depth must be finite and exceed OCP tolerance", feature_id)
    return {
        "kind": "countersink", "feature_id": feature_id,
        "source_feature_id": source_feature_id, "source_feature_kind": source_feature_kind,
        "plane": plane, "direction": direction,
        "main_diameter": float(main_diameter), "mouth_diameter": float(mouth_diameter),
        "included_angle_degrees": float(angle), "depth": depth,
        "support_face": _parse_face_reference(operation.get("support_face"), feature_id),
    }


def _parse_revolve(operation: Mapping[str, Any]) -> dict[str, Any]:
    required = {
        "feature_id", "sketch_id", "operation", "plane", "loops",
        "profile_entity_ids", "axis_origin", "direction", "angle", "orientation",
    }
    if set(operation) != required:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "revolve operation fields are invalid")
    feature_id, sketch_id = operation["feature_id"], operation["sketch_id"]
    if not isinstance(feature_id, str) or not feature_id or not isinstance(sketch_id, str) or not sketch_id:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "revolve feature or sketch ID is invalid")
    plane = _parse_plane(operation["plane"], feature_id)
    loops = _parse_loops(operation["loops"], feature_id)
    entity_ids = operation["profile_entity_ids"]
    if not isinstance(entity_ids, list) or len(entity_ids) != len(loops) or any(
        not isinstance(loop, list) or len(loop) != len(curves)
        for loop, curves in zip(entity_ids, loops)
    ):
        raise RebuildError("REVOLVE_PROFILE_INVALID", "profile entity identities do not match sketch loops", feature_id)
    flat_ids = [item for loop in entity_ids for item in loop]
    if any(not isinstance(item, str) or not item for item in flat_ids) or len(set(flat_ids)) != len(flat_ids):
        raise RebuildError("REVOLVE_PROFILE_INVALID", "profile entity identities are empty or duplicated", feature_id)
    axis_origin = _vec3(operation["axis_origin"], "axis origin", feature_id)
    direction = _vec3(operation["direction"], "axis direction", feature_id)
    magnitude = math.sqrt(_dot(direction, direction))
    if not math.isfinite(magnitude) or abs(magnitude - 1.0) > 1.0e-8:
        raise RebuildError("INVALID_DATUM_AXIS", "resolved axis direction must be a normalized finite vector", feature_id)
    material_operation = operation["operation"]
    if material_operation not in {"add", "cut"}:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "revolve operation must be add or cut", feature_id)
    angle_value = operation["angle"]
    if not isinstance(angle_value, Mapping):
        raise RebuildError("INVALID_REVOLVE_ANGLE", "revolve angle must be a typed object", feature_id)
    if angle_value == {"kind": "full_360"}:
        angle = {"kind": "full_360"}
    elif set(angle_value) == {"kind", "degrees"} and angle_value.get("kind") == "partial":
        degrees = angle_value["degrees"]
        if not isinstance(degrees, (float, int)) or isinstance(degrees, bool) or not math.isfinite(degrees) or not 0.0 < degrees < 360.0:
            raise RebuildError("INVALID_REVOLVE_ANGLE", "partial angle must be finite and greater than 0 and less than 360 degrees", feature_id)
        angle = {"kind": "partial", "degrees": float(degrees)}
    else:
        raise RebuildError("INVALID_REVOLVE_ANGLE", "revolve angle kind or fields are unsupported", feature_id)
    orientation = operation["orientation"]
    if orientation not in {"forward", "reverse"}:
        raise RebuildError("REBUILD_DEPENDENCY_ERROR", "revolve orientation is unsupported", feature_id)
    return {
        "kind": "revolve", "feature_id": feature_id, "sketch_id": sketch_id,
        "operation": material_operation, "plane": plane, "loops": loops,
        "profile_entity_ids": entity_ids, "axis_origin": axis_origin,
        "direction": direction, "angle": angle, "orientation": orientation,
        "source_feature_id": feature_id,
    }


def _parse_edge_reference(value: Any, feature_id: str) -> dict[str, Any]:
    required = {"source_feature_id", "selector", "qualifier"}
    if not isinstance(value, Mapping) or set(value) != required or not isinstance(value["source_feature_id"], str) or not value["source_feature_id"]:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "semantic edge reference fields are invalid", feature_id)
    selector_value = value["selector"]
    if not isinstance(selector_value, Mapping) or not isinstance(selector_value.get("kind"), str):
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "semantic edge selector is invalid", feature_id)
    selector = selector_value["kind"]
    if selector in {"profile_boundary", "pocket_entrance"}:
        expected = {"kind", "entity_id"} | ({"side"} if selector == "profile_boundary" else set())
        if set(selector_value) != expected or not isinstance(selector_value.get("entity_id"), str) or not selector_value["entity_id"]:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "profile edge selector is invalid", feature_id)
        if selector == "profile_boundary" and selector_value["side"] not in {"top", "bottom"}:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "profile edge side is invalid", feature_id)
        selector_value = dict(selector_value)
    elif selector == "revolve_profile_vertex":
        if set(selector_value) != {"kind", "entity_id", "endpoint"} or not isinstance(selector_value.get("entity_id"), str) or not selector_value["entity_id"] or selector_value["endpoint"] not in {"start", "end"}:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "revolve profile vertex selector is invalid", feature_id)
        selector_value = dict(selector_value)
    elif selector in {"hole_entrance", "hole_exit"}:
        if set(selector_value) != {"kind"}:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "hole edge selector fields are invalid", feature_id)
    elif selector in {"between_faces", "convex", "concave"}:
        if set(selector_value) != {"kind", "first", "second"}:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "adjacent face selector fields are invalid", feature_id)
        selector_value = {
            "kind": selector,
            "first": _parse_face_reference(selector_value["first"], feature_id),
            "second": _parse_face_reference(selector_value["second"], feature_id),
        }
        if selector_value["first"] == selector_value["second"]:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "adjacent semantic faces must be distinct", feature_id)
    elif selector == "between_generated_faces":
        if set(selector_value) != {"kind", "first", "second"}:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face adjacency fields are invalid", feature_id)
        first = _parse_generated_face_reference(selector_value["first"], feature_id)
        second = _parse_generated_face_reference(selector_value["second"], feature_id)
        if first == second or first["source_feature_id"] != value["source_feature_id"] or second["source_feature_id"] != value["source_feature_id"]:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face adjacency must name two distinct roles from the edge source feature", feature_id)
        selector_value = {"kind": selector, "first": first, "second": second}
    else:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", f"selector {selector!r} is unsupported", feature_id)
    qualifier = value["qualifier"]
    expected_q = {"curve_kind", "radius_mm", "length_mm", "midpoint", "tolerance_mm"}
    if not isinstance(qualifier, Mapping) or set(qualifier) != expected_q:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "edge qualifier fields are invalid", feature_id)
    curve_kind = qualifier["curve_kind"]
    if curve_kind not in {None, "line", "circular"}:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "edge curve qualifier is unsupported", feature_id)
    tolerance = qualifier["tolerance_mm"]
    if not isinstance(tolerance, (int, float)) or isinstance(tolerance, bool) or not math.isfinite(tolerance) or tolerance <= 0:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "edge tolerance must be finite and positive", feature_id)
    parsed_q = {"curve_kind": curve_kind, "tolerance_mm": float(tolerance)}
    for key in ("radius_mm", "length_mm"):
        number = qualifier[key]
        if number is not None and (not isinstance(number, (int, float)) or isinstance(number, bool) or not math.isfinite(number) or number <= 0):
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", f"{key} must be positive when provided", feature_id)
        parsed_q[key] = float(number) if number is not None else None
    midpoint = qualifier["midpoint"]
    parsed_q["midpoint"] = _vec3(midpoint, "edge midpoint", feature_id) if midpoint is not None else None
    return {"source_feature_id": value["source_feature_id"], "selector": selector_value, "qualifier": parsed_q}

def _parse_generated_face_reference(value: Any, feature_id: str) -> dict[str, Any]:
    required = {"source_feature_id", "selector", "qualifier"}
    if not isinstance(value, Mapping) or set(value) != required or not isinstance(value["source_feature_id"], str) or not value["source_feature_id"]:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face reference fields are invalid", feature_id)
    selector_value = value["selector"]
    if not isinstance(selector_value, Mapping) or not isinstance(selector_value.get("kind"), str):
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face selector is invalid", feature_id)
    kind = selector_value["kind"]
    if kind in {"sweep_start_cap", "sweep_end_cap", "loft_start_cap", "loft_end_cap"}:
        expected = {"kind"}
    elif kind == "sweep_lateral":
        expected = {"kind", "profile_entity_id"}
        if not isinstance(selector_value.get("profile_entity_id"), str) or not selector_value["profile_entity_id"]:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "Sweep lateral face requires a profile entity ID", feature_id)
    elif kind == "loft_lateral":
        expected = {"kind", "start_section_id", "end_section_id", "start_entity_id", "end_entity_id"}
        if any(not isinstance(selector_value.get(key), str) or not selector_value[key] for key in expected - {"kind"}) or selector_value["start_section_id"] == selector_value["end_section_id"]:
            raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "Loft lateral face requires adjacent section and corresponding profile entity IDs", feature_id)
    else:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", f"generated face role {kind!r} is unsupported", feature_id)
    if set(selector_value) != expected:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face selector fields are invalid", feature_id)
    qualifier = _parse_generated_face_qualifier(value["qualifier"], feature_id)
    return {"source_feature_id": value["source_feature_id"], "selector": dict(selector_value), "qualifier": qualifier}


def _parse_generated_face_qualifier(value: Any, feature_id: str) -> dict[str, Any]:
    expected = {"surface_kind", "normal", "centroid", "area_mm2", "area_tolerance_mm2", "tolerance_mm"}
    if not isinstance(value, Mapping) or set(value) != expected:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face qualifier fields are invalid", feature_id)
    kind = value["surface_kind"]
    if kind not in {None, "plane", "cylinder", "cone", "ruled_surface", "revolution_surface", "other"}:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face surface kind is invalid", feature_id)
    tolerance = value["tolerance_mm"]
    area = value["area_mm2"]
    area_tolerance = value["area_tolerance_mm2"]
    if not isinstance(tolerance, (int, float)) or isinstance(tolerance, bool) or not math.isfinite(tolerance) or tolerance <= 0:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face linear tolerance must be finite and positive", feature_id)
    if area is not None and (not isinstance(area, (int, float)) or isinstance(area, bool) or not math.isfinite(area) or area <= 0):
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face area must be finite and positive when provided", feature_id)
    if area_tolerance is not None and (not isinstance(area_tolerance, (int, float)) or isinstance(area_tolerance, bool) or not math.isfinite(area_tolerance) or area_tolerance <= 0):
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated face area tolerance must be finite and positive", feature_id)
    normal = _unit_vector(value["normal"], "generated face normal", feature_id) if value["normal"] is not None else None
    centroid = _vec3(value["centroid"], "generated face centroid", feature_id) if value["centroid"] is not None else None
    return {"surface_kind": kind, "normal": normal, "centroid": centroid, "area_mm2": float(area) if area is not None else None,
            "area_tolerance_mm2": float(area_tolerance) if area_tolerance is not None else None, "tolerance_mm": float(tolerance)}


def _parse_face_reference(value: Any, feature_id: str) -> dict[str, Any] | None:
    if value is None:
        return None
    required = {"source_feature_id", "selector", "normal", "plane_offset", "tolerance"}
    optional = {"generated_qualifier"}
    if not isinstance(value, Mapping) or not required.issubset(value) or set(value) - required - optional:
        raise RebuildError("INVALID_FACE_REFERENCE", "semantic face reference fields are invalid", feature_id)
    source_id, selector = value["source_feature_id"], value["selector"]
    planar_expected = {"top": (0, 0, 1), "bottom": (0, 0, -1), "positive_x": (1, 0, 0), "negative_x": (-1, 0, 0), "positive_y": (0, 1, 0), "negative_y": (0, -1, 0)}
    generated_selectors = {"sweep_start_cap", "sweep_end_cap", "loft_start_cap", "loft_end_cap"}
    if not isinstance(source_id, str) or not source_id or not isinstance(selector, str) or selector not in {*planar_expected, *generated_selectors}:
        raise RebuildError("INVALID_FACE_REFERENCE", "semantic face reference is invalid", feature_id)
    normal = _unit_vector(value["normal"], "face normal", feature_id)
    if selector in planar_expected and _distance3(normal, planar_expected[selector]) > 1e-8:
        raise RebuildError("INVALID_FACE_REFERENCE", "face selector and normal disagree", feature_id)
    offset, tolerance = value["plane_offset"], value["tolerance"]
    if not isinstance(offset, (int, float)) or isinstance(offset, bool) or not math.isfinite(offset) or not isinstance(tolerance, (int, float)) or isinstance(tolerance, bool) or not math.isfinite(tolerance) or tolerance <= 0:
        raise RebuildError("INVALID_FACE_REFERENCE", "face qualifier offset or tolerance is invalid", feature_id)
    qualifier_value = value.get("generated_qualifier")
    qualifier = _parse_generated_face_qualifier(qualifier_value, feature_id) if qualifier_value is not None else None
    if selector in generated_selectors and qualifier is None:
        qualifier = _parse_generated_face_qualifier({"surface_kind": None, "normal": None, "centroid": None, "area_mm2": None, "area_tolerance_mm2": None, "tolerance_mm": float(tolerance)}, feature_id)
    if selector in planar_expected and qualifier is not None:
        raise RebuildError("INVALID_FACE_REFERENCE", "generated qualifiers cannot be attached to a principal planar selector", feature_id)
    return {"source_feature_id": source_id, "selector": selector, "normal": normal, "plane_offset": float(offset), "tolerance": float(tolerance), "generated_qualifier": qualifier}


def _parse_plane(value: Any, feature_id: str) -> dict[str, tuple[float, float, float]]:
    if not isinstance(value, Mapping) or set(value) != {"origin", "u_axis", "v_axis", "normal"}:
        raise RebuildError("INVALID_DATUM", "sketch plane fields are invalid", feature_id)
    origin = _vec3(value["origin"], "plane origin", feature_id)
    u_axis = _unit_vector(value["u_axis"], "plane u axis", feature_id)
    v_axis = _unit_vector(value["v_axis"], "plane v axis", feature_id)
    normal = _unit_vector(value["normal"], "plane normal", feature_id)
    if abs(_dot(u_axis, v_axis)) > 1e-8 or abs(_dot(u_axis, normal)) > 1e-8 or abs(_dot(v_axis, normal)) > 1e-8:
        raise RebuildError("INVALID_DATUM", "sketch frame axes are not orthogonal", feature_id)
    cross = _cross(u_axis, v_axis)
    if _distance3(cross, normal) > 1e-8:
        raise RebuildError("INVALID_DATUM", "sketch frame is not right handed", feature_id)
    return {"origin": origin, "u_axis": u_axis, "v_axis": v_axis, "normal": normal}


def _parse_loops(value: Any, feature_id: str) -> list[list[dict[str, Any]]]:
    if not isinstance(value, list) or not value or any(not isinstance(loop, list) or not loop for loop in value):
        raise RebuildError("INVALID_SKETCH", "profile must have an outer loop", feature_id)
    result: list[list[dict[str, Any]]] = []
    for loop in value:
        curves = []
        for curve in loop:
            if not isinstance(curve, Mapping) or "kind" not in curve:
                raise RebuildError("INVALID_SKETCH", "profile curve is invalid", feature_id)
            kind = curve["kind"]
            if kind == "line" and set(curve) == {"kind", "start", "end"}:
                start = _vec2(curve["start"], feature_id)
                end = _vec2(curve["end"], feature_id)
                if _distance2(start, end) <= TOLERANCE:
                    raise RebuildError("INVALID_SKETCH", "profile contains a degenerate line", feature_id)
                curves.append({"kind": "line", "start": start, "end": end})
            elif kind == "arc" and set(curve) == {"kind", "start", "end", "center", "radius", "direction"}:
                start, end, center = _vec2(curve["start"], feature_id), _vec2(curve["end"], feature_id), _vec2(curve["center"], feature_id)
                radius = curve["radius"]
                if not isinstance(radius, (int, float)) or isinstance(radius, bool) or not math.isfinite(radius) or radius <= 0:
                    raise RebuildError("INVALID_SKETCH", "arc radius must be finite and positive", feature_id)
                if abs(_distance2(start, center) - radius) > TOLERANCE or abs(_distance2(end, center) - radius) > TOLERANCE or _distance2(start, end) <= TOLERANCE:
                    raise RebuildError("INVALID_SKETCH", "arc endpoints do not match its radius", feature_id)
                direction = curve["direction"]
                if direction not in {"counter_clockwise", "clockwise"}:
                    raise RebuildError("INVALID_SKETCH", "arc direction is invalid", feature_id)
                curves.append({"kind": "arc", "start": start, "end": end, "center": center, "radius": float(radius), "direction": direction})
            elif kind == "circle" and set(curve) == {"kind", "center", "radius"}:
                center = _vec2(curve["center"], feature_id)
                radius = curve["radius"]
                if not isinstance(radius, (int, float)) or isinstance(radius, bool) or not math.isfinite(radius) or radius <= 0:
                    raise RebuildError("INVALID_SKETCH", "circle radius must be finite and positive", feature_id)
                curves.append({"kind": "circle", "center": center, "radius": float(radius)})
            else:
                raise RebuildError("INVALID_SKETCH", "profile curve kind or fields are unsupported", feature_id)
        if len(curves) == 1 and curves[0]["kind"] == "circle":
            pass
        elif any(curve["kind"] == "circle" for curve in curves):
            raise RebuildError("INVALID_SKETCH", "circle must be a single-entity loop", feature_id)
        elif len(curves) < 2:
            raise RebuildError("OPEN_PROFILE", "closed profile needs at least two connected curves", feature_id)
        else:
            for index, curve in enumerate(curves):
                end = _curve_end(curve)
                start = _curve_start(curves[(index + 1) % len(curves)])
                if _distance2(end, start) > TOLERANCE:
                    raise RebuildError("OPEN_PROFILE", "profile loop is open or curves are out of order", feature_id)
        result.append(curves)
    return result


def _rebuild(request: dict[str, Any], *, shape_only: bool = False, section_plane: dict[str, Any] | None = None) -> dict[str, Any] | cq.Shape:
    body: cq.Shape | None = None
    operation_features: list[str] = []
    operation_timings: list[dict[str, Any]] = []
    feature_snapshots: dict[str, cq.Shape] = {}
    source_operations: defaultdict[str, list[dict[str, Any]]] = defaultdict(list)
    edge_provenance: defaultdict[str, set[str]] = defaultdict(set)
    face_provenance: defaultdict[str, set[str]] = defaultdict(set)
    previous_edge_counts: Counter[str] = Counter()
    previous_face_counts: Counter[str] = Counter()
    snapshot_ids: set[str] = set()
    for operation in request["operations"]:
        if operation["kind"] in {"extrude", "countersink"} and operation["support_face"] is not None:
            snapshot_ids.add(operation["support_face"]["source_feature_id"])
        if operation["kind"] in {"fillet", "chamfer"}:
            for reference in operation["edge_refs"]:
                snapshot_ids.add(reference["source_feature_id"])
                selector = reference["selector"]
                for face_key in ("first", "second"):
                    face = selector.get(face_key)
                    if face is not None:
                        snapshot_ids.add(face["source_feature_id"])
    for operation in request["operations"]:
        operation_started = time.perf_counter()
        topology_resolution_ms = 0.0
        kernel_operation_ms = 0.0
        feature_id = operation["feature_id"]
        if operation["kind"] == "sweep":
            if operation["operation"] == "cut" and body is None:
                raise RebuildError("NO_BODY", "Sweep CUT requires an existing body", feature_id)
            kernel_started = time.perf_counter()
            tool = _sweep_tool(operation)
            before = float(body.Volume()) if body is not None else 0.0
            try:
                if operation["operation"] == "add":
                    candidate = tool if body is None else body.fuse(tool)
                    if candidate is None or len(candidate.Solids()) != 1:
                        raise RebuildError("DETACHED_ADD", "Sweep ADD must produce one connected solid", feature_id)
                    _validate_body(candidate, feature_id)
                    if body is not None and float(candidate.Volume()) <= before + _volume_tolerance(before):
                        try:
                            overlap = body.intersect(tool)
                            overlap_volume = float(overlap.Volume()) if overlap is not None else 0.0
                        except Exception:
                            overlap_volume = 0.0
                        if overlap_volume <= _volume_tolerance(before):
                            raise RebuildError("DETACHED_ADD", "Sweep ADD does not intersect the existing body", feature_id)
                        raise RebuildError("SWEEP_BOOLEAN_FAILED", "Sweep ADD did not add material", feature_id)
                else:
                    candidate = body.cut(tool)
                    if candidate is None or not candidate.Solids() or float(candidate.Volume()) <= _volume_tolerance(before):
                        raise RebuildError("SWEEP_CUT_REMOVES_BODY", "Sweep CUT would remove the entire body", feature_id)
                    _validate_body(candidate, feature_id)
                    if float(candidate.Volume()) >= before - _volume_tolerance(before):
                        raise RebuildError("SWEEP_CUT_NO_INTERSECTION", "Sweep CUT did not intersect the existing body", feature_id)
                body = candidate
            except RebuildError:
                raise
            except Exception as error:
                raise RebuildError("SWEEP_BOOLEAN_FAILED", _safe_error(error), feature_id) from error
            kernel_operation_ms = (time.perf_counter() - kernel_started) * 1000.0
            source_feature_id = operation["source_feature_id"]
            operation_features.append(source_feature_id)
            source_operations[source_feature_id].append(operation)
        elif operation["kind"] == "loft":
            if operation["operation"] == "cut" and body is None:
                raise RebuildError("NO_BODY", "Loft CUT requires an existing body", feature_id)
            kernel_started = time.perf_counter()
            tool = _loft_tool(operation)
            before = float(body.Volume()) if body is not None else 0.0
            try:
                if operation["operation"] == "add":
                    candidate = tool if body is None else body.fuse(tool)
                    if candidate is None or len(candidate.Solids()) != 1:
                        raise RebuildError("LOFT_ADD_DETACHED", "Loft ADD must form one connected solid", feature_id)
                    _validate_body(candidate, feature_id)
                    if body is not None and float(candidate.Volume()) <= before + _volume_tolerance(before):
                        try:
                            overlap = body.intersect(tool)
                            overlap_volume = float(overlap.Volume()) if overlap is not None else 0.0
                        except Exception:
                            overlap_volume = 0.0
                        if overlap_volume <= _volume_tolerance(before):
                            raise RebuildError("LOFT_ADD_DETACHED", "Loft ADD does not intersect the existing body", feature_id)
                        raise RebuildError("LOFT_BOOLEAN_FAILED", "Loft ADD did not add material", feature_id)
                else:
                    candidate = body.cut(tool)
                    if candidate is None or not candidate.Solids() or float(candidate.Volume()) <= _volume_tolerance(before):
                        raise RebuildError("LOFT_CUT_REMOVES_BODY", "Loft CUT would remove the entire body", feature_id)
                    _validate_body(candidate, feature_id)
                    if float(candidate.Volume()) >= before - _volume_tolerance(before):
                        raise RebuildError("LOFT_CUT_NO_INTERSECTION", "Loft CUT did not intersect the existing body", feature_id)
                body = candidate
            except RebuildError:
                raise
            except Exception as error:
                raise RebuildError("LOFT_BOOLEAN_FAILED", _safe_error(error), feature_id) from error
            kernel_operation_ms = (time.perf_counter() - kernel_started) * 1000.0
            source_feature_id = operation["source_feature_id"]
            operation_features.append(source_feature_id)
            source_operations[source_feature_id].append(operation)
        elif operation["kind"] in {"extrude", "revolve"}:
            if operation["kind"] == "revolve":
                if operation["operation"] == "cut" and body is None:
                    raise RebuildError("NO_BODY", "Revolve CUT requires an existing body", feature_id)
                kernel_started = time.perf_counter()
                tool = _revolve_tool(operation, feature_id)
                if operation["operation"] == "add":
                    if body is None:
                        body = tool
                    else:
                        before = float(body.Volume())
                        try:
                            intersection = body.intersect(tool)
                            candidate = body.fuse(tool)
                        except Exception as error:
                            raise RebuildError("REVOLVE_GEOMETRY_FAILED", _safe_error(error), feature_id) from error
                        if candidate is None or len(candidate.Solids()) != 1:
                            raise RebuildError("DETACHED_ADD", "Revolve ADD is detached from the existing body; one connected solid is required", feature_id)
                        _validate_body(candidate, feature_id)
                        if float(candidate.Volume()) <= before + _volume_tolerance(before):
                            if intersection is None or float(intersection.Volume()) <= _volume_tolerance(before):
                                raise RebuildError("DETACHED_ADD", "Revolve ADD does not intersect the existing body", feature_id)
                            raise RebuildError("REVOLVE_GEOMETRY_FAILED", "Revolve ADD did not add material", feature_id)
                        body = candidate
                else:
                    before = float(body.Volume())
                    try:
                        candidate = body.cut(tool)
                    except Exception as error:
                        raise RebuildError("REVOLVE_GEOMETRY_FAILED", _safe_error(error), feature_id) from error
                    if candidate is None or not candidate.Solids() or float(candidate.Volume()) <= _volume_tolerance(before):
                        raise RebuildError("BODY_INVALID", "Revolve CUT would remove the entire body", feature_id)
                    _validate_body(candidate, feature_id)
                    if float(candidate.Volume()) >= before - _volume_tolerance(before):
                        raise RebuildError("REVOLVE_CUT_NO_INTERSECTION", "Revolve CUT did not remove material; move the profile into the existing body", feature_id)
                    body = candidate
            else:
                if operation["support_face"] is not None:
                    topology_started = time.perf_counter()
                    _resolve_face_reference(operation, feature_snapshots, source_operations, body)
                    topology_resolution_ms += (time.perf_counter() - topology_started) * 1000.0
                if operation["blind_hole"]:
                    if body is None:
                        raise RebuildError("NO_BODY", "blind hole requires an existing body", feature_id)
                    bounds = body.BoundingBox()
                    corners = [(x, y, z) for x in (bounds.xmin, bounds.xmax) for y in (bounds.ymin, bounds.ymax) for z in (bounds.zmin, bounds.zmax)]
                    remaining = max(_dot(point, operation["direction"]) for point in corners) - _dot(operation["plane"]["origin"], operation["direction"])
                    depth = operation["extent"].get("length")
                    if not isinstance(depth, (int, float)) or depth <= 0 or depth > remaining + 1e-7:
                        raise RebuildError("INVALID_HOLE_DEPTH", "blind hole depth exceeds material remaining along its axis", feature_id)
                kernel_started = time.perf_counter()
                face = _build_profile_face(operation["plane"], operation["loops"], feature_id)
                tool = _extrude_tool(face, body, operation, feature_id)
                if operation["operation"] == "add":
                    if body is None:
                        body = tool
                    else:
                        before = float(body.Volume())
                        try:
                            candidate = body.fuse(tool)
                        except Exception as error:
                            raise RebuildError("BOOLEAN_FAILED", _safe_error(error), feature_id) from error
                        _validate_body(candidate, feature_id)
                        if float(candidate.Volume()) <= before + _volume_tolerance(before):
                            raise RebuildError("BOOLEAN_FAILED", "ADD did not add material", feature_id)
                        body = candidate
                else:
                    if body is None:
                        raise RebuildError("NO_BODY", "CUT cannot run before an ADD creates a body", feature_id)
                    before = float(body.Volume())
                    is_pattern_instance = str(operation.get("source_feature_kind") or "").startswith("pattern:")
                    no_intersection_code = "PATTERN_INSTANCE_COLLISION" if is_pattern_instance else "CUT_NO_INTERSECTION"
                    try:
                        candidate = body.cut(tool)
                    except RebuildError:
                        raise
                    except Exception as error:
                        raise RebuildError("BOOLEAN_FAILED", _safe_error(error), feature_id) from error
                    if candidate is None or not candidate.Solids() or float(candidate.Volume()) <= _volume_tolerance(before):
                        code = "PATTERN_INSTANCE_COLLISION" if is_pattern_instance else "CUT_REMOVES_BODY"
                        message = "pattern instance would remove the entire body" if is_pattern_instance else "CUT would remove the entire body"
                        raise RebuildError(code, message, feature_id)
                    _validate_body(candidate, feature_id)
                    if float(candidate.Volume()) >= before - _volume_tolerance(before):
                        message = "pattern instance did not remove material" if is_pattern_instance else "cutting profile did not remove material"
                        raise RebuildError(no_intersection_code, message, feature_id)
                    body = candidate
            kernel_operation_ms = (time.perf_counter() - kernel_started) * 1000.0
            source_feature_id = operation["source_feature_id"]
            operation_features.append(source_feature_id)
            source_operations[source_feature_id].append(operation)
        elif operation["kind"] == "countersink":
            if body is None:
                raise RebuildError("NO_BODY", "Countersink CUT requires an existing body and its cylindrical main bore", feature_id)
            if operation["support_face"] is not None:
                topology_started = time.perf_counter()
                _resolve_face_reference(operation, feature_snapshots, source_operations, body)
                topology_resolution_ms += (time.perf_counter() - topology_started) * 1000.0
            kernel_started = time.perf_counter()
            before = float(body.Volume())
            is_pattern_instance = str(operation["source_feature_kind"]).startswith("pattern:")
            no_intersection_code = "PATTERN_INSTANCE_COLLISION" if is_pattern_instance else "COUNTERSINK_NO_INTERSECTION"
            try:
                cone = cq.Solid.makeCone(
                    operation["mouth_diameter"] / 2.0,
                    operation["main_diameter"] / 2.0,
                    operation["depth"],
                    cq.Vector(*operation["plane"]["origin"]),
                    cq.Vector(*operation["direction"]),
                )
                if cone is None or not cone.isValid() or not cone.Solids():
                    raise RebuildError("COUNTERSINK_GEOMETRY_FAILED", "OCP produced an invalid countersink cone", feature_id)
                candidate = body.cut(cone)
                if candidate is None or not candidate.Solids() or float(candidate.Volume()) <= _volume_tolerance(before):
                    code = "PATTERN_INSTANCE_COLLISION" if is_pattern_instance else "BODY_INVALID"
                    raise RebuildError(code, "Countersink CUT would remove the entire body", feature_id)
                _validate_body(candidate, feature_id)
                if float(candidate.Volume()) >= before - _volume_tolerance(before):
                    raise RebuildError(no_intersection_code, "countersink cone did not intersect the main bore entry", feature_id)
            except RebuildError:
                raise
            except Exception as error:
                raise RebuildError("COUNTERSINK_GEOMETRY_FAILED", _safe_error(error), feature_id) from error
            body = candidate
            kernel_operation_ms = (time.perf_counter() - kernel_started) * 1000.0
            source_feature_id = operation["source_feature_id"]
            operation_features.append(source_feature_id)
            source_operations[source_feature_id].append(operation)
        else:
            if body is None:
                raise RebuildError("NO_BODY", "edge treatment requires an existing body", feature_id)
            topology_started = time.perf_counter()
            edges = []
            for reference in operation["edge_refs"]:
                edge = _resolve_edge_reference(reference, body, feature_snapshots, source_operations, feature_id)
                if any(edge.isSame(existing) for existing in edges):
                    raise RebuildError("DUPLICATE_EDGE_REFERENCE", "two semantic references resolved to the same edge", feature_id)
                edges.append(edge)
            topology_resolution_ms = (time.perf_counter() - topology_started) * 1000.0
            kernel_started = time.perf_counter()
            error_code = "FILLET_GEOMETRY_FAILED" if operation["kind"] == "fillet" else "CHAMFER_GEOMETRY_FAILED"
            try:
                if operation["kind"] == "fillet":
                    candidate = body.fillet(operation["radius"], edges)
                else:
                    candidate = body.chamfer(operation["distance"], None, edges)
                _validate_body(candidate, feature_id)
            except RebuildError as error:
                if error.code in {"TOPOLOGY_REFERENCE_NOT_FOUND", "TOPOLOGY_REFERENCE_STALE", "AMBIGUOUS_TOPOLOGY_REFERENCE", "UNSUPPORTED_TOPOLOGY_REFERENCE", "DUPLICATE_EDGE_REFERENCE"}:
                    raise
                raise RebuildError(error_code, error.message, feature_id) from error
            except Exception as error:
                raise RebuildError(error_code, _safe_error(error), feature_id) from error
            body = candidate
            kernel_operation_ms = (time.perf_counter() - kernel_started) * 1000.0
            operation_features.append(feature_id)
        if operation["kind"] in {"fillet", "chamfer"}:
            source_feature_id = feature_id
        if source_feature_id in snapshot_ids:
            feature_snapshots[source_feature_id] = body.copy()
        previous_edge_counts, previous_face_counts = _record_topology_provenance(
            body, source_feature_id, previous_edge_counts, previous_face_counts,
            edge_provenance, face_provenance,
        )
        kind = operation["kind"]
        if kind in {"extrude", "revolve", "sweep", "loft"}:
            kind = f"{kind}_{operation['operation']}"
        elif kind == "countersink":
            kind = "countersink_cut"
        operation_timings.append({
            "feature_id": feature_id,
            "kind": kind,
            "topology_resolution_ms": topology_resolution_ms,
            "kernel_operation_ms": kernel_operation_ms,
            "elapsed_ms": (time.perf_counter() - operation_started) * 1000.0,
        })
    if body is None:
        raise RebuildError("NO_BODY", "feature history produced no body")
    try:
        body = body.clean()
    except Exception as error:
        raise RebuildError("BOOLEAN_FAILED", _safe_error(error)) from error
    _validate_body(body, None)
    if operation_features:
        _assign_cleaned_topology_provenance(
            body, operation_features[-1], edge_provenance, face_provenance,
        )
    if shape_only:
        return body
    result = _result(body, request["model_sha256"], request["operations"], operation_features, edge_provenance, face_provenance, operation_timings)
    if section_plane is None:
        return result
    section_started = time.perf_counter()
    section = _section_geometry(body, section_plane, request["model_sha256"])
    section["geometry_elapsed_ms"] = round((time.perf_counter() - section_started) * 1000.0, 6)
    return {"rebuild": result, "section_geometry": section}


def _section_geometry(shape: cq.Shape, plane: dict[str, Any], model_sha256: str) -> dict[str, Any]:
    """Intersect a validated body with a principal plane and return closed loops."""
    tolerance = 1e-6
    chord_tolerance = 0.01
    try:
        section_op = BRepAlgoAPI_Section(
            shape.wrapped,
            gp_Pln(gp_Pnt(*plane["origin"]), gp_Dir(*plane["normal"])),
            False,
        )
        section_op.Build()
        if not section_op.IsDone():
            raise RebuildError("SECTION_GEOMETRY_FAILED", "OCP section operation did not complete", stage="section_geometry")
        section_shape = section_op.Shape()
        edges = TopTools_HSequenceOfShape()
        explorer = TopExp_Explorer(section_shape, TopAbs_EDGE)
        while explorer.More():
            edges.Append(explorer.Current())
            explorer.Next()
        if edges.Length() == 0:
            raise RebuildError("SECTION_NO_INTERSECTION", "section plane does not produce a material boundary", stage="section_geometry")
        wires = TopTools_HSequenceOfShape()
        ShapeAnalysis_FreeBounds.ConnectEdgesToWires_s(edges, tolerance, False, wires)
        if wires.Length() == 0:
            raise RebuildError("SECTION_OPEN_BOUNDARY", "section edges could not be assembled into closed loops", stage="section_geometry")

        loops: list[list[list[float]]] = []
        assigned_edges = 0
        for index in range(1, wires.Length() + 1):
            wire = cq.Shape.cast(wires.Value(index))
            wire_edges = wire.Edges()
            assigned_edges += len(wire_edges)
            if not wire.IsClosed() or not wire.isValid() or not wire_edges:
                raise RebuildError("SECTION_OPEN_BOUNDARY", "section contains an open or invalid material boundary", stage="section_geometry")
            world_points: list[tuple[float, float, float]] = []
            for edge_index, edge in enumerate(wire_edges):
                sampler = GCPnts_QuasiUniformDeflection(BRepAdaptor_Curve(edge.wrapped), chord_tolerance)
                if not sampler.IsDone() or sampler.NbPoints() < 2:
                    raise RebuildError("SECTION_UNSUPPORTED_CURVE", "section edge could not be sampled at the declared tolerance", stage="section_geometry")
                sampled = [
                    tuple(float(c) for c in sampler.Value(point_index).Coord())
                    for point_index in range(1, sampler.NbPoints() + 1)
                ]
                if len(sampled) < 2:
                    raise RebuildError("SECTION_UNSUPPORTED_CURVE", "section edge could not be sampled at the declared tolerance", stage="section_geometry")
                if edge_index > 0 and world_points:
                    first_gap = _distance3(world_points[-1], sampled[0])
                    last_gap = _distance3(world_points[-1], sampled[-1])
                    if last_gap < first_gap:
                        sampled.reverse()
                    if min(first_gap, last_gap) > tolerance:
                        raise RebuildError("SECTION_OPEN_BOUNDARY", "section wire edge ordering is discontinuous", stage="section_geometry")
                for point in sampled:
                    if not world_points or _distance3(point, world_points[-1]) > 1e-9:
                        world_points.append(point)
            if len(world_points) < 4 or _distance3(world_points[-1], world_points[0]) > tolerance:
                raise RebuildError("SECTION_OPEN_BOUNDARY", "sampled section boundary is not closed", stage="section_geometry")
            world_points.pop()
            local_points = []
            for point in world_points:
                delta = tuple(point[axis] - plane["origin"][axis] for axis in range(3))
                local_points.append((_dot(delta, plane["u_axis"]), _dot(delta, plane["v_axis"])))
            area = sum(local_points[i][0] * local_points[(i + 1) % len(local_points)][1] - local_points[(i + 1) % len(local_points)][0] * local_points[i][1] for i in range(len(local_points))) / 2.0
            if abs(area) <= tolerance:
                raise RebuildError("SECTION_DEGENERATE_LOOP", "section produced a zero-area boundary", stage="section_geometry")
            if area < 0.0:
                local_points.reverse()
            start = min(range(len(local_points)), key=lambda i: (round(local_points[i][0], 9), round(local_points[i][1], 9)))
            local_points = local_points[start:] + local_points[:start]
            loops.append([[round(point[0], 9), round(point[1], 9)] for point in local_points])
        if assigned_edges != edges.Length():
            raise RebuildError("SECTION_OPEN_BOUNDARY", "not every section edge belongs to exactly one closed loop", stage="section_geometry")
        loops.sort(key=lambda loop: (min(point[0] for point in loop), min(point[1] for point in loop), len(loop)))
        return {
            "schema_version": 1,
            "model_sha256": model_sha256,
            "plane": {
                key: list(plane[key]) if isinstance(plane[key], tuple) else plane[key]
                for key in ("axis", "offset_mm", "view_side", "origin", "normal", "u_axis", "v_axis")
            },
            "loops": loops,
            "edge_count": edges.Length(),
            "sample_chord_tolerance_mm": chord_tolerance,
        }
    except RebuildError:
        raise
    except Exception as error:
        raise RebuildError("SECTION_GEOMETRY_FAILED", _safe_error(error), stage="section_geometry") from error


def build_formal_manufacturing_shape(value: Mapping[str, Any]) -> cq.Shape:
    """Rebuild one invocation-local source solid from immutable Manufacturing IR.

    The canonical STEP worker shares the ordered-operation executor and exact
    topology/boolean rules with drawing rebuild. It creates its own source solid
    from the versioned request; it never consumes a Drawing runtime Shape or a
    cached project artifact. This boundary returns no DrawingModel or export.
    """

    result = _rebuild(_parse_request(value), shape_only=True)
    if not isinstance(result, cq.Shape):
        raise RebuildError("BODY_INVALID", "parametric source rebuild did not return a CAD shape")
    return result


def _assign_cleaned_topology_provenance(
    shape: cq.Shape,
    final_feature_id: str,
    edge_provenance: defaultdict[str, set[str]],
    face_provenance: defaultdict[str, set[str]],
) -> None:
    """Attribute topology newly merged by the final exact Shape.clean().

    Coplanar boolean faces can merge during clean(), creating a new geometric
    fingerprint after the operation-level delta was recorded. Only records
    without an existing source are attributed to the final operation; stable
    hashes remain independent of transient OCP enumeration.
    """
    for record in _edge_evidence(shape):
        digest = record["id"].split(":")[1]
        if not edge_provenance.get(digest):
            edge_provenance[digest].add(final_feature_id)
    for record in _face_evidence(shape):
        digest = record["id"].split(":")[1]
        if not face_provenance.get(digest):
            face_provenance[digest].add(final_feature_id)


def _resolve_edge_reference(
    reference: dict[str, Any],
    body: cq.Shape,
    snapshots: dict[str, cq.Shape],
    source_operations: Mapping[str, list[dict[str, Any]]],
    feature_id: str,
) -> cq.Edge:
    source_id = reference["source_feature_id"]
    source = snapshots.get(source_id)
    if source is None:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "semantic edge source is not present in the rebuild prefix", feature_id)
    source_matches = _select_semantic_edges(reference, source, snapshots, source_operations, feature_id)
    if len(source_matches) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "semantic edge reference matches more than one source edge", feature_id)
    if not source_matches:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "semantic edge reference did not match its source feature", feature_id)
    source_edge = source_matches[0]
    qualifier = reference["qualifier"]
    tolerance = qualifier["tolerance_mm"]
    if reference["selector"]["kind"] == "between_generated_faces":
        # Resolve both face roles on the declared source snapshot, then carry
        # the exact engineering edge geometry forward. This remains valid when
        # a later, unrelated cut changes the cap face interior.
        current_candidates = body.Edges()
    else:
        current_candidates = _select_semantic_edges(reference, body, snapshots, source_operations, feature_id)
    candidates = [
        edge for edge in current_candidates
        if _edges_same_geometry(source_edge, edge, tolerance) and _edge_matches_qualifier(edge, qualifier, tolerance)
    ]
    if len(candidates) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "semantic edge reference matches multiple current edges", feature_id)
    if not candidates:
        raise RebuildError("TOPOLOGY_REFERENCE_STALE", "referenced edge was changed or removed by an upstream feature", feature_id)
    return candidates[0]


def _select_semantic_edges(
    reference: dict[str, Any],
    shape: cq.Shape,
    snapshots: dict[str, cq.Shape],
    source_operations: Mapping[str, list[dict[str, Any]]],
    feature_id: str,
) -> list[cq.Edge]:
    selector = reference["selector"]
    kind = selector["kind"]
    operations = source_operations.get(reference["source_feature_id"], [])
    result: list[cq.Edge] = []
    if kind == "revolve_profile_vertex":
        for operation in operations:
            if operation["kind"] != "revolve":
                continue
            result.extend(_revolve_profile_vertex_candidates(
                operation, shape, selector["entity_id"], selector["endpoint"],
                reference["qualifier"]["tolerance_mm"],
            ))
    elif kind in {"profile_boundary", "pocket_entrance"}:
        if not operations:
            return []
        for operation in operations:
            if operation["kind"] != "extrude":
                continue
            if kind == "profile_boundary" and operation["operation"] != "add":
                continue
            if kind == "pocket_entrance" and operation["operation"] != "cut":
                continue
            expected = _profile_entity_edge(operation, selector["entity_id"], kind, selector.get("side"), feature_id)
            if expected is not None:
                result.extend(edge for edge in shape.Edges() if _edges_same_geometry(expected, edge, reference["qualifier"]["tolerance_mm"]))
    elif kind in {"hole_entrance", "hole_exit"}:
        for operation in operations:
            if operation["kind"] != "extrude" or operation.get("source_feature_kind") != "hole" or not operation["feature_id"].endswith("::main"):
                continue
            circles = [curve for loop in operation["loops"] for curve in loop if curve["kind"] == "circle"]
            if len(circles) != 1:
                continue
            circle = circles[0]
            center = _world_point(operation["plane"], circle["center"])
            radius = circle["radius"]
            normal = operation["plane"]["normal"]
            direction = operation["direction"]
            matches = []
            for edge in shape.Edges():
                adaptor = BRepAdaptor_Curve(edge.wrapped)
                if adaptor.GetType() != GeomAbs_Circle:
                    continue
                candidate = adaptor.Circle()
                if abs(float(candidate.Radius()) - radius) > reference["qualifier"]["tolerance_mm"]:
                    continue
                axis = _point(candidate.Axis().Direction())
                if _distance3(axis, normal) > 1e-7 and _distance3(axis, tuple(-v for v in normal)) > 1e-7:
                    continue
                if _distance3(_closest_axis_point(_point(candidate.Location()), center, normal), center) > reference["qualifier"]["tolerance_mm"]:
                    continue
                matches.append(edge)
            if matches:
                projections = [(_dot(_point(BRepAdaptor_Curve(edge.wrapped).Circle().Location()), direction), edge) for edge in matches]
                target = min(value for value, _ in projections) if kind == "hole_entrance" else max(value for value, _ in projections)
                result.extend(edge for value, edge in projections if abs(value - target) <= reference["qualifier"]["tolerance_mm"])
    elif kind in {"between_faces", "convex", "concave"}:
        first = _unique_semantic_face(shape, selector["first"], feature_id, snapshots)
        second = _unique_semantic_face(shape, selector["second"], feature_id, snapshots)
        for edge in shape.Edges():
            adjacent = edge.ancestors(shape, "Face").Faces()
            if any(face.isSame(first) for face in adjacent) and any(face.isSame(second) for face in adjacent):
                if kind == "between_faces" or _edge_convexity(shape, edge, adjacent) == kind:
                    result.append(edge)
    elif kind == "between_generated_faces":
        first = _unique_generated_face(shape, selector["first"], feature_id, snapshots, source_operations)
        second = _unique_generated_face(shape, selector["second"], feature_id, snapshots, source_operations)
        for edge in shape.Edges():
            adjacent = edge.ancestors(shape, "Face").Faces()
            if any(face.isSame(first) for face in adjacent) and any(face.isSame(second) for face in adjacent):
                result.append(edge)
    else:
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "semantic edge selector is unsupported", feature_id)
    return [edge for edge in result if _edge_matches_qualifier(edge, reference["qualifier"], reference["qualifier"]["tolerance_mm"])]


def _revolve_profile_vertex_candidates(
    operation: dict[str, Any],
    shape: cq.Shape,
    entity_id: str,
    endpoint: str,
    tolerance: float,
) -> list[cq.Edge]:
    point2 = None
    for loop, identities in zip(operation["loops"], operation["profile_entity_ids"]):
        for curve, identity in zip(loop, identities):
            if identity != entity_id:
                continue
            if curve["kind"] == "circle":
                return []
            point2 = curve["start"] if endpoint == "start" else curve["end"]
            break
        if point2 is not None:
            break
    if point2 is None:
        return []
    point = _world_point(operation["plane"], point2)
    origin, direction = operation["axis_origin"], operation["direction"]
    axial = _dot(tuple(point[i] - origin[i] for i in range(3)), direction)
    center = tuple(origin[i] + axial * direction[i] for i in range(3))
    radius = _distance3(point, center)
    if radius <= tolerance:
        return []
    angle = math.tau if operation["angle"]["kind"] == "full_360" else math.radians(operation["angle"]["degrees"])
    expected_length = radius * angle
    result = []
    for edge in shape.Edges():
        adaptor = BRepAdaptor_Curve(edge.wrapped)
        if adaptor.GetType() != GeomAbs_Circle:
            continue
        circle = adaptor.Circle()
        if abs(float(circle.Radius()) - radius) > tolerance:
            continue
        if _distance3(_point(circle.Axis().Direction()), direction) > 1.0e-7 and _distance3(_point(circle.Axis().Direction()), tuple(-v for v in direction)) > 1.0e-7:
            continue
        if _distance3(_point(circle.Location()), center) > tolerance:
            continue
        if abs(float(edge.Length()) - expected_length) <= max(tolerance, 1.0e-6):
            result.append(edge)
    return result


def _profile_entity_edge(
    operation: dict[str, Any], entity_id: str, selector_kind: str, side: str | None, feature_id: str
) -> cq.Edge | None:
    for loop_index, (loop, identities) in enumerate(zip(operation["loops"], operation["profile_entity_ids"])):
        for entity_index, identity in enumerate(identities):
            if identity != entity_id:
                continue
            if selector_kind == "profile_boundary" and loop_index != 0:
                return None
            curve = loop[entity_index]
            edge = _edge_from_profile_curve(curve, operation["plane"], feature_id)
            offset = 0.0
            if selector_kind == "profile_boundary":
                extent = operation["extent"]
                if extent["kind"] == "distance":
                    offset = extent["length"] if side == "top" else 0.0
                elif extent["kind"] == "symmetric":
                    offset = extent["length"] / 2.0 if side == "top" else -extent["length"] / 2.0
                else:
                    return None
                edge = edge.translate(cq.Vector(*(offset * value for value in operation["direction"])))
            return edge
    return None


def _edge_from_profile_curve(curve: dict[str, Any], plane: dict[str, tuple[float, float, float]], feature_id: str) -> cq.Edge:
    if curve["kind"] == "circle":
        return cq.Edge.makeCircle(curve["radius"], cq.Vector(*_world_point(plane, curve["center"])), cq.Vector(*plane["normal"]))
    if curve["kind"] == "line":
        return cq.Edge.makeLine(cq.Vector(*_world_point(plane, curve["start"])), cq.Vector(*_world_point(plane, curve["end"])))
    start, end, center, radius = curve["start"], curve["end"], curve["center"], curve["radius"]
    a0 = math.atan2(start[1] - center[1], start[0] - center[0])
    a1 = math.atan2(end[1] - center[1], end[0] - center[0])
    sweep = (a1 - a0) % math.tau if curve["direction"] == "counter_clockwise" else (a0 - a1) % math.tau
    mid_angle = a0 + (sweep / 2.0 if curve["direction"] == "counter_clockwise" else -sweep / 2.0)
    mid = (center[0] + radius * math.cos(mid_angle), center[1] + radius * math.sin(mid_angle))
    return cq.Edge.makeThreePointArc(
        cq.Vector(*_world_point(plane, start)), cq.Vector(*_world_point(plane, mid)), cq.Vector(*_world_point(plane, end))
    )


def _edges_same_geometry(first: cq.Edge, second: cq.Edge, tolerance: float) -> bool:
    a, b = BRepAdaptor_Curve(first.wrapped), BRepAdaptor_Curve(second.wrapped)
    if a.GetType() != b.GetType() or abs(first.Length() - second.Length()) > tolerance:
        return False
    if a.GetType() == GeomAbs_Line:
        a_points = [_point(first.startPoint()), _point(first.endPoint())]
        b_points = [_point(second.startPoint()), _point(second.endPoint())]
        return ( _distance3(a_points[0], b_points[0]) <= tolerance and _distance3(a_points[1], b_points[1]) <= tolerance ) or ( _distance3(a_points[0], b_points[1]) <= tolerance and _distance3(a_points[1], b_points[0]) <= tolerance )
    if a.GetType() == GeomAbs_Circle:
        x, y = a.Circle(), b.Circle()
        if abs(float(x.Radius()) - float(y.Radius())) > tolerance:
            return False
        if _distance3(_point(x.Location()), _point(y.Location())) > tolerance:
            return False
        axes_match = _distance3(_point(x.Axis().Direction()), _point(y.Axis().Direction())) <= 1e-7 or _distance3(_point(x.Axis().Direction()), tuple(-v for v in _point(y.Axis().Direction()))) <= 1e-7
        if not axes_match:
            return False
        if abs(float(first.Length()) - float(second.Length())) > tolerance:
            return False
        if first.Closed() and second.Closed():
            return True
        a_points = [_point(first.startPoint()), _point(first.endPoint())]
        b_points = [_point(second.startPoint()), _point(second.endPoint())]
        return (_distance3(a_points[0], b_points[0]) <= tolerance and _distance3(a_points[1], b_points[1]) <= tolerance) or (_distance3(a_points[0], b_points[1]) <= tolerance and _distance3(a_points[1], b_points[0]) <= tolerance)
    return False


def _edge_matches_qualifier(edge: cq.Edge, qualifier: dict[str, Any], tolerance: float) -> bool:
    kind = BRepAdaptor_Curve(edge.wrapped).GetType()
    if qualifier["curve_kind"] == "line" and kind != GeomAbs_Line:
        return False
    if qualifier["curve_kind"] == "circular" and kind != GeomAbs_Circle:
        return False
    if qualifier["length_mm"] is not None and abs(edge.Length() - qualifier["length_mm"]) > tolerance:
        return False
    if qualifier["radius_mm"] is not None:
        if kind != GeomAbs_Circle or abs(float(BRepAdaptor_Curve(edge.wrapped).Circle().Radius()) - qualifier["radius_mm"]) > tolerance:
            return False
    if qualifier["midpoint"] is not None and _distance3(_point(edge.Center()), qualifier["midpoint"]) > tolerance:
        return False
    return True


def _unique_semantic_face(
    shape: cq.Shape,
    reference: dict[str, Any],
    feature_id: str,
    snapshots: Mapping[str, cq.Shape],
) -> cq.Face:
    source_id = reference["source_feature_id"]
    source = snapshots.get(source_id)
    if source is None:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", f"semantic face source {source_id} is not present in the rebuild prefix", feature_id)
    source_matches = _semantic_face_candidates(source, reference)
    if len(source_matches) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", f"semantic face from source {source_id} is ambiguous", feature_id)
    if not source_matches:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", f"semantic face selector did not match source feature {source_id}", feature_id)
    matches = _semantic_face_candidates(shape, reference)
    if len(matches) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "semantic adjacent face is ambiguous", feature_id)
    if not matches:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", f"semantic adjacent face from {source_id} was not found", feature_id)
    return matches[0]


def _semantic_face_candidates(shape: cq.Shape, reference: dict[str, Any]) -> list[cq.Face]:
    matches = []
    for face in shape.Faces():
        adaptor = BRepAdaptor_Surface(face.wrapped, True)
        if adaptor.GetType() != GeomAbs_Plane:
            continue
        try:
            normal = _unit_vector(_point(face.normalAt()), "candidate face normal", "")
        except Exception:
            continue
        if _distance3(normal, reference["normal"]) > 1e-7:
            continue
        offset = _dot(_point(face.Center()), reference["normal"])
        if abs(offset - reference["plane_offset"]) <= reference["tolerance"]:
            matches.append(face)
    return matches


def _edge_convexity(shape: cq.Shape, edge: cq.Edge, adjacent: list[cq.Face]) -> str:
    if len(adjacent) != 2:
        return "unknown"
    normals = [_unit_vector(_point(face.normalAt()), "face normal", "") for face in adjacent]
    midpoint = _point(edge.Center())
    step = 1.0e-4
    inside_directions = 0
    for sign_first, sign_second in ((1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)):
        direction = tuple(sign_first * normals[0][index] + sign_second * normals[1][index] for index in range(3))
        magnitude = math.sqrt(_dot(direction, direction))
        if magnitude <= TOLERANCE:
            return "unknown"
        point = cq.Vector(*(midpoint[index] + step * direction[index] / magnitude for index in range(3)))
        try:
            inside_directions += int(shape.isInside(point, 1e-7))
        except Exception:
            return "unknown"
    # At a convex solid edge only the direction opposite both outward normals
    # enters the solid. At a concave edge three of the four local sectors do.
    if inside_directions == 1:
        return "convex"
    if inside_directions == 3:
        return "concave"
    return "unknown"


def _closest_axis_point(point: tuple[float, ...], origin: tuple[float, ...], axis: tuple[float, ...]) -> tuple[float, float, float]:
    delta = tuple(point[index] - origin[index] for index in range(3))
    amount = _dot(delta, axis)
    return tuple(origin[index] + amount * axis[index] for index in range(3))

def _sweep_directed_curves(operation: Mapping[str, Any]) -> list[dict[str, Any]]:
    return operation["path"] if operation["direction"] == "forward" else _reverse_path_curves(operation["path"])


def _sweep_profile_frame(operation: Mapping[str, Any], cap: str) -> dict[str, tuple[float, float, float]]:
    curves = _sweep_directed_curves(operation)
    curve = curves[0] if cap == "start" else curves[-1]
    point2 = _curve_start(curve) if cap == "start" else _curve_end(curve)
    tangent2 = _curve_tangent(curve, cap == "start")
    path_plane = operation["path_plane"]
    tangent = tuple(path_plane["u_axis"][i] * tangent2[0] + path_plane["v_axis"][i] * tangent2[1] for i in range(3))
    binormal = path_plane["normal"]
    u_axis = _cross(binormal, tangent)
    path_point = _world_point(path_plane, point2)
    profile_origin = operation["profile_origin"]
    origin = tuple(path_point[i] - u_axis[i] * profile_origin[0] - binormal[i] * profile_origin[1] for i in range(3))
    return {"origin": origin, "u_axis": u_axis, "v_axis": binormal, "normal": tangent}


def _generated_cap_definition(operation: Mapping[str, Any], role: str, feature_id: str) -> tuple[dict[str, tuple[float, float, float]], list[list[dict[str, Any]]], str]:
    if role.startswith("sweep_") and operation["kind"] == "sweep" and operation["operation"] == "add":
        cap = "start" if role == "sweep_start_cap" else "end"
        profile_frame = _sweep_profile_frame(operation, cap)
        outward = dict(profile_frame)
        if cap == "start":
            outward["normal"] = tuple(-value for value in profile_frame["normal"])
            outward["v_axis"] = tuple(-value for value in profile_frame["v_axis"])
        return outward, operation["loops"], "sweep"
    if role.startswith("loft_") and operation["kind"] == "loft" and operation["operation"] == "add":
        cap = "start" if role == "loft_start_cap" else "end"
        section = operation["sections"][0] if cap == "start" else operation["sections"][-1]
        profile_frame = section["plane"]
        outward = dict(profile_frame)
        if cap == "start":
            outward["normal"] = tuple(-value for value in profile_frame["normal"])
            outward["v_axis"] = tuple(-value for value in profile_frame["v_axis"])
        return outward, section["loops"], "loft"
    raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", f"role {role} does not belong to an active ADD Sweep/Loft", feature_id)


def _generated_face_role_operation(reference: dict[str, Any], source_operations: Mapping[str, list[dict[str, Any]]], feature_id: str) -> dict[str, Any]:
    source_id = reference["source_feature_id"]
    selector = reference["selector"]
    role = selector["kind"]
    operations = source_operations.get(source_id, [])
    expected_kind = "sweep" if role.startswith("sweep_") else "loft"
    matches = [item for item in operations if item["kind"] == expected_kind]
    if not matches:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", f"generated topology source {source_id} is missing or is not a {expected_kind}", feature_id)
    if len(matches) != 1 or matches[0]["operation"] != "add":
        raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", "generated topology currently requires one active ADD Sweep/Loft source", feature_id)
    return matches[0]


def _surface_kind(face: cq.Face, expected_role: str | None = None) -> str:
    kind = BRepAdaptor_Surface(face.wrapped, True).GetType()
    if kind == GeomAbs_Plane:
        return "plane"
    if kind == GeomAbs_Cylinder:
        return "cylinder"
    if kind == GeomAbs_Cone:
        return "cone"
    if kind == GeomAbs_BSplineSurface and expected_role and expected_role.startswith("loft_lateral"):
        return "ruled_surface"
    return "other"


def _face_witness(face: cq.Face, expected_role: str | None = None) -> dict[str, Any]:
    adaptor = BRepAdaptor_Surface(face.wrapped, True)
    witness: dict[str, Any] = {"surface_kind": _surface_kind(face, expected_role), "area_mm2": float(face.Area()), "centroid": _point(face.Center())}
    if adaptor.GetType() == GeomAbs_Plane:
        witness["normal"] = _unit_vector(_point(face.normalAt()), "semantic face normal", "")
    bounds = face.BoundingBox()
    witness["bounds"] = ((float(bounds.xmin), float(bounds.ymin), float(bounds.zmin)), (float(bounds.xmax), float(bounds.ymax), float(bounds.zmax)))
    return witness


def _face_matches_generated_qualifier(face: cq.Face, qualifier: dict[str, Any], role: str) -> bool:
    witness = _face_witness(face, role)
    if qualifier["surface_kind"] is not None and witness["surface_kind"] != qualifier["surface_kind"]:
        return False
    if qualifier["area_mm2"] is not None:
        area_tolerance = qualifier["area_tolerance_mm2"] or 1.0e-5
        if abs(witness["area_mm2"] - qualifier["area_mm2"]) > area_tolerance:
            return False
    if qualifier["centroid"] is not None and _distance3(witness["centroid"], qualifier["centroid"]) > qualifier["tolerance_mm"]:
        return False
    if qualifier["normal"] is not None:
        if witness.get("normal") is None or _distance3(witness["normal"], qualifier["normal"]) > 1.0e-7:
            return False
    return True


def _cap_profile_frame(operation: dict[str, Any], role: str) -> dict[str, tuple[float, float, float]]:
    if operation["kind"] == "sweep":
        return _sweep_profile_frame(operation, "start" if role == "sweep_start_cap" else "end")
    cap = "start" if role == "loft_start_cap" else "end"
    return operation["sections"][0 if cap == "start" else -1]["plane"]


def _unique_cap_face(shape: cq.Shape, reference: dict[str, Any], operation: dict[str, Any], feature_id: str) -> cq.Face:
    selector = reference["selector"]
    role = selector["kind"] if isinstance(selector, Mapping) else selector
    expected_plane, loops, _ = _generated_cap_definition(operation, role, feature_id)
    expected_edges = [_edge_from_profile_curve(curve, _cap_profile_frame(operation, role), feature_id) for loop in loops for curve in loop]
    target_normal = reference["normal"]
    expected_offset = _dot(target_normal, expected_plane["origin"])
    if _distance3(target_normal, expected_plane["normal"]) > 1.0e-7 or abs(reference["plane_offset"] - expected_offset) > reference["tolerance"]:
        raise RebuildError("SOURCE_TOPOLOGY_CHANGED", "generated cap plane no longer matches its semantic role", feature_id)
    matches = []
    for face in shape.Faces():
        adaptor = BRepAdaptor_Surface(face.wrapped, True)
        if adaptor.GetType() != GeomAbs_Plane:
            continue
        try:
            normal = _unit_vector(_point(face.normalAt()), "candidate cap normal", feature_id)
        except Exception:
            continue
        if _distance3(normal, target_normal) > 1.0e-7 or abs(_dot(_point(face.Center()), target_normal) - reference["plane_offset"]) > reference["tolerance"]:
            continue
        face_edges = face.Edges()
        if not all(any(_edges_same_geometry(expected, candidate, reference["tolerance"]) for candidate in face_edges) for expected in expected_edges):
            continue
        if not _face_matches_generated_qualifier(face, reference["generated_qualifier"], role):
            continue
        matches.append(face)
    if len(matches) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", f"generated cap role {role} matches multiple planar faces", feature_id)
    if not matches:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", f"generated cap role {role} did not match a unique planar boundary", feature_id)
    return matches[0]


def _edges_on_shape(shape: cq.Shape, expected: cq.Edge, tolerance: float) -> list[cq.Edge]:
    return [edge for edge in shape.Edges() if _edges_same_geometry(expected, edge, tolerance)]


def _adjacent_faces(shape: cq.Shape, edge: cq.Edge) -> list[cq.Face]:
    try:
        return edge.ancestors(shape, "Face").Faces()
    except Exception:
        return []


def _common_faces(first: list[cq.Face], second: list[cq.Face]) -> list[cq.Face]:
    return [face for face in first if any(face.isSame(other) for other in second)]


def _circle_arcs_on_curve(shape: cq.Shape, expected: cq.Edge, tolerance: float) -> list[cq.Edge]:
    """Edges that lie on a circle even when a boolean kept only part of it.

    A full-length match is handled by `_edges_on_shape`. This fallback is only
    for the remaining arc: same center, radius, and axis, shorter length.
    """
    adaptor = BRepAdaptor_Curve(expected.wrapped)
    if adaptor.GetType() != GeomAbs_Circle:
        return []
    circle = adaptor.Circle()
    center = _point(circle.Location())
    radius = float(circle.Radius())
    axis = _point(circle.Axis().Direction())
    matches = []
    for edge in shape.Edges():
        current = BRepAdaptor_Curve(edge.wrapped)
        if current.GetType() != GeomAbs_Circle:
            continue
        other = current.Circle()
        if abs(float(other.Radius()) - radius) > tolerance:
            continue
        if _distance3(_point(other.Location()), center) > tolerance:
            continue
        other_axis = _point(other.Axis().Direction())
        if _distance3(other_axis, axis) > 1e-7 and _distance3(other_axis, tuple(-value for value in axis)) > 1e-7:
            continue
        matches.append(edge)
    return matches


def _incident_faces(shape: cq.Shape, edges: list[cq.Edge]) -> list[cq.Face]:
    faces: list[cq.Face] = []
    for edge in edges:
        for face in _adjacent_faces(shape, edge):
            if not any(face.isSame(existing) for existing in faces):
                faces.append(face)
    return faces


def _unique_sweep_lateral_face(shape: cq.Shape, reference: dict[str, Any], operation: dict[str, Any], feature_id: str) -> cq.Face:
    entity_id = reference["selector"]["profile_entity_id"]
    curves = []
    for ids, loop in zip(operation["profile_entity_ids"], operation["loops"]):
        for index, current_id in enumerate(ids):
            if current_id == entity_id:
                curves.append(loop[index])
    if len(curves) != 1:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "Sweep profile entity is absent or ambiguous", feature_id)
    tolerance = reference["qualifier"]["tolerance_mm"]
    start_frame = _sweep_profile_frame(operation, "start")
    end_frame = _sweep_profile_frame(operation, "end")
    start_expected = _edge_from_profile_curve(curves[0], start_frame, feature_id)
    end_expected = _edge_from_profile_curve(curves[0], end_frame, feature_id)
    start_exact = _edges_on_shape(shape, start_expected, tolerance)
    end_exact = _edges_on_shape(shape, end_expected, tolerance)
    if len(start_exact) > 1 or len(end_exact) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "Sweep profile boundary entity matches multiple cap edges", feature_id)
    # Fuse can delete part of the start circle and leave the arc that is still
    # on the boundary. That arc is the same profile entity, not a new one.
    start_edges = start_exact or _circle_arcs_on_curve(shape, start_expected, tolerance)
    end_edges = end_exact or _circle_arcs_on_curve(shape, end_expected, tolerance)
    if not start_edges or not end_edges:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "Sweep lateral face boundary no longer exists", feature_id)
    candidates = _common_faces(_incident_faces(shape, start_edges), _incident_faces(shape, end_edges))
    candidates = [face for face in candidates if _face_matches_generated_qualifier(face, reference["qualifier"], "sweep_lateral")]
    if len(candidates) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "Sweep lateral role matches multiple faces", feature_id)
    if not candidates:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "Sweep lateral role did not resolve to a unique face", feature_id)
    return candidates[0]


def _loft_lateral_source(operation: dict[str, Any], selector: dict[str, Any], feature_id: str) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]]:
    sections = operation["sections"]
    first_id, second_id = selector["start_section_id"], selector["end_section_id"]
    positions = [index for index, section in enumerate(sections) if section["sketch_id"] in {first_id, second_id}]
    if len(positions) != 2 or sections[positions[0]]["sketch_id"] != first_id or sections[positions[1]]["sketch_id"] != second_id or positions[1] != positions[0] + 1:
        raise RebuildError("SOURCE_TOPOLOGY_CHANGED", "Loft lateral reference sections are no longer adjacent and ordered", feature_id)
    first, second = sections[positions[0]], sections[positions[1]]
    def entity_curve(section: dict[str, Any], entity_id: str) -> dict[str, Any] | None:
        if len(section["loops"]) != 1 or len(section["profile_entity_ids"]) != 1:
            return None
        for identity, curve in zip(section["profile_entity_ids"][0], section["loops"][0]):
            if identity == entity_id:
                return curve
        return None
    first_curve = entity_curve(first, selector["start_entity_id"])
    second_curve = entity_curve(second, selector["end_entity_id"])
    if first_curve is None or second_curve is None:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "Loft lateral profile entity is missing from a declared section", feature_id)
    if first_curve["kind"] != second_curve["kind"]:
        raise RebuildError("SOURCE_TOPOLOGY_CHANGED", "Loft lateral section entities changed profile correspondence", feature_id)
    return first, second, first_curve, second_curve


def _unique_loft_lateral_face(shape: cq.Shape, reference: dict[str, Any], operation: dict[str, Any], feature_id: str) -> cq.Face:
    first, second, first_curve, second_curve = _loft_lateral_source(operation, reference["selector"], feature_id)
    tolerance = reference["qualifier"]["tolerance_mm"]
    first_edge_values = _edges_on_shape(shape, _edge_from_profile_curve(first_curve, first["plane"], feature_id), tolerance)
    second_edge_values = _edges_on_shape(shape, _edge_from_profile_curve(second_curve, second["plane"], feature_id), tolerance)
    if len(first_edge_values) > 1 or len(second_edge_values) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "Loft section profile entity matches multiple boundary edges", feature_id)
    if not first_edge_values or not second_edge_values:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "Loft lateral section boundary no longer exists", feature_id)
    candidates = _common_faces(_adjacent_faces(shape, first_edge_values[0]), _adjacent_faces(shape, second_edge_values[0]))
    candidates = [face for face in candidates if _face_matches_generated_qualifier(face, reference["qualifier"], "loft_lateral")]
    if len(candidates) > 1:
        raise RebuildError("AMBIGUOUS_TOPOLOGY_REFERENCE", "Loft lateral role matches multiple faces", feature_id)
    if not candidates:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "Loft lateral role did not resolve to a unique face", feature_id)
    return candidates[0]


def _unique_generated_face(shape: cq.Shape, reference: dict[str, Any], feature_id: str, snapshots: Mapping[str, cq.Shape], source_operations: Mapping[str, list[dict[str, Any]]]) -> cq.Face:
    source_id = reference["source_feature_id"]
    if snapshots.get(source_id) is None:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", f"generated face source {source_id} is not present in the rebuild prefix", feature_id)
    operation = _generated_face_role_operation(reference, source_operations, feature_id)
    role = reference["selector"]["kind"]
    if role in {"sweep_start_cap", "sweep_end_cap", "loft_start_cap", "loft_end_cap"}:
        expected_plane, _loops, _kind = _generated_cap_definition(operation, role, feature_id)
        normal = expected_plane["normal"]
        cap_reference = {
            **reference,
            "normal": normal,
            "plane_offset": _dot(normal, expected_plane["origin"]),
            "tolerance": reference["qualifier"]["tolerance_mm"],
            "generated_qualifier": reference["qualifier"],
        }
        return _unique_cap_face(shape, cap_reference, operation, feature_id)
    if role == "sweep_lateral":
        return _unique_sweep_lateral_face(shape, reference, operation, feature_id)
    if role == "loft_lateral":
        return _unique_loft_lateral_face(shape, reference, operation, feature_id)
    raise RebuildError("UNSUPPORTED_TOPOLOGY_REFERENCE", f"generated face role {role} is unsupported", feature_id)


def _resolve_face_reference(operation: dict[str, Any], snapshots: dict[str, cq.Shape], source_operations: Mapping[str, list[dict[str, Any]]], current_body: cq.Shape | None) -> None:
    reference = operation["support_face"]
    source_id = reference["source_feature_id"]
    feature_id = operation["feature_id"]
    source = snapshots.get(source_id)
    if source is None:
        raise RebuildError("TOPOLOGY_REFERENCE_NOT_FOUND", "semantic face source feature is not present in the rebuild prefix", feature_id)
    if reference["selector"] in {"sweep_start_cap", "sweep_end_cap", "loft_start_cap", "loft_end_cap"}:
        selector = {"kind": reference["selector"]}
        semantic = {"source_feature_id": source_id, "selector": selector, "qualifier": reference["generated_qualifier"]}
        _unique_generated_face(source, semantic, feature_id, snapshots, source_operations)
        try:
            _unique_generated_face_for_cap(current_body, semantic, reference, source_operations, feature_id)
        except RebuildError as error:
            if error.code in {"TOPOLOGY_REFERENCE_NOT_FOUND", "AMBIGUOUS_TOPOLOGY_REFERENCE"}:
                raise RebuildError("TOPOLOGY_REFERENCE_STALE", "generated cap role changed or is no longer unique on the current body", feature_id) from error
            raise
        return
    matches = []
    oriented_candidates = 0
    for face in source.Faces():
        adaptor = BRepAdaptor_Surface(face.wrapped, True)
        if adaptor.GetType() != GeomAbs_Plane:
            continue
        try:
            normal_value = face.normalAt()
            normal = _unit_vector(_point(normal_value), "candidate face normal", feature_id)
        except Exception:
            continue
        if _distance3(normal, reference["normal"]) > 1e-7:
            continue
        oriented_candidates += 1
        center = _point(face.Center())
        offset = _dot(center, reference["normal"])
        if abs(offset - reference["plane_offset"]) <= reference["tolerance"]:
            matches.append(face)
    if len(matches) > 1:
        raise RebuildError("AMBIGUOUS_FACE_REFERENCE", "semantic face selector matched more than one planar face", feature_id)
    if not matches:
        code = "INVALID_FACE_REFERENCE" if oriented_candidates else "UNSUPPORTED_SUPPORT_SURFACE"
        raise RebuildError(code, "semantic face selector did not resolve exactly one planar face with the qualified orientation and offset", feature_id)


def _unique_generated_face_for_cap(shape: cq.Shape, semantic: dict[str, Any], plane_reference: dict[str, Any], source_operations: Mapping[str, list[dict[str, Any]]], feature_id: str) -> cq.Face:
    if shape is None:
        raise RebuildError("TOPOLOGY_REFERENCE_STALE", "current body is unavailable for generated cap verification", feature_id)
    operation = _generated_face_role_operation(semantic, source_operations, feature_id)
    return _unique_cap_face(shape, {**semantic, "normal": plane_reference["normal"], "plane_offset": plane_reference["plane_offset"], "tolerance": plane_reference["tolerance"], "generated_qualifier": plane_reference["generated_qualifier"]}, operation, feature_id)


def _build_profile_face(plane: dict[str, tuple[float, float, float]], loops: list[list[dict[str, Any]]], feature_id: str) -> cq.Face:
    try:
        wires = [_build_wire(loop, plane, feature_id) for loop in loops]
        face = cq.Face.makeFromWires(wires[0], wires[1:])
    except RebuildError:
        raise
    except Exception as error:
        code = "INVALID_INNER_LOOP" if len(loops) > 1 else "SELF_INTERSECTION"
        raise RebuildError(code, _safe_error(error), feature_id) from error
    if face is None or not face.isValid():
        raise RebuildError("SELF_INTERSECTION", "profile loops do not bound a valid plane face", feature_id)
    if not math.isfinite(float(face.Area())) or float(face.Area()) <= TOLERANCE:
        raise RebuildError("INVALID_SKETCH", "profile area is zero or non-finite", feature_id)
    for inner in wires[1:]:
        if float(cq.Face.makeFromWires(wires[0], [inner]).Area()) >= float(cq.Face.makeFromWires(wires[0]).Area()) - TOLERANCE:
            raise RebuildError("INVALID_INNER_LOOP", "inner loop is outside the outer profile", feature_id)
    return face


def _validate_revolve_profile(operation: dict[str, Any], feature_id: str) -> None:
    plane = operation["plane"]
    origin = operation["axis_origin"]
    direction = operation["direction"]
    normal = plane["normal"]
    if abs(_dot(direction, normal)) > 1.0e-8:
        raise RebuildError("REVOLVE_PROFILE_INVALID", "revolve axis must lie in the Sketch profile plane", feature_id)
    plane_distance = abs(_dot(tuple(origin[i] - plane["origin"][i] for i in range(3)), normal))
    if plane_distance > 1.0e-6:
        raise RebuildError("REVOLVE_PROFILE_INVALID", "revolve axis line does not lie in the Sketch profile plane", feature_id)
    du, dv = _dot(direction, plane["u_axis"]), _dot(direction, plane["v_axis"])
    axis_length = math.hypot(du, dv)
    if not math.isfinite(axis_length) or axis_length <= TOLERANCE:
        raise RebuildError("INVALID_DATUM_AXIS", "revolve axis direction is degenerate in the profile plane", feature_id)
    radial = (-dv / axis_length, du / axis_length)
    delta = tuple(origin[i] - plane["origin"][i] for i in range(3))
    axis_point = (_dot(delta, plane["u_axis"]), _dot(delta, plane["v_axis"]))

    def signed_radius(point: tuple[float, float]) -> float:
        return (point[0] - axis_point[0]) * radial[0] + (point[1] - axis_point[1]) * radial[1]

    values: list[float] = []
    for loop in operation["loops"]:
        for curve in loop:
            if curve["kind"] == "line":
                values.extend((signed_radius(curve["start"]), signed_radius(curve["end"])))
            elif curve["kind"] == "circle":
                center = signed_radius(curve["center"])
                values.extend((center - curve["radius"], center + curve["radius"]))
            else:
                start, end, center, radius = curve["start"], curve["end"], curve["center"], curve["radius"]
                a0 = math.atan2(start[1] - center[1], start[0] - center[0])
                a1 = math.atan2(end[1] - center[1], end[0] - center[0])
                direction_name = curve["direction"]
                sweep = (a1 - a0) % math.tau if direction_name == "counter_clockwise" else (a0 - a1) % math.tau
                center_radius = signed_radius(center)
                phase = math.atan2(radial[1], radial[0])
                angles = [a0, a1]
                for extremum in (phase, phase + math.pi):
                    delta_angle = (extremum - a0) % math.tau if direction_name == "counter_clockwise" else (a0 - extremum) % math.tau
                    if delta_angle <= sweep + 1.0e-10:
                        angles.append(extremum)
                values.extend(center_radius + radius * math.cos(angle - phase) for angle in angles)
    if not values or not all(math.isfinite(value) for value in values):
        raise RebuildError("REVOLVE_PROFILE_INVALID", "revolve profile radial bounds are not finite", feature_id)
    if min(values) < -1.0e-7 and max(values) > 1.0e-7:
        raise RebuildError("REVOLVE_PROFILE_CROSSES_AXIS", "closed profile crosses the rotation axis; move the profile wholly to one side of the axis", feature_id)


def _revolve_tool(operation: dict[str, Any], feature_id: str) -> cq.Solid:
    _validate_revolve_profile(operation, feature_id)
    _build_profile_face(operation["plane"], operation["loops"], feature_id)
    wires = [_build_wire(loop, operation["plane"], feature_id) for loop in operation["loops"]]
    angle = 360.0 if operation["angle"]["kind"] == "full_360" else operation["angle"]["degrees"]
    direction = operation["direction"]
    if operation["orientation"] == "reverse":
        direction = tuple(-value for value in direction)
    axis_start = cq.Vector(*operation["axis_origin"])
    axis_end = cq.Vector(*(operation["axis_origin"][i] + direction[i] for i in range(3)))
    try:
        tool = cq.Solid.revolve(wires[0], wires[1:], angle, axis_start, axis_end)
    except Exception as error:
        raise RebuildError("REVOLVE_GEOMETRY_FAILED", _safe_error(error), feature_id) from error
    if tool is None or not tool.isValid():
        raise RebuildError("REVOLVE_SELF_INTERSECTION", "revolve produced an invalid or self-intersecting B-Rep", feature_id)
    if len(tool.Solids()) != 1:
        raise RebuildError("MULTI_BODY_NOT_ALLOWED", "revolve must produce exactly one connected solid", feature_id)
    if not math.isfinite(float(tool.Volume())) or float(tool.Volume()) <= TOLERANCE:
        raise RebuildError("REVOLVE_GEOMETRY_FAILED", "revolve produced zero or non-finite material volume", feature_id)
    return tool


def _build_path_wire(operation: Mapping[str, Any]) -> cq.Wire:
    feature_id=operation["feature_id"]
    curves=operation["path"] if operation["direction"]=="forward" else _reverse_path_curves(operation["path"])
    plane=operation["path_plane"]
    edges=[]
    for curve in curves:
        if curve["kind"]=="line":
            edges.append(cq.Edge.makeLine(cq.Vector(*_world_point(plane,curve["start"])),cq.Vector(*_world_point(plane,curve["end"]))))
        else:
            start,end,center,radius=curve["start"],curve["end"],curve["center"],curve["radius"]
            a0=math.atan2(start[1]-center[1],start[0]-center[0]); a1=math.atan2(end[1]-center[1],end[0]-center[0])
            sweep=(a1-a0)%math.tau if curve["direction"]=="counter_clockwise" else (a0-a1)%math.tau
            amid=a0+(sweep/2 if curve["direction"]=="counter_clockwise" else -sweep/2)
            mid=(center[0]+radius*math.cos(amid),center[1]+radius*math.sin(amid))
            edges.append(cq.Edge.makeThreePointArc(cq.Vector(*_world_point(plane,start)),cq.Vector(*_world_point(plane,mid)),cq.Vector(*_world_point(plane,end))))
    builder=BRepBuilderAPI_MakeWire()
    for edge in edges:builder.Add(edge.wrapped)
    if not builder.IsDone():raise RebuildError("PATH_INVALID","OCP could not assemble the ordered Path wire",feature_id)
    wire=cq.Wire(builder.Wire())
    if wire.IsClosed() or not wire.isValid() or len(wire.Edges())!=len(edges):
        raise RebuildError("PATH_INVALID","OCP Path wire is closed, invalid, or changed its declared edge count",feature_id)
    return wire


def _sweep_tool(operation: Mapping[str, Any]) -> cq.Shape:
    feature_id=operation["feature_id"]
    face=_build_profile_face(operation["plane"],operation["loops"],feature_id)
    anchor=_world_point(operation["plane"],operation["profile_origin"])
    classifier=BRepClass_FaceClassifier(face.wrapped,gp_Pnt(*anchor),1e-7)
    if classifier.State()!=TopAbs_IN:
        raise RebuildError("SWEEP_PROFILE_ANCHOR_INVALID","profile anchor must lie in the material region of the closed Sketch profile",feature_id)
    wires=[_build_wire(loop,operation["plane"],feature_id) for loop in operation["loops"]]
    path=_build_path_wire(operation)
    try:
        # OCP's corrected non-Frenet mode transports a planar section with the
        # declared path-plane binormal. C1-only joins make transition handling irrelevant.
        tool=cq.Solid.sweep(wires[0],wires[1:],path,makeSolid=True,isFrenet=False,mode=None,transitionMode="transformed")
    except RebuildError:
        raise
    except Exception as error:
        raise RebuildError("SWEEP_GEOMETRY_FAILED",_safe_error(error),feature_id) from error
    if tool is None or not tool.isValid() or len(tool.Solids())!=1:
        raise RebuildError("SWEEP_SELF_INTERSECTION","Sweep produced an invalid or disconnected solid",feature_id)
    if not math.isfinite(float(tool.Volume())) or float(tool.Volume())<=TOLERANCE:
        raise RebuildError("SWEEP_GEOMETRY_FAILED","Sweep produced zero or non-finite material volume",feature_id)
    return tool


def _loft_tool(operation: Mapping[str, Any]) -> cq.Solid:
    feature_id = operation["feature_id"]
    wires = []
    expected_signature = None
    for section in operation["sections"]:
        loops = section["loops"]
        if len(loops) != 1:
            raise RebuildError("LOFT_INNER_LOOPS_UNSUPPORTED", "Loft section contains an unsupported inner loop", feature_id)
        signature = tuple(curve["kind"] for curve in loops[0])
        if expected_signature is not None and signature != expected_signature:
            raise RebuildError("LOFT_PROFILE_INCOMPATIBLE", "Loft wires do not have verified one-to-one curve correspondence", feature_id)
        expected_signature = signature
        face = _build_profile_face(section["plane"], loops, feature_id)
        if not face.isValid() or not math.isfinite(float(face.Area())) or float(face.Area()) <= TOLERANCE:
            raise RebuildError("LOFT_PROFILE_DEGENERATE", "Loft section does not form a valid nonzero planar face", feature_id)
        wires.append(_build_wire(loops[0], section["plane"], feature_id))
    if len(wires) < 2:
        raise RebuildError("LOFT_SECTION_COUNT_INVALID", "Loft needs at least two sections", feature_id)
    try:
        from OCP.BRepOffsetAPI import BRepOffsetAPI_ThruSections
        builder = BRepOffsetAPI_ThruSections(True, True, TOLERANCE)
        # The EngineeringModel and request validators already prove exact
        # one-to-one section curve correspondence; do not let OCP resample or
        # rotate wires to guess a different correspondence.
        builder.CheckCompatibility(False)
        for wire in wires:
            builder.AddWire(wire.wrapped)
        builder.Build()
        if not builder.IsDone():
            raise RebuildError("LOFT_GEOMETRY_FAILED", "OCP could not complete the ruled section transition", feature_id)
        tool = cq.Solid(builder.Shape())
    except RebuildError:
        raise
    except Exception as error:
        raise RebuildError("LOFT_GEOMETRY_FAILED", _safe_error(error), feature_id) from error
    if tool is None or not tool.isValid() or len(tool.Solids()) != 1:
        raise RebuildError("LOFT_GEOMETRY_INVALID", "Loft must produce exactly one valid solid", feature_id)
    if not math.isfinite(float(tool.Volume())) or float(tool.Volume()) <= TOLERANCE:
        raise RebuildError("LOFT_GEOMETRY_FAILED", "Loft produced zero or non-finite material volume", feature_id)
    return tool


def _build_wire(curves: list[dict[str, Any]], plane: dict[str, tuple[float, float, float]], feature_id: str) -> cq.Wire:
    edges = []
    for curve in curves:
        if curve["kind"] == "circle":
            center = _world_point(plane, curve["center"])
            edges.append(cq.Edge.makeCircle(curve["radius"], cq.Vector(*center), cq.Vector(*plane["normal"])))
        elif curve["kind"] == "line":
            edges.append(cq.Edge.makeLine(cq.Vector(*_world_point(plane, curve["start"])), cq.Vector(*_world_point(plane, curve["end"]))))
        else:
            start, end, center, radius = curve["start"], curve["end"], curve["center"], curve["radius"]
            a0 = math.atan2(start[1] - center[1], start[0] - center[0])
            a1 = math.atan2(end[1] - center[1], end[0] - center[0])
            sweep = (a1 - a0) % math.tau if curve["direction"] == "counter_clockwise" else (a0 - a1) % math.tau
            mid_angle = a0 + (sweep / 2.0 if curve["direction"] == "counter_clockwise" else -sweep / 2.0)
            mid = (center[0] + radius * math.cos(mid_angle), center[1] + radius * math.sin(mid_angle))
            edges.append(cq.Edge.makeThreePointArc(
                cq.Vector(*_world_point(plane, start)),
                cq.Vector(*_world_point(plane, mid)),
                cq.Vector(*_world_point(plane, end)),
            ))
    builder = BRepBuilderAPI_MakeWire()
    for edge in edges:
        builder.Add(edge.wrapped)
    if not builder.IsDone():
        raise RebuildError("OPEN_PROFILE", "kernel could not assemble a closed profile wire", feature_id)
    wire = cq.Wire(builder.Wire())
    if not wire.IsClosed() or not wire.isValid():
        raise RebuildError("OPEN_PROFILE", "profile wire is not closed and valid", feature_id)
    return wire


def _extrude_tool(face: cq.Face, body: cq.Shape | None, operation: dict[str, Any], feature_id: str) -> cq.Shape:
    direction = operation["direction"]
    extent = operation["extent"]
    if extent["kind"] == "distance":
        length = extent["length"]
        start_face = face
    elif extent["kind"] == "symmetric":
        length = extent["length"]
        shift = -0.5 * length
        start_face = face.translate(cq.Vector(*(shift * value for value in direction)))
    else:
        if operation["operation"] != "cut" or body is None:
            raise RebuildError("INVALID_EXTRUDE_EXTENT", "THROUGH_ALL requires an existing body and CUT", feature_id)
        bounds = body.BoundingBox()
        corners = [
            (x, y, z)
            for x in (bounds.xmin, bounds.xmax)
            for y in (bounds.ymin, bounds.ymax)
            for z in (bounds.zmin, bounds.zmax)
        ]
        projections = [_dot(point, direction) for point in corners]
        plane_projection = _dot(operation["plane"]["origin"], direction)
        diagonal = math.sqrt((bounds.xmax - bounds.xmin) ** 2 + (bounds.ymax - bounds.ymin) ** 2 + (bounds.zmax - bounds.zmin) ** 2)
        envelope = max(1.0, diagonal * 0.01)
        start_projection = min(projections) - envelope
        end_projection = max(projections) + envelope
        shift = start_projection - plane_projection
        start_face = face.translate(cq.Vector(*(shift * value for value in direction)))
        length = end_projection - start_projection
    try:
        tool = cq.Solid.extrudeLinear(start_face, cq.Vector(*(length * value for value in direction)))
    except Exception as error:
        raise RebuildError("BOOLEAN_FAILED", _safe_error(error), feature_id) from error
    if not tool.isValid() or not tool.Solids() or float(tool.Volume()) <= TOLERANCE:
        raise RebuildError("BODY_INVALID", "extrusion did not produce a valid solid", feature_id)
    if len(tool.Solids()) != 1:
        raise RebuildError("MULTI_BODY_NOT_ALLOWED", "extrusion produced more than one connected solid", feature_id)
    return tool


def _validate_body(shape: cq.Shape, feature_id: str | None) -> None:
    if shape is None or not shape.isValid():
        raise RebuildError("BODY_INVALID", "boolean result is not a valid B-Rep", feature_id)
    solids = shape.Solids()
    if len(solids) != 1:
        raise RebuildError("MULTI_BODY_NOT_ALLOWED", "boolean result must be one connected solid", feature_id)
    bounds = shape.BoundingBox()
    volume = float(shape.Volume())
    values = [bounds.xmin, bounds.xmax, bounds.ymin, bounds.ymax, bounds.zmin, bounds.zmax, volume]
    if not all(math.isfinite(float(value)) for value in values) or volume <= TOLERANCE:
        raise RebuildError("BODY_INVALID", "boolean result has invalid bounds or volume", feature_id)


def _result(
    shape: cq.Shape,
    model_sha256: str,
    operations: list[dict[str, Any]],
    operation_features: list[str],
    edge_provenance: Mapping[str, set[str]],
    face_provenance: Mapping[str, set[str]],
    operation_timings: list[dict[str, Any]],
) -> dict[str, Any]:
    bounds = shape.BoundingBox()
    bbox = {
        "x_min": _rounded(bounds.xmin), "x_max": _rounded(bounds.xmax),
        "y_min": _rounded(bounds.ymin), "y_max": _rounded(bounds.ymax),
        "z_min": _rounded(bounds.zmin), "z_max": _rounded(bounds.zmax),
    }
    frame = _body_frame(shape, operations[0])
    edge_evidence = _edge_evidence(shape, edge_provenance)
    face_evidence = _face_evidence(shape, face_provenance)
    view_evidence = {}
    for name in VIEWS:
        view_evidence[name] = _hlr_evidence(shape, name)
    evidence = {
        "model_sha256": model_sha256,
        "frame": frame,
        "volume_mm3": _rounded(shape.Volume()),
        "operation_features": operation_features,
        "edges": edge_evidence,
        "faces": face_evidence,
        "views": view_evidence,
    }
    if not edge_evidence or not face_evidence or any(not edges for edges in view_evidence.values()):
        raise RebuildError("BODY_INVALID", "validated body did not yield complete exact spatial evidence")
    return {
        "schema_version": SCHEMA_VERSION,
        "valid": True,
        "body_count": 1,
        "solid_count": 1,
        "operation_count": len(operations),
        "operation_timings": operation_timings,
        "bbox": bbox,
        "spatial_evidence": evidence,
    }


def _edge_evidence(shape: cq.Shape, provenance: Mapping[str, set[str]] | None = None) -> list[dict[str, Any]]:
    records = []
    for edge in shape.Edges():
        adaptor = BRepAdaptor_Curve(edge.wrapped)
        kind = adaptor.GetType()
        if kind == GeomAbs_Line:
            start, end = _point(edge.startPoint()), _point(edge.endPoint())
            if tuple(end) < tuple(start):
                start, end = end, start
            curve = {"kind": "line", "start": start, "end": end}
            descriptor = curve
        elif kind == GeomAbs_Circle:
            circle = adaptor.Circle()
            center = _point(circle.Location())
            axis = _point(circle.Axis().Direction())
            start, end = _point(edge.startPoint()), _point(edge.endPoint())
            if tuple(end) < tuple(start):
                start, end = end, start
            radius = float(circle.Radius())
            curve = {"kind": "circular", "center": center, "axis": _canonical_direction(axis), "radius": _rounded(radius), "start": start, "end": end}
            descriptor = curve
        elif kind == GeomAbs_Ellipse:
            ellipse = adaptor.Ellipse()
            start, end = _point(edge.startPoint()), _point(edge.endPoint())
            if tuple(end) < tuple(start):
                start, end = end, start
            curve = {
                "kind": "elliptical", "center": _point(ellipse.Location()),
                "axis": _canonical_direction(_point(ellipse.Axis().Direction())),
                "major_axis": _canonical_direction(_point(ellipse.XAxis().Direction())),
                "major_radius": _rounded(float(ellipse.MajorRadius())),
                "minor_radius": _rounded(float(ellipse.MinorRadius())), "start": start, "end": end,
            }
            descriptor = curve
        elif kind in {GeomAbs_BSplineCurve, GeomAbs_BezierCurve}:
            points = _sample_spatial_curve(edge, SPATIAL_CURVE_MAX_DEVIATION_MM)
            if tuple(points[-1]) < tuple(points[0]) or (
                tuple(points[-1]) == tuple(points[0]) and tuple(reversed(points)) < tuple(points)
            ):
                points.reverse()
            if any(first == second for first, second in zip(points, points[1:])):
                points = [point for index, point in enumerate(points) if index == 0 or point != points[index - 1]]
            if len(points) < 2:
                raise RebuildError("BODY_INVALID", "B-Spline edge collapsed under the bounded Spatial sampling contract")
            curve = {
                "kind": "sampled_curve",
                "points": points,
                "max_deviation_mm": SPATIAL_CURVE_MAX_DEVIATION_MM,
            }
            descriptor = curve
        else:
            raise RebuildError("BODY_INVALID", f"B-Rep contains unsupported edge curve type {kind!r}")
        canonical = json.dumps(descriptor, sort_keys=True, separators=(",", ":"))
        records.append({"descriptor": descriptor, "curve": curve, "length_mm": _rounded(edge.Length()), "key": canonical})
    result = _stable_records(records, "edge")
    if provenance is not None:
        for record in result:
            digest = record["id"].split(":")[1]
            record["source_features"] = sorted(provenance.get(digest, set()))
    return result


def _sample_spatial_curve(edge: cq.Edge, tolerance: float) -> list[list[float]]:
    max_depth = 20

    def point_at(fraction: float) -> tuple[float, float, float]:
        return _point(edge.positionAt(fraction))

    def distance_to_segment(point: tuple[float, ...], start: tuple[float, ...], end: tuple[float, ...]) -> float:
        direction = tuple(end[index] - start[index] for index in range(3))
        offset = tuple(point[index] - start[index] for index in range(3))
        length_squared = sum(value * value for value in direction)
        if length_squared <= TOLERANCE * TOLERANCE:
            return _distance3(point, start)
        fraction = max(0.0, min(1.0, sum(offset[index] * direction[index] for index in range(3)) / length_squared))
        projection = tuple(start[index] + fraction * direction[index] for index in range(3))
        return _distance3(point, projection)

    output = [point_at(0.0)]

    def refine(t0: float, p0: tuple[float, ...], t1: float, p1: tuple[float, ...], depth: int) -> None:
        span = t1 - t0
        probes = [
            (t0 + span * fraction, point_at(t0 + span * fraction))
            for fraction in (0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875)
        ]
        error = max(distance_to_segment(point, p0, p1) for _, point in probes)
        if error <= tolerance:
            output.append(p1)
            if len(output) > MAX_SPATIAL_CURVE_SAMPLE_POINTS:
                raise RebuildError("BODY_INVALID", "B-Spline edge exceeded the bounded Spatial sample-count limit")
            return
        if depth >= max_depth:
            raise RebuildError("BODY_INVALID", "B-Spline edge exceeded the bounded Spatial sampling depth")
        midpoint_t, midpoint = probes[3]
        refine(t0, p0, midpoint_t, midpoint, depth + 1)
        refine(midpoint_t, midpoint, t1, p1, depth + 1)

    initial_intervals = 8
    initial_points = [point_at(index / initial_intervals) for index in range(initial_intervals + 1)]
    output = [initial_points[0]]
    for index in range(initial_intervals):
        refine(index / initial_intervals, initial_points[index], (index + 1) / initial_intervals,
               initial_points[index + 1], 0)
    points = [[_rounded(float(value)) for value in point] for point in output]
    deduplicated = [points[0]]
    for point in points[1:]:
        if point != deduplicated[-1]:
            deduplicated.append(point)
    if len(deduplicated) < 2:
        raise RebuildError("BODY_INVALID", "B-Spline edge collapsed under the bounded Spatial sampling contract")
    return deduplicated


def _face_evidence(shape: cq.Shape, provenance: Mapping[str, set[str]] | None = None) -> list[dict[str, Any]]:
    records = []
    for face in shape.Faces():
        adaptor = BRepAdaptor_Surface(face.wrapped, True)
        kind = adaptor.GetType()
        if kind == GeomAbs_Plane:
            plane = adaptor.Plane()
            surface = {"kind": "plane", "origin": _point(plane.Location()), "normal": _point(plane.Axis().Direction())}
        elif kind == GeomAbs_Cylinder:
            cylinder = adaptor.Cylinder()
            surface = {
                "kind": "cylinder", "axis_origin": _point(cylinder.Location()),
                "axis": _canonical_direction(_point(cylinder.Axis().Direction())), "radius": _rounded(cylinder.Radius()),
            }
        elif kind == GeomAbs_Cone:
            cone = adaptor.Cone()
            raw_axis = _point(cone.Axis().Direction())
            canonical_axis = _canonical_direction(raw_axis)
            axis_orientation = 1.0 if _dot(raw_axis, canonical_axis) >= 0.0 else -1.0
            surface = {
                "kind": "cone", "axis_origin": _point(cone.Location()),
                "axis": canonical_axis,
                "reference_radius": _rounded(cone.RefRadius()),
                # OCP may reverse a face's parameter axis on STEP import.
                # Normalize the signed angle with the same axis direction so
                # the witness describes the same geometric cone, not its seam orientation.
                "semi_angle_radians": _rounded(cone.SemiAngle() * axis_orientation),
            }
        elif kind == GeomAbs_Torus:
            torus = adaptor.Torus()
            surface = {
                "kind": "torus", "center": _point(torus.Location()),
                "axis": _canonical_direction(_point(torus.Axis().Direction())),
                "major_radius": _rounded(torus.MajorRadius()),
                "minor_radius": _rounded(torus.MinorRadius()),
            }
        elif kind == GeomAbs_BSplineSurface:
            # OCP emits B-Spline side patches for compatible ruled Loft
            # sections. Keep these as explicitly sampled derived evidence;
            # they are not promoted to editable or semantic topology refs.
            try:
                vertices, triangles = face.copy().tessellate(SPATIAL_CURVE_MAX_DEVIATION_MM, 0.1)
            except Exception as error:
                raise RebuildError("BODY_INVALID", "B-Spline surface exceeded the bounded Spatial tessellation contract") from error
            points = sorted({tuple(_rounded(float(value)) for value in vertex.toTuple()) for vertex in vertices})
            if not points or len(points) > MAX_SPATIAL_BSPLINE_FACE_SAMPLE_POINTS or not triangles:
                raise RebuildError("BODY_INVALID", "B-Spline surface exceeded the bounded Spatial sample-count contract")
            surface = {
                "kind": "sampled_bspline_surface",
                "vertices": points,
                "triangle_count": len(triangles),
                "linear_deviation_mm": SPATIAL_CURVE_MAX_DEVIATION_MM,
                "angular_tolerance_radians": 0.1,
            }
        else:
            raise RebuildError("BODY_INVALID", "B-Rep contains an unsupported face surface")
        bounds = face.BoundingBox()
        descriptor = {
            "surface": surface,
            "bounds_min": [_rounded(bounds.xmin), _rounded(bounds.ymin), _rounded(bounds.zmin)],
            "bounds_max": [_rounded(bounds.xmax), _rounded(bounds.ymax), _rounded(bounds.zmax)],
            "area_mm2": _rounded(face.Area()),
        }
        records.append({"descriptor": descriptor, **descriptor, "key": json.dumps(descriptor, sort_keys=True, separators=(",", ":"))})
    result = _stable_records(records, "face")
    if provenance is not None:
        for record in result:
            digest = record["id"].split(":")[1]
            record["source_features"] = sorted(provenance.get(digest, set()))
    return result


def _record_topology_provenance(
    shape: cq.Shape,
    feature_id: str,
    previous_edges: Counter[str],
    previous_faces: Counter[str],
    edge_provenance: defaultdict[str, set[str]],
    face_provenance: defaultdict[str, set[str]],
) -> tuple[Counter[str], Counter[str]]:
    current_edges = Counter(record["id"].split(":")[1] for record in _edge_evidence(shape))
    current_faces = Counter(record["id"].split(":")[1] for record in _face_evidence(shape))
    for key in previous_edges.keys() - current_edges.keys():
        edge_provenance.pop(key, None)
    for key, count in current_edges.items():
        if count > previous_edges.get(key, 0) or not edge_provenance.get(key):
            edge_provenance[key].add(feature_id)
    for key in previous_faces.keys() - current_faces.keys():
        face_provenance.pop(key, None)
    for key, count in current_faces.items():
        if count > previous_faces.get(key, 0) or not face_provenance.get(key):
            face_provenance[key].add(feature_id)
    return current_edges, current_faces


def _canonical_direction(value: tuple[float, ...]) -> tuple[float, float, float]:
    for component in value:
        if abs(component) > 1.0e-12:
            return tuple(value) if component > 0 else tuple(-item for item in value)
    return tuple(value)


def _stable_records(records: list[dict[str, Any]], prefix: str) -> list[dict[str, Any]]:
    records.sort(key=lambda record: record["key"])
    counts: defaultdict[str, int] = defaultdict(int)
    result = []
    for record in records:
        digest = hashlib.sha256(record["key"].encode("utf-8")).hexdigest()[:20]
        ordinal = counts[digest]
        counts[digest] += 1
        result.append({"id": f"{prefix}:{digest}:{ordinal:04d}", **{key: value for key, value in record.items() if key not in {"key", "descriptor"}}})
    return result


def _hlr_evidence(shape: cq.Shape, name: str) -> list[dict[str, Any]]:
    toward, right, up = VIEWS[name]
    hlr = HLRBRep_Algo()
    hlr.Add(shape.wrapped)
    projector = HLRAlgo_Projector(gp_Ax2(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(*toward), gp_Dir(*right)))
    hlr.Projector(projector)
    hlr.Update()
    hlr.Hide()
    classified = HLRBRep_HLRToShape(hlr)
    groups = [
        ("visible", [classified.VCompound(), classified.Rg1LineVCompound(), classified.RgNLineVCompound(), classified.OutLineVCompound()]),
        ("hidden", [classified.HCompound(), classified.Rg1LineHCompound(), classified.RgNLineHCompound(), classified.OutLineHCompound()]),
    ]
    bounds = shape.BoundingBox()
    corners = [(x, y, z) for x in (bounds.xmin, bounds.xmax) for y in (bounds.ymin, bounds.ymax) for z in (bounds.zmin, bounds.zmax)]
    depths = [_dot(point, toward) for point in corners]
    items: list[dict[str, Any]] = []
    for visibility, compounds in groups:
        for compound in compounds:
            if compound.IsNull():
                continue
            projected_shape = cq.Shape.cast(compound)
            for edge in projected_shape.Edges():
                adaptor = BRepAdaptor_Curve(edge.wrapped)
                curve_kind = adaptor.GetType()
                if curve_kind in {GeomAbs_BSplineCurve, GeomAbs_BezierCurve}:
                    if _projected_edge_is_line(edge, right, up, HLR_CURVE_MAX_DEVIATION_MM, already_projected=True):
                        geometry = _projected_edge(edge, right, up, toward, already_projected=True)
                        geometries = [geometry] if geometry is not None else []
                        derived_approximation = geometry is not None
                        approximation_tolerance = HLR_CURVE_MAX_DEVIATION_MM if geometry is not None else None
                    else:
                        approximation_tolerance = HLR_CURVE_MAX_DEVIATION_MM
                        geometries = _tessellate_projected_curve(
                            edge, right, up, approximation_tolerance, already_projected=True
                        )
                        derived_approximation = True
                else:
                    # HLR edges, including analytic circles, are already in
                    # the projector's local x/y plane. Keep exact curves exact.
                    geometry = _projected_edge(edge, right, up, toward, already_projected=True)
                    if geometry is None:
                        continue
                    geometries = [geometry]
                    derived_approximation = False
                    approximation_tolerance = None
                for geometry in geometries:
                    descriptor = json.dumps({"geometry": geometry, "visibility": visibility}, sort_keys=True, separators=(",", ":"))
                    items.append({
                        "geometry": geometry, "visibility": visibility,
                        "derived_approximation": derived_approximation,
                        "approximation_tolerance_mm": approximation_tolerance,
                        "depth_min": _rounded(min(depths)), "depth_max": _rounded(max(depths)), "key": descriptor,
                    })
    items.sort(key=lambda item: item["key"])
    seen: defaultdict[str, int] = defaultdict(int)
    output = []
    for item in items:
        digest = hashlib.sha256(item["key"].encode("utf-8")).hexdigest()[:20]
        ordinal = seen[digest]
        seen[digest] += 1
        output.append({
            "id": f"{name}:{digest}:{ordinal:04d}",
            "geometry": item["geometry"],
            "depth_min": item["depth_min"],
            "depth_max": item["depth_max"],
            "visibility": item["visibility"],
            "derived_approximation": item["derived_approximation"],
            "approximation_tolerance_mm": item["approximation_tolerance_mm"],
        })
    return output


def _projected_edge(
    edge: cq.Edge,
    right: tuple[float, ...],
    up: tuple[float, ...],
    toward: tuple[float, ...],
    *,
    already_projected: bool = False,
) -> dict[str, Any] | None:
    adaptor = BRepAdaptor_Curve(edge.wrapped)
    kind = adaptor.GetType()
    if kind == GeomAbs_Line:
        start = _project_point(_point(edge.startPoint()), right, up, already_projected=already_projected)
        end = _project_point(_point(edge.endPoint()), right, up, already_projected=already_projected)
        if _distance2(start, end) <= TOLERANCE:
            # A 3D edge parallel to the viewing direction has no 2D extent.
            # HLR may retain it as a zero-length segment; omit that projection.
            return None
        return {"kind": "segment", "start": {"x": start[0], "y": start[1]}, "end": {"x": end[0], "y": end[1]}}
    if kind == GeomAbs_Circle:
        circle = adaptor.Circle()
        center3 = _point(circle.Location())
        center = _project_point(center3, right, up, already_projected=already_projected)
        radius = float(circle.Radius())
        sweep = float(edge.Length()) / radius
        if sweep >= math.tau - 1e-6:
            return {"kind": "circle", "center": {"x": _rounded(center[0]), "y": _rounded(center[1])}, "radius": _rounded(radius)}
        start = _project_point(_point(edge.startPoint()), right, up, already_projected=already_projected)
        end = _project_point(_point(edge.endPoint()), right, up, already_projected=already_projected)
        a0 = math.atan2(start[1] - center[1], start[0] - center[0])
        a1 = math.atan2(end[1] - center[1], end[0] - center[0])
        forward = (a1 - a0) % math.tau
        if abs(forward - sweep) <= abs((math.tau - forward) - sweep):
            start_angle, end_angle = a0, a1
        else:
            start_angle, end_angle = a1, a0
        return {
            "kind": "arc", "center": {"x": _rounded(center[0]), "y": _rounded(center[1])},
            "radius": _rounded(radius), "start_angle": start_angle, "end_angle": end_angle,
        }
    if kind == GeomAbs_BezierCurve:
        if _projected_edge_is_line(edge, right, up, HLR_CURVE_MAX_DEVIATION_MM, already_projected=already_projected):
            start = _project_point(_point(edge.startPoint()), right, up, already_projected=already_projected)
            end = _project_point(_point(edge.endPoint()), right, up, already_projected=already_projected)
            if _distance2(start, end) <= TOLERANCE:
                return None
            return {"kind": "segment", "start": {"x": start[0], "y": start[1]}, "end": {"x": end[0], "y": end[1]}}
        raise RebuildError("BODY_INVALID", "HLR Bezier curve requires bounded tessellation before projection")
    if kind == GeomAbs_BSplineCurve:
        curve = adaptor.BSpline()
        if curve.Degree() != 1:
            if _projected_edge_is_line(edge, right, up, HLR_CURVE_MAX_DEVIATION_MM, already_projected=already_projected):
                start = _project_point(_point(edge.startPoint()), right, up, already_projected=already_projected)
                end = _project_point(_point(edge.endPoint()), right, up, already_projected=already_projected)
                return {"kind": "segment", "start": {"x": start[0], "y": start[1]}, "end": {"x": end[0], "y": end[1]}}
            raise RebuildError("BODY_INVALID", "HLR emitted a non-linear spline that cannot be restored within the declared projection tolerance")
        poles = [
            _project_point(_point(curve.Pole(index)), right, up, already_projected=already_projected)
            for index in range(1, curve.NbPoles() + 1)
        ]
        segment = _line_from_poles(poles)
        if segment is not None:
            start, end = segment
            if _distance2(start, end) <= TOLERANCE:
                return None
            return {"kind": "segment", "start": {"x": start[0], "y": start[1]}, "end": {"x": end[0], "y": end[1]}}
        circular = _circle_from_poles(poles)
        if circular is not None:
            center, radius, start_angle, end_angle = circular
            if end_angle is None:
                return {"kind": "circle", "center": {"x": center[0], "y": center[1]}, "radius": radius}
            return {
                "kind": "arc", "center": {"x": center[0], "y": center[1]},
                "radius": radius, "start_angle": start_angle, "end_angle": end_angle,
            }
        raise RebuildError("BODY_INVALID", "HLR spline cannot be restored to an exact line or circle")
    raise RebuildError("BODY_INVALID", "HLR emitted an unsupported projected curve")


def _projected_circle_geometries(
    edge: cq.Edge,
    right: tuple[float, ...],
    up: tuple[float, ...],
    view_alignment: float,
) -> tuple[list[dict[str, Any]], float]:
    line_tolerance = 1.0e-5
    if view_alignment <= 1.0e-9 and _projected_edge_is_line(edge, right, up, line_tolerance):
        start = _project_point(_point(edge.startPoint()), right, up)
        end = _project_point(_point(edge.endPoint()), right, up)
        if _distance2(start, end) <= TOLERANCE:
            return [], line_tolerance
        return ([{"kind": "segment", "start": {"x": start[0], "y": start[1]}, "end": {"x": end[0], "y": end[1]}}], line_tolerance)
    tolerance = 1.0e-4
    return _tessellate_projected_curve(edge, right, up, tolerance), tolerance


def _projected_extremum_fractions(
    edge: cq.Edge,
    right: tuple[float, ...],
    up: tuple[float, ...],
    *,
    already_projected: bool,
) -> list[float]:
    """Parameters where a projected curve reaches a local axis extreme.

    A chord inside the deviation tolerance can still miss the extreme by more
    than the 1e-5 mm overall-dimension contract. The extreme itself has to be
    a polyline vertex. The chord tolerance is unchanged.
    """
    sample_count = 64

    def point_at(fraction: float) -> tuple[float, float]:
        return _project_point(
            _point(edge.positionAt(fraction)), right, up, already_projected=already_projected
        )

    samples = [point_at(index / sample_count) for index in range(sample_count + 1)]

    def coordinate(fraction: float, axis: int) -> float:
        return point_at(min(1.0, max(0.0, fraction)))[axis]

    def refine(left: float, right_fraction: float, axis: int, find_max: bool) -> float:
        golden = (math.sqrt(5.0) - 1.0) / 2.0
        low, high = left, right_fraction

        def score(fraction: float) -> float:
            value = coordinate(fraction, axis)
            return -value if find_max else value

        left_probe = high - golden * (high - low)
        right_probe = low + golden * (high - low)
        left_score, right_score = score(left_probe), score(right_probe)
        for _ in range(60):
            if high - low <= 1.0e-12:
                break
            if left_score < right_score:
                high, right_probe, right_score = right_probe, left_probe, left_score
                left_probe = high - golden * (high - low)
                left_score = score(left_probe)
            else:
                low, left_probe, left_score = left_probe, right_probe, right_score
                right_probe = low + golden * (high - low)
                right_score = score(right_probe)
        return (low + high) / 2.0

    fractions: list[float] = []
    for axis in (0, 1):
        for index in range(1, sample_count):
            before = samples[index][axis] - samples[index - 1][axis]
            after = samples[index + 1][axis] - samples[index][axis]
            if (before > 0.0 and after > 0.0) or (before < 0.0 and after < 0.0):
                continue
            if before == 0.0 and after == 0.0:
                fractions.append(index / sample_count)
                continue
            fractions.append(refine((index - 1) / sample_count, (index + 1) / sample_count, axis, before > 0.0))
    return fractions


def _tessellate_projected_curve(
    edge: cq.Edge,
    right: tuple[float, ...],
    up: tuple[float, ...],
    tolerance: float,
    *,
    already_projected: bool = False,
) -> list[dict[str, Any]]:
    max_depth = 16

    def point_at(fraction: float) -> tuple[float, float]:
        return _project_point(
            _point(edge.positionAt(fraction)), right, up, already_projected=already_projected
        )

    fractions = [index / 8.0 for index in range(9)]
    fractions.extend(_projected_extremum_fractions(edge, right, up, already_projected=already_projected))
    fractions.sort()
    deduped = [fractions[0]]
    for fraction in fractions[1:]:
        if fraction - deduped[-1] > 1.0e-12:
            deduped.append(min(1.0, fraction))
    points = [point_at(fraction) for fraction in deduped]
    output = [points[0]]

    def refine(t0: float, p0: tuple[float, float], t1: float, p1: tuple[float, float], depth: int) -> None:
        span = t1 - t0
        probes = [(t0 + span * fraction, point_at(t0 + span * fraction)) for fraction in (0.25, 0.5, 0.75)]
        error = max(_point_segment_distance(point, p0, p1) for _, point in probes)
        if error <= tolerance:
            output.append(p1)
            return
        if depth >= max_depth:
            raise RebuildError("BODY_INVALID", "projected circular curve exceeded the bounded discretization depth")
        midpoint_t, midpoint = probes[1]
        refine(t0, p0, midpoint_t, midpoint, depth + 1)
        refine(midpoint_t, midpoint, t1, p1, depth + 1)

    for index in range(len(deduped) - 1):
        refine(deduped[index], points[index], deduped[index + 1], points[index + 1], 0)
    geometries = []
    for start, end in zip(output, output[1:]):
        if _distance2(start, end) > TOLERANCE:
            geometries.append({
                "kind": "segment",
                "start": {"x": start[0], "y": start[1]},
                "end": {"x": end[0], "y": end[1]},
            })
    return geometries


def _point_segment_distance(
    point: tuple[float, float],
    start: tuple[float, float],
    end: tuple[float, float],
) -> float:
    dx, dy = end[0] - start[0], end[1] - start[1]
    length_squared = dx * dx + dy * dy
    if length_squared <= TOLERANCE * TOLERANCE:
        return _distance2(point, start)
    fraction = max(0.0, min(1.0, ((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / length_squared))
    projection = (start[0] + fraction * dx, start[1] + fraction * dy)
    return _distance2(point, projection)


def _projected_edge_is_line(
    edge: cq.Edge,
    right: tuple[float, ...],
    up: tuple[float, ...],
    tolerance: float,
    *,
    already_projected: bool = False,
) -> bool:
    points = [
        _project_point(
            _point(edge.positionAt(index / 64.0)), right, up, already_projected=already_projected
        )
        for index in range(65)
    ]
    start, end = points[0], points[-1]
    dx, dy = end[0] - start[0], end[1] - start[1]
    length = math.hypot(dx, dy)
    if length <= tolerance:
        return False
    ux, uy = dx / length, dy / length
    previous = -tolerance
    for point in points:
        offset_x, offset_y = point[0] - start[0], point[1] - start[1]
        along = offset_x * ux + offset_y * uy
        cross = abs(dx * offset_y - dy * offset_x) / length
        if cross > tolerance or along < -tolerance or along > length + tolerance or along + tolerance < previous:
            return False
        previous = along
    return True


def _line_from_poles(
    poles: list[tuple[float, float]],
) -> tuple[tuple[float, float], tuple[float, float]] | None:
    if len(poles) < 2:
        return None
    start, end = poles[0], poles[-1]
    dx, dy = end[0] - start[0], end[1] - start[1]
    length = math.hypot(dx, dy)
    if length <= TOLERANCE:
        return (start, end)
    tolerance = 1.0e-10 * max(1.0, length)
    positions = []
    for point in poles:
        cross = dx * (point[1] - start[1]) - dy * (point[0] - start[0])
        if abs(cross) > tolerance * length:
            return None
        positions.append(((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / (length * length))
    forward = all(left <= right + 1.0e-10 for left, right in zip(positions, positions[1:]))
    reverse = all(left >= right - 1.0e-10 for left, right in zip(positions, positions[1:]))
    if not (forward or reverse) or min(positions) < -1.0e-10 or max(positions) > 1.0 + 1.0e-10:
        return None
    return start, end


def _circle_from_poles(
    poles: list[tuple[float, float]],
) -> tuple[tuple[float, float], float, float, float | None] | None:
    unique = []
    for point in poles:
        if not any(_distance2(point, other) <= 1.0e-12 for other in unique):
            unique.append(point)
    if len(unique) < 3:
        return None
    candidates = sorted({0, len(unique) // 4, len(unique) // 2, (3 * len(unique)) // 4, len(unique) - 1})
    triple = max(
        combinations(candidates, 3),
        key=lambda indexes: abs(
            unique[indexes[0]][0] * (unique[indexes[1]][1] - unique[indexes[2]][1])
            + unique[indexes[1]][0] * (unique[indexes[2]][1] - unique[indexes[0]][1])
            + unique[indexes[2]][0] * (unique[indexes[0]][1] - unique[indexes[1]][1])
        ),
    )
    first, middle, last = (unique[index] for index in triple)
    determinant = 2.0 * (
        first[0] * (middle[1] - last[1])
        + middle[0] * (last[1] - first[1])
        + last[0] * (first[1] - middle[1])
    )
    if abs(determinant) <= 1.0e-12:
        return None
    first_sq = first[0] ** 2 + first[1] ** 2
    middle_sq = middle[0] ** 2 + middle[1] ** 2
    last_sq = last[0] ** 2 + last[1] ** 2
    center = (
        (first_sq * (middle[1] - last[1]) + middle_sq * (last[1] - first[1]) + last_sq * (first[1] - middle[1])) / determinant,
        (first_sq * (last[0] - middle[0]) + middle_sq * (first[0] - last[0]) + last_sq * (middle[0] - first[0])) / determinant,
    )
    radius = _distance2(center, first)
    if radius <= TOLERANCE:
        return None
    radial_tolerance = max(2.0e-7, radius * 1.0e-9)
    if any(abs(_distance2(center, point) - radius) > radial_tolerance for point in poles):
        return None
    angles = [math.atan2(point[1] - center[1], point[0] - center[0]) for point in poles]
    steps = [((end - start + math.pi) % math.tau) - math.pi for start, end in zip(angles, angles[1:])]
    nonzero = [step for step in steps if abs(step) > 1.0e-10]
    if not nonzero or any(step * nonzero[0] < 0.0 for step in nonzero):
        return None
    sweep = sum(nonzero)
    if abs(sweep) >= math.tau - 1.0e-6:
        return (_rounded(center[0]), _rounded(center[1])), _rounded(radius), 0.0, None
    if abs(sweep) <= 1.0e-10 or abs(sweep) > math.tau:
        return None
    start_angle, end_angle = (angles[0], angles[-1]) if sweep > 0 else (angles[-1], angles[0])
    return (
        (_rounded(center[0]), _rounded(center[1])),
        _rounded(radius),
        start_angle,
        end_angle,
    )


def _body_frame(shape: cq.Shape, first: dict[str, Any]) -> dict[str, Any]:
    if first["kind"] == "revolve":
        origin = first["axis_origin"]
        axis = first["direction"]
        reference = (0.0, 0.0, 1.0) if abs(axis[2]) < 0.9 else (1.0, 0.0, 0.0)
        u_axis = _unit_vector(_cross(reference, axis), "body frame axis", first["feature_id"])
        v_axis = _cross(axis, u_axis)
    elif first["kind"] == "sweep":
        origin = first["path_plane"]["origin"]
        axis = first["path_plane"]["normal"]
        u_axis = first["path_plane"]["u_axis"]
        v_axis = first["path_plane"]["v_axis"]
    elif first["kind"] == "loft":
        first_section, last_section = first["sections"][0], first["sections"][-1]
        origin = first_section["plane"]["origin"]
        delta = tuple(last_section["plane"]["origin"][i] - origin[i] for i in range(3))
        length = math.sqrt(sum(value*value for value in delta))
        if not math.isfinite(length) or length <= 1e-8:
            raise RebuildError("LOFT_SECTION_ORDER_INVALID", "Loft sections have no measurable axis separation", first["feature_id"])
        axis = tuple(value/length for value in delta)
        u_axis = first_section["plane"]["u_axis"]
        v_axis = first_section["plane"]["v_axis"]
    else:
        origin = first["plane"]["origin"]
        axis = first["direction"]
        u_axis = first["plane"]["u_axis"]
        v_axis = _cross(axis, u_axis)
    bounds = shape.BoundingBox()
    corners = [(x, y, z) for x in (bounds.xmin, bounds.xmax) for y in (bounds.ymin, bounds.ymax) for z in (bounds.zmin, bounds.zmax)]
    projections = [_dot(_sub(point, origin), axis) for point in corners]
    return {
        "origin": list(origin), "u_axis": list(u_axis), "v_axis": list(v_axis), "axis": list(axis),
        "axis_min": _rounded(min(projections)), "axis_max": _rounded(max(projections)),
    }


def _world_point(plane: dict[str, tuple[float, float, float]], point: tuple[float, float]) -> tuple[float, float, float]:
    return tuple(plane["origin"][i] + plane["u_axis"][i] * point[0] + plane["v_axis"][i] * point[1] for i in range(3))


def _point(value: Any) -> tuple[float, float, float]:
    # CadQuery returns its own Vector from Edge.startPoint()/endPoint(); those
    # expose lowercase coordinates and intentionally do not support indexing.
    if all(hasattr(value, axis) for axis in ("x", "y", "z")):
        return (_rounded(value.x), _rounded(value.y), _rounded(value.z))
    if hasattr(value, "X"):
        return (_rounded(value.X()), _rounded(value.Y()), _rounded(value.Z()))
    return (_rounded(value[0]), _rounded(value[1]), _rounded(value[2]))


def _project_point(
    point: tuple[float, float, float],
    right: tuple[float, ...],
    up: tuple[float, ...],
    *,
    already_projected: bool = False,
) -> tuple[float, float]:
    if already_projected:
        # HLRBRep_HLRToShape returns edges in its projector's local 2D plane;
        # local x/y already follow right/up. A second world-space transform
        # collapses dimensions in the Top and Left views.
        return (_rounded(point[0]), _rounded(point[1]))
    return (_rounded(_dot(point, right)), _rounded(_dot(point, up)))


def _vec2(value: Any, feature_id: str) -> tuple[float, float]:
    if not isinstance(value, (list, tuple)) or len(value) != 2 or not all(isinstance(v, (float, int)) and not isinstance(v, bool) and math.isfinite(v) for v in value):
        raise RebuildError("INVALID_SKETCH", "profile point must have two finite coordinates", feature_id)
    return (float(value[0]), float(value[1]))


def _vec3(value: Any, label: str, feature_id: str) -> tuple[float, float, float]:
    if not isinstance(value, (list, tuple)) or len(value) != 3 or not all(isinstance(v, (float, int)) and not isinstance(v, bool) and math.isfinite(v) for v in value):
        raise RebuildError("INVALID_DATUM", f"{label} must have three finite coordinates", feature_id)
    return (float(value[0]), float(value[1]), float(value[2]))


def _unit_vector(value: Any, label: str, feature_id: str) -> tuple[float, float, float]:
    vector = _vec3(value, label, feature_id)
    magnitude = math.sqrt(_dot(vector, vector))
    if magnitude <= TOLERANCE:
        raise RebuildError("INVALID_DATUM", f"{label} must be nonzero", feature_id)
    return tuple(component / magnitude for component in vector)


def _curve_start(curve: dict[str, Any]) -> tuple[float, float]:
    return curve.get("start", curve.get("center"))


def _curve_end(curve: dict[str, Any]) -> tuple[float, float]:
    return curve.get("end", curve.get("center"))


def _dot(left: tuple[float, ...], right: tuple[float, ...]) -> float:
    return sum(a * b for a, b in zip(left, right))


def _cross(left: tuple[float, ...], right: tuple[float, ...]) -> tuple[float, float, float]:
    return (
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    )


def _sub(left: tuple[float, ...], right: tuple[float, ...]) -> tuple[float, float, float]:
    return tuple(a - b for a, b in zip(left, right))


def _distance2(left: tuple[float, float], right: tuple[float, float]) -> float:
    return math.hypot(left[0] - right[0], left[1] - right[1])


def _distance3(left: tuple[float, ...], right: tuple[float, ...]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(left, right)))


def _volume_tolerance(volume: float) -> float:
    return max(1e-7, abs(volume) * 1e-12)


def _rounded(value: float) -> float:
    return round(float(value), 9)


def _safe_error(error: Exception) -> str:
    return str(error).replace("\n", " ").strip()[:512] or error.__class__.__name__


def main() -> int:
    try:
        payload = json.load(sys.stdin)
        response = section_parametric_model(payload) if isinstance(payload, Mapping) and payload.get("action") == "section" else rebuild_parametric_model(payload)
    except Exception:
        response = {"ok": False, "error": {"stage": "parametric_rebuild", "code": "REBUILD_DEPENDENCY_ERROR", "message": "request must be a valid JSON object"}}
    sys.stdout.write(json.dumps(response, sort_keys=True, separators=(",", ":")))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
