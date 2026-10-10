"""Validated, ephemeral STEP delivery for SolidBuildSpec v1.

STEP is treated as an exchange artifact only.  This module builds the
CadQuery solid once, exports that exact shape, imports the file through the
CadQuery/OCP importer, and compares measurable and material witnesses.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Callable, Mapping

import cadquery as cq

from .solid_backend import _apply_cut, _build_body, _build_profile_tool
from .solid_build_spec import (
    CircleBodySpec,
    ClosedProfileBodySpec,
    RectangleBodySpec,
    RoundedRectangleBodySpec,
    SlotCutSpec,
    SolidBuildSpecError,
    SolidBuildSpecV1,
    parse_solid_build_spec,
)


SCHEMA_VERSION = 1
STEP_EQ_TOLERANCE_MM = 1e-6
STEP_VOLUME_ABS_TOLERANCE_MM3 = 1e-5
STEP_VOLUME_REL_TOLERANCE = 1e-9
STEP_SURFACE_AREA_ABS_TOLERANCE_MM2 = 1e-5
STEP_SURFACE_AREA_REL_TOLERANCE = 1e-9
STEP_CENTER_OF_MASS_TOLERANCE_MM = 1e-6


class StepDeliveryError(ValueError):
    """Stable, structured error at the atomic STEP delivery boundary."""

    def __init__(
        self,
        code: str,
        message: str,
        *,
        stage: str = "step_delivery",
        path: str | None = None,
        operation_id: str | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.stage = stage
        self.path = path
        self.operation_id = operation_id

    def as_error(self) -> dict[str, Any]:
        error: dict[str, Any] = {
            "stage": self.stage,
            "code": self.code,
            "message": self.message,
        }
        if self.path is not None:
            error["path"] = self.path
        if self.operation_id is not None:
            error["operation_id"] = self.operation_id
        return error


def build_and_export_step(
    spec_json: Mapping[str, Any],
    *,
    _temp_factory: Callable[..., Any] = tempfile.TemporaryDirectory,
    _exporter: Callable[[cq.Shape, str], Any] | None = None,
    _importer: Callable[[str], Any] | None = None,
) -> dict[str, Any]:
    """Build, export, reimport, and validate one ephemeral STEP artifact."""

    total_started = time.perf_counter()
    try:
        source_shape, operation_results, probes, body_source, parametric = _prepare_step_source(spec_json)
        source_witness = _solid_witness(source_shape)
        source_material = _probe_material(source_shape, probes)
        manufacturing_build_ms = (time.perf_counter() - total_started) * 1000.0
        with _temp_factory(prefix="aicad-step-") as temp_dir:
            return _deliver_validated_step(
                source_shape, operation_results, source_witness, source_material, probes,
                _confined_temp_dir(temp_dir) / "solid.step", _exporter or export_step,
                _importer or reimport_step, body_source, parametric,
                total_started=total_started, manufacturing_build_ms=manufacturing_build_ms,
            )
    except SolidBuildSpecError as error:
        return {"ok": False, "error": _as_step_error(error, "CAD-STEP-001")}
    except StepDeliveryError as error:
        return {"ok": False, "error": error.as_error()}
    except Exception as error:  # Never leak a kernel traceback through stdout.
        return {
            "ok": False,
            "error": {
                "stage": "step_delivery",
                "code": "CAD-STEP-005",
                "message": _safe_exception_message(error),
            },
        }


def build_and_export_step_to_path(spec_json: Mapping[str, Any], artifact_path: str) -> dict[str, Any]:
    """Worker entry point that leaves the validated artifact for Rust to read."""

    total_started = time.perf_counter()
    try:
        source_shape, operation_results, probes, body_source, parametric = _prepare_step_source(spec_json)
        source_witness = _solid_witness(source_shape)
        source_material = _probe_material(source_shape, probes)
        manufacturing_build_ms = (time.perf_counter() - total_started) * 1000.0
        path = _controlled_worker_path(artifact_path)
        return _deliver_validated_step(
            source_shape, operation_results, source_witness, source_material, probes,
            path, export_step, reimport_step, body_source, parametric,
            total_started=total_started, manufacturing_build_ms=manufacturing_build_ms,
        )
    except SolidBuildSpecError as error:
        return {"ok": False, "error": _as_step_error(error, "CAD-STEP-001")}
    except StepDeliveryError as error:
        return {"ok": False, "error": error.as_error()}
    except Exception as error:
        return {
            "ok": False,
            "error": {
                "stage": "step_delivery",
                "code": "CAD-STEP-005",
                "message": _safe_exception_message(error),
            },
        }


def _prepare_step_source(
    value: Mapping[str, Any],
) -> tuple[cq.Shape, list[dict[str, str]], list[dict[str, Any]], dict[str, str], bool]:
    if isinstance(value, Mapping) and "manufacturing_kind" in value:
        if set(value) != {"manufacturing_kind", "spec"} or value.get("manufacturing_kind") != "project_v2":
            raise StepDeliveryError(
                "MANUFACTURING_LOWERING_FAILED",
                "formal manufacturing dispatch is invalid or unsupported",
                stage="manufacturing_lowering",
            )
        from .parametric_rebuild import (
            RebuildError,
            build_formal_manufacturing_shape,
            parse_parametric_request,
        )

        try:
            request = parse_parametric_request(value["spec"])
            shape = build_formal_manufacturing_shape(value["spec"])
        except RebuildError as error:
            raise StepDeliveryError(
                error.code, error.message, stage="manufacturing_build",
                operation_id=error.feature_id,
            ) from error
        try:
            _validate_source_solid(shape)
        except StepDeliveryError as error:
            raise StepDeliveryError(
                error.code, error.message, stage="manufacturing_build",
                operation_id=error.operation_id,
            ) from error
        operations = request["operations"]
        operation_results = [
            {
                "operation_id": operation["feature_id"],
                "source_id": operation["source_feature_id"],
                "status": "applied",
            }
            for operation in operations
        ]
        first_add = next(
            (
                operation for operation in operations
                if operation["kind"] in {"extrude", "revolve", "sweep", "loft"}
                and operation["operation"] == "add"
            ),
            None,
        )
        if first_add is None:
            raise StepDeliveryError(
                "NO_BODY", "parametric manufacturing history has no ADD body",
                stage="manufacturing_lowering",
            )
        source_id = first_add["source_feature_id"]
        body_source = {"feature": source_id, "entity": source_id}
        probes = _parametric_material_probe_plan(request, shape)
        return shape, operation_results, probes, body_source, True

    spec = parse_solid_build_spec(value)
    shape, operation_results = _build_validated_source(spec)
    probes = _material_probe_plan(spec, shape)
    return shape, operation_results, probes, dict(spec.body_source), False


def _deliver_validated_step(
    source_shape: cq.Shape,
    operation_results: list[dict[str, str]],
    source_witness: dict[str, Any],
    source_material: list[bool],
    probes: list[dict[str, Any]],
    artifact_path: Path,
    exporter: Callable[[cq.Shape, str], Any],
    importer: Callable[[str], Any],
    body_source: dict[str, str],
    parametric: bool,
    *,
    total_started: float,
    manufacturing_build_ms: float,
) -> dict[str, Any]:
    serialization_started = time.perf_counter()
    try:
        exporter(source_shape, str(artifact_path))
    except Exception as error:
        raise StepDeliveryError("CAD-STEP-002", _safe_exception_message(error)) from error
    _validate_artifact(artifact_path)
    artifact = _artifact_metadata(artifact_path)
    step_serialization_ms = (time.perf_counter() - serialization_started) * 1000.0
    reimport_started = time.perf_counter()
    try:
        imported_value = importer(str(artifact_path))
        imported_shape = _single_solid(imported_value)
    except StepDeliveryError:
        raise
    except Exception as error:
        raise StepDeliveryError("CAD-STEP-003", _safe_exception_message(error)) from error
    if imported_shape is source_shape:
        raise StepDeliveryError("CAD-STEP-003", "STEP importer returned the source object")
    imported_witness = _solid_witness(imported_shape)
    imported_material = _probe_material(imported_shape, probes)
    step_reimport_ms = (time.perf_counter() - reimport_started) * 1000.0
    equivalence_started = time.perf_counter()
    feature_geometry_match = (
        _analytic_geometry_matches(source_shape, imported_shape) if parametric else True
    )
    equivalence = _equivalence(
        source_witness, imported_witness, source_material, imported_material,
        feature_geometry_match=feature_geometry_match,
        require_topology_match=not parametric,
    )
    equivalence_validation_ms = (time.perf_counter() - equivalence_started) * 1000.0
    if not equivalence["all_match"]:
        failed = ",".join(name for name, matched in equivalence.items() if name != "all_match" and not matched)
        measurement_deltas = (
            f"volume_delta_mm3={abs(source_witness['volume_mm3'] - imported_witness['volume_mm3']):.9g}, "
            f"surface_area_delta_mm2={abs(source_witness['surface_area_mm2'] - imported_witness['surface_area_mm2']):.9g}"
        )
        raise StepDeliveryError(
            "CAD-STEP-004",
            f"source and reimported STEP witnesses differ: {failed or 'unknown witness'}; {measurement_deltas}",
            stage="step_equivalence",
        )
    return {
        "ok": True,
        "schema_version": SCHEMA_VERSION,
        "format": "step",
        "unit": "mm",
        "coordinate_frame": "PartLocal",
        "artifact": artifact,
        "source_solid": source_witness,
        "reimported_solid": imported_witness,
        "equivalence": equivalence,
        "material_witness": {
            "probe_count": len(probes),
            "all_match": True,
            "probes": [
                {
                    "label": probe["label"],
                    "point": probe["point"],
                    "source_material": source_value,
                    "reimported_material": imported_value,
                }
                for probe, source_value, imported_value in zip(probes, source_material, imported_material)
            ],
        },
        "operation_results": operation_results,
        "body_source": body_source,
        "timings_ms": {
            "manufacturing_build_ms": _rounded_timing(manufacturing_build_ms),
            "step_serialization_ms": _rounded_timing(step_serialization_ms),
            "step_reimport_ms": _rounded_timing(step_reimport_ms),
            "equivalence_validation_ms": _rounded_timing(equivalence_validation_ms),
            "total_ms": _rounded_timing((time.perf_counter() - total_started) * 1000.0),
        },
    }

def export_step(shape: cq.Shape, path: str) -> None:
    """Export the supplied source shape to the controlled `.step` path."""

    if Path(path).suffix != ".step":
        raise StepDeliveryError("CAD-STEP-002", "STEP artifact extension must be .step")
    cq.exporters.export(shape, path, cq.exporters.ExportTypes.STEP)


def reimport_step(path: str) -> Any:
    """Import a STEP file through CadQuery's real OCP-backed importer."""

    if Path(path).suffix != ".step":
        raise StepDeliveryError("CAD-STEP-003", "STEP importer requires a .step artifact")
    return cq.importers.importStep(path)


def _build_validated_source(spec: SolidBuildSpecV1) -> tuple[cq.Shape, list[dict[str, str]]]:
    """Build one source solid; this is the only build in the delivery path."""

    shape = _build_body(spec)
    operation_results: list[dict[str, str]] = []
    for operation in spec.operations:
        try:
            before_volume = float(shape.Volume())
            candidate = _apply_cut(shape, operation)
            after_volume = float(candidate.Volume())
        except SolidBuildSpecError:
            raise
        except Exception as error:
            raise StepDeliveryError(
                "CAD-STEP-001",
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
    _validate_source_solid(shape)
    return shape, operation_results


def _validate_source_solid(shape: cq.Shape) -> None:
    if shape is None or not shape.isValid() or len(shape.Solids()) != 1:
        raise StepDeliveryError("CAD-STEP-001", "source CadQuery result is not one valid solid")
    try:
        _solid_witness(shape)
    except StepDeliveryError as error:
        raise StepDeliveryError("CAD-STEP-001", error.message) from error


def _solid_witness(shape: cq.Shape) -> dict[str, Any]:
    try:
        valid = bool(shape.isValid())
        solid_count = len(shape.Solids())
        bounds = shape.BoundingBox()
        values = (bounds.xmin, bounds.xmax, bounds.ymin, bounds.ymax, bounds.zmin, bounds.zmax)
        volume = float(shape.Volume())
        surface_area = float(shape.Area())
        center_of_mass = cq.Shape.centerOfMass(shape)
        center_values = (center_of_mass.x, center_of_mass.y, center_of_mass.z)
        edge_count = len(shape.Edges())
        face_count = len(shape.Faces())
        vertex_count = len(shape.Vertices())
    except Exception as error:
        raise StepDeliveryError("CAD-STEP-003", _safe_exception_message(error)) from error
    if (
        not valid
        or solid_count != 1
        or not all(_finite(value) for value in (*values, volume, surface_area, *center_values))
        or volume <= 0.0
        or surface_area <= 0.0
        or min(edge_count, face_count, vertex_count) <= 0
    ):
        finite_bounds = all(_finite(value) for value in values)
        finite_measurements = all(_finite(value) for value in (volume, surface_area, *center_values))
        raise StepDeliveryError(
            "CAD-STEP-003",
            f"solid witness is invalid (is_valid={valid}, solids={solid_count}, "
            f"finite_bounds={finite_bounds}, finite_measurements={finite_measurements}, "
            f"volume_positive={volume > 0.0}, area_positive={surface_area > 0.0}, "
            f"topology_counts={(edge_count, face_count, vertex_count)})",
        )
    return {
        "valid": True,
        "solid_count": solid_count,
        "bbox": {
            "x_min": _rounded(bounds.xmin),
            "x_max": _rounded(bounds.xmax),
            "y_min": _rounded(bounds.ymin),
            "y_max": _rounded(bounds.ymax),
            "z_min": _rounded(bounds.zmin),
            "z_max": _rounded(bounds.zmax),
        },
        "volume_mm3": _rounded(volume),
        "surface_area_mm2": _rounded(surface_area),
        "center_of_mass": [_rounded(value) for value in center_values],
        "edge_count": edge_count,
        "face_count": face_count,
        "vertex_count": vertex_count,
    }


def _single_solid(value: Any) -> cq.Shape:
    """Normalize CadQuery import results without exposing them in the API."""

    candidates: list[Any] = []
    if isinstance(value, cq.Shape):
        candidates = [value]
    else:
        try:
            candidates = list(value.solids().vals())
        except Exception:
            try:
                candidates = list(value.vals())
            except Exception as error:
                raise StepDeliveryError("CAD-STEP-003", "STEP importer returned no CadQuery shape") from error
    if len(candidates) != 1 or not isinstance(candidates[0], cq.Shape):
        raise StepDeliveryError("CAD-STEP-003", "STEP importer did not return exactly one solid")
    shape = candidates[0]
    if len(shape.Solids()) != 1:
        raise StepDeliveryError("CAD-STEP-003", "STEP importer returned a non-single-solid result")
    return shape


def _material_probe_plan(spec: SolidBuildSpecV1, source_shape: cq.Shape | None = None) -> list[dict[str, Any]]:
    body = spec.body
    thickness = body.z.maximum - body.z.minimum
    points: list[dict[str, Any]] = []
    if isinstance(body, RectangleBodySpec):
        candidates = (
            (body.origin.x + body.width * 0.25, body.origin.y + body.height * 0.25),
            (body.origin.x + body.width * 0.75, body.origin.y + body.height * 0.25),
            (body.origin.x + body.width * 0.25, body.origin.y + body.height * 0.75),
            (body.origin.x + body.width * 0.75, body.origin.y + body.height * 0.75),
        )
    elif isinstance(body, CircleBodySpec):
        radius = body.radius * 0.5
        candidates = (
            (body.center.x, body.center.y),
            (body.center.x + radius, body.center.y),
            (body.center.x - radius, body.center.y),
            (body.center.x, body.center.y + radius),
        )
    elif isinstance(body, RoundedRectangleBodySpec):
        inset_x = body.width / 2.0 - body.corner_radius / 2.0
        inset_y = body.height / 2.0 - body.corner_radius / 2.0
        candidates = (
            (body.center.x, body.center.y),
            (body.center.x - inset_x, body.center.y - inset_y),
            (body.center.x + inset_x, body.center.y - inset_y),
            (body.center.x - inset_x, body.center.y + inset_y),
            (body.center.x + inset_x, body.center.y + inset_y),
        )
    elif isinstance(body, ClosedProfileBodySpec):
        width = body.bounds.x_max - body.bounds.x_min
        height = body.bounds.y_max - body.bounds.y_min
        fractions = (0.1, 0.3, 0.5, 0.7, 0.9)
        candidates = tuple(
            (body.bounds.x_min + width * x_fraction, body.bounds.y_min + height * y_fraction)
            for y_fraction in fractions
            for x_fraction in fractions
        )
    else:  # pragma: no cover - the DTO parser constructs only known variants.
        raise StepDeliveryError("CAD-STEP-001", "unsupported body DTO")
    for index, (x, y) in enumerate(candidates):
        points.append({"label": f"body_material_{index}", "point": [x, y, body.z.minimum + thickness * 0.5]})
    for operation in spec.operations:
        if isinstance(operation, SlotCutSpec):
            # Sample the exact B-Rep tool generated from the delivered contour.
            # This is generic profile-operation witnessing, not slot reconstruction.
            tool = _build_profile_tool(operation.profile.segments, operation.z)
            bounds = tool.BoundingBox()
            fractions = tuple(index / 10.0 for index in range(1, 10))
            z_levels = tuple(
                operation.z.minimum + (operation.z.maximum - operation.z.minimum) * fraction
                for fraction in (0.2, 0.5, 0.8)
            )
            sample_index = 0
            for y_fraction in fractions:
                y = bounds.ymin + (bounds.ymax - bounds.ymin) * y_fraction
                for x_fraction in fractions:
                    x = bounds.xmin + (bounds.xmax - bounds.xmin) * x_fraction
                    if tool.isInside(cq.Vector(x, y, z_levels[1]), 1e-7):
                        for depth_label, z in zip(("low", "mid", "high"), z_levels):
                            points.append({
                                "label": f"operation_{operation.operation_id}_profile_void_{sample_index}_{depth_label}",
                                "point": [x, y, z],
                            })
                    sample_index += 1
            if source_shape is not None:
                # A small deterministic ring outside the profile witnesses
                # retained material, including against bbox-overcut substitutes.
                for ring_index, (x_fraction, y_fraction) in enumerate(
                    ( (0.05, 0.05), (0.5, 0.05), (0.95, 0.05),
                      (0.05, 0.5), (0.95, 0.5),
                      (0.05, 0.95), (0.5, 0.95), (0.95, 0.95) )
                ):
                    x = bounds.xmin + (bounds.xmax - bounds.xmin) * x_fraction
                    y = bounds.ymin + (bounds.ymax - bounds.ymin) * y_fraction
                    point = cq.Vector(x, y, z_levels[1])
                    if not tool.isInside(point, 1e-7) and source_shape.isInside(point, 1e-7):
                        for depth_label, z in zip(("low", "mid", "high"), z_levels):
                            points.append({
                                "label": f"operation_{operation.operation_id}_profile_material_{ring_index}_{depth_label}",
                                "point": [x, y, z],
                            })
            if not any(probe["label"].startswith(f"operation_{operation.operation_id}_profile_void_") for probe in points):
                raise StepDeliveryError(
                    "CAD-STEP-001", "profile cut has no deterministic interior material probes",
                    operation_id=operation.operation_id,
                )
        else:
            for label, fraction in (("low", 0.2), ("mid", 0.5), ("high", 0.8)):
                points.append({
                    "label": f"operation_{operation.operation_id}_{label}",
                    "point": [operation.center.x, operation.center.y, body.z.minimum + thickness * fraction],
                })
    return points


def _parametric_material_probe_plan(
    request: Mapping[str, Any], shape: cq.Shape
) -> list[dict[str, Any]]:
    bounds = shape.BoundingBox()
    extents = (bounds.xlen, bounds.ylen, bounds.zlen)
    if not all(_finite(value) and value > 0.0 for value in extents):
        raise StepDeliveryError(
            "MANUFACTURING_BUILD_FAILED",
            "parametric body has invalid bounds for STEP equivalence probes",
            stage="manufacturing_build",
        )
    points: list[dict[str, Any]] = []
    fractions = (0.125, 0.375, 0.625, 0.875)
    for index, (x_fraction, y_fraction, z_fraction) in enumerate(
        (x, y, z) for z in fractions for y in fractions for x in fractions
    ):
        points.append({
            "label": f"parametric_body_grid_{index:02d}",
            "point": [
                bounds.xmin + bounds.xlen * x_fraction,
                bounds.ymin + bounds.ylen * y_fraction,
                bounds.zmin + bounds.zlen * z_fraction,
            ],
        })

    corners = [
        (x, y, z)
        for x in (bounds.xmin, bounds.xmax)
        for y in (bounds.ymin, bounds.ymax)
        for z in (bounds.zmin, bounds.zmax)
    ]
    for operation in request["operations"]:
        if operation["kind"] == "sweep":
            points.extend(_sweep_material_probe_plan(operation))
            continue
        if operation["kind"] == "loft":
            points.extend(_loft_material_probe_plan(operation))
            continue
        if operation["kind"] == "countersink":
            plane, direction = operation["plane"], operation["direction"]
            depth = operation["depth"]
            sample_depth = depth * 0.5
            outer_radius = operation["mouth_diameter"] / 2.0 - sample_depth * math.tan(math.radians(operation["included_angle_degrees"]) / 2.0)
            sample_radius = (operation["main_diameter"] / 2.0 + outer_radius) * 0.5
            point = [
                plane["origin"][i] + direction[i] * sample_depth + plane["u_axis"][i] * sample_radius
                for i in range(3)
            ]
            points.append({"label": f"countersink_wall_{operation['feature_id']}", "point": point})
            continue
        if operation["kind"] != "extrude" or operation.get("source_feature_kind") != "hole":
            continue
        plane, direction, extent = operation["plane"], operation["direction"], operation["extent"]
        origin, u_axis, v_axis = plane["origin"], plane["u_axis"], plane["v_axis"]
        if extent["kind"] == "distance":
            sample_depth = float(extent["length"]) * 0.5
        else:
            start_projection = sum(origin[i] * direction[i] for i in range(3))
            sample_depth = max(
                abs(sum(corner[i] * direction[i] for i in range(3)) - start_projection)
                for corner in corners
            ) * 0.5
        for loop in operation["loops"]:
            for curve in loop:
                if curve["kind"] != "circle":
                    continue
                cx, cy = curve["center"]
                center = [
                    origin[i] + u_axis[i] * cx + v_axis[i] * cy
                    + direction[i] * sample_depth
                    for i in range(3)
                ]
                points.append({
                    "label": f"hole_axis_{operation['feature_id']}",
                    "point": center,
                })
    return points


def _loft_material_probe_plan(operation: Mapping[str, Any]) -> list[dict[str, Any]]:
    from OCP.BRepClass import BRepClass_FaceClassifier
    from OCP.TopAbs import TopAbs_IN
    from OCP.gp import gp_Pnt
    from .parametric_rebuild import _build_profile_face, _world_point

    probes: list[dict[str, Any]] = []
    for index, section in enumerate(operation["sections"]):
        plane = section["plane"]
        loops = section["loops"]
        face = _build_profile_face(plane, loops, operation["feature_id"])
        coordinates: list[tuple[float, float]] = []
        for curve in loops[0]:
            if curve["kind"] == "circle":
                cx, cy = curve["center"]
                radius = curve["radius"]
                coordinates.extend(((cx-radius,cy-radius),(cx+radius,cy+radius)))
            else:
                coordinates.extend((tuple(curve["start"]), tuple(curve["end"])))
                if curve["kind"] == "arc":
                    cx, cy = curve["center"]
                    radius = curve["radius"]
                    coordinates.extend(((cx-radius,cy-radius),(cx+radius,cy+radius)))
        if not coordinates:
            raise StepDeliveryError(
                "MANUFACTURING_BUILD_FAILED", "Loft section has no profile bounds",
                stage="manufacturing_build", operation_id=operation["feature_id"],
            )
        xmin=min(point[0] for point in coordinates); xmax=max(point[0] for point in coordinates)
        ymin=min(point[1] for point in coordinates); ymax=max(point[1] for point in coordinates)
        candidates=[(xmin+(xmax-xmin)*fx,ymin+(ymax-ymin)*fy)
            for fy in (0.2,0.5,0.8) for fx in (0.2,0.5,0.8)]
        interior=[]
        for point in candidates:
            world=_world_point(plane,point)
            classifier=BRepClass_FaceClassifier(face.wrapped,gp_Pnt(*world),1e-7)
            if classifier.State()==TopAbs_IN:
                interior.append(point)
        if not interior:
            center=face.CenterOfMass()
            interior=[(
                sum((coordinate-plane["origin"][i])*plane["u_axis"][i] for i,coordinate in enumerate((center.x,center.y,center.z))),
                sum((coordinate-plane["origin"][i])*plane["v_axis"][i] for i,coordinate in enumerate((center.x,center.y,center.z))),
            )]
        for sample_index,point in enumerate(interior):
            world=_world_point(plane,point)
            probes.append({
                "label": f"loft_{operation['feature_id']}_section_{index}_material_{sample_index}",
                "point": list(world),
            })
    if len(probes)<len(operation["sections"]):
        raise StepDeliveryError(
            "MANUFACTURING_BUILD_FAILED", "Loft did not produce one material witness per section",
            stage="manufacturing_build", operation_id=operation["feature_id"],
        )
    return probes


def _sweep_material_probe_plan(operation: Mapping[str, Any]) -> list[dict[str, Any]]:
    """Sample engineering-request profile points along the declared Path.

    The station and section coordinates come from the normalized manufacturing
    request. The source and re-imported STEP solids are independently queried at
    these same world points; no kernel face/edge ordinal is involved.
    """

    from OCP.BRepClass import BRepClass_FaceClassifier
    from OCP.TopAbs import TopAbs_IN
    from OCP.gp import gp_Pnt
    from .parametric_rebuild import (
        _build_profile_face, _curve_tangent, _reverse_path_curves, _world_point,
    )

    plane = operation["plane"]
    path_plane = operation["path_plane"]
    profile_origin = operation["profile_origin"]
    profile_face = _build_profile_face(plane, operation["loops"], operation["feature_id"])

    coordinates: list[tuple[float, float]] = []
    for loop in operation["loops"]:
        for curve in loop:
            if curve["kind"] == "circle":
                cx, cy = curve["center"]
                radius = curve["radius"]
                coordinates.extend(((cx - radius, cy - radius), (cx + radius, cy + radius)))
            else:
                coordinates.extend((tuple(curve["start"]), tuple(curve["end"])))
                if curve["kind"] == "arc":
                    cx, cy = curve["center"]
                    radius = curve["radius"]
                    coordinates.extend(((cx - radius, cy - radius), (cx + radius, cy + radius)))
    if not coordinates:
        raise StepDeliveryError(
            "MANUFACTURING_BUILD_FAILED", "Sweep profile has no geometric bounds",
            stage="manufacturing_build", operation_id=operation["feature_id"],
        )
    xmin = min(value[0] for value in coordinates)
    xmax = max(value[0] for value in coordinates)
    ymin = min(value[1] for value in coordinates)
    ymax = max(value[1] for value in coordinates)
    section_points: list[tuple[float, float]] = []
    for fy in (0.1, 0.3, 0.5, 0.7, 0.9):
        for fx in (0.1, 0.3, 0.5, 0.7, 0.9):
            candidate = (xmin + (xmax - xmin) * fx, ymin + (ymax - ymin) * fy)
            world = _world_point(plane, candidate)
            classifier = BRepClass_FaceClassifier(profile_face.wrapped, gp_Pnt(*world), 1e-7)
            if classifier.State() == TopAbs_IN:
                section_points.append(candidate)
    if not section_points:
        center = profile_face.CenterOfMass()
        candidate = (
            sum((coordinate - plane["origin"][i]) * plane["u_axis"][i] for i, coordinate in enumerate((center.x, center.y, center.z))),
            sum((coordinate - plane["origin"][i]) * plane["v_axis"][i] for i, coordinate in enumerate((center.x, center.y, center.z))),
        )
        world = _world_point(plane, candidate)
        classifier = BRepClass_FaceClassifier(profile_face.wrapped, gp_Pnt(*world), 1e-7)
        if classifier.State() == TopAbs_IN:
            section_points.append(candidate)
    if not section_points:
        raise StepDeliveryError(
            "MANUFACTURING_BUILD_FAILED", "Sweep profile has no deterministic interior witness",
            stage="manufacturing_build", operation_id=operation["feature_id"],
        )

    curves = operation["path"] if operation["direction"] == "forward" else _reverse_path_curves(operation["path"])
    probes: list[dict[str, Any]] = []
    seen: list[tuple[float, float, float]] = []
    for segment_index, curve in enumerate(curves):
        for fraction in (0.0, 0.5, 1.0):
            if curve["kind"] == "line":
                point2 = tuple(curve["start"][i] + (curve["end"][i] - curve["start"][i]) * fraction for i in range(2))
                tangent2 = _curve_tangent(curve, True)
            else:
                start_angle = math.atan2(curve["start"][1] - curve["center"][1], curve["start"][0] - curve["center"][0])
                sweep = (
                    math.atan2(curve["end"][1] - curve["center"][1], curve["end"][0] - curve["center"][0]) - start_angle
                ) % math.tau
                if curve["direction"] == "clockwise":
                    sweep = -((-sweep) % math.tau)
                angle = start_angle + sweep * fraction
                radius = curve["radius"]
                point2 = (curve["center"][0] + radius * math.cos(angle), curve["center"][1] + radius * math.sin(angle))
                sign = 1.0 if curve["direction"] == "counter_clockwise" else -1.0
                tangent2 = (-sign * math.sin(angle), sign * math.cos(angle))
            tangent = tuple(
                path_plane["u_axis"][i] * tangent2[0] + path_plane["v_axis"][i] * tangent2[1]
                for i in range(3)
            )
            binormal = path_plane["normal"]
            section_u = (
                binormal[1] * tangent[2] - binormal[2] * tangent[1],
                binormal[2] * tangent[0] - binormal[0] * tangent[2],
                binormal[0] * tangent[1] - binormal[1] * tangent[0],
            )
            path_point = _world_point(path_plane, point2)
            for sample_index, (x, y) in enumerate(section_points):
                offset_x, offset_y = x - profile_origin[0], y - profile_origin[1]
                world = tuple(
                    path_point[i] + section_u[i] * offset_x + binormal[i] * offset_y
                    for i in range(3)
                )
                if any(math.dist(world, prior) <= 1e-8 for prior in seen):
                    continue
                seen.append(world)
                probes.append({
                    "label": f"sweep_{operation['feature_id']}_segment_{segment_index}_station_{fraction:.1f}_profile_{sample_index}",
                    "point": list(world),
                })
    if not probes:
        raise StepDeliveryError(
            "MANUFACTURING_BUILD_FAILED", "Sweep Path has no deterministic material stations",
            stage="manufacturing_build", operation_id=operation["feature_id"],
        )
    return probes


def _analytic_geometry_matches(source: cq.Shape, imported: cq.Shape) -> bool:
    """Compare stable curved geometry measurements without kernel ordinals.

    STEP may reorganize plane patches or topological indices. The v2 witness
    therefore compares the unique analytic edge/surface descriptors relevant
    to holes, revolved profiles, cones, and round treatments, alongside the
    independent bounds, mass, area, and material-point witnesses.
    """

    try:
        from .parametric_rebuild import _edge_evidence, _face_evidence

        def signatures(shape: cq.Shape) -> dict[str, list[dict[str, Any]]]:
            edges = []
            for record in _edge_evidence(shape):
                curve = record["curve"]
                if curve["kind"] not in {"circular", "elliptical"}:
                    continue
                signature = {key: value for key, value in curve.items() if key not in {"start", "end"}}
                edges.append(signature)
            faces = []
            for record in _face_evidence(shape):
                surface = record["surface"]
                if surface["kind"] not in {"cylinder", "cone", "torus"}:
                    continue
                faces.append(surface)
            return {"curved_edges": _unique_signatures(edges), "curved_faces": _unique_signatures(faces)}

        first, second = signatures(source), signatures(imported)
        differences = [
            f"{kind} ({len(first[kind])} source, {len(second[kind])} reimported)"
            for kind in first
            if not _signature_sets_match(first[kind], second[kind])
        ]
        if differences:
            raise StepDeliveryError(
                "STEP_EQUIVALENCE_FAILED",
                "analytic geometry descriptors differ: " + "; ".join(differences),
                stage="step_equivalence",
            )
        return True
    except StepDeliveryError:
        raise
    except Exception as error:
        raise StepDeliveryError(
            "STEP_EQUIVALENCE_FAILED",
            f"analytic feature witness could not be compared: {_safe_exception_message(error)}",
            stage="step_equivalence",
        ) from error


def _unique_signatures(values: list[dict[str, Any]]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for value in values:
        if not any(_signature_close(value, existing) for existing in result):
            result.append(value)
    return result


def _signature_sets_match(
    expected: list[dict[str, Any]], actual: list[dict[str, Any]]
) -> bool:
    remaining = list(actual)
    for signature in expected:
        match = next((index for index, item in enumerate(remaining) if _signature_close(signature, item)), None)
        if match is None:
            return False
        remaining.pop(match)
    return not remaining


def _signature_close(left: Any, right: Any) -> bool:
    if isinstance(left, bool) or isinstance(right, bool):
        return left is right
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return _finite(float(left)) and _finite(float(right)) and abs(float(left) - float(right)) <= 1.0e-5
    if isinstance(left, Mapping) and isinstance(right, Mapping):
        return left.keys() == right.keys() and all(_signature_close(left[key], right[key]) for key in left)
    if isinstance(left, (list, tuple)) and isinstance(right, (list, tuple)):
        return len(left) == len(right) and all(_signature_close(a, b) for a, b in zip(left, right))
    return left == right


def _probe_material(shape: cq.Shape, probes: list[dict[str, Any]]) -> list[bool]:
    values: list[bool] = []
    for probe in probes:
        x, y, z = probe["point"]
        try:
            values.append(bool(shape.isInside(cq.Vector(x, y, z), 1e-7)))
        except Exception as error:
            raise StepDeliveryError("CAD-STEP-003", _safe_exception_message(error)) from error
    return values


def _equivalence(
    source: dict[str, Any],
    imported: dict[str, Any],
    source_material: list[bool],
    imported_material: list[bool],
    *,
    feature_geometry_match: bool = True,
    require_topology_match: bool = True,
) -> dict[str, Any]:
    bbox_match = all(
        abs(source["bbox"][key] - imported["bbox"][key]) <= STEP_EQ_TOLERANCE_MM
        for key in ("x_min", "x_max", "y_min", "y_max", "z_min", "z_max")
    )
    source_volume = source["volume_mm3"]
    imported_volume = imported["volume_mm3"]
    volume_match = abs(source_volume - imported_volume) <= (
        STEP_VOLUME_ABS_TOLERANCE_MM3
        + STEP_VOLUME_REL_TOLERANCE * max(abs(source_volume), abs(imported_volume))
    )
    source_surface_area = source["surface_area_mm2"]
    imported_surface_area = imported["surface_area_mm2"]
    surface_area_match = abs(source_surface_area - imported_surface_area) <= (
        STEP_SURFACE_AREA_ABS_TOLERANCE_MM2
        + STEP_SURFACE_AREA_REL_TOLERANCE * max(abs(source_surface_area), abs(imported_surface_area))
    )
    center_of_mass_match = all(
        abs(source["center_of_mass"][index] - imported["center_of_mass"][index])
        <= STEP_CENTER_OF_MASS_TOLERANCE_MM
        for index in range(3)
    )
    topology_match = all(
        source[key] == imported[key]
        for key in ("edge_count", "face_count", "vertex_count")
    )
    material_match = source_material == imported_material
    return {
        "bbox_match": bbox_match,
        "volume_match": volume_match,
        "surface_area_match": surface_area_match,
        "center_of_mass_match": center_of_mass_match,
        "topology_match": topology_match,
        "feature_geometry_match": feature_geometry_match,
        "material_witness_match": material_match,
        "all_match": (
            bbox_match
            and volume_match
            and surface_area_match
            and center_of_mass_match
            and (topology_match or not require_topology_match)
            and feature_geometry_match
            and material_match
        ),
    }


def _confined_temp_dir(value: Any) -> Path:
    root = Path(value).resolve()
    if not root.is_dir():
        raise StepDeliveryError("CAD-STEP-005", "temporary directory is unavailable")
    return root


def _validate_artifact(path: Path) -> None:
    try:
        resolved = path.resolve()
        if resolved != path.parent.resolve() / path.name or resolved.suffix != ".step":
            raise StepDeliveryError("CAD-STEP-002", "STEP artifact path escaped the private temp directory")
        size = resolved.stat().st_size
    except StepDeliveryError:
        raise
    except OSError as error:
        raise StepDeliveryError("CAD-STEP-002", "STEP artifact could not be inspected") from error
    if size <= 0:
        raise StepDeliveryError("CAD-STEP-002", "STEP exporter produced an empty artifact")


def _controlled_worker_path(value: str) -> Path:
    path = Path(value)
    if path.suffix != ".step" or path.name != "validated.step":
        raise StepDeliveryError("CAD-STEP-002", "worker artifact path must be validated.step")
    try:
        parent = path.parent.resolve()
        if not parent.is_dir() or path.resolve() != parent / path.name or (path.exists() and path.is_symlink()):
            raise StepDeliveryError("CAD-STEP-002", "worker artifact path is not private and regular")
    except OSError as error:
        raise StepDeliveryError("CAD-STEP-002", "worker artifact path could not be inspected") from error
    return parent / path.name


def _artifact_metadata(path: Path) -> dict[str, Any]:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise StepDeliveryError("CAD-STEP-002", "STEP artifact could not be read") from error
    return {
        "extension": ".step",
        "size_bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def _as_step_error(error: SolidBuildSpecError, default_code: str) -> dict[str, Any]:
    result = error.as_error()
    result["stage"] = "step_delivery"
    if not result.get("code", "").startswith("CAD-STEP-"):
        result["code"] = default_code
    return result


def _finite(value: float) -> bool:
    return math.isfinite(float(value))


def _rounded(value: float) -> float:
    return round(float(value), 9)


def _rounded_timing(value: float) -> float:
    return round(float(value), 3)


def _safe_exception_message(error: Exception) -> str:
    message = str(error).replace("\n", " ").strip()
    return message[:512] or error.__class__.__name__


def main() -> int:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--artifact-path")
    try:
        arguments = parser.parse_args()
        payload = json.load(sys.stdin)
        response = (
            build_and_export_step_to_path(payload, arguments.artifact_path)
            if arguments.artifact_path
            else build_and_export_step(payload)
        )
    except Exception:
        response = {
            "ok": False,
            "error": {
                "stage": "step_delivery",
                "code": "CAD-STEP-005",
                "message": "request must be a valid JSON object",
            },
        }
    sys.stdout.write(json.dumps(response, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
