"""Real B-Rep acceptance for the generic ordered Extrude kernel."""

from __future__ import annotations

import copy
import math
import unittest

import cadquery as cq

from aicad_cad.parametric_rebuild import _edge_evidence, _hlr_evidence, _parse_request, _rebuild, rebuild_parametric_model, section_parametric_model


MODEL_HASH = "4" * 64
PLANE = {
    "origin": [0.0, 0.0, 0.0],
    "u_axis": [1.0, 0.0, 0.0],
    "v_axis": [0.0, 1.0, 0.0],
    "normal": [0.0, 0.0, 1.0],
}


def line(start, end):
    return {"kind": "line", "start": list(start), "end": list(end)}


def rectangle(cx, cy, width, height):
    hx, hy = width / 2.0, height / 2.0
    points = [
        (cx - hx, cy - hy),
        (cx + hx, cy - hy),
        (cx + hx, cy + hy),
        (cx - hx, cy + hy),
    ]
    return [line(points[index], points[(index + 1) % 4]) for index in range(4)]


def circle(cx, cy, radius):
    return [{"kind": "circle", "center": [cx, cy], "radius": radius}]


def rounded_rectangle(cx, cy, width, height, radius):
    hx, hy = width / 2.0, height / 2.0
    return [
        line((cx - hx + radius, cy - hy), (cx + hx - radius, cy - hy)),
        {"kind": "arc", "start": [cx + hx - radius, cy - hy], "end": [cx + hx, cy - hy + radius], "center": [cx + hx - radius, cy - hy + radius], "radius": radius, "direction": "counter_clockwise"},
        line((cx + hx, cy - hy + radius), (cx + hx, cy + hy - radius)),
        {"kind": "arc", "start": [cx + hx, cy + hy - radius], "end": [cx + hx - radius, cy + hy], "center": [cx + hx - radius, cy + hy - radius], "radius": radius, "direction": "counter_clockwise"},
        line((cx + hx - radius, cy + hy), (cx - hx + radius, cy + hy)),
        {"kind": "arc", "start": [cx - hx + radius, cy + hy], "end": [cx - hx, cy + hy - radius], "center": [cx - hx + radius, cy + hy - radius], "radius": radius, "direction": "counter_clockwise"},
        line((cx - hx, cy + hy - radius), (cx - hx, cy - hy + radius)),
        {"kind": "arc", "start": [cx - hx, cy - hy + radius], "end": [cx - hx + radius, cy - hy], "center": [cx - hx + radius, cy - hy + radius], "radius": radius, "direction": "counter_clockwise"},
    ]


def slot(cx, cy, overall_length, width):
    half_centres = (overall_length - width) / 2.0
    return [
        line((cx - half_centres, cy - width / 2.0), (cx + half_centres, cy - width / 2.0)),
        {"kind": "arc", "start": [cx + half_centres, cy - width / 2.0], "end": [cx + half_centres, cy + width / 2.0], "center": [cx + half_centres, cy], "radius": width / 2.0, "direction": "counter_clockwise"},
        line((cx + half_centres, cy + width / 2.0), (cx - half_centres, cy + width / 2.0)),
        {"kind": "arc", "start": [cx - half_centres, cy + width / 2.0], "end": [cx - half_centres, cy - width / 2.0], "center": [cx - half_centres, cy], "radius": width / 2.0, "direction": "counter_clockwise"},
    ]


def operation(feature, profile, *, z=0.0, op="add", direction=(0.0, 0.0, 1.0), extent=None):
    plane = copy.deepcopy(PLANE)
    plane["origin"][2] = z
    if extent is None:
        extent = {"kind": "distance", "length": 10.0}
    return {
        "feature_id": feature,
        "sketch_id": f"{feature}_sketch",
        "operation": op,
        "plane": plane,
        "loops": [profile],
        "profile_entity_ids": [[f"{feature}-entity-{index}" for index, _curve in enumerate(profile)]],
        "direction": list(direction),
        "extent": extent,
    }


def countersink_operation(feature, source, plane, direction, main_diameter, mouth_diameter, angle_degrees, source_kind="hole"):
    return {
        "kind": "countersink",
        "data": {
            "feature_id": feature,
            "source_feature_id": source,
            "source_feature_kind": source_kind,
            "plane": copy.deepcopy(plane),
            "direction": list(direction),
            "main_diameter": main_diameter,
            "mouth_diameter": mouth_diameter,
            "included_angle_degrees": angle_degrees,
        },
    }


def request(*operations):
    envelopes = [item if isinstance(item, dict) and set(item) == {"kind", "data"} else {"kind": "extrude", "data": item} for item in operations]
    return {"schema_version": 6, "model_sha256": MODEL_HASH, "operations": envelopes}


def sweep_operation(feature, profile, path, path_plane, profile_plane, *, op="add", direction="forward", profile_origin=(0.0, 0.0)):
    return {
        "kind": "sweep",
        "data": {
            "feature_id": feature,
            "sketch_id": f"{feature}_profile",
            "path_id": f"{feature}_path",
            "operation": op,
            "plane": copy.deepcopy(profile_plane),
            "loops": [profile],
            "profile_entity_ids": [[f"{feature}-profile-{index}" for index, _curve in enumerate(profile)]],
            "profile_origin": list(profile_origin),
            "path_plane": copy.deepcopy(path_plane),
            "path": copy.deepcopy(path),
            "path_entity_ids": [f"{feature}-path-{index}" for index, _curve in enumerate(path)],
            "direction": direction,
            "orientation": "fixed_binormal",
        },
    }


def loft_section(sketch_id, z, profile):
    plane = copy.deepcopy(PLANE)
    plane["origin"][2] = z
    return {
        "sketch_id": sketch_id,
        "plane": plane,
        "loops": [copy.deepcopy(profile)],
        "profile_entity_ids": [[f"{sketch_id}-entity-{index}" for index, _ in enumerate(profile)]],
    }


def loft_operation(feature, sections, *, op="add", mode="ruled", orientation="aligned"):
    return {
        "kind": "loft",
        "data": {
            "feature_id": feature,
            "operation": op,
            "mode": mode,
            "orientation_policy": orientation,
            "sections": copy.deepcopy(sections),
        },
    }


def succeed(value):
    result = rebuild_parametric_model(value)
    if not result["ok"]:
        raise AssertionError(result["error"])
    return result["result"]


class ParametricRebuildTests(unittest.TestCase):
    def test_hlr_projection_coordinates_preserve_front_top_and_left_spans(self):
        shape = cq.Workplane("XY").box(100, 60, 10, centered=(True, True, False)).val()
        expected = {
            "front": (-50.0, -30.0, 50.0, 30.0),
            "top": (-50.0, -10.0, 50.0, 0.0),
            "left": (0.0, -30.0, 10.0, 30.0),
        }

        for view, bounds in expected.items():
            with self.subTest(view=view):
                points = []
                for record in _hlr_evidence(shape, view):
                    geometry = record["geometry"]
                    if geometry["kind"] == "segment":
                        points.extend((geometry["start"], geometry["end"]))
                    elif geometry["kind"] == "circle":
                        center = geometry["center"]
                        radius = geometry["radius"]
                        points.extend(
                            (
                                {"x": center["x"] - radius, "y": center["y"] - radius},
                                {"x": center["x"] + radius, "y": center["y"] + radius},
                            )
                        )
                actual = (
                    min(point["x"] for point in points),
                    min(point["y"] for point in points),
                    max(point["x"] for point in points),
                    max(point["y"] for point in points),
                )
                for observed, target in zip(actual, bounds):
                    self.assertAlmostEqual(observed, target, places=6)

    def test_multi_axis_through_holes_execute_real_booleans_in_all_signed_directions(self):
        base = operation("body", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        cases = [
            ("z_plus", PLANE, (0.0, 0.0, 1.0)),
            ("z_minus", {"origin": [0, 0, 20], "u_axis": [1, 0, 0], "v_axis": [0, 1, 0], "normal": [0, 0, 1]}, (0.0, 0.0, -1.0)),
            ("x_plus", {"origin": [-70, 0, 10], "u_axis": [0, 1, 0], "v_axis": [0, 0, 1], "normal": [1, 0, 0]}, (1.0, 0.0, 0.0)),
            ("x_minus", {"origin": [70, 0, 10], "u_axis": [0, 1, 0], "v_axis": [0, 0, 1], "normal": [1, 0, 0]}, (-1.0, 0.0, 0.0)),
            ("y_plus", {"origin": [0, -50, 10], "u_axis": [1, 0, 0], "v_axis": [0, 0, -1], "normal": [0, 1, 0]}, (0.0, 1.0, 0.0)),
            ("y_minus", {"origin": [0, 50, 10], "u_axis": [1, 0, 0], "v_axis": [0, 0, -1], "normal": [0, 1, 0]}, (0.0, -1.0, 0.0)),
        ]
        cuts = []
        for feature, plane, direction in cases:
            cut = operation(feature, circle(0, 0, 2.0), op="cut", direction=direction, extent={"kind": "through_all"})
            cut["plane"] = copy.deepcopy(plane)
            cut["source_feature_id"] = feature
            cut["source_feature_kind"] = "hole"
            cuts.append(cut)
        for cut in cuts:
            result = succeed(request(base, cut))
            self.assertEqual(result["solid_count"], 1)
            self.assertEqual(result["spatial_evidence"]["operation_features"], ["plate", cut["source_feature_id"]])
            self.assertLess(result["spatial_evidence"]["volume_mm3"], 280000.0)
            self.assertEqual(set(result["spatial_evidence"]["views"]), {"front", "top", "left"})

    def test_counterbore_is_two_exact_coaxial_cuts_but_one_source_feature(self):
        base = operation("plate_add", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        main = operation("hole-main", circle(0, 0, 4.0), op="cut", direction=(0, 0, -1), z=20.0, extent={"kind": "through_all"})
        main["source_feature_id"] = "hole"
        main["source_feature_kind"] = "hole"
        cb = operation("hole-counterbore", circle(0, 0, 7.0), op="cut", direction=(0, 0, -1), z=20.0, extent={"kind": "distance", "length": 5.0})
        cb["source_feature_id"] = "hole"
        cb["source_feature_kind"] = "hole"
        cb["blind_hole"] = True
        result = succeed(request(base, main, cb))
        expected = 140 * 100 * 20 - math.pi * 4**2 * 20 - math.pi * (7**2 - 4**2) * 5
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], expected, places=4)
        self.assertEqual(result["spatial_evidence"]["operation_features"], ["plate", "hole", "hole"])

    def test_spatial_evidence_samples_nonanalytic_ocp_edges_with_bounded_error(self):
        edge = cq.Edge.makeSpline([
            cq.Vector(0, 0, 0), cq.Vector(2, 0, 0), cq.Vector(3, 2, 1), cq.Vector(5, 2, 1),
        ])
        first = _edge_evidence(edge)[0]
        repeated = _edge_evidence(edge)[0]
        self.assertEqual(first, repeated)
        self.assertEqual(first["curve"]["kind"], "sampled_curve")
        self.assertEqual(first["curve"]["max_deviation_mm"], 0.05)
        self.assertGreater(len(first["curve"]["points"]), 2)
        samples = [tuple(point) for point in first["curve"]["points"]]

        def distance_to_segment(point, start, end):
            direction = tuple(end[index] - start[index] for index in range(3))
            offset = tuple(point[index] - start[index] for index in range(3))
            squared = sum(value * value for value in direction)
            fraction = max(0.0, min(1.0, sum(offset[index] * direction[index] for index in range(3)) / squared))
            projection = tuple(start[index] + fraction * direction[index] for index in range(3))
            return math.sqrt(sum((point[index] - projection[index]) ** 2 for index in range(3)))

        for index in range(1001):
            point = tuple(edge.positionAt(index / 1000).toTuple())
            deviation = min(distance_to_segment(point, start, end) for start, end in zip(samples, samples[1:]))
            self.assertLessEqual(deviation, 0.05 + 1.0e-8)
        self.assertGreater(first["length_mm"], 0.0)

    def test_g6b_countersink_is_an_exact_cone_in_all_signed_principal_directions(self):
        cases = [
            ("z_plus", PLANE, (0, 0, 1), 10.0),
            ("z_minus", {"origin": [0, 0, 10], "u_axis": [1, 0, 0], "v_axis": [0, 1, 0], "normal": [0, 0, 1]}, (0, 0, -1), 10.0),
            ("x_plus", {"origin": [-50, 0, 5], "u_axis": [0, 1, 0], "v_axis": [0, 0, 1], "normal": [1, 0, 0]}, (1, 0, 0), 100.0),
            ("x_minus", {"origin": [50, 0, 5], "u_axis": [0, 1, 0], "v_axis": [0, 0, 1], "normal": [1, 0, 0]}, (-1, 0, 0), 100.0),
            ("y_plus", {"origin": [0, -30, 5], "u_axis": [1, 0, 0], "v_axis": [0, 0, -1], "normal": [0, 1, 0]}, (0, 1, 0), 60.0),
            ("y_minus", {"origin": [0, 30, 5], "u_axis": [1, 0, 0], "v_axis": [0, 0, -1], "normal": [0, 1, 0]}, (0, -1, 0), 60.0),
        ]
        base = operation("plate_add", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0})
        base["source_feature_id"] = "plate"
        main_radius, mouth_radius, cone_depth = 2.0, 5.0, 3.0
        cone_volume = math.pi * cone_depth * (mouth_radius**2 + mouth_radius * main_radius + main_radius**2) / 3.0
        for name, plane, direction, axis_length in cases:
            with self.subTest(direction=name):
                main = operation(f"{name}-main", circle(0, 0, main_radius), op="cut", direction=direction, extent={"kind": "through_all"})
                main["plane"] = copy.deepcopy(plane)
                main["source_feature_id"] = name
                main["source_feature_kind"] = "hole"
                cone = countersink_operation(f"{name}-sink", name, plane, direction, 4.0, 10.0, 90.0)
                result = succeed(request(base, main, cone))
                expected = 100 * 60 * 10 - math.pi * main_radius**2 * axis_length - cone_volume + math.pi * main_radius**2 * cone_depth
                self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], expected, places=4)
                self.assertEqual(result["solid_count"], 1)
                self.assertEqual(result["spatial_evidence"]["operation_features"], ["plate", name, name])
                self.assertEqual(result["operation_timings"][-1]["kind"], "countersink_cut")
                cones = [face["surface"] for face in result["spatial_evidence"]["faces"] if face["surface"]["kind"] == "cone"]
                self.assertTrue(cones, "real OCP conical surface evidence must be present")
                self.assertTrue(any(math.isclose(abs(face["semi_angle_radians"]), math.pi / 4, abs_tol=1e-7) for face in cones))
                self.assertEqual(set(result["spatial_evidence"]["views"]), {"front", "top", "left"})

    def test_g6b_countersink_validation_and_no_intersection_fail_closed(self):
        base = operation("plate_add", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0})
        base["source_feature_id"] = "plate"
        plane = {"origin": [0, 0, 10], "u_axis": [1, 0, 0], "v_axis": [0, 1, 0], "normal": [0, 0, 1]}
        main = operation("hole::main", circle(0, 0, 2), op="cut", direction=(0, 0, -1), z=10, extent={"kind": "through_all"})
        main.update({"source_feature_id": "hole", "source_feature_kind": "hole"})
        cone = countersink_operation("hole::countersink", "hole", plane, (0, 0, -1), 4, 10, 90)
        cases = [
            ("mouth diameter", {**cone["data"], "mouth_diameter": 4.0}, "INVALID_COUNTERSINK"),
            ("angle", {**cone["data"], "included_angle_degrees": 180.0}, "INVALID_COUNTERSINK"),
            ("missing source", {**cone["data"], "source_feature_id": "missing"}, "FEATURE_DEPENDENCY_ERROR"),
        ]
        for label, data, code in cases:
            with self.subTest(case=label):
                result = rebuild_parametric_model(request(base, main, {"kind": "countersink", "data": data}))
                self.assertFalse(result["ok"])
                self.assertEqual(result["error"]["code"], code)
        outside = copy.deepcopy(main)
        outside["plane"]["origin"][2] = 14
        detached_cone = countersink_operation("hole::countersink", "hole", {**plane, "origin": [0, 0, 14]}, (0, 0, -1), 4, 10, 90)
        no_hit = rebuild_parametric_model(request(base, outside, detached_cone))
        self.assertFalse(no_hit["ok"])
        self.assertEqual(no_hit["error"]["code"], "COUNTERSINK_NO_INTERSECTION")

    def test_g6b_explicit_pattern_cuts_holes_and_extrude_cuts_at_nonuniform_local_targets(self):
        base = operation("plate_add", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0})
        base["source_feature_id"] = "plate"
        targets = [(20.0, 4.0), (-12.0, 17.0), (16.0, -21.0)]
        cases = []
        seed = operation("seed::main", circle(0, 0, 2.0), op="cut", direction=(0, 0, -1), z=10, extent={"kind": "through_all"})
        seed.update({"source_feature_id": "seed", "source_feature_kind": "hole"})
        cases.append((seed, "hole_pattern"))
        cut_seed = operation("cut_seed", rectangle(0, 0, 8, 4), op="cut", direction=(0, 0, -1), z=10, extent={"kind": "through_all"})
        cut_seed.update({"source_feature_id": "cut_seed", "source_feature_kind": "extrude"})
        cases.append((cut_seed, "cut_pattern"))
        for source_seed, pattern_id in cases:
            operations = [base, source_seed]
            for index, (u, v) in enumerate(targets, 1):
                instance = copy.deepcopy(source_seed)
                instance["feature_id"] = f"{pattern_id}::instance-{index:04d}"
                instance["source_feature_id"] = pattern_id
                instance["source_feature_kind"] = "pattern:explicit_placement"
                instance["plane"]["origin"][0] += u
                instance["plane"]["origin"][1] += v
                operations.append(instance)
            with self.subTest(pattern=pattern_id):
                result = succeed(request(*operations))
                self.assertEqual(result["solid_count"], 1)
                self.assertEqual(result["spatial_evidence"]["operation_features"], ["plate", source_seed["source_feature_id"], *([pattern_id] * len(targets))])
                self.assertEqual(len(result["operation_timings"]), 2 + len(targets))
                self.assertLess(result["spatial_evidence"]["volume_mm3"], 60000.0)

    def test_blind_hole_and_counterbore_are_exact_coaxial_cuts(self):
        base = operation("plate_add", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        main = operation("hole-main", circle(0, 0, 4.0), op="cut", direction=(0, 0, -1), z=20.0, extent={"kind": "distance", "length": 10.0})
        main["source_feature_id"] = "hole"
        main["source_feature_kind"] = "hole"
        main["blind_hole"] = True
        cb = operation("hole-counterbore", circle(0, 0, 7.0), op="cut", direction=(0, 0, -1), z=20.0, extent={"kind": "distance", "length": 5.0})
        cb["source_feature_id"] = "hole"
        cb["source_feature_kind"] = "hole"
        cb["blind_hole"] = True
        result = succeed(request(base, main, cb))
        expected = 140 * 100 * 20 - math.pi * 4**2 * 10 - math.pi * (7**2 - 4**2) * 5
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], expected, places=4)
        self.assertEqual(result["spatial_evidence"]["operation_features"], ["plate", "hole", "hole"])

    def test_pattern_instance_without_intersection_fails_with_stable_collision_code(self):
        base = operation("plate_add", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0})
        base["source_feature_id"] = "plate"
        seed = operation("seed-cut", circle(0, 0, 3.0), op="cut", direction=(0, 0, -1), z=10.0, extent={"kind": "through_all"})
        seed["source_feature_id"] = "seed"
        seed["source_feature_kind"] = "hole"
        instance = operation("pattern-instance", circle(200, 0, 3.0), op="cut", direction=(0, 0, -1), z=10.0, extent={"kind": "through_all"})
        instance["source_feature_id"] = "pattern"
        instance["source_feature_kind"] = "pattern:linear"
        result = rebuild_parametric_model(request(base, seed, instance))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "PATTERN_INSTANCE_COLLISION")

    def test_blind_side_hole_and_semantic_face_reference_resolve_by_geometry(self):
        base = operation("body", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        plane = {"origin": [70.0, 0.0, 0.0], "u_axis": [0.0, 1.0, 0.0], "v_axis": [0.0, 0.0, 1.0], "normal": [1.0, 0.0, 0.0]}
        side = operation("side-hole", circle(0, 0, 3.0), op="cut", direction=(-1.0, 0.0, 0.0), extent={"kind": "distance", "length": 15.0})
        side["plane"] = plane
        side["source_feature_id"] = "side-hole"
        side["source_feature_kind"] = "hole"
        side["blind_hole"] = True
        side["support_face"] = {"source_feature_id": "plate", "selector": "positive_x", "normal": [1, 0, 0], "plane_offset": 70.0, "tolerance": 1e-6}
        result = succeed(request(base, side))
        self.assertEqual(result["solid_count"], 1)
        self.assertLess(result["spatial_evidence"]["volume_mm3"], 280000.0)

    def test_invalid_semantic_face_and_excessive_blind_depth_fail_closed(self):
        base = operation("body", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        base["source_feature_id"] = "plate"
        side = operation("side-hole", circle(0, 0, 3.0), op="cut", direction=(-1.0, 0.0, 0.0), extent={"kind": "distance", "length": 15.0})
        side["plane"] = {"origin": [70, 0, 0], "u_axis": [0, 1, 0], "v_axis": [0, 0, 1], "normal": [1, 0, 0]}
        side["source_feature_kind"] = "hole"
        side["blind_hole"] = True
        side["support_face"] = {"source_feature_id": "plate", "selector": "positive_x", "normal": [1, 0, 0], "plane_offset": 80.0, "tolerance": 1e-6}
        bad_face = rebuild_parametric_model(request(base, side))
        self.assertFalse(bad_face["ok"])
        self.assertEqual(bad_face["error"]["code"], "INVALID_FACE_REFERENCE")
        side["support_face"]["plane_offset"] = 70.0
        for depth in (150.0, 0.0, -1.0):
            side["extent"] = {"kind": "distance", "length": depth}
            invalid_depth = rebuild_parametric_model(request(base, side))
            self.assertFalse(invalid_depth["ok"])
            self.assertEqual(invalid_depth["error"]["code"], "INVALID_HOLE_DEPTH")

    def test_rectangle_add_returns_real_body_measurements_and_all_hlr_views(self):
        result = succeed(request(operation("base_add", rectangle(0, 0, 100, 60))))
        self.assertEqual(result["solid_count"], 1)
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], 60000.0, places=5)
        self.assertEqual(result["bbox"], {"x_min": -50.0, "x_max": 50.0, "y_min": -30.0, "y_max": 30.0, "z_min": 0.0, "z_max": 10.0})
        self.assertGreater(len(result["spatial_evidence"]["edges"]), 0)
        self.assertGreater(len(result["spatial_evidence"]["faces"]), 0)
        self.assertEqual(set(result["spatial_evidence"]["views"]), {"front", "top", "left"})
        self.assertTrue(all(result["spatial_evidence"]["views"][view] for view in ("front", "top", "left")))

    def test_add_cut_and_add_use_one_sequential_body(self):
        result = succeed(request(
            operation("plate", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0}),
            operation("boss", circle(0, 0, 10), z=10, extent={"kind": "distance", "length": 8.0}),
            operation("pocket", rounded_rectangle(0, 0, 18, 10, 3), z=18, op="cut", direction=(0, 0, -1), extent={"kind": "distance", "length": 4.0}),
            operation("side_boss", rectangle(25, 0, 20, 20), z=10, extent={"kind": "distance", "length": 5.0}),
        ))
        expected = 60000 + math.pi * 10**2 * 8 + 20 * 20 * 5 - (18 * 10 - (4 - math.pi) * 9) * 4
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], expected, places=4)
        self.assertEqual(result["spatial_evidence"]["operation_features"], ["plate", "boss", "pocket", "side_boss"])
        self.assertEqual(result["solid_count"], 1)
        projected = [edge["geometry"]["kind"] for edges in result["spatial_evidence"]["views"].values() for edge in edges]
        self.assertIn("arc", projected)
        self.assertIn("circle", projected)

    def test_through_all_slot_is_exact_and_deterministic(self):
        value = request(
            operation("plate", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0}),
            operation("slot_cut", slot(-10, 0, 30, 8), z=10, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"}),
        )
        first = succeed(value)
        second = succeed(value)
        self.assertEqual([item["kind"] for item in first["operation_timings"]], ["extrude_add", "extrude_cut"])
        self.assertTrue(all(item["kernel_operation_ms"] > 0 and item["elapsed_ms"] >= item["kernel_operation_ms"] for item in first["operation_timings"]))
        first.pop("operation_timings")
        second.pop("operation_timings")
        self.assertEqual(first, second)
        area = 8 * (30 - 8) + math.pi * 4**2
        self.assertAlmostEqual(first["spatial_evidence"]["volume_mm3"], 60000 - area * 10, places=4)

    def test_symmetric_extent_is_centered_around_the_profile_plane(self):
        result = succeed(request(
            operation("symmetric", rectangle(0, 0, 20, 10), extent={"kind": "symmetric", "length": 12.0}),
        ))
        self.assertAlmostEqual(result["spatial_evidence"]["volume_mm3"], 2400.0, places=5)
        self.assertEqual(result["bbox"]["z_min"], -6.0)
        self.assertEqual(result["bbox"]["z_max"], 6.0)

    def test_xz_and_yz_extrusions_keep_their_directions(self):
        xz_plane = {"origin": [0.0, 0.0, 0.0], "u_axis": [1.0, 0.0, 0.0], "v_axis": [0.0, 0.0, 1.0], "normal": [0.0, -1.0, 0.0]}
        xz = operation("xz_add", rectangle(0, 0, 20, 30))
        xz["plane"] = xz_plane
        xz["direction"] = [0.0, -1.0, 0.0]
        xz["extent"] = {"kind": "distance", "length": 6.0}
        xz_result = succeed(request(xz))
        self.assertEqual(xz_result["bbox"]["y_min"], -6.0)
        self.assertEqual(xz_result["bbox"]["z_max"], 15.0)

        yz_plane = {"origin": [0.0, 0.0, 0.0], "u_axis": [0.0, 1.0, 0.0], "v_axis": [0.0, 0.0, 1.0], "normal": [1.0, 0.0, 0.0]}
        yz = operation("yz_add", rectangle(0, 0, 20, 30))
        yz["plane"] = yz_plane
        yz["direction"] = [1.0, 0.0, 0.0]
        yz["extent"] = {"kind": "distance", "length": 6.0}
        yz_result = succeed(request(yz))
        self.assertEqual(yz_result["bbox"]["x_max"], 6.0)
        self.assertEqual(yz_result["bbox"]["z_max"], 15.0)

    def test_exact_section_preserves_through_hole_and_is_deterministic(self):
        base = operation("plate", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        through = operation("through", circle(0, 0, 10), z=20, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
        value = {"action": "section", "rebuild": request(base, through), "section_plane": {"axis": "z", "offset_mm": 10.0, "view_side": "positive"}}
        first = section_parametric_model(value)
        second = section_parametric_model(value)
        self.assertTrue(first["ok"], first.get("error"))
        result = first["result"]
        geometry = result["section_geometry"]
        self.assertEqual(geometry["schema_version"], 1)
        self.assertEqual(geometry["model_sha256"], MODEL_HASH)
        self.assertEqual(geometry["plane"]["normal"], [0.0, 0.0, 1.0])
        self.assertEqual(len(geometry["loops"]), 2)  # separate outer material boundary and hole
        self.assertGreaterEqual(geometry["edge_count"], 5)
        self.assertAlmostEqual(geometry["sample_chord_tolerance_mm"], 0.01)
        without_timings = copy.deepcopy(geometry)
        other = copy.deepcopy(second["result"]["section_geometry"])
        without_timings.pop("geometry_elapsed_ms")
        other.pop("geometry_elapsed_ms")
        self.assertEqual(without_timings, other)
        self.assertEqual(result["rebuild"]["spatial_evidence"]["volume_mm3"], succeed(request(base, through))["spatial_evidence"]["volume_mm3"])

    def test_section_through_blind_pocket_and_side_blind_hole_uses_real_cut_depth(self):
        base = operation("plate", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        pocket = operation("pocket", rectangle(30, 0, 40, 24), z=20, op="cut", direction=(0, 0, -1), extent={"kind": "distance", "length": 6.0})
        pocket_section = section_parametric_model({"action": "section", "rebuild": request(base, pocket), "section_plane": {"axis": "x", "offset_mm": 30.0, "view_side": "positive"}})
        self.assertTrue(pocket_section["ok"], pocket_section.get("error"))
        pocket_loop_points = [point for loop in pocket_section["result"]["section_geometry"]["loops"] for point in loop]
        self.assertTrue(any(abs(point[0] + 14.0) <= 0.01 for point in pocket_loop_points))  # z=14 pocket floor in local -z coordinate

        side_plane = {"origin": [70.0, 0.0, 10.0], "u_axis": [0.0, 1.0, 0.0], "v_axis": [0.0, 0.0, 1.0], "normal": [1.0, 0.0, 0.0]}
        side = operation("side_blind", circle(0, 10, 4), op="cut", direction=(-1, 0, 0), extent={"kind": "distance", "length": 15.0})
        side["plane"] = side_plane
        side["source_feature_kind"] = "hole"
        side_section = section_parametric_model({"action": "section", "rebuild": request(base, side), "section_plane": {"axis": "y", "offset_mm": 0.0, "view_side": "positive"}})
        self.assertTrue(side_section["ok"], side_section.get("error"))
        side_points = [point for loop in side_section["result"]["section_geometry"]["loops"] for point in loop]
        self.assertTrue(any(abs(point[0] - 70.0) <= 0.01 for point in side_points))
        self.assertTrue(any(abs(point[0] - 55.0) <= 0.01 for point in side_points))

    def test_section_without_intersection_or_with_invalid_contract_fails_closed(self):
        base = operation("plate", rectangle(0, 0, 140, 100), extent={"kind": "distance", "length": 20.0})
        absent = section_parametric_model({"action": "section", "rebuild": request(base), "section_plane": {"axis": "x", "offset_mm": 1000.0, "view_side": "positive"}})
        self.assertFalse(absent["ok"])
        self.assertEqual(absent["error"]["stage"], "section_geometry")
        self.assertEqual(absent["error"]["code"], "SECTION_NO_INTERSECTION")
        invalid = section_parametric_model({"action": "section", "rebuild": request(base), "section_plane": {"axis": "diagonal", "offset_mm": 0.0, "view_side": "positive"}})
        self.assertFalse(invalid["ok"])
        self.assertEqual(invalid["error"]["code"], "SECTION_REQUEST_INVALID")

    def test_detached_add_cut_without_intersection_and_full_cut_fail_closed(self):
        plate = operation("plate", rectangle(0, 0, 100, 60), extent={"kind": "distance", "length": 10.0})
        detached = operation("detached", rectangle(0, 0, 10, 10), z=30, extent={"kind": "distance", "length": 5.0})
        result = rebuild_parametric_model(request(plate, detached))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "MULTI_BODY_NOT_ALLOWED")

        outside_cut = operation("outside_cut", circle(100, 0, 5), z=10, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
        result = rebuild_parametric_model(request(plate, outside_cut))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "CUT_NO_INTERSECTION")

        full_cut = operation("full_cut", circle(0, 0, 100), z=10, op="cut", direction=(0, 0, -1), extent={"kind": "through_all"})
        result = rebuild_parametric_model(request(plate, full_cut))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "CUT_REMOVES_BODY")

        first_cut = operation("first_cut", circle(0, 0, 2), op="cut", extent={"kind": "distance", "length": 2.0})
        result = rebuild_parametric_model(request(first_cut))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "NO_BODY")

    def test_sweep_add_supports_straight_arc_rectangle_and_c1_composite_paths(self):
        path_plane = copy.deepcopy(PLANE)
        straight = sweep_operation(
            "straight", circle(0, 0, 1.0), [line((0, 0), (0, 10))], path_plane,
            {"origin": [0, 0, 0], "u_axis": [-1, 0, 0], "v_axis": [0, 0, 1], "normal": [0, 1, 0]},
        )
        straight_result = succeed(request(straight))
        self.assertEqual(straight_result["solid_count"], 1)
        self.assertAlmostEqual(straight_result["spatial_evidence"]["volume_mm3"], math.pi * 10, places=5)
        self.assertEqual(straight_result["spatial_evidence"]["operation_features"], ["straight"])

        arc_path = [{"kind": "arc", "start": [10, 0], "end": [0, 10], "center": [0, 0], "radius": 10.0, "direction": "counter_clockwise"}]
        arc_plane = {"origin": [10, 0, 0], "u_axis": [-1, 0, 0], "v_axis": [0, 0, 1], "normal": [0, 1, 0]}
        arc = sweep_operation("arc", circle(0, 0, 1.0), arc_path, path_plane, arc_plane)
        arc_result = succeed(request(arc))
        self.assertAlmostEqual(arc_result["spatial_evidence"]["volume_mm3"], math.pi * 1.0**2 * 10.0 * math.pi / 2.0, places=4)
        self.assertIn("torus", {face["surface"]["kind"] for face in arc_result["spatial_evidence"]["faces"]})
        self.assertEqual(set(arc_result["spatial_evidence"]["views"]), {"front", "top", "left"})

        rectangle_path = sweep_operation("rect_arc", rectangle(0, 0, 2, 4), arc_path, path_plane, arc_plane)
        rectangle_result = succeed(request(rectangle_path))
        self.assertEqual(rectangle_result["solid_count"], 1)
        self.assertAlmostEqual(rectangle_result["spatial_evidence"]["volume_mm3"], 8.0 * 10.0 * math.pi / 2.0, places=3)
        self.assertIn("cylinder", {face["surface"]["kind"] for face in rectangle_result["spatial_evidence"]["faces"]})

        composite_path = [
            line((10, -5), (10, 0)),
            arc_path[0],
        ]
        composite = sweep_operation("composite", circle(0, 0, 1.0), composite_path, path_plane,
            {"origin": [10, -5, 0], "u_axis": [-1, 0, 0], "v_axis": [0, 0, 1], "normal": [0, 1, 0]})
        composite_result = succeed(request(composite))
        self.assertGreater(composite_result["spatial_evidence"]["volume_mm3"], arc_result["spatial_evidence"]["volume_mm3"])
        self.assertEqual(composite_result["solid_count"], 1)

    def test_sweep_cut_add_fusion_and_revolve_history_are_real_single_body_booleans(self):
        base = operation("base", rectangle(0, 0, 20, 20), extent={"kind": "distance", "length": 5.0})
        base["source_feature_id"] = "base"
        path_plane = {"origin": [0, 0, 2.5], "u_axis": [1, 0, 0], "v_axis": [0, 1, 0], "normal": [0, 0, 1]}
        arc_path = [{"kind": "arc", "start": [10, 0], "end": [0, 10], "center": [0, 0], "radius": 10.0, "direction": "counter_clockwise"}]
        profile_plane = {"origin": [10, 0, 2.5], "u_axis": [0, -1, 0], "v_axis": [0, 0, 1], "normal": [-1, 0, 0]}
        # The path is in XY; its CCW tangent at (10,0) is +Y, so the profile
        # normal is +Y. Use local U=-X and V=Z, a right-handed Y-normal frame.
        profile_plane["normal"] = [0, 1, 0]
        profile_plane["u_axis"] = [-1, 0, 0]
        add_sweep = sweep_operation("sweep_add", circle(0, 0, 1), arc_path, path_plane, profile_plane)
        fused = succeed(request(base, add_sweep))
        self.assertEqual(fused["solid_count"], 1)
        self.assertGreater(fused["spatial_evidence"]["volume_mm3"], 20 * 20 * 5)
        self.assertGreater(fused["bbox"]["x_max"], 10.0)

        cut_base = operation("cut_base", rectangle(0, 0, 40, 40), extent={"kind": "distance", "length": 20.0})
        cut_base["source_feature_id"] = "cut_base"
        cut_path_plane = {"origin": [0, 0, 10], "u_axis": [1, 0, 0], "v_axis": [0, 1, 0], "normal": [0, 0, 1]}
        cut_profile_plane = {"origin": [12, 0, 10], "u_axis": [-1, 0, 0], "v_axis": [0, 0, 1], "normal": [0, 1, 0]}
        cut_path = [{"kind": "arc", "start": [12, 0], "end": [0, 12], "center": [0, 0], "radius": 12.0, "direction": "counter_clockwise"}]
        sweep_cut = sweep_operation("sweep_cut", circle(0, 0, 2), cut_path, cut_path_plane, cut_profile_plane, op="cut")
        cut_result = succeed(request(cut_base, sweep_cut))
        self.assertEqual(cut_result["solid_count"], 1)
        self.assertLess(cut_result["spatial_evidence"]["volume_mm3"], 40 * 40 * 20)
        section = section_parametric_model({"action": "section", "rebuild": request(cut_base, sweep_cut), "section_plane": {"axis": "x", "offset_mm": 0.0, "view_side": "positive"}})
        self.assertTrue(section["ok"], section.get("error"))
        self.assertGreaterEqual(len(section["result"]["section_geometry"]["loops"]), 2)

        # Revolve body followed by the same general curved Sweep CUT path.
        revolve = {
            "kind": "revolve",
            "data": {
                "feature_id": "revolve_body", "sketch_id": "cyl_profile", "operation": "add",
                "plane": {"origin": [0,0,0], "u_axis": [1,0,0], "v_axis": [0,0,1], "normal": [0,-1,0]},
                "loops": [rectangle(7.5, 10, 15, 20)],
                "profile_entity_ids": [[f"rev-{i}" for i in range(4)]],
                "axis_origin": [0,0,0], "direction": [0,0,1], "angle": {"kind": "full_360"}, "orientation": "forward",
            },
        }
        rev_path_plane = {"origin": [0,0,10], "u_axis": [1,0,0], "v_axis": [0,1,0], "normal": [0,0,1]}
        rev_profile_plane = {"origin": [6,0,10], "u_axis": [-1,0,0], "v_axis": [0,0,1], "normal": [0,1,0]}
        rev_curve = [{"kind": "arc", "start": [6,0], "end": [0,6], "center": [0,0], "radius": 6, "direction": "counter_clockwise"}]
        rev_cut = sweep_operation("rev_sweep_cut", circle(0,0,1), rev_curve, rev_path_plane, rev_profile_plane, op="cut")
        mixed = succeed(request(revolve, rev_cut))
        self.assertEqual(mixed["solid_count"], 1)
        self.assertLess(mixed["spatial_evidence"]["volume_mm3"], 3.141592653589793 * 15**2 * 20)

    def test_sweep_negative_matrix_and_rebuild_determinism(self):
        path_plane = copy.deepcopy(PLANE)
        arc_path = [{"kind": "arc", "start": [10, 0], "end": [0, 10], "center": [0, 0], "radius": 10.0, "direction": "counter_clockwise"}]
        profile_plane = {"origin": [10,0,0], "u_axis": [-1,0,0], "v_axis": [0,0,1], "normal": [0,1,0]}
        good = sweep_operation("valid", circle(0,0,1), arc_path, path_plane, profile_plane)
        variants=[]
        open_profile=copy.deepcopy(good);open_profile["data"]["loops"]=[[line((0,0),(1,0))]];open_profile["data"]["profile_entity_ids"]=[["open"]];variants.append(("OPEN_PROFILE",open_profile))
        discontinuous=copy.deepcopy(good);discontinuous["data"]["path"]=[line((0,0),(1,0)),line((2,0),(3,0))];discontinuous["data"]["path_entity_ids"]=["a","b"];variants.append(("PATH_DISCONTINUOUS",discontinuous))
        degenerate=copy.deepcopy(good);degenerate["data"]["path"]=[line((0,0),(0,0))];degenerate["data"]["path_entity_ids"]=["bad"];variants.append(("PATH_DEGENERATE",degenerate))
        wrong_frame=copy.deepcopy(good);wrong_frame["data"]["plane"]["u_axis"]=[1,0,0];wrong_frame["data"]["plane"]["normal"]=[0,-1,0];variants.append(("SWEEP_PROFILE_ORIENTATION_INVALID",wrong_frame))
        self_crossing=[
            {"kind":"arc","start":[0,0],"end":[-2,0],"center":[-1,0],"radius":1,"direction":"counter_clockwise"},
            {"kind":"arc","start":[-2,0],"end":[0,0],"center":[-1,0],"radius":1,"direction":"counter_clockwise"},
            {"kind":"arc","start":[0,0],"end":[2,0],"center":[1,0],"radius":1,"direction":"clockwise"},
        ]
        crossing=copy.deepcopy(good);crossing["data"]["path"]=self_crossing;crossing["data"]["path_entity_ids"]=["a","b","c"];crossing["data"]["path_plane"]=PLANE;crossing["data"]["plane"]={"origin":[0,0,0],"u_axis":[-1,0,0],"v_axis":[0,0,1],"normal":[0,1,0]};variants.append(("PATH_SELF_INTERSECTION",crossing))
        for expected, operation_ in variants:
            with self.subTest(expected=expected):
                result=rebuild_parametric_model(request(operation_))
                self.assertFalse(result["ok"])
                self.assertEqual(result["error"]["code"],expected)

        detached_base=operation("detached_base",rectangle(0,0,10,10),z=30,extent={"kind":"distance","length":5})
        detached_base["source_feature_id"]="detached_base"
        detached=copy.deepcopy(good);detached["data"]["plane"]["origin"]=[10,0,0]
        result=rebuild_parametric_model(request(detached_base,detached))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"],"DETACHED_ADD")

        base=operation("base",rectangle(0,0,40,40),extent={"kind":"distance","length":20})
        base["source_feature_id"]="base"
        outside=copy.deepcopy(good);outside["data"]["operation"]="cut";outside["data"]["path_plane"]["origin"]=[100,0,0];outside["data"]["plane"]["origin"]=[110,0,0]
        result=rebuild_parametric_model(request(base,outside))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"],"SWEEP_CUT_NO_INTERSECTION")

        baseline=succeed(request(good))
        path_edit=copy.deepcopy(good);path_edit["data"]["path"][0]["radius"]=12;path_edit["data"]["path"][0]["start"]=[12,0];path_edit["data"]["path"][0]["end"]=[0,12];path_edit["data"]["path"][0]["center"]=[0,0];path_edit["data"]["plane"]["origin"]=[12,0,0]
        edited=succeed(request(path_edit))
        self.assertNotEqual(baseline["bbox"],edited["bbox"])
        profile_edit=copy.deepcopy(good);profile_edit["data"]["loops"]=[circle(0,0,2)]
        profile_edit["data"]["profile_entity_ids"]=[["larger-profile"]]
        wider=succeed(request(profile_edit))
        self.assertGreater(wider["spatial_evidence"]["volume_mm3"],baseline["spatial_evidence"]["volume_mm3"])
        repeat=succeed(request(good))
        self.assertEqual(baseline["bbox"],repeat["bbox"])
        self.assertEqual(baseline["spatial_evidence"]["volume_mm3"],repeat["spatial_evidence"]["volume_mm3"])
        self.assertEqual(baseline["spatial_evidence"]["edges"],repeat["spatial_evidence"]["edges"])

    def test_g7b_ruled_three_section_loft_add_uses_all_sections_and_rebuilds_deterministically(self):
        sections = [
            loft_section("section_a", 0.0, circle(0, 0, 10.0)),
            loft_section("section_b", 30.0, circle(5, 0, 20.0)),
            loft_section("section_c", 70.0, circle(-3, 2, 13.0)),
        ]
        value = request(loft_operation("three_section_loft", sections))
        result = succeed(value)
        self.assertEqual(result["solid_count"], 1)
        self.assertEqual(result["body_count"], 1)
        self.assertEqual(result["operation_timings"][0]["kind"], "loft_add")
        self.assertEqual(result["spatial_evidence"]["operation_features"], ["three_section_loft"])
        self.assertGreater(result["spatial_evidence"]["volume_mm3"], 0.0)
        self.assertAlmostEqual(result["bbox"]["z_min"], 0.0, delta=1e-6)
        self.assertAlmostEqual(result["bbox"]["z_max"], 70.0, delta=1e-6)

        shape = _rebuild(_parse_request(value), shape_only=True)
        # The intermediate Sketch is measured at its own station; the next
        # section is independently witnessed between middle and top stations.
        self.assertTrue(shape.isInside(cq.Vector(24.9, 0.0, 30.0), 1e-7))
        self.assertFalse(shape.isInside(cq.Vector(25.1, 0.0, 30.0), 1e-7))
        # At z=55 the ruled generator is 5/8 of the way from
        # (center=(5,0), r=20) to (center=(-3,2), r=13):
        # center=(0,1.25), r=15.625.
        self.assertTrue(shape.isInside(cq.Vector(15.4, 1.25, 55.0), 1e-7))
        self.assertFalse(shape.isInside(cq.Vector(15.8, 1.25, 55.0), 1e-7))

        edited_sections = copy.deepcopy(sections)
        edited_sections[1]["loops"] = [circle(5, 0, 16.0)]
        edited = succeed(request(loft_operation("three_section_loft", edited_sections)))
        self.assertNotEqual(result["spatial_evidence"]["volume_mm3"], edited["spatial_evidence"]["volume_mm3"])

        repeat = succeed(value)
        self.assertEqual(result["bbox"], repeat["bbox"])
        self.assertEqual(result["spatial_evidence"]["volume_mm3"], repeat["spatial_evidence"]["volume_mm3"])
        self.assertEqual(len(result["spatial_evidence"]["faces"]), len(repeat["spatial_evidence"]["faces"]))

    def test_g7b_ruled_rectangle_loft_and_non_z_principal_axis(self):
        bottom = loft_section("rect_bottom", 0.0, rectangle(0, 0, 20, 10))
        top = loft_section("rect_top", 20.0, rectangle(3, 2, 30, 14))
        rectangular = succeed(request(loft_operation("rect_loft", [bottom, top])))
        self.assertEqual(rectangular["solid_count"], 1)
        self.assertAlmostEqual(rectangular["bbox"]["z_max"], 20.0, delta=1e-6)
        self.assertGreater(rectangular["spatial_evidence"]["volume_mm3"], 6000.0)

        x_plane = {
            "origin": [0.0, 0.0, 0.0], "u_axis": [0.0, 1.0, 0.0],
            "v_axis": [0.0, 0.0, 1.0], "normal": [1.0, 0.0, 0.0],
        }
        end_plane = {**x_plane, "origin": [25.0, 0.0, 0.0]}
        x_sections = [
            {"sketch_id": "x_start", "plane": x_plane, "loops": [circle(0, 0, 5)], "profile_entity_ids": [["x_start_circle"]]},
            {"sketch_id": "x_end", "plane": end_plane, "loops": [circle(2, 0, 8)], "profile_entity_ids": [["x_end_circle"]]},
        ]
        x_result = succeed(request(loft_operation("x_axis_loft", x_sections)))
        self.assertEqual(x_result["solid_count"], 1)
        self.assertAlmostEqual(x_result["bbox"]["x_min"], 0.0, delta=1e-6)
        self.assertAlmostEqual(x_result["bbox"]["x_max"], 25.0, delta=1e-6)

    def test_g7b_loft_add_to_body_cut_cavity_and_real_section_geometry(self):
        base = operation("base", rectangle(0, 0, 100, 80), extent={"kind": "distance", "length": 40.0})
        base["source_feature_id"] = "base"
        add_sections = [
            loft_section("add_a", 40.0, circle(0, 0, 8)),
            loft_section("add_b", 55.0, circle(1, 0, 10)),
            loft_section("add_c", 70.0, circle(2, 1, 7)),
        ]
        added = succeed(request(base, loft_operation("loft_add", add_sections)))
        self.assertEqual(added["solid_count"], 1)
        self.assertEqual(added["spatial_evidence"]["operation_features"], ["base", "loft_add"])
        self.assertGreater(added["spatial_evidence"]["volume_mm3"], 100 * 80 * 40)

        cut_sections = [
            loft_section("cut_a", 2.0, circle(0, 0, 6)),
            loft_section("cut_b", 20.0, circle(0, 0, 10)),
            loft_section("cut_c", 38.0, circle(0, 0, 7)),
        ]
        cut_value = request(base, loft_operation("loft_cut", cut_sections, op="cut"))
        cut = succeed(cut_value)
        self.assertEqual(cut["solid_count"], 1)
        self.assertEqual(cut["operation_timings"][-1]["kind"], "loft_cut")
        self.assertLess(cut["spatial_evidence"]["volume_mm3"], 100 * 80 * 40)
        shape = _rebuild(_parse_request(cut_value), shape_only=True)
        self.assertFalse(shape.isInside(cq.Vector(0, 0, 20), 1e-7))
        self.assertTrue(shape.isInside(cq.Vector(10.2, 0, 20), 1e-7))

        section = section_parametric_model({
            "action": "section",
            "rebuild": cut_value,
            "section_plane": {"axis": "y", "offset_mm": 0.0, "view_side": "positive"},
        })
        self.assertTrue(section["ok"], section.get("error"))
        geometry = section["result"]["section_geometry"]
        self.assertGreaterEqual(len(geometry["loops"]), 2)
        self.assertGreaterEqual(geometry["edge_count"], 8)
        self.assertEqual(geometry["sample_chord_tolerance_mm"], 0.01)

    def test_g7b_loft_validation_and_boolean_failures_are_closed(self):
        good = [
            loft_section("a", 0.0, circle(0, 0, 5)),
            loft_section("b", 10.0, circle(0, 0, 8)),
        ]
        invalid = [
            (loft_operation("one", good[:1]), "LOFT_SECTION_COUNT_INVALID"),
            (loft_operation("reverse_order", [good[1], good[0]]), "LOFT_SECTION_ORDER_INVALID"),
            (loft_operation("mode", good, mode="smooth"), "LOFT_MODE_UNSUPPORTED"),
            (loft_operation("inner", [
                {**good[0], "loops": [good[0]["loops"][0], circle(0, 0, 1)]},
                good[1],
            ]), "LOFT_INNER_LOOPS_UNSUPPORTED"),
            (loft_operation("different_topology", [good[0], loft_section("rect", 10.0, rectangle(0, 0, 10, 8))]), "LOFT_PROFILE_INCOMPATIBLE"),
        ]
        for operation_value, expected in invalid:
            with self.subTest(code=expected):
                result = rebuild_parametric_model(request(operation_value))
                self.assertFalse(result["ok"])
                self.assertEqual(result["error"]["code"], expected)

        collinear = [line((0, 0), (4, 0)), line((4, 0), (8, 0)), line((8, 0), (0, 0))]
        degenerate = rebuild_parametric_model(request(loft_operation("degenerate", [
            loft_section("degenerate_a", 0.0, collinear),
            loft_section("degenerate_b", 10.0, collinear),
        ])))
        self.assertFalse(degenerate["ok"])
        self.assertEqual(degenerate["error"]["code"], "LOFT_PROFILE_DEGENERATE")

        crossing = [
            line((0, 0), (4, 4)), line((4, 4), (0, 4)),
            line((0, 4), (3, 0)), line((3, 0), (0, 0)),
        ]
        self_intersection = rebuild_parametric_model(request(loft_operation("self_intersection", [
            loft_section("crossing_a", 0.0, crossing),
            loft_section("crossing_b", 10.0, crossing),
        ])))
        self.assertFalse(self_intersection["ok"])
        self.assertEqual(self_intersection["error"]["code"], "SELF_INTERSECTION")

        base = operation("plate", rectangle(0, 0, 40, 40), extent={"kind": "distance", "length": 20.0})
        detached_sections = [
            loft_section("far_a", 30.0, circle(100, 0, 5)),
            loft_section("far_b", 40.0, circle(100, 0, 8)),
        ]
        detached = rebuild_parametric_model(request(base, loft_operation("detached", detached_sections)))
        self.assertFalse(detached["ok"])
        self.assertEqual(detached["error"]["code"], "LOFT_ADD_DETACHED")

        no_hit_sections = [
            loft_section("miss_a", 0.0, circle(100, 0, 5)),
            loft_section("miss_b", 20.0, circle(100, 0, 8)),
        ]
        no_hit = rebuild_parametric_model(request(base, loft_operation("miss", no_hit_sections, op="cut")))
        self.assertFalse(no_hit["ok"])
        self.assertEqual(no_hit["error"]["code"], "LOFT_CUT_NO_INTERSECTION")

        full_sections = [
            loft_section("full_a", 0.0, circle(0, 0, 100)),
            loft_section("full_b", 20.0, circle(0, 0, 100)),
        ]
        full_cut = rebuild_parametric_model(request(base, loft_operation("full", full_sections, op="cut")))
        self.assertFalse(full_cut["ok"])
        self.assertEqual(full_cut["error"]["code"], "LOFT_CUT_REMOVES_BODY")

    def test_profile_and_extent_errors_have_stable_codes(self):
        broken = operation("open", [line((0, 0), (20, 0)), line((20, 0), (20, 20))])
        result = rebuild_parametric_model(request(broken))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "OPEN_PROFILE")

        bad_extent = operation("bad", rectangle(0, 0, 10, 10), extent={"kind": "distance", "length": 0.0})
        result = rebuild_parametric_model(request(bad_extent))
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "INVALID_EXTRUDE_EXTENT")


if __name__ == "__main__":
    unittest.main()
