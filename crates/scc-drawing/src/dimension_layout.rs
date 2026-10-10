//! Deterministic dimension layout (Phase 8.6): *where* each dimension is
//! drawn, in sheet-space millimetres.
//!
//! Layering (ADR-027):
//!
//! ```text
//! DrawingDimension   (semantic: what is measured, where the number is from)
//!         ↓ deterministic layout
//! DrawingDimensionLayout   (presentation geometry: lines, anchors)
//!         ↓ future
//! Renderer / Formatter     ("120", arrow polygons, fonts)
//! ```
//!
//! Two independent chains meet here and must agree:
//!
//! - the dimension **value** comes from the engineering model (Phase 8.5);
//! - the **witness geometry** comes from solved drawing geometry
//!   (`target FeatureId → DrawingSourceMap → primitives → transform`).
//!
//! Before a layout is produced the two are cross-checked with a fixed
//! epsilon; a mismatch means engineering/solved consistency is broken and
//! fails closed (`SCC-DRW-014`) instead of picking one and drawing it.
//!
//! All coordinates are sheet space (`+X` right, `+Y` up, origin bottom-left;
//! ADR-024). Offsets, stack pitch, overhang, gaps, and arrow size are **paper
//! millimetres** and never scale with [`crate::DrawingScale`] — only witness
//! spans scale. MVP placement sides are fixed: linear-horizontal above the
//! object, linear-vertical right of the object, diameter through-circle at a
//! fixed angle, patterns as upper-right callouts. Geometry placement is never
//! re-fitted; the sheet envelope simply grows toward +X/+Y.

use std::collections::BTreeMap;

use scc_engineering::source_map::EngineeringSourceMap;
use serde::{Deserialize, Serialize};

use crate::dimension::{
    DimensionValue, DrawingDimension, DrawingDimensionId, DrawingDimensionKind,
    DrawingDimensionTarget, LinearAxis, PatternShape, ResolvedDimensionValue,
};
use crate::model::{DrawingBounds, DrawingPrimitive, DrawingView};
use crate::source_map::DrawingSourceMap;
use crate::{DrawingError, DrawingPoint, DrawingViewTransform};

/// Distance from the object edge to the first dimension line, paper mm.
pub const DEFAULT_DIMENSION_OFFSET_MM: f64 = 8.0;
/// Vertical/horizontal pitch between stacked dimension lines, paper mm.
pub const DEFAULT_DIMENSION_STACK_PITCH_MM: f64 = 7.0;
/// How far extension lines run past the dimension line, paper mm.
pub const DEFAULT_EXTENSION_OVERHANG_MM: f64 = 2.0;
/// Gap between the object edge and the extension-line start, paper mm.
pub const DEFAULT_EXTENSION_GAP_MM: f64 = 1.0;
/// Arrowhead size hint for renderers, paper mm (never a polygon here).
pub const DEFAULT_ARROW_SIZE_MM: f64 = 2.5;
/// Gap between the dimension line and its text anchor, paper mm.
pub const DEFAULT_DIMENSION_TEXT_GAP_MM: f64 = 1.5;

/// Fixed diameter-leader axis angle through the circle centre, degrees.
pub const DIAMETER_AXIS_ANGLE_DEG: f64 = 45.0;

/// Witness-comparison epsilon (model mm). The solver resolves coordinates to a
/// tighter epsilon of its own; this slightly looser bound tolerates float
/// noise across the lowering→solve→draw path while still catching real
/// mismatches. Defined in ADR-027.
pub const WITNESS_EPSILON_MM: f64 = 1.0e-6;

/// A straight layout segment in sheet space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutSegment {
    pub start: DrawingPoint,
    pub end: DrawingPoint,
}

/// Where an arrow goes: tip position plus the unit direction pointing *into*
/// the measured span. Renderers draw the actual arrowhead (sized by
/// [`DEFAULT_ARROW_SIZE_MM`]); no renderer-specific polygon lives here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArrowAnchor {
    pub point: DrawingPoint,
    pub direction: DrawingPoint,
}

/// Whether dimension text reads horizontally or rotated vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOrientation {
    Horizontal,
    Vertical,
}

/// The presentation geometry of one laid-out dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "geometry", rename_all = "snake_case")]
pub enum DimensionLayoutGeometry {
    /// One dimension line, two extension lines, two arrow anchors, text anchor.
    Linear {
        dimension_line: LayoutSegment,
        extension_lines: [LayoutSegment; 2],
        arrows: [ArrowAnchor; 2],
        text_anchor: DrawingPoint,
        text_orientation: TextOrientation,
    },
    /// One straight axis through the circle centre at the fixed angle; arrows
    /// on the circle edge; text beyond the upper-right end.
    Diameter {
        center: DrawingPoint,
        axis_line: LayoutSegment,
        arrows: [ArrowAnchor; 2],
        text_anchor: DrawingPoint,
    },
    /// One radius leader from the arc centre to its exact boundary, with one
    /// inward-pointing arrow and horizontal `R...` text beyond the boundary.
    Radius {
        center: DrawingPoint,
        radius_line: LayoutSegment,
        arrow: ArrowAnchor,
        text_anchor: DrawingPoint,
    },
    /// Anchor on the pattern's full footprint, short leader, text outside.
    PatternCallout {
        anchor_on_pattern: DrawingPoint,
        leader: LayoutSegment,
        text_anchor: DrawingPoint,
        footprint: DrawingBounds,
    },
}

impl DimensionLayoutGeometry {
    /// Every point referenced by this layout.
    pub(crate) fn points(&self) -> Vec<&DrawingPoint> {
        match self {
            DimensionLayoutGeometry::Linear {
                dimension_line,
                extension_lines,
                arrows,
                text_anchor,
                ..
            } => vec![
                &dimension_line.start,
                &dimension_line.end,
                &extension_lines[0].start,
                &extension_lines[0].end,
                &extension_lines[1].start,
                &extension_lines[1].end,
                &arrows[0].point,
                &arrows[1].point,
                text_anchor,
            ],
            DimensionLayoutGeometry::Diameter {
                center,
                axis_line,
                arrows,
                text_anchor,
            } => vec![
                center,
                &axis_line.start,
                &axis_line.end,
                &arrows[0].point,
                &arrows[1].point,
                text_anchor,
            ],
            DimensionLayoutGeometry::Radius {
                center,
                radius_line,
                arrow,
                text_anchor,
            } => vec![
                center,
                &radius_line.start,
                &radius_line.end,
                &arrow.point,
                text_anchor,
            ],
            DimensionLayoutGeometry::PatternCallout {
                anchor_on_pattern,
                leader,
                text_anchor,
                ..
            } => vec![anchor_on_pattern, &leader.start, &leader.end, text_anchor],
        }
    }
}

/// The laid-out presentation geometry for exactly one dimension. References
/// the dimension by id only — values, sources, and targets are never
/// duplicated here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingDimensionLayout {
    pub dimension: DrawingDimensionId,
    pub geometry: DimensionLayoutGeometry,
}

impl DrawingDimensionLayout {
    /// Bounding box of every point in this layout (sheet space).
    pub fn extents(&self) -> DrawingBounds {
        let mut acc: Option<DrawingBounds> = None;
        for p in self.geometry.points() {
            let b = DrawingBounds {
                min_x: p.x,
                min_y: p.y,
                max_x: p.x,
                max_y: p.y,
            };
            match &mut acc {
                Some(a) => a.include(&b),
                None => acc = Some(b),
            }
        }
        acc.unwrap_or_else(DrawingBounds::zero)
    }
}

/// One feature's primitives in deterministic id order.
fn primitives_for<'a>(
    feature: &scc_engineering::FeatureId,
    drawing_source_map: &'a DrawingSourceMap,
    eng_map: &'a EngineeringSourceMap,
    primitives: &'a BTreeMap<crate::DrawingPrimitiveId, DrawingPrimitive>,
) -> Vec<&'a DrawingPrimitive> {
    drawing_source_map
        .primitives_for_feature(feature, eng_map)
        .iter()
        .filter_map(|id| primitives.get(id))
        .collect()
}

/// The first rectangle of a feature (model space): (left, right, bottom, top).
fn model_rectangle(prims: &[&DrawingPrimitive]) -> Option<(f64, f64, f64, f64)> {
    if let Some(rectangle) = prims.iter().find_map(|p| match p {
        DrawingPrimitive::Rectangle {
            origin,
            width,
            height,
            ..
        } => Some((origin.x, origin.x + width, origin.y, origin.y + height)),
        _ => None,
    }) {
        return Some(rectangle);
    }

    let mut bounds: Option<DrawingBounds> = None;
    for primitive in prims {
        let primitive_bounds = crate::builder::primitive_bounds(primitive).ok()?;
        match &mut bounds {
            Some(acc) => acc.include(&primitive_bounds),
            None => bounds = Some(primitive_bounds),
        }
    }
    bounds.map(|bounds| (bounds.min_x, bounds.max_x, bounds.min_y, bounds.max_y))
}

/// The first circle of a feature (model space): (centre, radius).
fn model_circle(prims: &[&DrawingPrimitive]) -> Option<(DrawingPoint, f64)> {
    prims.iter().find_map(|p| match p {
        DrawingPrimitive::Circle { center, radius, .. } => Some((*center, *radius)),
        _ => None,
    })
}

/// The first exact arc of a feature (model space): (centre, radius, start angle).
fn model_arc(prims: &[&DrawingPrimitive]) -> Option<(DrawingPoint, f64, f64)> {
    prims.iter().find_map(|primitive| match primitive {
        DrawingPrimitive::Arc {
            center,
            radius,
            start_angle,
            ..
        } => Some((*center, *radius, *start_angle)),
        _ => None,
    })
}

/// Union model-space footprint over every circle of a feature.
fn model_circle_footprint(prims: &[&DrawingPrimitive]) -> Option<DrawingBounds> {
    let mut acc: Option<DrawingBounds> = None;
    for p in prims {
        if let DrawingPrimitive::Circle { center, radius, .. } = p {
            let b = DrawingBounds {
                min_x: center.x - radius,
                min_y: center.y - radius,
                max_x: center.x + radius,
                max_y: center.y + radius,
            };
            match &mut acc {
                Some(a) => a.include(&b),
                None => acc = Some(b),
            }
        }
    }
    acc
}

/// Fail-closed comparison of an engineering value against its witness (mm).
fn require_witness_matches(
    dimension_id: &DrawingDimensionId,
    engineering_mm: f64,
    witness_mm: f64,
) -> Result<(), DrawingError> {
    if witness_mm.is_finite() && (engineering_mm - witness_mm).abs() <= WITNESS_EPSILON_MM {
        Ok(())
    } else {
        Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} engineering value {} mm does not match its geometry witness {} mm",
                dimension_id, engineering_mm, witness_mm
            ),
        })
    }
}

/// Generate deterministic layouts for every dimension of a view.
///
/// `geometry_sheet_bounds` is the view's geometry-only sheet content bounds —
/// the reference edge for side placement. Witness geometry comes from the
/// view's primitives through the source maps, transformed once.
pub fn generate_dimension_layouts(
    primitives: &BTreeMap<crate::DrawingPrimitiveId, DrawingPrimitive>,
    dimensions: &BTreeMap<DrawingDimensionId, DrawingDimension>,
    drawing_source_map: &DrawingSourceMap,
    eng_map: &EngineeringSourceMap,
    transform: &DrawingViewTransform,
    geometry_sheet_bounds: &DrawingBounds,
) -> Result<BTreeMap<DrawingDimensionId, DrawingDimensionLayout>, DrawingError> {
    // Stacking order: BTreeMap iteration is sorted by semantic dimension id
    // ("{feature}.{slot}") — deterministic, grouped by feature then slot.
    let mut layouts = BTreeMap::new();
    let mut horizontal_stack = 0usize;
    let mut vertical_stack = 0usize;

    for (dim_id, dimension) in dimensions {
        let DrawingDimensionTarget::Feature { feature } = &dimension.target else {
            return Err(DrawingError::InvalidDimensionWitness {
                reason: format!(
                    "dimension {} body target must use validated parametric layout",
                    dim_id
                ),
            });
        };
        let feature_prims = primitives_for(feature, drawing_source_map, eng_map, primitives);

        let layout = match (&dimension.kind, &dimension.value) {
            (DrawingDimensionKind::Linear { axis }, DimensionValue::Measured { resolved, .. }) => {
                let Some((left, right, bottom, top)) = model_rectangle(&feature_prims) else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "linear dimension {} has no rectangle witness for feature {}",
                            dim_id, feature
                        ),
                    });
                };
                let ResolvedDimensionValue::LengthMm { mm } = resolved else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!("linear dimension {} has a non-length value", dim_id),
                    });
                };
                match axis {
                    LinearAxis::Horizontal => {
                        require_witness_matches(dim_id, *mm, right - left)?;
                        let x1 = transform.model_to_sheet(DrawingPoint::new(left, 0.0)).x;
                        let x2 = transform.model_to_sheet(DrawingPoint::new(right, 0.0)).x;
                        let rect_top = transform.model_to_sheet(DrawingPoint::new(0.0, top)).y;
                        let dim_y = geometry_sheet_bounds.max_y
                            + DEFAULT_DIMENSION_OFFSET_MM
                            + horizontal_stack as f64 * DEFAULT_DIMENSION_STACK_PITCH_MM;
                        horizontal_stack += 1;
                        DrawingDimensionLayout {
                            dimension: dim_id.clone(),
                            geometry: DimensionLayoutGeometry::Linear {
                                dimension_line: LayoutSegment {
                                    start: DrawingPoint::new(x1, dim_y),
                                    end: DrawingPoint::new(x2, dim_y),
                                },
                                extension_lines: [
                                    LayoutSegment {
                                        start: DrawingPoint::new(
                                            x1,
                                            rect_top + DEFAULT_EXTENSION_GAP_MM,
                                        ),
                                        end: DrawingPoint::new(
                                            x1,
                                            dim_y + DEFAULT_EXTENSION_OVERHANG_MM,
                                        ),
                                    },
                                    LayoutSegment {
                                        start: DrawingPoint::new(
                                            x2,
                                            rect_top + DEFAULT_EXTENSION_GAP_MM,
                                        ),
                                        end: DrawingPoint::new(
                                            x2,
                                            dim_y + DEFAULT_EXTENSION_OVERHANG_MM,
                                        ),
                                    },
                                ],
                                arrows: [
                                    ArrowAnchor {
                                        point: DrawingPoint::new(x1, dim_y),
                                        direction: DrawingPoint::new(1.0, 0.0),
                                    },
                                    ArrowAnchor {
                                        point: DrawingPoint::new(x2, dim_y),
                                        direction: DrawingPoint::new(-1.0, 0.0),
                                    },
                                ],
                                text_anchor: DrawingPoint::new(
                                    (x1 + x2) / 2.0,
                                    dim_y + DEFAULT_DIMENSION_TEXT_GAP_MM,
                                ),
                                text_orientation: TextOrientation::Horizontal,
                            },
                        }
                    }
                    LinearAxis::Vertical => {
                        require_witness_matches(dim_id, *mm, top - bottom)?;
                        let y1 = transform.model_to_sheet(DrawingPoint::new(0.0, bottom)).y;
                        let y2 = transform.model_to_sheet(DrawingPoint::new(0.0, top)).y;
                        let rect_right = transform.model_to_sheet(DrawingPoint::new(right, 0.0)).x;
                        let dim_x = geometry_sheet_bounds.max_x
                            + DEFAULT_DIMENSION_OFFSET_MM
                            + vertical_stack as f64 * DEFAULT_DIMENSION_STACK_PITCH_MM;
                        vertical_stack += 1;
                        DrawingDimensionLayout {
                            dimension: dim_id.clone(),
                            geometry: DimensionLayoutGeometry::Linear {
                                dimension_line: LayoutSegment {
                                    start: DrawingPoint::new(dim_x, y1),
                                    end: DrawingPoint::new(dim_x, y2),
                                },
                                extension_lines: [
                                    LayoutSegment {
                                        start: DrawingPoint::new(
                                            rect_right + DEFAULT_EXTENSION_GAP_MM,
                                            y1,
                                        ),
                                        end: DrawingPoint::new(
                                            dim_x + DEFAULT_EXTENSION_OVERHANG_MM,
                                            y1,
                                        ),
                                    },
                                    LayoutSegment {
                                        start: DrawingPoint::new(
                                            rect_right + DEFAULT_EXTENSION_GAP_MM,
                                            y2,
                                        ),
                                        end: DrawingPoint::new(
                                            dim_x + DEFAULT_EXTENSION_OVERHANG_MM,
                                            y2,
                                        ),
                                    },
                                ],
                                arrows: [
                                    ArrowAnchor {
                                        point: DrawingPoint::new(dim_x, y1),
                                        direction: DrawingPoint::new(0.0, 1.0),
                                    },
                                    ArrowAnchor {
                                        point: DrawingPoint::new(dim_x, y2),
                                        direction: DrawingPoint::new(0.0, -1.0),
                                    },
                                ],
                                text_anchor: DrawingPoint::new(
                                    dim_x + DEFAULT_DIMENSION_TEXT_GAP_MM,
                                    (y1 + y2) / 2.0,
                                ),
                                text_orientation: TextOrientation::Vertical,
                            },
                        }
                    }
                }
            }
            (
                DrawingDimensionKind::Diameter | DrawingDimensionKind::OverallDiameter,
                DimensionValue::Measured { resolved, .. },
            ) => {
                let Some((center, radius)) = model_circle(&feature_prims) else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "diameter dimension {} has no circle witness for feature {}",
                            dim_id, feature
                        ),
                    });
                };
                let ResolvedDimensionValue::LengthMm { mm } = resolved else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!("diameter dimension {} has a non-length value", dim_id),
                    });
                };
                require_witness_matches(dim_id, *mm, 2.0 * radius)?;
                let c = transform.model_to_sheet(center);
                let r_sheet = radius * transform.scale.0;
                let dir_x = DIAMETER_AXIS_ANGLE_DEG.to_radians().cos();
                let dir_y = DIAMETER_AXIS_ANGLE_DEG.to_radians().sin();
                let a = DrawingPoint::new(c.x - r_sheet * dir_x, c.y - r_sheet * dir_y);
                let b = DrawingPoint::new(c.x + r_sheet * dir_x, c.y + r_sheet * dir_y);
                DrawingDimensionLayout {
                    dimension: dim_id.clone(),
                    geometry: DimensionLayoutGeometry::Diameter {
                        center: c,
                        axis_line: LayoutSegment { start: a, end: b },
                        arrows: [
                            ArrowAnchor {
                                point: a,
                                direction: DrawingPoint::new(dir_x, dir_y),
                            },
                            ArrowAnchor {
                                point: b,
                                direction: DrawingPoint::new(-dir_x, -dir_y),
                            },
                        ],
                        text_anchor: DrawingPoint::new(
                            b.x + dir_x * DEFAULT_DIMENSION_TEXT_GAP_MM,
                            b.y + dir_y * DEFAULT_DIMENSION_TEXT_GAP_MM,
                        ),
                    },
                }
            }
            (
                DrawingDimensionKind::Diameter,
                DimensionValue::HoleCallout {
                    semantic,
                    pattern: None,
                },
            ) => {
                let Some((center, radius)) = model_circle(&feature_prims) else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "diameter dimension {} has no circle witness for feature {}",
                            dim_id, feature
                        ),
                    });
                };
                let ResolvedDimensionValue::LengthMm { mm } = semantic.diameter.resolved else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!("diameter dimension {} has a non-length value", dim_id),
                    });
                };
                require_witness_matches(dim_id, mm, 2.0 * radius)?;
                let c = transform.model_to_sheet(center);
                let r_sheet = radius * transform.scale.0;
                let dir_x = DIAMETER_AXIS_ANGLE_DEG.to_radians().cos();
                let dir_y = DIAMETER_AXIS_ANGLE_DEG.to_radians().sin();
                let a = DrawingPoint::new(c.x - r_sheet * dir_x, c.y - r_sheet * dir_y);
                let b = DrawingPoint::new(c.x + r_sheet * dir_x, c.y + r_sheet * dir_y);
                DrawingDimensionLayout {
                    dimension: dim_id.clone(),
                    geometry: DimensionLayoutGeometry::Diameter {
                        center: c,
                        axis_line: LayoutSegment { start: a, end: b },
                        arrows: [
                            ArrowAnchor {
                                point: a,
                                direction: DrawingPoint::new(dir_x, dir_y),
                            },
                            ArrowAnchor {
                                point: b,
                                direction: DrawingPoint::new(-dir_x, -dir_y),
                            },
                        ],
                        text_anchor: DrawingPoint::new(
                            b.x + dir_x * DEFAULT_DIMENSION_TEXT_GAP_MM,
                            b.y + dir_y * DEFAULT_DIMENSION_TEXT_GAP_MM,
                        ),
                    },
                }
            }
            (DrawingDimensionKind::Pattern, DimensionValue::Pattern { shape })
            | (
                DrawingDimensionKind::Pattern,
                DimensionValue::HoleCallout {
                    pattern: Some(shape),
                    ..
                },
            ) => {
                // Count integrity (circular patterns): represented circles must
                // equal the engineering count — the pipeline expands exactly
                // `count` elements, so a mismatch means lost presentation.
                if let PatternShape::Circular { count, .. } = shape {
                    let expected = match count.resolved {
                        ResolvedDimensionValue::Count { count } => count,
                        _ => 0,
                    };
                    if expected != feature_prims.len() as u32 {
                        return Err(DrawingError::InvalidDimensionWitness {
                            reason: format!(
                                "pattern dimension {} expects {} circles but {} are represented",
                                dim_id,
                                expected,
                                feature_prims.len()
                            ),
                        });
                    }
                }
                let Some(footprint_model) = model_circle_footprint(&feature_prims) else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "pattern dimension {} has no circle footprint for feature {}",
                            dim_id, feature
                        ),
                    });
                };
                let fmin = transform.model_to_sheet(DrawingPoint::new(
                    footprint_model.min_x,
                    footprint_model.min_y,
                ));
                let fmax = transform.model_to_sheet(DrawingPoint::new(
                    footprint_model.max_x,
                    footprint_model.max_y,
                ));
                let anchor = DrawingPoint::new(fmax.x, fmax.y);
                let leader_end = DrawingPoint::new(
                    fmax.x + DEFAULT_DIMENSION_OFFSET_MM,
                    fmax.y + DEFAULT_DIMENSION_OFFSET_MM,
                );
                DrawingDimensionLayout {
                    dimension: dim_id.clone(),
                    geometry: DimensionLayoutGeometry::PatternCallout {
                        anchor_on_pattern: anchor,
                        leader: LayoutSegment {
                            start: anchor,
                            end: leader_end,
                        },
                        text_anchor: DrawingPoint::new(
                            leader_end.x + DEFAULT_DIMENSION_TEXT_GAP_MM,
                            leader_end.y,
                        ),
                        footprint: DrawingBounds {
                            min_x: fmin.x,
                            min_y: fmin.y,
                            max_x: fmax.x,
                            max_y: fmax.y,
                        },
                    },
                }
            }
            (DrawingDimensionKind::Radius, DimensionValue::Measured { resolved, .. }) => {
                let Some((center, radius, start_angle)) = model_arc(&feature_prims) else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!(
                            "radius dimension {} has no arc witness for feature {}",
                            dim_id, feature
                        ),
                    });
                };
                let ResolvedDimensionValue::LengthMm { mm } = resolved else {
                    return Err(DrawingError::InvalidDimensionWitness {
                        reason: format!("radius dimension {} has a non-length value", dim_id),
                    });
                };
                require_witness_matches(dim_id, *mm, radius)?;
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
                DrawingDimensionLayout {
                    dimension: dim_id.clone(),
                    geometry: DimensionLayoutGeometry::Radius {
                        center,
                        radius_line: LayoutSegment {
                            start: center,
                            end: leader_end,
                        },
                        arrow: ArrowAnchor {
                            point: boundary,
                            direction: DrawingPoint::new(-direction.x, -direction.y),
                        },
                        text_anchor: DrawingPoint::new(
                            leader_end.x + DEFAULT_DIMENSION_TEXT_GAP_MM,
                            leader_end.y,
                        ),
                    },
                }
            }
            _ => {
                return Err(DrawingError::InvalidDimensionLayout {
                    reason: format!("dimension {} kind and value disagree", dim_id),
                })
            }
        };
        layouts.insert(dim_id.clone(), layout);
    }
    Ok(layouts)
}

/// Validate dimension/layout pairing and geometry, fail-closed.
///
/// Checks: every dimension has a layout; every layout references an existing
/// dimension (no orphans); every referenced point is finite; linear/dimension
/// lines are non-degenerate.
pub fn validate_dimension_layout_integrity(view: &DrawingView) -> Result<(), DrawingError> {
    for dim_id in view.dimensions.keys() {
        if !view.dimension_layouts.contains_key(dim_id) {
            return Err(DrawingError::MissingDimensionLayout {
                dimension: dim_id.clone(),
            });
        }
    }
    for (layout_id, layout) in &view.dimension_layouts {
        if !view.dimensions.contains_key(layout_id) {
            return Err(DrawingError::InvalidDimensionLayout {
                reason: format!(
                    "layout references dimension {} which does not exist",
                    layout.dimension
                ),
            });
        }
        for point in layout.geometry.points() {
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err(DrawingError::InvalidDimensionLayout {
                    reason: format!("layout {} contains a non-finite coordinate", layout_id),
                });
            }
        }
        let degenerate = match &layout.geometry {
            DimensionLayoutGeometry::Linear { dimension_line, .. } => {
                segment_length_sq(dimension_line) <= WITNESS_EPSILON_MM * WITNESS_EPSILON_MM
            }
            DimensionLayoutGeometry::Diameter { axis_line, .. } => {
                segment_length_sq(axis_line) <= WITNESS_EPSILON_MM * WITNESS_EPSILON_MM
            }
            DimensionLayoutGeometry::Radius { radius_line, .. } => {
                segment_length_sq(radius_line) <= WITNESS_EPSILON_MM * WITNESS_EPSILON_MM
            }
            DimensionLayoutGeometry::PatternCallout { .. } => false,
        };
        if degenerate {
            return Err(DrawingError::InvalidDimensionLayout {
                reason: format!("layout {} has a degenerate dimension line", layout_id),
            });
        }
    }
    Ok(())
}

fn segment_length_sq(segment: &LayoutSegment) -> f64 {
    let dx = segment.end.x - segment.start.x;
    let dy = segment.end.y - segment.start.y;
    dx * dx + dy * dy
}
