import tempfile
import unittest
from pathlib import Path

import cadquery as cq

from aicad_cad.feature_graph import build_feature_graph
from aicad_cad.worker import build_document


def manufacturing():
    return {
        "fits": [], "tolerances": [], "datums": [],
        "geometric_controls": [], "surface_finishes": [], "threads": [],
    }


def flange_document():
    return {
        "version": "1.0",
        "kind": "feature_graph_v1",
        "name": "six-hole-flange",
        "units": "mm",
        "parameters": {
            "outer_diameter": {"value": 160, "source": "user"},
            "thickness": {"value": 18, "source": "user"},
            "bore_diameter": {"value": 60, "source": "user"},
            "mounting_hole_diameter": {"value": 14, "source": "user"},
            "mounting_pcd": {"value": 120, "source": "user"},
            "through_cut_length": {
                "value": 20,
                "source": "derived",
                "expression": {"op": "add", "args": [{"parameter": "thickness"}, 2]},
            },
        },
        "features": [
            {
                "id": "body", "type": "cylinder", "operation": "add",
                "diameter": {"parameter": "outer_diameter"},
                "length": {"parameter": "thickness"}, "axis": "z",
                "origin": {"x": 0, "y": 0, "z": 0},
            },
            {
                "id": "center_bore", "type": "cylinder", "operation": "cut",
                "diameter": {"parameter": "bore_diameter"},
                "length": {"parameter": "through_cut_length"}, "axis": "z",
                "origin": {"x": 0, "y": 0, "z": -1},
            },
            {
                "id": "mounting_pattern", "type": "circular_hole_pattern", "operation": "cut",
                "diameter": {"parameter": "mounting_hole_diameter"},
                "length": {"parameter": "through_cut_length"},
                "count": 6, "pitch_diameter": {"parameter": "mounting_pcd"},
                "start_angle": 0, "axis": "z", "origin": {"x": 0, "y": 0, "z": -1},
            },
        ],
        "requirements": [
            {"id": "req_body", "text": "Ø160 x 18 body", "disposition": "geometry", "feature_ids": ["body"]},
            {"id": "req_holes", "text": "Ø60 bore and 6 holes", "disposition": "geometry", "feature_ids": ["center_bore", "mounting_pattern"]},
            {"id": "req_cut_length", "text": "Cuts pass through the body", "disposition": "derived", "parameter_ids": ["through_cut_length"]},
        ],
        "manufacturing": {
            **manufacturing(),
            "fits": [{
                "id": "fit_center_bore", "feature_id": "center_bore",
                "nominal_diameter": 60, "designation": "H7",
            }],
        },
        "assumptions": [],
        "unresolved": [],
        "source_hash": "flange-source",
    }


def bracket_document():
    return {
        "version": "1.0",
        "kind": "feature_graph_v1",
        "name": "mounting-bracket",
        "units": "mm",
        "parameters": {
            "width": {"value": 80, "source": "user"},
            "depth": {"value": 50, "source": "user"},
            "base_thickness": {"value": 10, "source": "user"},
            "upright_height": {"value": 60, "source": "user"},
            "hole_diameter": {"value": 12, "source": "user"},
        },
        "features": [
            {
                "id": "base", "type": "box", "operation": "add",
                "size": {"x": {"parameter": "width"}, "y": {"parameter": "depth"}, "z": {"parameter": "base_thickness"}},
                "origin": {"x": -40, "y": -25, "z": 0},
            },
            {
                "id": "upright", "type": "box", "operation": "add",
                "size": {"x": {"parameter": "width"}, "y": 10, "z": {"parameter": "upright_height"}},
                "origin": {"x": -40, "y": -5, "z": 5},
            },
            {
                "id": "mounting_hole", "type": "cylinder", "operation": "cut",
                "diameter": {"parameter": "hole_diameter"}, "length": 12, "axis": "y",
                "origin": {"x": 0, "y": -6, "z": 42},
            },
        ],
        "requirements": [
            {"id": "req_base", "text": "80 x 50 base", "disposition": "geometry", "feature_ids": ["base"]},
            {"id": "req_upright", "text": "60 high upright with Ø12 hole", "disposition": "geometry", "feature_ids": ["upright", "mounting_hole"]},
        ],
        "manufacturing": manufacturing(),
        "assumptions": [],
        "unresolved": [],
        "source_hash": "bracket-source",
    }


class FeatureGraphCadTests(unittest.TestCase):
    def test_same_kernel_builds_flange_and_bracket_as_valid_single_solids(self):
        flange = build_feature_graph(flange_document())
        bracket = build_feature_graph(bracket_document())

        flange_bounds = flange.shape.val().BoundingBox()
        bracket_bounds = bracket.shape.val().BoundingBox()
        self.assertTrue(flange.shape.val().isValid())
        self.assertEqual(len(flange.shape.val().Solids()), 1)
        self.assertAlmostEqual(flange_bounds.xlen, 160, places=6)
        self.assertAlmostEqual(flange_bounds.ylen, 160, places=6)
        self.assertAlmostEqual(flange_bounds.zlen, 18, places=6)
        self.assertEqual(flange.feature_ids, ("body", "center_bore", "mounting_pattern"))

        self.assertTrue(bracket.shape.val().isValid())
        self.assertEqual(len(bracket.shape.val().Solids()), 1)
        self.assertAlmostEqual(bracket_bounds.xlen, 80, places=6)
        self.assertAlmostEqual(bracket_bounds.ylen, 50, places=6)
        self.assertAlmostEqual(bracket_bounds.zlen, 65, places=6)
        self.assertEqual(bracket.feature_ids, ("base", "upright", "mounting_hole"))

    def test_rejects_no_op_cut_and_disconnected_addition(self):
        no_op = flange_document()
        no_op["features"][1]["origin"] = {"x": 1000, "y": 1000, "z": -1}
        with self.assertRaisesRegex(ValueError, "did not change the solid"):
            build_feature_graph(no_op)

        disconnected = bracket_document()
        disconnected["features"][1]["origin"] = {"x": 1000, "y": 1000, "z": 5}
        with self.assertRaisesRegex(ValueError, "disconnected"):
            build_feature_graph(disconnected)

    def test_generic_document_exports_non_step_derivatives_and_auditable_coverage(self):
        with tempfile.TemporaryDirectory(prefix="aicad-feature-graph-") as directory:
            manifest = build_document(flange_document(), Path(directory))
            self.assertEqual(manifest["document_kind"], "feature_graph_v1")
            self.assertEqual(manifest["coverage"]["status"], "complete")
            self.assertEqual(manifest["coverage"]["geometry"], 2)
            self.assertEqual(manifest["coverage"]["derived"], 1)
            self.assertEqual(manifest["parameter_evidence"]["derived"], 1)
            self.assertEqual(manifest["parameter_evidence"]["total"], 6)
            self.assertEqual(manifest["manufacturing_evidence"]["ids"], ["fit_center_bore"])
            self.assertEqual(
                [record["id"] for record in manifest["feature_records"]],
                ["body", "center_bore", "mounting_pattern"],
            )
            self.assertEqual(set(manifest["artifacts"]), {"source", "brep", "stl", "glb", "svg"})
            self.assertNotIn("step", manifest["artifacts"])


if __name__ == "__main__":
    unittest.main()
