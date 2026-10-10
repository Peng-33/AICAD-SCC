//! Phase 9.7 integration of the frozen projection and visibility artifacts.
//!
//! This module is intentionally a narrow adapter: it validates the complete
//! upstream evidence chain once, copies projected Segment/Circle values into
//! Drawing primitives, maps visibility to drawing roles, and computes only
//! sheet-space placement. It never projects, classifies, clips, splits, or
//! evaluates engineering geometry.

use std::collections::{BTreeMap, BTreeSet};

use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_engineering::feature::{DrawingProperty, DrawingPropertyKind, ProjectionMethod};
use scc_engineering::model::EngineeringModel;
use scc_projection::{
    PrincipalViewKind, ProjectedGeometry, ProjectionModel, ProjectionPrimitiveId,
};
use scc_projection_consistency::{validate_projection_consistency, ProjectionConsistencyError};
use scc_spatial::SpatialModel;
use scc_visibility::{VisibilityClass, VisibilityModel};
use thiserror::Error;

use crate::annotation::resolve_part_title;
use crate::model::{
    DrawingBounds, DrawingModel, DrawingPrimitive, DrawingPrimitiveRole, DrawingSheet, DrawingView,
    DrawingViewTransform,
};
use crate::source_map::DrawingSourceMap;
use crate::{
    DrawingError, DrawingPoint, DrawingPrimitiveId, DrawingScale, DrawingSheetId, DrawingViewId,
    DrawingViewKind,
};

/// Fixed paper-space gap between orthogonally related views.
pub const VIEW_GAP_MM: f64 = 10.0;

/// Inputs for the formal three-view drawing integration.
///
/// Engineering data is read only for drawing facts (scale, projection method,
/// and title). Geometry, spatial extent, projected coordinates, and visibility
/// all come from their already-frozen sidecar artifacts.
pub struct ThreeViewDrawingInputs<'a> {
    pub engineering: &'a EngineeringModel,
    pub spatial: &'a SpatialModel,
    pub projection: &'a ProjectionModel,
    pub visibility: &'a VisibilityModel,
}

/// Fail-closed errors local to the Phase 9.7 adapter.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ThreeViewDrawingError {
    /// Upstream PCV diagnostics are deliberately preserved unchanged.
    #[error(transparent)]
    Consistency(#[from] ProjectionConsistencyError),
    /// Existing drawing validation errors retain their established code.
    #[error(transparent)]
    Drawing(#[from] DrawingError),
    #[error("drawing projection method is missing")]
    MissingProjectionMethod,
    #[error("projected geometry is unsupported (view={view:?}, primitive={primitive})")]
    UnsupportedProjectedGeometry {
        view: DrawingViewKind,
        primitive: ProjectionPrimitiveId,
    },
    #[error("invalid three-view layout: {reason}")]
    InvalidThreeViewLayout { reason: String },
    #[error("projected drawing source is missing (view={view:?}, primitive={primitive})")]
    MissingProjectionSource {
        view: DrawingViewKind,
        primitive: ProjectionPrimitiveId,
    },
    #[error("visibility mapping is invalid (view={view:?}, primitive={primitive})")]
    InvalidVisibilityMapping {
        view: DrawingViewKind,
        primitive: ProjectionPrimitiveId,
    },
}

impl ThreeViewDrawingError {
    /// Stable code for local Phase 9.7 failures. Upstream errors expose their
    /// own PCV code through `StructuredError` conversion.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Consistency(error) => error.code(),
            Self::Drawing(error) => error.code(),
            Self::MissingProjectionMethod => "SCC-DRW-019",
            Self::UnsupportedProjectedGeometry { .. } => "SCC-DRW-020",
            Self::InvalidThreeViewLayout { .. } => "SCC-DRW-021",
            Self::MissingProjectionSource { .. } => "SCC-DRW-022",
            Self::InvalidVisibilityMapping { .. } => "SCC-DRW-023",
        }
    }
}

impl From<ThreeViewDrawingError> for StructuredError {
    fn from(error: ThreeViewDrawingError) -> Self {
        match error {
            // Do not wrap PCV: its stage/code/path are the diagnostic contract.
            ThreeViewDrawingError::Consistency(error) => error.into(),
            ThreeViewDrawingError::Drawing(error) => error.into(),
            other => StructuredError::new(other.code(), ErrorStage::Drawing, other.to_string())
                .with_recoverable(true),
        }
    }
}

impl From<ThreeViewDrawingError> for SccError {
    fn from(error: ThreeViewDrawingError) -> Self {
        SccError::Structured(Box::new(error.into()))
    }
}

/// Build the formal Front/Top/Left drawing from frozen Phase 9 artifacts.
///
/// The consistency validator is the first substantive operation and is
/// called exactly once. Everything after it is a typed, deterministic copy or
/// placement operation over validated data.
pub fn build_three_view_drawing(
    inputs: &ThreeViewDrawingInputs<'_>,
) -> Result<DrawingModel, ThreeViewDrawingError> {
    validate_projection_consistency(inputs.spatial, inputs.projection, inputs.visibility)?;

    let method = projection_method(inputs.engineering)?;
    let scale = drawing_scale(inputs.engineering)?;

    let ordered = [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ];
    let mut views = BTreeMap::new();
    let mut source_map = DrawingSourceMap::new();
    for kind in ordered {
        let projection_view = inputs.projection.views.get(&kind).ok_or_else(|| {
            ThreeViewDrawingError::InvalidThreeViewLayout {
                reason: format!("projection view {kind:?} is absent after consistency validation"),
            }
        })?;
        let drawing_kind = drawing_view_kind(kind);
        let mut primitives = BTreeMap::new();
        for (projection_id, projected) in &projection_view.primitives {
            let class = inputs
                .visibility
                .class_for(kind, projection_id)
                .ok_or_else(|| ThreeViewDrawingError::InvalidVisibilityMapping {
                    view: drawing_kind,
                    primitive: projection_id.clone(),
                })?;
            if class == VisibilityClass::Occluded {
                continue;
            }
            let role = match class {
                VisibilityClass::Visible => DrawingPrimitiveRole::Object,
                VisibilityClass::Hidden => DrawingPrimitiveRole::Hidden,
                VisibilityClass::Occluded => continue,
            };
            let drawing_id = DrawingPrimitiveId(projection_id.0.clone());
            let primitive =
                projected_primitive(projected.geometry.clone(), role).ok_or_else(|| {
                    ThreeViewDrawingError::UnsupportedProjectedGeometry {
                        view: drawing_kind,
                        primitive: projection_id.clone(),
                    }
                })?;
            primitives.insert(drawing_id.clone(), primitive);
            source_map.record_projected(drawing_id, projection_id.clone());
        }
        let model_bounds = drawing_bounds(&primitives, drawing_kind)?;
        let view_id = DrawingViewId(view_id_spelling(kind).into());
        views.insert(
            view_id.clone(),
            DrawingView {
                id: view_id,
                kind: drawing_kind,
                display_label: None,
                transform: DrawingViewTransform::identity_with_scale(scale),
                primitives,
                dimensions: BTreeMap::new(),
                dimension_layouts: BTreeMap::new(),
                center_geometry: BTreeMap::new(),
                model_bounds,
            },
        );
    }

    apply_layout(&mut views, method, scale)?;
    crate::three_view_annotations::build_three_view_annotations(
        &mut views,
        inputs.engineering,
        inputs.spatial,
        inputs.projection,
        &source_map,
    )?;
    let associative_dimensions =
        crate::associative_dimension::build_parametric_associative_dimensions(
            inputs.engineering,
            inputs.spatial,
            &mut views,
        )?;
    crate::annotation_layout::optimize_three_view_layouts(&mut views)?;
    crate::three_view_annotations::normalize_annotation_bounds(&mut views)?;

    let sheet_id = inputs
        .engineering
        .part
        .as_ref()
        .map(|part| DrawingSheetId(part.id.to_string()))
        .unwrap_or_else(|| DrawingSheetId("three_view".into()));
    let mut annotations = BTreeMap::new();
    if let Some(title) = resolve_part_title(inputs.engineering.part.as_ref())? {
        annotations.insert(title.id.clone(), title);
    }
    let mut sheets = BTreeMap::new();
    sheets.insert(
        sheet_id.clone(),
        DrawingSheet {
            id: sheet_id,
            views,
            annotations,
        },
    );
    let drawing = DrawingModel {
        sheets,
        source_map,
        associative_dimensions,
    };
    validate_three_view_integrity(&drawing, inputs.projection)?;
    crate::associative_dimension::validate_parametric_associative_dimensions(
        &drawing,
        inputs.engineering,
        inputs.spatial,
    )?;
    crate::annotation::validate_annotation_integrity(
        &drawing.sheets,
        inputs.engineering.part.as_ref(),
    )?;
    Ok(drawing)
}

fn projection_method(
    engineering: &EngineeringModel,
) -> Result<ProjectionMethod, ThreeViewDrawingError> {
    match engineering
        .drawing
        .get(DrawingPropertyKind::ProjectionMethod)
    {
        Some(DrawingProperty::ProjectionMethod { method }) => Ok(*method),
        _ => Err(ThreeViewDrawingError::MissingProjectionMethod),
    }
}

fn drawing_scale(engineering: &EngineeringModel) -> Result<DrawingScale, ThreeViewDrawingError> {
    let scale = match engineering.drawing.get(DrawingPropertyKind::Scale) {
        Some(DrawingProperty::Scale { value }) => DrawingScale(*value),
        _ => DrawingScale::default(),
    };
    if !scale.is_valid() {
        return Err(DrawingError::InvalidDrawingScale { scale: scale.0 }.into());
    }
    Ok(scale)
}

fn drawing_view_kind(kind: PrincipalViewKind) -> DrawingViewKind {
    match kind {
        PrincipalViewKind::Front => DrawingViewKind::Front,
        PrincipalViewKind::Top => DrawingViewKind::Top,
        PrincipalViewKind::Left => DrawingViewKind::Left,
    }
}

fn view_id_spelling(kind: PrincipalViewKind) -> &'static str {
    match kind {
        PrincipalViewKind::Front => "front",
        PrincipalViewKind::Top => "top",
        PrincipalViewKind::Left => "left",
    }
}

fn projected_primitive(
    geometry: ProjectedGeometry,
    role: DrawingPrimitiveRole,
) -> Option<DrawingPrimitive> {
    Some(match geometry {
        ProjectedGeometry::Segment { start, end } => DrawingPrimitive::Line {
            role,
            start: DrawingPoint::new(start.u, start.v),
            end: DrawingPoint::new(end.u, end.v),
        },
        ProjectedGeometry::Circle { center, radius } => DrawingPrimitive::Circle {
            role,
            center: DrawingPoint::new(center.u, center.v),
            radius,
        },
        ProjectedGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => DrawingPrimitive::Arc {
            role,
            center: DrawingPoint::new(center.u, center.v),
            radius,
            start_angle,
            end_angle,
        },
    })
}

fn drawing_bounds(
    primitives: &BTreeMap<DrawingPrimitiveId, DrawingPrimitive>,
    view: DrawingViewKind,
) -> Result<DrawingBounds, ThreeViewDrawingError> {
    let mut result: Option<DrawingBounds> = None;
    for primitive in primitives.values() {
        let bounds = crate::builder::primitive_bounds(primitive)?;
        if let Some(existing) = &mut result {
            existing.min_x = existing.min_x.min(bounds.min_x);
            existing.min_y = existing.min_y.min(bounds.min_y);
            existing.max_x = existing.max_x.max(bounds.max_x);
            existing.max_y = existing.max_y.max(bounds.max_y);
        } else {
            result = Some(bounds);
        }
    }
    let bounds = result.ok_or_else(|| ThreeViewDrawingError::InvalidThreeViewLayout {
        reason: format!("view {view:?} has no drawable primitives"),
    })?;
    if !bounds.is_finite() || bounds.max_x < bounds.min_x || bounds.max_y < bounds.min_y {
        return Err(ThreeViewDrawingError::InvalidThreeViewLayout {
            reason: format!("view {view:?} has invalid final model bounds"),
        });
    }
    Ok(bounds)
}

fn scaled(value: f64, scale: DrawingScale) -> Result<f64, ThreeViewDrawingError> {
    let result = value * scale.0;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(ThreeViewDrawingError::InvalidThreeViewLayout {
            reason: "model coordinate overflows after applying DrawingScale".into(),
        })
    }
}

fn finite_sum(a: f64, b: f64) -> Result<f64, ThreeViewDrawingError> {
    let value = a + b;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ThreeViewDrawingError::InvalidThreeViewLayout {
            reason: "view placement overflows sheet space".into(),
        })
    }
}

fn finite_difference(a: f64, b: f64) -> Result<f64, ThreeViewDrawingError> {
    finite_sum(a, -b)
}

fn scaled_edge(
    bounds: DrawingBounds,
    scale: DrawingScale,
) -> Result<(f64, f64, f64, f64), ThreeViewDrawingError> {
    Ok((
        scaled(bounds.min_x, scale)?,
        scaled(bounds.min_y, scale)?,
        scaled(bounds.max_x, scale)?,
        scaled(bounds.max_y, scale)?,
    ))
}

fn apply_layout(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    method: ProjectionMethod,
    scale: DrawingScale,
) -> Result<(), ThreeViewDrawingError> {
    let front_id = DrawingViewId("front".into());
    let top_id = DrawingViewId("top".into());
    let left_id = DrawingViewId("left".into());
    let front = views
        .get(&front_id)
        .ok_or_else(|| layout_error("front view missing"))?;
    let top = views
        .get(&top_id)
        .ok_or_else(|| layout_error("top view missing"))?;
    let left = views
        .get(&left_id)
        .ok_or_else(|| layout_error("left view missing"))?;
    let f = scaled_edge(front.model_bounds, scale)?;
    let t = scaled_edge(top.model_bounds, scale)?;
    let l = scaled_edge(left.model_bounds, scale)?;

    // The common-axis origins initially coincide at (0, 0). Placement moves
    // only the non-shared axis and therefore aligns Front/Top X and
    // Front/Left Y without centering bboxes or changing view geometry.
    let top_y = match method {
        ProjectionMethod::ThirdAngle => {
            finite_sum(f.3, VIEW_GAP_MM).and_then(|v| finite_difference(v, t.1))
        }
        ProjectionMethod::FirstAngle => {
            finite_sum(f.1, -VIEW_GAP_MM).and_then(|v| finite_difference(v, t.3))
        }
    }?;
    let left_x = match method {
        ProjectionMethod::ThirdAngle => {
            finite_sum(f.0, -VIEW_GAP_MM).and_then(|v| finite_difference(v, l.2))
        }
        ProjectionMethod::FirstAngle => {
            finite_sum(f.2, VIEW_GAP_MM).and_then(|v| finite_difference(v, l.0))
        }
    }?;
    set_translation(views, &front_id, DrawingPoint::new(0.0, 0.0))?;
    set_translation(views, &top_id, DrawingPoint::new(0.0, top_y))?;
    set_translation(views, &left_id, DrawingPoint::new(left_x, 0.0))?;

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for view in views.values() {
        let bounds = view.sheet_bounds();
        if !bounds.is_finite() {
            return Err(layout_error("view bounds are non-finite after placement"));
        }
        min_x = min_x.min(bounds.min_x);
        min_y = min_y.min(bounds.min_y);
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return Err(layout_error(
            "no finite view bounds available for sheet placement",
        ));
    }
    let dx = finite_sum(DrawingSheet::DEFAULT_SHEET_MARGIN_MM, -min_x)?;
    let dy = finite_sum(DrawingSheet::DEFAULT_SHEET_MARGIN_MM, -min_y)?;
    for view in views.values_mut() {
        let tx = finite_sum(view.transform.translation.x, dx)?;
        let ty = finite_sum(view.transform.translation.y, dy)?;
        view.transform = DrawingViewTransform::new(scale, DrawingPoint::new(tx, ty))?;
        if !view.sheet_bounds().is_finite() {
            return Err(layout_error("final view bounds are non-finite"));
        }
    }
    Ok(())
}

fn set_translation(
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    id: &DrawingViewId,
    translation: DrawingPoint,
) -> Result<(), ThreeViewDrawingError> {
    let view = views
        .get_mut(id)
        .ok_or_else(|| layout_error("view missing"))?;
    view.transform = DrawingViewTransform::new(view.transform.scale, translation)?;
    Ok(())
}

fn layout_error(reason: impl Into<String>) -> ThreeViewDrawingError {
    ThreeViewDrawingError::InvalidThreeViewLayout {
        reason: reason.into(),
    }
}

fn validate_three_view_integrity(
    drawing: &DrawingModel,
    projection: &ProjectionModel,
) -> Result<(), ThreeViewDrawingError> {
    let sheet = drawing
        .principal_sheet()
        .ok_or_else(|| layout_error("drawing has no sheet"))?;
    if sheet.views.len() != 3 {
        return Err(layout_error(
            "formal drawing must contain exactly three views",
        ));
    }
    let expected = [
        (DrawingViewId("front".into()), DrawingViewKind::Front),
        (DrawingViewId("top".into()), DrawingViewKind::Top),
        (DrawingViewId("left".into()), DrawingViewKind::Left),
    ];
    let mut expected_ids = BTreeSet::new();
    for (id, kind) in expected {
        expected_ids.insert(id.clone());
        let view = sheet
            .views
            .get(&id)
            .ok_or_else(|| layout_error(format!("view {kind:?} is missing")))?;
        if view.kind != kind || !view.transform.scale.is_valid() || !view.sheet_bounds().is_finite()
        {
            return Err(layout_error(format!("view {kind:?} has invalid structure")));
        }
        for (drawing_id, primitive) in &view.primitives {
            let projected_id = drawing
                .source_map
                .projected_for_primitive(drawing_id)
                .ok_or_else(|| ThreeViewDrawingError::MissingProjectionSource {
                    view: kind,
                    primitive: ProjectionPrimitiveId(drawing_id.0.clone()),
                })?;
            let projection_view = projection_view_for_kind(projection, kind);
            if !projection_view.is_some_and(|entry| entry.primitives.contains_key(projected_id)) {
                return Err(ThreeViewDrawingError::MissingProjectionSource {
                    view: kind,
                    primitive: projected_id.clone(),
                });
            }
            if !primitive_finite(primitive) {
                return Err(layout_error(format!(
                    "view {kind:?} has non-finite primitive"
                )));
            }
        }
    }
    if sheet
        .views
        .keys()
        .any(|id| !expected_ids.iter().any(|expected| expected == id))
    {
        return Err(layout_error(
            "formal drawing contains an unexpected view id",
        ));
    }
    if drawing.source_map.iter().next().is_some() {
        return Err(layout_error(
            "formal drawing contains legacy entity provenance",
        ));
    }
    for (drawing_id, projected_id) in drawing.source_map.projected_iter() {
        if drawing.locate_primitive(drawing_id).is_none() {
            return Err(ThreeViewDrawingError::MissingProjectionSource {
                view: DrawingViewKind::Front,
                primitive: projected_id.clone(),
            });
        }
    }
    Ok(())
}

fn projection_view_for_kind(
    projection: &ProjectionModel,
    kind: DrawingViewKind,
) -> Option<&scc_projection::ProjectionView> {
    let principal = match kind {
        DrawingViewKind::Front => PrincipalViewKind::Front,
        DrawingViewKind::Top => PrincipalViewKind::Top,
        DrawingViewKind::Left => PrincipalViewKind::Left,
        DrawingViewKind::Principal | DrawingViewKind::SectionAA => return None,
    };
    projection.views.get(&principal)
}

fn primitive_finite(primitive: &DrawingPrimitive) -> bool {
    match primitive {
        DrawingPrimitive::Line { start, end, .. } => {
            start.x.is_finite() && start.y.is_finite() && end.x.is_finite() && end.y.is_finite()
        }
        DrawingPrimitive::Circle { center, radius, .. } => {
            center.x.is_finite() && center.y.is_finite() && radius.is_finite() && *radius > 0.0
        }
        DrawingPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            ..
        } => {
            center.x.is_finite()
                && center.y.is_finite()
                && radius.is_finite()
                && *radius > 0.0
                && start_angle.is_finite()
                && end_angle.is_finite()
        }
        _ => false,
    }
}
