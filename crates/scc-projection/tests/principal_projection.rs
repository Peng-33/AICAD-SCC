//! Phase 9.4 principal-view lowering tests.
//!
//! The spatial input is decoded from JSON so these tests exercise the same
//! serde boundary that makes the projection preflight necessary.

use scc_projection::{
    project_principal_views, PrincipalViewKind, ProjectedGeometry, ProjectionError,
    ProjectionPrimitiveId, ProjectionPrimitiveRole, SpatialElementRef,
};
use scc_spatial::SpatialCutId;
use serde_json::{json, Value};

const SPATIAL_FIXTURE: &str = r#"{
    "body": {
        "footprint": {
            "Rectangle": {
                "origin": { "x": -60.0, "y": -40.0 },
                "width": 120.0,
                "height": 80.0
            }
        },
        "z": { "min": 0.0, "max": 10.0 },
        "source_feature": "plate",
        "source_entity": "plate"
    },
    "cuts": {
        "a_hole:cut:0000": {
            "center": { "x": 0.0, "y": 0.0 },
            "radius": 5.0,
            "z": { "min": 0.0, "max": 10.0 },
            "kind": { "kind": "through_all" },
            "source_feature": "a_hole",
            "source_entity": "a_hole"
        }
    },
    "bounds": {
        "min_x": -60.0, "max_x": 60.0,
        "min_y": -40.0, "max_y": 40.0,
        "min_z": 0.0, "max_z": 10.0
    },
    "construction_circles": {},
    "source": { "geometry_source_root": 1, "geometry_source_step": 7 }
}"#;

fn spatial() -> scc_spatial::SpatialModel {
    serde_json::from_str(SPATIAL_FIXTURE).expect("valid spatial fixture")
}

fn body_only_spatial() -> scc_spatial::SpatialModel {
    let mut value = spatial_value();
    value["cuts"] = json!({});
    spatial_from_value(value)
}

fn spatial_value() -> Value {
    serde_json::from_str(SPATIAL_FIXTURE).expect("valid spatial JSON")
}

fn spatial_from_value(value: Value) -> scc_spatial::SpatialModel {
    serde_json::from_value(value).expect("valid spatial value")
}

fn spatial_with_cut_count(count: usize) -> scc_spatial::SpatialModel {
    let mut value = spatial_value();
    let mut cuts = serde_json::Map::new();
    for index in 0..count {
        cuts.insert(
            format!("pattern:cut:{index:04}"),
            json!({
                "center": { "x": index as f64 * 3.0, "y": index as f64 * -2.0 },
                "radius": 2.0,
                "z": { "min": 0.0, "max": 10.0 },
                "kind": { "kind": "through_all" },
                "source_feature": "pattern",
                "source_entity": format!("pattern[{index}]")
            }),
        );
    }
    value["cuts"] = Value::Object(cuts);
    spatial_from_value(value)
}

fn primitive<'a>(
    model: &'a scc_projection::ProjectionModel,
    view: PrincipalViewKind,
    source: &str,
    index: usize,
) -> &'a scc_projection::ProjectionPrimitive {
    let id = ProjectionPrimitiveId(format!(
        "{}:{}:p:{index:04}",
        match view {
            PrincipalViewKind::Front => "front",
            PrincipalViewKind::Top => "top",
            PrincipalViewKind::Left => "left",
        },
        source
    ));
    &model.views[&view].primitives[&id]
}

fn assert_depth(primitive: &scc_projection::ProjectionPrimitive, min: f64, max: f64) {
    assert_eq!(primitive.depth.min, min);
    assert_eq!(primitive.depth.max, max);
}

#[test]
fn body_generates_eight_segments_in_each_canonical_view() {
    let model = project_principal_views(&body_only_spatial()).expect("projection succeeds");
    assert_eq!(
        model.views.keys().copied().collect::<Vec<_>>(),
        vec![
            PrincipalViewKind::Front,
            PrincipalViewKind::Top,
            PrincipalViewKind::Left
        ]
    );

    for view in [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ] {
        let view_model = &model.views[&view];
        assert_eq!(view_model.primitives.len(), 8);
        assert!(view_model
            .primitives
            .values()
            .all(|primitive| primitive.role == ProjectionPrimitiveRole::BodyBoundary));
        assert!(view_model
            .primitives
            .keys()
            .all(|id| { model.element_for_primitive(id) == Some(&SpatialElementRef::Body) }));

        for index in 0..4 {
            assert_eq!(
                primitive(&model, view, "body", index).geometry,
                primitive(&model, view, "body", index + 4).geometry,
                "near/far body edge geometry must remain coincident"
            );
            assert_ne!(
                primitive(&model, view, "body", index).depth,
                primitive(&model, view, "body", index + 4).depth,
                "near/far body edge depth evidence must remain distinct"
            );
        }
    }

    match &primitive(&model, PrincipalViewKind::Front, "body", 0).geometry {
        ProjectedGeometry::Segment { start, end } => {
            assert_eq!((start.u, start.v), (-60.0, -40.0));
            assert_eq!((end.u, end.v), (60.0, -40.0));
            assert_depth(
                primitive(&model, PrincipalViewKind::Front, "body", 0),
                10.0,
                10.0,
            );
        }
        other => panic!("expected body segment, got {other:?}"),
    }

    match &primitive(&model, PrincipalViewKind::Top, "body", 0).geometry {
        ProjectedGeometry::Segment { start, end } => {
            assert_eq!((start.u, start.v), (-60.0, -10.0));
            assert_eq!((end.u, end.v), (60.0, -10.0));
            assert_depth(
                primitive(&model, PrincipalViewKind::Top, "body", 0),
                40.0,
                40.0,
            );
        }
        other => panic!("expected body segment, got {other:?}"),
    }

    match &primitive(&model, PrincipalViewKind::Left, "body", 0).geometry {
        ProjectedGeometry::Segment { start, end } => {
            assert_eq!((start.u, start.v), (0.0, -40.0));
            assert_eq!((end.u, end.v), (10.0, -40.0));
            assert_depth(
                primitive(&model, PrincipalViewKind::Left, "body", 0),
                60.0,
                60.0,
            );
        }
        other => panic!("expected body segment, got {other:?}"),
    }
}

#[test]
fn one_cut_generates_two_four_four_exact_candidates() {
    let model = project_principal_views(&spatial()).expect("projection succeeds");
    let cut = "a_hole:cut:0000";
    assert_eq!(model.views[&PrincipalViewKind::Front].primitives.len(), 10);
    assert_eq!(model.views[&PrincipalViewKind::Top].primitives.len(), 12);
    assert_eq!(model.views[&PrincipalViewKind::Left].primitives.len(), 12);
    assert_eq!(model.sources.iter().count(), 34);

    for index in 0..2 {
        let item = primitive(&model, PrincipalViewKind::Front, cut, index);
        match &item.geometry {
            ProjectedGeometry::Circle { center, radius } => {
                assert_eq!((center.u, center.v, *radius), (0.0, 0.0, 5.0));
            }
            other => panic!("expected front circle, got {other:?}"),
        }
        assert_depth(
            item,
            if index == 0 { 0.0 } else { 10.0 },
            if index == 0 { 0.0 } else { 10.0 },
        );
    }

    let top_expected = [
        ((-5.0, 0.0), (5.0, 0.0), (-5.0, 5.0)),
        ((-5.0, -10.0), (5.0, -10.0), (-5.0, 5.0)),
        ((-5.0, 0.0), (-5.0, -10.0), (0.0, 0.0)),
        ((5.0, 0.0), (5.0, -10.0), (0.0, 0.0)),
    ];
    for (index, (start, end, depth)) in top_expected.into_iter().enumerate() {
        match &primitive(&model, PrincipalViewKind::Top, cut, index).geometry {
            ProjectedGeometry::Segment {
                start: actual_start,
                end: actual_end,
            } => {
                assert_eq!((actual_start.u, actual_start.v), start);
                assert_eq!((actual_end.u, actual_end.v), end);
            }
            other => panic!("expected top segment, got {other:?}"),
        }
        assert_depth(
            primitive(&model, PrincipalViewKind::Top, cut, index),
            depth.0,
            depth.1,
        );
    }

    let left_expected = [
        ((0.0, -5.0), (0.0, 5.0), (-5.0, 5.0)),
        ((10.0, -5.0), (10.0, 5.0), (-5.0, 5.0)),
        ((0.0, -5.0), (10.0, -5.0), (0.0, 0.0)),
        ((0.0, 5.0), (10.0, 5.0), (0.0, 0.0)),
    ];
    for (index, (start, end, depth)) in left_expected.into_iter().enumerate() {
        match &primitive(&model, PrincipalViewKind::Left, cut, index).geometry {
            ProjectedGeometry::Segment {
                start: actual_start,
                end: actual_end,
            } => {
                assert_eq!((actual_start.u, actual_start.v), start);
                assert_eq!((actual_end.u, actual_end.v), end);
            }
            other => panic!("expected left segment, got {other:?}"),
        }
        assert_depth(
            primitive(&model, PrincipalViewKind::Left, cut, index),
            depth.0,
            depth.1,
        );
    }
}

#[test]
fn blind_interval_is_consumed_directly_and_kind_is_not_recomputed() {
    let mut value = spatial_value();
    value["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 3.0, "max": 8.0 });
    value["cuts"]["a_hole:cut:0000"]["kind"] = json!({ "kind": "blind", "start_face": "opposite" });
    let model = project_principal_views(&spatial_from_value(value)).expect("projection succeeds");
    assert_depth(
        primitive(&model, PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        3.0,
        3.0,
    );
    assert_depth(
        primitive(&model, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        8.0,
        8.0,
    );
    assert_eq!(model.views[&PrincipalViewKind::Top].primitives.len(), 12);
}

#[test]
fn coincidences_and_outside_body_cuts_are_preserved_without_clipping() {
    let mut value = spatial_value();
    value["cuts"]["a_hole:cut:0000"]["center"] = json!({ "x": 100.0, "y": 0.0 });
    value["cuts"]["b_hole:cut:0000"] = json!({
        "center": { "x": 100.0, "y": 0.0 },
        "radius": 5.0,
        "z": { "min": 2.0, "max": 8.0 },
        "kind": { "kind": "blind", "start_face": "base" },
        "source_feature": "b_hole",
        "source_entity": "b_hole"
    });
    let model = project_principal_views(&spatial_from_value(value)).expect("projection succeeds");
    assert_eq!(model.views[&PrincipalViewKind::Front].primitives.len(), 12);
    assert_eq!(model.views[&PrincipalViewKind::Top].primitives.len(), 16);
    assert_eq!(model.views[&PrincipalViewKind::Left].primitives.len(), 16);
    assert_eq!(model.views[&PrincipalViewKind::Front].bounds.max_u, 105.0);

    let first = model.primitives_for_element(&SpatialElementRef::Cut {
        id: SpatialCutId("a_hole:cut:0000".into()),
    });
    assert_eq!(first.values().map(|ids| ids.len()).sum::<usize>(), 10);
    let second = model.primitives_for_element(&SpatialElementRef::Cut {
        id: SpatialCutId("b_hole:cut:0000".into()),
    });
    assert_eq!(second.values().map(|ids| ids.len()).sum::<usize>(), 10);
}

#[test]
fn construction_geometry_is_inert_and_does_not_change_ids_or_bounds() {
    let base = project_principal_views(&spatial()).expect("projection succeeds");
    let mut value = spatial_value();
    value["construction_circles"]["construction:reference"] = json!("reference_feature");
    let with_construction =
        project_principal_views(&spatial_from_value(value)).expect("projection succeeds");
    assert_eq!(base, with_construction);
    assert_eq!(
        serde_json::to_string(&base).expect("serialize"),
        serde_json::to_string(&with_construction).expect("serialize")
    );
}

#[test]
fn malformed_spatial_models_and_arithmetic_overflow_fail_closed() {
    let mut zero_width = spatial_value();
    zero_width["body"]["footprint"]["Rectangle"]["width"] = json!(0.0);
    assert_eq!(
        project_principal_views(&spatial_from_value(zero_width))
            .expect_err("zero width must fail")
            .code(),
        "SCC-PROJ-007"
    );

    let mut mismatch = spatial_value();
    mismatch["bounds"]["max_x"] = json!(61.0);
    assert_eq!(
        project_principal_views(&spatial_from_value(mismatch))
            .expect_err("bounds mismatch must fail")
            .code(),
        "SCC-PROJ-007"
    );

    let mut outside_z = spatial_value();
    outside_z["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 0.0, "max": 11.0 });
    assert_eq!(
        project_principal_views(&spatial_from_value(outside_z))
            .expect_err("cut outside body must fail")
            .code(),
        "SCC-PROJ-007"
    );

    let mut overflow = spatial_value();
    overflow["cuts"]["a_hole:cut:0000"]["center"]["x"] = json!(f64::MAX);
    overflow["cuts"]["a_hole:cut:0000"]["radius"] = json!(f64::MAX);
    assert_eq!(
        project_principal_views(&spatial_from_value(overflow))
            .expect_err("cut arithmetic overflow must fail")
            .code(),
        "SCC-PROJ-002"
    );
}

#[test]
fn serde_is_deterministic_and_integrity_rejects_tampered_bounds() {
    let model = project_principal_views(&spatial()).expect("projection succeeds");
    let encoded = serde_json::to_string_pretty(&model).expect("serialize");
    let decoded: scc_projection::ProjectionModel =
        serde_json::from_str(&encoded).expect("deserialize");
    decoded.validate_against(&spatial()).expect("valid model");
    assert_eq!(
        serde_json::to_string_pretty(&decoded).expect("serialize"),
        encoded
    );

    let mut tampered: Value = serde_json::from_str(&encoded).expect("value");
    tampered["views"]["front"]["bounds"]["max_u"] = json!(61.0);
    let decoded_tampered: scc_projection::ProjectionModel =
        serde_json::from_value(tampered).expect("schema remains decodable");
    assert!(matches!(
        decoded_tampered.validate_integrity(),
        Err(ProjectionError::InvalidProjectionBounds { .. })
    ));
}

#[test]
fn pattern_counts_are_uniform_over_spatial_cuts() {
    for (count, expected_front, expected_top, expected_left) in [
        (8, 24, 40, 40), // circular pattern
        (6, 20, 32, 32), // 2 x 3 grid
        (4, 16, 24, 24), // corner pattern
    ] {
        let model =
            project_principal_views(&spatial_with_cut_count(count)).expect("projection succeeds");
        assert_eq!(
            model.views[&PrincipalViewKind::Front].primitives.len(),
            expected_front
        );
        assert_eq!(
            model.views[&PrincipalViewKind::Top].primitives.len(),
            expected_top
        );
        assert_eq!(
            model.views[&PrincipalViewKind::Left].primitives.len(),
            expected_left
        );
        assert_eq!(model.sources.iter().count(), 24 + count * 10);
    }
}

#[test]
fn thickness_and_resolved_blind_intervals_propagate_without_pattern_recomputation() {
    let base = project_principal_views(&spatial()).expect("projection succeeds");
    let mut changed = spatial_value();
    changed["body"]["z"]["max"] = json!(12.0);
    changed["bounds"]["max_z"] = json!(12.0);
    changed["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 0.0, "max": 12.0 });
    let changed =
        project_principal_views(&spatial_from_value(changed)).expect("projection succeeds");

    // Front XY geometry stays fixed while the two rim depths follow cut.z.
    for index in 0..8 {
        assert_eq!(
            primitive(&base, PrincipalViewKind::Front, "body", index).geometry,
            primitive(&changed, PrincipalViewKind::Front, "body", index).geometry
        );
    }
    assert_eq!(
        base.views[&PrincipalViewKind::Front]
            .primitives
            .keys()
            .collect::<Vec<_>>(),
        changed.views[&PrincipalViewKind::Front]
            .primitives
            .keys()
            .collect::<Vec<_>>()
    );
    assert_depth(
        primitive(&changed, PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        0.0,
        0.0,
    );
    assert_depth(
        primitive(&changed, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        12.0,
        12.0,
    );
    assert_eq!(base.views[&PrincipalViewKind::Top].bounds.min_v, -10.0);
    assert_eq!(changed.views[&PrincipalViewKind::Top].bounds.min_v, -12.0);
    assert_eq!(changed.views[&PrincipalViewKind::Left].bounds.max_u, 12.0);

    // A base blind interval remains [0, 4] even after body thickness grows.
    let mut blind_base = spatial_value();
    blind_base["body"]["z"]["max"] = json!(12.0);
    blind_base["bounds"]["max_z"] = json!(12.0);
    blind_base["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 0.0, "max": 4.0 });
    blind_base["cuts"]["a_hole:cut:0000"]["kind"] =
        json!({ "kind": "blind", "start_face": "base" });
    let blind_base = project_principal_views(&spatial_from_value(blind_base))
        .expect("base blind projection succeeds");
    assert_depth(
        primitive(&blind_base, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        4.0,
        4.0,
    );

    // An opposite blind interval carries its already-resolved [8, 12]
    // interval directly; no depth is inferred from SpatialCutKind.
    let mut blind_opposite = spatial_value();
    blind_opposite["body"]["z"]["max"] = json!(12.0);
    blind_opposite["bounds"]["max_z"] = json!(12.0);
    blind_opposite["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 8.0, "max": 12.0 });
    blind_opposite["cuts"]["a_hole:cut:0000"]["kind"] =
        json!({ "kind": "blind", "start_face": "opposite" });
    let blind_opposite = project_principal_views(&spatial_from_value(blind_opposite))
        .expect("opposite blind projection succeeds");
    assert_depth(
        primitive(
            &blind_opposite,
            PrincipalViewKind::Front,
            "a_hole:cut:0000",
            0,
        ),
        8.0,
        8.0,
    );
    assert_eq!(
        primitive(
            &blind_opposite,
            PrincipalViewKind::Top,
            "a_hole:cut:0000",
            1
        )
        .geometry,
        ProjectedGeometry::Segment {
            start: scc_projection::ProjectedPoint2 { u: -5.0, v: -12.0 },
            end: scc_projection::ProjectedPoint2 { u: 5.0, v: -12.0 },
        }
    );
}

#[test]
fn integrity_rejects_mismatched_view_kind_and_canonical_frame() {
    let model = project_principal_views(&spatial()).expect("projection succeeds");
    let encoded = serde_json::to_string(&model).expect("serialize");
    let mut value: Value = serde_json::from_str(&encoded).expect("value");

    // Keep a valid Top frame, but attach it to the `front` map key. This is
    // exactly the kind of serde-tampered model that axis-only validation
    // would previously accept.
    value["views"]["front"]["kind"] = json!("top");
    value["views"]["front"]["frame"] = value["views"]["top"]["frame"].clone();
    let tampered: scc_projection::ProjectionModel =
        serde_json::from_value(value).expect("schema remains decodable");
    assert!(matches!(
        tampered.validate_integrity(),
        Err(ProjectionError::InvalidProjectionGeometry {
            reason: scc_projection::InvalidProjectionGeometryReason::NonCanonicalFrame
        })
    ));
}

fn composite_spatial() -> scc_spatial::SpatialModel {
    spatial_from_value(json!({
        "body": {
            "footprint": {
                "Composite": {
                    "segments": [
                        {
                            "kind": "line",
                            "start": {"x": -40.0, "y": -20.0},
                            "end": {"x": 20.0, "y": -20.0},
                            "source_entity": "edge_0",
                            "segment_index": 0
                        },
                        {
                            "kind": "arc",
                            "start": {"x": 20.0, "y": -20.0},
                            "end": {"x": 40.0, "y": 0.0},
                            "center": {"x": 20.0, "y": 0.0},
                            "radius": 20.0,
                            "direction": "counter_clockwise",
                            "source_entity": "edge_1",
                            "segment_index": 1
                        },
                        {
                            "kind": "arc",
                            "start": {"x": 40.0, "y": 0.0},
                            "end": {"x": 20.0, "y": 20.0},
                            "center": {"x": 20.0, "y": 0.0},
                            "radius": 20.0,
                            "direction": "counter_clockwise",
                            "source_entity": "edge_2",
                            "segment_index": 2
                        },
                        {
                            "kind": "line",
                            "start": {"x": 20.0, "y": 20.0},
                            "end": {"x": -40.0, "y": 20.0},
                            "source_entity": "edge_3",
                            "segment_index": 3
                        },
                        {
                            "kind": "arc",
                            "start": {"x": -40.0, "y": 20.0},
                            "end": {"x": -60.0, "y": 0.0},
                            "center": {"x": -40.0, "y": 0.0},
                            "radius": 20.0,
                            "direction": "counter_clockwise",
                            "source_entity": "edge_4",
                            "segment_index": 4
                        },
                        {
                            "kind": "arc",
                            "start": {"x": -60.0, "y": 0.0},
                            "end": {"x": -40.0, "y": -20.0},
                            "center": {"x": -40.0, "y": 0.0},
                            "radius": 20.0,
                            "direction": "counter_clockwise",
                            "source_entity": "edge_5",
                            "segment_index": 5
                        }
                    ]
                }
            },
            "z": {"min": 0.0, "max": 8.0},
            "source_feature": "composite_base",
            "source_entity": "edge_0"
        },
        "cuts": {},
        "bounds": {
            "min_x": -60.0, "max_x": 40.0,
            "min_y": -20.0, "max_y": 20.0,
            "min_z": 0.0, "max_z": 8.0
        },
        "construction_circles": {},
        "source": {"geometry_source_root": 1, "geometry_source_step": 7}
    }))
}

#[test]
fn front_composite_line_and_arc_use_body_segment_provenance() {
    let model = project_principal_views(&composite_spatial()).expect("projection succeeds");
    let front = &model.views[&PrincipalViewKind::Front];
    let mut line_indexes = Vec::new();
    let mut arc_indexes = Vec::new();
    for (id, primitive) in &front.primitives {
        match model.element_for_primitive(id) {
            Some(SpatialElementRef::Body) => {
                panic!("Front composite primitive {id} collapsed to Body")
            }
            Some(SpatialElementRef::BodySegment { index }) => match &primitive.geometry {
                ProjectedGeometry::Segment { .. } => line_indexes.push(*index),
                ProjectedGeometry::Arc { .. } => arc_indexes.push(*index),
                other => panic!("Front composite primitive {id} has unexpected geometry {other:?}"),
            },
            other => panic!("Front composite primitive {id} has unexpected source {other:?}"),
        }
        assert!(
            !id.0.contains("arc"),
            "primitive ids must not encode arc geometry: {id}"
        );
    }
    line_indexes.sort_unstable();
    line_indexes.dedup();
    arc_indexes.sort_unstable();
    arc_indexes.dedup();
    assert_eq!(line_indexes, vec![0, 3]);
    assert_eq!(arc_indexes, vec![1, 2, 4, 5]);
    assert_eq!(
        primitive(&model, PrincipalViewKind::Front, "body-segment-0000", 0).geometry,
        primitive(&model, PrincipalViewKind::Front, "body-segment-0000", 1).geometry
    );
    match &primitive(&model, PrincipalViewKind::Front, "body-segment-0001", 0).geometry {
        ProjectedGeometry::Arc { radius, .. } => assert_eq!(*radius, 20.0),
        other => panic!("segment 1 must remain a native arc, got {other:?}"),
    }

    for view in [PrincipalViewKind::Top, PrincipalViewKind::Left] {
        assert!(model.views[&view]
            .primitives
            .keys()
            .all(|id| { model.element_for_primitive(id) == Some(&SpatialElementRef::Body) }));
    }
}

#[test]
fn front_composite_body_only_provenance_is_rejected_by_identity() {
    let model = project_principal_views(&composite_spatial()).expect("projection succeeds");
    let front = &model.views[&PrincipalViewKind::Front];
    assert!(front.primitives.keys().all(|id| {
        !matches!(
            model.element_for_primitive(id),
            Some(SpatialElementRef::Body)
        )
    }));
}

#[test]
fn principal_projection_is_byte_stable_for_100_runs() {
    let expected = serde_json::to_string_pretty(
        &project_principal_views(&spatial()).expect("projection succeeds"),
    )
    .expect("serialize");
    for _ in 0..100 {
        let actual = serde_json::to_string_pretty(
            &project_principal_views(&spatial()).expect("projection succeeds"),
        )
        .expect("serialize");
        assert_eq!(actual, expected);
    }
}
