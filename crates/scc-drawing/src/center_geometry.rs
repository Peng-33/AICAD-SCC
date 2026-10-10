//! Drawing-derived center geometry (Phase 8.7): center marks and centerlines
//! for circular features.
//!
//! Three distinct concepts, never conflated (ADR-028):
//!
//! 1. **Construction geometry** — `ConstructionCircle` features arrive as
//!    engineering-backed `DrawingPrimitive`s with role `Construction`. They
//!    *are* reference geometry already and get **no** center marks.
//! 2. **Center marks / centerlines** — pure drawing-derived presentation for
//!    circular *object* features (`Hole`, `HolePattern` elements). They do not
//!    exist upstream: no EntityId is fabricated, no FeatureId is created,
//!    nothing enters the primitive source map, and nothing writes back to the
//!    engineering or geometry models.
//! 3. **Dimension layout axes** — a diameter layout's 45° axis line is
//!    dimension geometry, never a centerline; it exists regardless of the
//!    `ShowCenterlines` toggle.
//!
//! All coordinates are sheet space (ADR-024). The mark half-length and the
//! centerline overhang are **paper millimetres**: `DrawingScale` moves and
//! scales the transformed circle but never these presentation constants.

use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_engineering::feature::{BaseProfile, EngineeringFeature};
use scc_engineering::source_map::EngineeringSourceMap;
use scc_engineering::FeatureId;
use serde::{Deserialize, Serialize};

use crate::dimension_layout::{LayoutSegment, WITNESS_EPSILON_MM};
use crate::model::{DrawingPrimitive, DrawingView};
use crate::source_map::DrawingSourceMap;
use crate::{DrawingError, DrawingPoint, DrawingViewTransform};

/// Half length of a center-mark cross, paper mm (6 mm total).
pub const DEFAULT_CENTER_MARK_HALF_LENGTH_MM: f64 = 3.0;
/// How far a centerline extends past the circle edge, paper mm.
pub const DEFAULT_CENTERLINE_OVERHANG_MM: f64 = 2.0;

/// Strong-typed, deterministic identifier of one center-geometry element.
///
/// Identity only — derived from the element kind and its source entity (e.g.
/// `"center_mark:center_hole"`, `"h_centerline:bolt_pattern_array[3]"`). The
/// typed fields carry the semantics; the id string is never parsed back.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingCenterGeometryId(pub String);

impl std::fmt::Display for DrawingCenterGeometryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Orientation of an axial centerline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CenterlineAxis {
    Horizontal,
    Vertical,
}

/// A small cross centred on a circle's sheet-space centre.
///
/// The two segments are horizontal and vertical, each
/// `2 × DEFAULT_CENTER_MARK_HALF_LENGTH_MM` long — paper mm at any scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CenterMark {
    /// The engineering feature this presentation derives from.
    pub source_feature: FeatureId,
    /// The exact presented circle entity, when derived from one.
    pub source_entity: Option<EntityId>,
    pub horizontal: LayoutSegment,
    pub vertical: LayoutSegment,
}

impl CenterMark {
    /// The cross centre (intersection midpoint of the two segments).
    pub fn center(&self) -> DrawingPoint {
        DrawingPoint::new(
            (self.horizontal.start.x + self.horizontal.end.x) / 2.0,
            (self.vertical.start.y + self.vertical.end.y) / 2.0,
        )
    }
}

/// One axial centerline extending past a circle's edge by
/// [`DEFAULT_CENTERLINE_OVERHANG_MM`] paper mm on both ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Centerline {
    pub source_feature: FeatureId,
    pub source_entity: Option<EntityId>,
    pub axis: CenterlineAxis,
    pub segment: LayoutSegment,
}

/// Drawing-only derived center presentation for one circle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawingCenterGeometry {
    CenterMark(CenterMark),
    Centerline(Centerline),
}

impl DrawingCenterGeometry {
    /// The engineering feature this element derives from.
    pub fn source_feature(&self) -> &FeatureId {
        match self {
            DrawingCenterGeometry::CenterMark(m) => &m.source_feature,
            DrawingCenterGeometry::Centerline(l) => &l.source_feature,
        }
    }

    /// Every point referenced by this element.
    pub(crate) fn points(&self) -> Vec<&DrawingPoint> {
        match self {
            DrawingCenterGeometry::CenterMark(m) => vec![
                &m.horizontal.start,
                &m.horizontal.end,
                &m.vertical.start,
                &m.vertical.end,
            ],
            DrawingCenterGeometry::Centerline(l) => vec![&l.segment.start, &l.segment.end],
        }
    }

    /// Bounding box of this element (sheet space).
    pub fn extents(&self) -> crate::DrawingBounds {
        let mut acc: Option<crate::DrawingBounds> = None;
        for p in self.points() {
            let b = crate::DrawingBounds {
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
        acc.unwrap_or_else(crate::DrawingBounds::zero)
    }
}

/// Generate deterministic center marks + axial centerlines for every
/// presented **object-role circle** whose feature is a `Hole` or
/// `HolePattern`.
///
/// - Construction circles are skipped by role (they are reference geometry).
/// - Non-circle representations of hole features fail closed
///   (`SCC-DRW-015`) — the builder never guesses a centre from a shape that
///   is not a circle.
/// - Element provenance comes from the strong-typed maps, never from parsing
///   id strings.
#[allow(clippy::too_many_arguments)]
pub fn generate_center_geometry(
    primitives: &BTreeMap<crate::DrawingPrimitiveId, DrawingPrimitive>,
    drawing_source_map: &DrawingSourceMap,
    eng_map: &EngineeringSourceMap,
    features: &BTreeMap<FeatureId, EngineeringFeature>,
    transform: &DrawingViewTransform,
) -> Result<BTreeMap<DrawingCenterGeometryId, DrawingCenterGeometry>, DrawingError> {
    let mut out = BTreeMap::new();
    // Provenance inverse once per call.
    let entity_to_feature = eng_map.entity_to_feature();
    // Deterministic iteration: primitives are keyed by entity-derived ids.
    for (prim_id, primitive) in primitives {
        let Some(entity) = drawing_source_map.entity_for_primitive(prim_id) else {
            return Err(DrawingError::MissingCenterSource {
                reason: format!("primitive {} has no provenance", prim_id),
            });
        };
        let Some(feature_id) = entity_to_feature.get(entity) else {
            return Err(DrawingError::MissingCenterSource {
                reason: format!("entity {} has no engineering feature", entity),
            });
        };
        let Some(feature) = features.get(feature_id) else {
            return Err(DrawingError::MissingCenterSource {
                reason: format!(
                    "feature {} does not exist in the engineering model",
                    feature_id
                ),
            });
        };
        // Hole-like features and circular material bases carry center
        // presentation. Body/cut provenance stays distinct in their sources.
        let circle_like = matches!(
            feature,
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Circle { .. },
                ..
            } | EngineeringFeature::Hole { .. }
                | EngineeringFeature::HolePattern { .. }
        );
        let DrawingPrimitive::Circle {
            role,
            center,
            radius,
        } = primitive
        else {
            // A hole/pattern feature must be represented by circles; any other
            // shape under it is a broken presentation (fail closed).
            if circle_like {
                return Err(DrawingError::InvalidCenterGeometry {
                    reason: format!(
                        "circular feature {} ({}) is represented by a non-circle primitive",
                        feature_id, prim_id
                    ),
                });
            }
            continue;
        };
        if !circle_like || *role != crate::DrawingPrimitiveRole::Object {
            // Construction circles are reference geometry themselves and get
            // no center presentation (ADR-028); other object shapes (e.g.
            // base profiles) are not circular either.
            continue;
        }

        let c = transform.model_to_sheet(*center);
        let r_sheet = radius * transform.scale.0;
        let mark_half = DEFAULT_CENTER_MARK_HALF_LENGTH_MM;
        let overhang = DEFAULT_CENTERLINE_OVERHANG_MM;

        let mark_id = DrawingCenterGeometryId(format!("center_mark:{}", prim_id));
        out.insert(
            mark_id,
            DrawingCenterGeometry::CenterMark(CenterMark {
                source_feature: feature_id.clone(),
                source_entity: Some(entity.clone()),
                horizontal: LayoutSegment {
                    start: DrawingPoint::new(c.x - mark_half, c.y),
                    end: DrawingPoint::new(c.x + mark_half, c.y),
                },
                vertical: LayoutSegment {
                    start: DrawingPoint::new(c.x, c.y - mark_half),
                    end: DrawingPoint::new(c.x, c.y + mark_half),
                },
            }),
        );

        for (axis, id_prefix) in [
            (CenterlineAxis::Horizontal, "h_centerline"),
            (CenterlineAxis::Vertical, "v_centerline"),
        ] {
            let segment = match axis {
                CenterlineAxis::Horizontal => LayoutSegment {
                    start: DrawingPoint::new(c.x - r_sheet - overhang, c.y),
                    end: DrawingPoint::new(c.x + r_sheet + overhang, c.y),
                },
                CenterlineAxis::Vertical => LayoutSegment {
                    start: DrawingPoint::new(c.x, c.y - r_sheet - overhang),
                    end: DrawingPoint::new(c.x, c.y + r_sheet + overhang),
                },
            };
            out.insert(
                DrawingCenterGeometryId(format!("{}:{}", id_prefix, prim_id)),
                DrawingCenterGeometry::Centerline(Centerline {
                    source_feature: feature_id.clone(),
                    source_entity: Some(entity.clone()),
                    axis,
                    segment,
                }),
            );
        }
    }
    Ok(out)
}

/// Validate the center geometry attached to a view, fail-closed.
///
/// Checks per element: finite coordinates; non-degenerate segments; the source
/// feature exists in the engineering model; when an entity is referenced it
/// belongs to that feature; and the feature still has a represented circle in
/// this view's presentation.
pub fn validate_center_geometry_integrity(
    view: &DrawingView,
    drawing_source_map: &DrawingSourceMap,
    eng_map: &EngineeringSourceMap,
    features: &BTreeMap<FeatureId, EngineeringFeature>,
) -> Result<(), DrawingError> {
    for (element_id, element) in &view.center_geometry {
        for point in element.points() {
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err(DrawingError::InvalidCenterGeometry {
                    reason: format!(
                        "center element {} contains a non-finite coordinate",
                        element_id
                    ),
                });
            }
        }
        let segments_degenerate: Vec<LayoutSegment> = match element {
            DrawingCenterGeometry::CenterMark(m) => vec![m.horizontal, m.vertical],
            DrawingCenterGeometry::Centerline(l) => vec![l.segment],
        };
        for seg in &segments_degenerate {
            let dx = seg.end.x - seg.start.x;
            let dy = seg.end.y - seg.start.y;
            if dx * dx + dy * dy <= WITNESS_EPSILON_MM * WITNESS_EPSILON_MM {
                return Err(DrawingError::InvalidCenterGeometry {
                    reason: format!("center element {} has a degenerate segment", element_id),
                });
            }
        }
        let feature = element.source_feature();
        let Some(engineering_feature) = features.get(feature) else {
            return Err(DrawingError::MissingCenterSource {
                reason: format!(
                    "center element {} sources feature {} which does not exist",
                    element_id, feature
                ),
            });
        };
        // Only circular material bases and hole/pattern features may carry
        // center presentation.
        if !matches!(
            engineering_feature,
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Circle { .. },
                ..
            } | EngineeringFeature::Hole { .. }
                | EngineeringFeature::HolePattern { .. }
        ) {
            return Err(DrawingError::InvalidCenterGeometry {
                reason: format!(
                    "center element {} sources {} which is not a circular feature",
                    element_id, feature
                ),
            });
        }
        if let Some(entity) = match element {
            DrawingCenterGeometry::CenterMark(m) => m.source_entity.clone(),
            DrawingCenterGeometry::Centerline(l) => l.source_entity.clone(),
        } {
            if !eng_map.entities_for(feature).contains(&entity) {
                return Err(DrawingError::InvalidCenterGeometry {
                    reason: format!(
                        "center element {} references entity {} which does not belong to feature {}",
                        element_id, entity, feature
                    ),
                });
            }
        }
        // The sourced feature must still be represented by a circle here.
        let represented_circle = drawing_source_map
            .primitives_for_feature(feature, eng_map)
            .iter()
            .any(|id| {
                matches!(
                    view.primitives.get(id),
                    Some(DrawingPrimitive::Circle { .. })
                )
            });
        if !represented_circle {
            return Err(DrawingError::MissingCenterSource {
                reason: format!(
                    "center element {} sources feature {} which has no represented circle",
                    element_id, feature
                ),
            });
        }
        // Cross-shape sanity for marks: both segments centred on one point.
        if let DrawingCenterGeometry::CenterMark(m) = element {
            let cross_x = (m.horizontal.start.x + m.horizontal.end.x) / 2.0;
            let cross_y = (m.vertical.start.y + m.vertical.end.y) / 2.0;
            let horizontal_flat = (m.horizontal.start.y - m.horizontal.end.y).abs();
            let vertical_flat = (m.vertical.start.x - m.vertical.end.x).abs();
            let crosses = (cross_x - m.vertical.start.x).abs() <= WITNESS_EPSILON_MM
                && (cross_y - m.horizontal.start.y).abs() <= WITNESS_EPSILON_MM;
            if !horizontal_flat.is_finite()
                || horizontal_flat > WITNESS_EPSILON_MM
                || vertical_flat > WITNESS_EPSILON_MM
                || !crosses
            {
                return Err(DrawingError::InvalidCenterGeometry {
                    reason: format!("center mark {} is not a proper cross", element_id),
                });
            }
        }
    }
    Ok(())
}
