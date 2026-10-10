//! General principal-plane section view assembly for verified OCP section loops.
//!
//! The worker supplies closed, sampled B-Rep section boundaries. This module
//! keeps the drawing presentation generic: outlines, even-odd material hatches,
//! a principal-view cutting-plane trace, and typed provenance all derive from
//! those loops and the EngineeringModel digest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{
    DrawingBounds, DrawingModel, DrawingPrimitive, DrawingPrimitiveRole, DrawingView,
    DrawingViewTransform,
};
use crate::source_map::{SectionPrimitiveOrigin, SectionPrimitiveSource};
use crate::{
    DrawingError, DrawingPoint, DrawingPrimitiveId, DrawingSheet, DrawingSourceMap, DrawingViewId,
    DrawingViewKind,
};

const HATCH_SPACING_MM: f64 = 3.0;
const CUTTING_PLANE_GAP_MM: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionViewSide {
    Positive,
    Negative,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionViewGeometry {
    pub model_sha256: String,
    pub axis: SectionAxis,
    pub offset_mm: f64,
    pub view_side: SectionViewSide,
    pub loops: Vec<Vec<DrawingPoint>>,
    pub sample_chord_tolerance_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionViewReport {
    pub boundary_segments: usize,
    pub hatch_segments: usize,
    pub cutting_plane_segments: usize,
}

pub fn append_section_aa(
    drawing: &mut DrawingModel,
    geometry: SectionViewGeometry,
) -> Result<SectionViewReport, DrawingError> {
    validate_section_geometry(&geometry)?;
    let sheet = drawing
        .principal_sheet_mut()
        .ok_or_else(|| section_error("drawing has no principal sheet"))?;
    if sheet
        .views
        .contains_key(&DrawingViewId("section_aa".into()))
    {
        return Err(section_error("drawing already contains Section A-A"));
    }

    let mut primitives = BTreeMap::new();
    let mut sources = Vec::new();
    let mut all_points = Vec::new();
    for (loop_index, loop_points) in geometry.loops.iter().enumerate() {
        all_points.extend(loop_points.iter().copied());
        for segment_index in 0..loop_points.len() {
            let id =
                DrawingPrimitiveId(format!("section_aa_boundary_{loop_index}_{segment_index}"));
            primitives.insert(
                id.clone(),
                DrawingPrimitive::Line {
                    role: DrawingPrimitiveRole::Object,
                    start: loop_points[segment_index],
                    end: loop_points[(segment_index + 1) % loop_points.len()],
                },
            );
            sources.push((
                id,
                SectionPrimitiveOrigin::BoundarySegment {
                    loop_index: loop_index as u32,
                    segment_index: segment_index as u32,
                },
            ));
        }
    }

    let hatches = hatch_material_regions(&geometry.loops)?;
    for (index, (start, end)) in hatches.iter().copied().enumerate() {
        let id = DrawingPrimitiveId(format!("section_aa_hatch_{index}"));
        primitives.insert(
            id.clone(),
            DrawingPrimitive::Line {
                role: DrawingPrimitiveRole::SectionHatch,
                start,
                end,
            },
        );
        sources.push((
            id,
            SectionPrimitiveOrigin::HatchSegment {
                hatch_index: index as u32,
            },
        ));
    }

    let model_bounds = bounds_of(&all_points)?;
    let scale = sheet
        .views
        .get(&DrawingViewId("front".into()))
        .ok_or_else(|| section_error("formal Front view is missing"))?
        .transform
        .scale;
    let current_max_x = sheet.content_bounds().max_x;
    let translation = DrawingPoint::new(
        current_max_x + CUTTING_PLANE_GAP_MM - model_bounds.min_x * scale.0,
        DrawingSheet::DEFAULT_SHEET_MARGIN_MM - model_bounds.min_y * scale.0,
    );
    let label = format!(
        "剖视 A-A · 观察方向 {}{} · {}={}",
        if geometry.view_side == SectionViewSide::Positive {
            "+"
        } else {
            "−"
        },
        axis_label(geometry.axis),
        axis_label(geometry.axis).to_ascii_lowercase(),
        format_mm(geometry.offset_mm),
    );
    let section_view = DrawingView {
        id: DrawingViewId("section_aa".into()),
        kind: DrawingViewKind::SectionAA,
        display_label: Some(label),
        transform: DrawingViewTransform::new(scale, translation)?,
        primitives,
        dimensions: BTreeMap::new(),
        dimension_layouts: BTreeMap::new(),
        center_geometry: BTreeMap::new(),
        model_bounds,
    };
    sheet.views.insert(section_view.id.clone(), section_view);
    for (id, origin) in sources {
        drawing.source_map.record_section(
            id,
            SectionPrimitiveSource {
                model_sha256: geometry.model_sha256.clone(),
                origin,
            },
        );
    }

    let cutting_plane_segments = add_cutting_plane_trace(drawing, &geometry)?;
    validate_section_view(drawing, &geometry.model_sha256)?;
    Ok(SectionViewReport {
        boundary_segments: geometry.loops.iter().map(Vec::len).sum(),
        hatch_segments: hatches.len(),
        cutting_plane_segments,
    })
}

fn add_cutting_plane_trace(
    drawing: &mut DrawingModel,
    geometry: &SectionViewGeometry,
) -> Result<usize, DrawingError> {
    let (view_id, start, end) = {
        let sheet = drawing
            .principal_sheet()
            .ok_or_else(|| section_error("drawing has no sheet"))?;
        let (view_id, coordinate, vertical) = match geometry.axis {
            SectionAxis::X => (DrawingViewId("front".into()), geometry.offset_mm, true),
            SectionAxis::Y => (DrawingViewId("front".into()), geometry.offset_mm, false),
            SectionAxis::Z => (DrawingViewId("top".into()), -geometry.offset_mm, false),
        };
        let view = sheet
            .views
            .get(&view_id)
            .ok_or_else(|| section_error("cutting-plane reference view is missing"))?;
        let bounds = view.model_bounds;
        if vertical {
            if coordinate < bounds.min_x - 1e-6 || coordinate > bounds.max_x + 1e-6 {
                return Err(section_error(
                    "section plane lies outside its Front-view extent",
                ));
            }
            (
                view_id,
                DrawingPoint::new(coordinate, bounds.min_y),
                DrawingPoint::new(coordinate, bounds.max_y),
            )
        } else {
            if coordinate < bounds.min_y - 1e-6 || coordinate > bounds.max_y + 1e-6 {
                return Err(section_error(
                    "section plane lies outside its principal-view extent",
                ));
            }
            (
                view_id,
                DrawingPoint::new(bounds.min_x, coordinate),
                DrawingPoint::new(bounds.max_x, coordinate),
            )
        }
    };
    if (start.x - end.x).abs() <= 1e-9 && (start.y - end.y).abs() <= 1e-9 {
        return Err(section_error("cutting-plane reference is degenerate"));
    }
    let id = DrawingPrimitiveId("section_aa_cutting_plane".into());
    drawing
        .principal_sheet_mut()
        .unwrap()
        .views
        .get_mut(&view_id)
        .unwrap()
        .primitives
        .insert(
            id.clone(),
            DrawingPrimitive::Line {
                role: DrawingPrimitiveRole::Reference,
                start,
                end,
            },
        );
    drawing.source_map.record_section(
        id,
        SectionPrimitiveSource {
            model_sha256: geometry.model_sha256.clone(),
            origin: SectionPrimitiveOrigin::CuttingPlaneSegment {
                axis: geometry.axis,
            },
        },
    );
    Ok(1)
}

fn validate_section_view(drawing: &DrawingModel, model_sha256: &str) -> Result<(), DrawingError> {
    let sheet = drawing
        .principal_sheet()
        .ok_or_else(|| section_error("drawing has no sheet"))?;
    let view = sheet
        .views
        .get(&DrawingViewId("section_aa".into()))
        .ok_or_else(|| section_error("Section A-A is missing"))?;
    if view.kind != DrawingViewKind::SectionAA
        || view.primitives.is_empty()
        || view.display_label.is_none()
    {
        return Err(section_error("Section A-A view is incomplete"));
    }
    for id in view.primitives.keys() {
        let source = drawing
            .source_map
            .section_for_primitive(id)
            .ok_or_else(|| section_error("section primitive has no exact-section provenance"))?;
        if source.model_sha256 != model_sha256 {
            return Err(section_error(
                "section primitive refers to a different EngineeringModel rebuild",
            ));
        }
    }
    if !DrawingSourceMap::covers_all(drawing) {
        return Err(section_error(
            "DrawingModel contains an unprovenanced primitive",
        ));
    }
    Ok(())
}

fn validate_section_geometry(geometry: &SectionViewGeometry) -> Result<(), DrawingError> {
    if geometry.model_sha256.len() != 64
        || !geometry
            .model_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !geometry.offset_mm.is_finite()
        || !geometry.sample_chord_tolerance_mm.is_finite()
        || geometry.sample_chord_tolerance_mm <= 0.0
        || geometry.sample_chord_tolerance_mm > 0.01
        || geometry.loops.is_empty()
        || geometry.loops.iter().any(|loop_| {
            loop_.len() < 3 || loop_.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
        })
    {
        return Err(section_error(
            "section geometry has invalid identity, tolerance, or boundary data",
        ));
    }
    for loop_points in &geometry.loops {
        let area = polygon_area(loop_points);
        if !area.is_finite() || area.abs() <= 1e-6 {
            return Err(section_error("section loop has zero or non-finite area"));
        }
        if loop_points.iter().enumerate().any(|(i, point)| {
            let next = loop_points[(i + 1) % loop_points.len()];
            (point.x - next.x).abs() <= 1e-10 && (point.y - next.y).abs() <= 1e-10
        }) {
            return Err(section_error(
                "section loop contains a degenerate boundary segment",
            ));
        }
    }
    Ok(())
}

fn hatch_material_regions(
    loops: &[Vec<DrawingPoint>],
) -> Result<Vec<(DrawingPoint, DrawingPoint)>, DrawingError> {
    let points: Vec<_> = loops.iter().flatten().copied().collect();
    let direction = DrawingPoint::new(
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    );
    let normal = DrawingPoint::new(-direction.y, direction.x);
    let projections: Vec<_> = points
        .iter()
        .map(|p| p.x * normal.x + p.y * normal.y)
        .collect();
    let min = projections.iter().copied().fold(f64::INFINITY, f64::min);
    let max = projections
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let mut result = Vec::new();
    let mut offset = (min / HATCH_SPACING_MM).floor() * HATCH_SPACING_MM;
    while offset <= max + 1e-9 {
        let mut intersections = Vec::new();
        for loop_points in loops {
            for i in 0..loop_points.len() {
                let a = loop_points[i];
                let b = loop_points[(i + 1) % loop_points.len()];
                let da = a.x * normal.x + a.y * normal.y;
                let db = b.x * normal.x + b.y * normal.y;
                if (da <= offset && db > offset) || (db <= offset && da > offset) {
                    let t = (offset - da) / (db - da);
                    let p = DrawingPoint::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
                    intersections.push((p.x * direction.x + p.y * direction.y, p));
                }
            }
        }
        intersections.sort_by(|left, right| left.0.total_cmp(&right.0));
        if intersections.len() % 2 != 0 {
            return Err(section_error(
                "section boundary does not define closed even-odd material intervals",
            ));
        }
        for pair in intersections.chunks_exact(2) {
            if pair[1].0 - pair[0].0 <= 1e-7 {
                continue;
            }
            let start = pair[0].1;
            let end = pair[1].1;
            let midpoint = DrawingPoint::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
            if !inside_even_odd(midpoint, loops) {
                return Err(section_error(
                    "hatch interval is not inside actual section material",
                ));
            }
            result.push((start, end));
        }
        offset += HATCH_SPACING_MM;
    }
    Ok(result)
}

fn inside_even_odd(point: DrawingPoint, loops: &[Vec<DrawingPoint>]) -> bool {
    let mut inside = false;
    for loop_points in loops {
        for i in 0..loop_points.len() {
            let a = loop_points[i];
            let b = loop_points[(i + 1) % loop_points.len()];
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
    }
    inside
}

fn bounds_of(points: &[DrawingPoint]) -> Result<DrawingBounds, DrawingError> {
    let first = points
        .first()
        .ok_or_else(|| section_error("section has no boundary points"))?;
    let mut bounds = DrawingBounds {
        min_x: first.x,
        min_y: first.y,
        max_x: first.x,
        max_y: first.y,
    };
    for point in points.iter().skip(1) {
        bounds.min_x = bounds.min_x.min(point.x);
        bounds.min_y = bounds.min_y.min(point.y);
        bounds.max_x = bounds.max_x.max(point.x);
        bounds.max_y = bounds.max_y.max(point.y);
    }
    Ok(bounds)
}

fn polygon_area(points: &[DrawingPoint]) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(i, point)| {
            let next = points[(i + 1) % points.len()];
            point.x * next.y - next.x * point.y
        })
        .sum::<f64>()
        / 2.0
}

fn axis_label(axis: SectionAxis) -> &'static str {
    match axis {
        SectionAxis::X => "X",
        SectionAxis::Y => "Y",
        SectionAxis::Z => "Z",
    }
}

fn format_mm(value: f64) -> String {
    let mut value = format!("{value:.3}");
    while value.contains('.') && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

fn section_error(reason: impl Into<String>) -> DrawingError {
    DrawingError::InvalidGeometryValue {
        entity_id: scc_core::ids::EntityId("section_a_a".into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawingScale, DrawingSheetId};
    use std::collections::BTreeMap;

    fn fixture() -> DrawingModel {
        let mut views = BTreeMap::new();
        for (id, kind, bounds) in [
            (
                "front",
                DrawingViewKind::Front,
                DrawingBounds {
                    min_x: -20.0,
                    min_y: -10.0,
                    max_x: 20.0,
                    max_y: 10.0,
                },
            ),
            (
                "top",
                DrawingViewKind::Top,
                DrawingBounds {
                    min_x: -20.0,
                    min_y: -10.0,
                    max_x: 20.0,
                    max_y: 10.0,
                },
            ),
            (
                "left",
                DrawingViewKind::Left,
                DrawingBounds {
                    min_x: -10.0,
                    min_y: -10.0,
                    max_x: 10.0,
                    max_y: 10.0,
                },
            ),
        ] {
            let view_id = DrawingViewId(id.into());
            views.insert(
                view_id.clone(),
                DrawingView {
                    id: view_id,
                    kind,
                    display_label: None,
                    transform: DrawingViewTransform::new(
                        DrawingScale(1.0),
                        DrawingPoint::new(25.0, 20.0),
                    )
                    .unwrap(),
                    primitives: BTreeMap::new(),
                    dimensions: BTreeMap::new(),
                    dimension_layouts: BTreeMap::new(),
                    center_geometry: BTreeMap::new(),
                    model_bounds: bounds,
                },
            );
        }
        let sheet_id = DrawingSheetId("fixture".into());
        let mut sheets = BTreeMap::new();
        sheets.insert(
            sheet_id.clone(),
            DrawingSheet {
                id: sheet_id,
                views,
                annotations: BTreeMap::new(),
            },
        );
        DrawingModel {
            sheets,
            source_map: DrawingSourceMap::new(),
            associative_dimensions: BTreeMap::new(),
        }
    }

    fn geometry() -> SectionViewGeometry {
        SectionViewGeometry {
            model_sha256: "a".repeat(64),
            axis: SectionAxis::X,
            offset_mm: 0.0,
            view_side: SectionViewSide::Positive,
            loops: vec![
                vec![
                    DrawingPoint::new(-20.0, -10.0),
                    DrawingPoint::new(20.0, -10.0),
                    DrawingPoint::new(20.0, 10.0),
                    DrawingPoint::new(-20.0, 10.0),
                ],
                vec![
                    DrawingPoint::new(-3.0, -3.0),
                    DrawingPoint::new(3.0, -3.0),
                    DrawingPoint::new(3.0, 3.0),
                    DrawingPoint::new(-3.0, 3.0),
                ],
            ],
            sample_chord_tolerance_mm: 0.01,
        }
    }

    #[test]
    fn hatch_is_clipped_around_hole_and_section_sources_cover_drawing() {
        let geometry = geometry();
        let hatches = hatch_material_regions(&geometry.loops).unwrap();
        assert!(!hatches.is_empty());
        for (start, end) in &hatches {
            let midpoint = DrawingPoint::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
            assert!(inside_even_odd(midpoint, &geometry.loops));
        }
        let direction = DrawingPoint::new(
            std::f64::consts::FRAC_1_SQRT_2,
            std::f64::consts::FRAC_1_SQRT_2,
        );
        let normal = DrawingPoint::new(-direction.y, direction.x);
        let through_center: Vec<_> = hatches
            .iter()
            .filter(|(start, _)| (start.x * normal.x + start.y * normal.y).abs() < 1e-8)
            .collect();
        assert_eq!(
            through_center.len(),
            2,
            "diagonal material hatch must split at the through-hole"
        );
        assert!(through_center.iter().all(|(start, end)| {
            let t0 = start.x * direction.x + start.y * direction.y;
            let t1 = end.x * direction.x + end.y * direction.y;
            (t0.min(t1) < -4.25 && t0.max(t1) <= -4.24) || (t0.min(t1) >= 4.24 && t0.max(t1) > 4.25)
        }));

        let mut drawing = fixture();
        let report = append_section_aa(&mut drawing, geometry).unwrap();
        assert!(report.boundary_segments >= 8);
        assert!(report.hatch_segments > 0);
        assert_eq!(report.cutting_plane_segments, 1);
        assert!(DrawingSourceMap::covers_all(&drawing));
        let svg = crate::drawing_to_svg(&drawing).unwrap();
        assert!(svg.contains("id=\"view_front\""));
        assert!(svg.contains("id=\"view_top\""));
        assert!(svg.contains("id=\"view_left\""));
        assert!(svg.contains("id=\"view_section_aa\""));
        assert!(svg.contains("id=\"label_esection_aa\""));
        assert!(svg.contains("id=\"section_aa_hatch\""));
        assert!(svg.contains("剖视 A-A"));
        let dxf = crate::drawing_to_dxf(&drawing).unwrap();
        assert!(dxf.contains("$INSUNITS\n70\n4"));
        assert!(dxf.contains("AICAD_HATCH"));
        assert!(dxf.contains("剖视 A-A"));
        assert!(dxf.contains("SECTION\n2\nENTITIES"));
    }
}
