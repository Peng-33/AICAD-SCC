//! DXF drawing renderer (Phase 8.9): the second dumb renderer over
//! [`DrawingModel`], sibling of [`crate::svg::drawing_to_svg`].
//!
//! Like the SVG renderer it never solves, lays out, fits, re-scales,
//! re-measures dimension values, or re-evaluates visibility — everything was
//! decided by the builder (ADR-030). Differences are purely syntactic:
//!
//! - **Coordinates**: DXF is Y-up Cartesian like Sheet Space, so
//!   `dxf = (sheet_x, sheet_y)` — the SVG Y-flip is *not* copied. Arcs pass
//!   their model angles through as degrees (native CCW semantics), with no
//!   sweep inversion.
//! - **Units**: `1 DXF drawing unit = 1 sheet mm`, declared via
//!   `$INSUNITS = 4`; extents come from [`DrawingSheet::bounds`] only.
//! - **Layers**: renderer-owned presentation routing (`AICAD_OBJECT`,
//!   `AICAD_CONSTRUCTION`, `AICAD_CENTER`, `AICAD_DIMENSION`,
//!   `AICAD_ANNOTATION`) — never reverse-engineered into engineering
//!   features.
//! - **Linetypes**: `CONTINUOUS`, `AICAD_DASHED`, `AICAD_CENTERLINE` are
//!   declared in TABLES with paper-mm patterns; every referenced name is
//!   defined.
//! - **Native DIMENSION entities**: resolved, witnessed linear/diameter/radius
//!   associations are emitted with anonymous graphics blocks sourced from the
//!   existing layout. Legacy and composite callouts remain visual-only.
//! - **Text**: single-line `TEXT`, UTF-8 (AC1027 encodes UTF-8); control
//!   characters are stripped defensively; XML escaping is never applied.

use scc_geometry::dxf::{
    arc_entity, circle_entity, closed_polyline_entity, group, line_entity, point_entity,
};
use scc_geometry::{svg::number, Point2};

use crate::annotation::DrawingTextAnchor;
use crate::associative_dimension::{AssociativeDimensionStatus, AssociativeDimensionUnit};
use crate::center_geometry::DrawingCenterGeometry;
use crate::dimension::format_dimension;
use crate::dimension::{
    DimensionValue, DrawingDimension, DrawingDimensionKind, ResolvedDimensionValue,
};
use crate::dimension_layout::{
    ArrowAnchor, DimensionLayoutGeometry, DrawingDimensionLayout, DEFAULT_ARROW_SIZE_MM,
    DEFAULT_EXTENSION_GAP_MM, DEFAULT_EXTENSION_OVERHANG_MM,
};
use crate::model::{DrawingModel, DrawingPrimitive, DrawingSheet, DrawingStrokeKind, DrawingView};
use crate::DrawingViewId;
use crate::{DrawingError, DrawingPoint, DEFAULT_DIMENSION_TEXT_HEIGHT_MM};

/// Object layer.
pub const LAYER_OBJECT: &str = "AICAD_OBJECT";
/// Construction layer.
pub const LAYER_CONSTRUCTION: &str = "AICAD_CONSTRUCTION";
/// Center layer.
pub const LAYER_CENTER: &str = "AICAD_CENTER";
/// Dimension layer.
pub const LAYER_DIMENSION: &str = "AICAD_DIMENSION";
/// Annotation layer.
pub const LAYER_ANNOTATION: &str = "AICAD_ANNOTATION";
/// Hidden projected geometry layer. It is emitted only when used.
pub const LAYER_HIDDEN: &str = "AICAD_HIDDEN";
pub const LAYER_HATCH: &str = "AICAD_HATCH";

/// Continuous linetype name (implicit default, declared explicitly).
pub const LTYPE_CONTINUOUS: &str = "CONTINUOUS";
/// Dashed linetype for construction geometry.
pub const LTYPE_DASHED: &str = "AICAD_DASHED";
/// Center linetype (long-short pattern) for center geometry.
pub const LTYPE_CENTERLINE: &str = "AICAD_CENTERLINE";

/// Render a derived drawing model into deterministic engineering DXF.
///
/// Consumes only the [`DrawingModel`]; fail-closed before any output is
/// produced if the drawing fails its integrity gates.
pub fn drawing_to_dxf(drawing: &DrawingModel) -> Result<String, DrawingError> {
    let sheet: &DrawingSheet = drawing
        .principal_sheet()
        .ok_or_else(|| invalid("drawing has no sheet"))?;
    let view: &DrawingView = sheet
        .views
        .values()
        .next()
        .ok_or_else(|| invalid("sheet has no views"))?;
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

    // Pre-render containment check: every emitted element must live inside
    // the sheet envelope (consistency error otherwise — never auto-grow).
    let within = |p: DrawingPoint| -> Result<(), DrawingError> {
        let eps = crate::dimension_layout::WITNESS_EPSILON_MM;
        if p.x.is_finite()
            && p.y.is_finite()
            && p.x >= bounds.min_x - eps
            && p.x <= bounds.max_x + eps
            && p.y >= bounds.min_y - eps
            && p.y <= bounds.max_y + eps
        {
            Ok(())
        } else {
            Err(invalid(format!(
                "element coordinate ({}, {}) lies outside the sheet bounds",
                p.x, p.y
            )))
        }
    };
    for view in sheet.views.values() {
        for primitive in view.primitives.values() {
            for p in primitive_model_points(primitive) {
                within(transform_point(&view.transform, p)?)?;
            }
        }
        for layout in view.dimension_layouts.values() {
            for p in layout.geometry.points() {
                within(*p)?;
            }
        }
        for element in view.center_geometry.values() {
            for p in element.points() {
                within(*p)?;
            }
        }
    }

    let native_dimensions = collect_native_dimensions(drawing)?;
    let mut out = String::new();
    let has_hidden = sheet.views.values().any(|entry| {
        entry
            .primitives
            .values()
            .any(|primitive| primitive_role(primitive).stroke_kind() == DrawingStrokeKind::Hidden)
    });
    let has_hatch = sheet.views.values().any(|entry| {
        entry
            .primitives
            .values()
            .any(|primitive| primitive_role(primitive).stroke_kind() == DrawingStrokeKind::Hatch)
    });
    emit_header(&mut out, bounds, !native_dimensions.is_empty());
    emit_tables(&mut out, has_hidden, has_hatch, &native_dimensions);
    emit_native_blocks(&mut out, &native_dimensions)?;
    out.push_str("0\nSECTION\n2\nENTITIES\n");
    if sheet.views.len() == 1 && view.kind == crate::DrawingViewKind::Principal {
        // Keep the Phase 8 entity ordering byte-identical.
        emit_primitives(&mut out, view, DrawingStrokeKind::Object)?;
        emit_primitives(&mut out, view, DrawingStrokeKind::Construction)?;
        emit_centers(&mut out, view)?;
        emit_dimensions(&mut out, view, &native_dimensions)?;
    } else {
        for entry in sheet.views.values() {
            emit_primitives(&mut out, entry, DrawingStrokeKind::Hatch)?;
            emit_primitives(&mut out, entry, DrawingStrokeKind::Object)?;
            emit_primitives(&mut out, entry, DrawingStrokeKind::Hidden)?;
            // Phase 11.1: dimensions and center geometry use the existing
            // Phase 8 DXF semantic emitters in each formal view.
            emit_centers(&mut out, entry)?;
            emit_dimensions(&mut out, entry, &native_dimensions)?;
            emit_view_label(&mut out, entry)?;
        }
    }
    emit_annotations(&mut out, sheet)?;
    out.push_str("0\nENDSEC\n0\nEOF\n");
    Ok(out)
}

/// Convert a shared low-level writer result into a renderer error.
fn dx(result: Result<(), scc_geometry::GeometryError>) -> Result<(), DrawingError> {
    result.map_err(Into::into)
}

fn invalid(reason: impl Into<String>) -> DrawingError {
    DrawingError::InvalidDxfDrawing {
        reason: reason.into(),
    }
}

fn transform_point(
    transform: &crate::DrawingViewTransform,
    p: DrawingPoint,
) -> Result<DrawingPoint, DrawingError> {
    let sheet = transform.model_to_sheet(p);
    if sheet.x.is_finite() && sheet.y.is_finite() {
        Ok(sheet)
    } else {
        Err(invalid("non-finite coordinate reached the renderer"))
    }
}

fn primitive_model_points(primitive: &DrawingPrimitive) -> Vec<DrawingPoint> {
    match primitive {
        DrawingPrimitive::Point { position, .. } => vec![*position],
        DrawingPrimitive::Line { start, end, .. } => vec![*start, *end],
        DrawingPrimitive::Circle { center, .. } => vec![*center],
        DrawingPrimitive::Arc { center, .. } => vec![*center],
        DrawingPrimitive::Rectangle {
            origin,
            width,
            height,
            ..
        } => vec![
            *origin,
            DrawingPoint::new(origin.x + width, origin.y),
            DrawingPoint::new(origin.x + width, origin.y + height),
            DrawingPoint::new(origin.x, origin.y + height),
        ],
        DrawingPrimitive::Polygon { vertices, .. } => vertices.clone(),
    }
}

// ---------------------------------------------------------------------------
// Header / tables
// ---------------------------------------------------------------------------

fn emit_header(out: &mut String, bounds: crate::model::DrawingBounds, has_native_dimensions: bool) {
    let _ = group(out, 0, "SECTION");
    let _ = group(out, 2, "HEADER");
    let _ = group(out, 9, "$ACADVER");
    let _ = group(out, 1, "AC1027");
    let _ = group(out, 9, "$INSUNITS");
    let _ = group(out, 70, "4"); // millimeters
    if has_native_dimensions {
        let _ = group(out, 9, "$DIMLUNIT");
        let _ = group(out, 70, "2"); // decimal
    }
    let _ = group(out, 9, "$HANDSEED");
    let _ = group(out, 5, "FFFFFF");
    let _ = group(out, 9, "$EXTMIN");
    let _ = group(out, 10, &number(bounds.min_x));
    let _ = group(out, 20, &number(bounds.min_y));
    let _ = group(out, 30, "0");
    let _ = group(out, 9, "$EXTMAX");
    let _ = group(out, 10, &number(bounds.max_x));
    let _ = group(out, 20, &number(bounds.max_y));
    let _ = group(out, 30, "0");
    let _ = group(out, 0, "ENDSEC");
}

/// One dash-pattern linetype declaration: name, description, total length,
/// and its dash elements (positive = dash, negative = gap, paper mm).
const LINETYPES: [(&str, &str, f64, &[f64]); 3] = [
    (LTYPE_CONTINUOUS, "Solid line", 0.0, &[]),
    (LTYPE_CENTERLINE, "__ __ __", 14.0, &[8.0, -2.0, 2.0, -2.0]),
    (LTYPE_DASHED, "__ __", 6.0, &[4.0, -2.0]),
];

const LAYERS: [(&str, &str); 5] = [
    (LAYER_ANNOTATION, LTYPE_CONTINUOUS),
    (LAYER_CENTER, LTYPE_CENTERLINE),
    (LAYER_CONSTRUCTION, LTYPE_DASHED),
    (LAYER_DIMENSION, LTYPE_CONTINUOUS),
    (LAYER_OBJECT, LTYPE_CONTINUOUS),
];

fn emit_tables(
    out: &mut String,
    has_hidden: bool,
    has_hatch: bool,
    native_dimensions: &[NativeDimensionRecord],
) {
    let _ = group(out, 0, "SECTION");
    let _ = group(out, 2, "TABLES");

    let _ = group(out, 0, "TABLE");
    let _ = group(out, 2, "LTYPE");
    let _ = group(out, 5, "1200");
    let _ = group(out, 70, &(LINETYPES.len() as u32).to_string());
    for (name, description, total, dashes) in LINETYPES {
        let _ = group(out, 0, "LTYPE");
        let _ = group(out, 2, name);
        let _ = group(out, 70, "0");
        let _ = group(out, 3, description);
        let _ = group(out, 72, "65"); // A: always aligned text
        let _ = group(out, 73, &dashes.len().to_string());
        let _ = group(out, 40, &number(total));
        for dash in dashes {
            let _ = group(out, 49, &number(*dash));
            let _ = group(out, 74, "0");
        }
    }
    let _ = group(out, 0, "ENDTAB");

    let _ = group(out, 0, "TABLE");
    let _ = group(out, 2, "LAYER");
    let _ = group(out, 5, "1201");
    let layer_count = LAYERS.len() + usize::from(has_hidden) + usize::from(has_hatch);
    let _ = group(out, 70, &(layer_count as u32).to_string());
    for (name, linetype) in LAYERS {
        let _ = group(out, 0, "LAYER");
        let _ = group(out, 2, name);
        let _ = group(out, 70, "0");
        let _ = group(out, 62, "7");
        let _ = group(out, 6, linetype);
    }
    if has_hidden {
        let _ = group(out, 0, "LAYER");
        let _ = group(out, 2, LAYER_HIDDEN);
        let _ = group(out, 70, "0");
        let _ = group(out, 62, "7");
        let _ = group(out, 6, LTYPE_DASHED);
    }
    if has_hatch {
        let _ = group(out, 0, "LAYER");
        let _ = group(out, 2, LAYER_HATCH);
        let _ = group(out, 70, "0");
        let _ = group(out, 62, "7");
        let _ = group(out, 6, LTYPE_CONTINUOUS);
    }
    let _ = group(out, 0, "ENDTAB");

    if !native_dimensions.is_empty() {
        emit_block_record_table(out, native_dimensions);
        emit_dimension_style_table(out);
    }
    let _ = group(out, 0, "ENDSEC");
}

fn emit_block_record_table(out: &mut String, dimensions: &[NativeDimensionRecord]) {
    let _ = group(out, 0, "TABLE");
    let _ = group(out, 2, "BLOCK_RECORD");
    let _ = group(out, 5, "1202");
    let _ = group(out, 70, &(dimensions.len() as u32 + 2).to_string());
    for (name, handle) in [("*Model_Space", "1000"), ("*Paper_Space", "1001")] {
        emit_block_record(out, name, handle);
    }
    for dimension in dimensions {
        emit_block_record(out, &dimension.block_name, &dimension.block_record_handle);
    }
    let _ = group(out, 0, "ENDTAB");
}

fn emit_block_record(out: &mut String, name: &str, handle: &str) {
    let _ = group(out, 0, "BLOCK_RECORD");
    let _ = group(out, 5, handle);
    let _ = group(out, 100, "AcDbSymbolTableRecord");
    let _ = group(out, 100, "AcDbBlockTableRecord");
    let _ = group(out, 2, name);
    let _ = group(out, 70, "0");
}

fn emit_dimension_style_table(out: &mut String) {
    let _ = group(out, 0, "TABLE");
    let _ = group(out, 2, "STYLE");
    let _ = group(out, 5, "1203");
    let _ = group(out, 70, "1");
    let _ = group(out, 0, "STYLE");
    let _ = group(out, 5, "1101");
    let _ = group(out, 100, "AcDbSymbolTableRecord");
    let _ = group(out, 100, "AcDbTextStyleTableRecord");
    let _ = group(out, 2, "STANDARD");
    let _ = group(out, 70, "0");
    let _ = group(out, 40, "0");
    let _ = group(out, 41, "1");
    let _ = group(out, 50, "0");
    let _ = group(out, 71, "0");
    let _ = group(out, 42, &number(DEFAULT_DIMENSION_TEXT_HEIGHT_MM));
    let _ = group(out, 3, "txt");
    let _ = group(out, 4, "");
    let _ = group(out, 0, "ENDTAB");

    let _ = group(out, 0, "TABLE");
    let _ = group(out, 2, "DIMSTYLE");
    let _ = group(out, 5, "1204");
    let _ = group(out, 70, "1");
    let _ = group(out, 0, "DIMSTYLE");
    let _ = group(out, 105, "1100");
    let _ = group(out, 100, "AcDbSymbolTableRecord");
    let _ = group(out, 100, "AcDbDimStyleTableRecord");
    let _ = group(out, 2, NATIVE_DIMSTYLE);
    let _ = group(out, 70, "0");
    let _ = group(out, 3, "<>");
    let _ = group(out, 4, "");
    let _ = group(out, 40, "1");
    let _ = group(out, 41, &number(DEFAULT_ARROW_SIZE_MM));
    let _ = group(out, 42, &number(DEFAULT_EXTENSION_GAP_MM));
    let _ = group(out, 44, &number(DEFAULT_EXTENSION_OVERHANG_MM));
    let _ = group(out, 140, &number(DEFAULT_DIMENSION_TEXT_HEIGHT_MM));
    let _ = group(out, 144, "1");
    let _ = group(out, 147, &number(DEFAULT_DIMENSION_TEXT_HEIGHT_MM * 0.5));
    let _ = group(out, 170, "0");
    let _ = group(out, 171, "2");
    let _ = group(out, 172, "0");
    let _ = group(out, 173, "0");
    let _ = group(out, 174, "0");
    let _ = group(out, 175, "0");
    let _ = group(out, 177, "1");
    let _ = group(out, 178, "0");
    let _ = group(out, 179, "2");
    let _ = group(out, 271, "2");
    let _ = group(out, 277, "2");
    let _ = group(out, 280, "0");
    let _ = group(out, 281, "0");
    let _ = group(out, 282, "0");
    let _ = group(out, 283, "0");
    let _ = group(out, 284, "0");
    let _ = group(out, 285, "0");
    let _ = group(out, 286, "0");
    let _ = group(out, 288, "0");
    let _ = group(out, 289, "3");
    let _ = group(out, 340, "1101");
    let _ = group(out, 0, "ENDTAB");
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

fn layer_for(kind: DrawingStrokeKind) -> &'static str {
    match kind {
        DrawingStrokeKind::Object => LAYER_OBJECT,
        DrawingStrokeKind::Construction => LAYER_CONSTRUCTION,
        DrawingStrokeKind::Center => LAYER_CENTER,
        DrawingStrokeKind::Hidden => LAYER_HIDDEN,
        DrawingStrokeKind::Hatch => LAYER_HATCH,
    }
}

fn emit_primitive_entity(
    out: &mut String,
    primitive: &DrawingPrimitive,
    transform: &crate::DrawingViewTransform,
    kind: DrawingStrokeKind,
) -> Result<(), DrawingError> {
    let layer = layer_for(kind);
    let to_sheet = |p: DrawingPoint| transform_point(transform, p);
    match primitive {
        DrawingPrimitive::Point { position, .. } => {
            let p = to_sheet(*position)?;
            dx(point_entity(out, p.x, p.y, layer))
        }
        DrawingPrimitive::Line { start, end, .. } => {
            let a = to_sheet(*start)?;
            let b = to_sheet(*end)?;
            dx(line_entity(out, a.x, a.y, b.x, b.y, layer))
        }
        DrawingPrimitive::Circle { center, radius, .. } => {
            let c = to_sheet(*center)?;
            dx(circle_entity(
                out,
                c.x,
                c.y,
                radius * transform.scale.0,
                layer,
            ))
        }
        DrawingPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            ..
        } => {
            // DXF arcs are natively CCW in Y-up degrees — the model angles
            // pass through unchanged (no SVG-style sweep inversion).
            let c = to_sheet(*center)?;
            dx(arc_entity(
                out,
                c.x,
                c.y,
                radius * transform.scale.0,
                start_angle.to_degrees(),
                end_angle.to_degrees(),
                LAYER_OBJECT,
            ))
        }
        DrawingPrimitive::Rectangle {
            origin,
            width,
            height,
            ..
        } => {
            let a = to_sheet(*origin)?;
            let b = to_sheet(DrawingPoint::new(origin.x + width, origin.y + height))?;
            dx(closed_polyline_entity(
                out,
                &[
                    Point2 { x: a.x, y: a.y },
                    Point2 { x: b.x, y: a.y },
                    Point2 { x: b.x, y: b.y },
                    Point2 { x: a.x, y: b.y },
                ],
                LAYER_OBJECT,
            ))
        }
        DrawingPrimitive::Polygon { vertices, .. } => {
            let mut pts = Vec::with_capacity(vertices.len());
            for v in vertices {
                let p = to_sheet(*v)?;
                pts.push(Point2 { x: p.x, y: p.y });
            }
            dx(closed_polyline_entity(out, &pts, layer))
        }
    }
}

fn emit_primitives(
    out: &mut String,
    view: &DrawingView,
    kind: DrawingStrokeKind,
) -> Result<(), DrawingError> {
    for primitive in view.primitives.values() {
        if primitive_role(primitive).stroke_kind() == kind {
            emit_primitive_entity(out, primitive, &view.transform, kind)?;
        }
    }
    Ok(())
}

fn primitive_role(primitive: &DrawingPrimitive) -> &crate::DrawingPrimitiveRole {
    match primitive {
        DrawingPrimitive::Point { role, .. }
        | DrawingPrimitive::Line { role, .. }
        | DrawingPrimitive::Circle { role, .. }
        | DrawingPrimitive::Arc { role, .. }
        | DrawingPrimitive::Rectangle { role, .. }
        | DrawingPrimitive::Polygon { role, .. } => role,
    }
}

fn emit_centers(out: &mut String, view: &DrawingView) -> Result<(), DrawingError> {
    for element in view.center_geometry.values() {
        match element {
            DrawingCenterGeometry::CenterMark(mark) => {
                emit_center_segment(out, &mark.horizontal)?;
                emit_center_segment(out, &mark.vertical)?;
            }
            DrawingCenterGeometry::Centerline(line) => {
                emit_center_segment(out, &line.segment)?;
            }
        }
    }
    Ok(())
}

fn emit_center_segment(
    out: &mut String,
    segment: &crate::dimension_layout::LayoutSegment,
) -> Result<(), DrawingError> {
    dx(line_entity(
        out,
        segment.start.x,
        segment.start.y,
        segment.end.x,
        segment.end.y,
        LAYER_CENTER,
    ))
}

const NATIVE_DIMSTYLE: &str = "AICAD_MM";

#[derive(Debug, Clone)]
struct NativeDimensionRecord {
    view: DrawingViewId,
    dimension: DrawingDimension,
    layout: DrawingDimensionLayout,
    block_name: String,
    block_record_handle: String,
    block_handle: String,
    end_block_handle: String,
    dimension_handle: String,
    first_graphic_handle: u32,
    drawing_measurement_mm: f64,
    text: String,
}

fn collect_native_dimensions(
    drawing: &DrawingModel,
) -> Result<Vec<NativeDimensionRecord>, DrawingError> {
    let mut result = Vec::new();
    for (index, association) in drawing.associative_dimensions.values().enumerate() {
        if association.status != AssociativeDimensionStatus::Resolved {
            continue;
        }
        let Some(dimension_id) = &association.drawing_dimension else {
            return Err(invalid(
                "resolved associative dimension has no DrawingDimension id",
            ));
        };
        let view = drawing
            .sheets
            .values()
            .flat_map(|sheet| sheet.views.values())
            .find(|view| view.id == association.view)
            .ok_or_else(|| invalid("resolved associative dimension references a missing view"))?;
        let dimension = view.dimensions.get(dimension_id).ok_or_else(|| {
            invalid("resolved associative dimension references a missing DrawingDimension")
        })?;
        if dimension.id != *dimension_id || dimension.kind != association.kind {
            return Err(invalid(
                "associative dimension semantics do not match its DrawingDimension",
            ));
        }
        let layout = view
            .dimension_layouts
            .get(dimension_id)
            .ok_or_else(|| invalid("resolved associative dimension has no legal layout"))?;
        // Composite callouts remain on the existing visual path. Only simple
        // dimensions whose exact measured length is independently witnessed
        // can become native DIMENSION entities.
        let actual_mm = match (&dimension.kind, &dimension.value, &layout.geometry) {
            (
                DrawingDimensionKind::Linear { .. },
                DimensionValue::Measured {
                    resolved: ResolvedDimensionValue::LengthMm { mm },
                    ..
                },
                DimensionLayoutGeometry::Linear { .. },
            )
            | (
                DrawingDimensionKind::Diameter | DrawingDimensionKind::OverallDiameter,
                DimensionValue::Measured {
                    resolved: ResolvedDimensionValue::LengthMm { mm },
                    ..
                },
                DimensionLayoutGeometry::Diameter { .. },
            )
            | (
                DrawingDimensionKind::Radius,
                DimensionValue::Measured {
                    resolved: ResolvedDimensionValue::LengthMm { mm },
                    ..
                },
                DimensionLayoutGeometry::Radius { .. },
            ) => *mm,
            _ => continue,
        };
        if !actual_mm.is_finite() || actual_mm <= 0.0 {
            return Err(invalid(
                "native associative dimension value must be finite and positive",
            ));
        }
        if association.unit != AssociativeDimensionUnit::Millimetres
            || association.nominal != Some(ResolvedDimensionValue::LengthMm { mm: actual_mm })
        {
            return Err(invalid(
                "native DXF dimensions require a matching millimetre nominal value",
            ));
        }
        let witness = association.witness.as_ref().ok_or_else(|| {
            invalid("native associative dimension has no validated geometry witness")
        })?;
        if witness
            .drawing_measurement_mm
            .is_none_or(|observed| (observed - actual_mm).abs() > witness.tolerance_mm)
        {
            return Err(invalid(
                "native associative dimension has no matching Drawing observation",
            ));
        }
        let drawing_measurement_mm = native_drawing_measurement(layout)?;
        let expected_drawing_measurement_mm = actual_mm * view.transform.scale.0;
        if !drawing_measurement_mm.is_finite()
            || drawing_measurement_mm <= 0.0
            || (drawing_measurement_mm - expected_drawing_measurement_mm).abs()
                > witness.tolerance_mm * view.transform.scale.0
        {
            return Err(invalid(
                "native DXF definition points disagree with the engineering value and view scale",
            ));
        }
        let base = 0x2000_u32 + (index as u32) * 0x100;
        result.push(NativeDimensionRecord {
            view: view.id.clone(),
            dimension: dimension.clone(),
            layout: layout.clone(),
            block_name: format!("*D{}", result.len() + 1),
            block_record_handle: format!("{:X}", base),
            block_handle: format!("{:X}", base + 1),
            end_block_handle: format!("{:X}", base + 2),
            dimension_handle: format!("{:X}", base + 3),
            first_graphic_handle: base + 4,
            drawing_measurement_mm,
            text: format_dimension(dimension),
        });
    }
    Ok(result)
}

fn native_drawing_measurement(layout: &DrawingDimensionLayout) -> Result<f64, DrawingError> {
    let distance = |a: DrawingPoint, b: DrawingPoint| (a.x - b.x).hypot(a.y - b.y);
    match &layout.geometry {
        DimensionLayoutGeometry::Linear { arrows, .. } => {
            Ok(distance(arrows[0].point, arrows[1].point))
        }
        DimensionLayoutGeometry::Diameter { arrows, .. } => {
            Ok(distance(arrows[0].point, arrows[1].point))
        }
        DimensionLayoutGeometry::Radius { center, arrow, .. } => Ok(distance(*center, arrow.point)),
        DimensionLayoutGeometry::PatternCallout { .. } => Err(invalid(
            "pattern callouts cannot be serialized as native measured dimensions",
        )),
    }
}

fn emit_native_blocks(
    out: &mut String,
    dimensions: &[NativeDimensionRecord],
) -> Result<(), DrawingError> {
    if dimensions.is_empty() {
        return Ok(());
    }
    let _ = group(out, 0, "SECTION");
    let _ = group(out, 2, "BLOCKS");
    for (name, record_handle, block_handle, end_handle) in [
        ("*Model_Space", "1000", "1010", "1011"),
        ("*Paper_Space", "1001", "1012", "1013"),
    ] {
        let _ = group(out, 0, "BLOCK");
        let _ = group(out, 5, block_handle);
        let _ = group(out, 330, record_handle);
        let _ = group(out, 100, "AcDbEntity");
        let _ = group(out, 8, "0");
        let _ = group(out, 100, "AcDbBlockBegin");
        let _ = group(out, 2, name);
        let _ = group(out, 70, "0");
        let _ = group(out, 10, "0");
        let _ = group(out, 20, "0");
        let _ = group(out, 30, "0");
        let _ = group(out, 3, name);
        let _ = group(out, 1, "");
        let _ = group(out, 0, "ENDBLK");
        let _ = group(out, 5, end_handle);
        let _ = group(out, 330, record_handle);
        let _ = group(out, 100, "AcDbEntity");
        let _ = group(out, 8, "0");
        let _ = group(out, 100, "AcDbBlockEnd");
    }
    for record in dimensions {
        let _ = group(out, 0, "BLOCK");
        let _ = group(out, 5, &record.block_handle);
        let _ = group(out, 330, &record.block_record_handle);
        let _ = group(out, 100, "AcDbEntity");
        let _ = group(out, 8, "0");
        let _ = group(out, 100, "AcDbBlockBegin");
        let _ = group(out, 2, &record.block_name);
        let _ = group(out, 70, "1"); // anonymous, dimension-generated block
        let _ = group(out, 10, "0");
        let _ = group(out, 20, "0");
        let _ = group(out, 30, "0");
        let _ = group(out, 3, &record.block_name);
        let _ = group(out, 1, "");
        emit_dimension_block_graphics(out, record)?;
        let _ = group(out, 0, "ENDBLK");
        let _ = group(out, 5, &record.end_block_handle);
        let _ = group(out, 330, &record.block_record_handle);
        let _ = group(out, 100, "AcDbEntity");
        let _ = group(out, 8, "0");
        let _ = group(out, 100, "AcDbBlockEnd");
    }
    let _ = group(out, 0, "ENDSEC");
    Ok(())
}

fn emit_dimension_block_graphics(
    out: &mut String,
    record: &NativeDimensionRecord,
) -> Result<(), DrawingError> {
    let mut handle = record.first_graphic_handle;
    match &record.layout.geometry {
        DimensionLayoutGeometry::Linear {
            dimension_line,
            extension_lines,
            arrows,
            text_anchor,
            text_orientation,
        } => {
            for extension in extension_lines {
                emit_native_block_line(out, record, &mut handle, extension.start, extension.end);
            }
            emit_native_block_line(
                out,
                record,
                &mut handle,
                dimension_line.start,
                dimension_line.end,
            );
            for anchor in arrows {
                emit_native_block_arrow(out, record, &mut handle, anchor)?;
            }
            emit_native_block_text(
                out,
                record,
                &mut handle,
                *text_anchor,
                matches!(
                    text_orientation,
                    crate::dimension_layout::TextOrientation::Vertical
                ),
            )?;
        }
        DimensionLayoutGeometry::Diameter {
            axis_line,
            arrows,
            text_anchor,
            ..
        } => {
            emit_native_block_line(out, record, &mut handle, axis_line.start, axis_line.end);
            for anchor in arrows {
                emit_native_block_arrow(out, record, &mut handle, anchor)?;
            }
            emit_native_block_text(out, record, &mut handle, *text_anchor, false)?;
        }
        DimensionLayoutGeometry::Radius {
            radius_line,
            arrow,
            text_anchor,
            ..
        } => {
            emit_native_block_line(out, record, &mut handle, radius_line.start, radius_line.end);
            emit_native_block_arrow(out, record, &mut handle, arrow)?;
            emit_native_block_text(out, record, &mut handle, *text_anchor, false)?;
        }
        DimensionLayoutGeometry::PatternCallout { .. } => {
            return Err(invalid(
                "native DIMENSION blocks cannot render composite callouts",
            ));
        }
    }
    Ok(())
}

fn emit_native_block_line(
    out: &mut String,
    record: &NativeDimensionRecord,
    handle: &mut u32,
    start: DrawingPoint,
    end: DrawingPoint,
) {
    let current = format!("{:X}", *handle);
    *handle += 1;
    let _ = group(out, 0, "LINE");
    let _ = group(out, 5, &current);
    let _ = group(out, 330, &record.block_record_handle);
    let _ = group(out, 100, "AcDbEntity");
    let _ = group(out, 8, LAYER_DIMENSION);
    let _ = group(out, 100, "AcDbLine");
    let _ = group(out, 10, &number(start.x));
    let _ = group(out, 20, &number(start.y));
    let _ = group(out, 30, "0");
    let _ = group(out, 11, &number(end.x));
    let _ = group(out, 21, &number(end.y));
    let _ = group(out, 31, "0");
}

fn emit_native_block_arrow(
    out: &mut String,
    record: &NativeDimensionRecord,
    handle: &mut u32,
    anchor: &ArrowAnchor,
) -> Result<(), DrawingError> {
    let d = anchor.direction;
    let len = d.x.hypot(d.y);
    if !anchor.point.x.is_finite() || !anchor.point.y.is_finite() || len <= 0.0 {
        return Err(invalid("native DXF arrow anchor is invalid"));
    }
    let (ux, uy) = (d.x / len, d.y / len);
    let base = (
        anchor.point.x - ux * DEFAULT_ARROW_SIZE_MM,
        anchor.point.y - uy * DEFAULT_ARROW_SIZE_MM,
    );
    let half = 1.0;
    let current = format!("{:X}", *handle);
    *handle += 1;
    let _ = group(out, 0, "SOLID");
    let _ = group(out, 5, &current);
    let _ = group(out, 330, &record.block_record_handle);
    let _ = group(out, 100, "AcDbEntity");
    let _ = group(out, 8, LAYER_DIMENSION);
    let _ = group(out, 100, "AcDbTrace");
    for (code, value) in [
        (10, (anchor.point.x, anchor.point.y)),
        (11, (base.0 - uy * half, base.1 + ux * half)),
        (12, (base.0 + uy * half, base.1 - ux * half)),
        (13, (anchor.point.x, anchor.point.y)),
    ] {
        let _ = group(out, code, &number(value.0));
        let _ = group(out, code + 10, &number(value.1));
        let _ = group(out, code + 20, "0");
    }
    Ok(())
}

fn emit_native_block_text(
    out: &mut String,
    record: &NativeDimensionRecord,
    handle: &mut u32,
    anchor: DrawingPoint,
    vertical: bool,
) -> Result<(), DrawingError> {
    if !anchor.x.is_finite() || !anchor.y.is_finite() {
        return Err(invalid("native DXF text anchor is invalid"));
    }
    let current = format!("{:X}", *handle);
    *handle += 1;
    let _ = group(out, 0, "TEXT");
    let _ = group(out, 5, &current);
    let _ = group(out, 330, &record.block_record_handle);
    let _ = group(out, 100, "AcDbEntity");
    let _ = group(out, 8, LAYER_DIMENSION);
    let _ = group(out, 100, "AcDbText");
    let _ = group(out, 10, &number(anchor.x));
    let _ = group(out, 20, &number(anchor.y));
    let _ = group(out, 30, "0");
    let _ = group(out, 40, &number(DEFAULT_DIMENSION_TEXT_HEIGHT_MM));
    let _ = group(out, 1, &sanitize_dxf_text(&record.text));
    let _ = group(out, 50, if vertical { "90" } else { "0" });
    let _ = group(out, 72, "1");
    let _ = group(out, 11, &number(anchor.x));
    let _ = group(out, 21, &number(anchor.y));
    let _ = group(out, 31, "0");
    Ok(())
}

fn emit_native_dimension_entity(
    out: &mut String,
    record: &NativeDimensionRecord,
) -> Result<(), DrawingError> {
    let sheet_owner = "1000";
    let text_anchor = dimension_text_anchor(&record.layout)?;
    let (definition, type_flag) = match (&record.dimension.kind, &record.layout.geometry) {
        (
            DrawingDimensionKind::Linear { .. },
            DimensionLayoutGeometry::Linear { dimension_line, .. },
        ) => (
            DrawingPoint::new(
                (dimension_line.start.x + dimension_line.end.x) / 2.0,
                (dimension_line.start.y + dimension_line.end.y) / 2.0,
            ),
            0_u16,
        ),
        (
            DrawingDimensionKind::Diameter | DrawingDimensionKind::OverallDiameter,
            DimensionLayoutGeometry::Diameter { arrows, .. },
        ) => (arrows[1].point, 3_u16),
        (DrawingDimensionKind::Radius, DimensionLayoutGeometry::Radius { center, .. }) => {
            (*center, 4_u16)
        }
        _ => {
            return Err(invalid(
                "native dimension kind and layout geometry disagree",
            ))
        }
    };

    let _ = group(out, 0, "DIMENSION");
    let _ = group(out, 5, &record.dimension_handle);
    let _ = group(out, 330, sheet_owner);
    let _ = group(out, 100, "AcDbEntity");
    let _ = group(out, 8, LAYER_DIMENSION);
    let _ = group(out, 100, "AcDbDimension");
    let _ = group(out, 280, "0");
    let _ = group(out, 2, &record.block_name);
    let _ = group(out, 10, &number(definition.x));
    let _ = group(out, 20, &number(definition.y));
    let _ = group(out, 30, "0");
    let _ = group(out, 11, &number(text_anchor.x));
    let _ = group(out, 21, &number(text_anchor.y));
    let _ = group(out, 31, "0");
    let _ = group(out, 70, &(type_flag + 32 + 128).to_string());
    let _ = group(out, 1, &sanitize_dxf_text(&record.text));
    let _ = group(out, 42, &number(record.drawing_measurement_mm));
    let _ = group(out, 3, NATIVE_DIMSTYLE);
    let _ = group(out, 71, "5");
    let _ = group(out, 72, "1");
    let _ = group(out, 210, "0");
    let _ = group(out, 220, "0");
    let _ = group(out, 230, "1");

    match (&record.dimension.kind, &record.layout.geometry) {
        (
            DrawingDimensionKind::Linear { .. },
            DimensionLayoutGeometry::Linear {
                dimension_line,
                arrows,
                ..
            },
        ) => {
            let angle = (dimension_line.end.y - dimension_line.start.y)
                .atan2(dimension_line.end.x - dimension_line.start.x)
                .to_degrees();
            let _ = group(out, 100, "AcDbRotatedDimension");
            let _ = group(out, 50, &number(angle));
            emit_point_group(out, 13, arrows[0].point)?;
            emit_point_group(out, 14, arrows[1].point)?;
        }
        (
            DrawingDimensionKind::Diameter | DrawingDimensionKind::OverallDiameter,
            DimensionLayoutGeometry::Diameter { arrows, .. },
        ) => {
            let _ = group(out, 100, "AcDbDiametricDimension");
            emit_point_group(out, 15, arrows[0].point)?;
            let _ = group(out, 40, &number(record.drawing_measurement_mm / 2.0));
        }
        (DrawingDimensionKind::Radius, DimensionLayoutGeometry::Radius { arrow, .. }) => {
            let _ = group(out, 100, "AcDbRadialDimension");
            emit_point_group(out, 15, arrow.point)?;
            let _ = group(out, 40, &number(record.drawing_measurement_mm));
        }
        _ => {
            return Err(invalid(
                "native dimension kind and layout geometry disagree",
            ))
        }
    }
    Ok(())
}
fn emit_point_group(out: &mut String, code: u16, point: DrawingPoint) -> Result<(), DrawingError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(invalid("native DXF definition point is non-finite"));
    }
    let _ = group(out, code, &number(point.x));
    let _ = group(out, code + 10, &number(point.y));
    let _ = group(out, code + 20, "0");
    Ok(())
}

fn dimension_text_anchor(layout: &DrawingDimensionLayout) -> Result<DrawingPoint, DrawingError> {
    match layout.geometry {
        DimensionLayoutGeometry::Linear { text_anchor, .. }
        | DimensionLayoutGeometry::Diameter { text_anchor, .. }
        | DimensionLayoutGeometry::Radius { text_anchor, .. }
        | DimensionLayoutGeometry::PatternCallout { text_anchor, .. } => Ok(text_anchor),
    }
}

fn emit_dimensions(
    out: &mut String,
    view: &DrawingView,
    native_dimensions: &[NativeDimensionRecord],
) -> Result<(), DrawingError> {
    for (dim_id, dimension) in &view.dimensions {
        if let Some(native) = native_dimensions
            .iter()
            .find(|candidate| candidate.view == view.id && candidate.dimension.id == *dim_id)
        {
            emit_native_dimension_entity(out, native)?;
            continue;
        }
        let layout = view
            .dimension_layouts
            .get(dim_id)
            .ok_or_else(|| invalid(format!("dimension {} has no layout", dim_id)))?;
        let text = format_dimension(dimension);
        match &layout.geometry {
            DimensionLayoutGeometry::Linear {
                dimension_line,
                extension_lines,
                arrows,
                text_anchor,
                text_orientation,
            } => {
                for ext in extension_lines {
                    dx(line_entity(
                        out,
                        ext.start.x,
                        ext.start.y,
                        ext.end.x,
                        ext.end.y,
                        LAYER_DIMENSION,
                    ))?;
                }
                dx(line_entity(
                    out,
                    dimension_line.start.x,
                    dimension_line.start.y,
                    dimension_line.end.x,
                    dimension_line.end.y,
                    LAYER_DIMENSION,
                ))?;
                for arrow in arrows {
                    emit_arrow(out, arrow)?;
                }
                emit_text(
                    out,
                    LAYER_DIMENSION,
                    &text,
                    *text_anchor,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_alignment(DrawingTextAnchor::Middle),
                    matches!(
                        text_orientation,
                        crate::dimension_layout::TextOrientation::Vertical
                    ),
                )?;
            }
            DimensionLayoutGeometry::Diameter {
                axis_line,
                arrows,
                text_anchor,
                ..
            } => {
                dx(line_entity(
                    out,
                    axis_line.start.x,
                    axis_line.start.y,
                    axis_line.end.x,
                    axis_line.end.y,
                    LAYER_DIMENSION,
                ))?;
                for arrow in arrows {
                    emit_arrow(out, arrow)?;
                }
                emit_text(
                    out,
                    LAYER_DIMENSION,
                    &text,
                    *text_anchor,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_alignment(DrawingTextAnchor::Start),
                    false,
                )?;
            }
            DimensionLayoutGeometry::Radius {
                radius_line,
                arrow,
                text_anchor,
                ..
            } => {
                dx(line_entity(
                    out,
                    radius_line.start.x,
                    radius_line.start.y,
                    radius_line.end.x,
                    radius_line.end.y,
                    LAYER_DIMENSION,
                ))?;
                emit_arrow(out, arrow)?;
                emit_text(
                    out,
                    LAYER_DIMENSION,
                    &text,
                    *text_anchor,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_alignment(DrawingTextAnchor::Start),
                    false,
                )?;
            }
            DimensionLayoutGeometry::PatternCallout {
                leader,
                text_anchor,
                ..
            } => {
                dx(line_entity(
                    out,
                    leader.start.x,
                    leader.start.y,
                    leader.end.x,
                    leader.end.y,
                    LAYER_DIMENSION,
                ))?;
                emit_text(
                    out,
                    LAYER_DIMENSION,
                    &text,
                    *text_anchor,
                    DEFAULT_DIMENSION_TEXT_HEIGHT_MM,
                    anchor_alignment(DrawingTextAnchor::Start),
                    false,
                )?;
            }
        }
    }
    Ok(())
}

/// Part-title annotations as `TEXT` on the annotation layer.
fn emit_view_label(out: &mut String, view: &DrawingView) -> Result<(), DrawingError> {
    let Some(label) = &view.display_label else {
        return Ok(());
    };
    let position = view
        .display_label_position()
        .ok_or_else(|| invalid("view label has no anchor"))?;
    emit_text(
        out,
        LAYER_ANNOTATION,
        label,
        position,
        3.5,
        anchor_alignment(DrawingTextAnchor::Start),
        false,
    )
}

fn emit_annotations(out: &mut String, sheet: &DrawingSheet) -> Result<(), DrawingError> {
    for annotation in sheet.annotations.values() {
        emit_text(
            out,
            LAYER_ANNOTATION,
            &annotation.text,
            annotation.position,
            annotation.text_height_mm,
            anchor_alignment(annotation.anchor),
            false,
        )?;
    }
    Ok(())
}

/// Filled-triangle arrowhead emitted as a `SOLID` entity (tip, base corners).
fn emit_arrow(out: &mut String, arrow: &ArrowAnchor) -> Result<(), DrawingError> {
    if !arrow.point.x.is_finite() || !arrow.point.y.is_finite() {
        return Err(invalid("non-finite arrow anchor"));
    }
    let d = arrow.direction;
    let len = (d.x * d.x + d.y * d.y).sqrt();
    if len <= 0.0 {
        return Err(invalid("degenerate arrow direction"));
    }
    let (ux, uy) = (d.x / len, d.y / len);
    let tip = arrow.point;
    let base = (
        tip.x - ux * DEFAULT_ARROW_SIZE_MM,
        tip.y - uy * DEFAULT_ARROW_SIZE_MM,
    );
    let half = 1.0_f64;
    let _ = group(out, 0, "SOLID");
    let _ = group(out, 100, "AcDbEntity");
    let _ = group(out, 8, LAYER_DIMENSION);
    let _ = group(out, 100, "AcDbTrace");
    let _ = group(out, 10, &number(tip.x));
    let _ = group(out, 20, &number(tip.y));
    let _ = group(out, 30, "0");
    let _ = group(out, 11, &number(base.0 - uy * half));
    let _ = group(out, 21, &number(base.1 + ux * half));
    let _ = group(out, 31, "0");
    let _ = group(out, 12, &number(base.0 + uy * half));
    let _ = group(out, 22, &number(base.1 - ux * half));
    let _ = group(out, 32, "0");
    Ok(())
}

/// DXF horizontal-justification code for a domain anchor.
fn anchor_alignment(anchor: DrawingTextAnchor) -> u16 {
    match anchor {
        DrawingTextAnchor::Start => 0,
        DrawingTextAnchor::Middle => 1,
        DrawingTextAnchor::End => 2,
    }
}

/// Single-line `TEXT` on the annotation/dimension layers.
///
/// Vertical orientation uses rotation **90° CCW** — native to the Y-up DXF
/// plane (never the SVG `-90`, which belongs to the flipped SVG frame).
#[allow(clippy::too_many_arguments)]
fn emit_text(
    out: &mut String,
    layer: &str,
    content: &str,
    anchor: DrawingPoint,
    height_mm: f64,
    hjust: u16,
    vertical: bool,
) -> Result<(), DrawingError> {
    if !anchor.x.is_finite() || !anchor.y.is_finite() {
        return Err(invalid("non-finite text anchor"));
    }
    let rotation = if vertical { 90.0 } else { 0.0 };
    let sanitized = sanitize_dxf_text(content);
    let _ = group(out, 0, "TEXT");
    let _ = group(out, 100, "AcDbEntity");
    let _ = group(out, 8, layer);
    let _ = group(out, 100, "AcDbText");
    let _ = group(out, 10, &number(anchor.x));
    let _ = group(out, 20, &number(anchor.y));
    let _ = group(out, 30, "0");
    let _ = group(out, 40, &number(height_mm));
    let _ = group(out, 1, &sanitized);
    let _ = group(out, 50, &number(rotation));
    if hjust != 0 {
        let _ = group(out, 72, &hjust.to_string());
        // Second alignment point required when justification ≠ left.
        let _ = group(out, 11, &number(anchor.x));
        let _ = group(out, 21, &number(anchor.y));
        let _ = group(out, 31, "0");
    }
    Ok(())
}

/// Dimension text lives on the dimension layer; annotation text overrides via
/// the caller's own emission path. Kept as a function to make that explicit.
/// Strip ASCII control characters (including CR/LF) that would corrupt the
/// group-code structure. Legitimate Unicode passes through untouched.
fn sanitize_dxf_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod native_dimension_tests {
    use super::*;
    use crate::associative_dimension::{
        AssociativeDimension, AssociativeDimensionReference, DimensionGeometryWitness,
        DimensionObservability, DimensionValueProvenance,
    };
    use crate::dimension::{
        DimensionAxis3D, DimensionValue, DimensionValueSource, DrawingDimensionId,
        DrawingDimensionTarget, LinearAxis, ParametricDimensionParameter, ResolvedDimensionValue,
    };
    use crate::dimension_layout::LayoutSegment;
    use crate::model::{DrawingBounds, DrawingPrimitiveRole};
    use crate::{DrawingPrimitiveId, DrawingSheetId, DrawingViewKind};
    use std::collections::BTreeMap;

    const MODEL_DIGEST: &str = "0123456789abcdef";

    fn measured(
        id: &str,
        kind: DrawingDimensionKind,
        value_mm: f64,
        source: DimensionValueSource,
        target: DrawingDimensionTarget,
    ) -> DrawingDimension {
        DrawingDimension {
            id: DrawingDimensionId(id.into()),
            target,
            kind,
            specification: None,
            value: DimensionValue::Measured {
                source,
                resolved: ResolvedDimensionValue::LengthMm { mm: value_mm },
            },
        }
    }

    fn association(
        dimension: &DrawingDimension,
        reference: AssociativeDimensionReference,
        value_mm: f64,
    ) -> AssociativeDimension {
        AssociativeDimension {
            annotation_id: dimension.id.0.clone(),
            drawing_dimension: Some(dimension.id.clone()),
            kind: dimension.kind.clone(),
            reference,
            view: DrawingViewId("front".into()),
            nominal: Some(ResolvedDimensionValue::LengthMm { mm: value_mm }),
            unit: AssociativeDimensionUnit::Millimetres,
            sources: dimension.sources().into_iter().cloned().collect(),
            provenance: DimensionValueProvenance::GeometryDerived,
            witness: Some(DimensionGeometryWitness {
                model_sha256: MODEL_DIGEST.into(),
                evidence_ids: vec!["face:canonical:fixture".into()],
                geometry_measurement_mm: value_mm,
                drawing_measurement_mm: Some(value_mm),
                component_measurements_mm: BTreeMap::new(),
                tolerance_mm: 1.0e-5,
            }),
            observability: DimensionObservability::Observable,
            status: AssociativeDimensionStatus::Resolved,
            diagnostic: None,
        }
    }

    fn fixture() -> DrawingModel {
        let mut dimensions = BTreeMap::new();
        let mut layouts = BTreeMap::new();
        let mut associations = BTreeMap::new();

        let linear_id = DrawingDimensionId("fixture.linear".into());
        let linear = measured(
            &linear_id.0,
            DrawingDimensionKind::Linear {
                axis: LinearAxis::Horizontal,
            },
            80.0,
            DimensionValueSource::VerifiedBodyGeometry {
                model_sha256: MODEL_DIGEST.into(),
                axis: DimensionAxis3D::X,
            },
            DrawingDimensionTarget::Body,
        );
        layouts.insert(
            linear_id.clone(),
            DrawingDimensionLayout {
                dimension: linear_id.clone(),
                geometry: DimensionLayoutGeometry::Linear {
                    dimension_line: LayoutSegment {
                        start: DrawingPoint::new(20.0, 82.0),
                        end: DrawingPoint::new(100.0, 82.0),
                    },
                    extension_lines: [
                        LayoutSegment {
                            start: DrawingPoint::new(20.0, 20.0),
                            end: DrawingPoint::new(20.0, 82.0),
                        },
                        LayoutSegment {
                            start: DrawingPoint::new(100.0, 20.0),
                            end: DrawingPoint::new(100.0, 82.0),
                        },
                    ],
                    arrows: [
                        ArrowAnchor {
                            point: DrawingPoint::new(20.0, 82.0),
                            direction: DrawingPoint::new(1.0, 0.0),
                        },
                        ArrowAnchor {
                            point: DrawingPoint::new(100.0, 82.0),
                            direction: DrawingPoint::new(-1.0, 0.0),
                        },
                    ],
                    text_anchor: DrawingPoint::new(60.0, 84.0),
                    text_orientation: crate::dimension_layout::TextOrientation::Horizontal,
                },
            },
        );
        associations.insert(
            linear_id.clone(),
            association(
                &linear,
                AssociativeDimensionReference::BodyEnvelope {
                    axis: DimensionAxis3D::X,
                },
                80.0,
            ),
        );
        dimensions.insert(linear_id, linear);

        let boss = scc_engineering::FeatureId("boss".into());
        let diameter_id = DrawingDimensionId("fixture.diameter".into());
        let diameter = measured(
            &diameter_id.0,
            DrawingDimensionKind::Diameter,
            20.0,
            DimensionValueSource::EngineeringFeatureParameter {
                feature: boss.clone(),
                parameter: ParametricDimensionParameter::HoleDiameter,
            },
            DrawingDimensionTarget::Feature {
                feature: boss.clone(),
            },
        );
        layouts.insert(
            diameter_id.clone(),
            DrawingDimensionLayout {
                dimension: diameter_id.clone(),
                geometry: DimensionLayoutGeometry::Diameter {
                    center: DrawingPoint::new(60.0, 45.0),
                    axis_line: LayoutSegment {
                        start: DrawingPoint::new(50.0, 45.0),
                        end: DrawingPoint::new(70.0, 45.0),
                    },
                    arrows: [
                        ArrowAnchor {
                            point: DrawingPoint::new(50.0, 45.0),
                            direction: DrawingPoint::new(1.0, 0.0),
                        },
                        ArrowAnchor {
                            point: DrawingPoint::new(70.0, 45.0),
                            direction: DrawingPoint::new(-1.0, 0.0),
                        },
                    ],
                    text_anchor: DrawingPoint::new(78.0, 45.0),
                },
            },
        );
        associations.insert(
            diameter_id.clone(),
            association(
                &diameter,
                AssociativeDimensionReference::FeatureParameter {
                    feature: boss,
                    parameter: ParametricDimensionParameter::HoleDiameter,
                },
                20.0,
            ),
        );
        dimensions.insert(diameter_id, diameter);

        let fillet = scc_engineering::FeatureId("fillet".into());
        let radius_id = DrawingDimensionId("fixture.radius".into());
        let radius = measured(
            &radius_id.0,
            DrawingDimensionKind::Radius,
            5.0,
            DimensionValueSource::EngineeringFeatureParameter {
                feature: fillet.clone(),
                parameter: ParametricDimensionParameter::FilletRadius,
            },
            DrawingDimensionTarget::Feature {
                feature: fillet.clone(),
            },
        );
        layouts.insert(
            radius_id.clone(),
            DrawingDimensionLayout {
                dimension: radius_id.clone(),
                geometry: DimensionLayoutGeometry::Radius {
                    center: DrawingPoint::new(45.0, 45.0),
                    radius_line: LayoutSegment {
                        start: DrawingPoint::new(45.0, 45.0),
                        end: DrawingPoint::new(50.0, 45.0),
                    },
                    arrow: ArrowAnchor {
                        point: DrawingPoint::new(50.0, 45.0),
                        direction: DrawingPoint::new(-1.0, 0.0),
                    },
                    text_anchor: DrawingPoint::new(57.0, 45.0),
                },
            },
        );
        associations.insert(
            radius_id.clone(),
            association(
                &radius,
                AssociativeDimensionReference::FeatureParameter {
                    feature: fillet,
                    parameter: ParametricDimensionParameter::FilletRadius,
                },
                5.0,
            ),
        );
        dimensions.insert(radius_id, radius);

        let view_id = DrawingViewId("front".into());
        let mut views = BTreeMap::new();
        views.insert(
            view_id.clone(),
            DrawingView {
                id: view_id,
                kind: DrawingViewKind::Front,
                display_label: None,
                transform: crate::DrawingViewTransform::identity(),
                primitives: BTreeMap::from([(
                    DrawingPrimitiveId("fixture.base".into()),
                    DrawingPrimitive::Rectangle {
                        role: DrawingPrimitiveRole::Object,
                        origin: DrawingPoint::new(20.0, 20.0),
                        width: 80.0,
                        height: 50.0,
                    },
                )]),
                dimensions,
                dimension_layouts: layouts,
                center_geometry: BTreeMap::new(),
                model_bounds: DrawingBounds {
                    min_x: 20.0,
                    min_y: 20.0,
                    max_x: 100.0,
                    max_y: 70.0,
                },
            },
        );
        DrawingModel {
            sheets: BTreeMap::from([(
                DrawingSheetId("fixture".into()),
                DrawingSheet {
                    id: DrawingSheetId("fixture".into()),
                    views,
                    annotations: BTreeMap::new(),
                },
            )]),
            source_map: crate::DrawingSourceMap::new(),
            associative_dimensions: associations,
        }
    }

    #[test]
    fn native_dimensions_serialize_as_linear_diameter_and_radius_entities() {
        let dxf = drawing_to_dxf(&fixture()).unwrap();
        let entities = dxf
            .split("2\nENTITIES\n")
            .nth(1)
            .unwrap()
            .split("0\nENDSEC")
            .next()
            .unwrap();
        assert_eq!(entities.matches("0\nDIMENSION\n").count(), 3);
        assert!(dxf.contains("2\nDIMSTYLE\n"));
        assert!(dxf.contains("2\nAICAD_MM\n"));
        assert!(dxf.contains("2\n*D1\n"));
        assert!(dxf.contains("2\n*D2\n"));
        assert!(dxf.contains("2\n*D3\n"));
        assert!(entities.contains("100\nAcDbRotatedDimension\n"));
        assert!(entities.contains("100\nAcDbDiametricDimension\n"));
        assert!(entities.contains("100\nAcDbRadialDimension\n"));
        assert!(entities.contains("1\n80\n42\n80\n"));
        assert!(entities.contains("1\nØ20\n42\n20\n"));
        assert!(entities.contains("1\nR5\n42\n5\n"));
        assert!(dxf.contains("$INSUNITS\n70\n4\n"));
        assert!(dxf.contains("$DIMLUNIT\n70\n2\n"));
        assert!(dxf.contains("0\nSOLID\n"));
        assert!(dxf.contains("0\nLINE\n"));
        assert!(dxf.contains("0\nTEXT\n"));

        if let Ok(path) = std::env::var("AICAD_DXF_NATIVE_ARTIFACT") {
            std::fs::write(path, dxf).expect("write requested parser fixture");
        }
    }
}
