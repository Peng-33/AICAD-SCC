//! Phase 9.7 formal three-view integration tests.

use std::collections::BTreeMap;

use scc_drawing::dimension::{
    format_dimension, DimensionValue, DimensionValueSource, HoleDatum, PatternSemanticField,
    PatternShape, ResolvedDimensionValue,
};
use scc_drawing::{
    build_three_view_drawing, drawing_to_dxf, drawing_to_svg, DrawingPrimitiveRole,
    ThreeViewDrawingError, ThreeViewDrawingInputs, VIEW_GAP_MM,
};
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::{
    DrawingProperty, DrawingPropertyKind, EngineeringFeature, HoleDefinition, HolePattern,
    ProjectionMethod,
};
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::spatial::{HoleExtent, HoleStartFace, SourcedLength};
use scc_engineering::specification::{
    DecimalPlaces, DimensionalSpecification, DimensionalTolerance, EngineeringDimensionRef,
};
use scc_engineering::{ArcDirection, EngineeringModel, FeatureId, PartId, Point2D, ProfileSegment};
use scc_projection::{project_principal_views, PrincipalViewKind};
use scc_spatial::{lower_spatial, SpatialInputs};
use scc_visibility::classify_visibility;

fn apply(model: &mut EngineeringModel, command: EngineeringCommand) {
    CommandExecutor::apply(model, &command).unwrap();
}

fn pipeline(
    method: ProjectionMethod,
    scale: f64,
) -> (
    EngineeringModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
    scc_visibility::VisibilityModel,
) {
    pipeline_with_values(method, scale, 120.0, 80.0, 10.0, 10.0)
}

fn pipeline_with_values(
    method: ProjectionMethod,
    scale: f64,
    width: f64,
    height: f64,
    thickness: f64,
    diameter: f64,
) -> (
    EngineeringModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
    scc_visibility::VisibilityModel,
) {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("three_view_plate".into()),
            name: "three_view_plate".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle { width, height },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("center_hole".into()),
                diameter,
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: thickness }),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: FeatureId("center_hole".into()),
            extent: Some(HoleExtent::ThroughAll),
        },
    );
    model
        .drawing
        .set(DrawingProperty::ProjectionMethod { method });
    model.drawing.set(DrawingProperty::Scale { value: scale });

    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let spatial = lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    (model, spatial, projection, visibility)
}

fn build_model_with_artifacts(
    model: EngineeringModel,
) -> (
    scc_drawing::DrawingModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
) {
    build_model_with_artifacts_at_scale(model, 1.0)
}

fn build_model_with_artifacts_at_scale(
    mut model: EngineeringModel,
    scale: f64,
) -> (
    scc_drawing::DrawingModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
) {
    if !matches!(
        model.drawing.get(DrawingPropertyKind::ProjectionMethod),
        Some(DrawingProperty::ProjectionMethod { .. })
    ) {
        model.drawing.set(DrawingProperty::ProjectionMethod {
            method: ProjectionMethod::ThirdAngle,
        });
    }
    model.drawing.set(DrawingProperty::Scale { value: scale });
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    let hole_ids: Vec<_> = model
        .features
        .iter()
        .filter_map(|(id, feature)| {
            if matches!(
                feature,
                EngineeringFeature::Hole { .. } | EngineeringFeature::HolePattern { .. }
            ) {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    for id in hole_ids {
        if model.spatial.hole_extents.contains_key(&id) {
            continue;
        }
        apply(
            &mut model,
            EngineeringCommand::SetHoleExtent {
                target: id,
                extent: Some(HoleExtent::ThroughAll),
            },
        );
    }
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let spatial = lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    (drawing, spatial, projection)
}

fn build_model(model: EngineeringModel) -> scc_drawing::DrawingModel {
    build_model_with_artifacts(model).0
}

fn phase114_single_hole_model(extent: HoleExtent) -> EngineeringModel {
    let mut model = phase113_base_model(120.0, 80.0);
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("center_hole".into()),
                diameter: 10.0,
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: FeatureId("center_hole".into()),
            extent: Some(extent),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn phase114_circular_model(extent: HoleExtent) -> EngineeringModel {
    let mut model = phase113_circular_model(Point2D::new(0.0, 0.0));
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: FeatureId("bolt_pattern".into()),
            extent: Some(extent),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn phase114_grid_model(extent: HoleExtent) -> EngineeringModel {
    let mut model = phase113_base_model(120.0, 100.0);
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("grid_pattern".into()),
                pattern: HolePattern::Grid {
                    rows: 2,
                    columns: 3,
                    spacing_x: 20.0,
                    spacing_y: 15.0,
                    hole: HoleDefinition { diameter: 6.0 },
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: FeatureId("grid_pattern".into()),
            extent: Some(extent),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn symmetric_spec(value: f64) -> DimensionalSpecification {
    DimensionalSpecification::new(
        DimensionalTolerance::symmetric(value).unwrap(),
        Some(DecimalPlaces(2)),
    )
    .unwrap()
}

fn bilateral_spec(upper: f64, lower: f64) -> DimensionalSpecification {
    DimensionalSpecification::new(
        DimensionalTolerance::bilateral(upper, lower).unwrap(),
        Some(DecimalPlaces(2)),
    )
    .unwrap()
}

fn set_spec(
    model: &mut EngineeringModel,
    target: EngineeringDimensionRef,
    specification: DimensionalSpecification,
) {
    model
        .set_dimensional_specification(target, Some(specification))
        .unwrap();
}

fn phase115_grid_model() -> EngineeringModel {
    let mut model = phase114_grid_model(HoleExtent::Blind {
        start_face: HoleStartFace::Base,
        depth: SourcedLength::Literal { value: 4.0 },
    });
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Width,
        },
        symmetric_spec(0.10),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Height,
        },
        bilateral_spec(0.20, -0.05),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::PartThickness,
        symmetric_spec(0.05),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("grid_pattern".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Diameter,
        },
        symmetric_spec(0.02),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::HoleDepth {
            feature: FeatureId("grid_pattern".into()),
        },
        symmetric_spec(0.10),
    );
    for axis in [
        scc_engineering::specification::DimensionAxis::X,
        scc_engineering::specification::DimensionAxis::Y,
    ] {
        set_spec(
            &mut model,
            EngineeringDimensionRef::HoleLocation {
                feature: FeatureId("grid_pattern".into()),
                axis,
            },
            symmetric_spec(0.01),
        );
    }
    for slot in [
        scc_engineering::binding::FeatureParameterSlot::SpacingX,
        scc_engineering::binding::FeatureParameterSlot::SpacingY,
    ] {
        set_spec(
            &mut model,
            EngineeringDimensionRef::FeatureSlot {
                feature: FeatureId("grid_pattern".into()),
                slot,
            },
            symmetric_spec(0.10),
        );
    }
    model
}

fn phase115_plate_model() -> EngineeringModel {
    let mut model = phase113_base_model(120.0, 80.0);
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Width,
        },
        symmetric_spec(0.10),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Height,
        },
        bilateral_spec(0.20, -0.05),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::PartThickness,
        symmetric_spec(0.05),
    );
    model
}

fn phase115_circular_model() -> EngineeringModel {
    let mut model = phase114_circular_model(HoleExtent::Blind {
        start_face: HoleStartFace::Base,
        depth: SourcedLength::Literal { value: 4.0 },
    });
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("bolt_pattern".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Diameter,
        },
        symmetric_spec(0.03),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("bolt_pattern".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::PitchCircleDiameter,
        },
        symmetric_spec(0.10),
    );
    set_spec(
        &mut model,
        EngineeringDimensionRef::HoleDepth {
            feature: FeatureId("bolt_pattern".into()),
        },
        symmetric_spec(0.10),
    );
    for axis in [
        scc_engineering::specification::DimensionAxis::X,
        scc_engineering::specification::DimensionAxis::Y,
    ] {
        set_spec(
            &mut model,
            EngineeringDimensionRef::HoleLocation {
                feature: FeatureId("bolt_pattern".into()),
                axis,
            },
            symmetric_spec(0.01),
        );
    }
    model
}

fn all_dimension_texts(drawing: &scc_drawing::DrawingModel) -> Vec<(String, String)> {
    let mut texts = Vec::new();
    for (view_id, view) in &drawing.principal_sheet().unwrap().views {
        for (dimension_id, dimension) in &view.dimensions {
            texts.push((
                format!("{}:{}", view_id.0, dimension_id.0),
                format_dimension(dimension),
            ));
        }
    }
    texts.sort();
    texts
}

fn phase114_corner_model(extent: HoleExtent) -> EngineeringModel {
    let mut model = phase113_base_model(120.0, 80.0);
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("corner_pattern".into()),
                pattern: HolePattern::Corner {
                    offset_x: 12.0,
                    offset_y: 10.0,
                    hole: HoleDefinition { diameter: 6.0 },
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: FeatureId("corner_pattern".into()),
            extent: Some(extent),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn validate_three_view_artifacts(
    drawing: &scc_drawing::DrawingModel,
    spatial: &scc_spatial::SpatialModel,
    projection: &scc_projection::ProjectionModel,
) -> Result<(), scc_drawing::DrawingError> {
    scc_drawing::three_view_annotations::validate_three_view_annotations(
        &drawing.principal_sheet().unwrap().views,
        spatial,
        projection,
        &drawing.source_map,
    )
}

fn phase113_base_model(width: f64, height: f64) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("phase113_part".into()),
            name: "phase113_part".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle { width, height },
            },
        },
    );
    model
}

fn phase113_circular_model(center: Point2D) -> EngineeringModel {
    let mut model = phase113_base_model(160.0, 160.0);
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("bolt_pattern".into()),
                pattern: HolePattern::Circular {
                    center,
                    pitch_circle_diameter: 80.0,
                    count: 4,
                    start_angle: 0.0,
                    hole: HoleDefinition { diameter: 8.0 },
                },
            },
        },
    );
    model
}

fn phase121_circular_model(center: Point2D, with_flange_features: bool) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("phase121_flange".into()),
            name: "phase121_flange".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("circular_base".into()),
                name: "circular_base".into(),
                profile: scc_engineering::BaseProfile::Circle {
                    center,
                    diameter: 160.0,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 20.0 }),
        },
    );
    if with_flange_features {
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("center_hole".into()),
                    diameter: 60.0,
                },
            },
        );
        for (id, value) in [("center_hole_x", center.x), ("center_hole_y", center.y)] {
            apply(
                &mut model,
                EngineeringCommand::DeclareParameter {
                    id: scc_engineering::ParameterId(id.into()),
                    value: scc_engineering::parameter::ParameterValue::Scalar(value),
                    origin: scc_engineering::model::ParameterOrigin::User,
                    authority: scc_core::authority::AuthorityLevel::L0,
                },
            );
        }
        apply(
            &mut model,
            EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("center_hole".into()),
                slot: scc_engineering::binding::FeatureParameterSlot::CenterX,
                parameter: scc_engineering::ParameterId("center_hole_x".into()),
            },
        );
        apply(
            &mut model,
            EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("center_hole".into()),
                slot: scc_engineering::binding::FeatureParameterSlot::CenterY,
                parameter: scc_engineering::ParameterId("center_hole_y".into()),
            },
        );
        apply(
            &mut model,
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("center_hole".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        );
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: FeatureId("bolt_pattern".into()),
                    pattern: HolePattern::Circular {
                        center,
                        pitch_circle_diameter: 120.0,
                        count: 8,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 14.0 },
                    },
                },
            },
        );
        apply(
            &mut model,
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_pattern".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        );
    }
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn build_phase121_circular(
    model: EngineeringModel,
) -> (
    scc_drawing::DrawingModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
) {
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let spatial = lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    (drawing, spatial, projection)
}

fn phase122_rounded_model(center: Point2D, with_pattern: bool) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("phase122_rounded".into()),
            name: "phase122_rounded".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("rounded_base".into()),
                name: "rounded_base".into(),
                profile: scc_engineering::BaseProfile::RoundedRectangle {
                    center,
                    width: 120.0,
                    height: 80.0,
                    corner_radius: 10.0,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 8.0 }),
        },
    );
    if with_pattern {
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: FeatureId("corner_pattern".into()),
                    pattern: HolePattern::Corner {
                        offset_x: 20.0,
                        offset_y: 20.0,
                        hole: HoleDefinition { diameter: 8.0 },
                    },
                },
            },
        );
        apply(
            &mut model,
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("corner_pattern".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        );
    }
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn phase122_composite_model(with_hole: bool) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("phase122_composite".into()),
            name: "phase122_composite".into(),
        },
    );
    let segments = vec![
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
    ];
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("composite_base".into()),
                name: "composite_base".into(),
                profile: scc_engineering::BaseProfile::Composite { segments },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 8.0 }),
        },
    );
    if with_hole {
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("center_hole".into()),
                    diameter: 10.0,
                },
            },
        );
        apply(
            &mut model,
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("center_hole".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        );
    }
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn build_phase122(
    model: EngineeringModel,
) -> (
    scc_drawing::DrawingModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
) {
    build_phase121_circular(model)
}

fn build(method: ProjectionMethod, scale: f64) -> scc_drawing::DrawingModel {
    let (model, spatial, projection, visibility) = pipeline(method, scale);
    build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap()
}

fn view(
    drawing: &scc_drawing::DrawingModel,
    kind: scc_drawing::DrawingViewKind,
) -> &scc_drawing::DrawingView {
    drawing
        .principal_sheet()
        .unwrap()
        .views
        .values()
        .find(|view| view.kind == kind)
        .unwrap()
}

#[test]
fn phase121_disk_preserves_circle_body_and_true_side_projections() {
    let (drawing, spatial, projection) =
        build_phase121_circular(phase121_circular_model(Point2D::new(0.0, 0.0), false));
    assert!(matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Circle { center, radius }
            if center.x == 0.0 && center.y == 0.0 && radius == 80.0
    ));
    assert_eq!(
        spatial.bounds,
        scc_spatial::Bounds3D {
            min_x: -80.0,
            max_x: 80.0,
            min_y: -80.0,
            max_y: 80.0,
            min_z: 0.0,
            max_z: 20.0,
        }
    );
    for kind in [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ] {
        let view = &projection.views[&kind];
        let body = projection
            .sources
            .primitives_for_element(&scc_projection::SpatialElementRef::Body);
        let body_primitives: Vec<_> = body
            .iter()
            .filter(|id| view.primitives.contains_key(id))
            .collect();
        let expected = if kind == PrincipalViewKind::Front {
            2
        } else {
            8
        };
        assert_eq!(
            body_primitives.len(),
            expected,
            "{kind:?} body primitive count"
        );
        if kind != PrincipalViewKind::Front {
            assert!(body_primitives.iter().all(|id| matches!(
                view.primitives[*id].geometry,
                scc_projection::ProjectedGeometry::Segment { .. }
            )));
        }
    }

    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert!(front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.overall_diameter".into()
        )));
    assert!(!front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.overall_width".into()
        )));
    assert!(!front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.overall_height".into()
        )));
    assert_eq!(
        format_dimension(
            &front.dimensions
                [&scc_drawing::DrawingDimensionId("three_view.overall_diameter".into())]
        ),
        "\u{00D8}160"
    );
    assert!(front.center_geometry.values().any(|element| {
        matches!(
            element,
            scc_drawing::DrawingCenterGeometry::CenterMark(mark)
                if mark.source_feature == FeatureId("circular_base".into())
        )
    }));
    let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
    assert_eq!(
        format_dimension(
            &left.dimensions[&scc_drawing::DrawingDimensionId("three_view.thickness".into())]
        ),
        "20"
    );
}

#[test]
fn phase122_rounded_rectangle_preserves_exact_profile_views_and_radius_semantic() {
    let (drawing, spatial, projection) =
        build_phase122(phase122_rounded_model(Point2D::new(30.0, -20.0), true));
    let scc_spatial::SpatialFootprint::Composite { segments } = &spatial.body.footprint else {
        panic!("rounded rectangle must remain an exact composite spatial footprint");
    };
    assert_eq!(segments.len(), 8);
    assert_eq!(spatial.bounds.min_x, -30.0);
    assert_eq!(spatial.bounds.max_x, 90.0);
    assert_eq!(spatial.bounds.min_y, -60.0);
    assert_eq!(spatial.bounds.max_y, 20.0);
    assert_eq!(spatial.cuts.len(), 4);

    let front_projection = &projection.views[&PrincipalViewKind::Front];
    assert_eq!(
        front_projection
            .primitives
            .values()
            .filter(|primitive| matches!(
                primitive.geometry,
                scc_projection::ProjectedGeometry::Arc { .. }
            ))
            .count(),
        8
    );
    let top_bounds = projection.views[&PrincipalViewKind::Top].bounds;
    let left_bounds = projection.views[&PrincipalViewKind::Left].bounds;
    assert_eq!(top_bounds.max_u - top_bounds.min_u, 120.0);
    assert_eq!(top_bounds.max_v - top_bounds.min_v, 8.0);
    assert_eq!(left_bounds.max_u - left_bounds.min_u, 8.0);
    assert_eq!(left_bounds.max_v - left_bounds.min_v, 80.0);

    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.overall_width".into())]
        ),
        "120"
    );
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.overall_height".into())]
        ),
        "80"
    );
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.corner_radius".into())]
        ),
        "R10"
    );
    assert!(matches!(
        front.dimension_layouts
            [&scc_drawing::DrawingDimensionId("three_view.corner_radius".into())]
            .geometry,
        scc_drawing::DimensionLayoutGeometry::Radius { .. }
    ));
    assert_eq!(
        format_dimension(
            &front.dimensions
                [&scc_drawing::DrawingDimensionId("three_view.pattern:corner_pattern".into())]
        ),
        "4× Ø8 THRU"
    );
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();
    assert!(svg.contains("R10") && svg.contains("<path"));
    assert!(dxf.contains("\nARC\n") && dxf.contains("R10"));
    assert_front_body_segment_provenance(&spatial, &projection);
    assert!(!matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Rectangle { .. }
    ));
}

fn assert_front_body_segment_provenance(
    spatial: &scc_spatial::SpatialModel,
    projection: &scc_projection::ProjectionModel,
) {
    let scc_spatial::SpatialFootprint::Composite { segments } = &spatial.body.footprint else {
        panic!("composite/rounded spatial truth must remain an exact composite footprint");
    };
    let front = &projection.views[&PrincipalViewKind::Front];
    for (id, primitive) in &front.primitives {
        match projection.sources.element_for_primitive(id) {
            Some(scc_projection::SpatialElementRef::Body) => {
                panic!("Front profile edge {id} must not collapse to Body")
            }
            Some(scc_projection::SpatialElementRef::BodySegment { index }) => {
                let segment = segments
                    .iter()
                    .find(|segment| segment.segment_index() == *index)
                    .unwrap_or_else(|| panic!("unknown BodySegment index {index}"));
                match (&primitive.geometry, segment) {
                    (
                        scc_projection::ProjectedGeometry::Segment { .. },
                        scc_spatial::SpatialProfileSegment::Line { .. },
                    )
                    | (
                        scc_projection::ProjectedGeometry::Arc { .. },
                        scc_spatial::SpatialProfileSegment::Arc { .. },
                    ) => {}
                    (geometry, _) => panic!(
                        "Front BodySegment {index} geometry {geometry:?} does not match the typed segment"
                    ),
                }
                assert!(
                    !id.0.contains("arc"),
                    "typed provenance must not parse primitive ids: {id}"
                );
            }
            Some(scc_projection::SpatialElementRef::Cut { .. })
            | Some(scc_projection::SpatialElementRef::SlotCut { .. })
            | Some(scc_projection::SpatialElementRef::SlotSegment { .. }) => {}
            None => panic!("Front primitive {id} has no typed source"),
        }
    }
    for kind in [PrincipalViewKind::Top, PrincipalViewKind::Left] {
        let view = &projection.views[&kind];
        assert!(view.primitives.keys().any(|id| {
            matches!(
                projection.sources.element_for_primitive(id),
                Some(scc_projection::SpatialElementRef::Body)
            )
        }));
        assert!(view.primitives.keys().all(|id| {
            !matches!(
                projection.sources.element_for_primitive(id),
                Some(scc_projection::SpatialElementRef::BodySegment { .. })
            )
        }));
    }
}

#[test]
fn phase122_composite_line_arc_hole_is_exact_and_deterministic() {
    let model = phase122_composite_model(true);
    let (drawing, spatial, projection) = build_phase122(model.clone());
    let scc_spatial::SpatialFootprint::Composite { segments } = &spatial.body.footprint else {
        panic!("composite source must remain an exact composite spatial footprint");
    };
    assert_eq!(segments.len(), 6);
    assert!(matches!(
        segments[0],
        scc_spatial::SpatialProfileSegment::Line {
            segment_index: 0,
            ..
        }
    ));
    assert!(matches!(
        segments[1],
        scc_spatial::SpatialProfileSegment::Arc {
            segment_index: 1,
            ..
        }
    ));
    assert!(matches!(
        segments[2],
        scc_spatial::SpatialProfileSegment::Arc {
            segment_index: 2,
            ..
        }
    ));
    assert!(matches!(
        segments[3],
        scc_spatial::SpatialProfileSegment::Line {
            segment_index: 3,
            ..
        }
    ));
    assert!(matches!(
        segments[4],
        scc_spatial::SpatialProfileSegment::Arc {
            segment_index: 4,
            ..
        }
    ));
    assert!(matches!(
        segments[5],
        scc_spatial::SpatialProfileSegment::Arc {
            segment_index: 5,
            ..
        }
    ));
    assert_eq!(spatial.bounds.min_x, -60.0);
    assert_eq!(spatial.bounds.max_x, 40.0);
    assert_eq!(spatial.bounds.min_y, -20.0);
    assert_eq!(spatial.bounds.max_y, 20.0);
    assert_eq!(spatial.cuts.len(), 1);
    assert_eq!(
        projection.views[&PrincipalViewKind::Front]
            .primitives
            .values()
            .filter(|primitive| matches!(
                primitive.geometry,
                scc_projection::ProjectedGeometry::Arc { .. }
            ))
            .count(),
        8
    );
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert!(!front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.corner_radius".into()
        )));
    assert_eq!(
        format_dimension(
            &front.dimensions
                [&scc_drawing::DrawingDimensionId("three_view.hole_diameter:center_hole".into())]
        ),
        "Ø10 THRU"
    );
    let expected_svg = drawing_to_svg(&drawing).unwrap();
    let expected_dxf = drawing_to_dxf(&drawing).unwrap();
    assert!(expected_svg.contains("<path"));
    assert!(expected_dxf.contains("\nARC\n"));
    for _ in 0..100 {
        let (next, _, _) = build_phase122(model.clone());
        assert_eq!(drawing_to_svg(&next).unwrap(), expected_svg);
        assert_eq!(drawing_to_dxf(&next).unwrap(), expected_dxf);
    }
    assert_front_body_segment_provenance(&spatial, &projection);
}

#[test]
fn phase122_rounded_origin_fixture_emits_width_height_radius_and_thickness() {
    let (drawing, spatial, projection) =
        build_phase122(phase122_rounded_model(Point2D::new(0.0, 0.0), false));
    assert!(matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Composite { .. }
    ));
    assert_eq!(spatial.bounds.min_x, -60.0);
    assert_eq!(spatial.bounds.max_x, 60.0);
    assert_eq!(spatial.bounds.min_y, -40.0);
    assert_eq!(spatial.bounds.max_y, 40.0);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.overall_width".into())]
        ),
        "120"
    );
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.overall_height".into())]
        ),
        "80"
    );
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.corner_radius".into())]
        ),
        "R10"
    );
    let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
    assert_eq!(
        format_dimension(
            &left.dimensions[&scc_drawing::DrawingDimensionId("three_view.thickness".into())]
        ),
        "8"
    );
    assert_front_body_segment_provenance(&spatial, &projection);
}

#[test]
fn phase122_composite_without_hole_keeps_native_front_line_and_arc() {
    let (drawing, spatial, projection) = build_phase122(phase122_composite_model(false));
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert!(front
        .primitives
        .values()
        .any(|primitive| matches!(primitive, scc_drawing::DrawingPrimitive::Line { .. })));
    assert!(front
        .primitives
        .values()
        .any(|primitive| matches!(primitive, scc_drawing::DrawingPrimitive::Arc { .. })));
    assert!(!front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.corner_radius".into()
        )));
    assert_front_body_segment_provenance(&spatial, &projection);
}

fn assert_rendered_golden(name: &str, actual: &str, embedded: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name);
    if std::env::var("SCC_WRITE_GOLDENS").ok().as_deref() == Some("1") {
        std::fs::write(path, actual).unwrap();
        return;
    }
    assert_eq!(actual, embedded, "golden asset drifted: {name}");
}

#[test]
fn phase122_goldens_match_renderer_output() {
    let cases = [
        (
            phase122_rounded_model(Point2D::new(0.0, 0.0), false),
            "phase122_rounded_rectangle.svg",
            "phase122_rounded_rectangle.dxf",
            include_str!("goldens/phase122_rounded_rectangle.svg"),
            include_str!("goldens/phase122_rounded_rectangle.dxf"),
        ),
        (
            phase122_composite_model(false),
            "phase122_composite_profile.svg",
            "phase122_composite_profile.dxf",
            include_str!("goldens/phase122_composite_profile.svg"),
            include_str!("goldens/phase122_composite_profile.dxf"),
        ),
    ];
    for (model, svg_name, dxf_name, expected_svg, expected_dxf) in cases {
        let (drawing, _, _) = build_phase122(model);
        assert_rendered_golden(svg_name, &drawing_to_svg(&drawing).unwrap(), expected_svg);
        assert_rendered_golden(dxf_name, &drawing_to_dxf(&drawing).unwrap(), expected_dxf);
    }
}

#[test]
fn phase121_flagship_flange_reuses_holes_pattern_pcd_and_renderers() {
    let (drawing, spatial, projection) =
        build_phase121_circular(phase121_circular_model(Point2D::new(30.0, -20.0), true));
    assert!(matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Circle { center, radius }
            if center.x == 30.0 && center.y == -20.0 && radius == 80.0
    ));
    assert_eq!(spatial.cuts.len(), 9);
    assert!(projection
        .sources
        .iter()
        .any(|(_, source)| matches!(source, scc_projection::SpatialElementRef::Body)));
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let diameter =
        &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.overall_diameter".into())];
    assert!(matches!(
        diameter.kind,
        scc_drawing::DrawingDimensionKind::OverallDiameter
    ));
    assert_eq!(format_dimension(diameter), "\u{00D8}160");
    assert_eq!(
        format_dimension(
            &front.dimensions
                [&scc_drawing::DrawingDimensionId("three_view.hole_diameter:center_hole".into())]
        ),
        "\u{00D8}60 THRU"
    );
    let pattern = &front.dimensions
        [&scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into())];
    assert_eq!(
        format_dimension(pattern),
        "8\u{00D7} \u{00D8}14 THRU PCD \u{00D8}120"
    );
    assert!(front.center_geometry.values().any(|element| {
        matches!(
            element,
            scc_drawing::DrawingCenterGeometry::CenterMark(mark)
                if mark.source_feature == FeatureId("circular_base".into())
        )
    }));
    assert!(front.center_geometry.values().any(|element| {
        matches!(
            element,
            scc_drawing::DrawingCenterGeometry::CenterMark(mark)
                if mark.source_feature == FeatureId("center_hole".into())
        )
    }));
    let top = view(&drawing, scc_drawing::DrawingViewKind::Top);
    let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
    assert!(top.primitives.values().any(|primitive| matches!(
        primitive,
        scc_drawing::DrawingPrimitive::Line {
            role: DrawingPrimitiveRole::Hidden,
            ..
        }
    )));
    assert!(left.primitives.values().any(|primitive| matches!(
        primitive,
        scc_drawing::DrawingPrimitive::Line {
            role: DrawingPrimitiveRole::Hidden,
            ..
        }
    )));
    assert_eq!(
        format_dimension(
            &left.dimensions[&scc_drawing::DrawingDimensionId("three_view.thickness".into())]
        ),
        "20"
    );
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();
    assert!(
        svg.contains("\u{00D8}160")
            && svg.contains("\u{00D8}60 THRU")
            && svg.contains("8\u{00D7} \u{00D8}14 THRU PCD \u{00D8}120")
    );
    assert!(dxf.contains("AICAD_DIMENSION") && dxf.contains("AICAD_CENTER"));
}

#[test]
fn phase115_tolerance_formats_width_height_thickness_hole_location_spacing_and_depth() {
    let (drawing, _, _) = build_model_with_artifacts(phase115_grid_model());
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
    let dimension = |id: &str| &front.dimensions[&scc_drawing::DrawingDimensionId(id.into())];
    assert_eq!(
        format_dimension(dimension("three_view.overall_width")),
        "120.00 ±0.10"
    );
    assert_eq!(
        format_dimension(dimension("three_view.overall_height")),
        "100.00 +0.20/-0.05"
    );
    assert_eq!(
        format_dimension(
            &left.dimensions[&scc_drawing::DrawingDimensionId("three_view.thickness".into(),)]
        ),
        "10.00 ±0.05"
    );
    assert_eq!(
        format_dimension(dimension("three_view.pattern:grid_pattern")),
        "2×3 Ø6.00 ±0.02 ↧4.00 ±0.10 BASE"
    );
    assert_eq!(
        format_dimension(dimension("three_view.hole_location_x:grid_pattern")),
        "40.00 ±0.01"
    );
    assert_eq!(
        format_dimension(dimension("three_view.hole_location_y:grid_pattern")),
        "42.50 ±0.01"
    );
    assert_eq!(
        format_dimension(dimension("three_view.pattern_spacing_x:grid_pattern")),
        "20.00 ±0.10"
    );
    assert_eq!(
        format_dimension(dimension("three_view.pattern_spacing_y:grid_pattern")),
        "15.00 ±0.10"
    );

    let pattern = dimension("three_view.pattern:grid_pattern");
    let DimensionValue::HoleCallout {
        semantic,
        pattern:
            Some(PatternShape::Grid {
                spacing_x,
                spacing_y,
                ..
            }),
    } = &pattern.value
    else {
        panic!("expected typed grid hole callout");
    };
    assert!(semantic.diameter.specification.is_some());
    assert!(matches!(
        semantic.extent,
        scc_drawing::HoleCalloutExtent::Blind { ref depth, .. }
            if depth.specification.is_some()
    ));
    assert!(spacing_x.specification.is_some());
    assert!(spacing_y.specification.is_some());

    let json = serde_json::to_string(&drawing).unwrap();
    assert!(json.contains("symmetric"));
    assert!(!json.contains("120.00 ±0.10"));
}

#[test]
fn phase115_circular_pcd_and_diameter_tolerances_reach_hole_callout() {
    let (drawing, _, _) = build_model_with_artifacts(phase115_circular_model());
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let pattern = &front.dimensions
        [&scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into())];
    assert_eq!(
        format_dimension(pattern),
        "4× Ø8.00 ±0.03 ↧4.00 ±0.10 BASE PCD Ø80.00 ±0.10"
    );
    let DimensionValue::HoleCallout {
        pattern:
            Some(PatternShape::Circular {
                pitch_circle_diameter,
                ..
            }),
        ..
    } = &pattern.value
    else {
        panic!("expected circular callout");
    };
    assert!(pitch_circle_diameter.specification.is_some());
}

#[test]
fn phase115_scale_and_first_third_angle_do_not_change_formatted_text() {
    let nominal = phase115_plate_model();
    let third = build_model_with_artifacts_at_scale(nominal.clone(), 1.0).0;
    let half = build_model_with_artifacts_at_scale(nominal.clone(), 0.5).0;
    let double = build_model_with_artifacts_at_scale(nominal.clone(), 2.0).0;
    assert_eq!(all_dimension_texts(&third), all_dimension_texts(&half));
    assert_eq!(all_dimension_texts(&third), all_dimension_texts(&double));

    let mut first_model = nominal;
    first_model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::FirstAngle,
    });
    let first = build_model_with_artifacts_at_scale(first_model, 1.0).0;
    assert_eq!(all_dimension_texts(&third), all_dimension_texts(&first));
}

#[test]
fn phase115_typed_specification_mutation_fails_closed_at_drawing_boundary() {
    let model = phase115_grid_model();
    let (drawing, _, _) = build_model_with_artifacts(model.clone());
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let width =
        &front.dimensions[&scc_drawing::DrawingDimensionId("three_view.overall_width".into())];
    assert!(scc_drawing::validate_dimension_integrity(front, &model).is_ok());

    let mut model_without_width = model.clone();
    model_without_width
        .dimensional_specifications
        .remove(&EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Width,
        });
    assert!(scc_drawing::validate_dimension_integrity(front, &model_without_width).is_err());

    let mut tampered = drawing.clone();
    let front_mut = tampered
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&scc_drawing::DrawingViewId("front".into()))
        .unwrap();
    let tampered_width = front_mut
        .dimensions
        .get_mut(&scc_drawing::DrawingDimensionId(
            "three_view.overall_width".into(),
        ))
        .unwrap();
    tampered_width
        .specification
        .as_mut()
        .unwrap()
        .value
        .tolerance = DimensionalTolerance::Symmetric { plus_minus: 9.99 };
    let tampered_front = view(&tampered, scc_drawing::DrawingViewKind::Front);
    assert!(scc_drawing::validate_dimension_integrity(tampered_front, &model).is_err());
    assert_eq!(format_dimension(width), "120.00 ±0.10");
}

#[test]
fn phase114_direct_blind_callouts_use_typed_spatial_depth_and_side() {
    for (side, expected_text, expected_min, expected_max) in [
        (HoleStartFace::Base, "Ø10 ↧4 BASE", 0.0, 4.0),
        (HoleStartFace::Opposite, "Ø10 ↧4 OPP", 6.0, 10.0),
    ] {
        let (drawing, spatial, projection) =
            build_model_with_artifacts(phase114_single_hole_model(HoleExtent::Blind {
                start_face: side,
                depth: SourcedLength::Literal { value: 4.0 },
            }));
        let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
        let dimension = &front.dimensions
            [&scc_drawing::DrawingDimensionId("three_view.hole_diameter:center_hole".into())];
        assert_eq!(format_dimension(dimension), expected_text);
        let DimensionValue::HoleCallout {
            semantic,
            pattern: None,
        } = &dimension.value
        else {
            panic!("expected direct HoleCallout semantic");
        };
        let scc_drawing::HoleCalloutExtent::Blind {
            depth,
            side: semantic_side,
        } = &semantic.extent
        else {
            panic!("expected blind HoleCallout extent");
        };
        assert_eq!(*semantic_side, side);
        assert_eq!(depth.resolved, ResolvedDimensionValue::LengthMm { mm: 4.0 });
        assert!(matches!(
            &depth.source,
            DimensionValueSource::ResolvedHoleDepth {
                feature,
                entity,
            } if feature == &FeatureId("center_hole".into())
                && entity == &semantic.provenance.representative_entity
        ));
        assert_eq!(semantic.provenance.witnesses.len(), 1);
        let witness = &semantic.provenance.witnesses[0];
        assert_eq!(witness.z.min, expected_min);
        assert_eq!(witness.z.max, expected_max);
        assert!(matches!(
            witness.kind,
            scc_spatial::SpatialCutKind::Blind { start_face } if start_face == side
        ));
        assert!(validate_three_view_artifacts(&drawing, &spatial, &projection).is_ok());
    }
}

#[test]
fn phase114_pattern_blind_callouts_keep_quantity_depth_and_side_typed() {
    for (side, expected_text, expected_min, expected_max) in [
        (HoleStartFace::Base, "4× Ø8 ↧4 BASE", 0.0, 4.0),
        (HoleStartFace::Opposite, "4× Ø8 ↧4 OPP", 6.0, 10.0),
    ] {
        let (drawing, spatial, projection) =
            build_model_with_artifacts(phase114_circular_model(HoleExtent::Blind {
                start_face: side,
                depth: SourcedLength::Literal { value: 4.0 },
            }));
        let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
        let dimension = &front.dimensions
            [&scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into())];
        assert_eq!(format_dimension(dimension), expected_text);
        let DimensionValue::HoleCallout {
            semantic,
            pattern: Some(PatternShape::Circular { count, .. }),
        } = &dimension.value
        else {
            panic!("expected circular pattern HoleCallout semantic");
        };
        assert_eq!(count.resolved, ResolvedDimensionValue::Count { count: 4 });
        assert!(matches!(
            &semantic.quantity,
            Some(component)
                if component.source
                    == DimensionValueSource::ResolvedPatternSemantic {
                        feature: FeatureId("bolt_pattern".into()),
                        field: PatternSemanticField::Count,
                    }
                    && component.resolved == ResolvedDimensionValue::Count { count: 4 }
        ));
        let scc_drawing::HoleCalloutExtent::Blind {
            depth,
            side: semantic_side,
        } = &semantic.extent
        else {
            panic!("expected blind pattern HoleCallout extent");
        };
        assert_eq!(*semantic_side, side);
        assert_eq!(depth.resolved, ResolvedDimensionValue::LengthMm { mm: 4.0 });
        assert_eq!(semantic.provenance.witnesses.len(), 4);
        assert!(semantic.provenance.witnesses.iter().all(|witness| {
            witness.z.min == expected_min
                && witness.z.max == expected_max
                && matches!(
                    witness.kind,
                    scc_spatial::SpatialCutKind::Blind { start_face } if start_face == side
                )
        }));
        assert!(validate_three_view_artifacts(&drawing, &spatial, &projection).is_ok());
    }
}

#[test]
fn phase114_grid_and_corner_blind_callouts_keep_pattern_semantics() {
    let extent = || HoleExtent::Blind {
        start_face: HoleStartFace::Base,
        depth: SourcedLength::Literal { value: 4.0 },
    };
    for (model, feature, expected_text) in [
        (
            phase114_grid_model(extent()),
            FeatureId("grid_pattern".into()),
            "2×3 Ø6 ↧4 BASE",
        ),
        (
            phase114_corner_model(extent()),
            FeatureId("corner_pattern".into()),
            "4× Ø6 ↧4 BASE",
        ),
    ] {
        let (drawing, spatial, projection) = build_model_with_artifacts(model);
        let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
        let dimension_id =
            scc_drawing::DrawingDimensionId(format!("three_view.pattern:{}", feature));
        let dimension = &front.dimensions[&dimension_id];
        assert_eq!(format_dimension(dimension), expected_text);
        let DimensionValue::HoleCallout {
            semantic,
            pattern: Some(PatternShape::Grid { .. } | PatternShape::Corner { .. }),
        } = &dimension.value
        else {
            panic!("expected grid or corner HoleCallout semantic");
        };
        assert!(semantic.quantity.is_none());
        assert!(matches!(
            semantic.extent,
            scc_drawing::HoleCalloutExtent::Blind {
                side: HoleStartFace::Base,
                ..
            }
        ));
        assert!(validate_three_view_artifacts(&drawing, &spatial, &projection).is_ok());
    }
}

#[test]
fn phase114_blind_callout_text_is_scale_invariant_and_renderer_parity_holds() {
    let (drawing, _, _) = build_model_with_artifacts_at_scale(
        phase114_single_hole_model(HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 4.0 },
        }),
        2.0,
    );
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let dimension = &front.dimensions
        [&scc_drawing::DrawingDimensionId("three_view.hole_diameter:center_hole".into())];
    assert_eq!(format_dimension(dimension), "Ø10 ↧4 BASE");
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();
    assert!(svg.contains("Ø10 ↧4 BASE"));
    assert!(dxf.contains("Ø10 ↧4 BASE"));
}

#[test]
fn phase114_callout_semantic_and_spatial_witness_tampering_fails_closed() {
    let (mut drawing, spatial, projection) =
        build_model_with_artifacts(phase114_single_hole_model(HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 4.0 },
        }));
    let dimension_id =
        scc_drawing::DrawingDimensionId("three_view.hole_diameter:center_hole".into());
    let front = drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&scc_drawing::DrawingViewId("front".into()))
        .unwrap();
    let dimension = front.dimensions.get_mut(&dimension_id).unwrap();
    if let DimensionValue::HoleCallout {
        semantic,
        pattern: None,
    } = &mut dimension.value
    {
        let scc_drawing::HoleCalloutExtent::Blind { depth, side } = &mut semantic.extent else {
            panic!("expected blind semantic");
        };
        *side = HoleStartFace::Opposite;
        depth.resolved = ResolvedDimensionValue::LengthMm { mm: 3.0 };
    } else {
        panic!("expected direct HoleCallout semantic");
    }
    assert!(matches!(
        validate_three_view_artifacts(&drawing, &spatial, &projection),
        Err(scc_drawing::DrawingError::InvalidDimensionWitness { .. })
            | Err(scc_drawing::DrawingError::InvalidDrawingDimension { .. })
    ));

    let (mut pattern_drawing, spatial, projection) =
        build_model_with_artifacts(phase114_circular_model(HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 4.0 },
        }));
    let pattern_id = scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into());
    let front = pattern_drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&scc_drawing::DrawingViewId("front".into()))
        .unwrap();
    let dimension = front.dimensions.get_mut(&pattern_id).unwrap();
    if let DimensionValue::HoleCallout {
        semantic,
        pattern: Some(PatternShape::Circular { .. }),
    } = &mut dimension.value
    {
        semantic.provenance.witnesses[1] = semantic.provenance.witnesses[0].clone();
    } else {
        panic!("expected circular pattern HoleCallout semantic");
    }
    assert!(matches!(
        validate_three_view_artifacts(&pattern_drawing, &spatial, &projection),
        Err(scc_drawing::DrawingError::InvalidDimensionWitness { .. })
            | Err(scc_drawing::DrawingError::InvalidDrawingDimension { .. })
    ));
}

#[test]
fn phase114_spatial_cut_mismatch_and_projection_inference_fail_closed() {
    let (drawing, mut spatial, projection) =
        build_model_with_artifacts(phase114_circular_model(HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 4.0 },
        }));
    let first_cut = spatial.cuts.keys().next().unwrap().clone();
    spatial.cuts.get_mut(&first_cut).unwrap().radius = 5.0;
    assert!(matches!(
        validate_three_view_artifacts(&drawing, &spatial, &projection),
        Err(scc_drawing::DrawingError::InvalidDimensionWitness { .. })
            | Err(scc_drawing::DrawingError::InvalidDrawingDimension { .. })
    ));

    let mut model = phase114_single_hole_model(HoleExtent::Blind {
        start_face: HoleStartFace::Base,
        depth: SourcedLength::Literal { value: 4.0 },
    });
    let (_, spatial, projection) = build_model_with_artifacts(model.clone());
    model.spatial.hole_extents.insert(
        FeatureId("center_hole".into()),
        HoleExtent::Blind {
            start_face: HoleStartFace::Opposite,
            depth: SourcedLength::Literal { value: 4.0 },
        },
    );
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    let error = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap_err();
    assert!(matches!(
        error,
        scc_drawing::ThreeViewDrawingError::Drawing(
            scc_drawing::DrawingError::InvalidDimensionWitness { .. }
                | scc_drawing::DrawingError::InvalidDrawingDimension { .. }
        )
    ));

    model.spatial.hole_extents.clear();
    let error = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap_err();
    assert!(matches!(
        error,
        scc_drawing::ThreeViewDrawingError::Drawing(
            scc_drawing::DrawingError::InvalidDimensionWitness { .. }
                | scc_drawing::DrawingError::InvalidDrawingDimension { .. }
        )
    ));
}

#[test]
fn phase113_single_hole_location_uses_resolved_cut_and_body_edge() {
    let mut model = phase113_base_model(120.0, 80.0);
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("offset_hole".into()),
                diameter: 8.0,
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::DeclareParameter {
            id: scc_engineering::ParameterId("hole_x".into()),
            value: scc_engineering::parameter::ParameterValue::Scalar(20.0),
            origin: scc_engineering::model::ParameterOrigin::User,
            authority: scc_core::authority::AuthorityLevel::L0,
        },
    );
    apply(
        &mut model,
        EngineeringCommand::DeclareParameter {
            id: scc_engineering::ParameterId("hole_y".into()),
            value: scc_engineering::parameter::ParameterValue::Scalar(-10.0),
            origin: scc_engineering::model::ParameterOrigin::User,
            authority: scc_core::authority::AuthorityLevel::L0,
        },
    );
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: FeatureId("offset_hole".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::CenterX,
            parameter: scc_engineering::ParameterId("hole_x".into()),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: FeatureId("offset_hole".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::CenterY,
            parameter: scc_engineering::ParameterId("hole_y".into()),
        },
    );
    let drawing = build_model(model);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let x = &front.dimensions
        [&scc_drawing::DrawingDimensionId("three_view.hole_location_x:offset_hole".into())];
    let y = &front.dimensions
        [&scc_drawing::DrawingDimensionId("three_view.hole_location_y:offset_hole".into())];
    assert_eq!(format_dimension(x), "80");
    assert_eq!(format_dimension(y), "30");
    assert!(matches!(
        &x.value,
        DimensionValue::Measured {
            source: DimensionValueSource::ResolvedHoleLocation {
                datum: HoleDatum::LeftEdge,
                ..
            },
            resolved: ResolvedDimensionValue::LengthMm { mm: 80.0 },
        }
    ));
    assert!(matches!(
        &y.value,
        DimensionValue::Measured {
            source: DimensionValueSource::ResolvedHoleLocation {
                datum: HoleDatum::BottomEdge,
                ..
            },
            resolved: ResolvedDimensionValue::LengthMm { mm: 30.0 },
        }
    ));
}

#[test]
fn phase113_pattern_dimensions_keep_typed_semantics_and_resolved_witnesses() {
    let mut circular = phase113_base_model(160.0, 160.0);
    apply(
        &mut circular,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("bolt_pattern".into()),
                pattern: HolePattern::Circular {
                    center: Point2D::new(10.0, -5.0),
                    pitch_circle_diameter: 80.0,
                    count: 4,
                    start_angle: 0.0,
                    hole: HoleDefinition { diameter: 8.0 },
                },
            },
        },
    );
    let drawing = build_model(circular);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let circular_dim = &front.dimensions
        [&scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into())];
    assert_eq!(format_dimension(circular_dim), "4× Ø8 THRU");
    let DimensionValue::HoleCallout {
        semantic,
        pattern:
            Some(PatternShape::Circular {
                count,
                pitch_circle_diameter,
                hole_diameter,
            }),
    } = &circular_dim.value
    else {
        panic!("expected circular pattern shape");
    };
    assert_eq!(count.resolved, ResolvedDimensionValue::Count { count: 4 });
    assert!(matches!(
        &semantic.extent,
        scc_drawing::HoleCalloutExtent::Through
    ));
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId(
                "three_view.hole_location_x:bolt_pattern".into(),
            )]
        ),
        "90"
    );
    assert_eq!(
        format_dimension(
            &front.dimensions[&scc_drawing::DrawingDimensionId(
                "three_view.hole_location_y:bolt_pattern".into(),
            )]
        ),
        "75"
    );
    assert!(matches!(
        &front.dimensions
            [&scc_drawing::DrawingDimensionId("three_view.hole_location_x:bolt_pattern".into(),)]
            .value,
        DimensionValue::Measured {
            source: DimensionValueSource::ResolvedPatternLocation { .. },
            ..
        }
    ));
    assert_eq!(
        pitch_circle_diameter.source,
        DimensionValueSource::ResolvedPatternSemantic {
            feature: FeatureId("bolt_pattern".into()),
            field: PatternSemanticField::PitchCircleDiameter,
        }
    );
    assert_eq!(
        hole_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 8.0 }
    );
    assert!(!front
        .dimensions
        .keys()
        .any(|id| id.0.starts_with("three_view.hole_diameter:bolt_pattern")));

    let mut grid = phase113_base_model(120.0, 100.0);
    apply(
        &mut grid,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("grid_pattern".into()),
                pattern: HolePattern::Grid {
                    rows: 2,
                    columns: 3,
                    spacing_x: 20.0,
                    spacing_y: 15.0,
                    hole: HoleDefinition { diameter: 6.0 },
                },
            },
        },
    );
    let drawing = build_model(grid);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert_eq!(
        format_dimension(
            &front.dimensions
                [&scc_drawing::DrawingDimensionId("three_view.pattern:grid_pattern".into(),)]
        ),
        "2×3 Ø6 THRU"
    );
    assert!(front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.pattern_spacing_x:grid_pattern".into(),
        )));
    assert!(front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.pattern_spacing_y:grid_pattern".into(),
        )));

    let mut corner = phase113_base_model(120.0, 80.0);
    apply(
        &mut corner,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("corner_pattern".into()),
                pattern: HolePattern::Corner {
                    offset_x: 12.0,
                    offset_y: 10.0,
                    hole: HoleDefinition { diameter: 6.0 },
                },
            },
        },
    );
    let drawing = build_model(corner);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert_eq!(
        format_dimension(
            &front.dimensions
                [&scc_drawing::DrawingDimensionId("three_view.pattern:corner_pattern".into(),)]
        ),
        "4× Ø6 THRU"
    );
    assert!(front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.hole_location_x:corner_pattern".into(),
        )));
    assert!(front
        .dimensions
        .contains_key(&scc_drawing::DrawingDimensionId(
            "three_view.hole_location_y:corner_pattern".into(),
        )));
}

#[test]
fn phase113_pattern_location_is_translation_invariant_and_keeps_pattern_facts() {
    let original = build_model(phase113_circular_model(Point2D::new(0.0, 0.0)));
    let translated = build_model(phase113_circular_model(Point2D::new(20.0, 10.0)));
    let original_front = view(&original, scc_drawing::DrawingViewKind::Front);
    let translated_front = view(&translated, scc_drawing::DrawingViewKind::Front);
    let x_id = scc_drawing::DrawingDimensionId("three_view.hole_location_x:bolt_pattern".into());
    let y_id = scc_drawing::DrawingDimensionId("three_view.hole_location_y:bolt_pattern".into());
    assert_eq!(format_dimension(&original_front.dimensions[&x_id]), "80");
    assert_eq!(format_dimension(&original_front.dimensions[&y_id]), "80");
    assert_eq!(format_dimension(&translated_front.dimensions[&x_id]), "100");
    assert_eq!(format_dimension(&translated_front.dimensions[&y_id]), "90");
    let pattern_id = scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into());
    assert_eq!(
        format_dimension(&original_front.dimensions[&pattern_id]),
        format_dimension(&translated_front.dimensions[&pattern_id])
    );
}

#[test]
fn phase113_mutated_pattern_quantity_fails_closed() {
    let (mut drawing, spatial, projection) =
        build_model_with_artifacts(phase113_circular_model(Point2D::new(0.0, 0.0)));
    let front = drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&scc_drawing::DrawingViewId("front".into()))
        .unwrap();
    let id = scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into());
    let dimension = front.dimensions.get_mut(&id).unwrap();
    if let DimensionValue::HoleCallout {
        pattern: Some(PatternShape::Circular { count, .. }),
        ..
    } = &mut dimension.value
    {
        count.resolved = ResolvedDimensionValue::Count { count: 5 };
    } else {
        panic!("expected circular pattern");
    }
    let err = scc_drawing::three_view_annotations::validate_three_view_annotations(
        &drawing.principal_sheet().unwrap().views,
        &spatial,
        &projection,
        &drawing.source_map,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDimensionWitness { .. }
            | scc_drawing::DrawingError::InvalidDrawingDimension { .. }
    ));
}

#[test]
fn phase113_mutated_pattern_diameter_fails_closed() {
    let (mut drawing, spatial, projection) =
        build_model_with_artifacts(phase113_circular_model(Point2D::new(0.0, 0.0)));
    let front = drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&scc_drawing::DrawingViewId("front".into()))
        .unwrap();
    let id = scc_drawing::DrawingDimensionId("three_view.pattern:bolt_pattern".into());
    let dimension = front.dimensions.get_mut(&id).unwrap();
    if let DimensionValue::HoleCallout {
        pattern: Some(PatternShape::Circular { hole_diameter, .. }),
        ..
    } = &mut dimension.value
    {
        hole_diameter.resolved = ResolvedDimensionValue::LengthMm { mm: 16.0 };
    } else {
        panic!("expected circular pattern");
    }
    let err = scc_drawing::three_view_annotations::validate_three_view_annotations(
        &drawing.principal_sheet().unwrap().views,
        &spatial,
        &projection,
        &drawing.source_map,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDimensionWitness { .. }
            | scc_drawing::DrawingError::InvalidDrawingDimension { .. }
    ));
}

#[test]
fn builds_three_canonical_views_from_frozen_sidecars() {
    let drawing = build(ProjectionMethod::ThirdAngle, 1.0);
    let sheet = drawing.principal_sheet().unwrap();
    assert_eq!(sheet.views.len(), 3);
    assert_eq!(sheet.annotations.len(), 1);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
    let top = view(&drawing, scc_drawing::DrawingViewKind::Top);
    assert_eq!(front.dimensions.len(), 5);
    assert_eq!(left.dimensions.len(), 1);
    assert!(top.dimensions.is_empty());
    assert_eq!(front.center_geometry.len(), 3);
    assert!(left.center_geometry.is_empty() && top.center_geometry.is_empty());
    assert!(sheet
        .views
        .values()
        .all(|view| view.model_bounds.is_finite()));
    assert!(drawing.source_map.iter().next().is_none());
    assert_eq!(
        sheet
            .views
            .keys()
            .map(|id| id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["front", "left", "top"]
    );
    assert!(sheet
        .views
        .values()
        .flat_map(|view| view.primitives.values())
        .any(|primitive| matches!(
            primitive,
            scc_drawing::DrawingPrimitive::Line {
                role: DrawingPrimitiveRole::Hidden,
                ..
            }
        )));
}

#[test]
fn visibility_mapping_is_object_hidden_or_omitted() {
    let (model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    let projected_count: usize = projection.views.values().map(|v| v.primitives.len()).sum();
    let emitted_count: usize = drawing
        .principal_sheet()
        .unwrap()
        .views
        .values()
        .map(|v| v.primitives.len())
        .sum();
    let occluded_count: usize = visibility
        .views
        .values()
        .flat_map(|v| v.decisions.values())
        .filter(|d| d.class == scc_visibility::VisibilityClass::Occluded)
        .count();
    assert_eq!(emitted_count + occluded_count, projected_count);
    assert!(drawing
        .principal_sheet()
        .unwrap()
        .views
        .values()
        .flat_map(|v| v.primitives.values())
        .all(|primitive| matches!(
            primitive,
            scc_drawing::DrawingPrimitive::Line { role, .. }
                if *role == DrawingPrimitiveRole::Object || *role == DrawingPrimitiveRole::Hidden
        ) || matches!(
            primitive,
            scc_drawing::DrawingPrimitive::Circle { role, .. }
                if *role == DrawingPrimitiveRole::Object || *role == DrawingPrimitiveRole::Hidden
        )));
}

#[test]
fn first_and_third_angle_change_only_layout() {
    let third = build(ProjectionMethod::ThirdAngle, 1.0);
    let first = build(ProjectionMethod::FirstAngle, 1.0);
    for kind in [
        scc_drawing::DrawingViewKind::Front,
        scc_drawing::DrawingViewKind::Top,
        scc_drawing::DrawingViewKind::Left,
    ] {
        assert_eq!(view(&third, kind).primitives, view(&first, kind).primitives);
        assert_eq!(
            view(&third, kind).model_bounds,
            view(&first, kind).model_bounds
        );
    }
    let tf = view(&third, scc_drawing::DrawingViewKind::Front).sheet_bounds();
    let tt = view(&third, scc_drawing::DrawingViewKind::Top).sheet_bounds();
    let tl = view(&third, scc_drawing::DrawingViewKind::Left).sheet_bounds();
    assert!((tt.min_y - tf.max_y - VIEW_GAP_MM).abs() < 1e-9);
    assert!((tf.min_x - tl.max_x - VIEW_GAP_MM).abs() < 1e-9);
    let ff = view(&first, scc_drawing::DrawingViewKind::Front).sheet_bounds();
    let ft = view(&first, scc_drawing::DrawingViewKind::Top).sheet_bounds();
    let fl = view(&first, scc_drawing::DrawingViewKind::Left).sheet_bounds();
    assert!((ff.min_y - ft.max_y - VIEW_GAP_MM).abs() < 1e-9);
    assert!((fl.min_x - ff.max_x - VIEW_GAP_MM).abs() < 1e-9);
}

#[test]
fn shared_axes_and_scale_are_deterministic() {
    let small = build(ProjectionMethod::ThirdAngle, 0.5);
    let normal = build(ProjectionMethod::ThirdAngle, 1.0);
    let large = build(ProjectionMethod::ThirdAngle, 2.0);
    for drawing in [&small, &normal, &large] {
        let f = view(drawing, scc_drawing::DrawingViewKind::Front);
        let t = view(drawing, scc_drawing::DrawingViewKind::Top);
        let l = view(drawing, scc_drawing::DrawingViewKind::Left);
        assert!((f.transform.translation.x - t.transform.translation.x).abs() < 1e-9);
        assert!((f.transform.translation.y - l.transform.translation.y).abs() < 1e-9);
        let tf = f.sheet_bounds();
        let tt = t.sheet_bounds();
        let tl = l.sheet_bounds();
        assert!((tt.min_y - tf.max_y - VIEW_GAP_MM).abs() < 1e-9);
        assert!((tf.min_x - tl.max_x - VIEW_GAP_MM).abs() < 1e-9);
    }
    let small_front = view(&small, scc_drawing::DrawingViewKind::Front).sheet_bounds();
    let normal_front = view(&normal, scc_drawing::DrawingViewKind::Front).sheet_bounds();
    let large_front = view(&large, scc_drawing::DrawingViewKind::Front).sheet_bounds();
    let small_width = small_front.max_x - small_front.min_x;
    let normal_width = normal_front.max_x - normal_front.min_x;
    let large_width = large_front.max_x - large_front.min_x;
    assert!((small_width * 2.0 - normal_width).abs() < 1e-9);
    assert!((large_width - normal_width * 2.0).abs() < 1e-9);
    assert!((normal.principal_sheet().unwrap().content_bounds().min_x - 10.0).abs() < 1e-9);
    assert!((normal.principal_sheet().unwrap().content_bounds().min_y - 10.0).abs() < 1e-9);
    assert_eq!(
        small
            .source_map
            .projected_iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>(),
        normal
            .source_map
            .projected_iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        normal
            .source_map
            .projected_iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>(),
        large
            .source_map
            .projected_iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn renderers_present_hidden_geometry_in_distinct_channels() {
    let drawing = build(ProjectionMethod::ThirdAngle, 1.0);
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();
    assert!(svg.contains("view_front") && svg.contains("view_top") && svg.contains("view_left"));
    assert!(svg.contains("_hidden\"") && svg.contains("stroke-dasharray=\"4 2\""));
    assert!(dxf.contains("AICAD_HIDDEN"));
    assert!(!svg.contains("id=\"construction\""));
    let primitive_count: usize = drawing
        .principal_sheet()
        .unwrap()
        .views
        .values()
        .map(|view| view.primitives.len())
        .sum();
    assert_eq!(svg.matches("id=\"p_").count(), primitive_count);
    assert_eq!(
        dxf.matches("\n8\nAICAD_OBJECT\n").count() + dxf.matches("\n8\nAICAD_HIDDEN\n").count(),
        primitive_count
    );
}

#[test]
fn mandatory_gate_preserves_upstream_pcv_error() {
    let (model, spatial, projection, mut visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    visibility.views.remove(&PrincipalViewKind::Front);
    let error = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap_err();
    match error {
        ThreeViewDrawingError::Consistency(error) => assert!(error.code().starts_with("SCC-PCV-")),
        other => panic!("expected original PCV error, got {other:?}"),
    }
}

#[test]
fn missing_projection_method_fails_closed() {
    let (mut model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    model
        .drawing
        .properties
        .remove(&DrawingPropertyKind::ProjectionMethod);
    let error = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap_err();
    assert!(matches!(
        error,
        ThreeViewDrawingError::MissingProjectionMethod
    ));
}

#[test]
fn inputs_and_output_are_byte_deterministic() {
    let (model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    let before = (
        serde_json::to_vec(&model).unwrap(),
        serde_json::to_vec(&spatial).unwrap(),
        serde_json::to_vec(&projection).unwrap(),
        serde_json::to_vec(&visibility).unwrap(),
    );
    let one = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    let one_json = serde_json::to_vec(&one).unwrap();
    let one_svg = drawing_to_svg(&one).unwrap();
    let one_dxf = drawing_to_dxf(&one).unwrap();
    for _ in 0..100 {
        let next = build_three_view_drawing(&ThreeViewDrawingInputs {
            engineering: &model,
            spatial: &spatial,
            projection: &projection,
            visibility: &visibility,
        })
        .unwrap();
        assert_eq!(serde_json::to_vec(&next).unwrap(), one_json);
        assert_eq!(drawing_to_svg(&next).unwrap(), one_svg);
        assert_eq!(drawing_to_dxf(&next).unwrap(), one_dxf);
    }
    let after = (
        serde_json::to_vec(&model).unwrap(),
        serde_json::to_vec(&spatial).unwrap(),
        serde_json::to_vec(&projection).unwrap(),
        serde_json::to_vec(&visibility).unwrap(),
    );
    assert_eq!(before, after);
}

#[test]
fn phase11_annotation_semantics_and_typed_provenance() {
    let (model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
    let top = view(&drawing, scc_drawing::DrawingViewKind::Top);
    assert_eq!(
        format_dimension(
            front
                .dimensions
                .get(&scc_drawing::DrawingDimensionId(
                    "three_view.overall_width".into()
                ))
                .unwrap()
        ),
        "120"
    );
    assert_eq!(
        format_dimension(
            front
                .dimensions
                .get(&scc_drawing::DrawingDimensionId(
                    "three_view.overall_height".into()
                ))
                .unwrap()
        ),
        "80"
    );
    assert_eq!(
        format_dimension(
            front
                .dimensions
                .get(&scc_drawing::DrawingDimensionId(
                    "three_view.hole_diameter:center_hole".into()
                ))
                .unwrap()
        ),
        "Ø10 THRU"
    );
    assert_eq!(
        format_dimension(
            left.dimensions
                .get(&scc_drawing::DrawingDimensionId(
                    "three_view.thickness".into()
                ))
                .unwrap()
        ),
        "10"
    );
    assert_eq!(
        front.center_geometry.len(),
        3,
        "one mark plus two centerlines for the Front hole"
    );
    assert!(top.dimensions.is_empty());
    for dimension in front.dimensions.values().chain(left.dimensions.values()) {
        assert!(matches!(
            dimension.value,
            DimensionValue::Measured {
                source: DimensionValueSource::ResolvedGeometry { .. }
                    | DimensionValueSource::ResolvedHoleLocation { .. },
                resolved: ResolvedDimensionValue::LengthMm { .. },
            } | DimensionValue::HoleCallout { .. }
        ));
    }
}

#[test]
fn phase11_dimension_propagation_and_scale_keep_model_units() {
    let cases = [
        (120.0, 80.0, 10.0, 10.0),
        (160.0, 80.0, 10.0, 10.0),
        (120.0, 100.0, 10.0, 10.0),
        (120.0, 80.0, 12.0, 10.0),
        (120.0, 80.0, 10.0, 12.0),
    ];
    for (width, height, thickness, diameter) in cases {
        let (model, spatial, projection, visibility) = pipeline_with_values(
            ProjectionMethod::ThirdAngle,
            1.0,
            width,
            height,
            thickness,
            diameter,
        );
        let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
            engineering: &model,
            spatial: &spatial,
            projection: &projection,
            visibility: &visibility,
        })
        .unwrap();
        let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
        let left = view(&drawing, scc_drawing::DrawingViewKind::Left);
        assert_eq!(
            format_dimension(
                front
                    .dimensions
                    .get(&scc_drawing::DrawingDimensionId(
                        "three_view.overall_width".into()
                    ))
                    .unwrap()
            ),
            scc_geometry::svg::number(width)
        );
        assert_eq!(
            format_dimension(
                front
                    .dimensions
                    .get(&scc_drawing::DrawingDimensionId(
                        "three_view.overall_height".into()
                    ))
                    .unwrap()
            ),
            scc_geometry::svg::number(height)
        );
        assert_eq!(
            format_dimension(
                left.dimensions
                    .get(&scc_drawing::DrawingDimensionId(
                        "three_view.thickness".into()
                    ))
                    .unwrap()
            ),
            scc_geometry::svg::number(thickness)
        );
        assert_eq!(
            format_dimension(
                front
                    .dimensions
                    .get(&scc_drawing::DrawingDimensionId(
                        "three_view.hole_diameter:center_hole".into()
                    ))
                    .unwrap()
            ),
            format!("Ø{} THRU", scc_geometry::svg::number(diameter))
        );
    }
    let (model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    let half = build(ProjectionMethod::ThirdAngle, 0.5);
    let double = build(ProjectionMethod::ThirdAngle, 2.0);
    let width_id = scc_drawing::DrawingDimensionId("three_view.overall_width".into());
    let normal_width = match &view(&drawing, scc_drawing::DrawingViewKind::Front).dimension_layouts
        [&width_id]
        .geometry
    {
        scc_drawing::DimensionLayoutGeometry::Linear { dimension_line, .. } => {
            dimension_line.end.x - dimension_line.start.x
        }
        _ => unreachable!(),
    };
    let half_width = match &view(&half, scc_drawing::DrawingViewKind::Front).dimension_layouts
        [&width_id]
        .geometry
    {
        scc_drawing::DimensionLayoutGeometry::Linear { dimension_line, .. } => {
            dimension_line.end.x - dimension_line.start.x
        }
        _ => unreachable!(),
    };
    let double_width = match &view(&double, scc_drawing::DrawingViewKind::Front).dimension_layouts
        [&width_id]
        .geometry
    {
        scc_drawing::DimensionLayoutGeometry::Linear { dimension_line, .. } => {
            dimension_line.end.x - dimension_line.start.x
        }
        _ => unreachable!(),
    };
    assert!((half_width * 2.0 - normal_width).abs() < 1e-9);
    assert!((double_width - normal_width * 2.0).abs() < 1e-9);
    for scaled in [&half, &drawing, &double] {
        assert_eq!(
            format_dimension(
                view(scaled, scc_drawing::DrawingViewKind::Front)
                    .dimensions
                    .get(&width_id)
                    .unwrap()
            ),
            "120"
        );
    }
}

#[test]
fn phase11_first_third_semantics_and_renderer_roles() {
    let third = build(ProjectionMethod::ThirdAngle, 1.0);
    let first = build(ProjectionMethod::FirstAngle, 1.0);
    for kind in [
        scc_drawing::DrawingViewKind::Front,
        scc_drawing::DrawingViewKind::Left,
        scc_drawing::DrawingViewKind::Top,
    ] {
        assert_eq!(view(&third, kind).dimensions, view(&first, kind).dimensions);
    }
    let svg = drawing_to_svg(&third).unwrap();
    let dxf = drawing_to_dxf(&third).unwrap();
    assert!(
        svg.contains("data-role=\"centerline\"") && svg.contains("stroke-dasharray=\"8 2 2 2\"")
    );
    assert!(svg.contains("stroke-dasharray=\"4 2\"") && svg.contains("Ø10"));
    assert!(
        dxf.contains("AICAD_CENTER")
            && dxf.contains("AICAD_CENTERLINE")
            && dxf.contains("AICAD_DIMENSION")
    );
    let sheet = third.principal_sheet().unwrap();
    for view in sheet.views.values() {
        if let Some(bounds) = view.layout_extents() {
            assert!(bounds.min_x >= 10.0 - 1e-9 && bounds.min_y >= 10.0 - 1e-9);
        }
        if let Some(bounds) = view.center_geometry_extents() {
            assert!(bounds.min_x >= 10.0 - 1e-9 && bounds.min_y >= 10.0 - 1e-9);
        }
    }
}

#[test]
fn phase11_mutation_resolved_value_is_rejected_by_witness_gate() {
    let (model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    let mut drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    let front_id = scc_drawing::DrawingViewId("front".into());
    let dim_id = scc_drawing::DrawingDimensionId("three_view.overall_width".into());
    let dimension = drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&front_id)
        .unwrap()
        .dimensions
        .get_mut(&dim_id)
        .unwrap();
    if let DimensionValue::Measured {
        resolved: ResolvedDimensionValue::LengthMm { mm },
        ..
    } = &mut dimension.value
    {
        *mm *= 2.0;
    } else {
        panic!("expected linear length");
    }
    let err = scc_drawing::three_view_annotations::validate_three_view_annotations(
        &drawing.principal_sheet().unwrap().views,
        &spatial,
        &projection,
        &drawing.source_map,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDimensionWitness { .. }
    ));
}

#[test]
fn phase11_diameter_ignores_later_engineering_literal_mutation() {
    let (mut model, spatial, projection, visibility) = pipeline(ProjectionMethod::ThirdAngle, 1.0);
    if let Some(EngineeringFeature::Hole { diameter, .. }) =
        model.features.get_mut(&FeatureId("center_hole".into()))
    {
        *diameter = 99.0;
    }
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    assert_eq!(
        format_dimension(
            view(&drawing, scc_drawing::DrawingViewKind::Front)
                .dimensions
                .get(&scc_drawing::DrawingDimensionId(
                    "three_view.hole_diameter:center_hole".into()
                ))
                .unwrap()
        ),
        "Ø10 THRU"
    );
}

#[test]
fn phase115_tolerance_goldens_match_renderer_output() {
    let drawing = build_model_with_artifacts(phase115_grid_model()).0;
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();
    assert_rendered_golden(
        "tolerance_plate.svg",
        &svg,
        include_str!("goldens/tolerance_plate.svg"),
    );
    assert_rendered_golden(
        "tolerance_plate.dxf",
        &dxf,
        include_str!("goldens/tolerance_plate.dxf"),
    );
}

#[test]
fn formal_three_view_goldens_match_renderer_output() {
    let drawing = build(ProjectionMethod::ThirdAngle, 1.0);
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();
    assert_rendered_golden(
        "three_view_plate.svg",
        &svg,
        include_str!("goldens/three_view_plate.svg"),
    );
    assert_rendered_golden(
        "three_view_plate.dxf",
        &dxf,
        include_str!("goldens/three_view_plate.dxf"),
    );
}

#[test]
fn phase121_circular_goldens_match_renderer_output() {
    let cases = [
        (
            phase121_circular_model(Point2D::new(0.0, 0.0), false),
            "phase121_circular_disk.svg",
            "phase121_circular_disk.dxf",
            include_str!("goldens/phase121_circular_disk.svg"),
            include_str!("goldens/phase121_circular_disk.dxf"),
        ),
        (
            phase121_circular_model(Point2D::new(30.0, -20.0), true),
            "phase121_circular_flange.svg",
            "phase121_circular_flange.dxf",
            include_str!("goldens/phase121_circular_flange.svg"),
            include_str!("goldens/phase121_circular_flange.dxf"),
        ),
    ];
    for (model, svg_name, dxf_name, expected_svg, expected_dxf) in cases {
        let (drawing, _, _) = build_phase121_circular(model);
        assert_rendered_golden(svg_name, &drawing_to_svg(&drawing).unwrap(), expected_svg);
        assert_rendered_golden(dxf_name, &drawing_to_dxf(&drawing).unwrap(), expected_dxf);
    }
}

#[test]
fn phase11_2_linear_collision_reflows_deterministically() {
    let mut first = build(ProjectionMethod::ThirdAngle, 1.0);
    let mut second = build(ProjectionMethod::ThirdAngle, 1.0);
    let width_id = scc_drawing::DrawingDimensionId("three_view.overall_width".into());
    let height_id = scc_drawing::DrawingDimensionId("three_view.overall_height".into());

    for drawing in [&mut first, &mut second] {
        let front = drawing
            .principal_sheet_mut()
            .unwrap()
            .views
            .get_mut(&scc_drawing::DrawingViewId("front".into()))
            .unwrap();
        let width_geometry = front.dimension_layouts[&width_id].geometry.clone();
        front
            .dimension_layouts
            .get_mut(&height_id)
            .unwrap()
            .geometry = width_geometry;
        let report = scc_drawing::optimize_dimension_layouts(front).unwrap();
        assert_eq!(report.adjusted_layouts, 1);
        scc_drawing::validate_dimension_layout_collisions(front).unwrap();
    }

    let first_front = view(&first, scc_drawing::DrawingViewKind::Front);
    let second_front = view(&second, scc_drawing::DrawingViewKind::Front);
    assert_eq!(
        first_front.dimension_layouts,
        second_front.dimension_layouts
    );
}

#[test]
fn phase11_2_diameter_collision_fails_closed_without_detaching_axis() {
    let mut drawing = build(ProjectionMethod::ThirdAngle, 1.0);
    let front = drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&scc_drawing::DrawingViewId("front".into()))
        .unwrap();
    let diameter_id =
        scc_drawing::DrawingDimensionId("three_view.hole_diameter:center_hole".into());
    let duplicate_id = scc_drawing::DrawingDimensionId("test.duplicate_diameter".into());
    let mut duplicate = front.dimensions[&diameter_id].clone();
    duplicate.id = duplicate_id.clone();
    front.dimensions.insert(duplicate_id.clone(), duplicate);
    let mut duplicate_layout = front.dimension_layouts[&diameter_id].clone();
    duplicate_layout.dimension = duplicate_id.clone();
    front
        .dimension_layouts
        .insert(duplicate_id.clone(), duplicate_layout);

    let error = scc_drawing::optimize_dimension_layouts(front).unwrap_err();
    assert!(matches!(
        error,
        scc_drawing::DrawingError::InvalidDimensionLayout { .. }
    ));
    assert!(error
        .to_string()
        .contains("three_view.hole_diameter:center_hole"));
    assert!(error.to_string().contains("test.duplicate_diameter"));
}

#[test]
fn projected_sources_have_no_orphans() {
    let drawing = build(ProjectionMethod::ThirdAngle, 1.0);
    let primitive_ids: BTreeMap<_, _> = drawing
        .principal_sheet()
        .unwrap()
        .views
        .values()
        .flat_map(|view| view.primitives.keys().map(|id| (id.clone(), view.kind)))
        .collect();
    let sources: BTreeMap<_, _> = drawing
        .source_map
        .projected_iter()
        .map(|(id, source)| (id.clone(), source.clone()))
        .collect();
    assert_eq!(primitive_ids.len(), sources.len());
    assert!(primitive_ids.keys().all(|id| sources.contains_key(id)));
    let json = serde_json::to_string(&drawing.source_map).unwrap();
    assert!(json.contains("projected_primitives"));
    let first = sources.values().next().unwrap();
    assert!(matches!(
        drawing
            .source_map
            .projected_source_for_primitive(sources.keys().next().unwrap()),
        Some(scc_drawing::DrawingSource::Projected { .. })
    ));
    assert!(matches!(first, scc_projection::ProjectionPrimitiveId(_)));
}
