//! Integration coverage for the Phase 9.5 visibility contract.

use scc_projection::{
    project_principal_views, PrincipalViewKind, ProjectedGeometry, ProjectionPrimitiveId,
    ProjectionPrimitiveRole,
};
use scc_visibility::{classify_visibility, VisibilityClass, VisibilityError};
use serde_json::{json, Map, Value};

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

fn spatial_value() -> Value {
    serde_json::from_str(SPATIAL_FIXTURE).expect("fixture JSON")
}

fn spatial_from_value(value: Value) -> scc_spatial::SpatialModel {
    serde_json::from_value(value).expect("fixture SpatialModel")
}

fn spatial() -> scc_spatial::SpatialModel {
    spatial_from_value(spatial_value())
}

fn body_only() -> scc_spatial::SpatialModel {
    let mut value = spatial_value();
    value["cuts"] = json!({});
    spatial_from_value(value)
}

fn projection(spatial: &scc_spatial::SpatialModel) -> scc_projection::ProjectionModel {
    project_principal_views(spatial).expect("fixture projection")
}

fn class_counts(
    visibility: &scc_visibility::VisibilityModel,
    view: PrincipalViewKind,
) -> (usize, usize, usize) {
    (
        visibility.visible_ids(view).len(),
        visibility.hidden_ids(view).len(),
        visibility.occluded_ids(view).len(),
    )
}

fn id(view: PrincipalViewKind, source: &str, index: usize) -> ProjectionPrimitiveId {
    let view = match view {
        PrincipalViewKind::Front => "front",
        PrincipalViewKind::Top => "top",
        PrincipalViewKind::Left => "left",
    };
    ProjectionPrimitiveId(format!("{view}:{source}:p:{index:04}"))
}

fn class(
    visibility: &scc_visibility::VisibilityModel,
    view: PrincipalViewKind,
    source: &str,
    index: usize,
) -> VisibilityClass {
    visibility
        .class_for(view, &id(view, source, index))
        .expect("decision for fixture primitive")
}

fn cut_value(x: f64, y: f64, radius: f64, z_min: f64, z_max: f64) -> Value {
    json!({
        "center": { "x": x, "y": y },
        "radius": radius,
        "z": { "min": z_min, "max": z_max },
        "kind": { "kind": "through_all" },
        "source_feature": "pattern",
        "source_entity": "pattern"
    })
}

fn spatial_with_two_cuts(second_x: f64, second_y: f64) -> scc_spatial::SpatialModel {
    let mut value = spatial_value();
    let mut cuts = Map::new();
    cuts.insert(
        "a_hole:cut:0000".into(),
        cut_value(0.0, 0.0, 5.0, 0.0, 10.0),
    );
    cuts.insert(
        "b_hole:cut:0000".into(),
        cut_value(second_x, second_y, 5.0, 0.0, 10.0),
    );
    value["cuts"] = Value::Object(cuts);
    spatial_from_value(value)
}

fn spatial_with_pattern(count: usize, blind_base: bool) -> scc_spatial::SpatialModel {
    let mut value = spatial_value();
    let positions = [
        (-40.0, -20.0),
        (-20.0, -20.0),
        (0.0, -20.0),
        (20.0, -20.0),
        (-40.0, 20.0),
        (-20.0, 20.0),
        (0.0, 20.0),
        (20.0, 20.0),
    ];
    let (z_min, z_max) = if blind_base { (0.0, 4.0) } else { (0.0, 10.0) };
    let mut cuts = Map::new();
    for (index, (x, y)) in positions.into_iter().take(count).enumerate() {
        let mut cut = cut_value(x, y, 4.0, z_min, z_max);
        if blind_base {
            cut["kind"] = json!({ "kind": "blind", "start_face": "base" });
        }
        cuts.insert(format!("pattern:cut:{index:04}"), cut);
    }
    value["cuts"] = Value::Object(cuts);
    spatial_from_value(value)
}

fn swap_primitive_values(
    projection: &mut scc_projection::ProjectionModel,
    view: PrincipalViewKind,
    first: ProjectionPrimitiveId,
    second: ProjectionPrimitiveId,
) {
    let primitives = &mut projection.views.get_mut(&view).expect("view").primitives;
    let first_value = primitives.remove(&first).expect("first primitive");
    let second_value = primitives.remove(&second).expect("second primitive");
    primitives.insert(first, second_value);
    primitives.insert(second, first_value);
}

fn assert_unsupported(result: Result<impl std::fmt::Debug, VisibilityError>) {
    match result {
        Err(VisibilityError::UnsupportedVisibilityConfiguration { .. }) => {}
        other => panic!("expected unsupported visibility configuration, got {other:?}"),
    }
}

#[test]
fn body_front_top_left_are_depth_classified() {
    let projection = projection(&body_only());
    let visibility = classify_visibility(&body_only(), &projection).expect("visibility");
    for view in [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ] {
        assert_eq!(class_counts(&visibility, view), (4, 0, 4));
    }
}

#[test]
fn front_through_and_blind_rules_are_explicit() {
    let through_spatial = spatial();
    let through_projection = projection(&through_spatial);
    let through = classify_visibility(&through_spatial, &through_projection).expect("through");
    assert_eq!(class_counts(&through, PrincipalViewKind::Front), (5, 0, 5));
    assert_eq!(
        class(&through, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        VisibilityClass::Visible
    );
    assert_eq!(
        class(&through, PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        VisibilityClass::Occluded
    );

    let mut base_value = spatial_value();
    base_value["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 0.0, "max": 4.0 });
    base_value["cuts"]["a_hole:cut:0000"]["kind"] =
        json!({ "kind": "blind", "start_face": "base" });
    let base_spatial = spatial_from_value(base_value);
    let base_projection = projection(&base_spatial);
    let base = classify_visibility(&base_spatial, &base_projection).expect("blind base");
    assert_eq!(class_counts(&base, PrincipalViewKind::Front), (4, 1, 5));
    assert_eq!(
        class(&base, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        VisibilityClass::Hidden
    );
    assert_eq!(
        class(&base, PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        VisibilityClass::Occluded
    );

    let mut opposite_value = spatial_value();
    opposite_value["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 6.0, "max": 10.0 });
    opposite_value["cuts"]["a_hole:cut:0000"]["kind"] =
        json!({ "kind": "blind", "start_face": "opposite" });
    let opposite_spatial = spatial_from_value(opposite_value);
    let opposite_projection = projection(&opposite_spatial);
    let opposite =
        classify_visibility(&opposite_spatial, &opposite_projection).expect("blind opposite");
    assert_eq!(class_counts(&opposite, PrincipalViewKind::Front), (5, 0, 5));
    assert_eq!(
        class(&opposite, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        VisibilityClass::Visible
    );
    assert_eq!(
        class(&opposite, PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        VisibilityClass::Occluded
    );
}

#[test]
fn through_and_blind_side_views_classify_rims_and_tangents() {
    let through_spatial = spatial();
    let through_projection = projection(&through_spatial);
    let through = classify_visibility(&through_spatial, &through_projection).expect("through");
    for view in [PrincipalViewKind::Top, PrincipalViewKind::Left] {
        assert_eq!(class_counts(&through, view), (4, 2, 6));
    }

    let mut value = spatial_value();
    value["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 0.0, "max": 4.0 });
    value["cuts"]["a_hole:cut:0000"]["kind"] = json!({ "kind": "blind", "start_face": "base" });
    let blind_spatial = spatial_from_value(value);
    let blind_projection = projection(&blind_spatial);
    let blind = classify_visibility(&blind_spatial, &blind_projection).expect("blind");
    for view in [PrincipalViewKind::Top, PrincipalViewKind::Left] {
        assert_eq!(class_counts(&blind, view), (4, 3, 5));
    }
}

#[test]
fn multi_cut_counts_scale_without_pattern_branches() {
    let through_spatial = spatial_with_pattern(8, false);
    let through_projection = projection(&through_spatial);
    let through = classify_visibility(&through_spatial, &through_projection).expect("pattern");
    assert_eq!(
        class_counts(&through, PrincipalViewKind::Front),
        (12, 0, 12)
    );
    for view in [PrincipalViewKind::Top, PrincipalViewKind::Left] {
        assert_eq!(class_counts(&through, view), (4, 16, 20));
    }

    let blind_spatial = spatial_with_pattern(8, true);
    let blind_projection = projection(&blind_spatial);
    let blind = classify_visibility(&blind_spatial, &blind_projection).expect("blind pattern");
    assert_eq!(class_counts(&blind, PrincipalViewKind::Front), (4, 8, 12));
    for view in [PrincipalViewKind::Top, PrincipalViewKind::Left] {
        assert_eq!(class_counts(&blind, view), (4, 24, 12));
    }
}

#[test]
fn disjoint_cuts_are_supported_and_surface_rim_uses_containment() {
    let spatial = spatial_with_two_cuts(30.0, 0.0);
    let mut projection = projection(&spatial);
    let top_id = id(PrincipalViewKind::Top, "b_hole:cut:0000", 0);
    if let ProjectedGeometry::Segment { start, end } = &mut projection
        .views
        .get_mut(&PrincipalViewKind::Top)
        .expect("top")
        .primitives
        .get_mut(&top_id)
        .expect("rim")
        .geometry
    {
        start.u = -4.0;
        end.u = 4.0;
    } else {
        panic!("expected rim segment");
    }
    let visibility = classify_visibility(&spatial, &projection).expect("visibility");
    assert_eq!(
        class(&visibility, PrincipalViewKind::Top, "b_hole:cut:0000", 0),
        VisibilityClass::Occluded
    );
}

#[test]
fn partial_rim_contact_fails_closed() {
    let spatial = spatial();
    let mut projection = projection(&spatial);
    let top_id = id(PrincipalViewKind::Top, "a_hole:cut:0000", 0);
    if let ProjectedGeometry::Segment { start, end } = &mut projection
        .views
        .get_mut(&PrincipalViewKind::Top)
        .expect("top")
        .primitives
        .get_mut(&top_id)
        .expect("rim")
        .geometry
    {
        start.u = -65.0;
        start.v = 0.0;
        end.u = 0.0;
        end.v = 0.0;
    } else {
        panic!("expected rim segment");
    }
    projection
        .views
        .get_mut(&PrincipalViewKind::Top)
        .expect("top")
        .bounds
        .min_u = -65.0;

    let error = classify_visibility(&spatial, &projection).expect_err("partial rim");
    assert!(matches!(
        error,
        VisibilityError::UnsupportedVisibilityConfiguration {
            reason: scc_visibility::UnsupportedVisibilityReason::PartialCandidateClassification
        }
    ));
}

#[test]
fn insertion_order_and_endpoint_orientation_do_not_change_winner() {
    let spatial = spatial();
    let original_projection = projection(&spatial);
    let mut reordered = original_projection.clone();
    swap_primitive_values(
        &mut reordered,
        PrincipalViewKind::Front,
        id(PrincipalViewKind::Front, "body", 0),
        id(PrincipalViewKind::Front, "body", 4),
    );
    swap_primitive_values(
        &mut reordered,
        PrincipalViewKind::Front,
        id(PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        id(PrincipalViewKind::Front, "a_hole:cut:0000", 1),
    );
    if let ProjectedGeometry::Segment { start, end } = &mut reordered
        .views
        .get_mut(&PrincipalViewKind::Top)
        .expect("top")
        .primitives
        .get_mut(&id(PrincipalViewKind::Top, "body", 0))
        .expect("body")
        .geometry
    {
        std::mem::swap(start, end);
    }
    let visibility = classify_visibility(&spatial, &reordered).expect("reordered");
    assert_eq!(
        class(&visibility, PrincipalViewKind::Front, "body", 0),
        VisibilityClass::Occluded
    );
    assert_eq!(
        class(&visibility, PrincipalViewKind::Front, "body", 4),
        VisibilityClass::Visible
    );
    assert_eq!(
        class(&visibility, PrincipalViewKind::Front, "a_hole:cut:0000", 0),
        VisibilityClass::Visible
    );
    assert_eq!(
        class(&visibility, PrincipalViewKind::Front, "a_hole:cut:0000", 1),
        VisibilityClass::Occluded
    );
}

#[test]
fn raw_projection_is_not_mutated_and_decisions_are_complete() {
    let spatial = spatial();
    let projection = projection(&spatial);
    let raw_before = serde_json::to_vec(&projection).expect("projection JSON");
    let visibility = classify_visibility(&spatial, &projection).expect("visibility");
    let raw_after = serde_json::to_vec(&projection).expect("projection JSON");
    assert_eq!(raw_before, raw_after);
    visibility.validate_against(&projection).expect("complete");
    for view in [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ] {
        let raw_count = projection.views[&view].primitives.len();
        let counts = class_counts(&visibility, view);
        assert_eq!(counts.0 + counts.1 + counts.2, raw_count);
    }
    assert_eq!(visibility.views.len(), 3);
}

#[test]
fn construction_references_do_not_change_visibility() {
    let base_spatial = spatial();
    let base_projection = projection(&base_spatial);
    let base_visibility = classify_visibility(&base_spatial, &base_projection).expect("base");
    let mut value = spatial_value();
    value["construction_circles"] = json!({ "construction:reference": "reference_feature" });
    let construction_spatial = spatial_from_value(value);
    let construction_projection = projection(&construction_spatial);
    let construction_visibility =
        classify_visibility(&construction_spatial, &construction_projection).expect("construction");
    assert_eq!(base_projection, construction_projection);
    assert_eq!(base_visibility, construction_visibility);
}

#[test]
fn unsupported_domain_configurations_fail_closed() {
    let overlap = spatial_with_two_cuts(8.0, 0.0);
    assert_unsupported(classify_visibility(&overlap, &projection(&overlap)));
    let tangent = spatial_with_two_cuts(10.0, 0.0);
    assert_unsupported(classify_visibility(&tangent, &projection(&tangent)));

    let mut outside_value = spatial_value();
    outside_value["cuts"]["a_hole:cut:0000"]["center"]["x"] = json!(58.0);
    let outside_spatial = spatial_from_value(outside_value);
    assert_unsupported(classify_visibility(
        &outside_spatial,
        &projection(&outside_spatial),
    ));

    let tangent_body_spatial = {
        let mut value = spatial_value();
        value["cuts"]["a_hole:cut:0000"]["center"]["x"] = json!(55.0);
        spatial_from_value(value)
    };
    assert_unsupported(classify_visibility(
        &tangent_body_spatial,
        &projection(&tangent_body_spatial),
    ));

    let body_outside = {
        let mut value = spatial_value();
        value["cuts"]["a_hole:cut:0000"]["center"]["x"] = json!(500.0);
        spatial_from_value(value)
    };
    assert_unsupported(classify_visibility(
        &body_outside,
        &projection(&body_outside),
    ));
}

#[test]
fn malformed_projection_and_source_are_rejected() {
    let spatial = spatial();
    let mut malformed = projection(&spatial);
    let first = malformed
        .views
        .get_mut(&PrincipalViewKind::Front)
        .expect("front")
        .primitives
        .keys()
        .next()
        .cloned()
        .expect("primitive");
    malformed
        .views
        .get_mut(&PrincipalViewKind::Front)
        .expect("front")
        .primitives
        .remove(&first);
    assert_eq!(
        classify_visibility(&spatial, &malformed)
            .expect_err("missing raw primitive")
            .code(),
        "SCC-VIS-001"
    );

    let mut wrong_role = projection(&spatial);
    wrong_role
        .views
        .get_mut(&PrincipalViewKind::Front)
        .expect("front")
        .primitives
        .get_mut(&id(PrincipalViewKind::Front, "body", 0))
        .expect("body")
        .role = ProjectionPrimitiveRole::CutBoundary;
    assert_eq!(
        classify_visibility(&spatial, &wrong_role)
            .expect_err("wrong role")
            .code(),
        "SCC-VIS-001"
    );

    let body_spatial = body_only();
    assert_eq!(
        classify_visibility(&body_spatial, &projection(&spatial))
            .expect_err("stale source")
            .code(),
        "SCC-VIS-001"
    );
}

#[test]
fn serde_roundtrip_and_one_hundred_runs_are_byte_stable() {
    let spatial = spatial_with_pattern(8, false);
    let projection_model = projection(&spatial);
    let projection_bytes = serde_json::to_vec(&projection_model).expect("projection JSON");
    let expected_visibility = classify_visibility(&spatial, &projection_model).expect("visibility");
    let expected_bytes = serde_json::to_vec(&expected_visibility).expect("visibility JSON");
    for _ in 0..100 {
        let current_projection = projection(&spatial);
        let current_visibility =
            classify_visibility(&spatial, &current_projection).expect("visibility");
        assert_eq!(
            projection_bytes,
            serde_json::to_vec(&current_projection).expect("projection JSON")
        );
        assert_eq!(
            expected_bytes,
            serde_json::to_vec(&current_visibility).expect("visibility JSON")
        );
    }
    let roundtrip: scc_visibility::VisibilityModel =
        serde_json::from_slice(&expected_bytes).expect("visibility roundtrip");
    roundtrip
        .validate_against(&projection_model)
        .expect("validated roundtrip");
}

#[test]
fn parameter_and_negative_coordinate_cases_remain_derived_from_inputs() {
    let mut value = spatial_value();
    value["body"]["z"]["max"] = json!(12.0);
    value["bounds"]["max_z"] = json!(12.0);
    value["cuts"]["a_hole:cut:0000"]["z"] = json!({ "min": 0.0, "max": 12.0 });
    let through_spatial = spatial_from_value(value);
    let through_projection = projection(&through_spatial);
    let through = classify_visibility(&through_spatial, &through_projection).expect("thickness");
    assert_eq!(class_counts(&through, PrincipalViewKind::Front), (5, 0, 5));

    let mut negative = spatial_value();
    negative["body"]["footprint"]["Rectangle"]["origin"] = json!({ "x": -100.0, "y": -100.0 });
    negative["bounds"] = json!({
        "min_x": -100.0, "max_x": 20.0,
        "min_y": -100.0, "max_y": -20.0,
        "min_z": 0.0, "max_z": 10.0
    });
    negative["cuts"]["a_hole:cut:0000"]["center"] = json!({ "x": -40.0, "y": -60.0 });
    let negative_spatial = spatial_from_value(negative);
    let negative_projection = projection(&negative_spatial);
    let negative_visibility =
        classify_visibility(&negative_spatial, &negative_projection).expect("negative");
    assert_eq!(
        class_counts(&negative_visibility, PrincipalViewKind::Front),
        (5, 0, 5)
    );
    assert_eq!(
        class_counts(&negative_visibility, PrincipalViewKind::Top),
        (4, 2, 6)
    );
    assert!(negative_projection.views[&PrincipalViewKind::Left]
        .primitives
        .values()
        .any(|primitive| primitive.depth.max < 0.0));
}

#[test]
fn visibility_integrity_rejects_missing_and_orphan_decisions() {
    let spatial = spatial();
    let projection = projection(&spatial);
    let visibility = classify_visibility(&spatial, &projection).expect("visibility");
    let mut missing = visibility.clone();
    missing
        .views
        .get_mut(&PrincipalViewKind::Front)
        .expect("front")
        .decisions
        .pop_first();
    assert!(matches!(
        missing.validate_against(&projection),
        Err(VisibilityError::InvalidVisibilityDecision { .. })
            | Err(VisibilityError::MissingVisibilityDecision { .. })
    ));

    let mut orphan = visibility;
    orphan
        .views
        .get_mut(&PrincipalViewKind::Front)
        .expect("front")
        .decisions
        .insert(
            ProjectionPrimitiveId("orphan".into()),
            scc_visibility::VisibilityDecision {
                class: VisibilityClass::Visible,
            },
        );
    assert!(matches!(
        orphan.validate_against(&projection),
        Err(VisibilityError::InvalidVisibilityDecision { .. })
    ));
}
