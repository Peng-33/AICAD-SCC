import copy
import hashlib
import math
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import cadquery as cq

from aicad_cad.step_delivery import (
    STEP_EQ_TOLERANCE_MM,
    _equivalence,
    _material_probe_plan,
    _probe_material,
    _solid_witness,
    build_and_export_step,
    build_and_export_step_to_path,
)

from aicad_cad.solid_backend import build_solid_from_spec
from aicad_cad.solid_build_spec import parse_solid_build_spec

from test_solid_backend import (
    GOLDEN,
    add_cut,
    circle_spec,
    concave_profile_spec,
    empty_spec,
    golden_spec,
    mixed_arc_profile_spec,
    rounded_rectangle_spec,
    shape_for,
    slot_profile_30deg,
    add_slot,
)

POLYGON_GOLDEN_PATH = (
    Path(__file__).resolve().parents[4]
    / "crates/scc-manufacturing/tests/goldens/polygon_solid_spec.json"
)
COMPOSITE_GOLDEN_PATH = (
    Path(__file__).resolve().parents[4]
    / "crates/scc-manufacturing/tests/goldens/composite_solid_spec.json"
)


def polygon_solid_spec():
    return json.loads(POLYGON_GOLDEN_PATH.read_text(encoding="utf-8"))


def composite_solid_spec():
    return json.loads(COMPOSITE_GOLDEN_PATH.read_text(encoding="utf-8"))



def five_hole_spec():
    spec = golden_spec()
    add_cut(spec, 4, "center-through", (0, 0), radius=5.0)
    return spec


def circular_flagship_spec():
    spec = circle_spec((0.0, 0.0), radius=80.0, thickness=20.0)
    add_cut(spec, 0, "center-through", (0.0, 0.0), radius=30.0, z=(0.0, 20.0))
    for index in range(8):
        angle = 2.0 * math.pi * index / 8.0
        add_cut(
            spec,
            index + 1,
            f"pattern:cut:{index:04}",
            (60.0 * math.cos(angle), 60.0 * math.sin(angle)),
            radius=7.0,
            z=(0.0, 20.0),
        )
    return spec


def project_v2_plate_with_through_hole():
    plane = {
        "origin": [0.0, 0.0, 0.0],
        "u_axis": [1.0, 0.0, 0.0],
        "v_axis": [0.0, 1.0, 0.0],
        "normal": [0.0, 0.0, 1.0],
    }
    points = [(-50.0, -30.0), (50.0, -30.0), (50.0, 30.0), (-50.0, 30.0)]
    base = {
        "feature_id": "base", "sketch_id": "base-sketch", "operation": "add",
        "plane": plane,
        "loops": [[{"kind": "line", "start": list(start), "end": list(end)}
                   for start, end in zip(points, points[1:] + points[:1])]],
        "profile_entity_ids": [[f"base-edge-{index}" for index in range(4)]],
        "direction": [0.0, 0.0, 1.0],
        "extent": {"kind": "distance", "length": 12.0},
        "source_feature_id": "base", "source_feature_kind": "extrude",
    }
    hole_plane = {**plane, "origin": [0.0, 0.0, 12.0]}
    hole = {
        "feature_id": "hole::main", "sketch_id": "hole-sketch", "operation": "cut",
        "plane": hole_plane,
        "loops": [[{"kind": "circle", "center": [10.0, 5.0], "radius": 4.0}]],
        "profile_entity_ids": [["hole-circle"]],
        "direction": [0.0, 0.0, -1.0], "extent": {"kind": "through_all"},
        "source_feature_id": "hole", "source_feature_kind": "hole",
        "blind_hole": False,
    }
    return {
        "manufacturing_kind": "project_v2",
        "spec": {
            "schema_version": 6, "model_sha256": "4" * 64,
            "operations": [
                {"kind": "extrude", "data": base},
                {"kind": "extrude", "data": hole},
            ],
        },
    }


def project_v2_loft_fixture(operation="add"):
    plane = {
        "origin": [0.0, 0.0, 0.0],
        "u_axis": [1.0, 0.0, 0.0],
        "v_axis": [0.0, 1.0, 0.0],
        "normal": [0.0, 0.0, 1.0],
    }
    features = []
    if operation == "cut":
        points = [(-50.0, -40.0), (50.0, -40.0), (50.0, 40.0), (-50.0, 40.0)]
        base = {
            "feature_id": "loft_cut_base", "sketch_id": "loft_cut_base_sketch", "operation": "add",
            "plane": plane,
            "loops": [[{"kind": "line", "start": list(start), "end": list(end)}
                       for start, end in zip(points, points[1:] + points[:1])]],
            "profile_entity_ids": [[f"loft-cut-base-{index}" for index in range(4)]],
            "direction": [0.0, 0.0, 1.0], "extent": {"kind": "distance", "length": 40.0},
            "source_feature_id": "loft_cut_base", "source_feature_kind": "extrude",
        }
        features.append({"kind": "extrude", "data": base})
    sections = []
    for index, (z, cx, cy, radius) in enumerate(((5.0, 0.0, 0.0, 4.0), (20.0, 1.0, 0.0, 8.0), (35.0, 2.0, 1.0, 5.0))):
        section_plane = {**plane, "origin": [0.0, 0.0, z]}
        sections.append({
            "sketch_id": f"loft_section_{index}", "plane": section_plane,
            "loops": [[{"kind": "circle", "center": [cx, cy], "radius": radius}]],
            "profile_entity_ids": [[f"loft-section-{index}-circle"]],
        })
    loft = {
        "feature_id": "loft_body", "operation": operation, "mode": "ruled",
        "orientation_policy": "aligned", "sections": sections,
    }
    features.append({"kind": "loft", "data": loft})
    return {
        "manufacturing_kind": "project_v2",
        "spec": {"schema_version": 6, "model_sha256": "7" * 64, "operations": features},
    }


def project_v2_sweep_arc():
    path_plane = {
        "origin": [0.0, 0.0, 0.0],
        "u_axis": [1.0, 0.0, 0.0],
        "v_axis": [0.0, 1.0, 0.0],
        "normal": [0.0, 0.0, 1.0],
    }
    profile_plane = {
        "origin": [0.0, 0.0, 0.0],
        "u_axis": [0.0, 1.0, 0.0],
        "v_axis": [0.0, 0.0, 1.0],
        "normal": [1.0, 0.0, 0.0],
    }
    sweep = {
        "feature_id": "curved_sweep",
        "sketch_id": "circular_profile",
        "path_id": "quarter_arc",
        "operation": "add",
        "plane": profile_plane,
        "loops": [[{"kind": "circle", "center": [0.0, 0.0], "radius": 2.0}]],
        "profile_entity_ids": [["profile-circle"]],
        "profile_origin": [0.0, 0.0],
        "path_plane": path_plane,
        "path": [{
            "kind": "arc", "start": [0.0, 0.0], "end": [10.0, 10.0],
            "center": [0.0, 10.0], "radius": 10.0,
            "direction": "counter_clockwise",
        }],
        "path_entity_ids": ["path-arc"],
        "direction": "forward",
        "orientation": "fixed_binormal",
    }
    return {
        "manufacturing_kind": "project_v2",
        "spec": {
            "schema_version": 6,
            "model_sha256": "5" * 64,
            "operations": [{"kind": "sweep", "data": sweep}],
        },
    }


def result(spec):
    value = build_and_export_step(spec)
    if not value["ok"]:
        raise AssertionError(value)
    return value


def translated_mixed_arc_profile_spec():
    spec = mixed_arc_profile_spec()
    for segment in spec["part"]["body"]["footprint"]["segments"]:
        for key in ("start", "end", "center"):
            if key in segment:
                segment[key]["x"] += 30.0
                segment[key]["y"] -= 20.0
    bounds = spec["part"]["bounds"]
    bounds["min_x"] += 30.0
    bounds["max_x"] += 30.0
    bounds["min_y"] -= 20.0
    bounds["max_y"] -= 20.0
    return spec


def compare_with_candidate(spec, candidate):
    source = shape_for(spec)
    probes = _material_probe_plan(parse_solid_build_spec(spec), source)
    equivalence = _equivalence(
        _solid_witness(source),
        _solid_witness(candidate),
        _probe_material(source, probes),
        _probe_material(candidate, probes),
    )
    return source, equivalence


def _build_slot_tool_for_test(spec):
    from aicad_cad.solid_backend import _build_profile_tool
    from aicad_cad.solid_build_spec import parse_solid_build_spec
    decoded = parse_solid_build_spec(spec)
    operation = decoded.operations[0]
    return _build_profile_tool(operation.profile.segments, operation.z).BoundingBox()


class TrackingTemp:
    def __init__(self):
        self.inner = None
        self.exited = False

    def __call__(self, **kwargs):
        self.inner = tempfile.TemporaryDirectory(**kwargs)
        return self

    def __enter__(self):
        return self.inner.__enter__()

    def __exit__(self, *args):
        self.exited = True
        return self.inner.__exit__(*args)


class StepDeliveryTests(unittest.TestCase):
    def test_project_v2_history_uses_shared_rebuild_and_validates_the_delivered_step_bytes(self):
        spec = project_v2_plate_with_through_hole()
        delivered = build_and_export_step(spec)
        self.assertTrue(delivered["ok"], delivered.get("error"))
        self.assertEqual(delivered["source_solid"]["solid_count"], 1)
        self.assertEqual(delivered["source_solid"]["bbox"], {
            "x_min": -50.0, "x_max": 50.0,
            "y_min": -30.0, "y_max": 30.0,
            "z_min": 0.0, "z_max": 12.0,
        })
        self.assertEqual(delivered["body_source"], {"feature": "base", "entity": "base"})
        self.assertEqual(
            [(item["operation_id"], item["source_id"], item["status"])
             for item in delivered["operation_results"]],
            [("base", "base", "applied"), ("hole::main", "hole", "applied")],
        )
        self.assertTrue(delivered["equivalence"]["feature_geometry_match"])
        self.assertTrue(delivered["equivalence"]["all_match"])
        self.assertGreaterEqual(delivered["material_witness"]["probe_count"], 65)
        timings = delivered["timings_ms"]
        self.assertEqual(set(timings), {
            "manufacturing_build_ms", "step_serialization_ms", "step_reimport_ms",
            "equivalence_validation_ms", "total_ms",
        })
        self.assertTrue(all(math.isfinite(value) and value >= 0 for value in timings.values()))
        self.assertGreaterEqual(timings["total_ms"], sum(
            timings[key] for key in (
                "manufacturing_build_ms", "step_serialization_ms", "step_reimport_ms",
                "equivalence_validation_ms",
            )
        ))

        # Drawing rebuild and manufacturing use the same validated request and
        # executor, so the core OCP body witnesses must agree.
        from aicad_cad.parametric_rebuild import rebuild_parametric_model
        drawing = rebuild_parametric_model(spec["spec"])
        self.assertTrue(drawing["ok"], drawing.get("error"))
        self.assertEqual(drawing["result"]["bbox"], delivered["source_solid"]["bbox"])
        self.assertAlmostEqual(
            drawing["result"]["spatial_evidence"]["volume_mm3"],
            delivered["source_solid"]["volume_mm3"], places=5,
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            artifact_path = Path(temp_dir) / "validated.step"
            result = build_and_export_step_to_path(spec, str(artifact_path))
            self.assertTrue(result["ok"], result.get("error"))
            self.assertEqual(
                hashlib.sha256(artifact_path.read_bytes()).hexdigest(),
                result["artifact"]["sha256"],
            )

    def test_project_v2_ruled_loft_add_and_cut_reimport_same_validated_bytes(self):
        from aicad_cad.parametric_rebuild import rebuild_parametric_model, section_parametric_model

        for operation in ("add", "cut"):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as temp_dir:
                spec = project_v2_loft_fixture(operation)
                artifact_path = Path(temp_dir) / "validated.step"
                delivered = build_and_export_step_to_path(spec, str(artifact_path))
                self.assertTrue(delivered["ok"], delivered.get("error"))
                self.assertEqual(hashlib.sha256(artifact_path.read_bytes()).hexdigest(), delivered["artifact"]["sha256"])
                self.assertEqual(delivered["source_solid"]["solid_count"], 1)
                self.assertEqual(delivered["reimported_solid"]["solid_count"], 1)
                self.assertTrue(delivered["equivalence"]["all_match"], delivered["equivalence"])
                self.assertTrue(delivered["material_witness"]["all_match"])
                loft_probes = [probe for probe in delivered["material_witness"]["probes"] if probe["label"].startswith("loft_loft_body_section_")]
                self.assertEqual(len({probe["label"].split("_material_")[0] for probe in loft_probes}), 3)
                self.assertTrue(all(probe["source_material"] == probe["reimported_material"] for probe in loft_probes))

                drawing = rebuild_parametric_model(spec["spec"])
                self.assertTrue(drawing["ok"], drawing.get("error"))
                self.assertEqual(drawing["result"]["bbox"], delivered["source_solid"]["bbox"])
                self.assertAlmostEqual(drawing["result"]["spatial_evidence"]["volume_mm3"], delivered["source_solid"]["volume_mm3"], places=5)
                self.assertEqual(set(drawing["result"]["spatial_evidence"]["views"]), {"front", "top", "left"})

                section = section_parametric_model({
                    "action": "section", "rebuild": spec["spec"],
                    "section_plane": {"axis": "y", "offset_mm": 0.0, "view_side": "positive"},
                })
                self.assertTrue(section["ok"], section.get("error"))
                self.assertGreaterEqual(len(section["result"]["section_geometry"]["loops"]), 1)
                self.assertEqual(section["result"]["section_geometry"]["sample_chord_tolerance_mm"], 0.01)

    def test_project_v2_curved_sweep_is_reimported_and_the_delivered_bytes_are_the_validated_bytes(self):
        from aicad_cad.parametric_rebuild import _face_evidence, rebuild_parametric_model

        spec = project_v2_sweep_arc()
        with tempfile.TemporaryDirectory() as temp_dir:
            artifact_path = Path(temp_dir) / "validated.step"
            delivered = build_and_export_step_to_path(spec, str(artifact_path))
            self.assertTrue(delivered["ok"], delivered.get("error"))
            self.assertEqual(
                hashlib.sha256(artifact_path.read_bytes()).hexdigest(),
                delivered["artifact"]["sha256"],
            )
            self.assertEqual(delivered["source_solid"]["solid_count"], 1)
            self.assertEqual(delivered["reimported_solid"]["solid_count"], 1)
            self.assertTrue(delivered["source_solid"]["valid"])
            self.assertTrue(delivered["reimported_solid"]["valid"])
            self.assertEqual(delivered["body_source"], {"feature": "curved_sweep", "entity": "curved_sweep"})
            self.assertTrue(delivered["equivalence"]["feature_geometry_match"])
            self.assertTrue(delivered["equivalence"]["all_match"])
            self.assertTrue(delivered["material_witness"]["all_match"])
            self.assertGreaterEqual(delivered["material_witness"]["probe_count"], 12)
            self.assertTrue(all(
                probe["source_material"] == probe["reimported_material"]
                for probe in delivered["material_witness"]["probes"]
            ))

            imported = cq.importers.importStep(str(artifact_path)).val()
            analytic_faces = [
                record["surface"] for record in _face_evidence(imported)
                if record["surface"]["kind"] in {"torus", "cylinder"}
            ]
            self.assertTrue(any(face["kind"] == "torus" for face in analytic_faces), analytic_faces)
            self.assertTrue(any(
                abs(face["major_radius"] - 10.0) <= 1e-5
                and abs(face["minor_radius"] - 2.0) <= 1e-5
                for face in analytic_faces if face["kind"] == "torus"
            ), analytic_faces)

            drawing = rebuild_parametric_model(spec["spec"])
            self.assertTrue(drawing["ok"], drawing.get("error"))
            self.assertEqual(drawing["result"]["bbox"], delivered["source_solid"]["bbox"])
            self.assertAlmostEqual(
                drawing["result"]["spatial_evidence"]["volume_mm3"],
                delivered["source_solid"]["volume_mm3"], places=5,
            )

    def test_project_v2_chamfered_hole_normalizes_cone_axis_on_step_reimport(self):
        spec = project_v2_plate_with_through_hole()
        spec["spec"]["operations"].append({
            "kind": "chamfer",
            "data": {
                "feature_id": "hole-mouth-chamfer",
                "edge_refs": [{
                    "source_feature_id": "hole",
                    "selector": {"kind": "hole_entrance"},
                    "qualifier": {
                        "curve_kind": "circular", "radius_mm": 4.0,
                        "length_mm": None, "midpoint": None, "tolerance_mm": 1e-6,
                    },
                }],
                "distance": 0.5,
            },
        })
        delivered = build_and_export_step(spec)
        self.assertTrue(delivered["ok"], delivered.get("error"))
        self.assertTrue(delivered["equivalence"]["feature_geometry_match"])
        self.assertTrue(delivered["equivalence"]["all_match"])

    def test_project_v2_detached_add_fails_before_step_serialization(self):
        spec = project_v2_plate_with_through_hole()
        detached = copy.deepcopy(spec["spec"]["operations"][0]["data"])
        detached["feature_id"] = "detached-boss"
        detached["sketch_id"] = "detached-sketch"
        detached["source_feature_id"] = "detached-boss"
        detached["plane"] = {**detached["plane"], "origin": [200.0, 0.0, 0.0]}
        spec["spec"]["operations"] = [spec["spec"]["operations"][0], {"kind": "extrude", "data": detached}]
        response = build_and_export_step(spec)
        self.assertFalse(response["ok"])
        self.assertIn(response["error"]["code"], {"DETACHED_ADD", "MULTI_BODY_NOT_ALLOWED"})
        self.assertEqual(response["error"]["stage"], "manufacturing_build")
        self.assertNotIn("artifact", response)

    def test_circle_disk_translation_annulus_and_flagship_roundtrip(self):
        disk = result(circle_spec())
        self.assertEqual(disk["source_solid"]["bbox"], {
            "x_min": -80.0, "x_max": 80.0,
            "y_min": -80.0, "y_max": 80.0,
            "z_min": 0.0, "z_max": 20.0,
        })
        self.assertAlmostEqual(disk["source_solid"]["volume_mm3"], math.pi * 80.0 * 80.0 * 20.0, places=3)
        self.assertTrue(disk["equivalence"]["all_match"])

        translated = result(circle_spec((30.0, -20.0)))
        self.assertEqual(translated["source_solid"]["bbox"], {
            "x_min": -50.0, "x_max": 110.0,
            "y_min": -100.0, "y_max": 60.0,
            "z_min": 0.0, "z_max": 20.0,
        })
        self.assertTrue(translated["equivalence"]["all_match"])

        annulus_spec = circle_spec((30.0, -20.0))
        add_cut(annulus_spec, 0, "center-through", (30.0, -20.0), radius=30.0, z=(0.0, 20.0))
        annulus = result(annulus_spec)
        self.assertAlmostEqual(
            annulus["source_solid"]["volume_mm3"],
            math.pi * (80.0 * 80.0 - 30.0 * 30.0) * 20.0,
            places=3,
        )
        self.assertTrue(annulus["equivalence"]["all_match"])

        flagship = result(circular_flagship_spec())
        self.assertEqual(len(flagship["operation_results"]), 9)
        self.assertEqual(flagship["material_witness"]["probe_count"], 4 + 9 * 3)
        self.assertTrue(flagship["equivalence"]["all_match"])

    def test_slot_step_roundtrip_and_adversarial_geometry_witnesses(self):
        spec = empty_spec()
        add_slot(spec, 0, "slot:cut:0000")
        delivered = result(spec)
        self.assertEqual(delivered["operation_results"][0]["status"], "applied")
        self.assertTrue(delivered["equivalence"]["all_match"])
        slot_void_probes = [probe for probe in delivered["material_witness"]["probes"] if "_profile_void_" in probe["label"]]
        slot_material_probes = [probe for probe in delivered["material_witness"]["probes"] if "_profile_material_" in probe["label"]]
        self.assertGreater(len(slot_void_probes), 3)
        self.assertGreater(len(slot_material_probes), 3)
        self.assertTrue(all(not probe["source_material"] and not probe["reimported_material"] for probe in slot_void_probes))
        self.assertTrue(all(probe["source_material"] and probe["reimported_material"] for probe in slot_material_probes))
        self.assertEqual(len({probe["point"][2] for probe in slot_void_probes}), 3)

        source = shape_for(spec)
        source_witness = _solid_witness(source)
        base = shape_for(empty_spec())
        _, omission = compare_with_candidate(spec, base)
        self.assertFalse(omission["volume_match"])
        self.assertFalse(omission["material_witness_match"])
        self.assertFalse(omission["all_match"])

        def transform_profile(profile, fn):
            transformed = copy.deepcopy(profile)
            for segment in transformed:
                for key in ("start", "end", "center"):
                    if key in segment:
                        segment[key]["x"], segment[key]["y"] = fn(segment[key]["x"], segment[key]["y"])
            return transformed

        center = (15.0, -5.0)
        angle = math.radians(60.0)
        wrong_orientation = copy.deepcopy(spec)
        wrong_orientation["operations"]["0"]["profile"] = {"segments": transform_profile(
            slot_profile_30deg(),
            lambda x, y: (center[0] + (x-center[0])*math.cos(angle) - (y-center[1])*math.sin(angle),
                          center[1] + (x-center[0])*math.sin(angle) + (y-center[1])*math.cos(angle)),
        )}
        wrong_orientation_shape = shape_for(wrong_orientation)
        _, orientation_equivalence = compare_with_candidate(spec, wrong_orientation_shape)
        self.assertEqual(source_witness["bbox"], _solid_witness(wrong_orientation_shape)["bbox"])
        self.assertTrue(orientation_equivalence["volume_match"])
        self.assertTrue(orientation_equivalence["surface_area_match"])
        self.assertFalse(orientation_equivalence["material_witness_match"])
        self.assertFalse(orientation_equivalence["all_match"])

        # Widen only the normal direction, preserving cap-center spacing.
        wider = copy.deepcopy(spec)
        line = wider["operations"]["0"]["profile"]["segments"][0]
        dx = line["end"]["x"] - line["start"]["x"]
        dy = line["end"]["y"] - line["start"]["y"]
        axis_length = math.hypot(dx, dy)
        normal = (-dy / axis_length, dx / axis_length)
        widen = 14.0 / 12.0
        profile = wider["operations"]["0"]["profile"]["segments"]
        for segment in profile:
            for key in ("start", "end", "center"):
                if key in segment:
                    point = segment[key]
                    projection = (point["x"]-center[0])*normal[0] + (point["y"]-center[1])*normal[1]
                    point["x"] += normal[0] * projection * (widen - 1.0)
                    point["y"] += normal[1] * projection * (widen - 1.0)
            if segment["kind"] == "arc":
                segment["radius"] *= widen
        wider_shape = shape_for(wider)
        _, width_equivalence = compare_with_candidate(spec, wider_shape)
        self.assertFalse(width_equivalence["volume_match"])
        self.assertFalse(width_equivalence["all_match"])

        chord = copy.deepcopy(spec)
        contour = chord["operations"]["0"]["profile"]["segments"]
        for index, segment in enumerate(contour):
            if segment["kind"] == "arc":
                contour[index] = {"kind": "line", "start": segment["start"], "end": segment["end"]}
        chord_shape = shape_for(chord)
        _, chord_equivalence = compare_with_candidate(spec, chord_shape)
        self.assertFalse(chord_equivalence["surface_area_match"])
        self.assertFalse(chord_equivalence["all_match"])

        tool_bounds = _build_slot_tool_for_test(spec)
        bbox_tool = cq.Solid.makeBox(
            tool_bounds.xmax-tool_bounds.xmin, tool_bounds.ymax-tool_bounds.ymin, 10.0,
            cq.Vector(tool_bounds.xmin, tool_bounds.ymin, 0.0),
        )
        bbox_substitution = base.cut(bbox_tool).clean()
        _, bbox_equivalence = compare_with_candidate(spec, bbox_substitution)
        self.assertEqual(source_witness["bbox"], _solid_witness(bbox_substitution)["bbox"])
        self.assertFalse(bbox_equivalence["volume_match"])
        self.assertFalse(bbox_equivalence["material_witness_match"])
        self.assertFalse(bbox_equivalence["all_match"])

    def test_closed_profile_concave_and_mixed_arc_step_roundtrip(self):
        concave = result(concave_profile_spec())
        self.assertEqual(concave["source_solid"]["bbox"], {
            "x_min": 0.0, "x_max": 100.0,
            "y_min": 0.0, "y_max": 100.0,
            "z_min": 0.0, "z_max": 10.0,
        })
        self.assertAlmostEqual(concave["source_solid"]["volume_mm3"], 76000.0, places=5)
        self.assertEqual(concave["material_witness"]["probe_count"], 25)
        self.assertTrue(concave["equivalence"]["all_match"])

        mixed = result(mixed_arc_profile_spec())
        self.assertEqual(mixed["source_solid"]["bbox"]["x_max"], 100.0)
        self.assertEqual(mixed["source_solid"]["bbox"]["y_max"], 60.0)
        self.assertTrue(mixed["equivalence"]["all_match"])

    def test_translated_closed_profile_preserves_cadquery_step_bbox_and_center_of_mass(self):
        origin = result(mixed_arc_profile_spec())
        translated = result(translated_mixed_arc_profile_spec())
        expected_bbox = {
            "x_min": 30.0, "x_max": 130.0,
            "y_min": -20.0, "y_max": 40.0,
            "z_min": 0.0, "z_max": 10.0,
        }
        self.assertEqual(translated["source_solid"]["bbox"], expected_bbox)
        self.assertEqual(translated["reimported_solid"]["bbox"], expected_bbox)
        for witness in (translated["source_solid"], translated["reimported_solid"]):
            self.assertTrue(witness["valid"])
        for index, offset in enumerate((30.0, -20.0, 0.0)):
            self.assertAlmostEqual(
                translated["source_solid"]["center_of_mass"][index]
                - origin["source_solid"]["center_of_mass"][index],
                offset,
                places=6,
            )
            self.assertAlmostEqual(
                translated["reimported_solid"]["center_of_mass"][index]
                - origin["reimported_solid"]["center_of_mass"][index],
                offset,
                places=6,
            )
        self.assertTrue(translated["equivalence"]["all_match"])
        self.assertTrue(translated["equivalence"]["surface_area_match"])
        self.assertTrue(translated["equivalence"]["center_of_mass_match"])
        self.assertTrue(translated["equivalence"]["topology_match"])
        self.assertTrue(translated["equivalence"]["material_witness_match"])

    def test_rust_polygon_golden_cadquery_step_roundtrip(self):
        spec = polygon_solid_spec()
        value = result(spec)
        expected_bbox = {
            "x_min": 0.0, "x_max": 120.0,
            "y_min": 0.0, "y_max": 90.0,
            "z_min": 0.0, "z_max": 8.0,
        }
        self.assertEqual(value["source_solid"]["bbox"], expected_bbox)
        self.assertEqual(value["reimported_solid"]["bbox"], expected_bbox)
        self.assertTrue(value["equivalence"]["all_match"])
        self.assertTrue(value["equivalence"]["material_witness_match"])
        self.assertTrue(value["source_solid"]["valid"])
        self.assertTrue(value["reimported_solid"]["valid"])

    def test_composite_golden_uses_generic_exact_closed_profile(self):
        spec = composite_solid_spec()
        self.assertEqual(spec["part"]["body"]["footprint"]["kind"], "closed_profile")
        segments = spec["part"]["body"]["footprint"]["segments"]
        self.assertEqual([segment["kind"] for segment in segments], [
            "line", "arc", "line", "line", "arc", "line", "line",
        ])
        self.assertEqual(segments[1]["direction"], "counter_clockwise")
        self.assertEqual(segments[4]["direction"], "clockwise")

        base = copy.deepcopy(spec)
        base["operations"] = {}
        base["source_map"]["operations"] = {}
        decoded = parse_solid_build_spec(base)
        self.assertEqual([type(item).__name__ for item in decoded.body.segments], [
            "LineProfileSegment", "ArcProfileSegment", "LineProfileSegment",
            "LineProfileSegment", "ArcProfileSegment", "LineProfileSegment",
            "LineProfileSegment",
        ])
        shape = shape_for(base)
        self.assertEqual(sum(edge.geomType() == "CIRCLE" for edge in shape.Edges()), 4)
        backend = build_solid_from_spec(spec)
        self.assertTrue(backend["ok"], backend)
        self.assertEqual(backend["operation_count"], 5)
        self.assertEqual(backend["body_source"]["entity"], "main_segment_0")

    def test_composite_step_roundtrip_preserves_arcs_cuts_and_validated_artifact(self):
        spec = composite_solid_spec()
        value = result(spec)
        expected_bbox = {
            "x_min": 30.0, "x_max": 130.0,
            "y_min": -20.0, "y_max": 40.0,
            "z_min": 0.0, "z_max": 10.0,
        }
        self.assertEqual(value["source_solid"]["bbox"], expected_bbox)
        self.assertEqual(value["reimported_solid"]["bbox"], expected_bbox)
        self.assertEqual(value["source_solid"]["solid_count"], 1)
        self.assertEqual(value["reimported_solid"]["solid_count"], 1)
        self.assertEqual(len(value["operation_results"]), 5)
        self.assertTrue(value["source_solid"]["valid"])
        self.assertTrue(value["reimported_solid"]["valid"])
        self.assertTrue(value["equivalence"]["all_match"])
        self.assertTrue(value["equivalence"]["bbox_match"])
        self.assertTrue(value["equivalence"]["volume_match"])
        self.assertTrue(value["equivalence"]["surface_area_match"])
        self.assertTrue(value["equivalence"]["center_of_mass_match"])
        self.assertTrue(value["equivalence"]["topology_match"])
        self.assertTrue(value["equivalence"]["material_witness_match"])
        self.assertGreater(value["artifact"]["size_bytes"], 1000)
        self.assertEqual(len(value["artifact"]["sha256"]), 64)

    def test_composite_wrong_arc_direction_and_chord_are_rejected_by_equivalence(self):
        spec = composite_solid_spec()
        source = shape_for(spec)
        source_witness = _solid_witness(source)

        chord = copy.deepcopy(spec)
        chord_segments = chord["part"]["body"]["footprint"]["segments"]
        arc = chord_segments[1]
        chord_segments[1] = {"kind": "line", "start": arc["start"], "end": arc["end"]}
        chord_shape = shape_for(chord)
        _, chord_equivalence = compare_with_candidate(spec, chord_shape)
        self.assertEqual(source_witness["bbox"], _solid_witness(chord_shape)["bbox"])
        self.assertFalse(chord_equivalence["volume_match"])
        self.assertFalse(chord_equivalence["all_match"])

        wrong_direction = copy.deepcopy(spec)
        wrong_direction["part"]["body"]["footprint"]["segments"][1]["direction"] = "clockwise"
        wrong_shape = shape_for(wrong_direction)
        _, direction_equivalence = compare_with_candidate(spec, wrong_shape)
        self.assertFalse(direction_equivalence["all_match"])

        wrong_radius = copy.deepcopy(spec)
        wrong_radius["part"]["body"]["footprint"]["segments"][1]["radius"] = 21.0
        rejected = build_solid_from_spec(wrong_radius)
        self.assertFalse(rejected["ok"])

    def test_composite_bbox_substitution_and_translation_loss_are_rejected(self):
        spec = composite_solid_spec()
        source = shape_for(spec)
        bbox_shape = cq.Solid.makeBox(100.0, 60.0, 10.0, cq.Vector(30.0, -20.0, 0.0))
        source_witness = _solid_witness(source)
        bbox_witness = _solid_witness(bbox_shape)
        _, bbox_equivalence = compare_with_candidate(spec, bbox_shape)
        self.assertEqual(source_witness["bbox"], bbox_witness["bbox"])
        self.assertFalse(bbox_equivalence["volume_match"])
        self.assertFalse(bbox_equivalence["material_witness_match"])
        self.assertFalse(bbox_equivalence["all_match"])

        untranslated = copy.deepcopy(spec)
        for segment in untranslated["part"]["body"]["footprint"]["segments"]:
            for key in ("start", "end", "center"):
                if key in segment:
                    segment[key]["x"] -= 30.0
                    segment[key]["y"] += 20.0
        bounds = untranslated["part"]["bounds"]
        bounds["min_x"] -= 30.0
        bounds["max_x"] -= 30.0
        bounds["min_y"] += 20.0
        bounds["max_y"] += 20.0
        for operation in untranslated["operations"].values():
            operation["center"]["x"] -= 30.0
            operation["center"]["y"] += 20.0
        untranslated_shape = shape_for(untranslated)
        _, translation_equivalence = compare_with_candidate(spec, untranslated_shape)
        self.assertFalse(translation_equivalence["bbox_match"])
        self.assertFalse(translation_equivalence["center_of_mass_match"])
        self.assertFalse(translation_equivalence["all_match"])

    def test_polygon_bbox_extrusion_substitution_is_rejected(self):
        spec = polygon_solid_spec()
        bbox_shape = cq.Solid.makeBox(120.0, 90.0, 8.0, cq.Vector(0, 0, 0))
        source_shape = shape_for(spec)
        source_witness = _solid_witness(source_shape)
        bbox_witness = _solid_witness(bbox_shape)
        _, equivalence = compare_with_candidate(spec, bbox_shape)

        self.assertEqual(source_witness["bbox"], bbox_witness["bbox"])
        self.assertNotEqual(source_witness["volume_mm3"], bbox_witness["volume_mm3"])
        self.assertFalse(equivalence["volume_match"])
        self.assertFalse(equivalence["material_witness_match"])
        self.assertFalse(equivalence["all_match"])

    def test_polygon_wrong_vertex_with_same_bbox_is_rejected(self):
        spec = polygon_solid_spec()
        wrong_contour = copy.deepcopy(spec)
        segments = wrong_contour["part"]["body"]["footprint"]["segments"]
        segments[0]["end"]["y"] = 2.0
        segments[1]["start"]["y"] = 2.0
        wrong_shape = shape_for(wrong_contour)
        source_witness = _solid_witness(shape_for(spec))
        wrong_witness = _solid_witness(wrong_shape)
        _, equivalence = compare_with_candidate(spec, wrong_shape)

        self.assertEqual(source_witness["bbox"], wrong_witness["bbox"])
        self.assertNotEqual(source_witness["volume_mm3"], wrong_witness["volume_mm3"])
        self.assertFalse(equivalence["volume_match"])
        self.assertFalse(equivalence["all_match"])
    def test_bbox_extrusion_substitution_is_rejected_by_actual_equivalence(self):
        spec = concave_profile_spec()
        source_shape = shape_for(spec)
        bbox_shape = cq.Solid.makeBox(100.0, 100.0, 10.0, cq.Vector(0, 0, 0))
        source_witness = _solid_witness(source_shape)
        bbox_witness = _solid_witness(bbox_shape)
        _, equivalence = compare_with_candidate(spec, bbox_shape)

        self.assertEqual(source_witness["bbox"], bbox_witness["bbox"])
        self.assertNotEqual(source_witness["volume_mm3"], bbox_witness["volume_mm3"])
        self.assertFalse(equivalence["all_match"])
        self.assertFalse(equivalence["volume_match"])
        self.assertFalse(equivalence["material_witness_match"])

    def test_arc_to_chord_substitution_is_rejected_by_actual_equivalence(self):
        spec = mixed_arc_profile_spec()
        chord_spec = copy.deepcopy(spec)
        chord_spec["part"]["body"]["footprint"]["segments"][1] = {
            "kind": "line",
            "start": {"x": 80.0, "y": 0.0},
            "end": {"x": 100.0, "y": 20.0},
        }
        source_shape = shape_for(spec)
        chord_shape = shape_for(chord_spec)
        source_witness = _solid_witness(source_shape)
        chord_witness = _solid_witness(chord_shape)
        _, equivalence = compare_with_candidate(spec, chord_shape)

        self.assertEqual(source_witness["bbox"], chord_witness["bbox"])
        self.assertNotEqual(source_witness["volume_mm3"], chord_witness["volume_mm3"])
        self.assertFalse(equivalence["all_match"])

    def test_translation_loss_to_origin_is_rejected_by_actual_equivalence(self):
        translated = translated_mixed_arc_profile_spec()
        origin_shape = shape_for(mixed_arc_profile_spec())
        _, equivalence = compare_with_candidate(translated, origin_shape)

        self.assertFalse(equivalence["bbox_match"])
        self.assertFalse(equivalence["center_of_mass_match"])
        self.assertFalse(equivalence["all_match"])

    def test_open_closed_profile_is_rejected_before_face_builder(self):
        open_spec = mixed_arc_profile_spec()
        open_spec["part"]["body"]["footprint"]["segments"].pop()
        with patch("aicad_cad.solid_backend._build_closed_profile") as face_builder:
            value = build_solid_from_spec(open_spec)
        self.assertFalse(value["ok"])
        self.assertEqual(value["error"]["code"], "CAD-BACKEND-004")
        face_builder.assert_not_called()

    def test_rounded_rectangle_translation_hole_and_exact_step_roundtrip(self):
        spec = rounded_rectangle_spec((25.0, -15.0), width=120.0, height=80.0, radius=10.0, thickness=8.0)
        add_cut(spec, 0, "center-through", (25.0, -15.0), radius=6.0, z=(0.0, 8.0))
        value = result(spec)
        self.assertEqual(value["source_solid"]["bbox"], {
            "x_min": -35.0, "x_max": 85.0,
            "y_min": -55.0, "y_max": 25.0,
            "z_min": 0.0, "z_max": 8.0,
        })
        expected_volume = ((120.0 * 80.0 - (4.0 - math.pi) * 10.0 * 10.0) - math.pi * 6.0 * 6.0) * 8.0
        self.assertAlmostEqual(value["source_solid"]["volume_mm3"], expected_volume, places=3)
        self.assertEqual(value["source_solid"]["solid_count"], 1)
        self.assertEqual(value["reimported_solid"]["solid_count"], 1)
        self.assertTrue(value["equivalence"]["all_match"])
        self.assertTrue(value["material_witness"]["all_match"])

    def test_body_export_reimport_valid_single_solid_bbox_volume(self):
        value = result(empty_spec())
        self.assertEqual(value["format"], "step")
        self.assertEqual(value["artifact"]["extension"], ".step")
        self.assertGreater(value["artifact"]["size_bytes"], 0)
        self.assertEqual(len(value["artifact"]["sha256"]), 64)
        self.assertTrue(value["source_solid"]["valid"])
        self.assertTrue(value["reimported_solid"]["valid"])
        self.assertEqual(value["source_solid"]["solid_count"], 1)
        self.assertEqual(value["reimported_solid"]["solid_count"], 1)
        self.assertTrue(value["equivalence"]["bbox_match"])
        self.assertTrue(value["equivalence"]["volume_match"])

    def test_through_blind_base_and_blind_opposite_material_witnesses(self):
        through = empty_spec()
        add_cut(through, 0, "through", (20, 30), radius=5)
        value = result(through)
        self.assertTrue(value["equivalence"]["material_witness_match"])
        self.assertEqual(value["source_solid"]["bbox"], value["reimported_solid"]["bbox"])
        self.assertLessEqual(abs(value["source_solid"]["volume_mm3"] - value["reimported_solid"]["volume_mm3"]), 1e-5)

        for interval, expected_low, expected_high in (((0, 4), False, True), ((6, 10), True, False)):
            spec = empty_spec()
            add_cut(spec, 0, "blind", (0, 0), radius=5, z=interval)
            value = result(spec)
            labels = {item["label"]: item for item in value["material_witness"]["probes"]}
            self.assertEqual(labels["operation_0_low"]["source_material"], expected_low)
            self.assertEqual(labels["operation_0_high"]["source_material"], expected_high)
            self.assertTrue(value["equivalence"]["all_match"])

    def test_nonzero_origin_negative_coordinates_and_hole_xy_roundtrip(self):
        spec = empty_spec()
        spec["part"]["body"]["footprint"]["origin"] = {"x": -20.0, "y": 30.0}
        spec["part"]["bounds"] = {
            "min_x": -20.0, "max_x": 100.0, "min_y": 30.0, "max_y": 110.0,
            "min_z": 0.0, "max_z": 10.0,
        }
        add_cut(spec, 0, "global-hole", (20, 60), radius=5)
        value = result(spec)
        self.assertEqual(value["source_solid"]["bbox"], value["reimported_solid"]["bbox"])
        self.assertEqual(value["source_solid"]["bbox"]["x_min"], -20.0)
        self.assertEqual(value["source_solid"]["bbox"]["y_min"], 30.0)
        self.assertTrue(value["equivalence"]["all_match"])

    def test_two_five_and_eight_hole_patterns_roundtrip(self):
        two = empty_spec()
        add_cut(two, 0, "two:0", (-20, 0), radius=4)
        add_cut(two, 1, "two:1", (20, 0), radius=4)
        self.assertTrue(result(two)["equivalence"]["all_match"])

        five = result(five_hole_spec())
        self.assertEqual(five["source_solid"]["solid_count"], 1)
        self.assertTrue(five["equivalence"]["all_match"])

        eight = empty_spec()
        for index, center in enumerate(((-45, -25), (45, -25), (-45, 25), (45, 25), (-30, 0), (30, 0), (0, -15), (0, 15))):
            add_cut(eight, index, f"eight:{index}", center, radius=3)
        value = result(eight)
        self.assertEqual(value["material_witness"]["probe_count"], 4 + 8 * 3)
        self.assertTrue(value["equivalence"]["all_match"])

    def test_width_thickness_derived_depth_and_pattern_closure(self):
        width = empty_spec()
        width["part"]["body"]["footprint"]["width"] = 160.0
        width["part"]["bounds"]["max_x"] = 100.0
        self.assertEqual(result(width)["source_solid"]["bbox"]["x_max"], 100.0)

        thick = empty_spec()
        thick["part"]["body"]["z"]["max"] = 12.0
        thick["part"]["bounds"]["max_z"] = 12.0
        add_cut(thick, 0, "through-12", (0, 0), radius=5, z=(0, 12))
        self.assertEqual(result(thick)["source_solid"]["bbox"]["z_max"], 12.0)

        derived = empty_spec()
        add_cut(derived, 0, "derived-depth", (20, 30), radius=5, z=(2, 8))
        self.assertTrue(result(derived)["equivalence"]["all_match"])

    def test_first_and_third_angle_are_entity_independent(self):
        first = result(empty_spec())
        third = result(empty_spec())
        self.assertEqual(first["source_solid"], third["source_solid"])
        self.assertEqual(first["reimported_solid"], third["reimported_solid"])
        self.assertNotEqual(first["artifact"]["size_bytes"], 0)

    def test_outside_overlap_duplicate_and_partial_cuts_roundtrip(self):
        outside = empty_spec()
        add_cut(outside, 0, "outside", (1000, 1000), radius=5)
        value = result(outside)
        self.assertEqual(value["source_solid"]["volume_mm3"], 96000.0)

        overlap = empty_spec()
        add_cut(overlap, 0, "overlap:0", (0, 0), radius=5)
        add_cut(overlap, 1, "overlap:1", (2, 0), radius=5)
        self.assertTrue(result(overlap)["equivalence"]["all_match"])

        partial = empty_spec()
        add_cut(partial, 0, "partial", (-60, 0), radius=5)
        self.assertTrue(result(partial)["equivalence"]["all_match"])

    def test_mutation_fake_reimport_and_witness_mismatch_fail(self):
        source = {}

        def capture_export(shape, path):
            source["shape"] = shape
            Path(path).write_bytes(b"fake-but-nonempty")

        fake = build_and_export_step(
            empty_spec(),
            _exporter=capture_export,
            _importer=lambda _: source["shape"],
        )
        self.assertFalse(fake["ok"])
        self.assertEqual(fake["error"]["code"], "CAD-STEP-003")

        mismatch = build_and_export_step(
            empty_spec(),
            _exporter=lambda _shape, path: Path(path).write_bytes(b"fake"),
            _importer=lambda _path: cq.Solid.makeBox(121, 80, 10, cq.Vector(-60, -40, 0)),
        )
        self.assertFalse(mismatch["ok"])
        self.assertEqual(mismatch["error"]["code"], "CAD-STEP-004")

        source = empty_spec()
        add_cut(source, 0, "source-hole", (20, 30), radius=5)
        alternate = cq.Solid.makeBox(120, 80, 10, cq.Vector(-60, -40, 0)).cut(
            cq.Solid.makeCylinder(5, 10, cq.Vector(-20, -30, 0), cq.Vector(0, 0, 1))
        ).clean()
        witness_mismatch = build_and_export_step(
            source,
            _exporter=lambda _shape, path: Path(path).write_bytes(b"fake"),
            _importer=lambda _path: alternate,
        )
        self.assertFalse(witness_mismatch["ok"])
        self.assertEqual(witness_mismatch["error"]["code"], "CAD-STEP-004")

        volume_source = empty_spec()
        lower_volume = cq.Solid.makeBox(120, 80, 10, cq.Vector(-60, -40, 0)).cut(
            cq.Solid.makeCylinder(2, 10, cq.Vector(0, 0, 0), cq.Vector(0, 0, 1))
        ).clean()
        volume_mismatch = build_and_export_step(
            volume_source,
            _exporter=lambda _shape, path: Path(path).write_bytes(b"fake"),
            _importer=lambda _path: lower_volume,
        )
        self.assertFalse(volume_mismatch["ok"])
        self.assertEqual(volume_mismatch["error"]["code"], "CAD-STEP-004")

    def test_export_import_empty_and_exception_errors_are_structured(self):
        for exporter, importer, code in (
            (lambda _shape, path: Path(path).write_bytes(b""), None, "CAD-STEP-002"),
            (lambda _shape, _path: (_ for _ in ()).throw(RuntimeError("export failed")), None, "CAD-STEP-002"),
            (lambda _shape, path: Path(path).write_bytes(b"not-a-step"), lambda _path: (_ for _ in ()).throw(RuntimeError("import failed")), "CAD-STEP-003"),
        ):
            value = build_and_export_step(empty_spec(), _exporter=exporter, _importer=importer)
            self.assertFalse(value["ok"])
            self.assertEqual(value["error"]["stage"], "step_delivery")
            self.assertEqual(value["error"]["code"], code)

    def test_cleanup_runs_on_success_and_failure(self):
        success_tracker = TrackingTemp()
        self.assertTrue(build_and_export_step(empty_spec(), _temp_factory=success_tracker)["ok"])
        self.assertTrue(success_tracker.exited)

        failure_tracker = TrackingTemp()
        value = build_and_export_step(
            empty_spec(),
            _temp_factory=failure_tracker,
            _exporter=lambda _shape, _path: (_ for _ in ()).throw(RuntimeError("boom")),
        )
        self.assertFalse(value["ok"])
        self.assertTrue(failure_tracker.exited)

    def test_semantic_determinism_100_runs_and_no_path_is_delivered(self):
        values = [result(empty_spec()) for _ in range(100)]
        semantic = [
            (value["source_solid"], value["reimported_solid"], value["equivalence"], value["material_witness"])
            for value in values
        ]
        self.assertEqual(len({json.dumps(value, sort_keys=True) for value in semantic}), 1)
        self.assertTrue(all("path" not in value["artifact"] for value in values))

    def test_same_bbox_and_volume_different_contours_fail_centroid_witness(self):
        base = cq.Solid.makeBox(100, 100, 10, cq.Vector(0, 0, 0))
        right_notch = cq.Solid.makeBox(40, 40, 10, cq.Vector(60, 60, 0))
        left_notch = cq.Solid.makeBox(40, 40, 10, cq.Vector(0, 60, 0))
        right_l = base.cut(right_notch).clean()
        left_l = base.cut(left_notch).clean()

        source = _solid_witness(right_l)
        imported = _solid_witness(left_l)
        equivalence = _equivalence(source, imported, [], [])

        self.assertTrue(equivalence["bbox_match"])
        self.assertTrue(equivalence["volume_match"])
        self.assertTrue(equivalence["surface_area_match"])
        self.assertTrue(equivalence["topology_match"])
        self.assertFalse(equivalence["center_of_mass_match"])
        self.assertFalse(equivalence["all_match"])

    def test_static_boundary_has_no_reverse_engineering_or_ui_path(self):
        source = (Path(__file__).parents[1] / "aicad_cad" / "step_delivery.py").read_text(encoding="utf-8")
        for forbidden in (
            "EngineeringModel", "GeometryModel", "SpatialModel", "step_to_engineering_model",
            "FeatureGraph", "CadDocument", "shell=True", "eval(", "exec(", "user_output_path",
        ):
            self.assertNotIn(forbidden, source)
        self.assertIn("cq.importers.importStep", source)
        self.assertIn("cq.exporters.export", source)
        self.assertIn("STEP_EQ_TOLERANCE_MM", source)
        self.assertEqual(json.loads(GOLDEN.read_text(encoding="utf-8"))["schema_version"], 1)


if __name__ == "__main__":
    unittest.main()
