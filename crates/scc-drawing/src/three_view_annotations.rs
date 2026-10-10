//! Phase 11.1 production annotations for the integrated Front/Top/Left drawing.
//!
//! This is a deliberately small adapter over the Phase 8 drawing annotation
//! types. It consumes only resolved Spatial/Projection artifacts and the
//! already-created drawing primitives. It never evaluates engineering
//! parameters, re-lowers hole extents, regenerates pattern geometry, or parses
//! deterministic identifiers.

use std::collections::{BTreeMap, BTreeSet};

use scc_core::ids::EntityId;
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::feature::{EngineeringFeature, HolePattern};
use scc_engineering::lowering::ResolvedEngineeringView;
use scc_engineering::specification::{DimensionAxis, EngineeringDimensionRef};
use scc_engineering::{FeatureId, HoleExtent};
use scc_projection::{PrincipalViewKind, ProjectedGeometry, ProjectionModel, SpatialElementRef};
use scc_spatial::{SpatialCutKind, SpatialModel};

use crate::center_geometry::{
    CenterMark, Centerline, CenterlineAxis, DrawingCenterGeometry, DrawingCenterGeometryId,
    DEFAULT_CENTERLINE_OVERHANG_MM, DEFAULT_CENTER_MARK_HALF_LENGTH_MM,
};
use crate::dimension::{
    drawing_specification, DimensionComponent, DimensionValue, DimensionValueSource,
    DrawingDimension, DrawingDimensionId, DrawingDimensionKind, DrawingDimensionSpecification,
    DrawingDimensionTarget, HoleCalloutExtent, HoleCalloutProvenance, HoleCalloutSemantic,
    HoleCalloutWitness, HoleDatum, LinearAxis, PatternSemanticField, PatternShape,
    ResolvedDimensionValue, SlotCalloutSemantic,
};
use crate::dimension_layout::{
    ArrowAnchor, DimensionLayoutGeometry, DrawingDimensionLayout, LayoutSegment, TextOrientation,
    DEFAULT_DIMENSION_OFFSET_MM, DEFAULT_DIMENSION_TEXT_GAP_MM, DEFAULT_EXTENSION_GAP_MM,
    DEFAULT_EXTENSION_OVERHANG_MM, DIAMETER_AXIS_ANGLE_DEG,
};
use crate::model::{
    DrawingBounds, DrawingPrimitive, DrawingPrimitiveRole, DrawingSheet, DrawingView,
};
use crate::source_map::DrawingSourceMap;
use crate::{DrawingError, DrawingPoint, DrawingViewId, DrawingViewTransform};

/// Build the frozen Phase 11.1 annotation set into the existing three views.
///
/// Ownership is fixed here: Front owns overall width, overall height, and one
/// hole-diameter callout per hole-bearing feature; Left owns thickness; Top
/// owns none of these overall dimensions. A circular pattern gets one
/// representative diameter callout, while every presented Front circle gets
/// its own center mark and two axial centerlines.
pub fn build_three_view_annotations(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    engineering: &scc_engineering::EngineeringModel,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
) -> Result<(), DrawingError> {
    if matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Parametric { .. }
    ) {
        // G1B carries exact geometry from generic feature history but does not
        // infer dimension semantics from final B-Rep topology. Views and
        // formal source maps still flow through the normal DrawingModel path.
        return Ok(());
    }
    let front_id = DrawingViewId("front".into());
    let left_id = DrawingViewId("left".into());
    let front_projection = projection
        .views
        .get(&PrincipalViewKind::Front)
        .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
            reason: "projection front is missing".into(),
        })?;
    let left_projection = projection
        .views
        .get(&PrincipalViewKind::Left)
        .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
            reason: "projection left is missing".into(),
        })?;
    let body_source = body_source(spatial)?;
    let front_body =
        projected_bounds_for_source(front_projection, &SpatialElementRef::Body, projection)?;
    let left_body =
        projected_bounds_for_source(left_projection, &SpatialElementRef::Body, projection)?;
    let (front_transform, front_geometry_bounds) = {
        let front = views
            .get(&front_id)
            .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                reason: "three-view front is missing".into(),
            })?;
        (front.transform, front.sheet_bounds())
    };
    let (left_transform, left_geometry_bounds) = {
        let left = views
            .get(&left_id)
            .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                reason: "three-view left is missing".into(),
            })?;
        (left.transform, left.sheet_bounds())
    };

    let circular_body = matches!(
        &spatial.body.footprint,
        scc_spatial::SpatialFootprint::Circle { .. }
    );
    let rounded_body = matches!(
        engineering.features.get(&body_source.0),
        Some(EngineeringFeature::BaseProfile {
            profile: scc_engineering::BaseProfile::RoundedRectangle { .. },
            ..
        })
    );
    {
        let front =
            views
                .get_mut(&front_id)
                .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                    reason: "three-view front is missing".into(),
                })?;
        if circular_body {
            let (center, radius) = projected_circle_for_source(
                front_projection,
                &SpatialElementRef::Body,
                projection,
            )?;
            let diameter = 2.0 * radius;
            if !diameter.is_finite() || diameter <= 0.0 {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: "circular body has an invalid projected diameter".into(),
                });
            }
            let dimension_id = DrawingDimensionId("three_view.overall_diameter".into());
            let dimension = DrawingDimension {
                id: dimension_id.clone(),
                target: DrawingDimensionTarget::Feature {
                    feature: body_source.0.clone(),
                },
                kind: DrawingDimensionKind::OverallDiameter,
                specification: drawing_specification(
                    engineering,
                    EngineeringDimensionRef::FeatureSlot {
                        feature: body_source.0.clone(),
                        slot: FeatureParameterSlot::Diameter,
                    },
                ),
                value: DimensionValue::Measured {
                    source: DimensionValueSource::ResolvedGeometry {
                        feature: body_source.0.clone(),
                        entity: body_source.1.clone(),
                    },
                    resolved: ResolvedDimensionValue::LengthMm { mm: diameter },
                },
            };
            front.dimensions.insert(dimension_id.clone(), dimension);
            front.dimension_layouts.insert(
                dimension_id.clone(),
                DrawingDimensionLayout {
                    dimension: dimension_id,
                    geometry: diameter_layout(
                        &front_transform,
                        center,
                        radius,
                        &DrawingDimensionId("three_view.overall_diameter".into()),
                    ),
                },
            );
        } else {
            insert_linear_dimension(
                front,
                DrawingDimensionId("three_view.overall_width".into()),
                body_source.0.clone(),
                body_source.1.clone(),
                LinearAxis::Horizontal,
                front_body.max_x - front_body.min_x,
                drawing_specification(
                    engineering,
                    EngineeringDimensionRef::FeatureSlot {
                        feature: body_source.0.clone(),
                        slot: FeatureParameterSlot::Width,
                    },
                ),
                linear_layout(
                    &front_transform,
                    front_body,
                    front_geometry_bounds,
                    LinearAxis::Horizontal,
                ),
            )?;
            insert_linear_dimension(
                front,
                DrawingDimensionId("three_view.overall_height".into()),
                body_source.0.clone(),
                body_source.1.clone(),
                LinearAxis::Vertical,
                front_body.max_y - front_body.min_y,
                drawing_specification(
                    engineering,
                    EngineeringDimensionRef::FeatureSlot {
                        feature: body_source.0.clone(),
                        slot: FeatureParameterSlot::Height,
                    },
                ),
                linear_layout(
                    &front_transform,
                    front_body,
                    front_geometry_bounds,
                    LinearAxis::Vertical,
                ),
            )?;
            if rounded_body {
                let (center, radius, start_angle) = projected_arc_for_source(
                    front_projection,
                    &SpatialElementRef::Body,
                    projection,
                )?;
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: "rounded body has an invalid projected corner radius".into(),
                    });
                }
                let dimension_id = DrawingDimensionId("three_view.corner_radius".into());
                front.dimensions.insert(
                    dimension_id.clone(),
                    DrawingDimension {
                        id: dimension_id.clone(),
                        target: DrawingDimensionTarget::Feature {
                            feature: body_source.0.clone(),
                        },
                        kind: DrawingDimensionKind::Radius,
                        specification: drawing_specification(
                            engineering,
                            EngineeringDimensionRef::FeatureSlot {
                                feature: body_source.0.clone(),
                                slot: FeatureParameterSlot::Radius,
                            },
                        ),
                        value: DimensionValue::Measured {
                            source: DimensionValueSource::ResolvedGeometry {
                                feature: body_source.0.clone(),
                                entity: body_source.1.clone(),
                            },
                            resolved: ResolvedDimensionValue::LengthMm { mm: radius },
                        },
                    },
                );
                front.dimension_layouts.insert(
                    dimension_id.clone(),
                    DrawingDimensionLayout {
                        dimension: dimension_id,
                        geometry: radius_layout(&front_transform, center, radius, start_angle),
                    },
                );
            }
        }
    }
    {
        let left = views
            .get_mut(&left_id)
            .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                reason: "three-view left is missing".into(),
            })?;
        insert_linear_dimension(
            left,
            DrawingDimensionId("three_view.thickness".into()),
            body_source.0.clone(),
            body_source.1.clone(),
            LinearAxis::Horizontal,
            left_body.max_x - left_body.min_x,
            drawing_specification(engineering, EngineeringDimensionRef::PartThickness),
            linear_layout(
                &left_transform,
                left_body,
                left_geometry_bounds,
                LinearAxis::Horizontal,
            ),
        )?;
    }

    let mut seen_features = BTreeSet::new();
    let front_primitive_snapshot = views
        .get(&front_id)
        .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
            reason: "three-view front is missing".into(),
        })?
        .primitives
        .clone();
    for (drawing_id, primitive) in &front_primitive_snapshot {
        let DrawingPrimitive::Circle {
            role,
            center,
            radius,
        } = primitive
        else {
            continue;
        };
        if *role != DrawingPrimitiveRole::Object {
            continue;
        }
        let projected_id = source_map
            .projected_for_primitive(drawing_id)
            .ok_or_else(|| DrawingError::MissingCenterSource {
                reason: format!("primitive {} has no projected provenance", drawing_id),
            })?;
        if !matches!(
            projection.sources.element_for_primitive(projected_id),
            Some(SpatialElementRef::Cut { .. })
        ) {
            continue;
        }
        let (feature, entity) = source_for_projected(projected_id, spatial, projection)?;
        if !seen_features.insert(feature.clone()) {
            continue;
        }
        let dimension_id = DrawingDimensionId(format!("three_view.hole_diameter:{}", feature));
        let diameter_specification = drawing_specification(
            engineering,
            EngineeringDimensionRef::FeatureSlot {
                feature: feature.clone(),
                slot: FeatureParameterSlot::Diameter,
            },
        );
        let diameter = 2.0 * *radius;
        let c = front_transform.model_to_sheet(*center);
        let layout = diameter_layout(&front_transform, c, *radius, &dimension_id);
        let dimension = DrawingDimension {
            id: dimension_id.clone(),
            target: DrawingDimensionTarget::Feature {
                feature: feature.clone(),
            },
            kind: DrawingDimensionKind::Diameter,
            specification: diameter_specification,
            value: DimensionValue::Measured {
                source: DimensionValueSource::ResolvedGeometry { feature, entity },
                resolved: ResolvedDimensionValue::LengthMm { mm: diameter },
            },
        };
        let front =
            views
                .get_mut(&front_id)
                .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                    reason: "three-view front is missing".into(),
                })?;
        front.dimensions.insert(dimension_id.clone(), dimension);
        front.dimension_layouts.insert(
            dimension_id.clone(),
            DrawingDimensionLayout {
                dimension: dimension_id,
                geometry: layout,
            },
        );
    }

    // Center geometry is generated for the circular body and every presented
    // cut circle. Their Body/Cut provenance remains distinct in the source map.
    let mut seen_center_sources = BTreeSet::new();
    for (drawing_id, primitive) in &front_primitive_snapshot {
        let DrawingPrimitive::Circle {
            role,
            center,
            radius,
        } = primitive
        else {
            continue;
        };
        if *role != DrawingPrimitiveRole::Object {
            continue;
        }
        let projected_id = source_map
            .projected_for_primitive(drawing_id)
            .ok_or_else(|| DrawingError::MissingCenterSource {
                reason: format!("primitive {} has no projected provenance", drawing_id),
            })?;
        let (feature, entity) = source_for_projected(projected_id, spatial, projection)?;
        if !seen_center_sources.insert((feature.clone(), entity.clone())) {
            continue;
        }
        let front =
            views
                .get_mut(&front_id)
                .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                    reason: "three-view front is missing".into(),
                })?;
        add_center_geometry(
            &mut front.center_geometry,
            drawing_id,
            &feature,
            &entity,
            *center,
            *radius,
            &front_transform,
        );
    }

    insert_slot_callouts(views, engineering, spatial)?;
    build_three_view_semantic_annotations(views, engineering, spatial, projection, source_map)?;
    validate_three_view_annotations(views, spatial, projection, source_map)?;
    for view in views.values() {
        crate::dimension::validate_dimension_integrity(view, engineering)?;
    }
    Ok(())
}

fn insert_slot_callouts(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    engineering: &scc_engineering::EngineeringModel,
    spatial: &SpatialModel,
) -> Result<(), DrawingError> {
    let front_id = DrawingViewId("front".into());
    let front_transform = views
        .get(&front_id)
        .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
            reason: "three-view front is missing".into(),
        })?
        .transform;
    for slot in spatial.slot_cuts.values() {
        let feature = slot.source_feature.clone();
        let entity = slot
            .segments
            .first()
            .map(|segment| segment.source_entity().clone())
            .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                reason: format!("slot {} has no segment witness", feature),
            })?;
        let dimension_id = DrawingDimensionId(format!("three_view.slot:{}", feature));
        let overall_length = crate::dimension::resolve_component(
            engineering,
            &feature,
            FeatureParameterSlot::Length,
        )?;
        let width = crate::dimension::resolve_component(
            engineering,
            &feature,
            FeatureParameterSlot::Width,
        )?;
        match (overall_length.resolved, width.resolved) {
            (
                ResolvedDimensionValue::LengthMm { mm: overall },
                ResolvedDimensionValue::LengthMm { mm: width_mm },
            ) if (overall - slot.overall_length).abs() < 1.0e-9
                && (width_mm - slot.width).abs() < 1.0e-9 => {}
            _ => {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!(
                        "slot {} drawing values disagree with spatial truth",
                        feature
                    ),
                })
            }
        }
        let dimension = DrawingDimension {
            id: dimension_id.clone(),
            target: DrawingDimensionTarget::Feature {
                feature: feature.clone(),
            },
            kind: DrawingDimensionKind::SlotCallout,
            specification: None,
            value: DimensionValue::SlotCallout {
                semantic: SlotCalloutSemantic {
                    overall_length,
                    width,
                    feature: feature.clone(),
                    entity,
                },
            },
        };
        let layout = slot_callout_layout(&front_transform, slot);
        let front =
            views
                .get_mut(&front_id)
                .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                    reason: "three-view front is missing".into(),
                })?;
        front.dimensions.insert(dimension_id.clone(), dimension);
        front.dimension_layouts.insert(
            dimension_id.clone(),
            DrawingDimensionLayout {
                dimension: dimension_id,
                geometry: layout,
            },
        );
    }
    Ok(())
}

/// Move the complete presentation envelope so geometry and generated
/// annotations share the deterministic sheet margin. Layout and center data
/// are already in sheet space, so they must be translated together with the
/// view transform.
pub fn normalize_annotation_bounds(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
) -> Result<(), DrawingError> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for view in views.values() {
        include_min(view.sheet_bounds(), &mut min_x, &mut min_y);
        if let Some(bounds) = view.layout_extents() {
            include_min(bounds, &mut min_x, &mut min_y);
        }
        if let Some(bounds) = view.center_geometry_extents() {
            include_min(bounds, &mut min_x, &mut min_y);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return Err(DrawingError::InvalidDrawingTransform {
            reason: "annotation envelope is non-finite".into(),
        });
    }
    let dx = DrawingSheet::DEFAULT_SHEET_MARGIN_MM - min_x;
    let dy = DrawingSheet::DEFAULT_SHEET_MARGIN_MM - min_y;
    if !dx.is_finite() || !dy.is_finite() {
        return Err(DrawingError::InvalidDrawingTransform {
            reason: "annotation envelope translation is non-finite".into(),
        });
    }
    for view in views.values_mut() {
        view.transform = DrawingViewTransform::new(
            view.transform.scale,
            DrawingPoint::new(
                view.transform.translation.x + dx,
                view.transform.translation.y + dy,
            ),
        )?;
        for layout in view.dimension_layouts.values_mut() {
            translate_layout(&mut layout.geometry, dx, dy);
        }
        for element in view.center_geometry.values_mut() {
            translate_center(element, dx, dy);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_linear_dimension(
    view: &mut DrawingView,
    id: DrawingDimensionId,
    feature: FeatureId,
    entity: EntityId,
    axis: LinearAxis,
    value: f64,
    specification: Option<DrawingDimensionSpecification>,
    layout: DimensionLayoutGeometry,
) -> Result<(), DrawingError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!("dimension {} has invalid resolved geometry span", id),
        });
    }
    let dimension = DrawingDimension {
        id: id.clone(),
        target: DrawingDimensionTarget::Feature {
            feature: feature.clone(),
        },
        kind: DrawingDimensionKind::Linear { axis },
        specification,
        value: DimensionValue::Measured {
            source: DimensionValueSource::ResolvedGeometry { feature, entity },
            resolved: ResolvedDimensionValue::LengthMm { mm: value },
        },
    };
    view.dimensions.insert(id.clone(), dimension);
    view.dimension_layouts.insert(
        id.clone(),
        DrawingDimensionLayout {
            dimension: id,
            geometry: layout,
        },
    );
    Ok(())
}

fn body_source(spatial: &SpatialModel) -> Result<(FeatureId, EntityId), DrawingError> {
    Ok((
        spatial.body.source_feature.clone(),
        spatial.body.source_entity.clone(),
    ))
}

fn source_for_projected(
    projected_id: &scc_projection::ProjectionPrimitiveId,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<(FeatureId, EntityId), DrawingError> {
    let source = projection
        .sources
        .element_for_primitive(projected_id)
        .ok_or_else(|| DrawingError::MissingCenterSource {
            reason: format!("projected primitive {} has no source", projected_id),
        })?;
    match source {
        SpatialElementRef::Body | SpatialElementRef::BodySegment { .. } => body_source(spatial),
        SpatialElementRef::Cut { id } => spatial
            .cuts
            .get(id)
            .map(|cut| (cut.source_feature.clone(), cut.source_entity.clone()))
            .ok_or_else(|| DrawingError::MissingCenterSource {
                reason: format!(
                    "projected primitive {} references missing spatial cut",
                    projected_id
                ),
            }),
        SpatialElementRef::SlotCut { id } | SpatialElementRef::SlotSegment { id, .. } => spatial
            .slot_cuts
            .get(id)
            .and_then(|slot| {
                slot.segments
                    .first()
                    .map(|segment| (slot.source_feature.clone(), segment.source_entity().clone()))
            })
            .ok_or_else(|| DrawingError::MissingCenterSource {
                reason: format!(
                    "projected primitive {} references missing spatial slot",
                    projected_id
                ),
            }),
    }
}

fn projected_bounds_for_source(
    view: &scc_projection::ProjectionView,
    source: &SpatialElementRef,
    projection: &ProjectionModel,
) -> Result<DrawingBounds, DrawingError> {
    let mut result: Option<DrawingBounds> = None;
    for (id, primitive) in &view.primitives {
        if !source_matches_requested(source, view_source(projection, id)) {
            continue;
        }
        let bounds = match &primitive.geometry {
            ProjectedGeometry::Segment { start, end } => DrawingBounds {
                min_x: start.u.min(end.u),
                min_y: start.v.min(end.v),
                max_x: start.u.max(end.u),
                max_y: start.v.max(end.v),
            },
            ProjectedGeometry::Circle { center, radius } => DrawingBounds {
                min_x: center.u - radius,
                min_y: center.v - radius,
                max_x: center.u + radius,
                max_y: center.v + radius,
            },
            ProjectedGeometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => crate::builder::primitive_bounds(&DrawingPrimitive::Arc {
                role: DrawingPrimitiveRole::Object,
                center: DrawingPoint::new(center.u, center.v),
                radius: *radius,
                start_angle: *start_angle,
                end_angle: *end_angle,
            })?,
        };
        match &mut result {
            Some(current) => current.include(&bounds),
            None => result = Some(bounds),
        }
    }
    result.ok_or_else(|| DrawingError::InvalidDimensionWitness {
        reason: "projected body has no geometry witness".into(),
    })
}

fn projected_circle_for_source(
    view: &scc_projection::ProjectionView,
    source: &SpatialElementRef,
    projection: &ProjectionModel,
) -> Result<(DrawingPoint, f64), DrawingError> {
    let mut circles = Vec::new();
    for (id, primitive) in &view.primitives {
        if !source_matches_requested(source, view_source(projection, id)) {
            continue;
        }
        let ProjectedGeometry::Circle { center, radius } = &primitive.geometry else {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "circular body projection contains a non-circle witness".into(),
            });
        };
        if primitive.role != scc_projection::ProjectionPrimitiveRole::BodyBoundary {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "circular body projection has a non-body role".into(),
            });
        }
        circles.push((center.u, center.v, *radius));
    }
    if circles.len() != 2 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "circular body projection requires two end-face circle witnesses, got {}",
                circles.len()
            ),
        });
    }
    let first = circles[0];
    let second = circles[1];
    if !first.0.is_finite()
        || !first.1.is_finite()
        || !first.2.is_finite()
        || first.2 <= 0.0
        || (first.0 - second.0).abs() > 1.0e-6
        || (first.1 - second.1).abs() > 1.0e-6
        || (first.2 - second.2).abs() > 1.0e-6
    {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "circular body projection end-face witnesses disagree".into(),
        });
    }
    Ok((DrawingPoint::new(first.0, first.1), first.2))
}

fn projected_arc_for_source(
    view: &scc_projection::ProjectionView,
    source: &SpatialElementRef,
    projection: &ProjectionModel,
) -> Result<(DrawingPoint, f64, f64), DrawingError> {
    let mut arcs = Vec::new();
    for (id, primitive) in &view.primitives {
        if !source_matches_requested(source, view_source(projection, id)) {
            continue;
        }
        if let ProjectedGeometry::Arc {
            center,
            radius,
            start_angle,
            ..
        } = &primitive.geometry
        {
            if primitive.role != scc_projection::ProjectionPrimitiveRole::BodyBoundary {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: "rounded body arc has a non-body projection role".into(),
                });
            }
            arcs.push((center.u, center.v, *radius, *start_angle));
        }
    }
    let Some(first) = arcs.first().copied() else {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "rounded body projection has no arc witness".into(),
        });
    };
    if arcs.iter().any(|arc| {
        !arc.0.is_finite()
            || !arc.1.is_finite()
            || !arc.2.is_finite()
            || !arc.3.is_finite()
            || arc.2 <= 0.0
            || (arc.2 - first.2).abs() > 1.0e-6
    }) {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "rounded body projected arc witnesses disagree".into(),
        });
    }
    Ok((DrawingPoint::new(first.0, first.1), first.2, first.3))
}

fn view_source<'a>(
    projection: &'a ProjectionModel,
    id: &scc_projection::ProjectionPrimitiveId,
) -> Option<&'a SpatialElementRef> {
    projection.sources.element_for_primitive(id)
}

fn source_matches_requested(
    requested: &SpatialElementRef,
    actual: Option<&SpatialElementRef>,
) -> bool {
    actual.is_some_and(|actual| {
        actual == requested
            || matches!(
                (requested, actual),
                (
                    SpatialElementRef::Body,
                    SpatialElementRef::BodySegment { .. }
                )
            )
    })
}

fn linear_layout(
    transform: &DrawingViewTransform,
    witness: DrawingBounds,
    geometry: DrawingBounds,
    axis: LinearAxis,
) -> DimensionLayoutGeometry {
    match axis {
        LinearAxis::Horizontal => {
            let x1 = transform
                .model_to_sheet(DrawingPoint::new(witness.min_x, witness.min_y))
                .x;
            let x2 = transform
                .model_to_sheet(DrawingPoint::new(witness.max_x, witness.min_y))
                .x;
            let y = geometry.min_y - DEFAULT_DIMENSION_OFFSET_MM;
            DimensionLayoutGeometry::Linear {
                dimension_line: LayoutSegment {
                    start: DrawingPoint::new(x1, y),
                    end: DrawingPoint::new(x2, y),
                },
                extension_lines: [
                    LayoutSegment {
                        start: DrawingPoint::new(x1, y + DEFAULT_EXTENSION_OVERHANG_MM),
                        end: DrawingPoint::new(
                            x1,
                            transform
                                .model_to_sheet(DrawingPoint::new(witness.min_x, witness.min_y))
                                .y
                                - DEFAULT_EXTENSION_GAP_MM,
                        ),
                    },
                    LayoutSegment {
                        start: DrawingPoint::new(x2, y + DEFAULT_EXTENSION_OVERHANG_MM),
                        end: DrawingPoint::new(
                            x2,
                            transform
                                .model_to_sheet(DrawingPoint::new(witness.max_x, witness.min_y))
                                .y
                                - DEFAULT_EXTENSION_GAP_MM,
                        ),
                    },
                ],
                arrows: [
                    ArrowAnchor {
                        point: DrawingPoint::new(x1, y),
                        direction: DrawingPoint::new(1.0, 0.0),
                    },
                    ArrowAnchor {
                        point: DrawingPoint::new(x2, y),
                        direction: DrawingPoint::new(-1.0, 0.0),
                    },
                ],
                text_anchor: DrawingPoint::new((x1 + x2) / 2.0, y - DEFAULT_DIMENSION_TEXT_GAP_MM),
                text_orientation: TextOrientation::Horizontal,
            }
        }
        LinearAxis::Vertical => {
            let y1 = transform
                .model_to_sheet(DrawingPoint::new(witness.min_x, witness.min_y))
                .y;
            let y2 = transform
                .model_to_sheet(DrawingPoint::new(witness.min_x, witness.max_y))
                .y;
            let x = geometry.min_x - DEFAULT_DIMENSION_OFFSET_MM;
            DimensionLayoutGeometry::Linear {
                dimension_line: LayoutSegment {
                    start: DrawingPoint::new(x, y1),
                    end: DrawingPoint::new(x, y2),
                },
                extension_lines: [
                    LayoutSegment {
                        start: DrawingPoint::new(x + DEFAULT_EXTENSION_OVERHANG_MM, y1),
                        end: DrawingPoint::new(
                            transform
                                .model_to_sheet(DrawingPoint::new(witness.min_x, witness.min_y))
                                .x
                                - DEFAULT_EXTENSION_GAP_MM,
                            y1,
                        ),
                    },
                    LayoutSegment {
                        start: DrawingPoint::new(x + DEFAULT_EXTENSION_OVERHANG_MM, y2),
                        end: DrawingPoint::new(
                            transform
                                .model_to_sheet(DrawingPoint::new(witness.min_x, witness.max_y))
                                .x
                                - DEFAULT_EXTENSION_GAP_MM,
                            y2,
                        ),
                    },
                ],
                arrows: [
                    ArrowAnchor {
                        point: DrawingPoint::new(x, y1),
                        direction: DrawingPoint::new(0.0, 1.0),
                    },
                    ArrowAnchor {
                        point: DrawingPoint::new(x, y2),
                        direction: DrawingPoint::new(0.0, -1.0),
                    },
                ],
                text_anchor: DrawingPoint::new(x - DEFAULT_DIMENSION_TEXT_GAP_MM, (y1 + y2) / 2.0),
                text_orientation: TextOrientation::Vertical,
            }
        }
    }
}

fn slot_callout_layout(
    transform: &DrawingViewTransform,
    slot: &scc_spatial::SlotCut,
) -> DimensionLayoutGeometry {
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for segment in &slot.segments {
        let Some((_x0, x1, y0, y1)) = segment.bounds() else {
            continue;
        };
        max_x = max_x.max(x1);
        min_y = min_y.min(y0);
        max_y = max_y.max(y1);
    }
    let right = transform.model_to_sheet(DrawingPoint::new(max_x, (min_y + max_y) / 2.0));
    let anchor = DrawingPoint::new(right.x, right.y);
    let leader_end = DrawingPoint::new(
        anchor.x + DEFAULT_DIMENSION_OFFSET_MM,
        anchor.y + DEFAULT_DIMENSION_OFFSET_MM,
    );
    let text_anchor = DrawingPoint::new(leader_end.x + DEFAULT_DIMENSION_TEXT_GAP_MM, leader_end.y);
    DimensionLayoutGeometry::PatternCallout {
        anchor_on_pattern: anchor,
        leader: LayoutSegment {
            start: anchor,
            end: leader_end,
        },
        text_anchor,
        footprint: DrawingBounds {
            min_x: anchor.x.min(text_anchor.x),
            min_y: anchor.y.min(text_anchor.y) - 1.0,
            max_x: anchor.x.max(text_anchor.x),
            max_y: anchor.y.max(text_anchor.y) + 1.0,
        },
    }
}

pub(crate) fn diameter_layout(
    transform: &DrawingViewTransform,
    center: DrawingPoint,
    radius: f64,
    id: &DrawingDimensionId,
) -> DimensionLayoutGeometry {
    let r = radius * transform.scale.0;
    let angle = DIAMETER_AXIS_ANGLE_DEG.to_radians();
    let (dx, dy) = (angle.cos(), angle.sin());
    let a = DrawingPoint::new(center.x - r * dx, center.y - r * dy);
    let b = DrawingPoint::new(center.x + r * dx, center.y + r * dy);
    let _ = id;
    DimensionLayoutGeometry::Diameter {
        center,
        axis_line: LayoutSegment { start: a, end: b },
        arrows: [
            ArrowAnchor {
                point: a,
                direction: DrawingPoint::new(dx, dy),
            },
            ArrowAnchor {
                point: b,
                direction: DrawingPoint::new(-dx, -dy),
            },
        ],
        text_anchor: DrawingPoint::new(
            b.x + dx * DEFAULT_DIMENSION_TEXT_GAP_MM,
            b.y + dy * DEFAULT_DIMENSION_TEXT_GAP_MM,
        ),
    }
}

fn radius_layout(
    transform: &DrawingViewTransform,
    center: DrawingPoint,
    radius: f64,
    start_angle: f64,
) -> DimensionLayoutGeometry {
    let center = transform.model_to_sheet(center);
    let direction = DrawingPoint::new(start_angle.cos(), start_angle.sin());
    let boundary = DrawingPoint::new(
        center.x + radius * transform.scale.0 * direction.x,
        center.y + radius * transform.scale.0 * direction.y,
    );
    let leader_end = DrawingPoint::new(
        boundary.x + DEFAULT_DIMENSION_OFFSET_MM * direction.x,
        boundary.y + DEFAULT_DIMENSION_OFFSET_MM * direction.y,
    );
    DimensionLayoutGeometry::Radius {
        center,
        radius_line: LayoutSegment {
            start: center,
            end: leader_end,
        },
        arrow: ArrowAnchor {
            point: boundary,
            direction: DrawingPoint::new(-direction.x, -direction.y),
        },
        text_anchor: DrawingPoint::new(leader_end.x + DEFAULT_DIMENSION_TEXT_GAP_MM, leader_end.y),
    }
}

fn add_center_geometry(
    out: &mut BTreeMap<DrawingCenterGeometryId, DrawingCenterGeometry>,
    drawing_id: &crate::DrawingPrimitiveId,
    feature: &FeatureId,
    entity: &EntityId,
    center: DrawingPoint,
    radius: f64,
    transform: &DrawingViewTransform,
) {
    let c = transform.model_to_sheet(center);
    let r = radius * transform.scale.0;
    let half = DEFAULT_CENTER_MARK_HALF_LENGTH_MM;
    let overhang = DEFAULT_CENTERLINE_OVERHANG_MM;
    out.insert(
        DrawingCenterGeometryId(format!("phase11.center_mark:{}", drawing_id.0)),
        DrawingCenterGeometry::CenterMark(CenterMark {
            source_feature: feature.clone(),
            source_entity: Some(entity.clone()),
            horizontal: LayoutSegment {
                start: DrawingPoint::new(c.x - half, c.y),
                end: DrawingPoint::new(c.x + half, c.y),
            },
            vertical: LayoutSegment {
                start: DrawingPoint::new(c.x, c.y - half),
                end: DrawingPoint::new(c.x, c.y + half),
            },
        }),
    );
    for (axis, name) in [
        (CenterlineAxis::Horizontal, "h"),
        (CenterlineAxis::Vertical, "v"),
    ] {
        let segment = match axis {
            CenterlineAxis::Horizontal => LayoutSegment {
                start: DrawingPoint::new(c.x - r - overhang, c.y),
                end: DrawingPoint::new(c.x + r + overhang, c.y),
            },
            CenterlineAxis::Vertical => LayoutSegment {
                start: DrawingPoint::new(c.x, c.y - r - overhang),
                end: DrawingPoint::new(c.x, c.y + r + overhang),
            },
        };
        out.insert(
            DrawingCenterGeometryId(format!("phase11.{}_centerline:{}", name, drawing_id.0)),
            DrawingCenterGeometry::Centerline(Centerline {
                source_feature: feature.clone(),
                source_entity: Some(entity.clone()),
                axis,
                segment,
            }),
        );
    }
}

fn include_min(bounds: DrawingBounds, min_x: &mut f64, min_y: &mut f64) {
    *min_x = (*min_x).min(bounds.min_x);
    *min_y = (*min_y).min(bounds.min_y);
}

fn translate_point(point: &mut DrawingPoint, dx: f64, dy: f64) {
    point.x += dx;
    point.y += dy;
}

fn translate_segment(segment: &mut LayoutSegment, dx: f64, dy: f64) {
    translate_point(&mut segment.start, dx, dy);
    translate_point(&mut segment.end, dx, dy);
}

fn translate_layout(geometry: &mut DimensionLayoutGeometry, dx: f64, dy: f64) {
    match geometry {
        DimensionLayoutGeometry::Linear {
            dimension_line,
            extension_lines,
            arrows,
            text_anchor,
            ..
        } => {
            translate_segment(dimension_line, dx, dy);
            for segment in extension_lines {
                translate_segment(segment, dx, dy);
            }
            for arrow in arrows {
                translate_point(&mut arrow.point, dx, dy);
            }
            translate_point(text_anchor, dx, dy);
        }
        DimensionLayoutGeometry::Diameter {
            center,
            axis_line,
            arrows,
            text_anchor,
        } => {
            translate_point(center, dx, dy);
            translate_segment(axis_line, dx, dy);
            for arrow in arrows {
                translate_point(&mut arrow.point, dx, dy);
            }
            translate_point(text_anchor, dx, dy);
        }
        DimensionLayoutGeometry::Radius {
            center,
            radius_line,
            arrow,
            text_anchor,
        } => {
            translate_point(center, dx, dy);
            translate_segment(radius_line, dx, dy);
            translate_point(&mut arrow.point, dx, dy);
            translate_point(text_anchor, dx, dy);
        }
        DimensionLayoutGeometry::PatternCallout {
            anchor_on_pattern,
            leader,
            text_anchor,
            footprint,
        } => {
            translate_point(anchor_on_pattern, dx, dy);
            translate_segment(leader, dx, dy);
            translate_point(text_anchor, dx, dy);
            footprint.min_x += dx;
            footprint.max_x += dx;
            footprint.min_y += dy;
            footprint.max_y += dy;
        }
    }
}

fn translate_center(element: &mut DrawingCenterGeometry, dx: f64, dy: f64) {
    match element {
        DrawingCenterGeometry::CenterMark(mark) => {
            translate_segment(&mut mark.horizontal, dx, dy);
            translate_segment(&mut mark.vertical, dx, dy);
        }
        DrawingCenterGeometry::Centerline(line) => translate_segment(&mut line.segment, dx, dy),
    }
}

#[derive(Debug, Clone)]
struct PresentedHole {
    feature: FeatureId,
    entity: EntityId,
    center: DrawingPoint,
    radius: f64,
}

fn build_three_view_semantic_annotations(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    engineering: &scc_engineering::EngineeringModel,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
) -> Result<(), DrawingError> {
    let resolver = ResolvedEngineeringView::build(engineering).map_err(|error| {
        DrawingError::InvalidDrawingDimension {
            reason: format!("pattern semantic resolution failed: {error}"),
        }
    })?;
    let front_id = DrawingViewId("front".into());
    let (transform, geometry_bounds) = {
        let front = views
            .get(&front_id)
            .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                reason: "three-view front is missing".into(),
            })?;
        (front.transform, front.sheet_bounds())
    };
    let front_projection = projection
        .views
        .get(&PrincipalViewKind::Front)
        .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
            reason: "projection front is missing".into(),
        })?;
    let body_bounds =
        projected_bounds_for_source(front_projection, &SpatialElementRef::Body, projection)?;
    let body_source = body_source(spatial)?;
    let linear_body_datums = matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Rectangle { .. }
    );
    let mut by_feature: BTreeMap<FeatureId, Vec<PresentedHole>> = BTreeMap::new();
    for hole in presented_front_holes(views, spatial, projection, source_map)? {
        by_feature
            .entry(hole.feature.clone())
            .or_default()
            .push(hole);
    }
    for (feature_id, mut holes) in by_feature {
        holes.sort_by(|a, b| a.entity.cmp(&b.entity));
        let feature = engineering.features.get(&feature_id).ok_or_else(|| {
            DrawingError::InvalidDrawingDimension {
                reason: format!("presented hole source feature {} is missing", feature_id),
            }
        })?;
        match feature {
            EngineeringFeature::Hole { .. } => {
                let semantic =
                    hole_callout_semantic(engineering, spatial, &feature_id, &holes, None)?;
                let diameter_id =
                    DrawingDimensionId(format!("three_view.hole_diameter:{}", feature_id));
                let front = views.get_mut(&front_id).ok_or_else(|| {
                    DrawingError::InvalidSourceIntegrity {
                        reason: "three-view front is missing".into(),
                    }
                })?;
                front.dimensions.insert(
                    diameter_id.clone(),
                    DrawingDimension {
                        id: diameter_id.clone(),
                        target: DrawingDimensionTarget::Feature {
                            feature: feature_id.clone(),
                        },
                        kind: DrawingDimensionKind::Diameter,
                        specification: None,
                        value: DimensionValue::HoleCallout {
                            semantic,
                            pattern: None,
                        },
                    },
                );
                let hole = holes
                    .first()
                    .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                        reason: format!("hole feature {} has no layout witness", feature_id),
                    })?;
                let center = transform.model_to_sheet(hole.center);
                let layout = diameter_layout(&transform, center, hole.radius, &diameter_id);
                front.dimension_layouts.insert(
                    diameter_id.clone(),
                    DrawingDimensionLayout {
                        dimension: diameter_id,
                        geometry: layout,
                    },
                );
                if linear_body_datums {
                    if let Some(hole) = holes.first() {
                        add_location_dimension(
                            views,
                            &front_id,
                            engineering,
                            &transform,
                            geometry_bounds,
                            &feature_id,
                            hole,
                            HoleDatum::LeftEdge,
                            body_bounds.min_x,
                            body_bounds.min_y,
                            &body_source,
                        )?;
                        add_location_dimension(
                            views,
                            &front_id,
                            engineering,
                            &transform,
                            geometry_bounds,
                            &feature_id,
                            hole,
                            HoleDatum::BottomEdge,
                            body_bounds.min_x,
                            body_bounds.min_y,
                            &body_source,
                        )?;
                    }
                }
            }
            EngineeringFeature::HolePattern { pattern, .. } => {
                let old_id = DrawingDimensionId(format!("three_view.hole_diameter:{}", feature_id));
                let pattern_id = DrawingDimensionId(format!("three_view.pattern:{}", feature_id));
                let representative =
                    holes
                        .first()
                        .ok_or_else(|| DrawingError::InvalidDrawingDimension {
                            reason: format!("pattern {} has no presented circles", feature_id),
                        })?;
                if holes
                    .iter()
                    .any(|hole| (hole.radius - representative.radius).abs() > 1.0e-6)
                {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "pattern {} has inconsistent resolved cut radii",
                            feature_id
                        ),
                    });
                }
                if let Some(front) = views.get_mut(&front_id) {
                    front.dimensions.remove(&old_id);
                    front.dimension_layouts.remove(&old_id);
                }
                let shape = pattern_shape(&resolver, feature, &feature_id, representative)?;
                let semantic =
                    hole_callout_semantic(engineering, spatial, &feature_id, &holes, Some(&shape))?;
                let front = views.get_mut(&front_id).ok_or_else(|| {
                    DrawingError::InvalidSourceIntegrity {
                        reason: "three-view front is missing".into(),
                    }
                })?;
                front.dimensions.insert(
                    pattern_id.clone(),
                    DrawingDimension {
                        id: pattern_id.clone(),
                        target: DrawingDimensionTarget::Feature {
                            feature: feature_id.clone(),
                        },
                        kind: DrawingDimensionKind::Pattern,
                        specification: None,
                        value: DimensionValue::HoleCallout {
                            semantic,
                            pattern: Some(shape),
                        },
                    },
                );
                front.dimension_layouts.insert(
                    pattern_id.clone(),
                    DrawingDimensionLayout {
                        dimension: pattern_id,
                        geometry: pattern_layout(&transform, &holes),
                    },
                );
                match pattern {
                    HolePattern::Circular { .. } => {
                        let center = average_hole_center(&holes)?;
                        if linear_body_datums {
                            add_pattern_location_dimension(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                representative,
                                center,
                                HoleDatum::LeftEdge,
                                body_bounds.min_x,
                                body_bounds.min_y,
                                &body_source,
                            )?;
                            add_pattern_location_dimension(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                representative,
                                center,
                                HoleDatum::BottomEdge,
                                body_bounds.min_x,
                                body_bounds.min_y,
                                &body_source,
                            )?;
                        }
                    }
                    HolePattern::Grid { .. } => {
                        let rows = resolver
                            .slot_value(
                                feature,
                                scc_engineering::binding::FeatureParameterSlot::Rows,
                            )
                            .map_err(|e| DrawingError::InvalidDrawingDimension {
                                reason: e.to_string(),
                            })? as u32;
                        let columns = resolver
                            .slot_value(
                                feature,
                                scc_engineering::binding::FeatureParameterSlot::Columns,
                            )
                            .map_err(|e| DrawingError::InvalidDrawingDimension {
                                reason: e.to_string(),
                            })? as u32;
                        let first = holes
                            .iter()
                            .min_by(|a, b| {
                                a.center
                                    .y
                                    .total_cmp(&b.center.y)
                                    .then(a.center.x.total_cmp(&b.center.x))
                            })
                            .unwrap();
                        if linear_body_datums {
                            add_location_dimension(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                first,
                                HoleDatum::LeftEdge,
                                body_bounds.min_x,
                                body_bounds.min_y,
                                &body_source,
                            )?;
                            add_location_dimension(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                first,
                                HoleDatum::BottomEdge,
                                body_bounds.min_x,
                                body_bounds.min_y,
                                &body_source,
                            )?;
                        }
                        if columns > 1 {
                            let pair = row_pair(&holes)?;
                            add_pattern_spacing(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                pair.0,
                                pair.1,
                                LinearAxis::Horizontal,
                            )?;
                        }
                        if rows > 1 {
                            let pair = column_pair(&holes)?;
                            add_pattern_spacing(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                pair.0,
                                pair.1,
                                LinearAxis::Vertical,
                            )?;
                        }
                    }
                    HolePattern::Corner { .. } => {
                        let left_bottom = holes
                            .iter()
                            .min_by(|a, b| {
                                a.center
                                    .x
                                    .total_cmp(&b.center.x)
                                    .then(a.center.y.total_cmp(&b.center.y))
                            })
                            .unwrap();
                        let right_bottom = holes
                            .iter()
                            .filter(|h| (h.center.y - left_bottom.center.y).abs() <= 1.0e-6)
                            .max_by(|a, b| a.center.x.total_cmp(&b.center.x))
                            .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                                reason: format!(
                                    "corner pattern {} has no horizontal spacing witness",
                                    feature_id
                                ),
                            })?;
                        let left_top = holes
                            .iter()
                            .filter(|h| (h.center.x - left_bottom.center.x).abs() <= 1.0e-6)
                            .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
                            .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                                reason: format!(
                                    "corner pattern {} has no vertical spacing witness",
                                    feature_id
                                ),
                            })?;
                        if linear_body_datums {
                            add_location_dimension(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                left_bottom,
                                HoleDatum::LeftEdge,
                                body_bounds.min_x,
                                body_bounds.min_y,
                                &body_source,
                            )?;
                            add_location_dimension(
                                views,
                                &front_id,
                                engineering,
                                &transform,
                                geometry_bounds,
                                &feature_id,
                                left_bottom,
                                HoleDatum::BottomEdge,
                                body_bounds.min_x,
                                body_bounds.min_y,
                                &body_source,
                            )?;
                        }
                        add_pattern_spacing(
                            views,
                            &front_id,
                            engineering,
                            &transform,
                            geometry_bounds,
                            &feature_id,
                            left_bottom,
                            right_bottom,
                            LinearAxis::Horizontal,
                        )?;
                        add_pattern_spacing(
                            views,
                            &front_id,
                            engineering,
                            &transform,
                            geometry_bounds,
                            &feature_id,
                            left_bottom,
                            left_top,
                            LinearAxis::Vertical,
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn presented_front_holes(
    views: &BTreeMap<DrawingViewId, DrawingView>,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
) -> Result<Vec<PresentedHole>, DrawingError> {
    let front = views.get(&DrawingViewId("front".into())).ok_or_else(|| {
        DrawingError::InvalidSourceIntegrity {
            reason: "front missing".into(),
        }
    })?;
    let mut holes = Vec::new();
    for (drawing_id, primitive) in &front.primitives {
        let DrawingPrimitive::Circle {
            role: DrawingPrimitiveRole::Object | DrawingPrimitiveRole::Hidden,
            center,
            radius,
        } = primitive
        else {
            continue;
        };
        let projected = source_map
            .projected_for_primitive(drawing_id)
            .ok_or_else(|| DrawingError::MissingCenterSource {
                reason: format!("primitive {} has no projected provenance", drawing_id),
            })?;
        let (feature, entity) = source_for_projected(projected, spatial, projection)?;
        if spatial.cuts.values().any(|cut| cut.source_entity == entity) {
            holes.push(PresentedHole {
                feature,
                entity,
                center: *center,
                radius: *radius,
            });
        }
    }
    Ok(holes)
}

fn hole_callout_semantic(
    engineering: &scc_engineering::EngineeringModel,
    spatial: &SpatialModel,
    feature_id: &FeatureId,
    holes: &[PresentedHole],
    pattern: Option<&PatternShape>,
) -> Result<HoleCalloutSemantic, DrawingError> {
    let representative = holes
        .iter()
        .min_by(|a, b| a.entity.cmp(&b.entity))
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: format!("hole feature {} has no presented witness", feature_id),
        })?;
    let representative_cut = spatial
        .cut_for_entity(&representative.entity)
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: format!(
                "hole feature {} representative {} has no spatial cut",
                feature_id, representative.entity
            ),
        })?;
    let cuts = spatial.cuts_for_feature(feature_id);
    if cuts.is_empty() {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!("hole feature {} has no spatial cut witnesses", feature_id),
        });
    }

    for hole in holes {
        let cut = spatial.cut_for_entity(&hole.entity).ok_or_else(|| {
            DrawingError::InvalidDimensionWitness {
                reason: format!(
                    "hole feature {} presented entity {} has no spatial witness",
                    feature_id, hole.entity
                ),
            }
        })?;
        if (hole.radius - cut.radius).abs() > 1.0e-6 {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: format!(
                    "hole feature {} projected radius disagrees with spatial cut {}",
                    feature_id, hole.entity
                ),
            });
        }
    }

    let extent = engineering
        .spatial
        .hole_extents
        .get(feature_id)
        .ok_or_else(|| DrawingError::InvalidDrawingDimension {
            reason: format!("hole feature {} has no typed HoleExtent", feature_id),
        })?;
    let body_z = spatial.body.z;
    for (_, cut) in &cuts {
        if !cut.radius.is_finite()
            || cut.radius <= 0.0
            || !cut.z.min.is_finite()
            || !cut.z.max.is_finite()
            || cut.z.max <= cut.z.min
        {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: format!("hole feature {} has an invalid spatial cut", feature_id),
            });
        }
    }

    match extent {
        HoleExtent::ThroughAll => {
            for (_, cut) in &cuts {
                if !matches!(cut.kind, SpatialCutKind::ThroughAll)
                    || !same_z_interval(cut.z, body_z)
                {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "hole feature {} ThroughAll semantic disagrees with spatial witness",
                            feature_id
                        ),
                    });
                }
            }
        }
        HoleExtent::Blind { start_face, .. } => {
            let reference_depth = representative_cut.z.max - representative_cut.z.min;
            if !reference_depth.is_finite() || reference_depth <= 0.0 {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!(
                        "hole feature {} has an invalid blind depth witness",
                        feature_id
                    ),
                });
            }
            for (_, cut) in &cuts {
                let kind_side = match cut.kind {
                    SpatialCutKind::Blind { start_face: side } => side,
                    SpatialCutKind::ThroughAll => {
                        return Err(DrawingError::InvalidDimensionWitness {
                            reason: format!(
                                "hole feature {} blind semantic disagrees with ThroughAll cut",
                                feature_id
                            ),
                        })
                    }
                };
                if kind_side != *start_face
                    || !same_float(cut.z.max - cut.z.min, reference_depth)
                    || !blind_interval_matches(cut.z, body_z, *start_face)
                {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "hole feature {} blind semantic disagrees with spatial witness",
                            feature_id
                        ),
                    });
                }
            }
        }
    }

    let diameter = DimensionComponent {
        source: DimensionValueSource::ResolvedGeometry {
            feature: feature_id.clone(),
            entity: representative.entity.clone(),
        },
        resolved: ResolvedDimensionValue::LengthMm {
            mm: representative_cut.radius * 2.0,
        },
        specification: drawing_specification(
            engineering,
            EngineeringDimensionRef::FeatureSlot {
                feature: feature_id.clone(),
                slot: FeatureParameterSlot::Diameter,
            },
        ),
    };
    let quantity = match pattern {
        Some(PatternShape::Circular { count, .. }) => Some(count.clone()),
        Some(PatternShape::Grid { .. } | PatternShape::Corner { .. }) | None => None,
    };
    let extent = match extent {
        HoleExtent::ThroughAll => HoleCalloutExtent::Through,
        HoleExtent::Blind { start_face, .. } => HoleCalloutExtent::Blind {
            depth: DimensionComponent {
                source: DimensionValueSource::ResolvedHoleDepth {
                    feature: feature_id.clone(),
                    entity: representative.entity.clone(),
                },
                resolved: ResolvedDimensionValue::LengthMm {
                    mm: representative_cut.z.max - representative_cut.z.min,
                },
                specification: drawing_specification(
                    engineering,
                    EngineeringDimensionRef::HoleDepth {
                        feature: feature_id.clone(),
                    },
                ),
            },
            side: *start_face,
        },
    };
    let witnesses = cuts
        .into_iter()
        .map(|(_, cut)| HoleCalloutWitness {
            entity: cut.source_entity.clone(),
            radius: cut.radius,
            z: cut.z,
            kind: cut.kind,
        })
        .collect();
    Ok(HoleCalloutSemantic {
        diameter,
        quantity,
        show_pitch_circle_diameter: pattern.is_some()
            && matches!(
                spatial.body.footprint,
                scc_spatial::SpatialFootprint::Circle { .. }
            ),
        extent,
        provenance: HoleCalloutProvenance {
            feature: feature_id.clone(),
            representative_entity: representative.entity.clone(),
            witnesses,
        },
    })
}

fn same_float(first: f64, second: f64) -> bool {
    first.is_finite() && second.is_finite() && (first - second).abs() <= 1.0e-6
}

fn same_z_interval(first: scc_spatial::ZInterval, second: scc_spatial::ZInterval) -> bool {
    same_float(first.min, second.min) && same_float(first.max, second.max)
}

fn blind_interval_matches(
    interval: scc_spatial::ZInterval,
    body: scc_spatial::ZInterval,
    side: scc_engineering::HoleStartFace,
) -> bool {
    match side {
        scc_engineering::HoleStartFace::Base => {
            same_float(interval.min, body.min) && interval.max < body.max - 1.0e-6
        }
        scc_engineering::HoleStartFace::Opposite => {
            same_float(interval.max, body.max) && interval.min > body.min + 1.0e-6
        }
    }
}

fn semantic_component(
    resolver: &ResolvedEngineeringView<'_>,
    feature: &EngineeringFeature,
    feature_id: &FeatureId,
    slot: scc_engineering::binding::FeatureParameterSlot,
    field: PatternSemanticField,
) -> Result<DimensionComponent, DrawingError> {
    let value =
        resolver
            .slot_value(feature, slot)
            .map_err(|e| DrawingError::InvalidDrawingDimension {
                reason: e.to_string(),
            })?;
    let resolved = match field {
        PatternSemanticField::Count
        | PatternSemanticField::Rows
        | PatternSemanticField::Columns => {
            if !value.is_finite()
                || value.fract() != 0.0
                || !(1.0..=f64::from(u32::MAX)).contains(&value)
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!("pattern semantic {} is not a valid count", feature_id),
                });
            }
            ResolvedDimensionValue::Count {
                count: value as u32,
            }
        }
        _ => ResolvedDimensionValue::LengthMm { mm: value },
    };
    if !resolved.is_valid() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("pattern semantic {} is invalid", feature_id),
        });
    }
    Ok(DimensionComponent {
        source: DimensionValueSource::ResolvedPatternSemantic {
            feature: feature_id.clone(),
            field,
        },
        resolved,
        specification: drawing_specification(
            resolver.model(),
            EngineeringDimensionRef::FeatureSlot {
                feature: feature_id.clone(),
                slot,
            },
        ),
    })
}

fn pattern_shape(
    resolver: &ResolvedEngineeringView<'_>,
    feature: &EngineeringFeature,
    feature_id: &FeatureId,
    representative: &PresentedHole,
) -> Result<PatternShape, DrawingError> {
    use scc_engineering::binding::FeatureParameterSlot as Slot;
    let diameter = DimensionComponent {
        source: DimensionValueSource::ResolvedGeometry {
            feature: feature_id.clone(),
            entity: representative.entity.clone(),
        },
        resolved: ResolvedDimensionValue::LengthMm {
            mm: representative.radius * 2.0,
        },
        specification: drawing_specification(
            resolver.model(),
            EngineeringDimensionRef::FeatureSlot {
                feature: feature_id.clone(),
                slot: Slot::Diameter,
            },
        ),
    };
    match feature {
        EngineeringFeature::HolePattern {
            pattern: HolePattern::Circular { .. },
            ..
        } => Ok(PatternShape::Circular {
            count: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::Count,
                PatternSemanticField::Count,
            )?,
            pitch_circle_diameter: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::PitchCircleDiameter,
                PatternSemanticField::PitchCircleDiameter,
            )?,
            hole_diameter: diameter,
        }),
        EngineeringFeature::HolePattern {
            pattern: HolePattern::Grid { .. },
            ..
        } => Ok(PatternShape::Grid {
            rows: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::Rows,
                PatternSemanticField::Rows,
            )?,
            columns: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::Columns,
                PatternSemanticField::Columns,
            )?,
            spacing_x: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::SpacingX,
                PatternSemanticField::SpacingX,
            )?,
            spacing_y: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::SpacingY,
                PatternSemanticField::SpacingY,
            )?,
            hole_diameter: diameter,
        }),
        EngineeringFeature::HolePattern {
            pattern: HolePattern::Corner { .. },
            ..
        } => Ok(PatternShape::Corner {
            offset_x: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::OffsetX,
                PatternSemanticField::OffsetX,
            )?,
            offset_y: semantic_component(
                resolver,
                feature,
                feature_id,
                Slot::OffsetY,
                PatternSemanticField::OffsetY,
            )?,
            hole_diameter: diameter,
        }),
        _ => Err(DrawingError::InvalidDrawingDimension {
            reason: format!("feature {} is not a hole pattern", feature_id),
        }),
    }
}

fn pattern_layout(
    transform: &DrawingViewTransform,
    holes: &[PresentedHole],
) -> DimensionLayoutGeometry {
    let mut footprint = DrawingBounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for hole in holes {
        let center = transform.model_to_sheet(hole.center);
        let radius = hole.radius * transform.scale.0;
        footprint.min_x = footprint.min_x.min(center.x - radius);
        footprint.min_y = footprint.min_y.min(center.y - radius);
        footprint.max_x = footprint.max_x.max(center.x + radius);
        footprint.max_y = footprint.max_y.max(center.y + radius);
    }
    let representative = holes
        .iter()
        .min_by(|a, b| a.entity.cmp(&b.entity))
        .expect("pattern layout requires a representative hole");
    let representative_center = transform.model_to_sheet(representative.center);
    let representative_radius = representative.radius * transform.scale.0;
    let angle = DIAMETER_AXIS_ANGLE_DEG.to_radians();
    let anchor = DrawingPoint::new(
        representative_center.x + representative_radius * angle.cos(),
        representative_center.y + representative_radius * angle.sin(),
    );
    let leader_end = DrawingPoint::new(
        anchor.x + DEFAULT_DIMENSION_OFFSET_MM,
        anchor.y + DEFAULT_DIMENSION_OFFSET_MM,
    );
    DimensionLayoutGeometry::PatternCallout {
        anchor_on_pattern: anchor,
        leader: LayoutSegment {
            start: anchor,
            end: leader_end,
        },
        text_anchor: DrawingPoint::new(leader_end.x + DEFAULT_DIMENSION_TEXT_GAP_MM, leader_end.y),
        footprint,
    }
}

fn average_hole_center(holes: &[PresentedHole]) -> Result<DrawingPoint, DrawingError> {
    if holes.is_empty() {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "pattern has no center witness".into(),
        });
    }
    let (x, y) = holes.iter().fold((0.0, 0.0), |(x, y), hole| {
        (x + hole.center.x, y + hole.center.y)
    });
    let count = holes.len() as f64;
    let center = DrawingPoint::new(x / count, y / count);
    if !center.x.is_finite() || !center.y.is_finite() {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "pattern center witness is not finite".into(),
        });
    }
    Ok(center)
}

#[allow(clippy::too_many_arguments)]
fn add_pattern_location_dimension(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    front_id: &DrawingViewId,
    engineering: &scc_engineering::EngineeringModel,
    transform: &DrawingViewTransform,
    geometry_bounds: DrawingBounds,
    feature: &FeatureId,
    representative: &PresentedHole,
    center: DrawingPoint,
    datum: HoleDatum,
    body_min_x: f64,
    body_min_y: f64,
    body_source: &(FeatureId, EntityId),
) -> Result<(), DrawingError> {
    let (axis, value, suffix) = match datum {
        HoleDatum::LeftEdge => (LinearAxis::Horizontal, center.x - body_min_x, "x"),
        HoleDatum::BottomEdge => (LinearAxis::Vertical, center.y - body_min_y, "y"),
    };
    let id = DrawingDimensionId(format!("three_view.hole_location_{}:{}", suffix, feature));
    let dimension_axis = match datum {
        HoleDatum::LeftEdge => DimensionAxis::X,
        HoleDatum::BottomEdge => DimensionAxis::Y,
    };
    let specification = drawing_specification(
        engineering,
        EngineeringDimensionRef::HoleLocation {
            feature: feature.clone(),
            axis: dimension_axis,
        },
    );
    let source = DimensionValueSource::ResolvedPatternLocation {
        feature: feature.clone(),
        representative_entity: representative.entity.clone(),
        datum_feature: body_source.0.clone(),
        datum_entity: body_source.1.clone(),
        datum,
    };
    let witness = DrawingBounds {
        min_x: body_min_x,
        min_y: body_min_y,
        max_x: center.x,
        max_y: center.y,
    };
    insert_resolved_linear(
        views,
        front_id,
        id,
        feature.clone(),
        source,
        axis,
        value,
        specification,
        linear_layout(transform, witness, geometry_bounds, axis),
    )
}

#[allow(clippy::too_many_arguments)]
fn add_location_dimension(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    front_id: &DrawingViewId,
    engineering: &scc_engineering::EngineeringModel,
    transform: &DrawingViewTransform,
    geometry_bounds: DrawingBounds,
    feature: &FeatureId,
    hole: &PresentedHole,
    datum: HoleDatum,
    body_min_x: f64,
    body_min_y: f64,
    body_source: &(FeatureId, EntityId),
) -> Result<(), DrawingError> {
    let (axis, value, witness, suffix) = match datum {
        HoleDatum::LeftEdge => (
            LinearAxis::Horizontal,
            hole.center.x - body_min_x,
            DrawingBounds {
                min_x: body_min_x,
                min_y: body_min_y,
                max_x: hole.center.x,
                max_y: hole.center.y,
            },
            "x",
        ),
        HoleDatum::BottomEdge => (
            LinearAxis::Vertical,
            hole.center.y - body_min_y,
            DrawingBounds {
                min_x: body_min_x,
                min_y: body_min_y,
                max_x: hole.center.x,
                max_y: hole.center.y,
            },
            "y",
        ),
    };
    let id = DrawingDimensionId(format!("three_view.hole_location_{}:{}", suffix, feature));
    let dimension_axis = match datum {
        HoleDatum::LeftEdge => DimensionAxis::X,
        HoleDatum::BottomEdge => DimensionAxis::Y,
    };
    let specification = drawing_specification(
        engineering,
        EngineeringDimensionRef::HoleLocation {
            feature: feature.clone(),
            axis: dimension_axis,
        },
    );
    let source = DimensionValueSource::ResolvedHoleLocation {
        feature: feature.clone(),
        entity: hole.entity.clone(),
        datum_feature: body_source.0.clone(),
        datum_entity: body_source.1.clone(),
        datum,
    };
    insert_resolved_linear(
        views,
        front_id,
        id,
        feature.clone(),
        source,
        axis,
        value,
        specification,
        linear_layout(transform, witness, geometry_bounds, axis),
    )
}

#[allow(clippy::too_many_arguments)]
fn add_pattern_spacing(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    front_id: &DrawingViewId,
    engineering: &scc_engineering::EngineeringModel,
    transform: &DrawingViewTransform,
    geometry_bounds: DrawingBounds,
    feature: &FeatureId,
    first: &PresentedHole,
    second: &PresentedHole,
    axis: LinearAxis,
) -> Result<(), DrawingError> {
    let value = spacing_value(first, second, axis);
    let slot = match (engineering.features.get(feature), axis) {
        (
            Some(EngineeringFeature::HolePattern {
                pattern: HolePattern::Grid { .. },
                ..
            }),
            LinearAxis::Horizontal,
        ) => FeatureParameterSlot::SpacingX,
        (
            Some(EngineeringFeature::HolePattern {
                pattern: HolePattern::Grid { .. },
                ..
            }),
            LinearAxis::Vertical,
        ) => FeatureParameterSlot::SpacingY,
        (
            Some(EngineeringFeature::HolePattern {
                pattern: HolePattern::Corner { .. },
                ..
            }),
            LinearAxis::Horizontal,
        ) => FeatureParameterSlot::OffsetX,
        (
            Some(EngineeringFeature::HolePattern {
                pattern: HolePattern::Corner { .. },
                ..
            }),
            LinearAxis::Vertical,
        ) => FeatureParameterSlot::OffsetY,
        _ => {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "pattern {} has no typed spacing slot for {:?}",
                    feature, axis
                ),
            })
        }
    };
    let specification = drawing_specification(
        engineering,
        EngineeringDimensionRef::FeatureSlot {
            feature: feature.clone(),
            slot,
        },
    );
    let id = DrawingDimensionId(format!(
        "three_view.pattern_spacing_{}:{}",
        if axis == LinearAxis::Horizontal {
            "x"
        } else {
            "y"
        },
        feature
    ));
    let witness = DrawingBounds {
        min_x: first.center.x.min(second.center.x),
        min_y: first.center.y.min(second.center.y),
        max_x: first.center.x.max(second.center.x),
        max_y: first.center.y.max(second.center.y),
    };
    let source = DimensionValueSource::ResolvedPatternSpacing {
        feature: feature.clone(),
        first_entity: first.entity.clone(),
        second_entity: second.entity.clone(),
        axis,
    };
    insert_resolved_linear(
        views,
        front_id,
        id,
        feature.clone(),
        source,
        axis,
        value,
        specification,
        linear_layout(transform, witness, geometry_bounds, axis),
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_resolved_linear(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    front_id: &DrawingViewId,
    id: DrawingDimensionId,
    feature: FeatureId,
    source: DimensionValueSource,
    axis: LinearAxis,
    value: f64,
    specification: Option<DrawingDimensionSpecification>,
    layout: DimensionLayoutGeometry,
) -> Result<(), DrawingError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!("dimension {} has invalid resolved distance", id),
        });
    }
    let front = views
        .get_mut(front_id)
        .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
            reason: "front missing".into(),
        })?;
    front.dimensions.insert(
        id.clone(),
        DrawingDimension {
            id: id.clone(),
            target: DrawingDimensionTarget::Feature { feature },
            kind: DrawingDimensionKind::Linear { axis },
            specification,
            value: DimensionValue::Measured {
                source,
                resolved: ResolvedDimensionValue::LengthMm { mm: value },
            },
        },
    );
    front.dimension_layouts.insert(
        id.clone(),
        DrawingDimensionLayout {
            dimension: id,
            geometry: layout,
        },
    );
    Ok(())
}

fn spacing_value(first: &PresentedHole, second: &PresentedHole, axis: LinearAxis) -> f64 {
    match axis {
        LinearAxis::Horizontal => (second.center.x - first.center.x).abs(),
        LinearAxis::Vertical => (second.center.y - first.center.y).abs(),
    }
}

fn row_pair(holes: &[PresentedHole]) -> Result<(&PresentedHole, &PresentedHole), DrawingError> {
    let first = holes
        .iter()
        .min_by(|a, b| {
            a.center
                .y
                .total_cmp(&b.center.y)
                .then(a.center.x.total_cmp(&b.center.x))
        })
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: "pattern has no row witness".into(),
        })?;
    let second = holes
        .iter()
        .filter(|h| (h.center.y - first.center.y).abs() <= 1.0e-6 && h.entity != first.entity)
        .min_by(|a, b| a.center.x.total_cmp(&b.center.x))
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: "pattern has no second-column witness".into(),
        })?;
    if second.entity == first.entity {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "pattern has no distinct column witness".into(),
        });
    }
    Ok((first, second))
}

fn column_pair(holes: &[PresentedHole]) -> Result<(&PresentedHole, &PresentedHole), DrawingError> {
    let first = holes
        .iter()
        .min_by(|a, b| {
            a.center
                .x
                .total_cmp(&b.center.x)
                .then(a.center.y.total_cmp(&b.center.y))
        })
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: "pattern has no column witness".into(),
        })?;
    let second = holes
        .iter()
        .filter(|h| (h.center.x - first.center.x).abs() <= 1.0e-6)
        .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: "pattern has no second-row witness".into(),
        })?;
    if second.entity == first.entity {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: "pattern has no distinct row witness".into(),
        });
    }
    Ok((first, second))
}

/// Validate the ownership, source, witness and center role contract after
/// construction. This validator intentionally uses the same typed maps as the
/// builder; it does not infer semantics from ids or labels.
pub fn validate_three_view_annotations(
    views: &BTreeMap<DrawingViewId, DrawingView>,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
) -> Result<(), DrawingError> {
    let front = views.get(&DrawingViewId("front".into())).ok_or_else(|| {
        DrawingError::InvalidSourceIntegrity {
            reason: "front missing".into(),
        }
    })?;
    let top = views.get(&DrawingViewId("top".into())).ok_or_else(|| {
        DrawingError::InvalidSourceIntegrity {
            reason: "top missing".into(),
        }
    })?;
    let left = views.get(&DrawingViewId("left".into())).ok_or_else(|| {
        DrawingError::InvalidSourceIntegrity {
            reason: "left missing".into(),
        }
    })?;
    if !top.dimensions.is_empty() || !top.dimension_layouts.is_empty() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: "Top contains Phase 11.1 overall dimensions".into(),
        });
    }
    let circular_body = matches!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Circle { .. }
    );
    if circular_body {
        let diameter_id = DrawingDimensionId("three_view.overall_diameter".into());
        if !front.dimensions.contains_key(&diameter_id) {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: "Front is missing three_view.overall_diameter".into(),
            });
        }
        for id in ["three_view.overall_width", "three_view.overall_height"] {
            if front
                .dimensions
                .contains_key(&DrawingDimensionId(id.into()))
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!("circular Front must not contain {id}"),
                });
            }
        }
    } else {
        for id in ["three_view.overall_width", "three_view.overall_height"] {
            if !front
                .dimensions
                .contains_key(&DrawingDimensionId(id.into()))
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!("Front is missing {id}"),
                });
            }
        }
        if front
            .dimensions
            .contains_key(&DrawingDimensionId("three_view.overall_diameter".into()))
        {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: "rectangular Front must not contain overall diameter".into(),
            });
        }
    }
    if !left
        .dimensions
        .contains_key(&DrawingDimensionId("three_view.thickness".into()))
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: "Left is missing thickness".into(),
        });
    }
    for view in [front, left] {
        crate::dimension_layout::validate_dimension_layout_integrity(view)?;
        for (dimension_id, dimension) in &view.dimensions {
            match &dimension.value {
                DimensionValue::Measured { source, resolved } => {
                    let (feature, entity) = validate_measured_source(spatial, source)?;
                    let ResolvedDimensionValue::LengthMm { mm } = resolved else {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!("dimension {} is not a length", dimension.id),
                        });
                    };
                    if !mm.is_finite() || *mm <= 0.0 {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!("dimension {} has an invalid value", dimension.id),
                        });
                    }
                    let expected = expected_dimension_value(
                        dimension_id,
                        view,
                        spatial,
                        projection,
                        source_map,
                        source,
                    )?;
                    if (*mm - expected).abs() > 1.0e-6 {
                        return Err(DrawingError::InvalidDimensionWitness {
                            reason: format!(
                                "dimension {} resolved value {} disagrees with projected witness {}",
                                dimension_id, mm, expected
                            ),
                        });
                    }
                    if matches!(source, DimensionValueSource::ResolvedGeometry { .. })
                        && !spatial_source_matches(spatial, feature, entity)
                    {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!("dimension {} has invalid typed source", dimension.id),
                        });
                    }
                }
                DimensionValue::Pattern { shape } => {
                    validate_pattern_dimension(dimension, shape, spatial)?;
                }
                DimensionValue::HoleCallout { semantic, pattern } => {
                    validate_hole_callout_dimension(
                        view,
                        dimension,
                        semantic,
                        pattern.as_ref(),
                        spatial,
                        projection,
                        source_map,
                    )?;
                }
                DimensionValue::SlotCallout { semantic } => {
                    if !spatial.slot_cuts.values().any(|slot| {
                        slot.source_feature == semantic.feature
                            && (slot.overall_length
                                - match semantic.overall_length.resolved {
                                    ResolvedDimensionValue::LengthMm { mm } => mm,
                                    _ => -1.0,
                                })
                            .abs()
                                < 1.0e-9
                    }) {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "slot callout {} is not backed by a spatial slot",
                                dimension.id
                            ),
                        });
                    }
                }
                DimensionValue::ParametricHoleCallout { .. } => {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!(
                            "parametric callout {} requires the validated ParametricSpatialEvidence path",
                            dimension.id
                        ),
                    });
                }
            }
        }
    }
    for (id, element) in &front.center_geometry {
        let (feature, entity) = match element {
            DrawingCenterGeometry::CenterMark(mark) => {
                (&mark.source_feature, mark.source_entity.as_ref())
            }
            DrawingCenterGeometry::Centerline(line) => {
                (&line.source_feature, line.source_entity.as_ref())
            }
        };
        let Some(entity) = entity else {
            return Err(DrawingError::MissingCenterSource {
                reason: format!("center {} has no entity source", id),
            });
        };
        if !spatial_source_matches(spatial, feature, entity) {
            return Err(DrawingError::MissingCenterSource {
                reason: format!("center {} has invalid typed source", id),
            });
        }
    }
    Ok(())
}

fn validate_hole_callout_dimension(
    view: &DrawingView,
    dimension: &DrawingDimension,
    semantic: &HoleCalloutSemantic,
    pattern: Option<&PatternShape>,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
) -> Result<(), DrawingError> {
    let DrawingDimensionTarget::Feature { feature } = &dimension.target else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} body target is invalid for a legacy hole callout",
                dimension.id
            ),
        });
    };
    if semantic.provenance.feature != *feature || semantic.provenance.witnesses.is_empty() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} has detached hole provenance", dimension.id),
        });
    }
    let cuts = spatial.cuts_for_feature(feature);
    if cuts.is_empty() || cuts.len() != semantic.provenance.witnesses.len() {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} hole witness cardinality disagrees with SpatialModel",
                dimension.id
            ),
        });
    }
    let mut witness_entities = BTreeSet::new();
    for witness in &semantic.provenance.witnesses {
        if !witness_entities.insert(witness.entity.clone()) {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: format!(
                    "dimension {} contains a duplicate hole witness",
                    dimension.id
                ),
            });
        }
        let Some(cut) = spatial.cut_for_entity(&witness.entity) else {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: format!(
                    "dimension {} references missing cut witness {}",
                    dimension.id, witness.entity
                ),
            });
        };
        if cut.source_feature != *feature
            || (cut.radius - witness.radius).abs() > 1.0e-6
            || !same_z_interval(cut.z, witness.z)
            || !same_cut_kind(cut.kind, witness.kind)
        {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: format!(
                    "dimension {} contains a tampered SpatialModel witness",
                    dimension.id
                ),
            });
        }
    }
    if cuts
        .iter()
        .any(|(_, cut)| !witness_entities.contains(&cut.source_entity))
    {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} omits a SpatialModel hole witness",
                dimension.id
            ),
        });
    }
    let representative = spatial
        .cut_for_entity(&semantic.provenance.representative_entity)
        .ok_or_else(|| DrawingError::InvalidDimensionWitness {
            reason: format!("dimension {} has no representative cut", dimension.id),
        })?;
    let (presented_center, presented_radius) = presented_hole_witness(
        view,
        spatial,
        projection,
        source_map,
        &semantic.provenance.representative_entity,
    )
    .ok_or_else(|| DrawingError::MissingCenterSource {
        reason: format!(
            "dimension {} representative hole is not attached to a projected circle",
            dimension.id
        ),
    })?;
    if (presented_radius - representative.radius).abs() > 1.0e-6 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} representative projected radius disagrees with SpatialModel",
                dimension.id
            ),
        });
    }
    let DimensionValueSource::ResolvedGeometry {
        feature: diameter_feature,
        entity: diameter_entity,
    } = &semantic.diameter.source
    else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} diameter source is not resolved geometry",
                dimension.id
            ),
        });
    };
    if diameter_feature != feature || diameter_entity != &semantic.provenance.representative_entity
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} diameter source is detached", dimension.id),
        });
    }
    let ResolvedDimensionValue::LengthMm { mm: diameter } = semantic.diameter.resolved else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} diameter is not a length", dimension.id),
        });
    };
    if (diameter - 2.0 * representative.radius).abs() > 1.0e-6 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} diameter disagrees with SpatialModel",
                dimension.id
            ),
        });
    }

    match &semantic.extent {
        HoleCalloutExtent::Through => {
            for (_, cut) in &cuts {
                if !matches!(cut.kind, SpatialCutKind::ThroughAll)
                    || !same_z_interval(cut.z, spatial.body.z)
                {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!("dimension {} Through witness mismatch", dimension.id),
                    });
                }
            }
        }
        HoleCalloutExtent::Blind { depth, side } => {
            let DimensionValueSource::ResolvedHoleDepth {
                feature: depth_feature,
                entity: depth_entity,
            } = &depth.source
            else {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!(
                        "dimension {} depth source is not typed Spatial provenance",
                        dimension.id
                    ),
                });
            };
            if depth_feature != feature
                || depth_entity != &semantic.provenance.representative_entity
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!("dimension {} depth source is detached", dimension.id),
                });
            }
            let ResolvedDimensionValue::LengthMm { mm: depth_mm } = depth.resolved else {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!("dimension {} depth is not a length", dimension.id),
                });
            };
            if !depth.resolved.is_valid()
                || !same_float(depth_mm, representative.z.max - representative.z.min)
            {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!(
                        "dimension {} depth disagrees with SpatialModel",
                        dimension.id
                    ),
                });
            }
            for (_, cut) in &cuts {
                let SpatialCutKind::Blind { start_face } = cut.kind else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!("dimension {} blind witness is ThroughAll", dimension.id),
                    });
                };
                if start_face != *side
                    || !same_float(
                        cut.z.max - cut.z.min,
                        representative.z.max - representative.z.min,
                    )
                    || !blind_interval_matches(cut.z, spatial.body.z, *side)
                {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "dimension {} blind side/depth witness mismatch",
                            dimension.id
                        ),
                    });
                }
            }
        }
    }

    if cuts
        .iter()
        .any(|(_, cut)| (cut.radius - representative.radius).abs() > 1.0e-6)
    {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} pattern cuts have inconsistent resolved radii",
                dimension.id
            ),
        });
    }
    if let Some(shape) = pattern {
        validate_pattern_dimension(dimension, shape, spatial)?;
    }
    if semantic.show_pitch_circle_diameter
        && !matches!(
            spatial.body.footprint,
            scc_spatial::SpatialFootprint::Circle { .. }
        )
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} enables PCD presentation for a non-circular body",
                dimension.id
            ),
        });
    }
    if semantic.show_pitch_circle_diameter
        && !matches!(pattern, Some(PatternShape::Circular { .. }))
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} enables PCD presentation without a circular pattern",
                dimension.id
            ),
        });
    }
    match (pattern, &semantic.quantity) {
        (Some(PatternShape::Circular { count, .. }), Some(quantity)) => {
            if quantity.source
                != (DimensionValueSource::ResolvedPatternSemantic {
                    feature: feature.clone(),
                    field: PatternSemanticField::Count,
                })
                || quantity.resolved != count.resolved
                || !matches!(quantity.resolved, ResolvedDimensionValue::Count { .. })
                || !quantity.resolved.is_valid()
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!(
                        "dimension {} callout quantity disagrees with its pattern semantic",
                        dimension.id
                    ),
                });
            }
        }
        (Some(PatternShape::Grid { .. } | PatternShape::Corner { .. }), None) | (None, None) => {}
        _ => {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "dimension {} callout pattern/quantity correlation is invalid",
                    dimension.id
                ),
            })
        }
    }
    let layout = view.dimension_layouts.get(&dimension.id).ok_or_else(|| {
        DrawingError::MissingDimensionLayout {
            dimension: dimension.id.clone(),
        }
    })?;
    match (pattern, &layout.geometry) {
        (
            Some(_),
            DimensionLayoutGeometry::PatternCallout {
                anchor_on_pattern, ..
            },
        ) => {
            let center = view.transform.model_to_sheet(presented_center);
            let radius = presented_radius * view.transform.scale.0;
            let angle = DIAMETER_AXIS_ANGLE_DEG.to_radians();
            let expected = DrawingPoint::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            );
            if !same_point(*anchor_on_pattern, expected) {
                return Err(DrawingError::InvalidDimensionLayout {
                    reason: format!(
                        "dimension {} leader is not attached to its representative witness",
                        dimension.id
                    ),
                });
            }
        }
        (None, DimensionLayoutGeometry::Diameter { center, .. }) => {
            let expected = view.transform.model_to_sheet(presented_center);
            if !same_point(*center, expected) {
                return Err(DrawingError::InvalidDimensionLayout {
                    reason: format!(
                        "dimension {} diameter layout is detached from its witness",
                        dimension.id
                    ),
                });
            }
        }
        _ => {
            return Err(DrawingError::InvalidDimensionLayout {
                reason: format!(
                    "dimension {} is not attached with its hole layout",
                    dimension.id
                ),
            })
        }
    }
    Ok(())
}

fn presented_hole_witness(
    view: &DrawingView,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
    entity: &EntityId,
) -> Option<(DrawingPoint, f64)> {
    view.primitives.iter().find_map(|(drawing_id, primitive)| {
        let DrawingPrimitive::Circle {
            role: DrawingPrimitiveRole::Object | DrawingPrimitiveRole::Hidden,
            center,
            radius,
        } = primitive
        else {
            return None;
        };
        let projected = source_map.projected_for_primitive(drawing_id)?;
        let SpatialElementRef::Cut { id } = projection.sources.element_for_primitive(projected)?
        else {
            return None;
        };
        let cut = spatial.cuts.get(id)?;
        (cut.source_entity == *entity).then_some((*center, *radius))
    })
}

fn same_point(first: DrawingPoint, second: DrawingPoint) -> bool {
    same_float(first.x, second.x) && same_float(first.y, second.y)
}

fn same_cut_kind(first: SpatialCutKind, second: SpatialCutKind) -> bool {
    match (first, second) {
        (SpatialCutKind::ThroughAll, SpatialCutKind::ThroughAll) => true,
        (
            SpatialCutKind::Blind { start_face: first },
            SpatialCutKind::Blind { start_face: second },
        ) => first == second,
        _ => false,
    }
}

fn validate_measured_source<'a>(
    spatial: &'a SpatialModel,
    source: &'a DimensionValueSource,
) -> Result<(&'a FeatureId, &'a EntityId), DrawingError> {
    match source {
        DimensionValueSource::ResolvedGeometry { feature, entity } => Ok((feature, entity)),
        DimensionValueSource::ResolvedHoleLocation {
            feature,
            entity,
            datum_feature,
            datum_entity,
            ..
        } => {
            if spatial.body.source_feature != *datum_feature
                || spatial.body.source_entity != *datum_entity
                || !spatial
                    .cuts
                    .values()
                    .any(|cut| cut.source_feature == *feature && cut.source_entity == *entity)
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: "hole-location source does not reference a body edge and cut".into(),
                });
            }
            Ok((feature, entity))
        }
        DimensionValueSource::ResolvedPatternLocation {
            feature,
            representative_entity,
            datum_feature,
            datum_entity,
            ..
        } => {
            if spatial.body.source_feature != *datum_feature
                || spatial.body.source_entity != *datum_entity
                || !spatial.cuts.values().any(|cut| {
                    cut.source_feature == *feature && cut.source_entity == *representative_entity
                })
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: "pattern-location source does not reference a body edge and cut".into(),
                });
            }
            Ok((feature, representative_entity))
        }
        DimensionValueSource::ResolvedPatternSpacing {
            feature,
            first_entity,
            second_entity,
            ..
        } => {
            let has_first = spatial
                .cuts
                .values()
                .any(|cut| cut.source_feature == *feature && cut.source_entity == *first_entity);
            let has_second = spatial
                .cuts
                .values()
                .any(|cut| cut.source_feature == *feature && cut.source_entity == *second_entity);
            if !has_first || !has_second || first_entity == second_entity {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: "pattern-spacing source does not reference two distinct cuts".into(),
                });
            }
            Ok((feature, first_entity))
        }
        DimensionValueSource::ResolvedPatternSemantic { .. } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: "pattern semantic cannot be a standalone measured dimension".into(),
            })
        }
        DimensionValueSource::ResolvedHoleDepth { .. } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: "hole depth cannot be a standalone measured dimension".into(),
            })
        }
        DimensionValueSource::EngineeringFeatureParameter { .. }
        | DimensionValueSource::VerifiedBodyGeometry { .. }
        | DimensionValueSource::Parameter { .. }
        | DimensionValueSource::FeatureSlot { .. } => Err(DrawingError::InvalidDrawingDimension {
            reason: "production three-view dimension is not resolved".into(),
        }),
    }
}

fn validate_pattern_dimension(
    dimension: &DrawingDimension,
    shape: &PatternShape,
    spatial: &SpatialModel,
) -> Result<(), DrawingError> {
    let DrawingDimensionTarget::Feature { feature } = &dimension.target else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} body target is invalid for a legacy pattern",
                dimension.id
            ),
        });
    };
    let cuts: Vec<_> = spatial
        .cuts
        .values()
        .filter(|cut| cut.source_feature == *feature)
        .collect();
    if cuts.is_empty() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("pattern {} has no resolved cut witness", dimension.id),
        });
    }
    match shape {
        PatternShape::Circular {
            count,
            pitch_circle_diameter,
            hole_diameter,
        } => {
            expect_semantic(count, feature, PatternSemanticField::Count)?;
            expect_semantic(
                pitch_circle_diameter,
                feature,
                PatternSemanticField::PitchCircleDiameter,
            )?;
            expect_diameter(hole_diameter, feature, &cuts)?;
            expect_count_matches(count, cuts.len(), &dimension.id)?;
            hole_diameter
        }
        PatternShape::Grid {
            rows,
            columns,
            spacing_x,
            spacing_y,
            hole_diameter,
        } => {
            expect_semantic(rows, feature, PatternSemanticField::Rows)?;
            expect_semantic(columns, feature, PatternSemanticField::Columns)?;
            expect_semantic(spacing_x, feature, PatternSemanticField::SpacingX)?;
            expect_semantic(spacing_y, feature, PatternSemanticField::SpacingY)?;
            expect_diameter(hole_diameter, feature, &cuts)?;
            let expected = count_value(rows)? * count_value(columns)?;
            if expected != cuts.len() as u32 {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!(
                        "pattern {} semantic count {} disagrees with {} resolved cuts",
                        dimension.id,
                        expected,
                        cuts.len()
                    ),
                });
            }
            hole_diameter
        }
        PatternShape::Corner {
            offset_x,
            offset_y,
            hole_diameter,
        } => {
            expect_semantic(offset_x, feature, PatternSemanticField::OffsetX)?;
            expect_semantic(offset_y, feature, PatternSemanticField::OffsetY)?;
            expect_diameter(hole_diameter, feature, &cuts)?;
            if cuts.len() != 4 {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!(
                        "corner pattern {} requires four resolved cuts, got {}",
                        dimension.id,
                        cuts.len()
                    ),
                });
            }
            hole_diameter
        }
    };
    Ok(())
}

fn expect_semantic(
    component: &DimensionComponent,
    feature: &FeatureId,
    field: PatternSemanticField,
) -> Result<(), DrawingError> {
    if component.source
        != (DimensionValueSource::ResolvedPatternSemantic {
            feature: feature.clone(),
            field,
        })
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "pattern semantic source for {} is not typed as {:?}",
                feature, field
            ),
        });
    }
    if !component.resolved.is_valid() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("pattern semantic {} is invalid", feature),
        });
    }
    Ok(())
}

fn expect_diameter(
    component: &DimensionComponent,
    feature: &FeatureId,
    cuts: &[&scc_spatial::CylindricalCut],
) -> Result<(), DrawingError> {
    let DimensionValueSource::ResolvedGeometry {
        feature: source_feature,
        entity,
    } = &component.source
    else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("pattern {} diameter is not geometry-resolved", feature),
        });
    };
    if source_feature != feature {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("pattern {} diameter has the wrong feature source", feature),
        });
    }
    let Some(cut) = cuts.iter().find(|cut| cut.source_entity == *entity) else {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!("pattern {} diameter has no cut witness", feature),
        });
    };
    let ResolvedDimensionValue::LengthMm { mm } = component.resolved else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("pattern {} diameter is not a length", feature),
        });
    };
    if !mm.is_finite() || mm <= 0.0 || (mm - 2.0 * cut.radius).abs() > 1.0e-6 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!("pattern {} diameter disagrees with resolved cut", feature),
        });
    }
    Ok(())
}

fn count_value(component: &DimensionComponent) -> Result<u32, DrawingError> {
    match component.resolved {
        ResolvedDimensionValue::Count { count } if count >= 1 => Ok(count),
        _ => Err(DrawingError::InvalidDrawingDimension {
            reason: "pattern count component is not a valid count".into(),
        }),
    }
}

fn expect_count_matches(
    component: &DimensionComponent,
    actual: usize,
    dimension_id: &DrawingDimensionId,
) -> Result<(), DrawingError> {
    if count_value(component)? != actual as u32 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "pattern {} semantic count disagrees with {} resolved cuts",
                dimension_id, actual
            ),
        });
    }
    Ok(())
}

fn expected_dimension_value(
    dimension_id: &DrawingDimensionId,
    view: &DrawingView,
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    source_map: &DrawingSourceMap,
    source: &DimensionValueSource,
) -> Result<f64, DrawingError> {
    let body = SpatialElementRef::Body;
    if dimension_id == &DrawingDimensionId("three_view.overall_width".into()) {
        let bounds = projected_bounds_for_source(
            projection
                .views
                .get(&PrincipalViewKind::Front)
                .expect("front checked"),
            &body,
            projection,
        )?;
        return Ok(bounds.max_x - bounds.min_x);
    }
    if dimension_id == &DrawingDimensionId("three_view.overall_height".into()) {
        let bounds = projected_bounds_for_source(
            projection
                .views
                .get(&PrincipalViewKind::Front)
                .expect("front checked"),
            &body,
            projection,
        )?;
        return Ok(bounds.max_y - bounds.min_y);
    }
    if dimension_id == &DrawingDimensionId("three_view.corner_radius".into()) {
        let DimensionValueSource::ResolvedGeometry { feature, entity } = source else {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "corner radius must be resolved from body geometry".into(),
            });
        };
        if *feature != spatial.body.source_feature || *entity != spatial.body.source_entity {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "corner radius source is not the resolved body".into(),
            });
        }
        let (_, radius, _) = projected_arc_for_source(
            projection
                .views
                .get(&PrincipalViewKind::Front)
                .expect("front checked"),
            &body,
            projection,
        )?;
        return Ok(radius);
    }
    if dimension_id == &DrawingDimensionId("three_view.overall_diameter".into()) {
        let DimensionValueSource::ResolvedGeometry { feature, entity } = source else {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "overall diameter must be resolved from body geometry".into(),
            });
        };
        if *feature != spatial.body.source_feature || *entity != spatial.body.source_entity {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "overall diameter source is not the circular body".into(),
            });
        }
        if !matches!(
            &spatial.body.footprint,
            scc_spatial::SpatialFootprint::Circle { .. }
        ) {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: "overall diameter is only valid for a circular body".into(),
            });
        }
        let (_, radius) = projected_circle_for_source(
            projection
                .views
                .get(&PrincipalViewKind::Front)
                .expect("front checked"),
            &body,
            projection,
        )?;
        return Ok(2.0 * radius);
    }
    if dimension_id == &DrawingDimensionId("three_view.thickness".into()) {
        let bounds = projected_bounds_for_source(
            projection
                .views
                .get(&PrincipalViewKind::Left)
                .expect("left checked"),
            &body,
            projection,
        )?;
        return Ok(bounds.max_x - bounds.min_x);
    }
    match source {
        DimensionValueSource::ResolvedHoleLocation {
            entity,
            datum_feature,
            datum_entity,
            datum,
            ..
        } => {
            if spatial.body.source_feature != *datum_feature
                || spatial.body.source_entity != *datum_entity
            {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: "hole location datum is not the resolved body edge".into(),
                });
            }
            let cut = spatial
                .cuts
                .values()
                .find(|cut| cut.source_entity == *entity)
                .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                    reason: format!("no resolved cut exists for hole location {}", entity),
                })?;
            let (min_x, min_y) = match &spatial.body.footprint {
                scc_spatial::SpatialFootprint::Rectangle { origin, .. } => (origin.x, origin.y),
                scc_spatial::SpatialFootprint::Composite { .. }
                | scc_spatial::SpatialFootprint::Parametric { .. } => {
                    (spatial.bounds.min_x, spatial.bounds.min_y)
                }
                scc_spatial::SpatialFootprint::Circle { .. } => {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: "linear edge datum is not supported for a circular body".into(),
                    })
                }
            };
            return Ok(match datum {
                HoleDatum::LeftEdge => cut.center.x - min_x,
                HoleDatum::BottomEdge => cut.center.y - min_y,
            });
        }
        DimensionValueSource::ResolvedPatternLocation {
            feature,
            representative_entity,
            datum_feature,
            datum_entity,
            datum,
        } => {
            if spatial.body.source_feature != *datum_feature
                || spatial.body.source_entity != *datum_entity
            {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: "pattern location datum is not the resolved body edge".into(),
                });
            }
            if !spatial.cuts.values().any(|cut| {
                cut.source_feature == *feature && cut.source_entity == *representative_entity
            }) {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!("no representative pattern cut exists for {}", feature),
                });
            }
            let (sum_x, sum_y, count) = spatial
                .cuts
                .values()
                .filter(|cut| cut.source_feature == *feature)
                .fold((0.0, 0.0, 0usize), |(x, y, n), cut| {
                    (x + cut.center.x, y + cut.center.y, n + 1)
                });
            if count == 0 {
                return Err(DrawingError::InvalidDimensionWitness {
                    reason: format!("pattern {} has no center witness", feature),
                });
            }
            let center_x = sum_x / count as f64;
            let center_y = sum_y / count as f64;
            let (min_x, min_y) = match &spatial.body.footprint {
                scc_spatial::SpatialFootprint::Rectangle { origin, .. } => (origin.x, origin.y),
                scc_spatial::SpatialFootprint::Composite { .. }
                | scc_spatial::SpatialFootprint::Parametric { .. } => {
                    (spatial.bounds.min_x, spatial.bounds.min_y)
                }
                scc_spatial::SpatialFootprint::Circle { .. } => {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: "linear edge datum is not supported for a circular body".into(),
                    })
                }
            };
            return Ok(match datum {
                HoleDatum::LeftEdge => center_x - min_x,
                HoleDatum::BottomEdge => center_y - min_y,
            });
        }
        DimensionValueSource::ResolvedPatternSpacing {
            feature,
            first_entity,
            second_entity,
            axis,
        } => {
            let first = spatial
                .cuts
                .values()
                .find(|cut| cut.source_feature == *feature && cut.source_entity == *first_entity)
                .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                    reason: format!("no first pattern spacing witness for {}", feature),
                })?;
            let second = spatial
                .cuts
                .values()
                .find(|cut| cut.source_feature == *feature && cut.source_entity == *second_entity)
                .ok_or_else(|| DrawingError::InvalidDimensionWitness {
                    reason: format!("no second pattern spacing witness for {}", feature),
                })?;
            return Ok(match axis {
                LinearAxis::Horizontal => (second.center.x - first.center.x).abs(),
                LinearAxis::Vertical => (second.center.y - first.center.y).abs(),
            });
        }
        DimensionValueSource::ResolvedGeometry { entity, .. } => {
            for (drawing_id, primitive) in &view.primitives {
                let DrawingPrimitive::Circle {
                    role: DrawingPrimitiveRole::Object,
                    radius,
                    ..
                } = primitive
                else {
                    continue;
                };
                let Some(projected_id) = source_map.projected_for_primitive(drawing_id) else {
                    continue;
                };
                let Some(SpatialElementRef::Cut { id }) =
                    projection.sources.element_for_primitive(projected_id)
                else {
                    continue;
                };
                let Some(cut) = spatial.cuts.get(id) else {
                    continue;
                };
                if cut.source_entity == *entity {
                    return Ok(2.0 * *radius);
                }
            }
        }
        DimensionValueSource::ResolvedPatternSemantic { .. }
        | DimensionValueSource::ResolvedHoleDepth { .. }
        | DimensionValueSource::EngineeringFeatureParameter { .. }
        | DimensionValueSource::VerifiedBodyGeometry { .. }
        | DimensionValueSource::Parameter { .. }
        | DimensionValueSource::FeatureSlot { .. } => {}
    }
    Err(DrawingError::InvalidDimensionWitness {
        reason: "no drawing witness exists for the resolved dimension".into(),
    })
}

fn spatial_source_matches(spatial: &SpatialModel, feature: &FeatureId, entity: &EntityId) -> bool {
    (spatial.body.source_feature == *feature && spatial.body.source_entity == *entity)
        || spatial
            .cuts
            .values()
            .any(|cut| cut.source_feature == *feature && cut.source_entity == *entity)
}
