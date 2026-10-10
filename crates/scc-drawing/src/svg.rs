//! SVG drawing renderer (Phase 8.8): a deliberately "dumb" renderer.
//!
//! [`drawing_to_svg`] consumes **only** a [`DrawingModel`] and translates its
//! already-decided presentation into deterministic SVG. It never solves, never
//! lays out, never fits, never re-scales, never re-measures a dimension value,
//! and never re-evaluates visibility policy — everything was decided by the
//! builder (ADR-029):
//!
//! - the viewBox comes from [`DrawingSheet::bounds`] and nothing else;
//! - every sheet-space point converts through the one rule
//!   `svg = (x, H − y)` (sheet +Y up → SVG +Y down), so text is emitted in
//!   already-converted coordinates and can never mirror;
//! - model-space primitives convert through the view's stored
//!   [`DrawingViewTransform`] exactly once;
//! - dimension values render from `DrawingDimension.value` via
//!   [`crate::dimension::format_dimension`]; layout geometry only says *where*;
//! - styles map from `DrawingStrokeKind`; dash patterns live here (renderer
//!   territory) and never flow back into the model.
//!
//! Output is self-contained and byte-for-byte deterministic: fixed attribute
//! order, `BTreeMap` iteration order, locale-free number formatting.

use scc_geometry::svg::{escape_xml, number};

use crate::annotation::DrawingTextAnchor;
use crate::center_geometry::DrawingCenterGeometry;
use crate::dimension::format_dimension;
use crate::dimension_layout::{
    ArrowAnchor, DimensionLayoutGeometry, DrawingDimensionLayout, DEFAULT_ARROW_SIZE_MM,
};
use crate::model::{
    DrawingModel, DrawingPrimitive, DrawingPrimitiveRole, DrawingSheet, DrawingStrokeKind,
    DrawingView,
};
use crate::{DrawingError, DrawingPoint};

/// The semantic role of any primitive variant.
fn primitive_role(primitive: &DrawingPrimitive) -> &DrawingPrimitiveRole {
    match primitive {
        DrawingPrimitive::Point { role, .. }
        | DrawingPrimitive::Line { role, .. }
        | DrawingPrimitive::Circle { role, .. }
        | DrawingPrimitive::Arc { role, .. }
        | DrawingPrimitive::Rectangle { role, .. }
        | DrawingPrimitive::Polygon { role, .. } => role,
    }
}

/// Object outline stroke width, paper mm.
pub const OBJECT_STROKE_WIDTH_MM: f64 = 0.5;
/// Thin stroke width for construction/center/dimension geometry, paper mm.
pub const THIN_STROKE_WIDTH_MM: f64 = 0.25;
/// Renderer-owned dash pattern for construction geometry (SVG user units).
pub const SVG_CONSTRUCTION_DASH_PATTERN: &str = "4 2";
/// Renderer-owned dash-dot pattern for center geometry (SVG user units).
pub const SVG_CENTER_DASH_PATTERN: &str = "8 2 2 2";
/// Renderer-owned dash pattern for hidden projected geometry.
pub const SVG_HIDDEN_DASH_PATTERN: &str = "4 2";
/// Dimension text height, paper mm (independent annotation constant).
pub const DEFAULT_DIMENSION_TEXT_HEIGHT_MM: f64 = 3.5;
/// Rendered point marker radius (legacy exporter convention).
const POINT_RADIUS_SVG: f64 = 0.75;
/// Arrowhead half-width at the base, paper mm (renderer-specific geometry).
const ARROW_HALF_WIDTH_MM: f64 = 1.0;

/// Render a derived drawing model into a deterministic engineering-drawing SVG.
///
/// Fail-closed: the model is validated first (view/layout pairing, annotation
/// validity, finite bounds); any non-finite coordinate encountered during
/// emission aborts with `SCC-DRW-017` instead of emitting garbage.
pub fn drawing_to_svg(drawing: &DrawingModel) -> Result<String, DrawingError> {
    let sheet = drawing
        .principal_sheet()
        .ok_or_else(|| missing("drawing has no sheet"))?;
    let view = sheet
        .views
        .values()
        .next()
        .ok_or_else(|| invalid("sheet has no views"))?;
    // Re-run the model-side integrity gates for every view; their structured
    // errors propagate unchanged (never lumped).
    for entry in sheet.views.values() {
        crate::dimension_layout::validate_dimension_layout_integrity(entry)?;
    }
    for annotation in sheet.annotations.values() {
        if !annotation.is_valid() {
            return Err(invalid(format!("annotation {} is invalid", annotation.id)));
        }
    }
    let bounds = sheet.bounds();
    if !bounds.is_finite() || bounds.max_x <= 0.0 || bounds.max_y <= 0.0 {
        return Err(invalid("sheet bounds are not finite positive extents"));
    }
    let height = bounds.max_y;

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}mm\" height=\"{}mm\">\n",
        number(bounds.max_x),
        number(height),
        number(bounds.max_x),
        number(height)
    ));
    if sheet.views.len() == 1 && view.kind == crate::DrawingViewKind::Principal {
        // Keep the Phase 8 serialization path byte-identical.
        emit_objects(&mut out, view, height)?;
        emit_styled_group(&mut out, view, height, DrawingStrokeKind::Construction)?;
        emit_centers(&mut out, view, height, false)?;
        emit_dimensions(&mut out, view, height)?;
    } else {
        // Phase 9.7: every formal view is rendered from its stored transform;
        // the renderer does not reproject, classify, fit, or alter geometry.
        for view in sheet.views.values() {
            let name = match view.kind {
                crate::DrawingViewKind::Front => "front",
                crate::DrawingViewKind::Top => "top",
                crate::DrawingViewKind::Left => "left",
                crate::DrawingViewKind::Principal => "principal",
                crate::DrawingViewKind::SectionAA => "section_aa",
            };
            out.push_str(&format!("<g id=\"view_{name}\">\n"));
            if view
                .primitives
                .values()
                .any(|p| primitive_role(p).stroke_kind() == DrawingStrokeKind::Hatch)
            {
                emit_named_group(
                    &mut out,
                    view,
                    height,
                    DrawingStrokeKind::Hatch,
                    &format!("{name}_hatch"),
                )?;
            }
            emit_named_group(
                &mut out,
                view,
                height,
                DrawingStrokeKind::Object,
                &format!("{name}_objects"),
            )?;
            if view
                .primitives
                .values()
                .any(|p| primitive_role(p).stroke_kind() == DrawingStrokeKind::Hidden)
            {
                emit_named_group(
                    &mut out,
                    view,
                    height,
                    DrawingStrokeKind::Hidden,
                    &format!("{name}_hidden"),
                )?;
            }
            // Phase 11.1: formal views carry the same Phase 8 dimension and
            // center geometry emitters as the legacy principal view.
            emit_centers(&mut out, view, height, true)?;
            emit_dimensions(&mut out, view, height)?;
            emit_view_label(&mut out, view, height)?;
            out.push_str("</g>\n");
        }
    }
    emit_annotations(&mut out, sheet, height)?;
    out.push_str("</svg>\n");
    Ok(out)
}

fn missing(reason: impl Into<String>) -> DrawingError {
    DrawingError::InvalidSvgDrawing {
        reason: reason.into(),
    }
}

fn invalid(reason: impl Into<String>) -> DrawingError {
    DrawingError::InvalidSvgDrawing {
        reason: reason.into(),
    }
}

/// Convert a sheet-space y to SVG space (the single flip rule).
fn flip(y: f64, height: f64) -> f64 {
    height - y
}

/// Convert a whole sheet-space point to SVG space through the single
/// ADR-029 rule `svg = (x, H − y)`. Every sheet-space endpoint — both ends of
/// every segment — must pass through this one helper so the two ends can
/// never convert asymmetrically again (Phase 8.9.1 remediation of B-1).
fn sheet_to_svg_point(p: DrawingPoint, height: f64) -> DrawingPoint {
    DrawingPoint::new(p.x, flip(p.y, height))
}

fn ensure_finite(p: DrawingPoint) -> Result<(), DrawingError> {
    if p.x.is_finite() && p.y.is_finite() {
        Ok(())
    } else {
        Err(invalid(format!(
            "non-finite coordinate ({}, {}) reached the renderer",
            p.x, p.y
        )))
    }
}

/// XML-safe element id: keep a conservative charset, replace the rest.
fn sanitize(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("e{cleaned}")
}

fn segment(
    out: &mut String,
    segment: &crate::dimension_layout::LayoutSegment,
    height: f64,
    extra_style: &str,
) -> Result<(), DrawingError> {
    // Both endpoints convert through the same helper (B-1 fix: the second
    // endpoint previously skipped the Y-flip).
    let a = sheet_to_svg_point(segment.start, height);
    let b = sheet_to_svg_point(segment.end, height);
    ensure_finite(a)?;
    ensure_finite(b)?;
    out.push_str(&format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}/>\n",
        number(a.x),
        number(a.y),
        number(b.x),
        number(b.y),
        extra_style,
    ));
    Ok(())
}

fn stroke_attrs(kind: DrawingStrokeKind) -> String {
    match kind {
        DrawingStrokeKind::Object => format!(
            " stroke=\"black\" stroke-width=\"{}\"",
            number(OBJECT_STROKE_WIDTH_MM)
        ),
        DrawingStrokeKind::Construction => format!(
            " stroke=\"black\" stroke-width=\"{}\" stroke-dasharray=\"{}\"",
            number(THIN_STROKE_WIDTH_MM),
            SVG_CONSTRUCTION_DASH_PATTERN
        ),
        DrawingStrokeKind::Center => format!(
            " stroke=\"black\" stroke-width=\"{}\" stroke-dasharray=\"{}\"",
            number(THIN_STROKE_WIDTH_MM),
            SVG_CENTER_DASH_PATTERN
        ),
        DrawingStrokeKind::Hidden => format!(
            " stroke=\"black\" stroke-width=\"{}\" stroke-dasharray=\"{}\"",
            number(THIN_STROKE_WIDTH_MM),
            SVG_HIDDEN_DASH_PATTERN
        ),
        DrawingStrokeKind::Hatch => format!(
            " stroke=\"black\" stroke-width=\"{}\"",
            number(THIN_STROKE_WIDTH_MM)
        ),
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

fn emit_objects(out: &mut String, view: &DrawingView, height: f64) -> Result<(), DrawingError> {
    out.push_str(&format!(
        "<g id=\"objects\" fill=\"none\"{}>\n",
        stroke_attrs(DrawingStrokeKind::Object)
    ));
    for (id, primitive) in &view.primitives {
        if primitive_role(primitive).stroke_kind() != DrawingStrokeKind::Object {
            continue;
        }
        emit_primitive(out, id.0.as_str(), primitive, &view.transform, height)?;
    }
    out.push_str("</g>\n");
    Ok(())
}

fn emit_named_group(
    out: &mut String,
    view: &DrawingView,
    height: f64,
    kind: DrawingStrokeKind,
    group_id: &str,
) -> Result<(), DrawingError> {
    out.push_str(&format!(
        "<g id=\"{group_id}\" fill=\"none\"{}>\n",
        stroke_attrs(kind)
    ));
    for (id, primitive) in &view.primitives {
        if primitive_role(primitive).stroke_kind() == kind {
            emit_primitive(out, id.0.as_str(), primitive, &view.transform, height)?;
        }
    }
    out.push_str("</g>\n");
    Ok(())
}

fn emit_styled_group(
    out: &mut String,
    view: &DrawingView,
    height: f64,
    kind: DrawingStrokeKind,
) -> Result<(), DrawingError> {
    let group_id = match kind {
        DrawingStrokeKind::Construction => "construction",
        DrawingStrokeKind::Hidden => "hidden",
        DrawingStrokeKind::Hatch => "hatch",
        _ => return Ok(()),
    };
    out.push_str(&format!(
        "<g id=\"{group_id}\" fill=\"none\"{}>\n",
        stroke_attrs(kind)
    ));
    for (id, primitive) in &view.primitives {
        if primitive_role(primitive).stroke_kind() != kind {
            continue;
        }
        emit_primitive(out, id.0.as_str(), primitive, &view.transform, height)?;
    }
    out.push_str("</g>\n");
    Ok(())
}

fn emit_view_label(out: &mut String, view: &DrawingView, height: f64) -> Result<(), DrawingError> {
    let Some(label) = &view.display_label else {
        return Ok(());
    };
    let position = view
        .display_label_position()
        .ok_or_else(|| invalid("view label has no anchor"))?;
    let p = sheet_to_svg_point(position, height);
    ensure_finite(p)?;
    out.push_str(&format!(
        "<text id=\"label_{}\" x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"3.5\" fill=\"black\">{}</text>\n",
        sanitize(&view.id.0), number(p.x), number(p.y), escape_xml(label),
    ));
    Ok(())
}

fn emit_primitive(
    out: &mut String,
    raw_id: &str,
    primitive: &DrawingPrimitive,
    transform: &crate::DrawingViewTransform,
    height: f64,
) -> Result<(), DrawingError> {
    let element_id = sanitize(raw_id);
    let data_id = escape_xml(raw_id);
    // Convert a model-space point to SVG space through the stored transform.
    let to_svg = |p: crate::DrawingPoint| -> Result<DrawingPoint, DrawingError> {
        let sheet = transform.model_to_sheet(p);
        ensure_finite(sheet)?;
        Ok(DrawingPoint::new(sheet.x, flip(sheet.y, height)))
    };
    match primitive {
        DrawingPrimitive::Point { position, .. } => {
            let p = to_svg(*position)?;
            out.push_str(&format!(
                "<circle id=\"p_{element_id}\" data-drawing-id=\"{data_id}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"black\" stroke=\"none\"/>\n",
                number(p.x),
                number(p.y),
                number(POINT_RADIUS_SVG)
            ));
        }
        DrawingPrimitive::Line { start, end, .. } => {
            let a = to_svg(*start)?;
            let b = to_svg(*end)?;
            out.push_str(&format!(
                "<line id=\"p_{element_id}\" data-drawing-id=\"{data_id}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                number(a.x),
                number(a.y),
                number(b.x),
                number(b.y)
            ));
        }
        DrawingPrimitive::Circle { center, radius, .. } => {
            let c = to_svg(*center)?;
            let r_sheet = radius * transform.scale.0;
            ensure_finite(DrawingPoint::new(r_sheet, 0.0))?;
            out.push_str(&format!(
                "<circle id=\"p_{element_id}\" data-drawing-id=\"{data_id}\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>\n",
                number(c.x),
                number(c.y),
                number(r_sheet)
            ));
        }
        DrawingPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            ..
        } => {
            // Endpoints from the model-space angles, transformed once. The
            // Y-flip reverses orientation, so an originally counter-clockwise
            // sweep renders with sweep-flag 0.
            let tau = std::f64::consts::TAU;
            let delta = (*end_angle - *start_angle).rem_euclid(tau);
            let large = if delta > std::f64::consts::PI { 1 } else { 0 };
            let p1 = to_svg(crate::DrawingPoint::new(
                center.x + radius * start_angle.cos(),
                center.y + radius * start_angle.sin(),
            ))?;
            let p2 = to_svg(crate::DrawingPoint::new(
                center.x + radius * end_angle.cos(),
                center.y + radius * end_angle.sin(),
            ))?;
            let r_sheet = radius * transform.scale.0;
            out.push_str(&format!(
                "<path id=\"p_{element_id}\" data-drawing-id=\"{data_id}\" d=\"M {} {} A {} {} 0 {large} 0 {} {}\"/>\n",
                number(p1.x),
                number(p1.y),
                number(r_sheet),
                number(r_sheet),
                number(p2.x),
                number(p2.y)
            ));
        }
        DrawingPrimitive::Rectangle {
            origin,
            width,
            height: rect_height,
            ..
        } => {
            let a = to_svg(*origin)?;
            let b = to_svg(crate::DrawingPoint::new(
                origin.x + width,
                origin.y + rect_height,
            ))?;
            let x = a.x.min(b.x);
            let y = a.y.min(b.y);
            let w = (b.x - a.x).abs();
            let h = (b.y - a.y).abs();
            out.push_str(&format!(
                "<rect id=\"p_{element_id}\" data-drawing-id=\"{data_id}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                number(x),
                number(y),
                number(w),
                number(h)
            ));
        }
        DrawingPrimitive::Polygon { vertices, .. } => {
            let mut points = String::new();
            for vertex in vertices {
                let p = to_svg(*vertex)?;
                points.push_str(&format!("{},{} ", number(p.x), number(p.y)));
            }
            out.push_str(&format!(
                "<polygon id=\"p_{element_id}\" data-drawing-id=\"{data_id}\" points=\"{}\"/>\n",
                points.trim_end()
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Center geometry
// ---------------------------------------------------------------------------

fn emit_centers(
    out: &mut String,
    view: &DrawingView,
    height: f64,
    include_roles: bool,
) -> Result<(), DrawingError> {
    out.push_str(&format!(
        "<g id=\"centers\" fill=\"none\"{}>\n",
        stroke_attrs(DrawingStrokeKind::Center)
    ));
    for (id, element) in &view.center_geometry {
        let data_id = escape_xml(id.0.as_str());
        let hook = if include_roles {
            let role = match element {
                DrawingCenterGeometry::CenterMark(_) => "center_mark",
                DrawingCenterGeometry::Centerline(_) => "centerline",
            };
            format!(" data-drawing-id=\"{data_id}\" data-role=\"{role}\"")
        } else {
            format!(" data-drawing-id=\"{data_id}\"")
        };
        match element {
            DrawingCenterGeometry::CenterMark(mark) => {
                segment(out, &mark.horizontal, height, &hook)?;
                segment(out, &mark.vertical, height, &hook)?;
            }
            DrawingCenterGeometry::Centerline(line) => {
                segment(out, &line.segment, height, &hook)?;
            }
        }
    }
    out.push_str("</g>\n");
    Ok(())
}

// ---------------------------------------------------------------------------
// Dimensions
// ---------------------------------------------------------------------------

fn emit_dimensions(out: &mut String, view: &DrawingView, height: f64) -> Result<(), DrawingError> {
    out.push_str("<g id=\"dimensions\" fill=\"none\" stroke=\"black\">\n");
    for (dim_id, dimension) in &view.dimensions {
        let layout: &DrawingDimensionLayout = view
            .dimension_layouts
            .get(dim_id)
            .ok_or_else(|| invalid(format!("dimension {} has no layout", dim_id)))?;
        let element_id = sanitize(dim_id.0.as_str());
        let thin = format!(" stroke-width=\"{}\"", number(THIN_STROKE_WIDTH_MM));
        match &layout.geometry {
            DimensionLayoutGeometry::Linear {
                dimension_line,
                extension_lines,
                arrows,
                text_anchor,
                text_orientation,
            } => {
                for ext in extension_lines {
                    segment(out, ext, height, &thin)?;
                }
                segment(out, dimension_line, height, &thin)?;
                for arrow in arrows {
                    emit_arrow(out, arrow, &element_id, height)?;
                }
                emit_text(
                    out,
                    &element_id,
                    &format_dimension(dimension),
                    *text_anchor,
                    height,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_word(DrawingTextAnchor::Middle),
                    *text_orientation,
                )?;
            }
            DimensionLayoutGeometry::Diameter {
                axis_line,
                arrows,
                text_anchor,
                ..
            } => {
                segment(out, axis_line, height, &thin)?;
                for arrow in arrows {
                    emit_arrow(out, arrow, &element_id, height)?;
                }
                emit_text(
                    out,
                    &element_id,
                    &format_dimension(dimension),
                    *text_anchor,
                    height,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_word(DrawingTextAnchor::Start),
                    crate::dimension_layout::TextOrientation::Horizontal,
                )?;
            }
            DimensionLayoutGeometry::Radius {
                radius_line,
                arrow,
                text_anchor,
                ..
            } => {
                segment(out, radius_line, height, &thin)?;
                emit_arrow(out, arrow, &element_id, height)?;
                emit_text(
                    out,
                    &element_id,
                    &format_dimension(dimension),
                    *text_anchor,
                    height,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_word(DrawingTextAnchor::Start),
                    crate::dimension_layout::TextOrientation::Horizontal,
                )?;
            }
            DimensionLayoutGeometry::PatternCallout {
                leader,
                text_anchor,
                ..
            } => {
                segment(out, leader, height, &thin)?;
                emit_text(
                    out,
                    &element_id,
                    &format_dimension(dimension),
                    *text_anchor,
                    height,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_word(DrawingTextAnchor::Start),
                    crate::dimension_layout::TextOrientation::Horizontal,
                )?;
            }
        }
    }
    out.push_str("</g>\n");
    Ok(())
}

/// Deterministic filled-triangle arrowhead from an anchor (tip + inward
/// direction). Pure renderer geometry — never written back to the model.
/// All three emitted vertices are sheet-space POINTS and convert through the
/// single ADR-029 flip rule `svg = (x, H − y)`; `direction` is only used to
/// build sheet-space base/wing offsets and never converts as a point.
fn emit_arrow(
    out: &mut String,
    arrow: &ArrowAnchor,
    element_id: &str,
    height: f64,
) -> Result<(), DrawingError> {
    ensure_finite(arrow.point)?;
    let length = DEFAULT_ARROW_SIZE_MM;
    let half_width = ARROW_HALF_WIDTH_MM;
    let d = arrow.direction;
    let d_len = (d.x * d.x + d.y * d.y).sqrt();
    if d_len <= 0.0 {
        return Err(invalid("arrow direction is degenerate"));
    }
    let (ux, uy) = (d.x / d_len, d.y / d_len);
    let tip = arrow.point;
    let base = (tip.x - ux * length, tip.y - uy * length);
    let p1 = DrawingPoint::new(tip.x, tip.y);
    let p2 = DrawingPoint::new(base.0 - uy * half_width, base.1 + ux * half_width);
    let p3 = DrawingPoint::new(base.0 + uy * half_width, base.1 - ux * half_width);
    // Phase 8.9.2: all three vertices are sheet-space POINTS; each converts
    // through the one ADR-029 rule. `direction` never converts as a point.
    let p1 = sheet_to_svg_point(p1, height);
    let p2 = sheet_to_svg_point(p2, height);
    let p3 = sheet_to_svg_point(p3, height);
    out.push_str(&format!(
        "<polygon id=\"a_{element_id}\" points=\"{},{} {},{} {},{}\" fill=\"black\" stroke=\"none\"/>\n",
        number(p1.x),
        number(p1.y),
        number(p2.x),
        number(p2.y),
        number(p3.x),
        number(p3.y)
    ));
    Ok(())
}

fn anchor_word(anchor: DrawingTextAnchor) -> &'static str {
    match anchor {
        DrawingTextAnchor::Start => "start",
        DrawingTextAnchor::Middle => "middle",
        DrawingTextAnchor::End => "end",
    }
}

/// Emit one `<text>` element in already-flipped coordinates (never mirrored).
#[allow(clippy::too_many_arguments)]
fn emit_text(
    out: &mut String,
    element_id: &str,
    content: &str,
    anchor: DrawingPoint,
    height: f64,
    text_height_mm: f64,
    svg_anchor: &'static str,
    orientation: crate::dimension_layout::TextOrientation,
) -> Result<(), DrawingError> {
    ensure_finite(anchor)?;
    let escaped = escape_xml(content);
    let rotate = match orientation {
        crate::dimension_layout::TextOrientation::Horizontal => String::new(),
        crate::dimension_layout::TextOrientation::Vertical => format!(
            " transform=\"rotate(-90 {} {})\"",
            number(anchor.x),
            number(flip(anchor.y, height))
        ),
    };
    out.push_str(&format!(
        "<text id=\"t_{element_id}\" x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"black\" stroke=\"none\" text-anchor=\"{svg_anchor}\"{rotate}>{escaped}</text>\n",
        number(anchor.x),
        number(flip(anchor.y, height)),
        number(text_height_mm)
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

fn emit_annotations(
    out: &mut String,
    sheet: &DrawingSheet,
    height: f64,
) -> Result<(), DrawingError> {
    out.push_str("<g id=\"annotations\" fill=\"black\" font-family=\"sans-serif\">\n");
    for (id, annotation) in &sheet.annotations {
        ensure_finite(annotation.position)?;
        let element_id = sanitize(id.0.as_str());
        emit_text(
            out,
            &element_id,
            &annotation.text,
            annotation.position,
            height,
            annotation.text_height_mm,
            anchor_word(annotation.anchor),
            crate::dimension_layout::TextOrientation::Horizontal,
        )?;
    }
    out.push_str("</g>\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_is_deterministic_and_xml_safe() {
        assert_eq!(sanitize("plate"), "eplate");
        assert_eq!(sanitize("bolt_pattern_array[3]"), "ebolt_pattern_array_3_");
        assert_eq!(sanitize("center_mark:x"), "ecenter_mark_x");
    }

    #[test]
    fn number_formatting_edges() {
        assert_eq!(number(120.0), "120");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(10.5), "10.5");
    }

    /// Render one sheet-space segment and pull back the four SVG endpoint
    /// coordinates (Phase 8.9.1: both endpoints must flip, never just one).
    fn render_segment(start: (f64, f64), end: (f64, f64)) -> (f64, f64, f64, f64) {
        let seg = crate::dimension_layout::LayoutSegment {
            start: DrawingPoint::new(start.0, start.1),
            end: DrawingPoint::new(end.0, end.1),
        };
        let mut out = String::new();
        segment(&mut out, &seg, 100.0, "").unwrap();
        // Fixed emission shape: <line x1="A" y1="B" x2="C" y2="D"/>
        let attrs = out
            .strip_prefix("<line ")
            .and_then(|s| s.strip_suffix(">\n"))
            .unwrap();
        let get = |key: &str| -> f64 {
            let rest = attrs.split(key).nth(1).unwrap();
            let rest = rest.strip_prefix("=\"").unwrap();
            let raw: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            raw.parse().unwrap()
        };
        (get("x1"), get("y1"), get("x2"), get("y2"))
    }

    #[test]
    fn horizontal_sheet_segment_stays_horizontal_in_svg() {
        // Sheet (20,25)→(30,25), H=100 → both endpoints y = 75.
        let (x1, y1, x2, y2) = render_segment((20.0, 25.0), (30.0, 25.0));
        assert_eq!((x1, y1, x2, y2), (20.0, 75.0, 30.0, 75.0));
        assert_eq!(y1, y2, "horizontal sheet segment must stay horizontal");
    }

    #[test]
    fn vertical_sheet_segment_stays_vertical_in_svg() {
        // Sheet (25,20)→(25,30), H=100 → (25,80)→(25,70).
        let (x1, y1, x2, y2) = render_segment((25.0, 20.0), (25.0, 30.0));
        assert_eq!((x1, y1, x2, y2), (25.0, 80.0, 25.0, 70.0));
        assert_eq!(x1, x2, "vertical sheet segment must stay vertical");
    }

    #[test]
    fn diagonal_sheet_segment_flips_both_endpoints_independently() {
        // Sheet (10,20)→(30,40), H=100 → (10,80)→(30,60).
        assert_eq!(
            render_segment((10.0, 20.0), (30.0, 40.0)),
            (10.0, 80.0, 30.0, 60.0)
        );
    }

    #[test]
    fn segment_length_is_preserved_by_the_y_mirror() {
        for (a, b) in [
            ((20.0_f64, 25.0), (30.0, 25.0)), // horizontal
            ((25.0, 20.0), (25.0, 30.0)),     // vertical
            ((10.0, 20.0), (30.0, 40.0)),     // diagonal
        ] {
            let sheet_len = ((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1)).sqrt();
            let (x1, y1, x2, y2) = render_segment(a, b);
            let svg_len = ((x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1)).sqrt();
            assert!((sheet_len - svg_len).abs() < 1.0e-9);
        }
    }

    /// Render one sheet-space arrowhead anchor and pull back the three SVG
    /// polygon vertices (Phase 8.9.2: every vertex must flip, never just the
    /// raw sheet coordinates).
    fn render_arrow(tip: (f64, f64), dir: (f64, f64)) -> [(f64, f64); 3] {
        let arrow = ArrowAnchor {
            point: DrawingPoint::new(tip.0, tip.1),
            direction: DrawingPoint::new(dir.0, dir.1),
        };
        let mut out = String::new();
        emit_arrow(&mut out, &arrow, "arrow_probe", 100.0).unwrap();
        // Fixed emission shape:
        // <polygon id="a_arrow_probe" points="X1,Y1 X2,Y2 X3,Y3" .../>
        let attrs = out
            .strip_prefix("<polygon ")
            .and_then(|s| s.strip_suffix(">\n"))
            .unwrap();
        let rest = attrs.split("points=\"").nth(1).unwrap();
        let pts = rest.split('"').next().unwrap();
        pts.split_whitespace()
            .map(|pair| {
                let mut it = pair.split(',');
                let x: f64 = it.next().unwrap().parse().unwrap();
                let y: f64 = it.next().unwrap().parse().unwrap();
                (x, y)
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    #[test]
    fn horizontal_arrow_flips_all_three_vertices() {
        // Sheet tip (20,25), inward dir (+x): base at x=17.5, wings at y=26/24.
        // H=100 → SVG (20,75),(17.5,74),(17.5,76): the winding flips with Y.
        let verts = render_arrow((20.0, 25.0), (1.0, 0.0));
        assert_eq!(verts, [(20.0, 75.0), (17.5, 74.0), (17.5, 76.0)]);
    }

    #[test]
    fn vertical_arrow_flips_all_three_vertices() {
        // Sheet tip (25,20), inward dir (+y): base at y=17.5, wings at x=24/26.
        // H=100 → SVG (25,80),(24,82.5),(26,82.5): the whole triangle moves
        // past the mirror line instead of staying in raw sheet space.
        let verts = render_arrow((25.0, 20.0), (0.0, 1.0));
        assert_eq!(verts, [(25.0, 80.0), (24.0, 82.5), (26.0, 82.5)]);
    }

    #[test]
    fn diagonal_arrow_vertices_flip_through_the_one_rule() {
        // Sheet tip (30,30), inward 45° dir; every expected coordinate is built
        // from SQRT_2 expressions — never pasted decimal literals.
        let c = std::f64::consts::SQRT_2 / 2.0;
        let verts = render_arrow((30.0, 30.0), (c, c));
        let close = |a: f64, b: f64| (a - b).abs() < 1.0e-9;
        // Tip flips exactly.
        assert_eq!(verts[0].0, 30.0);
        assert!(close(verts[0].1, 70.0));
        // Sheet-space p2 = (30−3.5c, 30−1.5c), p3 = (30−1.5c, 30−3.5c);
        // flipped through H=100.
        assert!(close(verts[1].0, 30.0 - 3.5 * c));
        assert!(close(verts[1].1, 100.0 - (30.0 - 1.5 * c)));
        assert!(close(verts[2].0, 30.0 - 1.5 * c));
        assert!(close(verts[2].1, 100.0 - (30.0 - 3.5 * c)));
    }

    #[test]
    fn arrow_tip_preservation_across_orientations() {
        // The first emitted vertex is the flipped anchor point itself,
        // exactly — for horizontal, vertical, and diagonal arrows alike.
        for (tip, dir) in [
            ((20.0_f64, 25.0), (1.0, 0.0)),
            ((25.0, 20.0), (0.0, 1.0)),
            (
                (30.0, 30.0),
                (
                    std::f64::consts::SQRT_2 / 2.0,
                    std::f64::consts::SQRT_2 / 2.0,
                ),
            ),
        ] {
            let verts = render_arrow(tip, dir);
            let want = sheet_to_svg_point(DrawingPoint::new(tip.0, tip.1), 100.0);
            assert_eq!(verts[0].0, want.x);
            assert_eq!(verts[0].1, want.y);
        }
    }

    #[test]
    fn arrow_triangle_edge_lengths_survive_the_mirror() {
        // The Y-flip reverses winding but preserves all pairwise distances:
        // tip→wing edges are √7.25, the base edge is 2·half-width = 2.
        for (tip, dir) in [
            ((20.0_f64, 25.0), (1.0, 0.0)),
            ((25.0, 20.0), (0.0, 1.0)),
            (
                (30.0, 30.0),
                (
                    std::f64::consts::SQRT_2 / 2.0,
                    std::f64::consts::SQRT_2 / 2.0,
                ),
            ),
        ] {
            let svg = render_arrow(tip, dir);
            // Sheet-space triple rebuilt from the same anchor geometry.
            let d_len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
            let (ux, uy) = (dir.0 / d_len, dir.1 / d_len);
            let base = (
                tip.0 - ux * DEFAULT_ARROW_SIZE_MM,
                tip.1 - uy * DEFAULT_ARROW_SIZE_MM,
            );
            let s1 = (tip.0, tip.1);
            let s2 = (
                base.0 - uy * ARROW_HALF_WIDTH_MM,
                base.1 + ux * ARROW_HALF_WIDTH_MM,
            );
            let s3 = (
                base.0 + uy * ARROW_HALF_WIDTH_MM,
                base.1 - ux * ARROW_HALF_WIDTH_MM,
            );
            let dist =
                |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
            for (sv, sh) in [
                ((svg[0], svg[1]), (s1, s2)),
                ((svg[0], svg[2]), (s1, s3)),
                ((svg[1], svg[2]), (s2, s3)),
            ] {
                let svg_d = dist(sv.0, sv.1);
                let sheet_d = dist(sh.0, sh.1);
                assert!((svg_d - sheet_d).abs() < 1.0e-9);
            }
            assert!((dist(svg[0], svg[1]) - 7.25_f64.sqrt()).abs() < 1.0e-9);
            assert!((dist(svg[0], svg[2]) - 7.25_f64.sqrt()).abs() < 1.0e-9);
            assert!((dist(svg[1], svg[2]) - 2.0).abs() < 1.0e-9);
        }
    }

    /// Render one `<text>` element through the private emitter and return
    /// the raw SVG line (Phase 8.10.1: paint isolation must ride on every
    /// emitted text, never inherited from a group-level stroke).
    fn render_text(
        content: &str,
        svg_anchor: &'static str,
        orientation: crate::dimension_layout::TextOrientation,
    ) -> String {
        let mut out = String::new();
        emit_text(
            &mut out,
            "text_probe",
            content,
            DrawingPoint::new(40.0, 30.0),
            100.0,
            DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
            svg_anchor,
            orientation,
        )
        .unwrap();
        out
    }

    #[test]
    fn emit_text_carries_paint_isolation() {
        let out = render_text(
            "42",
            "middle",
            crate::dimension_layout::TextOrientation::Horizontal,
        );
        // The text itself opts out of any inherited group stroke ...
        assert!(
            out.contains("stroke=\"none\""),
            "emitted text must carry stroke=\"none\" paint isolation"
        );
        // ... while keeping its own fill and typography attributes.
        assert!(out.contains("fill=\"black\""));
        assert!(out.contains("font-family=\"sans-serif\""));
        assert!(out.contains(&format!(
            "font-size=\"{}\"",
            number(DEFAULT_DIMENSION_TEXT_HEIGHT_MM)
        )));
        assert!(out.contains("text-anchor=\"middle\""));
        // Exactly one text element carrying exactly one isolation attribute.
        assert_eq!(out.matches("<text").count(), 1);
        assert_eq!(out.matches("stroke=\"none\"").count(), 1);
    }

    #[test]
    fn emit_text_preserves_geometry_attributes() {
        // Anchor sheet point (40,30) with H=100 flips to SVG y=70; the fixed
        // emission shape must survive the paint change byte-for-byte.
        let out = render_text(
            "42",
            "middle",
            crate::dimension_layout::TextOrientation::Horizontal,
        );
        assert_eq!(
            out,
            "<text id=\"t_text_probe\" x=\"40\" y=\"70\" font-family=\"sans-serif\" font-size=\"3.5\" fill=\"black\" stroke=\"none\" text-anchor=\"middle\">42</text>\n"
        );
    }

    #[test]
    fn emit_text_vertical_rotation_and_escape_unchanged() {
        // Vertical orientation keeps its rotate transform and it coexists
        // with the paint isolation; XML-special content still escapes.
        let out = render_text(
            "<W>&H",
            "start",
            crate::dimension_layout::TextOrientation::Vertical,
        );
        assert!(out.contains("transform=\"rotate(-90 40 70)\""));
        assert!(out.contains("stroke=\"none\""));
        assert!(out.contains(">&lt;W&gt;&amp;H</text>"));
        // The rotation pivot stays on the flipped anchor point.
        assert!(out.contains("x=\"40\" y=\"70\""));
    }
}
