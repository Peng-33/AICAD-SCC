import copy
import json
import math
import os
import subprocess
import sys
import unittest
from pathlib import Path

import cadquery as cq

from aicad_cad.solid_backend import _apply_cut, _build_body, build_solid_from_spec
from aicad_cad.solid_build_spec import SolidBuildSpecError, parse_solid_build_spec


ROOT = Path(__file__).resolve().parents[4]
GOLDEN = ROOT / "crates" / "scc-manufacturing" / "tests" / "goldens" / "installation_plate_solid_spec.json"


def golden_spec():
    return json.loads(GOLDEN.read_text(encoding="utf-8"))


def add_cut(spec, operation_id, source_id, center, radius=5.0, z=(0.0, 10.0)):
    spec["operations"][str(operation_id)] = {
        "type": "cylindrical_cut",
        "center": {"x": center[0], "y": center[1]},
        "radius": radius,
        "z": {"min": z[0], "max": z[1]},
        "axis": "positive_z",
    }
    spec["source_map"]["operations"][str(operation_id)] = source_id


def slot_profile_30deg():
    # Exact translated stadium contour only; downstream receives no slot dimensions.
    return [
        profile_line((-0.124355652982143, -6.803847577293368), (24.12435565298214, 7.196152422706632)),
        {
            "kind": "arc", "start": {"x": 24.12435565298214, "y": 7.196152422706632},
            "end": {"x": 30.12435565298214, "y": -3.196152422706632},
            "center": {"x": 27.12435565298214, "y": 2.0}, "radius": 6.0,
            "direction": "clockwise",
        },
        profile_line((30.12435565298214, -3.196152422706632), (5.875644347017857, -17.196152422706632)),
        {
            "kind": "arc", "start": {"x": 5.875644347017857, "y": -17.196152422706632},
            "end": {"x": -0.124355652982143, "y": -6.803847577293368},
            "center": {"x": 2.875644347017857, "y": -12.0}, "radius": 6.0,
            "direction": "clockwise",
        },
    ]


def add_slot(spec, operation_id, source_id, profile=None, z=(0.0, 10.0)):
    spec["operations"][str(operation_id)] = {
        "type": "slot_cut", "profile": {"segments": slot_profile_30deg() if profile is None else profile},
        "z": {"min": z[0], "max": z[1]}, "axis": "positive_z",
    }
    spec["source_map"]["operations"][str(operation_id)] = source_id


def empty_spec():
    spec = golden_spec()
    spec["operations"] = {}
    spec["source_map"]["operations"] = {}
    return spec


def circle_spec(center=(0.0, 0.0), radius=80.0, thickness=20.0):
    spec = empty_spec()
    cx, cy = center
    spec["part"]["body"]["footprint"] = {
        "kind": "circle",
        "center": {"x": cx, "y": cy},
        "radius": radius,
    }
    spec["part"]["body"]["z"] = {"min": 0.0, "max": thickness}
    spec["part"]["bounds"] = {
        "min_x": cx - radius,
        "max_x": cx + radius,
        "min_y": cy - radius,
        "max_y": cy + radius,
        "min_z": 0.0,
        "max_z": thickness,
    }
    return spec


def rounded_rectangle_spec(center=(0.0, 0.0), width=120.0, height=80.0, radius=10.0, thickness=8.0):
    spec = empty_spec()
    cx, cy = center
    spec["part"]["body"]["footprint"] = {
        "kind": "rounded_rectangle",
        "center": {"x": cx, "y": cy},
        "width": width,
        "height": height,
        "corner_radius": radius,
    }
    spec["part"]["body"]["z"] = {"min": 0.0, "max": thickness}
    spec["part"]["bounds"] = {
        "min_x": cx - width / 2.0,
        "max_x": cx + width / 2.0,
        "min_y": cy - height / 2.0,
        "max_y": cy + height / 2.0,
        "min_z": 0.0,
        "max_z": thickness,
    }
    return spec


def closed_profile_spec(segments, bounds=(0.0, 100.0, 0.0, 100.0), thickness=10.0):
    spec = empty_spec()
    spec["part"]["body"]["footprint"] = {"kind": "closed_profile", "segments": segments}
    spec["part"]["body"]["z"] = {"min": 0.0, "max": thickness}
    spec["part"]["bounds"] = {
        "min_x": bounds[0], "max_x": bounds[1],
        "min_y": bounds[2], "max_y": bounds[3],
        "min_z": 0.0, "max_z": thickness,
    }
    return spec


def profile_line(start, end):
    return {"kind": "line", "start": {"x": start[0], "y": start[1]}, "end": {"x": end[0], "y": end[1]}}


def concave_profile_spec():
    return closed_profile_spec([
        profile_line((0.0, 0.0), (100.0, 0.0)),
        profile_line((100.0, 0.0), (100.0, 60.0)),
        profile_line((100.0, 60.0), (40.0, 60.0)),
        profile_line((40.0, 60.0), (40.0, 100.0)),
        profile_line((40.0, 100.0), (0.0, 100.0)),
        profile_line((0.0, 100.0), (0.0, 0.0)),
    ])


def mixed_arc_profile_spec():
    return closed_profile_spec([
        profile_line((0.0, 0.0), (80.0, 0.0)),
        {
            "kind": "arc",
            "start": {"x": 80.0, "y": 0.0},
            "end": {"x": 100.0, "y": 20.0},
            "center": {"x": 80.0, "y": 20.0},
            "radius": 20.0,
            "direction": "counter_clockwise",
        },
        profile_line((100.0, 20.0), (100.0, 60.0)),
        profile_line((100.0, 60.0), (0.0, 60.0)),
        profile_line((0.0, 60.0), (0.0, 0.0)),
    ], bounds=(0.0, 100.0, 0.0, 60.0))


def shape_for(spec):
    decoded = parse_solid_build_spec(spec)
    shape = _build_body(decoded)
    for operation in decoded.operations:
        shape = _apply_cut(shape, operation)
    return shape


def material_at(shape, x, y, z):
    return shape.isInside(cq.Vector(x, y, z), 1e-7)


class SolidBackendTests(unittest.TestCase):
    def test_schema_v1_and_frozen_golden_are_accepted(self):
        spec = golden_spec()
        decoded = parse_solid_build_spec(spec)
        result = build_solid_from_spec(spec)
        self.assertEqual(spec["schema_version"], 1)
        self.assertEqual(len(decoded.operations), 4)
        self.assertTrue(result["ok"])

    def test_unknown_or_missing_schema_fails_closed(self):
        for value in (0, 2, 999, None):
            spec = golden_spec()
            if value is None:
                del spec["schema_version"]
            else:
                spec["schema_version"] = value
            result = build_solid_from_spec(spec)
            self.assertFalse(result["ok"])
            self.assertEqual(result["error"]["code"], "CAD-BACKEND-001")

    def test_missing_body_and_unknown_operation_fail_closed(self):
        missing = golden_spec()
        del missing["part"]["body"]
        self.assertEqual(build_solid_from_spec(missing)["error"]["code"], "CAD-BACKEND-004")

        unknown = golden_spec()
        unknown["operations"]["0"]["type"] = "pattern"
        result = build_solid_from_spec(unknown)
        self.assertEqual(result["error"]["code"], "CAD-BACKEND-002")
        self.assertEqual(result["error"]["operation_id"], "0")

    def test_body_bbox_volume_and_material_witness(self):
        spec = empty_spec()
        result = build_solid_from_spec(spec)
        self.assertEqual(result["bbox"], {
            "x_min": -60.0, "x_max": 60.0,
            "y_min": -40.0, "y_max": 40.0,
            "z_min": 0.0, "z_max": 10.0,
        })
        self.assertEqual(result["volume_mm3"], 96000.0)
        self.assertTrue(material_at(shape_for(spec), 0, 0, 5))

    def test_slot_cut_consumes_exact_profile_as_distinct_typed_operation(self):
        spec = empty_spec()
        add_slot(spec, 4, "slot:cut:0000")
        add_cut(spec, 5, "hole:cut:0000", (40, 0), radius=3.0)
        decoded = parse_solid_build_spec(spec)
        slot = decoded.operations[0]
        self.assertEqual(type(slot).__name__, "SlotCutSpec")
        self.assertEqual(len(slot.profile.segments), 4)
        self.assertFalse(hasattr(slot, "center"))
        self.assertFalse(hasattr(slot, "overall_length"))
        self.assertFalse(hasattr(slot, "width"))
        result = build_solid_from_spec(spec)
        self.assertTrue(result["ok"], result)
        self.assertEqual(result["operation_count"], 2)
        self.assertEqual([item["status"] for item in result["operation_results"]], ["applied", "applied"])
        slot_area = (40.0 - 12.0) * 12.0 + math.pi * 6.0**2
        expected = 96000.0 - slot_area * 10.0 - math.pi * 3.0**2 * 10.0
        self.assertAlmostEqual(result["volume_mm3"], expected, places=5)
        shape = shape_for(spec)
        self.assertFalse(material_at(shape, 15.0, -5.0, 5.0))
        self.assertTrue(material_at(shape, 50.0, 0.0, 5.0))

    def test_slot_profile_is_strictly_validated_without_repair(self):
        malformed = empty_spec()
        add_slot(malformed, 0, "slot:bad", profile=[])
        self.assertFalse(build_solid_from_spec(malformed)["ok"])

        disconnected = empty_spec()
        contour = slot_profile_30deg()
        contour[0]["end"]["x"] += 0.5
        add_slot(disconnected, 0, "slot:bad", profile=contour)
        self.assertIn("disconnected", build_solid_from_spec(disconnected)["error"]["message"])

        bad_interval = empty_spec()
        add_slot(bad_interval, 0, "slot:bad", z=(0.0, 9.0))
        self.assertIn("match", build_solid_from_spec(bad_interval)["error"]["message"])

    def test_through_hole_volume_and_witness(self):
        spec = empty_spec()
        add_cut(spec, 0, "cut:0", (20, 30), radius=5)
        result = build_solid_from_spec(spec)
        self.assertTrue(result["valid"])
        self.assertAlmostEqual(result["volume_mm3"], 96000 - math.pi * 25 * 10, places=5)
        shape = shape_for(spec)
        self.assertFalse(material_at(shape, 20, 30, 2))
        self.assertFalse(material_at(shape, 20, 30, 8))
        self.assertTrue(material_at(shape, 30, 30, 5))

    def test_blind_base_is_direct_interval_and_has_direction_witness(self):
        spec = empty_spec()
        add_cut(spec, 0, "blind-base", (0, 0), radius=5, z=(0, 4))
        result = build_solid_from_spec(spec)
        self.assertAlmostEqual(result["volume_mm3"], 96000 - math.pi * 25 * 4, places=5)
        shape = shape_for(spec)
        self.assertFalse(material_at(shape, 0, 0, 2))
        self.assertTrue(material_at(shape, 0, 0, 8))

    def test_blind_opposite_is_direct_interval_and_has_direction_witness(self):
        spec = empty_spec()
        add_cut(spec, 0, "blind-opposite", (0, 0), radius=5, z=(6, 10))
        result = build_solid_from_spec(spec)
        self.assertAlmostEqual(result["volume_mm3"], 96000 - math.pi * 25 * 4, places=5)
        shape = shape_for(spec)
        self.assertTrue(material_at(shape, 0, 0, 2))
        self.assertFalse(material_at(shape, 0, 0, 8))

    def test_nonzero_origin_negative_coordinates_and_global_hole_position(self):
        spec = empty_spec()
        body = spec["part"]["body"]
        body["footprint"]["origin"] = {"x": -20.0, "y": 30.0}
        spec["part"]["bounds"] = {
            "min_x": -20.0, "max_x": 100.0,
            "min_y": 30.0, "max_y": 110.0,
            "min_z": 0.0, "max_z": 10.0,
        }
        add_cut(spec, 0, "global-hole", (20, 60), radius=5)
        result = build_solid_from_spec(spec)
        self.assertEqual(result["bbox"]["x_min"], -20.0)
        self.assertEqual(result["bbox"]["x_max"], 100.0)
        self.assertEqual(result["bbox"]["y_min"], 30.0)
        self.assertEqual(result["bbox"]["y_max"], 110.0)
        self.assertFalse(material_at(shape_for(spec), 20, 60, 5))

    def test_two_cuts_eight_cuts_and_same_feature_provenance_are_preserved(self):
        two = empty_spec()
        add_cut(two, 10, "same-feature:0", (-20, 0), radius=4)
        add_cut(two, 11, "same-feature:1", (20, 0), radius=4)
        result = build_solid_from_spec(two)
        self.assertEqual(result["operation_count"], 2)
        self.assertEqual([item["operation_id"] for item in result["operation_results"]], ["10", "11"])
        self.assertEqual([item["source_id"] for item in result["operation_results"]], ["same-feature:0", "same-feature:1"])

        eight = empty_spec()
        for index, center in enumerate(((-45, -25), (45, -25), (-45, 25), (45, 25), (-30, 0), (30, 0), (0, -15), (0, 15))):
            add_cut(eight, index, f"expanded-cut:{index}", center, radius=3)
        result = build_solid_from_spec(eight)
        self.assertEqual(result["operation_count"], 8)
        self.assertTrue(result["valid"])
        shape = shape_for(eight)
        for center in ((-45, -25), (45, -25), (-45, 25), (45, 25), (-30, 0), (30, 0), (0, -15), (0, 15)):
            self.assertFalse(material_at(shape, center[0], center[1], 5))

    def test_width_and_thickness_propagate_from_spec_without_reinterpretation(self):
        width = empty_spec()
        width["part"]["body"]["footprint"]["width"] = 160.0
        width["part"]["bounds"]["max_x"] = 100.0
        width_result = build_solid_from_spec(width)
        self.assertEqual(width_result["bbox"]["x_max"] - width_result["bbox"]["x_min"], 160.0)

        thick = empty_spec()
        thick["part"]["body"]["z"]["max"] = 12.0
        thick["part"]["bounds"]["max_z"] = 12.0
        add_cut(thick, 0, "through", (0, 0), radius=5, z=(0, 12))
        thick_result = build_solid_from_spec(thick)
        self.assertEqual(thick_result["bbox"]["z_max"], 12.0)
        self.assertFalse(material_at(shape_for(thick), 0, 0, 2))
        self.assertFalse(material_at(shape_for(thick), 0, 0, 10))

    def test_overlap_outside_partial_and_duplicate_geometry_operations(self):
        overlap = empty_spec()
        add_cut(overlap, 0, "overlap:0", (0, 0), radius=5)
        add_cut(overlap, 1, "overlap:1", (2, 0), radius=5)
        result = build_solid_from_spec(overlap)
        self.assertTrue(result["valid"])
        self.assertEqual(result["operation_count"], 2)
        self.assertLess(result["volume_mm3"], 96000 - math.pi * 25 * 10)

        outside = empty_spec()
        add_cut(outside, 0, "outside", (1000, 1000), radius=5)
        result = build_solid_from_spec(outside)
        self.assertEqual(result["operation_results"][0]["status"], "no_material_change")
        self.assertEqual(result["volume_mm3"], 96000.0)

        duplicate = empty_spec()
        add_cut(duplicate, 0, "duplicate:0", (0, 0), radius=5)
        add_cut(duplicate, 1, "duplicate:1", (0, 0), radius=5)
        result = build_solid_from_spec(duplicate)
        self.assertEqual([item["status"] for item in result["operation_results"]], ["applied", "no_material_change"])

    def test_numeric_and_source_mutations_fail_closed(self):
        for field, value in (("radius", float("nan")), ("radius", float("inf")), ("radius", 0)):
            spec = empty_spec()
            add_cut(spec, 0, "bad", (0, 0))
            spec["operations"]["0"][field] = value
            result = build_solid_from_spec(spec)
            self.assertFalse(result["ok"])
            self.assertEqual(result["error"]["code"], "CAD-BACKEND-004")

        duplicate_source = empty_spec()
        add_cut(duplicate_source, 0, "same", (0, 0))
        add_cut(duplicate_source, 1, "same", (10, 0))
        result = build_solid_from_spec(duplicate_source)
        self.assertEqual(result["error"]["code"], "CAD-BACKEND-005")

    def test_circle_exact_extrusion_preserves_translation_and_volume(self):
        spec = circle_spec()
        decoded = parse_solid_build_spec(spec)
        self.assertEqual((decoded.body.center.x, decoded.body.center.y, decoded.body.radius), (0.0, 0.0, 80.0))
        result = build_solid_from_spec(spec)
        self.assertTrue(result["ok"])
        self.assertEqual(result["bbox"], {
            "x_min": -80.0, "x_max": 80.0,
            "y_min": -80.0, "y_max": 80.0,
            "z_min": 0.0, "z_max": 20.0,
        })
        self.assertAlmostEqual(result["volume_mm3"], math.pi * 80.0 * 80.0 * 20.0, places=3)
        self.assertTrue(material_at(shape_for(spec), 0.0, 0.0, 10.0))

        translated = circle_spec((30.0, -20.0))
        translated_result = build_solid_from_spec(translated)
        self.assertEqual(translated_result["bbox"], {
            "x_min": -50.0, "x_max": 110.0,
            "y_min": -100.0, "y_max": 60.0,
            "z_min": 0.0, "z_max": 20.0,
        })
        self.assertTrue(material_at(shape_for(translated), 30.0, -20.0, 10.0))

    def test_closed_profile_preserves_concavity_and_mixed_arc_exactly(self):
        concave = concave_profile_spec()
        decoded = parse_solid_build_spec(concave)
        self.assertEqual(decoded.body.segments[0].start, decoded.body.segments[-1].end)
        result = build_solid_from_spec(concave)
        self.assertTrue(result["ok"], result)
        self.assertEqual(result["bbox"], {
            "x_min": 0.0, "x_max": 100.0,
            "y_min": 0.0, "y_max": 100.0,
            "z_min": 0.0, "z_max": 10.0,
        })
        self.assertAlmostEqual(result["volume_mm3"], 76000.0, places=5)
        shape = shape_for(concave)
        self.assertFalse(material_at(shape, 70.0, 80.0, 5.0))
        self.assertTrue(material_at(shape, 20.0, 80.0, 5.0))

        mixed = mixed_arc_profile_spec()
        mixed_result = build_solid_from_spec(mixed)
        self.assertTrue(mixed_result["ok"], mixed_result)
        self.assertEqual(mixed_result["bbox"]["x_max"], 100.0)
        self.assertEqual(mixed_result["bbox"]["y_max"], 60.0)
        self.assertTrue(material_at(shape_for(mixed), 50.0, 30.0, 5.0))
        self.assertFalse(material_at(shape_for(mixed), 90.0, 1.0, 5.0))
        self.assertEqual(parse_solid_build_spec(mixed).body.segments[1].direction, "counter_clockwise")

    def test_closed_profile_rejects_bbox_mutation_and_unsupported_public_shapes(self):
        invalid_bounds = concave_profile_spec()
        invalid_bounds["part"]["bounds"]["max_x"] = 101.0
        result = build_solid_from_spec(invalid_bounds)
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "CAD-BACKEND-003")

        for kind in ("polygon", "composite", "slot"):
            unsupported = empty_spec()
            unsupported["part"]["body"]["footprint"] = {"kind": kind}
            result = build_solid_from_spec(unsupported)
            self.assertFalse(result["ok"])
            self.assertEqual(result["error"]["code"], "CAD-BACKEND-002")

    def test_rounded_rectangle_exact_profile_translation_and_volume(self):
        spec = rounded_rectangle_spec()
        decoded = parse_solid_build_spec(spec)
        self.assertEqual((decoded.body.center.x, decoded.body.center.y), (0.0, 0.0))
        self.assertEqual((decoded.body.width, decoded.body.height, decoded.body.corner_radius), (120.0, 80.0, 10.0))
        result = build_solid_from_spec(spec)
        self.assertTrue(result["ok"])
        expected_volume = (120.0 * 80.0 - (4.0 - math.pi) * 10.0 * 10.0) * 8.0
        self.assertAlmostEqual(result["volume_mm3"], expected_volume, places=3)
        shape = shape_for(spec)
        self.assertTrue(material_at(shape, 0.0, 0.0, 4.0))
        self.assertTrue(material_at(shape, 55.0, 35.0, 4.0))
        self.assertFalse(material_at(shape, 59.0, 39.0, 4.0))

        translated = rounded_rectangle_spec((25.0, -15.0))
        translated_result = build_solid_from_spec(translated)
        self.assertEqual(translated_result["bbox"], {
            "x_min": -35.0, "x_max": 85.0,
            "y_min": -55.0, "y_max": 25.0,
            "z_min": 0.0, "z_max": 8.0,
        })
        self.assertTrue(material_at(shape_for(translated), 25.0, -15.0, 4.0))

        asymmetric = rounded_rectangle_spec(width=140.0, height=60.0, radius=12.0, thickness=15.0)
        asymmetric_result = build_solid_from_spec(asymmetric)
        self.assertEqual(asymmetric_result["bbox"]["x_max"] - asymmetric_result["bbox"]["x_min"], 140.0)
        self.assertEqual(asymmetric_result["bbox"]["y_max"] - asymmetric_result["bbox"]["y_min"], 60.0)
        self.assertEqual(asymmetric_result["bbox"]["z_max"], 15.0)

    def test_rounded_rectangle_reuses_cylindrical_holes_and_is_deterministic(self):
        spec = rounded_rectangle_spec()
        add_cut(spec, 0, "center-through", (0.0, 0.0), radius=6.0, z=(0.0, 8.0))
        result = build_solid_from_spec(spec)
        self.assertTrue(result["ok"])
        expected_volume = ((120.0 * 80.0 - (4.0 - math.pi) * 10.0 * 10.0) - math.pi * 6.0 * 6.0) * 8.0
        self.assertAlmostEqual(result["volume_mm3"], expected_volume, places=3)
        self.assertFalse(material_at(shape_for(spec), 0.0, 0.0, 4.0))
        self.assertTrue(material_at(shape_for(spec), 40.0, 0.0, 4.0))
        encoded = [json.dumps(build_solid_from_spec(spec), sort_keys=True, separators=(",", ":")) for _ in range(20)]
        self.assertEqual(len(set(encoded)), 1)

    def test_rounded_rectangle_invalid_geometry_fails_closed(self):
        for field, value in (("width", 0.0), ("height", -1.0), ("corner_radius", 0.0), ("corner_radius", 41.0), ("corner_radius", float("nan")), ("corner_radius", float("inf"))):
            spec = rounded_rectangle_spec()
            spec["part"]["body"]["footprint"][field] = value
            result = build_solid_from_spec(spec)
            self.assertFalse(result["ok"], (field, value, result))
            self.assertEqual(result["error"]["code"], "CAD-BACKEND-004")

    def test_circle_invalid_radius_and_bounds_fail_closed(self):
        for radius in (0.0, -1.0, float("nan"), float("inf")):
            spec = circle_spec(radius=radius)
            result = build_solid_from_spec(spec)
            self.assertFalse(result["ok"])
            self.assertEqual(result["error"]["code"], "CAD-BACKEND-004")

        invalid_bounds = circle_spec()
        invalid_bounds["part"]["bounds"]["max_x"] = 81.0
        result = build_solid_from_spec(invalid_bounds)
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "CAD-BACKEND-003")

    def test_circle_cut_reuse_and_determinism(self):
        spec = circle_spec((30.0, -20.0))
        add_cut(spec, 0, "center-through", (30.0, -20.0), radius=30.0, z=(0.0, 20.0))
        result = build_solid_from_spec(spec)
        self.assertTrue(result["ok"])
        self.assertAlmostEqual(
            result["volume_mm3"],
            math.pi * (80.0 * 80.0 - 30.0 * 30.0) * 20.0,
            places=3,
        )
        self.assertFalse(material_at(shape_for(spec), 30.0, -20.0, 10.0))
        encoded = [json.dumps(build_solid_from_spec(spec), sort_keys=True, separators=(",", ":")) for _ in range(20)]
        self.assertEqual(len(set(encoded)), 1)

    def test_result_is_deterministic_and_worker_stdout_is_json_only(self):
        spec = golden_spec()
        results = [json.dumps(build_solid_from_spec(spec), sort_keys=True, separators=(",", ":")) for _ in range(100)]
        self.assertEqual(len(set(results)), 1)
        environment = {**os.environ, "PYTHONPATH": str(ROOT / "apps" / "scc-ui" / "cad")}
        process = subprocess.run(
            [sys.executable, "-m", "aicad_cad.solid_backend"],
            input=json.dumps(spec), text=True, capture_output=True, env=environment, check=False,
        )
        self.assertEqual(process.returncode, 0)
        self.assertEqual(len(process.stdout.splitlines()), 1)
        self.assertEqual(process.stderr, "")
        self.assertTrue(json.loads(process.stdout)["ok"])

    def test_static_boundary_has_no_dynamic_dispatch_or_product_export(self):
        for module_name in ("solid_backend.py", "solid_build_spec.py"):
            source = (ROOT / "apps" / "scc-ui" / "cad" / "aicad_cad" / module_name).read_text(encoding="utf-8")
            for forbidden in (
                "EngineeringModel", "GeometryModel", "SpatialModel", "HolePattern",
                "eval(", "exec(", "getattr(", "exportStep", "STEP", "FeatureGraph", "CadDocument",
            ):
                self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
