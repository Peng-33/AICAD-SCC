import json
import tempfile
import unittest
from pathlib import Path

import cadquery as cq

from aicad_cad.gearbox import build_gearbox
from aicad_cad.worker import build_document


def gearbox_spec():
    return {
        "version": "1.0",
        "kind": "precision_gearbox_housing_v1",
        "units": "mm",
        "parameters": {
            "envelope": {
                "base_length": 260,
                "base_width": 190,
                "total_height": 165,
                "base_thickness": 18,
                "body_length": 220,
                "body_width": 150,
                "body_height": 147,
            },
            "shell": {
                "wall_thickness": 10,
                "top_thickness": 12,
                "bottom_reinforcement": 16,
                "corner_radius": 18,
                "body_radius": 8,
                "inner_length": 190,
                "inner_width": 120,
                "inner_height": 118,
            },
            "axes": {
                "main": {
                    "x": 0,
                    "z": 92,
                    "through_diameter": 72,
                    "front_bearing_diameter": 110,
                    "front_bearing_depth": 24,
                    "front_step_diameter": 82,
                    "front_step_depth": 14,
                    "rear_bearing_diameter": 105,
                    "rear_bearing_depth": 22,
                    "rear_step_diameter": 80,
                    "rear_step_depth": 12,
                },
                "second": {
                    "x": 78,
                    "z": 92,
                    "through_diameter": 48,
                    "center_distance": 78,
                    "front_bearing_diameter": 78,
                    "front_bearing_depth": 20,
                    "rear_bearing_diameter": 75,
                    "rear_bearing_depth": 18,
                },
                "third": {
                    "x": 43.68065933568311,
                    "z": 48,
                    "through_diameter": 35,
                    "center_distance": 62,
                    "horizontal_side": "right",
                    "front_bearing_diameter": 62,
                    "front_bearing_depth": 18,
                    "rear_bearing_diameter": 60,
                    "rear_bearing_depth": 16,
                },
            },
            "mounting": {
                "ear_diameter": 42,
                "hole_diameter": 14,
                "counterbore_diameter": 24,
                "counterbore_depth": 6,
                "edge_offset_x": 25,
                "edge_offset_y": 25,
            },
            "top_opening": {
                "length": 142, "width": 88, "corner_radius": 10,
                "seal_groove_width": 4.5, "seal_groove_depth": 2.6, "seal_groove_offset": 10,
            },
            "side_openings": {
                "right_round": {"x_from_front": 118, "z": 72, "diameter": 58, "recess_diameter": 76, "recess_depth": 5},
                "left_rectangular": {"x_from_front": 125, "z": 76, "length": 82, "height": 56, "corner_radius": 8},
            },
            "internal_cavities": {
                "main": {"diameter": 142, "width": 62},
                "second": {"diameter": 108, "width": 48},
                "third": {"diameter": 88, "width": 42},
            },
            "locating": {
                "dowel_hole_diameter": 10, "dowel_hole_depth": 14, "dowel_spacing": 180,
                "dowel_front_offset": 22, "keyway_width": 14, "keyway_length": 50,
                "keyway_depth": 5, "keyway_spacing": 120,
            },
        },
        "manufacturing": {
            "fits": [],
            "tolerances": [],
            "datums": [],
            "geometric_controls": [],
            "surface_finishes": [],
            "threads": [],
        },
        "assumptions": [],
        "unresolved": [],
        "source_hash": "test-source-hash",
    }


class GearboxCadTests(unittest.TestCase):
    def test_builds_a_valid_260_by_190_by_165_brep(self):
        result = build_gearbox(gearbox_spec())
        shape = result.shape.val()
        bounds = shape.BoundingBox()

        self.assertTrue(shape.isValid())
        self.assertGreater(shape.Volume(), 0)
        self.assertAlmostEqual(bounds.xlen, 260, places=6)
        self.assertAlmostEqual(bounds.ylen, 190, places=6)
        self.assertAlmostEqual(bounds.zlen, 165, places=6)
        self.assertIn("axis.main.through_bore", result.feature_ids)
        self.assertIn("shell.internal_cavity", result.feature_ids)
        self.assertIn("top.seal_groove", result.feature_ids)
        self.assertIn("top.ten_cover_tap_holes", result.feature_ids)
        self.assertIn("axis.main.front_cover_pattern", result.feature_ids)
        self.assertIn("internal.main_clearance", result.feature_ids)
        self.assertIn("side.right_observation_opening", result.feature_ids)
        self.assertIn("locating.two_dowel_holes", result.feature_ids)
        self.assertIn("locating.two_keyways", result.feature_ids)

    def test_exports_non_step_compatibility_derivatives(self):
        with tempfile.TemporaryDirectory(prefix="aicad-cad-test-") as directory:
            manifest = build_document(gearbox_spec(), Path(directory))
            self.assertEqual(manifest["status"], "built")
            self.assertEqual(manifest["solid_valid"], True)
            self.assertEqual(
                manifest["bounding_box_mm"],
                {"x": 260.0, "y": 190.0, "z": 165.0},
            )
            self.assertEqual(
                set(manifest["artifacts"]),
                {"source", "brep", "stl", "glb", "svg"},
            )
            self.assertNotIn("step", manifest["artifacts"])
            for artifact in manifest["artifacts"].values():
                self.assertGreater(Path(directory, artifact["filename"]).stat().st_size, 0)


if __name__ == "__main__":
    unittest.main()
