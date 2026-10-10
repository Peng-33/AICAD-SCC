//! The pure Phase 9.5 classifier and supported-domain preflight.

use std::collections::BTreeMap;

use scc_projection::{
    PrincipalViewKind, ProjectedGeometry, ProjectionModel, ProjectionPrimitiveId,
    ProjectionPrimitiveRole, ProjectionView, SpatialElementRef,
};
use scc_spatial::{
    SpatialCutId, SpatialEdgeVisibility, SpatialFootprint, SpatialModel, SpatialPrincipalView,
};

use crate::error::{InvalidVisibilityInputReason, UnsupportedVisibilityReason, VisibilityError};
use crate::geometry::{
    approximately_equal, center_distance, circle_strictly_inside, circle_strictly_inside_circle,
    composite_circle_containment, finite_add, finite_sub, radii_strictly_separated,
    segment_contained_in_segment, segments_coincident, segments_intersect_or_touch,
};
use crate::model::{VisibilityClass, VisibilityDecision, VisibilityModel, VisibilityView};

const PRINCIPAL_VIEWS: [PrincipalViewKind; 3] = [
    PrincipalViewKind::Front,
    PrincipalViewKind::Top,
    PrincipalViewKind::Left,
];

/// Classify the supplied raw projection model without re-running projection.
///
/// Both arguments are borrowed read-only. A successful result has one
/// decision for every primitive in each of the three canonical views.
pub fn classify_visibility(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<VisibilityModel, VisibilityError> {
    projection.validate_integrity()?;
    projection.validate_against(spatial)?;
    if let SpatialFootprint::Parametric { evidence } = &spatial.body.footprint {
        return classify_parametric(evidence, projection);
    }
    validate_visibility_domain(spatial)?;
    validate_projection_shape(spatial, projection)?;

    let mut views = BTreeMap::new();
    for kind in PRINCIPAL_VIEWS {
        let projection_view = projection
            .views
            .get(&kind)
            .ok_or_else(|| malformed(format!("{kind:?} projection view is missing")))?;
        let (decisions, visible_body_segments) = classify_body(kind, projection_view)?;
        let mut decisions = decisions;
        for cut_id in spatial.cuts.keys() {
            let cut_ids = primitive_ids_for_cut(projection, kind, cut_id);
            let cut = spatial.cuts.get(cut_id).ok_or_else(|| {
                malformed(format!("cut {cut_id} disappeared during classification"))
            })?;
            classify_cut(
                kind,
                cut_id,
                cut_ids,
                CutClassificationContext {
                    cut,
                    body_z_max: spatial.body.z.max,
                    view: projection_view,
                    visible_body_segments: &visible_body_segments,
                    decisions: &mut decisions,
                },
            )?;
        }
        classify_slots(kind, projection, projection_view, &mut decisions)?;
        let view = VisibilityView { decisions };
        if view.decisions.len() != projection_view.primitives.len() {
            return Err(malformed(format!(
                "{kind:?} decision count does not cover all projection primitives"
            )));
        }
        views.insert(kind, view);
    }

    let result = VisibilityModel { views };
    result.validate_against(projection)?;
    Ok(result)
}

fn classify_parametric(
    evidence: &scc_spatial::ParametricSpatialEvidence,
    projection: &ProjectionModel,
) -> Result<VisibilityModel, VisibilityError> {
    let views = [
        (SpatialPrincipalView::Front, PrincipalViewKind::Front),
        (SpatialPrincipalView::Top, PrincipalViewKind::Top),
        (SpatialPrincipalView::Left, PrincipalViewKind::Left),
    ];
    let mut decisions_by_view = BTreeMap::new();
    for (spatial_kind, projection_kind) in views {
        let projected = projection
            .views
            .get(&projection_kind)
            .ok_or_else(|| malformed(format!("{projection_kind:?} projection view is missing")))?;
        let evidence_edges = evidence.views.get(&spatial_kind).ok_or_else(|| {
            malformed(format!(
                "{spatial_kind:?} kernel visibility evidence is missing"
            ))
        })?;
        let mut decisions = BTreeMap::new();
        for edge in evidence_edges {
            let id = ProjectionPrimitiveId(edge.id.clone());
            let primitive = projected.primitives.get(&id).ok_or_else(|| {
                malformed(format!("kernel edge {} has no formal projection", edge.id))
            })?;
            if decisions.contains_key(&id) {
                return Err(malformed(format!(
                    "duplicate kernel visibility edge {}",
                    edge.id
                )));
            }
            let class = match edge.visibility {
                SpatialEdgeVisibility::Visible => VisibilityClass::Visible,
                SpatialEdgeVisibility::Hidden => VisibilityClass::Hidden,
            };
            // The Projection model owns geometry and depth. HLR evidence owns
            // classification only; never copy geometry across this boundary.
            if !edge.depth_min.is_finite()
                || !edge.depth_max.is_finite()
                || edge.depth_min > edge.depth_max
                || !primitive.depth.min.is_finite()
                || !primitive.depth.max.is_finite()
            {
                return Err(malformed(format!(
                    "kernel depth evidence is invalid for {}",
                    edge.id
                )));
            }
            decisions.insert(id, VisibilityDecision { class });
        }
        if decisions.len() != projected.primitives.len() {
            return Err(malformed(format!(
                "{projection_kind:?} kernel visibility does not cover every projection primitive"
            )));
        }
        decisions_by_view.insert(projection_kind, VisibilityView { decisions });
    }
    let model = VisibilityModel {
        views: decisions_by_view,
    };
    model.validate_against(projection)?;
    Ok(model)
}

/// Validate only the analytic material/topology domain supported by Phase 9.5.
///
/// This check does not mutate or invalidate the SpatialModel. It is a
/// classifier applicability gate, not engineering manufacturability
/// validation.
pub fn validate_visibility_domain(spatial: &SpatialModel) -> Result<(), VisibilityError> {
    if matches!(spatial.body.footprint, SpatialFootprint::Parametric { .. }) {
        return Ok(());
    }
    let (x0, x1, y0, y1, body_circle, body_composite) = match &spatial.body.footprint {
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
                return Err(malformed("body footprint contains a non-finite value"));
            }
            if *width <= 0.0 || *height <= 0.0 {
                return Err(malformed("body footprint dimensions must be positive"));
            }
            let x1 = finite_add(origin.x, *width)
                .ok_or_else(|| malformed("body X coordinate overflowed"))?;
            let y1 = finite_add(origin.y, *height)
                .ok_or_else(|| malformed("body Y coordinate overflowed"))?;
            if x1 <= origin.x || y1 <= origin.y {
                return Err(malformed("body footprint coordinates are not increasing"));
            }
            (origin.x, x1, origin.y, y1, None, None)
        }
        SpatialFootprint::Circle { center, radius } => {
            if !center.x.is_finite() || !center.y.is_finite() || !radius.is_finite() {
                return Err(malformed(
                    "circular body footprint contains a non-finite value",
                ));
            }
            if *radius <= 0.0 {
                return Err(malformed("circular body radius must be positive"));
            }
            let x0 = finite_sub(center.x, *radius)
                .ok_or_else(|| malformed("body X coordinate overflowed"))?;
            let x1 = finite_add(center.x, *radius)
                .ok_or_else(|| malformed("body X coordinate overflowed"))?;
            let y0 = finite_sub(center.y, *radius)
                .ok_or_else(|| malformed("body Y coordinate overflowed"))?;
            let y1 = finite_add(center.y, *radius)
                .ok_or_else(|| malformed("body Y coordinate overflowed"))?;
            if x1 <= x0 || y1 <= y0 {
                return Err(malformed(
                    "circular body envelope coordinates are not increasing",
                ));
            }
            (x0, x1, y0, y1, Some((center.x, center.y, *radius)), None)
        }
        SpatialFootprint::Parametric { .. } => {
            return Err(malformed("parametric footprint requires HLR evidence"));
        }
        SpatialFootprint::Composite { segments } => {
            if segments.is_empty()
                || segments.iter().any(|segment| !segment.is_valid())
                || segments.windows(2).any(|pair| {
                    !approximately_equal(pair[0].end().x, pair[1].start().x)
                        || !approximately_equal(pair[0].end().y, pair[1].start().y)
                })
                || !approximately_equal(
                    segments
                        .last()
                        .map(|segment| segment.end().x)
                        .unwrap_or(0.0),
                    segments
                        .first()
                        .map(|segment| segment.start().x)
                        .unwrap_or(0.0),
                )
                || !approximately_equal(
                    segments
                        .last()
                        .map(|segment| segment.end().y)
                        .unwrap_or(0.0),
                    segments
                        .first()
                        .map(|segment| segment.start().y)
                        .unwrap_or(0.0),
                )
            {
                return Err(malformed(
                    "composite body profile is not one valid closed loop",
                ));
            }
            let mut bounds: Option<(f64, f64, f64, f64)> = None;
            for segment in segments {
                let (sx0, sx1, sy0, sy1) = segment
                    .bounds()
                    .ok_or_else(|| malformed("composite body segment bounds are invalid"))?;
                bounds = Some(match bounds {
                    None => (sx0, sx1, sy0, sy1),
                    Some((bx0, bx1, by0, by1)) => {
                        (bx0.min(sx0), bx1.max(sx1), by0.min(sy0), by1.max(sy1))
                    }
                });
            }
            let (x0, x1, y0, y1) =
                bounds.ok_or_else(|| malformed("composite body has no finite bounds"))?;
            if !(x1 > x0 && y1 > y0) {
                return Err(malformed("composite body bounds are not strict"));
            }
            (x0, x1, y0, y1, None, Some(segments.as_slice()))
        }
    };

    let z0 = spatial.body.z.min;
    let z1 = spatial.body.z.max;
    if !z0.is_finite() || !z1.is_finite() || z0 != 0.0 || z1 <= z0 {
        return Err(malformed("body Z interval is not canonical [0,T]"));
    }
    let bounds = spatial.bounds;
    if !bounds.min_x.is_finite()
        || !bounds.max_x.is_finite()
        || !bounds.min_y.is_finite()
        || !bounds.max_y.is_finite()
        || !bounds.min_z.is_finite()
        || !bounds.max_z.is_finite()
        || bounds.max_x <= bounds.min_x
        || bounds.max_y <= bounds.min_y
        || bounds.max_z <= bounds.min_z
    {
        return Err(malformed("spatial bounds are not finite and strict"));
    }
    if bounds.min_x != x0
        || bounds.max_x != x1
        || bounds.min_y != y0
        || bounds.max_y != y1
        || bounds.min_z != z0
        || bounds.max_z != z1
    {
        return Err(malformed("spatial bounds do not witness the body"));
    }

    let rectangle = (x0, x1, y0, y1);
    let mut cuts: Vec<(SpatialCutId, (f64, f64))> = Vec::new();
    for (id, cut) in &spatial.cuts {
        if !cut.center.x.is_finite()
            || !cut.center.y.is_finite()
            || !cut.radius.is_finite()
            || !cut.z.min.is_finite()
            || !cut.z.max.is_finite()
        {
            return Err(malformed(format!("cut {id} contains a non-finite value")));
        }
        if cut.radius <= 0.0 {
            return Err(malformed(format!("cut {id} radius must be positive")));
        }
        if cut.z.min < z0 || cut.z.max > z1 || cut.z.max <= cut.z.min {
            return Err(malformed(format!(
                "cut {id} Z interval is outside the body"
            )));
        }
        let left = finite_sub(cut.center.x, cut.radius)
            .ok_or_else(|| malformed(format!("cut {id} X extent overflowed")))?;
        let right = finite_add(cut.center.x, cut.radius)
            .ok_or_else(|| malformed(format!("cut {id} X extent overflowed")))?;
        let bottom = finite_sub(cut.center.y, cut.radius)
            .ok_or_else(|| malformed(format!("cut {id} Y extent overflowed")))?;
        let top = finite_add(cut.center.y, cut.radius)
            .ok_or_else(|| malformed(format!("cut {id} Y extent overflowed")))?;
        let margins = [
            finite_sub(left, rectangle.0),
            finite_sub(rectangle.1, right),
            finite_sub(bottom, rectangle.2),
            finite_sub(rectangle.3, top),
        ];
        let margins = match margins {
            [Some(a), Some(b), Some(c), Some(d)] => [a, b, c, d],
            _ => return Err(malformed(format!("cut {id} extent comparison overflowed"))),
        };
        let (inside, touches_boundary) = match (body_circle, body_composite) {
            (Some((body_x, body_y, body_radius)), _) => {
                let inside = circle_strictly_inside_circle(
                    (cut.center.x, cut.center.y),
                    cut.radius,
                    (body_x, body_y),
                    body_radius,
                )
                .ok_or_else(|| malformed(format!("cut {id} circular containment overflowed")))?;
                let distance = center_distance((cut.center.x, cut.center.y), (body_x, body_y))
                    .ok_or_else(|| malformed(format!("cut {id} circular distance overflowed")))?;
                let reach = finite_add(distance, cut.radius)
                    .ok_or_else(|| malformed(format!("cut {id} circular extent overflowed")))?;
                let margin = finite_sub(body_radius, reach)
                    .ok_or_else(|| malformed(format!("cut {id} circular margin overflowed")))?;
                (
                    inside,
                    margin.abs() <= crate::geometry::VISIBILITY_TOLERANCE_MM,
                )
            }
            (None, Some(segments)) => {
                let (inside, tangent) = composite_circle_containment(
                    (cut.center.x, cut.center.y),
                    cut.radius,
                    segments,
                )
                .ok_or_else(|| malformed(format!("cut {id} composite containment overflowed")))?;
                (inside, tangent)
            }
            (None, None) => (
                circle_strictly_inside((cut.center.x, cut.center.y), cut.radius, rectangle)
                    .ok_or_else(|| {
                        malformed(format!("cut {id} containment comparison overflowed"))
                    })?,
                margins
                    .iter()
                    .any(|margin| margin.abs() <= crate::geometry::VISIBILITY_TOLERANCE_MM),
            ),
        };
        if !inside {
            let reason = if touches_boundary {
                UnsupportedVisibilityReason::TangentBodyCut { id: id.clone() }
            } else {
                UnsupportedVisibilityReason::OutsideBodyCut { id: id.clone() }
            };
            return Err(unsupported(reason));
        }
        let touches_base = approximately_equal(cut.z.min, z0);
        let touches_top = approximately_equal(cut.z.max, z1);
        if !touches_base && !touches_top {
            return Err(unsupported(
                UnsupportedVisibilityReason::UnsupportedZTopology { id: id.clone() },
            ));
        }
        cuts.push((id.clone(), (cut.center.x, cut.center.y)));
    }

    for (index, (first_id, first_center)) in cuts.iter().enumerate() {
        let first = spatial
            .cuts
            .get(first_id)
            .ok_or_else(|| malformed(format!("cut {first_id} disappeared during preflight")))?;
        for (second_id, second_center) in cuts.iter().skip(index + 1) {
            let second = spatial.cuts.get(second_id).ok_or_else(|| {
                malformed(format!("cut {second_id} disappeared during preflight"))
            })?;
            let distance = center_distance(*first_center, *second_center).ok_or_else(|| {
                malformed(format!(
                    "distance between {first_id} and {second_id} overflowed"
                ))
            })?;
            let radius_sum = finite_add(first.radius, second.radius).ok_or_else(|| {
                malformed(format!(
                    "radius sum for {first_id} and {second_id} overflowed"
                ))
            })?;
            if approximately_equal(distance, 0.0) {
                return Err(unsupported(UnsupportedVisibilityReason::CoincidentCuts {
                    first: first_id.clone(),
                    second: second_id.clone(),
                }));
            }
            let separated = radii_strictly_separated(distance, radius_sum).ok_or_else(|| {
                malformed(format!(
                    "separation comparison for {first_id} and {second_id} overflowed"
                ))
            })?;
            if !separated {
                let reason = if approximately_equal(distance, radius_sum) {
                    UnsupportedVisibilityReason::TangentCuts {
                        first: first_id.clone(),
                        second: second_id.clone(),
                    }
                } else {
                    UnsupportedVisibilityReason::OverlappingCuts {
                        first: first_id.clone(),
                        second: second_id.clone(),
                    }
                };
                return Err(unsupported(reason));
            }
        }
    }
    Ok(())
}

fn validate_projection_shape(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<(), VisibilityError> {
    if projection.views.len() != PRINCIPAL_VIEWS.len() {
        return Err(malformed("projection must contain exactly three views"));
    }
    for kind in PRINCIPAL_VIEWS {
        let view = projection
            .views
            .get(&kind)
            .ok_or_else(|| malformed(format!("{kind:?} projection view is missing")))?;
        let body_ids = view
            .primitives
            .iter()
            .filter(|(id, _)| {
                matches!(
                    projection.element_for_primitive(id),
                    Some(SpatialElementRef::Body | SpatialElementRef::BodySegment { .. })
                )
            })
            .count();
        let circular_front = is_circular_body(spatial) && kind == PrincipalViewKind::Front;
        let composite_front = is_composite_body(spatial) && kind == PrincipalViewKind::Front;
        let expected_body_count = if circular_front {
            2
        } else if composite_front {
            2 * composite_segment_count(spatial)
        } else {
            8
        };
        if body_ids != expected_body_count {
            return Err(invalid_count(format!(
                "{kind:?} body candidate count is {body_ids}, expected {expected_body_count}"
            )));
        }
        for (id, primitive) in &view.primitives {
            let source = projection
                .element_for_primitive(id)
                .ok_or_else(|| malformed(format!("primitive {id} has no source mapping")))?;
            match source {
                SpatialElementRef::Body | SpatialElementRef::BodySegment { .. } => {
                    if primitive.role != ProjectionPrimitiveRole::BodyBoundary {
                        return Err(VisibilityError::InvalidVisibilityInput {
                            reason: InvalidVisibilityInputReason::RoleSourceMismatch(format!(
                                "body primitive {id} is not BodyBoundary"
                            )),
                        });
                    }
                    let expected_circle = circular_front;
                    let valid_geometry = if expected_circle {
                        matches!(primitive.geometry, ProjectedGeometry::Circle { .. })
                    } else if composite_front {
                        matches!(
                            primitive.geometry,
                            ProjectedGeometry::Segment { .. } | ProjectedGeometry::Arc { .. }
                        )
                    } else {
                        matches!(primitive.geometry, ProjectedGeometry::Segment { .. })
                    };
                    if !valid_geometry {
                        return Err(invalid_geometry(format!(
                            "body primitive {id} has the wrong shape for {kind:?}"
                        )));
                    }
                    if !approximately_equal(primitive.depth.min, primitive.depth.max) {
                        return Err(invalid_geometry(format!(
                            "body primitive {id} has non-point depth"
                        )));
                    }
                }
                SpatialElementRef::Cut { id: cut_id } => {
                    let cut = spatial.cuts.get(cut_id).ok_or_else(|| {
                        malformed(format!("primitive {id} references unknown cut {cut_id}"))
                    })?;
                    if primitive.role != ProjectionPrimitiveRole::CutBoundary {
                        return Err(VisibilityError::InvalidVisibilityInput {
                            reason: InvalidVisibilityInputReason::RoleSourceMismatch(format!(
                                "cut primitive {id} is not CutBoundary"
                            )),
                        });
                    }
                    match kind {
                        PrincipalViewKind::Front => {
                            if !matches!(primitive.geometry, ProjectedGeometry::Circle { .. })
                                || !approximately_equal(primitive.depth.min, primitive.depth.max)
                            {
                                return Err(invalid_geometry(format!(
                                    "front cut primitive {id} is not a point-depth circle"
                                )));
                            }
                        }
                        PrincipalViewKind::Top | PrincipalViewKind::Left => {
                            if !matches!(primitive.geometry, ProjectedGeometry::Segment { .. }) {
                                return Err(invalid_geometry(format!(
                                    "{kind:?} cut primitive {id} is not a segment"
                                )));
                            }
                        }
                    }
                    let expected = match kind {
                        PrincipalViewKind::Front => 2,
                        PrincipalViewKind::Top | PrincipalViewKind::Left => 4,
                    };
                    let count = view
                        .primitives
                        .iter()
                        .filter(|(candidate_id, _)| {
                            matches!(
                                projection.element_for_primitive(candidate_id),
                                Some(SpatialElementRef::Cut { id }) if id == cut_id
                            )
                        })
                        .count();
                    if count != expected {
                        return Err(invalid_count(format!(
                            "{kind:?} cut {cut_id} candidate count is {count}, expected {expected}"
                        )));
                    }
                    let _ = cut;
                }
                SpatialElementRef::SlotSegment { id: cut_id, .. } => {
                    if primitive.role != ProjectionPrimitiveRole::CutBoundary {
                        return Err(VisibilityError::InvalidVisibilityInput {
                            reason: InvalidVisibilityInputReason::RoleSourceMismatch(format!(
                                "slot primitive {id} is not CutBoundary"
                            )),
                        });
                    }
                    if kind != PrincipalViewKind::Front {
                        return Err(invalid_geometry(format!(
                            "slot segment {id} is only valid in Front"
                        )));
                    }
                    if !matches!(
                        primitive.geometry,
                        ProjectedGeometry::Segment { .. } | ProjectedGeometry::Arc { .. }
                    ) {
                        return Err(invalid_geometry(format!(
                            "front slot primitive {id} is not a line or arc"
                        )));
                    }
                    let count = view
                        .primitives
                        .iter()
                        .filter(|(candidate_id, _)| {
                            matches!(
                                projection.element_for_primitive(candidate_id),
                                Some(SpatialElementRef::SlotSegment { id, .. }) if id == cut_id
                            )
                        })
                        .count();
                    if count != 8 {
                        return Err(invalid_count(format!(
                            "Front slot {cut_id} candidate count is {count}, expected 8"
                        )));
                    }
                }
                SpatialElementRef::SlotCut { id: cut_id } => {
                    if primitive.role != ProjectionPrimitiveRole::CutBoundary {
                        return Err(VisibilityError::InvalidVisibilityInput {
                            reason: InvalidVisibilityInputReason::RoleSourceMismatch(format!(
                                "slot silhouette {id} is not CutBoundary"
                            )),
                        });
                    }
                    if kind == PrincipalViewKind::Front {
                        return Err(invalid_geometry(format!(
                            "slot silhouette {id} is not a Front source"
                        )));
                    }
                    if !matches!(primitive.geometry, ProjectedGeometry::Segment { .. }) {
                        return Err(invalid_geometry(format!(
                            "{kind:?} slot silhouette {id} is not a segment"
                        )));
                    }
                    let count = view
                        .primitives
                        .iter()
                        .filter(|(candidate_id, _)| {
                            matches!(
                                projection.element_for_primitive(candidate_id),
                                Some(SpatialElementRef::SlotCut { id }) if id == cut_id
                            )
                        })
                        .count();
                    if count != 4 {
                        return Err(invalid_count(format!(
                            "{kind:?} slot {cut_id} candidate count is {count}, expected 4"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn classify_body(
    kind: PrincipalViewKind,
    view: &ProjectionView,
) -> Result<
    (
        BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
        Vec<ProjectedGeometry>,
    ),
    VisibilityError,
> {
    let ids: Vec<ProjectionPrimitiveId> = view
        .primitives
        .iter()
        .filter_map(|(id, primitive)| {
            (primitive.role == ProjectionPrimitiveRole::BodyBoundary).then_some(id.clone())
        })
        .collect();
    if ids.len() == 2
        && ids.iter().all(|id| {
            matches!(
                view.primitives[id].geometry,
                ProjectedGeometry::Circle { .. }
            )
        })
    {
        let first = &view.primitives[&ids[0]].geometry;
        let second = &view.primitives[&ids[1]].geometry;
        if !circles_coincident(first, second) {
            return Err(invalid_geometry(format!(
                "{kind:?} circular body candidates are not coincident"
            )));
        }
        let winner = unique_depth_winner(kind, &ids, view)?;
        let mut decisions = BTreeMap::new();
        for id in ids {
            let class = if id == winner {
                VisibilityClass::Visible
            } else {
                VisibilityClass::Occluded
            };
            decisions.insert(id, VisibilityDecision { class });
        }
        return Ok((decisions, vec![view.primitives[&winner].geometry.clone()]));
    }

    let expected_groups = ids.len() / 2;
    let odd_candidate_count = !ids.len().is_multiple_of(2);
    let mut groups: Vec<Vec<ProjectionPrimitiveId>> = Vec::new();
    for id in ids {
        let geometry = &view.primitives[&id].geometry;
        let matching: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let representative = group
                    .first()
                    .and_then(|member| view.primitives.get(member))?;
                match segments_coincident(&representative.geometry, geometry) {
                    Some(true) => Some(index),
                    Some(false) => None,
                    None => None,
                }
            })
            .collect();
        if matching.len() > 1 {
            return Err(malformed(format!(
                "{kind:?} body candidate has ambiguous geometric grouping"
            )));
        }
        if let Some(index) = matching.first() {
            groups[*index].push(id);
        } else {
            groups.push(vec![id]);
        }
    }
    if odd_candidate_count
        || groups.len() != expected_groups
        || groups.iter().any(|group| group.len() != 2)
    {
        return Err(invalid_count(format!(
            "{kind:?} body must contain {expected_groups} coincident two-candidate edge groups"
        )));
    }

    let mut decisions = BTreeMap::new();
    let mut visible_geometry = Vec::new();
    for group in groups {
        let winner = unique_depth_winner(kind, &group, view)?;
        for id in &group {
            let class = if *id == winner {
                VisibilityClass::Visible
            } else {
                VisibilityClass::Occluded
            };
            decisions.insert(id.clone(), VisibilityDecision { class });
        }
        visible_geometry.push(view.primitives[&winner].geometry.clone());
    }
    Ok((decisions, visible_geometry))
}

fn is_circular_body(spatial: &SpatialModel) -> bool {
    matches!(spatial.body.footprint, SpatialFootprint::Circle { .. })
}

fn is_composite_body(spatial: &SpatialModel) -> bool {
    matches!(spatial.body.footprint, SpatialFootprint::Composite { .. })
}

fn composite_segment_count(spatial: &SpatialModel) -> usize {
    match &spatial.body.footprint {
        SpatialFootprint::Composite { segments } => segments.len(),
        _ => 0,
    }
}

struct CutClassificationContext<'a> {
    cut: &'a scc_spatial::CylindricalCut,
    body_z_max: f64,
    view: &'a ProjectionView,
    visible_body_segments: &'a [ProjectedGeometry],
    decisions: &'a mut BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
}

fn classify_cut(
    kind: PrincipalViewKind,
    cut_id: &SpatialCutId,
    ids: Vec<ProjectionPrimitiveId>,
    context: CutClassificationContext<'_>,
) -> Result<(), VisibilityError> {
    if ids.is_empty() {
        return Err(invalid_count(format!(
            "{kind:?} cut {cut_id} has no candidates"
        )));
    }
    match kind {
        PrincipalViewKind::Front => classify_front_cut(
            cut_id,
            ids,
            context.cut,
            context.body_z_max,
            context.view,
            context.decisions,
        ),
        PrincipalViewKind::Top | PrincipalViewKind::Left => classify_side_cut(
            kind,
            cut_id,
            ids,
            context.view,
            context.visible_body_segments,
            context.decisions,
        ),
    }
}

fn classify_slots(
    kind: PrincipalViewKind,
    projection: &ProjectionModel,
    view: &ProjectionView,
    decisions: &mut BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
) -> Result<(), VisibilityError> {
    match kind {
        PrincipalViewKind::Front => {
            let ids: Vec<ProjectionPrimitiveId> = view
                .primitives
                .keys()
                .filter(|id| {
                    matches!(
                        projection.element_for_primitive(id),
                        Some(SpatialElementRef::SlotSegment { .. })
                    )
                })
                .cloned()
                .collect();
            classify_coincident_pairs(kind, ids, view, decisions, VisibilityClass::Visible)
        }
        PrincipalViewKind::Top | PrincipalViewKind::Left => {
            for id in view.primitives.keys() {
                if matches!(
                    projection.element_for_primitive(id),
                    Some(SpatialElementRef::SlotCut { .. })
                ) {
                    decisions.insert(
                        id.clone(),
                        VisibilityDecision {
                            class: VisibilityClass::Hidden,
                        },
                    );
                }
            }
            Ok(())
        }
    }
}

fn classify_coincident_pairs(
    kind: PrincipalViewKind,
    mut remaining: Vec<ProjectionPrimitiveId>,
    view: &ProjectionView,
    decisions: &mut BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
    nearest_class: VisibilityClass,
) -> Result<(), VisibilityError> {
    remaining.sort();
    while let Some(first) = remaining.first().cloned() {
        remaining.retain(|id| id != &first);
        let first_primitive = &view.primitives[&first];
        let partner = remaining.iter().position(|id| {
            geometry_equal(&first_primitive.geometry, &view.primitives[id].geometry)
        });
        let Some(index) = partner else {
            return Err(invalid_geometry(format!(
                "{kind:?} slot geometry {first} has no coincident partner"
            )));
        };
        let second = remaining.remove(index);
        let winner = unique_depth_winner(kind, &[first.clone(), second.clone()], view)?;
        for id in [first, second] {
            let class = if id == winner {
                nearest_class
            } else {
                VisibilityClass::Occluded
            };
            decisions.insert(id, VisibilityDecision { class });
        }
    }
    Ok(())
}

fn geometry_equal(left: &ProjectedGeometry, right: &ProjectedGeometry) -> bool {
    match (left, right) {
        (
            ProjectedGeometry::Segment {
                start: a_start,
                end: a_end,
            },
            ProjectedGeometry::Segment {
                start: b_start,
                end: b_end,
            },
        ) => {
            (approximately_equal(a_start.u, b_start.u)
                && approximately_equal(a_start.v, b_start.v)
                && approximately_equal(a_end.u, b_end.u)
                && approximately_equal(a_end.v, b_end.v))
                || (approximately_equal(a_start.u, b_end.u)
                    && approximately_equal(a_start.v, b_end.v)
                    && approximately_equal(a_end.u, b_start.u)
                    && approximately_equal(a_end.v, b_start.v))
        }
        (
            ProjectedGeometry::Arc {
                center: a_center,
                radius: a_radius,
                start_angle: a_start,
                end_angle: a_end,
            },
            ProjectedGeometry::Arc {
                center: b_center,
                radius: b_radius,
                start_angle: b_start,
                end_angle: b_end,
            },
        ) => {
            approximately_equal(a_center.u, b_center.u)
                && approximately_equal(a_center.v, b_center.v)
                && approximately_equal(*a_radius, *b_radius)
                && approximately_equal(*a_start, *b_start)
                && approximately_equal(*a_end, *b_end)
        }
        _ => false,
    }
}

fn classify_front_cut(
    cut_id: &SpatialCutId,
    ids: Vec<ProjectionPrimitiveId>,
    cut: &scc_spatial::CylindricalCut,
    body_z_max: f64,
    view: &ProjectionView,
    decisions: &mut BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
) -> Result<(), VisibilityError> {
    if ids.len() != 2 {
        return Err(invalid_count(format!(
            "front cut {cut_id} has {} candidates, expected 2",
            ids.len()
        )));
    }
    let first = &view.primitives[&ids[0]];
    let second = &view.primitives[&ids[1]];
    let (first_geometry, second_geometry) = (&first.geometry, &second.geometry);
    if !matches!(first_geometry, ProjectedGeometry::Circle { .. })
        || !matches!(second_geometry, ProjectedGeometry::Circle { .. })
    {
        return Err(invalid_geometry(format!(
            "front cut {cut_id} circles are not both circles"
        )));
    }
    if !circles_coincident(first_geometry, second_geometry) {
        return Err(invalid_geometry(format!(
            "front cut {cut_id} circles are not coincident"
        )));
    }
    let winner = unique_depth_winner(PrincipalViewKind::Front, &ids, view)?;
    let nearest_class = if approximately_equal(cut.z.max, body_z_max) {
        VisibilityClass::Visible
    } else {
        VisibilityClass::Hidden
    };
    for id in ids {
        let class = if id == winner {
            nearest_class
        } else {
            VisibilityClass::Occluded
        };
        decisions.insert(id, VisibilityDecision { class });
    }
    Ok(())
}

fn circles_coincident(a: &ProjectedGeometry, b: &ProjectedGeometry) -> bool {
    match (a, b) {
        (
            ProjectedGeometry::Circle {
                center: a_center,
                radius: a_radius,
            },
            ProjectedGeometry::Circle {
                center: b_center,
                radius: b_radius,
            },
        ) => {
            approximately_equal(a_center.u, b_center.u)
                && approximately_equal(a_center.v, b_center.v)
                && approximately_equal(*a_radius, *b_radius)
        }
        _ => false,
    }
}

fn classify_side_cut(
    kind: PrincipalViewKind,
    cut_id: &SpatialCutId,
    ids: Vec<ProjectionPrimitiveId>,
    view: &ProjectionView,
    visible_body_segments: &[ProjectedGeometry],
    decisions: &mut BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
) -> Result<(), VisibilityError> {
    if ids.len() != 4 {
        return Err(invalid_count(format!(
            "{kind:?} cut {cut_id} has {} candidates, expected 4",
            ids.len()
        )));
    }
    let mut rims = Vec::new();
    let mut tangents = Vec::new();
    for id in ids {
        let primitive = view.primitives.get(&id).ok_or_else(|| {
            malformed(format!("{kind:?} cut {cut_id} candidate {id} disappeared"))
        })?;
        match &primitive.geometry {
            ProjectedGeometry::Segment { .. } => {
                if approximately_equal(primitive.depth.min, primitive.depth.max) {
                    tangents.push(id);
                } else {
                    rims.push(id);
                }
            }
            _ => {
                return Err(invalid_geometry(format!(
                    "{kind:?} cut {cut_id} candidate {id} is not a segment"
                )))
            }
        }
    }
    if rims.len() != 2 || tangents.len() != 2 {
        return Err(invalid_geometry(format!(
            "{kind:?} cut {cut_id} must contain two rims and two tangents"
        )));
    }
    for id in tangents {
        decisions.insert(
            id,
            VisibilityDecision {
                class: VisibilityClass::Hidden,
            },
        );
    }
    for id in rims {
        let primitive = &view.primitives[&id];
        let mut is_surface = false;
        let mut has_partial_contact = false;
        for body in visible_body_segments {
            if segment_contained_in_segment(&primitive.geometry, body).ok_or_else(|| {
                malformed(format!(
                    "{kind:?} cut {cut_id} containment comparison overflowed"
                ))
            })? {
                is_surface = true;
                break;
            }
            if segments_intersect_or_touch(&primitive.geometry, body).ok_or_else(|| {
                malformed(format!(
                    "{kind:?} cut {cut_id} contact comparison overflowed"
                ))
            })? {
                has_partial_contact = true;
            }
        }
        if has_partial_contact {
            return Err(unsupported(
                UnsupportedVisibilityReason::PartialCandidateClassification,
            ));
        }
        let class = if is_surface {
            VisibilityClass::Occluded
        } else {
            VisibilityClass::Hidden
        };
        decisions.insert(id, VisibilityDecision { class });
    }
    Ok(())
}

fn unique_depth_winner(
    kind: PrincipalViewKind,
    ids: &[ProjectionPrimitiveId],
    view: &ProjectionView,
) -> Result<ProjectionPrimitiveId, VisibilityError> {
    let mut winner: Option<ProjectionPrimitiveId> = None;
    for id in ids {
        let primitive = view
            .primitives
            .get(id)
            .ok_or_else(|| malformed(format!("{kind:?} primitive {id} disappeared")))?;
        if let Some(current_id) = &winner {
            let current = &view.primitives[current_id];
            if approximately_equal(primitive.depth.max, current.depth.max) {
                return Err(VisibilityError::AmbiguousCoincidentCandidates {
                    view: kind,
                    ids: ids.to_vec(),
                });
            }
            if primitive.depth.max > current.depth.max {
                winner = Some(id.clone());
            }
        } else {
            winner = Some(id.clone());
        }
    }
    winner.ok_or_else(|| malformed(format!("{kind:?} candidate group is empty")))
}

fn primitive_ids_for_cut(
    projection: &ProjectionModel,
    kind: PrincipalViewKind,
    cut_id: &SpatialCutId,
) -> Vec<ProjectionPrimitiveId> {
    projection.views[&kind]
        .primitives
        .keys()
        .filter_map(|id| {
            matches!(
                projection.element_for_primitive(id),
                Some(SpatialElementRef::Cut { id }) if id == cut_id
            )
            .then_some(id.clone())
        })
        .collect()
}

fn malformed(reason: impl Into<String>) -> VisibilityError {
    VisibilityError::InvalidVisibilityInput {
        reason: InvalidVisibilityInputReason::MalformedProjectionModel(reason.into()),
    }
}

fn invalid_geometry(reason: impl Into<String>) -> VisibilityError {
    VisibilityError::InvalidVisibilityInput {
        reason: InvalidVisibilityInputReason::UnexpectedGeometry(reason.into()),
    }
}

fn invalid_count(reason: impl Into<String>) -> VisibilityError {
    VisibilityError::InvalidVisibilityInput {
        reason: InvalidVisibilityInputReason::UnexpectedPrimitiveCount(reason.into()),
    }
}

fn unsupported(reason: UnsupportedVisibilityReason) -> VisibilityError {
    VisibilityError::UnsupportedVisibilityConfiguration { reason }
}
