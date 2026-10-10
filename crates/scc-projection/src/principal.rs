//! Phase 9.4 raw principal-view projection lowering.
//!
//! This module consumes only the resolved SpatialModel. It produces raw
//! candidates in deterministic Front, Top, Left order and deliberately leaves
//! visibility, hidden-line classification, styling, and drawing integration to
//! later phases.

use scc_spatial::{
    Bounds3D, CylindricalCut, SlotCut, SpatialArcDirection, SpatialCutId, SpatialFootprint,
    SpatialModel, SpatialPrincipalView, SpatialProfileSegment, SpatialProjectedGeometry,
};

use crate::error::{InvalidProjectionGeometryReason, InvalidSpatialModelReason};
use crate::frame::{OrthographicFrame, PartLocalPoint3, PrincipalViewKind, ProjectedPoint};
use crate::model::{
    ProjectedGeometry, ProjectionDepthRange, ProjectionModel, ProjectionModelBuilder,
    ProjectionPrimitiveRole, SpatialElementRef,
};
use crate::ProjectionError;

/// Lower a resolved spatial model into the three canonical raw principal
/// views. The input is read-only and is never re-solved or re-lowered.
pub fn project_principal_views(spatial: &SpatialModel) -> Result<ProjectionModel, ProjectionError> {
    if let SpatialFootprint::Parametric { evidence } = &spatial.body.footprint {
        return project_parametric_views(spatial, evidence);
    }
    let body = preflight(spatial)?;
    let mut builder = ProjectionModelBuilder::new();
    let views = [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ];

    for view in views {
        builder.add_view(view)?;
    }

    for view in views {
        add_body(&mut builder, view, &body, spatial)?;
        for (cut_id, cut) in &spatial.cuts {
            add_cut(&mut builder, view, cut_id, cut, spatial)?;
        }
        for (cut_id, slot) in &spatial.slot_cuts {
            add_slot(&mut builder, view, cut_id, slot, spatial)?;
        }
    }

    let model = builder.finish()?;
    model.validate_against(spatial)?;
    Ok(model)
}

fn project_parametric_views(
    spatial: &SpatialModel,
    evidence: &scc_spatial::ParametricSpatialEvidence,
) -> Result<ProjectionModel, ProjectionError> {
    let mut builder = ProjectionModelBuilder::new();
    let views = [
        (SpatialPrincipalView::Front, PrincipalViewKind::Front),
        (SpatialPrincipalView::Top, PrincipalViewKind::Top),
        (SpatialPrincipalView::Left, PrincipalViewKind::Left),
    ];
    for (_, view) in views {
        builder.add_view(view)?;
    }
    for (spatial_view, view) in views {
        let projected_edges = evidence
            .views
            .get(&spatial_view)
            .ok_or_else(|| invalid_spatial(InvalidSpatialModelReason::SpatialBoundsMismatch))?;
        for edge in projected_edges {
            let geometry = match &edge.geometry {
                SpatialProjectedGeometry::Segment { start, end } => ProjectedGeometry::Segment {
                    start: crate::frame::ProjectedPoint2 {
                        u: start.x,
                        v: start.y,
                    },
                    end: crate::frame::ProjectedPoint2 { u: end.x, v: end.y },
                },
                SpatialProjectedGeometry::Circle { center, radius } => ProjectedGeometry::Circle {
                    center: crate::frame::ProjectedPoint2 {
                        u: center.x,
                        v: center.y,
                    },
                    radius: *radius,
                },
                SpatialProjectedGeometry::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                } => ProjectedGeometry::Arc {
                    center: crate::frame::ProjectedPoint2 {
                        u: center.x,
                        v: center.y,
                    },
                    radius: *radius,
                    start_angle: *start_angle,
                    end_angle: *end_angle,
                },
            };
            let depth = ProjectionDepthRange::new(edge.depth_min, edge.depth_max)?;
            builder.add_primitive(
                view,
                crate::model::ProjectionPrimitiveId(edge.id.clone()),
                geometry,
                ProjectionPrimitiveRole::BodyBoundary,
                depth,
                SpatialElementRef::Body,
                spatial,
            )?;
        }
    }
    let model = builder.finish()?;
    model.validate_against(spatial)?;
    Ok(model)
}

#[derive(Debug, Clone)]
enum ValidatedBody {
    Rectangle {
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        z0: f64,
        z1: f64,
    },
    Circle {
        center_x: f64,
        center_y: f64,
        radius: f64,
        z0: f64,
        z1: f64,
    },
    Composite {
        segments: Vec<SpatialProfileSegment>,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        z0: f64,
        z1: f64,
    },
}

fn preflight(spatial: &SpatialModel) -> Result<ValidatedBody, ProjectionError> {
    let (x0, x1, y0, y1, circle, composite) = match &spatial.body.footprint {
        SpatialFootprint::Rectangle {
            origin,
            width,
            height,
        } => {
            if !origin.x.is_finite()
                || !origin.y.is_finite()
                || !width.is_finite()
                || !height.is_finite()
            {
                return Err(invalid_spatial(
                    InvalidSpatialModelReason::NonFiniteBodyValue,
                ));
            }
            if *width <= 0.0 || *height <= 0.0 {
                return Err(invalid_spatial(
                    InvalidSpatialModelReason::NonPositiveBodyDimension,
                ));
            }
            let x1 = origin.x + width;
            let y1 = origin.y + height;
            if !x1.is_finite() || !y1.is_finite() || x1 <= origin.x || y1 <= origin.y {
                return Err(invalid_spatial(
                    InvalidSpatialModelReason::BodyCoordinateOverflow,
                ));
            }
            (origin.x, x1, origin.y, y1, None, None)
        }
        SpatialFootprint::Circle { center, radius } => {
            if !center.x.is_finite() || !center.y.is_finite() || !radius.is_finite() {
                return Err(invalid_spatial(
                    InvalidSpatialModelReason::NonFiniteBodyValue,
                ));
            }
            if *radius <= 0.0 {
                return Err(invalid_spatial(
                    InvalidSpatialModelReason::NonPositiveBodyDimension,
                ));
            }
            let x0 = center.x - radius;
            let x1 = center.x + radius;
            let y0 = center.y - radius;
            let y1 = center.y + radius;
            if !x0.is_finite()
                || !x1.is_finite()
                || !y0.is_finite()
                || !y1.is_finite()
                || x1 <= x0
                || y1 <= y0
            {
                return Err(invalid_spatial(
                    InvalidSpatialModelReason::BodyCoordinateOverflow,
                ));
            }
            (x0, x1, y0, y1, Some((center.x, center.y, radius)), None)
        }
        SpatialFootprint::Composite { segments } => {
            let (x0, x1, y0, y1) = composite_bounds(segments)?;
            (x0, x1, y0, y1, None, Some(segments.clone()))
        }
        SpatialFootprint::Parametric { .. } => {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::SpatialBoundsMismatch,
            ));
        }
    };

    let z0 = spatial.body.z.min;
    let z1 = spatial.body.z.max;
    if !z0.is_finite() || !z1.is_finite() {
        return Err(invalid_spatial(
            InvalidSpatialModelReason::NonFiniteBodyValue,
        ));
    }
    if z0 != 0.0 || z1 <= z0 {
        return Err(invalid_spatial(
            InvalidSpatialModelReason::InvalidBodyZInterval,
        ));
    }

    let bounds = spatial.bounds;
    if !bounds_finite_and_strict(bounds) {
        return Err(invalid_spatial(
            InvalidSpatialModelReason::InvalidSpatialBounds,
        ));
    }
    let expected = Bounds3D {
        min_x: x0,
        max_x: x1,
        min_y: y0,
        max_y: y1,
        min_z: z0,
        max_z: z1,
    };
    if bounds != expected {
        return Err(invalid_spatial(
            InvalidSpatialModelReason::SpatialBoundsMismatch,
        ));
    }

    for cut in spatial.cuts.values() {
        if !cut.center.x.is_finite()
            || !cut.center.y.is_finite()
            || !cut.radius.is_finite()
            || !cut.z.min.is_finite()
            || !cut.z.max.is_finite()
        {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonFiniteCutValue,
            ));
        }
        if cut.radius <= 0.0 {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonPositiveCutRadius,
            ));
        }
        if cut.z.min < 0.0 || cut.z.max <= cut.z.min {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::InvalidCutZInterval,
            ));
        }
        if cut.z.min < z0 || cut.z.max > z1 {
            return Err(invalid_spatial(InvalidSpatialModelReason::CutOutsideBodyZ));
        }
    }

    match (circle, composite) {
        (Some((center_x, center_y, radius)), None) => Ok(ValidatedBody::Circle {
            center_x,
            center_y,
            radius: *radius,
            z0,
            z1,
        }),
        (None, Some(segments)) => Ok(ValidatedBody::Composite {
            segments,
            x0,
            x1,
            y0,
            y1,
            z0,
            z1,
        }),
        (None, None) => Ok(ValidatedBody::Rectangle {
            x0,
            x1,
            y0,
            y1,
            z0,
            z1,
        }),
        (Some(_), Some(_)) => Err(invalid_spatial(
            InvalidSpatialModelReason::SpatialBoundsMismatch,
        )),
    }
}

fn bounds_finite_and_strict(bounds: Bounds3D) -> bool {
    bounds.min_x.is_finite()
        && bounds.max_x.is_finite()
        && bounds.min_y.is_finite()
        && bounds.max_y.is_finite()
        && bounds.min_z.is_finite()
        && bounds.max_z.is_finite()
        && bounds.max_x > bounds.min_x
        && bounds.max_y > bounds.min_y
        && bounds.max_z > bounds.min_z
}

fn invalid_spatial(reason: InvalidSpatialModelReason) -> ProjectionError {
    ProjectionError::InvalidSpatialModel { reason }
}

fn composite_bounds(
    segments: &[SpatialProfileSegment],
) -> Result<(f64, f64, f64, f64), ProjectionError> {
    const PROFILE_TOLERANCE: f64 = 1.0e-9;
    let Some(first) = segments.first() else {
        return Err(invalid_spatial(
            InvalidSpatialModelReason::NonRectangularBody,
        ));
    };
    let first_start = first.start();
    let mut previous_end = first_start;
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (index, segment) in segments.iter().enumerate() {
        if !segment.is_valid() {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonRectangularBody,
            ));
        }
        let start = segment.start();
        let end = segment.end();
        if index > 0
            && ((previous_end.x - start.x).abs() > PROFILE_TOLERANCE
                || (previous_end.y - start.y).abs() > PROFILE_TOLERANCE)
        {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonRectangularBody,
            ));
        }
        let Some((segment_min_x, segment_max_x, segment_min_y, segment_max_y)) = segment.bounds()
        else {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonRectangularBody,
            ));
        };
        min_x = min_x.min(segment_min_x);
        max_x = max_x.max(segment_max_x);
        min_y = min_y.min(segment_min_y);
        max_y = max_y.max(segment_max_y);
        previous_end = end;
    }

    if (previous_end.x - first_start.x).abs() > PROFILE_TOLERANCE
        || (previous_end.y - first_start.y).abs() > PROFILE_TOLERANCE
        || !min_x.is_finite()
        || !max_x.is_finite()
        || !min_y.is_finite()
        || !max_y.is_finite()
        || max_x <= min_x
        || max_y <= min_y
    {
        return Err(invalid_spatial(
            InvalidSpatialModelReason::NonRectangularBody,
        ));
    }
    Ok((min_x, max_x, min_y, max_y))
}

fn add_body(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    body: &ValidatedBody,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let source = SpatialElementRef::Body;
    match body {
        ValidatedBody::Rectangle {
            x0,
            x1,
            y0,
            y1,
            z0,
            z1,
        } => add_oriented_box(
            builder,
            view,
            *x0,
            *x1,
            *y0,
            *y1,
            *z0,
            *z1,
            ProjectionPrimitiveRole::BodyBoundary,
            &source,
            spatial,
        ),
        ValidatedBody::Circle {
            center_x,
            center_y,
            radius,
            z0,
            z1,
        } => match view {
            PrincipalViewKind::Front => {
                let frame = frame_for(view);
                add_circle(
                    builder,
                    view,
                    frame,
                    point(*center_x, *center_y, *z0),
                    *radius,
                    depth_at(frame, point(*center_x, *center_y, *z0))?,
                    ProjectionPrimitiveRole::BodyBoundary,
                    &source,
                    spatial,
                )?;
                add_circle(
                    builder,
                    view,
                    frame,
                    point(*center_x, *center_y, *z1),
                    *radius,
                    depth_at(frame, point(*center_x, *center_y, *z1))?,
                    ProjectionPrimitiveRole::BodyBoundary,
                    &source,
                    spatial,
                )
            }
            PrincipalViewKind::Top | PrincipalViewKind::Left => add_oriented_box(
                builder,
                view,
                *center_x - *radius,
                *center_x + *radius,
                *center_y - *radius,
                *center_y + *radius,
                *z0,
                *z1,
                ProjectionPrimitiveRole::BodyBoundary,
                &source,
                spatial,
            ),
        },
        ValidatedBody::Composite {
            segments,
            x0,
            x1,
            y0,
            y1,
            z0,
            z1,
        } => match view {
            PrincipalViewKind::Front => {
                add_composite_front(builder, view, segments, *z0, *z1, &source, spatial)
            }
            PrincipalViewKind::Top | PrincipalViewKind::Left => add_oriented_box(
                builder,
                view,
                *x0,
                *x1,
                *y0,
                *y1,
                *z0,
                *z1,
                ProjectionPrimitiveRole::BodyBoundary,
                &source,
                spatial,
            ),
        },
    }
}

fn add_composite_front(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    segments: &[SpatialProfileSegment],
    z0: f64,
    z1: f64,
    _source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let frame = frame_for(view);
    for z in [z1, z0] {
        for segment in segments {
            let source = SpatialElementRef::BodySegment {
                index: segment.segment_index(),
            };
            match segment {
                SpatialProfileSegment::Line { start, end, .. } => add_segment(
                    builder,
                    view,
                    frame,
                    point(start.x, start.y, z),
                    point(end.x, end.y, z),
                    ProjectionPrimitiveRole::BodyBoundary,
                    &source,
                    spatial,
                )?,
                SpatialProfileSegment::Arc {
                    start,
                    end,
                    center,
                    radius,
                    direction,
                    ..
                } => add_profile_arc(
                    builder,
                    view,
                    frame,
                    point(center.x, center.y, z),
                    point(start.x, start.y, z),
                    point(end.x, end.y, z),
                    *radius,
                    *direction,
                    ProjectionPrimitiveRole::BodyBoundary,
                    &source,
                    spatial,
                )?,
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_oriented_box(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    z0: f64,
    z1: f64,
    role: ProjectionPrimitiveRole,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let frame = frame_for(view);
    let near = match view {
        PrincipalViewKind::Front => [
            point(x0, y0, z1),
            point(x1, y0, z1),
            point(x1, y1, z1),
            point(x0, y1, z1),
        ],
        PrincipalViewKind::Top => [
            point(x0, y1, z1),
            point(x1, y1, z1),
            point(x1, y1, z0),
            point(x0, y1, z0),
        ],
        PrincipalViewKind::Left => [
            point(x0, y0, z0),
            point(x0, y0, z1),
            point(x0, y1, z1),
            point(x0, y1, z0),
        ],
    };
    let far = match view {
        PrincipalViewKind::Front => [
            point(x0, y0, z0),
            point(x1, y0, z0),
            point(x1, y1, z0),
            point(x0, y1, z0),
        ],
        PrincipalViewKind::Top => [
            point(x0, y0, z1),
            point(x1, y0, z1),
            point(x1, y0, z0),
            point(x0, y0, z0),
        ],
        PrincipalViewKind::Left => [
            point(x1, y0, z0),
            point(x1, y0, z1),
            point(x1, y1, z1),
            point(x1, y1, z0),
        ],
    };
    add_loop(builder, view, frame, &near, role, source, spatial)?;
    add_loop(builder, view, frame, &far, role, source, spatial)?;
    Ok(())
}

fn add_loop(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    frame: OrthographicFrame,
    points: &[PartLocalPoint3; 4],
    role: ProjectionPrimitiveRole,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    for index in 0..4 {
        let next = (index + 1) % 4;
        add_segment(
            builder,
            view,
            frame,
            points[index],
            points[next],
            role,
            source,
            spatial,
        )?;
    }
    Ok(())
}

fn add_slot(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    cut_id: &SpatialCutId,
    slot: &SlotCut,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let (x0, x1, y0, y1) = slot_xy_bounds(&slot.segments)?;
    match view {
        PrincipalViewKind::Front => add_slot_front(
            builder,
            view,
            cut_id,
            &slot.segments,
            slot.z.min,
            slot.z.max,
            spatial,
        ),
        PrincipalViewKind::Top | PrincipalViewKind::Left => add_slot_side_silhouette(
            builder,
            view,
            x0,
            x1,
            y0,
            y1,
            slot.z.min,
            slot.z.max,
            &SpatialElementRef::SlotCut { id: cut_id.clone() },
            spatial,
        ),
    }
}

fn slot_xy_bounds(
    segments: &[SpatialProfileSegment],
) -> Result<(f64, f64, f64, f64), ProjectionError> {
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    for segment in segments {
        let Some((min_x, max_x, min_y, max_y)) = segment.bounds() else {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonRectangularBody,
            ));
        };
        bounds = Some(match bounds {
            None => (min_x, max_x, min_y, max_y),
            Some((x0, x1, y0, y1)) => (x0.min(min_x), x1.max(max_x), y0.min(min_y), y1.max(max_y)),
        });
    }
    bounds.ok_or_else(|| invalid_spatial(InvalidSpatialModelReason::NonRectangularBody))
}

#[allow(clippy::too_many_arguments)]
fn add_slot_side_silhouette(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    z0: f64,
    z1: f64,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let frame = frame_for(view);
    let points = match view {
        PrincipalViewKind::Front => {
            return Err(invalid_spatial(
                InvalidSpatialModelReason::NonRectangularBody,
            ))
        }
        PrincipalViewKind::Top => [
            point(x0, y1, z1),
            point(x1, y1, z1),
            point(x1, y1, z0),
            point(x0, y1, z0),
        ],
        PrincipalViewKind::Left => [
            point(x0, y0, z0),
            point(x0, y0, z1),
            point(x0, y1, z1),
            point(x0, y1, z0),
        ],
    };
    add_loop(
        builder,
        view,
        frame,
        &points,
        ProjectionPrimitiveRole::CutBoundary,
        source,
        spatial,
    )
}

fn add_slot_front(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    cut_id: &SpatialCutId,
    segments: &[SpatialProfileSegment],
    z0: f64,
    z1: f64,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let frame = frame_for(view);
    for z in [z1, z0] {
        for segment in segments {
            let source = SpatialElementRef::SlotSegment {
                id: cut_id.clone(),
                index: segment.segment_index(),
            };
            match segment {
                SpatialProfileSegment::Line { start, end, .. } => add_segment(
                    builder,
                    view,
                    frame,
                    point(start.x, start.y, z),
                    point(end.x, end.y, z),
                    ProjectionPrimitiveRole::CutBoundary,
                    &source,
                    spatial,
                )?,
                SpatialProfileSegment::Arc {
                    start,
                    end,
                    center,
                    radius,
                    direction,
                    ..
                } => add_profile_arc(
                    builder,
                    view,
                    frame,
                    point(center.x, center.y, z),
                    point(start.x, start.y, z),
                    point(end.x, end.y, z),
                    *radius,
                    *direction,
                    ProjectionPrimitiveRole::CutBoundary,
                    &source,
                    spatial,
                )?,
            }
        }
    }
    Ok(())
}

fn add_cut(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    cut_id: &SpatialCutId,
    cut: &CylindricalCut,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let frame = frame_for(view);
    let source = SpatialElementRef::Cut { id: cut_id.clone() };
    match view {
        PrincipalViewKind::Front => {
            add_circle(
                builder,
                view,
                frame,
                point(cut.center.x, cut.center.y, cut.z.min),
                cut.radius,
                depth_at(frame, point(cut.center.x, cut.center.y, cut.z.min))?,
                ProjectionPrimitiveRole::CutBoundary,
                &source,
                spatial,
            )?;
            add_circle(
                builder,
                view,
                frame,
                point(cut.center.x, cut.center.y, cut.z.max),
                cut.radius,
                depth_at(frame, point(cut.center.x, cut.center.y, cut.z.max))?,
                ProjectionPrimitiveRole::CutBoundary,
                &source,
                spatial,
            )?;
        }
        PrincipalViewKind::Top => {
            let x_minus = finite_sub(cut.center.x, cut.radius, Arithmetic::Geometry)?;
            let x_plus = finite_add(cut.center.x, cut.radius, Arithmetic::Geometry)?;
            let y_minus = finite_sub(cut.center.y, cut.radius, Arithmetic::Depth)?;
            let y_plus = finite_add(cut.center.y, cut.radius, Arithmetic::Depth)?;
            let za = cut.z.min;
            let zb = cut.z.max;

            add_segment_with_depth(
                builder,
                view,
                frame,
                point(x_minus, cut.center.y, za),
                point(x_plus, cut.center.y, za),
                ProjectionDepthRange::new(y_minus, y_plus)?,
                &source,
                spatial,
            )?;
            add_segment_with_depth(
                builder,
                view,
                frame,
                point(x_minus, cut.center.y, zb),
                point(x_plus, cut.center.y, zb),
                ProjectionDepthRange::new(y_minus, y_plus)?,
                &source,
                spatial,
            )?;
            add_segment_with_depth(
                builder,
                view,
                frame,
                point(x_minus, cut.center.y, za),
                point(x_minus, cut.center.y, zb),
                depth_from_axis_points(
                    frame,
                    point(x_minus, cut.center.y, za),
                    point(x_minus, cut.center.y, zb),
                )?,
                &source,
                spatial,
            )?;
            add_segment_with_depth(
                builder,
                view,
                frame,
                point(x_plus, cut.center.y, za),
                point(x_plus, cut.center.y, zb),
                depth_from_axis_points(
                    frame,
                    point(x_plus, cut.center.y, za),
                    point(x_plus, cut.center.y, zb),
                )?,
                &source,
                spatial,
            )?;
        }
        PrincipalViewKind::Left => {
            let y_minus = finite_sub(cut.center.y, cut.radius, Arithmetic::Geometry)?;
            let y_plus = finite_add(cut.center.y, cut.radius, Arithmetic::Geometry)?;
            let x_plus = finite_add(cut.center.x, cut.radius, Arithmetic::Depth)?;
            let x_minus = finite_sub(cut.center.x, cut.radius, Arithmetic::Depth)?;
            let depth_min = finite_neg(x_plus, Arithmetic::Depth)?;
            let depth_max = finite_neg(x_minus, Arithmetic::Depth)?;
            let za = cut.z.min;
            let zb = cut.z.max;

            add_segment_with_depth(
                builder,
                view,
                frame,
                point(cut.center.x, y_minus, za),
                point(cut.center.x, y_plus, za),
                ProjectionDepthRange::new(depth_min, depth_max)?,
                &source,
                spatial,
            )?;
            add_segment_with_depth(
                builder,
                view,
                frame,
                point(cut.center.x, y_minus, zb),
                point(cut.center.x, y_plus, zb),
                ProjectionDepthRange::new(depth_min, depth_max)?,
                &source,
                spatial,
            )?;
            add_segment_with_depth(
                builder,
                view,
                frame,
                point(cut.center.x, y_minus, za),
                point(cut.center.x, y_minus, zb),
                depth_from_axis_points(
                    frame,
                    point(cut.center.x, y_minus, za),
                    point(cut.center.x, y_minus, zb),
                )?,
                &source,
                spatial,
            )?;
            add_segment_with_depth(
                builder,
                view,
                frame,
                point(cut.center.x, y_plus, za),
                point(cut.center.x, y_plus, zb),
                depth_from_axis_points(
                    frame,
                    point(cut.center.x, y_plus, za),
                    point(cut.center.x, y_plus, zb),
                )?,
                &source,
                spatial,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_segment(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    frame: OrthographicFrame,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
    role: ProjectionPrimitiveRole,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let start_projected = project(frame, start)?;
    let end_projected = project(frame, end)?;
    let depth = depth_from_projected(start_projected, end_projected)?;
    add_projected_segment(
        builder,
        view,
        start_projected,
        end_projected,
        role,
        depth,
        source,
        spatial,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_segment_with_depth(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    frame: OrthographicFrame,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
    depth: ProjectionDepthRange,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let start_projected = project(frame, start)?;
    let end_projected = project(frame, end)?;
    add_projected_segment(
        builder,
        view,
        start_projected,
        end_projected,
        ProjectionPrimitiveRole::CutBoundary,
        depth,
        source,
        spatial,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_projected_segment(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    start: ProjectedPoint,
    end: ProjectedPoint,
    role: ProjectionPrimitiveRole,
    depth: ProjectionDepthRange,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    if start.u == end.u && start.v == end.v {
        return Ok(());
    }
    let id = builder.next_id(view, source);
    builder.add_primitive(
        view,
        id,
        ProjectedGeometry::Segment {
            start: crate::ProjectedPoint2 {
                u: start.u,
                v: start.v,
            },
            end: crate::ProjectedPoint2 { u: end.u, v: end.v },
        },
        role,
        depth,
        source.clone(),
        spatial,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_circle(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    frame: OrthographicFrame,
    center: PartLocalPoint3,
    radius: f64,
    depth: ProjectionDepthRange,
    role: ProjectionPrimitiveRole,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let center = project(frame, center)?;
    let id = builder.next_id(view, source);
    builder.add_primitive(
        view,
        id,
        ProjectedGeometry::Circle {
            center: crate::ProjectedPoint2 {
                u: center.u,
                v: center.v,
            },
            radius,
        },
        role,
        depth,
        source.clone(),
        spatial,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_profile_arc(
    builder: &mut ProjectionModelBuilder,
    view: PrincipalViewKind,
    frame: OrthographicFrame,
    center: PartLocalPoint3,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
    radius: f64,
    direction: SpatialArcDirection,
    role: ProjectionPrimitiveRole,
    source: &SpatialElementRef,
    spatial: &SpatialModel,
) -> Result<(), ProjectionError> {
    let center_projected = project(frame, center)?;
    let start_projected = project(frame, start)?;
    let end_projected = project(frame, end)?;
    let start_angle =
        (start_projected.v - center_projected.v).atan2(start_projected.u - center_projected.u);
    let end_angle =
        (end_projected.v - center_projected.v).atan2(end_projected.u - center_projected.u);
    let (start_angle, end_angle) = match direction {
        SpatialArcDirection::CounterClockwise => (start_angle, end_angle),
        SpatialArcDirection::Clockwise => (end_angle, start_angle),
    };
    let id = builder.next_id(view, source);
    builder.add_primitive(
        view,
        id,
        ProjectedGeometry::Arc {
            center: crate::ProjectedPoint2 {
                u: center_projected.u,
                v: center_projected.v,
            },
            radius,
            start_angle,
            end_angle,
        },
        role,
        depth_at(frame, center)?,
        source.clone(),
        spatial,
    )
}

fn depth_at(
    frame: OrthographicFrame,
    point: PartLocalPoint3,
) -> Result<ProjectionDepthRange, ProjectionError> {
    let projected = project(frame, point)?;
    ProjectionDepthRange::new(projected.depth, projected.depth)
}

fn depth_from_projected(
    start: ProjectedPoint,
    end: ProjectedPoint,
) -> Result<ProjectionDepthRange, ProjectionError> {
    ProjectionDepthRange::new(start.depth.min(end.depth), start.depth.max(end.depth))
}

fn depth_from_axis_points(
    frame: OrthographicFrame,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
) -> Result<ProjectionDepthRange, ProjectionError> {
    depth_from_projected(project(frame, start)?, project(frame, end)?)
}

fn project(
    frame: OrthographicFrame,
    point: PartLocalPoint3,
) -> Result<ProjectedPoint, ProjectionError> {
    let projected = frame.project_point(point);
    if !projected.u.is_finite() || !projected.v.is_finite() {
        return Err(ProjectionError::InvalidProjectionGeometry {
            reason: InvalidProjectionGeometryReason::NonFiniteCoordinate,
        });
    }
    if !projected.depth.is_finite() {
        return Err(ProjectionError::InvalidProjectionDepthRange {
            min: projected.depth,
            max: projected.depth,
        });
    }
    Ok(projected)
}

fn frame_for(view: PrincipalViewKind) -> OrthographicFrame {
    match view {
        PrincipalViewKind::Front => OrthographicFrame::front(),
        PrincipalViewKind::Top => OrthographicFrame::top(),
        PrincipalViewKind::Left => OrthographicFrame::left(),
    }
}

fn point(x: f64, y: f64, z: f64) -> PartLocalPoint3 {
    PartLocalPoint3 { x, y, z }
}

#[derive(Clone, Copy)]
enum Arithmetic {
    Geometry,
    Depth,
}

fn finite_add(a: f64, b: f64, arithmetic: Arithmetic) -> Result<f64, ProjectionError> {
    let value = a + b;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(arithmetic_error(arithmetic, value))
    }
}

fn finite_sub(a: f64, b: f64, arithmetic: Arithmetic) -> Result<f64, ProjectionError> {
    let value = a - b;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(arithmetic_error(arithmetic, value))
    }
}

fn finite_neg(value: f64, arithmetic: Arithmetic) -> Result<f64, ProjectionError> {
    let value = -value;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(arithmetic_error(arithmetic, value))
    }
}

fn arithmetic_error(arithmetic: Arithmetic, value: f64) -> ProjectionError {
    match arithmetic {
        Arithmetic::Geometry => ProjectionError::InvalidProjectionGeometry {
            reason: InvalidProjectionGeometryReason::NonFiniteCoordinate,
        },
        Arithmetic::Depth => ProjectionError::InvalidProjectionDepthRange {
            min: value,
            max: value,
        },
    }
}
