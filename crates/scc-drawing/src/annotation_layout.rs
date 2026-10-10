//! Deterministic Phase 11.2 annotation collision detection and bounded reflow.
//!
//! This module is a presentation-only pass over existing
//! [`DrawingDimensionLayout`] values. It uses a conservative, renderer-neutral
//! text envelope (character advance × paper text height) because the drawing
//! model intentionally does not own font metrics. Linear dimensions may move
//! farther out on their existing side; diameter axes and pattern leaders are
//! never moved because doing so would detach them from their witness geometry.
//! An unresolved collision fails closed instead of emitting a crowded drawing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dimension::{format_dimension, DrawingDimensionKind, LinearAxis};
use crate::dimension_layout::{
    DimensionLayoutGeometry, DrawingDimensionLayout, LayoutSegment, TextOrientation,
    DEFAULT_DIMENSION_STACK_PITCH_MM,
};
use crate::model::{DrawingBounds, DrawingView};
use crate::{DrawingDimensionId, DrawingError, DrawingPoint};

/// Conservative paper-space clearance around each dimension footprint.
pub const DEFAULT_ANNOTATION_COLLISION_CLEARANCE_MM: f64 = 1.0;
/// Approximate advance of one dimension-text glyph as a fraction of text height.
pub const APPROXIMATE_DIMENSION_GLYPH_ADVANCE: f64 = 0.6;
/// Maximum outward candidates tried for one reflowed linear dimension.
pub const MAX_ANNOTATION_REFLOW_ATTEMPTS: usize = 12;

/// A deterministic pair of dimension footprints that overlap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationLayoutCollision {
    pub first: DrawingDimensionId,
    pub second: DrawingDimensionId,
    pub overlap: DrawingBounds,
}

/// Result of one bounded layout pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationLayoutReport {
    pub adjusted_layouts: usize,
    pub collisions: Vec<AnnotationLayoutCollision>,
}

/// Reflow all dimensions in one view in stable ID order.
pub fn optimize_dimension_layouts(
    view: &mut DrawingView,
) -> Result<AnnotationLayoutReport, DrawingError> {
    let ids: Vec<DrawingDimensionId> = view.dimensions.keys().cloned().collect();
    let mut accepted: BTreeMap<DrawingDimensionId, DrawingBounds> = BTreeMap::new();
    let mut adjusted_layouts = 0usize;

    for id in ids {
        let dimension =
            view.dimensions
                .get(&id)
                .ok_or_else(|| DrawingError::InvalidDimensionLayout {
                    reason: format!("dimension {} disappeared during layout", id),
                })?;
        let original = view.dimension_layouts.get(&id).ok_or_else(|| {
            DrawingError::MissingDimensionLayout {
                dimension: id.clone(),
            }
        })?;
        let direction = if matches!(
            original.geometry,
            DimensionLayoutGeometry::PatternCallout { .. }
        ) {
            (1.0, 1.0)
        } else {
            reflow_direction(&dimension.kind)
        };
        let mut chosen = None;
        for attempt in 0..=MAX_ANNOTATION_REFLOW_ATTEMPTS {
            let mut candidate = original.clone();
            let dx = direction.0 * DEFAULT_DIMENSION_STACK_PITCH_MM * attempt as f64;
            let dy = direction.1 * DEFAULT_DIMENSION_STACK_PITCH_MM * attempt as f64;
            if matches!(
                candidate.geometry,
                DimensionLayoutGeometry::PatternCallout { .. }
            ) {
                extend_pattern_leader(&mut candidate.geometry, dx, dy);
            } else {
                translate_layout(&mut candidate.geometry, dx, dy);
            }
            let footprint = dimension_footprint(dimension, &candidate)?;
            if accepted
                .values()
                .all(|other| !footprints_overlap(*other, footprint))
            {
                chosen = Some((candidate, footprint, attempt));
                break;
            }
        }
        let Some((candidate, footprint, attempt)) = chosen else {
            let current = dimension_footprint(dimension, original)?;
            let collision = accepted.iter().find_map(|(other_id, other)| {
                overlap_bounds(*other, current).map(|overlap| AnnotationLayoutCollision {
                    first: other_id.clone(),
                    second: id.clone(),
                    overlap,
                })
            });
            return Err(DrawingError::InvalidDimensionLayout {
                reason: collision.map_or_else(
                    || format!("dimension {} cannot be placed without collision", id),
                    |c| format!("dimensions {} and {} collide", c.first, c.second),
                ),
            });
        };
        if attempt != 0 {
            adjusted_layouts += 1;
            view.dimension_layouts.insert(id.clone(), candidate);
        }
        accepted.insert(id, footprint);
    }

    Ok(AnnotationLayoutReport {
        adjusted_layouts,
        collisions: Vec::new(),
    })
}

/// Validate that all dimension footprints are separated by the fixed clearance.
pub fn validate_dimension_layout_collisions(view: &DrawingView) -> Result<(), DrawingError> {
    let ids: Vec<_> = view.dimensions.keys().cloned().collect();
    let mut footprints = BTreeMap::new();
    for id in &ids {
        let dimension =
            view.dimensions
                .get(id)
                .ok_or_else(|| DrawingError::InvalidDimensionLayout {
                    reason: format!("dimension {} is missing during collision validation", id),
                })?;
        let layout =
            view.dimension_layouts
                .get(id)
                .ok_or_else(|| DrawingError::MissingDimensionLayout {
                    dimension: id.clone(),
                })?;
        footprints.insert(id.clone(), dimension_footprint(dimension, layout)?);
    }
    let ordered: Vec<_> = footprints.iter().collect();
    for (index, (first, first_bounds)) in ordered.iter().enumerate() {
        for (second, second_bounds) in ordered.iter().skip(index + 1) {
            if let Some(overlap) = overlap_bounds(**first_bounds, **second_bounds) {
                return Err(DrawingError::InvalidDimensionLayout {
                    reason: format!(
                        "dimensions {} and {} overlap at ({}, {})–({}, {})",
                        first, second, overlap.min_x, overlap.min_y, overlap.max_x, overlap.max_y
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Reflow every formal view. Views without dimensions are accepted unchanged.
pub fn optimize_three_view_layouts(
    views: &mut BTreeMap<crate::DrawingViewId, DrawingView>,
) -> Result<AnnotationLayoutReport, DrawingError> {
    let mut adjusted_layouts = 0;
    for view in views.values_mut() {
        let report = optimize_dimension_layouts(view)?;
        adjusted_layouts += report.adjusted_layouts;
    }
    for view in views.values() {
        validate_dimension_layout_collisions(view)?;
    }
    Ok(AnnotationLayoutReport {
        adjusted_layouts,
        collisions: Vec::new(),
    })
}

fn reflow_direction(kind: &DrawingDimensionKind) -> (f64, f64) {
    match kind {
        // Phase 11.1 formal linear dimensions are below/left of their view.
        DrawingDimensionKind::Linear {
            axis: LinearAxis::Horizontal,
        } => (0.0, -1.0),
        DrawingDimensionKind::Linear {
            axis: LinearAxis::Vertical,
        } => (-1.0, 0.0),
        // Keep witness-attached diameter/pattern geometry fixed. A collision
        // will fail closed rather than detach the callout from its source.
        DrawingDimensionKind::Diameter
        | DrawingDimensionKind::OverallDiameter
        | DrawingDimensionKind::Radius => (0.0, 0.0),
        // Move only the leader endpoint and text; keep the feature anchor fixed.
        DrawingDimensionKind::Pattern | DrawingDimensionKind::SlotCallout => (1.0, 1.0),
    }
}

fn dimension_footprint(
    dimension: &crate::dimension::DrawingDimension,
    layout: &DrawingDimensionLayout,
) -> Result<DrawingBounds, DrawingError> {
    let mut bounds = match &layout.geometry {
        DimensionLayoutGeometry::PatternCallout { leader, .. } => DrawingBounds {
            min_x: leader.end.x,
            min_y: leader.end.y,
            max_x: leader.end.x,
            max_y: leader.end.y,
        },
        _ => layout.extents(),
    };
    let text = format_dimension(dimension);
    if text.is_empty() {
        return Err(DrawingError::InvalidDimensionLayout {
            reason: format!("dimension {} has empty formatted text", dimension.id),
        });
    }
    let (anchor, orientation) = match &layout.geometry {
        DimensionLayoutGeometry::Linear {
            text_anchor,
            text_orientation,
            ..
        } => (*text_anchor, *text_orientation),
        DimensionLayoutGeometry::Diameter { text_anchor, .. }
        | DimensionLayoutGeometry::Radius { text_anchor, .. }
        | DimensionLayoutGeometry::PatternCallout { text_anchor, .. } => {
            (*text_anchor, TextOrientation::Horizontal)
        }
    };
    let text_height = crate::svg::DEFAULT_DIMENSION_TEXT_HEIGHT_MM;
    let glyph_count = text.chars().count() as f64;
    let advance = glyph_count * text_height * APPROXIMATE_DIMENSION_GLYPH_ADVANCE;
    let (width, height) = match orientation {
        TextOrientation::Horizontal => (advance, text_height),
        TextOrientation::Vertical => (text_height, advance),
    };
    let text_bounds = DrawingBounds {
        min_x: anchor.x
            - if orientation == TextOrientation::Horizontal {
                width / 2.0
            } else {
                width
            },
        min_y: anchor.y - height * 0.25,
        max_x: anchor.x
            + if orientation == TextOrientation::Horizontal {
                width / 2.0
            } else {
                0.0
            },
        max_y: anchor.y + height * 0.75,
    };
    bounds.min_x = bounds.min_x.min(text_bounds.min_x);
    bounds.min_y = bounds.min_y.min(text_bounds.min_y);
    bounds.max_x = bounds.max_x.max(text_bounds.max_x);
    bounds.max_y = bounds.max_y.max(text_bounds.max_y);
    if !bounds.is_finite() {
        return Err(DrawingError::InvalidDimensionLayout {
            reason: format!("dimension {} has non-finite collision bounds", dimension.id),
        });
    }
    Ok(inflate(
        bounds,
        DEFAULT_ANNOTATION_COLLISION_CLEARANCE_MM / 2.0,
    ))
}

fn inflate(bounds: DrawingBounds, amount: f64) -> DrawingBounds {
    DrawingBounds {
        min_x: bounds.min_x - amount,
        min_y: bounds.min_y - amount,
        max_x: bounds.max_x + amount,
        max_y: bounds.max_y + amount,
    }
}

fn footprints_overlap(first: DrawingBounds, second: DrawingBounds) -> bool {
    overlap_bounds(first, second).is_some()
}

fn overlap_bounds(first: DrawingBounds, second: DrawingBounds) -> Option<DrawingBounds> {
    let overlap = DrawingBounds {
        min_x: first.min_x.max(second.min_x),
        min_y: first.min_y.max(second.min_y),
        max_x: first.max_x.min(second.max_x),
        max_y: first.max_y.min(second.max_y),
    };
    (overlap.max_x > overlap.min_x && overlap.max_y > overlap.min_y).then_some(overlap)
}

fn extend_pattern_leader(geometry: &mut DimensionLayoutGeometry, dx: f64, dy: f64) {
    if let DimensionLayoutGeometry::PatternCallout {
        leader,
        text_anchor,
        ..
    } = geometry
    {
        translate_point(&mut leader.end, dx, dy);
        translate_point(text_anchor, dx, dy);
    }
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
