"""Real OCP validation for semantic topology references and edge treatments."""
from __future__ import annotations

import copy
import unittest

import cadquery as cq
from aicad_cad.parametric_rebuild import _projected_circle_geometries, rebuild_parametric_model
from test_parametric_rebuild import MODEL_HASH, operation, rectangle, request, succeed, circle


def edge_ref(source: str, selector: dict, **qualifier):
    values = {"curve_kind": None, "radius_mm": None, "length_mm": None, "midpoint": None, "tolerance_mm": 1e-6}
    values.update(qualifier)
    return {"source_feature_id": source, "selector": selector, "qualifier": values}


def treatment(kind: str, feature_id: str, refs: list[dict], value: float):
    dimension = "radius" if kind == "fillet" else "distance"
    return {"kind": kind, "data": {"feature_id": feature_id, "edge_refs": refs, dimension: value}}


def generic_fixture():
    base = operation("plate-add", rectangle(0, 0, 120, 90), extent={"kind": "distance", "length": 20.0})
    base["source_feature_id"] = "plate"
    boss = operation("boss-add", rectangle(23, 13, 30, 22), z=20.0, extent={"kind": "distance", "length": 15.0})
    boss["source_feature_id"] = "boss"
    hole = operation("hole::main", circle(-30, -12, 8.0), z=20.0, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
    hole["source_feature_id"] = "hole"
    hole["source_feature_kind"] = "hole"
    pocket = operation("pocket-cut", rectangle(-32, 26, 40, 24), z=20.0, op="cut", direction=(0, 0, -1), extent={"kind": "distance", "length": 6.0})
    pocket["source_feature_id"] = "pocket"
    profile_edges = [edge_ref("boss", {"kind": "profile_boundary", "entity_id": f"boss-add-entity-{index}", "side": "top"}, curve_kind="line") for index in range(4)]
    hole_mouth = edge_ref("hole", {"kind": "hole_entrance"}, curve_kind="circular", radius_mm=8.0)
    return [base, boss, hole, pocket], profile_edges, hole_mouth


class G3SemanticTopologyTests(unittest.TestCase):
    def test_oblique_projected_circle_uses_bounded_polylines(self):
        edge = cq.Edge.makeCircle(10.0, cq.Vector(0, 0, 0), cq.Vector(0, 1, 1))
        geometries, tolerance = _projected_circle_geometries(edge, (1.0, 0.0, 0.0), (0.0, 0.0, 1.0), 2**-0.5)
        self.assertEqual(tolerance, 1.0e-4)
        self.assertGreater(len(geometries), 8)
        self.assertTrue(all(item["kind"] == "segment" for item in geometries))

        def segment_distance(point, start, end):
            dx, dy = end["x"] - start["x"], end["y"] - start["y"]
            denominator = dx * dx + dy * dy
            fraction = 0.0 if denominator == 0 else max(0.0, min(1.0, ((point[0] - start["x"]) * dx + (point[1] - start["y"]) * dy) / denominator))
            return ((point[0] - start["x"] - fraction * dx) ** 2 + (point[1] - start["y"] - fraction * dy) ** 2) ** 0.5

        for sample in range(257):
            point = edge.positionAt(sample / 256.0)
            projected = (point.x, point.z)
            error = min(segment_distance(projected, item["start"], item["end"]) for item in geometries)
            self.assertLessEqual(error, tolerance + 1.0e-6)

    def test_generic_fixture_executes_real_multiedge_fillet_and_hole_mouth_chamfer(self):
        steps, profile_edges, hole_mouth = generic_fixture()
        value = request(*steps, treatment("fillet", "boss-fillet", profile_edges, 3.0), treatment("chamfer", "hole-mouth-chamfer", [hole_mouth], 0.8))
        result = succeed(value)
        evidence = result["spatial_evidence"]
        self.assertEqual(result["solid_count"], 1)
        self.assertEqual(evidence["operation_features"], ["plate", "boss", "hole", "pocket", "boss-fillet", "hole-mouth-chamfer"])
        self.assertEqual(set(evidence["views"]), {"front", "top", "left"})
        self.assertTrue(all(evidence["views"][name] for name in ("front", "top", "left")))
        self.assertTrue(any(edge["curve"]["kind"] == "elliptical" for edge in evidence["edges"]))
        self.assertTrue(any(edge["curve"]["kind"] == "elliptical" and "boss-fillet" in edge["source_features"] for edge in evidence["edges"]))
        self.assertTrue(any(face["surface"]["kind"] == "cone" for face in evidence["faces"]))
        self.assertTrue(any(face["surface"]["kind"] == "cone" and "hole-mouth-chamfer" in face["source_features"] for face in evidence["faces"]))
        self.assertTrue(all(edge["source_features"] for edge in evidence["edges"]))
        self.assertTrue(all(face["source_features"] for face in evidence["faces"]))
        before = succeed(request(*steps))["spatial_evidence"]["views"]
        self.assertTrue(all(before[name] != evidence["views"][name] for name in ("front", "top", "left")))
        self.assertTrue(succeed(value)["spatial_evidence"]["views"] == evidence["views"])

    def test_profile_edge_reference_follows_upstream_dimension_change_by_entity_identity(self):
        steps, profile_edges, _hole_mouth = generic_fixture()
        boss = copy.deepcopy(steps[1])
        boss["loops"] = [rectangle(23, 13, 34, 26)]
        value = request(steps[0], boss, treatment("fillet", "boss-fillet", profile_edges, 3.0))
        self.assertEqual(succeed(value)["solid_count"], 1)

    def test_removed_or_missing_semantic_edge_fails_closed(self):
        steps, profile_edges, _ = generic_fixture()
        cross_cut = operation("cross-cut", rectangle(23, 2, 5, 5), z=35.0, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
        cross_cut["source_feature_id"] = "cross-cut"
        stale = rebuild_parametric_model(request(steps[0], steps[1], cross_cut, treatment("fillet", "f", [profile_edges[0]], 1.0)))
        self.assertFalse(stale["ok"])
        self.assertEqual(stale["error"]["code"], "TOPOLOGY_REFERENCE_STALE")
        missing_ref = edge_ref("boss", {"kind": "profile_boundary", "entity_id": "unknown-entity", "side": "top"})
        missing = rebuild_parametric_model(request(steps[0], steps[1], treatment("fillet", "f", [missing_ref], 1.0)))
        self.assertFalse(missing["ok"])
        self.assertEqual(missing["error"]["code"], "TOPOLOGY_REFERENCE_NOT_FOUND")

    def test_duplicate_source_entity_is_ambiguous_and_never_chooses_first(self):
        base = operation("base", rectangle(0, 0, 120, 90), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        first = operation("left-boss", rectangle(-25, 0, 20, 20), z=20.0, extent={"kind": "distance", "length": 10.0})
        second = operation("right-boss", rectangle(25, 0, 20, 20), z=20.0, extent={"kind": "distance", "length": 10.0})
        for item in (first, second):
            item["source_feature_id"] = "pattern"
            item["source_feature_kind"] = "pattern:linear"
            item["profile_entity_ids"] = [["same-0", "same-1", "same-2", "same-3"]]
        ref = edge_ref("pattern", {"kind": "profile_boundary", "entity_id": "same-0", "side": "top"})
        result = rebuild_parametric_model(request(base, first, second, treatment("fillet", "ambiguous-fillet", [ref], 1.0)))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "AMBIGUOUS_TOPOLOGY_REFERENCE")

    def test_hole_exit_and_pocket_entrance_selectors_rebuild(self):
        base = operation("base", rectangle(0, 0, 100, 80), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        hole = operation("hole::main", circle(0, 0, 8.0), z=20.0, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
        hole["source_feature_id"] = "hole"
        hole["source_feature_kind"] = "hole"
        exit_ref = edge_ref("hole", {"kind": "hole_exit"}, curve_kind="circular", radius_mm=8.0)
        exit_result = succeed(request(base, hole, treatment("chamfer", "exit-chamfer", [exit_ref], 0.5)))
        self.assertEqual(exit_result["solid_count"], 1)
        pocket = operation("pocket", rectangle(25, 0, 30, 20), z=20.0, op="cut", direction=(0, 0, -1), extent={"kind": "distance", "length": 6.0})
        pocket["source_feature_id"] = "pocket"
        pocket_ref = edge_ref("pocket", {"kind": "pocket_entrance", "entity_id": "pocket-entity-0"}, curve_kind="line")
        pocket_result = succeed(request(base, pocket, treatment("chamfer", "pocket-chamfer", [pocket_ref], 0.5)))
        self.assertEqual(pocket_result["solid_count"], 1)

    def test_adjacent_semantic_faces_and_convex_edge_selectors(self):
        base = operation("base", rectangle(0, 0, 20, 20), extent={"kind": "distance", "length": 10.0})
        base["source_feature_id"] = "plate"
        first = {"source_feature_id": "plate", "selector": "top", "normal": [0, 0, 1], "plane_offset": 10.0, "tolerance": 1e-6}
        second = {"source_feature_id": "plate", "selector": "positive_x", "normal": [1, 0, 0], "plane_offset": 10.0, "tolerance": 1e-6}
        ref = edge_ref("plate", {"kind": "between_faces", "first": first, "second": second})
        result = succeed(request(base, treatment("chamfer", "face-edge-chamfer", [ref], 0.5)))
        self.assertEqual(result["solid_count"], 1)
        convex = edge_ref("plate", {"kind": "convex", "first": first, "second": second})
        convex_result = succeed(request(base, treatment("fillet", "convex-fillet", [convex], 0.5)))
        self.assertEqual(convex_result["solid_count"], 1)

    def test_semantic_face_must_exist_on_its_declared_source_feature(self):
        base = operation("base", rectangle(0, 0, 20, 20), extent={"kind": "distance", "length": 10.0})
        base["source_feature_id"] = "plate"
        boss = operation("boss", rectangle(0, 0, 8, 8), z=10.0, extent={"kind": "distance", "length": 5.0})
        boss["source_feature_id"] = "boss"
        wrong_source_top = {"source_feature_id": "plate", "selector": "top", "normal": [0, 0, 1], "plane_offset": 15.0, "tolerance": 1e-6}
        boss_side = {"source_feature_id": "boss", "selector": "positive_x", "normal": [1, 0, 0], "plane_offset": 4.0, "tolerance": 1e-6}
        ref = edge_ref("boss", {"kind": "between_faces", "first": wrong_source_top, "second": boss_side})
        result = rebuild_parametric_model(request(base, boss, treatment("chamfer", "wrong-source-face", [ref], 0.5)))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "TOPOLOGY_REFERENCE_NOT_FOUND")

    def test_concave_pocket_floor_wall_edge_selector(self):
        base = operation("base", rectangle(0, 0, 100, 80), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        pocket = operation("pocket", rectangle(0, 0, 30, 20), z=20.0, op="cut", direction=(0, 0, -1), extent={"kind": "distance", "length": 6.0})
        pocket["source_feature_id"] = "pocket"
        floor = {"source_feature_id": "pocket", "selector": "top", "normal": [0, 0, 1], "plane_offset": 14.0, "tolerance": 1e-6}
        wall = {"source_feature_id": "pocket", "selector": "negative_x", "normal": [-1, 0, 0], "plane_offset": -15.0, "tolerance": 1e-6}
        ref = edge_ref("pocket", {"kind": "concave", "first": floor, "second": wall})
        result = succeed(request(base, pocket, treatment("chamfer", "pocket-floor-chamfer", [ref], 0.5)))
        self.assertEqual(result["solid_count"], 1)

    def test_top_and_bottom_profile_edges_resolve_and_multiple_chamfer_is_exact(self):
        plate = operation("plate-add", rectangle(0, 0, 120, 90), extent={"kind": "distance", "length": 20.0})
        plate["source_feature_id"] = "plate"
        top = edge_ref("plate", {"kind": "profile_boundary", "entity_id": "plate-add-entity-0", "side": "top"}, curve_kind="line")
        bottom = edge_ref("plate", {"kind": "profile_boundary", "entity_id": "plate-add-entity-0", "side": "bottom"}, curve_kind="line")
        top_only = succeed(request(plate, treatment("chamfer", "top-edge-chamfer", [top], 0.5)))
        bottom_only = succeed(request(plate, treatment("chamfer", "bottom-edge-chamfer", [bottom], 0.5)))
        both = succeed(request(plate, treatment("chamfer", "two-edge-chamfer", [top, bottom], 0.5)))
        self.assertEqual(top_only["solid_count"], 1)
        self.assertEqual(bottom_only["solid_count"], 1)
        self.assertEqual(both["solid_count"], 1)
        self.assertLess(both["spatial_evidence"]["volume_mm3"], top_only["spatial_evidence"]["volume_mm3"])

    def test_reference_survives_unrelated_downstream_cut_and_fillet_before_cut(self):
        steps, profile_edges, _ = generic_fixture()
        fillet = treatment("fillet", "boss-fillet", [profile_edges[0]], 1.0)
        before_cut = succeed(request(steps[0], steps[1], fillet))
        with_downstream_cut = succeed(request(steps[0], steps[1], fillet, steps[2], steps[3]))
        self.assertEqual(before_cut["spatial_evidence"]["operation_features"], ["plate", "boss", "boss-fillet"])
        self.assertEqual(with_downstream_cut["spatial_evidence"]["operation_features"], ["plate", "boss", "boss-fillet", "hole", "pocket"])
        self.assertNotEqual(before_cut["spatial_evidence"]["volume_mm3"], with_downstream_cut["spatial_evidence"]["volume_mm3"])

    def test_fillets_and_chamfers_rebuild_deterministically_after_parameter_edits(self):
        steps, profile_edges, hole_mouth = generic_fixture()
        no_treatment = succeed(request(steps[0], steps[1], steps[2], steps[3]))
        suppressed_fillets_rebuild = succeed(request(steps[0], steps[1], steps[2], steps[3]))
        radius_two = succeed(request(*steps, treatment("fillet", "boss-fillet", [profile_edges[0]], 2.0), treatment("chamfer", "hole-chamfer", [hole_mouth], 0.5)))
        radius_three = succeed(request(*steps, treatment("fillet", "boss-fillet", [profile_edges[0]], 3.0), treatment("chamfer", "hole-chamfer", [hole_mouth], 0.8)))
        repeat = succeed(request(*steps, treatment("fillet", "boss-fillet", [profile_edges[0]], 3.0), treatment("chamfer", "hole-chamfer", [hole_mouth], 0.8)))
        self.assertEqual(no_treatment["spatial_evidence"]["volume_mm3"], suppressed_fillets_rebuild["spatial_evidence"]["volume_mm3"])
        self.assertNotEqual(radius_two["spatial_evidence"]["volume_mm3"], radius_three["spatial_evidence"]["volume_mm3"])
        self.assertEqual(radius_three["spatial_evidence"]["views"], repeat["spatial_evidence"]["views"])
        self.assertEqual(radius_three["spatial_evidence"]["volume_mm3"], repeat["spatial_evidence"]["volume_mm3"])

    def test_cylindrical_boss_r1_fillet_preserves_torus_hole_c0_5_and_explicit_plate_c1(self):
        base = operation("plate-add", rectangle(0, 0, 180, 120), extent={"kind": "distance", "length": 24.0})
        base["source_feature_id"] = "complex-plate"
        boss = operation("left-boss-add", circle(-50, 0, 33.0), z=24.0, extent={"kind": "distance", "length": 24.0})
        boss["source_feature_id"] = "left-round-boss"
        hole = operation("left-hole::main", circle(-50, 0, 16.0), z=48.0, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
        hole["source_feature_id"] = "left-center-hole"
        hole["source_feature_kind"] = "hole"
        boss_top = edge_ref("left-round-boss", {"kind": "profile_boundary", "entity_id": "left-boss-add-entity-0", "side": "top"}, curve_kind="circular", radius_mm=33.0)
        hole_entrance = edge_ref("left-center-hole", {"kind": "hole_entrance"}, curve_kind="circular", radius_mm=16.0)
        plate_bottom = edge_ref("complex-plate", {"kind": "profile_boundary", "entity_id": "plate-add-entity-0", "side": "bottom"}, curve_kind="line")
        result = succeed(request(
            base,
            boss,
            hole,
            treatment("fillet", "left-boss-fillet", [boss_top], 1.0),
            treatment("chamfer", "left-hole-chamfer", [hole_entrance], 0.5),
            treatment("chamfer", "plate-bottom-chamfer", [plate_bottom], 1.0),
        ))
        evidence = result["spatial_evidence"]
        self.assertEqual(result["solid_count"], 1)
        self.assertTrue(any(face["surface"]["kind"] == "torus" and "left-boss-fillet" in face["source_features"] for face in evidence["faces"]))
        self.assertTrue(any(face["surface"]["kind"] == "cone" and "left-hole-chamfer" in face["source_features"] for face in evidence["faces"]))
        self.assertEqual(set(evidence["views"]), {"front", "top", "left"})
        derived_edges = [edge for view in evidence["views"].values() for edge in view if edge["derived_approximation"]]
        self.assertTrue(derived_edges)
        self.assertTrue(all(edge["approximation_tolerance_mm"] == 0.01 for edge in derived_edges))
        self.assertTrue(all(edge["geometry"]["kind"] == "segment" for edge in derived_edges))
        self.assertTrue(all(
            edge["geometry"]["start_angle"] != edge["geometry"]["end_angle"]
            for view in evidence["views"].values()
            for edge in view
            if edge["geometry"]["kind"] == "arc"
        ))

    def test_invalid_and_oversized_treatments_have_stable_errors(self):
        steps, profile_edges, _ = generic_fixture()
        invalid = rebuild_parametric_model(request(*steps, treatment("fillet", "bad-radius", [profile_edges[0]], float("nan"))))
        self.assertFalse(invalid["ok"])
        self.assertEqual(invalid["error"]["code"], "INVALID_FILLET_RADIUS")
        oversized = rebuild_parametric_model(request(steps[0], steps[1], treatment("fillet", "too-large", profile_edges, 100.0)))
        self.assertFalse(oversized["ok"])
        self.assertEqual(oversized["error"]["code"], "FILLET_GEOMETRY_FAILED")
        invalid_distance = rebuild_parametric_model(request(steps[0], steps[1], treatment("chamfer", "bad-distance", [profile_edges[0]], float("nan"))))
        self.assertFalse(invalid_distance["ok"])
        self.assertEqual(invalid_distance["error"]["code"], "INVALID_CHAMFER_DISTANCE")
        oversized_chamfer = rebuild_parametric_model(request(steps[0], steps[1], treatment("chamfer", "too-large-chamfer", [profile_edges[0]], 100.0)))
        self.assertFalse(oversized_chamfer["ok"])
        self.assertEqual(oversized_chamfer["error"]["code"], "CHAMFER_GEOMETRY_FAILED")


if __name__ == "__main__":
    unittest.main()
