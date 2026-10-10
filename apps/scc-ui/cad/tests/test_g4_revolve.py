"""G4 real CadQuery/OCP tests for generic ordered Revolve features."""

from __future__ import annotations

import math
import unittest

from aicad_cad.parametric_rebuild import rebuild_parametric_model

MODEL_HASH = "a" * 64
PLANE_XZ = {
    "origin": [0.0, 0.0, 0.0],
    "u_axis": [1.0, 0.0, 0.0],
    "v_axis": [0.0, 0.0, 1.0],
    "normal": [0.0, -1.0, 0.0],
}


def rectangle(x0: float, x1: float, r0: float, r1: float, prefix: str):
    points = [(x0, r0), (x1, r0), (x1, r1), (x0, r1)]
    curves, ids = [], []
    for index, (start, end) in enumerate(zip(points, points[1:] + points[:1])):
        curves.append({"kind": "line", "start": list(start), "end": list(end)})
        ids.append(f"{prefix}-{index}")
    return curves, ids


def revolve(feature_id="shaft", profile=None, *, axis_origin=(0, 0, 0), direction=(1, 0, 0),
            operation="add", angle=None, orientation="forward", plane=None):
    loop, ids = rectangle(0, 20, 0, 5, f"{feature_id}-edge") if profile is None else profile
    return {"kind": "revolve", "data": {
        "feature_id": feature_id, "sketch_id": f"{feature_id}-sketch", "operation": operation,
        "plane": plane or PLANE_XZ, "loops": [loop], "profile_entity_ids": [ids],
        "axis_origin": list(axis_origin), "direction": list(direction),
        "angle": angle or {"kind": "full_360"}, "orientation": orientation,
    }}


def request(*operations):
    return {"schema_version": 6, "model_sha256": MODEL_HASH, "operations": list(operations)}


def result_or_fail(value):
    response = rebuild_parametric_model(value)
    if not response["ok"]:
        raise AssertionError(response["error"])
    return response["result"]


def error_code(value):
    response = rebuild_parametric_model(value)
    if response["ok"]:
        raise AssertionError("expected a fail-closed error")
    return response["error"]["code"]


class GeneralRevolveTests(unittest.TestCase):
    def test_r1_full_360_cylinder_has_exact_volume_and_one_body(self):
        result = result_or_fail(request(revolve()))
        self.assertEqual(result["solid_count"], 1)
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], math.pi * 25 * 20, places=5)
        self.assertAlmostEqual(result["bbox"]["x_max"] - result["bbox"]["x_min"], 20, places=6)
        self.assertGreater(sum(face["surface"]["kind"] == "cylinder" for face in result["spatial_evidence"]["faces"]), 0)

    def test_r2_stepped_shaft_is_one_generic_revolve(self):
        points = [(0, 0), (80, 0), (80, 10), (60, 10), (60, 15), (20, 15), (20, 20), (0, 20)]
        curves = [{"kind": "line", "start": list(a), "end": list(b)} for a, b in zip(points, points[1:] + points[:1])]
        op = revolve("stepped", (curves, [f"step-{i}" for i in range(len(curves))]))
        result = result_or_fail(request(op))
        self.assertEqual(result["solid_count"], 1)
        self.assertAlmostEqual(result["bbox"]["x_max"] - result["bbox"]["x_min"], 80, places=6)
        self.assertAlmostEqual(result["bbox"]["z_max"], 20, places=6)
        self.assertEqual(result["spatial_evidence"]["operation_features"], ["stepped"])

    def test_r3_hollow_profile_with_inner_loop_revolves(self):
        outer, outer_ids = rectangle(0, 20, 5, 15, "outer")
        inner, inner_ids = rectangle(8, 12, 8, 12, "void")
        op = revolve("hollow", (outer, outer_ids))
        op["data"]["loops"].append(inner)
        op["data"]["profile_entity_ids"].append(inner_ids)
        result = result_or_fail(request(op))
        expected = math.pi * (15**2 - 5**2) * 20 - math.pi * (12**2 - 8**2) * 4
        self.assertEqual(result["solid_count"], 1)
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], expected, delta=1.0e-4)

    def test_conical_profile_keeps_exact_cone_and_planar_face_evidence(self):
        points = [(0, 0), (20, 0), (20, 5), (0, 10)]
        curves = [
            {"kind": "line", "start": list(start), "end": list(end)}
            for start, end in zip(points, points[1:] + points[:1])
        ]
        result = result_or_fail(request(revolve("taper", (curves, [f"taper-{i}" for i in range(len(curves))]))))
        kinds = {face["surface"]["kind"] for face in result["spatial_evidence"]["faces"]}
        self.assertEqual(result["solid_count"], 1)
        self.assertIn("cone", kinds)
        self.assertIn("plane", kinds)

    def test_r4_revolve_cut_creates_a_real_circumferential_groove(self):
        base = revolve("base", profile=rectangle(0, 80, 0, 10, "base-edge"))
        groove = revolve("groove", profile=rectangle(30, 35, 8, 12, "groove-edge"), operation="cut")
        result = result_or_fail(request(base, groove))
        self.assertEqual(result["solid_count"], 1)
        self.assertEqual(result["spatial_evidence"]["operation_features"], ["base", "groove"])
        expected = math.pi * (10**2 - 8**2) * 5
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], math.pi * 10**2 * 80 - expected, delta=1.0e-4)
        self.assertGreater(sum(face["surface"]["kind"] == "cylinder" for face in result["spatial_evidence"]["faces"]), 0)

    def test_revolve_add_to_existing_extrude_body_keeps_final_surface_provenance(self):
        base = {
            "kind": "extrude",
            "data": {
                "feature_id": "disk_add", "sketch_id": "disk_sketch", "operation": "add",
                "plane": {"origin": [0, 0, 0], "u_axis": [1, 0, 0], "v_axis": [0, 1, 0], "normal": [0, 0, 1]},
                "loops": [[{"kind": "circle", "center": [0, 0], "radius": 20}]],
                "profile_entity_ids": [["disk-circle"]], "direction": [0, 0, 1],
                "extent": {"kind": "distance", "length": 10}, "source_feature_id": "disk_add",
            },
        }
        ring = revolve(
            "ring_add", profile=rectangle(20, 25, 0, 10, "ring-edge"),
            axis_origin=(0, 0, 0), direction=(0, 0, 1),
            plane={"origin": [0, 0, 0], "u_axis": [1, 0, 0], "v_axis": [0, 0, 1], "normal": [0, -1, 0]},
        )
        result = result_or_fail(request(base, ring))
        self.assertEqual(result["solid_count"], 1)
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], math.pi * 25**2 * 10, delta=1.0e-4)
        known = {"disk_add", "ring_add"}
        for item in result["spatial_evidence"]["faces"] + result["spatial_evidence"]["edges"]:
            self.assertTrue(item["source_features"], item)
            self.assertTrue(set(item["source_features"]).issubset(known), item)

    def test_r5_r6_partial_revolves_are_valid_solids(self):
        for degrees, fraction in ((90, 0.25), (180, 0.5)):
            with self.subTest(degrees=degrees):
                result = result_or_fail(request(revolve(angle={"kind": "partial", "degrees": degrees})))
                self.assertEqual(result["solid_count"], 1)
                self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], math.pi * 25 * 20 * fraction, delta=1.0e-4)

        forward = result_or_fail(request(revolve(angle={"kind": "partial", "degrees": 90}, orientation="forward")))
        reverse = result_or_fail(request(revolve(angle={"kind": "partial", "degrees": 90}, orientation="reverse")))
        self.assertLess(forward["bbox"]["y_min"], -4.9)
        self.assertGreater(reverse["bbox"]["y_max"], 4.9)
        self.assertAlmostEqual(forward["spatial_evidence"]["volume_mm3"], reverse["spatial_evidence"]["volume_mm3"], delta=1.0e-5)

    def test_r7_r8_invalid_angles_fail_before_kernel_call(self):
        self.assertEqual(error_code(request(revolve(angle={"kind": "partial", "degrees": 0}))), "INVALID_REVOLVE_ANGLE")
        self.assertEqual(error_code(request(revolve(angle={"kind": "partial", "degrees": 360.1}))), "INVALID_REVOLVE_ANGLE")

    def test_r9_missing_or_zero_axis_fails_closed(self):
        missing = revolve()
        del missing["data"]["direction"]
        self.assertEqual(error_code(request(missing)), "REBUILD_DEPENDENCY_ERROR")
        self.assertEqual(error_code(request(revolve(direction=(0, 0, 0)))), "INVALID_DATUM_AXIS")

    def test_r10_profile_crossing_axis_is_rejected_before_revolve(self):
        self.assertEqual(error_code(request(revolve(profile=rectangle(0, 20, -2, 2, "cross")))), "REVOLVE_PROFILE_CROSSES_AXIS")

    def test_r11_detached_revolve_add_fails_one_body_contract(self):
        base = revolve("base", profile=rectangle(0, 20, 0, 5, "base-edge"))
        detached_plane = {**PLANE_XZ, "origin": [100.0, 0.0, 0.0]}
        detached = revolve("detached", profile=rectangle(0, 5, 0, 2, "det-edge"), axis_origin=(100, 0, 0), plane=detached_plane)
        self.assertEqual(error_code(request(base, detached)), "DETACHED_ADD")

    def test_r12_r13_profile_and_angle_changes_rebuild_deterministically(self):
        first = result_or_fail(request(revolve("editable")))
        second = result_or_fail(request(revolve("editable")))
        self.assertEqual(first["spatial_evidence"]["faces"], second["spatial_evidence"]["faces"])
        self.assertEqual(first["spatial_evidence"]["edges"], second["spatial_evidence"]["edges"])
        wider = result_or_fail(request(revolve("editable", profile=rectangle(0, 30, 0, 5, "wide"))))
        partial = result_or_fail(request(revolve("editable", angle={"kind": "partial", "degrees": 180})))
        self.assertGreater(wider["spatial_evidence"]["volume_mm3"], first["spatial_evidence"]["volume_mm3"])
        self.assertLess(partial["spatial_evidence"]["volume_mm3"], first["spatial_evidence"]["volume_mm3"])

    def test_revolve_generated_circular_edge_supports_semantic_fillet_and_chamfer(self):
        base = revolve("shaft", profile=rectangle(0, 20, 0, 5, "shaft-edge"))
        edge_ref = {"source_feature_id": "shaft", "selector": {
            "kind": "revolve_profile_vertex", "entity_id": "shaft-edge-2", "endpoint": "start",
        }, "qualifier": {
            "curve_kind": "circular", "radius_mm": 5.0, "length_mm": None,
            "midpoint": None, "tolerance_mm": 1.0e-5,
        }}
        for kind, dimension in (("fillet", "radius"), ("chamfer", "distance")):
            with self.subTest(kind=kind):
                treatment = {"kind": kind, "data": {
                    "feature_id": f"edge-{kind}", "edge_refs": [edge_ref], dimension: 0.5,
                }}
                result = result_or_fail(request(base, treatment))
                self.assertEqual(result["solid_count"], 1)
                self.assertGreater(result["spatial_evidence"]["volume_mm3"], 0)

    def test_revolve_result_keeps_exact_face_evidence_and_three_hlr_views(self):
        result = result_or_fail(request(revolve("evidence")))
        self.assertGreater(sum(face["surface"]["kind"] == "cylinder" for face in result["spatial_evidence"]["faces"]), 0)
        self.assertTrue(all(result["spatial_evidence"]["views"][view] for view in ("front", "top", "left")))


if __name__ == "__main__":
    unittest.main()
