use serde_json::json;

use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::EngineeringFeature;
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::spatial::{HoleExtent, SourcedLength};
use scc_engineering::{ArcDirection, EngineeringModel, FeatureId, PartId, Point2D, ProfileSegment};
use scc_projection::{
    project_principal_views, PrincipalViewKind, ProjectedGeometry, ProjectionDepthRange,
    ProjectionPrimitiveRole,
};
use scc_projection_consistency::{validate_projection_consistency, ProjectionConsistencyErrorKind};
use scc_spatial::{SpatialCutId, SpatialCutKind, SpatialFootprint, SpatialModel, ZInterval};
use scc_visibility::classify_visibility;

fn spatial() -> SpatialModel {
    serde_json::from_value(json!({
        "body": {
            "footprint": {"Rectangle": {
                "origin": {"x": -60.0, "y": -40.0},
                "width": 120.0,
                "height": 80.0
            }},
            "z": {"min": 0.0, "max": 10.0},
            "source_feature": "plate",
            "source_entity": "plate-face"
        },
        "cuts": {
            "cut-0": {
                "center": {"x": 0.0, "y": 0.0},
                "radius": 5.0,
                "z": {"min": 0.0, "max": 10.0},
                "kind": {"kind": "through_all"},
                "source_feature": "hole",
                "source_entity": "hole-circle"
            }
        },
        "bounds": {
            "min_x": -60.0, "max_x": 60.0,
            "min_y": -40.0, "max_y": 40.0,
            "min_z": 0.0, "max_z": 10.0
        },
        "construction_circles": {},
        "source": {"geometry_source_root": 1, "geometry_source_step": 2}
    }))
    .unwrap()
}

fn frozen_models() -> (
    SpatialModel,
    scc_projection::ProjectionModel,
    scc_visibility::VisibilityModel,
) {
    let spatial = spatial();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    (spatial, projection, visibility)
}

fn real_pipeline_models() -> (
    SpatialModel,
    scc_projection::ProjectionModel,
    scc_visibility::VisibilityModel,
) {
    let mut engineering = EngineeringModel::new();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::CreatePart {
            id: PartId("plate".into()),
            name: "Plate".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("hole".into()),
                diameter: 10.0,
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::SetHoleExtent {
            target: FeatureId("hole".into()),
            extent: Some(HoleExtent::ThroughAll),
        },
    )
    .unwrap();

    let (lowering, geometry) = lower_and_solve(&engineering).unwrap();
    let spatial = scc_spatial::lower_spatial(scc_spatial::SpatialInputs {
        engineering: &engineering,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    (spatial, projection, visibility)
}

fn composite_pipeline_models() -> (
    SpatialModel,
    scc_projection::ProjectionModel,
    scc_visibility::VisibilityModel,
) {
    let mut engineering = EngineeringModel::new();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::CreatePart {
            id: PartId("composite".into()),
            name: "Composite".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("composite_base".into()),
                name: "composite_base".into(),
                profile: scc_engineering::BaseProfile::Composite {
                    segments: vec![
                        ProfileSegment::Line {
                            start: Point2D::new(-40.0, -20.0),
                            end: Point2D::new(20.0, -20.0),
                        },
                        ProfileSegment::Arc {
                            start: Point2D::new(20.0, -20.0),
                            end: Point2D::new(40.0, 0.0),
                            center: Point2D::new(20.0, 0.0),
                            radius: 20.0,
                            direction: ArcDirection::CounterClockwise,
                        },
                        ProfileSegment::Arc {
                            start: Point2D::new(40.0, 0.0),
                            end: Point2D::new(20.0, 20.0),
                            center: Point2D::new(20.0, 0.0),
                            radius: 20.0,
                            direction: ArcDirection::CounterClockwise,
                        },
                        ProfileSegment::Line {
                            start: Point2D::new(20.0, 20.0),
                            end: Point2D::new(-40.0, 20.0),
                        },
                        ProfileSegment::Arc {
                            start: Point2D::new(-40.0, 20.0),
                            end: Point2D::new(-60.0, 0.0),
                            center: Point2D::new(-40.0, 0.0),
                            radius: 20.0,
                            direction: ArcDirection::CounterClockwise,
                        },
                        ProfileSegment::Arc {
                            start: Point2D::new(-60.0, 0.0),
                            end: Point2D::new(-40.0, -20.0),
                            center: Point2D::new(-40.0, 0.0),
                            radius: 20.0,
                            direction: ArcDirection::CounterClockwise,
                        },
                    ],
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut engineering,
        &EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 8.0 }),
        },
    )
    .unwrap();
    let (lowering, geometry) = lower_and_solve(&engineering).unwrap();
    let spatial = scc_spatial::lower_spatial(scc_spatial::SpatialInputs {
        engineering: &engineering,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    (spatial, projection, visibility)
}

#[test]
fn real_engineering_to_validator_pipeline_passes() {
    let (spatial, projection, visibility) = real_pipeline_models();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
}

#[test]
fn frozen_phase_9_5_fixture_passes_consistency_gate() {
    let (spatial, projection, visibility) = frozen_models();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
}

#[test]
fn wrong_visibility_class_is_not_accepted() {
    let (spatial, projection, mut visibility) = frozen_models();
    let id = projection.views[&PrincipalViewKind::Front]
        .primitives
        .keys()
        .next()
        .unwrap()
        .clone();
    let decision = visibility
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .decisions
        .get_mut(&id)
        .unwrap();
    decision.class = match decision.class {
        scc_visibility::VisibilityClass::Visible => scc_visibility::VisibilityClass::Hidden,
        _ => scc_visibility::VisibilityClass::Visible,
    };
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::VisibilityMismatch
    );
}

#[test]
fn wrong_geometry_is_not_accepted() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(p.geometry, ProjectedGeometry::Circle { .. }))
        .unwrap();
    primitive.geometry = ProjectedGeometry::Circle {
        center: scc_projection::ProjectedPoint2 { u: 1.0, v: 0.0 },
        radius: 5.0,
    };
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::GeometryMismatch
    );
}

#[test]
fn wrong_depth_is_not_accepted() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Top)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(p.geometry, ProjectedGeometry::Segment { .. }))
        .unwrap();
    primitive.depth = ProjectionDepthRange {
        min: 123.0,
        max: 124.0,
    };
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(error.kind(), ProjectionConsistencyErrorKind::DepthMismatch);
}

#[test]
fn missing_view_is_not_accepted() {
    let (spatial, mut projection, visibility) = frozen_models();
    projection.views.remove(&PrincipalViewKind::Left);
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::ViewOrPrimitiveMismatch
    );
}

#[test]
fn non_finite_state_is_not_accepted() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .primitives
        .values_mut()
        .next()
        .unwrap();
    if let ProjectedGeometry::Segment { start, .. } = &mut primitive.geometry {
        start.u = f64::NAN;
    }
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::NonFiniteNumericState
    );
}

#[test]
fn role_mutation_is_not_accepted() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .primitives
        .values_mut()
        .next()
        .unwrap();
    primitive.role = ProjectionPrimitiveRole::CutBoundary;
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::SourceOrRoleMismatch
    );
}

#[test]
fn unsupported_resolved_topology_is_rejected() {
    let (mut spatial, projection, visibility) = frozen_models();
    let mut value = serde_json::to_value(&spatial).unwrap();
    value["cuts"]["cut-0"]["kind"] = json!({"kind": "blind", "start_face": "base"});
    spatial = serde_json::from_value(value).unwrap();
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::UnsupportedTopology
    );
}

#[test]
fn source_map_mutation_is_not_accepted() {
    let (spatial, projection, visibility) = frozen_models();
    let mut value = serde_json::to_value(&projection).unwrap();
    let map = value["sources"]["primitive_sources"]
        .as_object_mut()
        .unwrap();
    let cut_key = map
        .iter()
        .find(|(_, source)| source["element"] == "cut")
        .map(|(id, _)| id.clone())
        .unwrap();
    map.insert(cut_key, json!({"element": "body"}));
    let projection: scc_projection::ProjectionModel = serde_json::from_value(value).unwrap();
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::SourceOrRoleMismatch
    );
}

#[test]
fn missing_source_map_reports_the_real_primitive() {
    let (spatial, projection, visibility) = frozen_models();
    let mut value = serde_json::to_value(&projection).unwrap();
    let map = value["sources"]["primitive_sources"]
        .as_object_mut()
        .unwrap();
    let primitive = map.keys().next().unwrap().clone();
    map.remove(&primitive);
    let projection: scc_projection::ProjectionModel = serde_json::from_value(value).unwrap();
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    match error {
        scc_projection_consistency::ProjectionConsistencyError::SourceOrRoleMismatch {
            primitive: Some(found),
            ..
        } => assert_eq!(found.0, primitive),
        other => panic!("expected source-map error with primitive, got {other:?}"),
    }
}

fn spatial_with_cuts(specs: &[(f64, f64, f64, f64, f64, SpatialCutKind)]) -> SpatialModel {
    let mut value = serde_json::to_value(spatial()).unwrap();
    let cuts = value["cuts"].as_object_mut().unwrap();
    cuts.clear();
    for (index, (x, y, radius, z_min, z_max, kind)) in specs.iter().enumerate() {
        cuts.insert(
            format!("cut-{index}"),
            json!({
                "center": {"x": x, "y": y},
                "radius": radius,
                "z": {"min": z_min, "max": z_max},
                "kind": serde_json::to_value(kind).unwrap(),
                "source_feature": format!("feature-{index}"),
                "source_entity": format!("circle-{index}")
            }),
        );
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn base_and_opposite_blind_intervals_pass_without_face_reinterpretation() {
    let mut spatial = spatial_with_cuts(&[
        (0.0, 0.0, 5.0, 0.0, 4.0, SpatialCutKind::ThroughAll),
        (20.0, 0.0, 5.0, 6.0, 10.0, SpatialCutKind::ThroughAll),
    ]);
    let mut value = serde_json::to_value(&spatial).unwrap();
    value["cuts"]["cut-0"]["kind"] = json!({"kind": "blind", "start_face": "base"});
    value["cuts"]["cut-1"]["kind"] = json!({"kind": "blind", "start_face": "opposite"});
    spatial = serde_json::from_value(value).unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
}

#[test]
fn swapped_blind_face_is_not_accepted() {
    let original = spatial_with_cuts(&[(0.0, 0.0, 5.0, 0.0, 4.0, SpatialCutKind::ThroughAll)]);
    let projection = project_principal_views(&original).unwrap();
    let visibility = classify_visibility(&original, &projection).unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value["cuts"]["cut-0"]["kind"] = json!({"kind": "blind", "start_face": "opposite"});
    let spatial: SpatialModel = serde_json::from_value(value).unwrap();
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::UnsupportedTopology
    );
}

#[test]
fn same_geometry_cuts_fail_closed_as_unsupported_ambiguity() {
    let spatial = spatial_with_cuts(&[
        (0.0, 0.0, 5.0, 0.0, 10.0, SpatialCutKind::ThroughAll),
        (0.0, 0.0, 5.0, 0.0, 10.0, SpatialCutKind::ThroughAll),
    ]);
    let (_, projection, visibility) = frozen_models();
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::UnsupportedTopology
    );
}

#[test]
fn eight_resolved_cuts_are_pattern_independent() {
    let positions = [
        (-45.0, -25.0),
        (-15.0, -25.0),
        (15.0, -25.0),
        (45.0, -25.0),
        (-45.0, 25.0),
        (-15.0, 25.0),
        (15.0, 25.0),
        (45.0, 25.0),
    ];
    let specs: Vec<_> = positions
        .iter()
        .map(|(x, y)| (*x, *y, 3.0, 0.0, 10.0, SpatialCutKind::ThroughAll))
        .collect();
    let spatial = spatial_with_cuts(&specs);
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
}

#[test]
fn input_models_are_byte_identical_after_validation() {
    let (spatial, projection, visibility) = frozen_models();
    let before_spatial = serde_json::to_vec(&spatial).unwrap();
    let before_projection = serde_json::to_vec(&projection).unwrap();
    let before_visibility = serde_json::to_vec(&visibility).unwrap();
    validate_projection_consistency(&spatial, &projection, &visibility).unwrap();
    assert_eq!(before_spatial, serde_json::to_vec(&spatial).unwrap());
    assert_eq!(before_projection, serde_json::to_vec(&projection).unwrap());
    assert_eq!(before_visibility, serde_json::to_vec(&visibility).unwrap());
}

#[test]
fn tolerance_is_shared_and_geometry_matching_is_inclusive_at_small_delta() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(p.geometry, ProjectedGeometry::Circle { .. }))
        .unwrap();
    if let ProjectedGeometry::Circle { center, .. } = &mut primitive.geometry {
        center.u += 0.5e-9;
    }
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );

    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(p.geometry, ProjectedGeometry::Circle { .. }))
        .unwrap();
    if let ProjectedGeometry::Circle { center, .. } = &mut primitive.geometry {
        center.u += 2.0e-9;
    }
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::GeometryMismatch
    );
}

#[test]
fn missing_visibility_decision_is_rejected() {
    let (spatial, projection, mut visibility) = frozen_models();
    let id = projection.views[&PrincipalViewKind::Top]
        .primitives
        .keys()
        .next()
        .unwrap()
        .clone();
    visibility
        .views
        .get_mut(&PrincipalViewKind::Top)
        .unwrap()
        .decisions
        .remove(&id);
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::ViewOrPrimitiveMismatch
    );
}

#[test]
fn missing_projection_primitive_is_rejected() {
    let (spatial, mut projection, visibility) = frozen_models();
    projection
        .views
        .get_mut(&PrincipalViewKind::Left)
        .unwrap()
        .primitives
        .pop_first();
    assert!(validate_projection_consistency(&spatial, &projection, &visibility).is_err());
}

#[test]
fn large_finite_coordinates_remain_safe() {
    let spatial = spatial_with_cuts(&[]);
    let mut value = serde_json::to_value(&spatial).unwrap();
    let rectangle = &mut value["body"]["footprint"]["Rectangle"];
    rectangle["origin"]["x"] = json!(-1.0e12);
    rectangle["origin"]["y"] = json!(-1.0e12);
    rectangle["width"] = json!(2.0e12);
    rectangle["height"] = json!(2.0e12);
    value["body"]["z"]["max"] = json!(1.0e12);
    value["bounds"] = json!({
        "min_x": -1.0e12, "max_x": 1.0e12,
        "min_y": -1.0e12, "max_y": 1.0e12,
        "min_z": 0.0, "max_z": 1.0e12
    });
    let spatial: SpatialModel = serde_json::from_value(value).unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
}

#[test]
fn large_finite_cut_geometry_and_depth_remain_safe() {
    let mut spatial =
        spatial_with_cuts(&[(0.0, 0.0, 1.0e6, 0.0, 10.0, SpatialCutKind::ThroughAll)]);
    let SpatialFootprint::Rectangle {
        origin,
        width,
        height,
    } = &mut spatial.body.footprint
    else {
        panic!("rectangle mutation fixture unexpectedly became circular");
    };
    origin.x = -1.0e12;
    origin.y = -1.0e12;
    *width = 2.0e12;
    *height = 2.0e12;
    spatial.bounds =
        scc_spatial::Bounds3D::new([-1.0e12, -1.0e12, 0.0], [1.0e12, 1.0e12, 10.0]).unwrap();
    spatial.body.z = ZInterval::new(0.0, 1.0e12).unwrap();
    spatial.bounds.max_z = 1.0e12;
    spatial
        .cuts
        .get_mut(&SpatialCutId("cut-0".into()))
        .unwrap()
        .z = ZInterval::new(0.0, 1.0e12).unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
}

#[test]
fn non_finite_spatial_state_maps_to_numeric_error() {
    let (mut spatial, projection, visibility) = frozen_models();
    let SpatialFootprint::Rectangle { origin, .. } = &mut spatial.body.footprint else {
        panic!("rectangle mutation fixture unexpectedly became circular");
    };
    origin.x = f64::NAN;
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::NonFiniteNumericState
    );
}

#[test]
fn orphan_visibility_decision_is_rejected() {
    let (spatial, projection, mut visibility) = frozen_models();
    visibility
        .views
        .get_mut(&PrincipalViewKind::Left)
        .unwrap()
        .decisions
        .insert(
            scc_projection::ProjectionPrimitiveId("orphan".into()),
            scc_visibility::VisibilityDecision {
                class: scc_visibility::VisibilityClass::Hidden,
            },
        );
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::ViewOrPrimitiveMismatch
    );
}

#[test]
fn radius_and_edge_mutations_are_not_accepted() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Front)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(p.geometry, ProjectedGeometry::Circle { .. }))
        .unwrap();
    if let ProjectedGeometry::Circle { radius, .. } = &mut primitive.geometry {
        *radius += 2.0e-9;
    }
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::GeometryMismatch
    );

    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Top)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(&p.geometry, ProjectedGeometry::Segment { start, end } if start.v == end.v))
        .unwrap();
    if let ProjectedGeometry::Segment { start, end } = &mut primitive.geometry {
        end.u = (start.u + end.u) / 2.0;
    }
    let error = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectionConsistencyErrorKind::GeometryMismatch
    );
}

#[test]
fn composite_front_requires_body_segment_provenance() {
    let (spatial, projection, visibility) = composite_pipeline_models();
    assert_eq!(
        validate_projection_consistency(&spatial, &projection, &visibility),
        Ok(())
    );
    let front = &projection.views[&PrincipalViewKind::Front];
    assert!(front.primitives.keys().any(|id| {
        matches!(
            projection.sources.element_for_primitive(id),
            Some(scc_projection::SpatialElementRef::BodySegment { index: 1 })
        ) && matches!(front.primitives[id].geometry, ProjectedGeometry::Arc { .. })
    }));
    assert!(front.primitives.keys().any(|id| {
        matches!(
            projection.sources.element_for_primitive(id),
            Some(scc_projection::SpatialElementRef::BodySegment { index: 0 })
        ) && matches!(
            front.primitives[id].geometry,
            ProjectedGeometry::Segment { .. }
        )
    }));

    let encoded = serde_json::to_value(&projection).unwrap();
    let mut tampered = encoded.clone();
    let sources = tampered["sources"]["primitive_sources"]
        .as_object_mut()
        .unwrap();
    for value in sources.values_mut() {
        if value.get("element").and_then(|element| element.as_str()) == Some("body_segment") {
            *value = json!({"element": "body"});
        }
    }
    let collapsed: scc_projection::ProjectionModel = serde_json::from_value(tampered).unwrap();
    assert!(validate_projection_consistency(&spatial, &collapsed, &visibility).is_err());
}

#[test]
fn repeated_validation_returns_the_same_first_error() {
    let (spatial, mut projection, visibility) = frozen_models();
    let primitive = projection
        .views
        .get_mut(&PrincipalViewKind::Top)
        .unwrap()
        .primitives
        .values_mut()
        .find(|p| matches!(p.geometry, ProjectedGeometry::Segment { .. }))
        .unwrap();
    primitive.depth = ProjectionDepthRange {
        min: 123.0,
        max: 124.0,
    };
    let first = validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
    for _ in 0..100 {
        let repeated =
            validate_projection_consistency(&spatial, &projection, &visibility).unwrap_err();
        assert_eq!(repeated, first);
    }
    assert_eq!(first.code(), "SCC-PCV-006");
}
