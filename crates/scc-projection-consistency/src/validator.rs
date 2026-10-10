//! Deterministic, non-mutating Phase 9.6 invariant validation.

use std::collections::{BTreeMap, BTreeSet};

use scc_projection::{
    OrthographicFrame, PartLocalPoint3, PrincipalViewKind, ProjectedGeometry, ProjectedPoint2,
    ProjectionDepthRange, ProjectionError, ProjectionModel, ProjectionPrimitive,
    ProjectionPrimitiveId, ProjectionPrimitiveRole, SpatialElementRef,
};
use scc_spatial::{
    SlotCut, SpatialArcDirection, SpatialCutId, SpatialFootprint, SpatialModel,
    SpatialPrincipalView, SpatialProfileSegment, SpatialProjectedGeometry,
};
use scc_visibility::{
    approximately_equal_mm, validate_visibility_domain, VisibilityClass, VisibilityError,
    VisibilityModel,
};

use crate::error::{ConsistencyModel, ProjectionConsistencyError};

const PRINCIPAL_VIEWS: [PrincipalViewKind; 3] = [
    PrincipalViewKind::Front,
    PrincipalViewKind::Top,
    PrincipalViewKind::Left,
];

/// Validate the complete frozen SpatialModel -> ProjectionModel ->
/// VisibilityModel evidence chain.
///
/// The function only reads its inputs. It deliberately does not call either
/// derived-artifact builder (project_principal_views or classify_visibility);
/// its local formulas check the already stored result.
pub fn validate_projection_consistency(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
) -> Result<(), ProjectionConsistencyError> {
    if let SpatialFootprint::Parametric { evidence } = &spatial.body.footprint {
        return validate_parametric_evidence(spatial, projection, visibility, evidence);
    }
    validate_domain(spatial)?;
    // The exact principal-view set is the shallow model boundary. Check it
    // before deep source coverage so a missing view cannot be misreported as
    // an orphan source-map entry.
    validate_principal_views(projection)?;
    // Run the explicit source-map gate before the upstream integrity helper.
    // ProjectionModel::validate_integrity deliberately reports a missing
    // coverage entry with an empty id; this gate retains the real offending
    // primitive in the PCV diagnostic.
    validate_source_map(spatial, projection)?;
    validate_projection_structure(spatial, projection)?;
    visibility
        .validate_against(projection)
        .map_err(map_visibility_error)?;
    validate_roles_and_cardinality(spatial, projection)?;
    validate_geometry_and_depth(spatial, projection)?;
    validate_visibility_invariants(spatial, projection, visibility)?;
    Ok(())
}

fn validate_parametric_evidence(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
    evidence: &scc_spatial::ParametricSpatialEvidence,
) -> Result<(), ProjectionConsistencyError> {
    projection
        .validate_integrity()
        .map_err(map_projection_error)?;
    projection
        .validate_against(spatial)
        .map_err(map_projection_error)?;
    visibility
        .validate_against(projection)
        .map_err(map_visibility_error)?;
    let expected_views = [
        (SpatialPrincipalView::Front, PrincipalViewKind::Front),
        (SpatialPrincipalView::Top, PrincipalViewKind::Top),
        (SpatialPrincipalView::Left, PrincipalViewKind::Left),
    ];
    if evidence.model_sha256.len() != 64
        || !evidence
            .model_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !evidence.volume_mm3.is_finite()
        || evidence.volume_mm3 <= 0.0
        || evidence.edges.is_empty()
        || evidence.faces.is_empty()
        || !spatial.bounds.min_x.is_finite()
        || !spatial.bounds.max_x.is_finite()
        || !spatial.bounds.min_y.is_finite()
        || !spatial.bounds.max_y.is_finite()
        || !spatial.bounds.min_z.is_finite()
        || !spatial.bounds.max_z.is_finite()
        || spatial.bounds.max_x <= spatial.bounds.min_x
        || spatial.bounds.max_y <= spatial.bounds.min_y
        || spatial.bounds.max_z <= spatial.bounds.min_z
    {
        return Err(unsupported(
            "parametric body evidence is incomplete or invalid",
        ));
    }
    for (evidence_view, projection_view) in expected_views {
        let raw_edges = evidence.views.get(&evidence_view).ok_or_else(|| {
            unsupported(format!(
                "parametric {evidence_view:?} HLR evidence is missing"
            ))
        })?;
        let raw_view = projection.views.get(&projection_view).ok_or_else(|| {
            unsupported(format!("parametric {projection_view:?} view is missing"))
        })?;
        if raw_edges.len() != raw_view.primitives.len() {
            return Err(unsupported(format!(
                "parametric {projection_view:?} HLR and Projection cardinalities differ"
            )));
        }
        let mut ids = BTreeSet::new();
        for edge in raw_edges {
            if !ids.insert(edge.id.as_str()) {
                return Err(unsupported(format!(
                    "parametric {projection_view:?} HLR has duplicate edge IDs"
                )));
            }
            let id = ProjectionPrimitiveId(edge.id.clone());
            let Some(primitive) = raw_view.primitives.get(&id) else {
                return Err(unsupported(format!(
                    "parametric HLR edge {} has no Projection primitive",
                    edge.id
                )));
            };
            if primitive.role != ProjectionPrimitiveRole::BodyBoundary
                || primitive.depth.min != edge.depth_min
                || primitive.depth.max != edge.depth_max
            {
                return Err(unsupported(format!(
                    "parametric Projection role or depth differs from HLR edge {}",
                    edge.id
                )));
            }
            let expected_geometry = match &edge.geometry {
                SpatialProjectedGeometry::Segment { start, end } => ProjectedGeometry::Segment {
                    start: ProjectedPoint2 {
                        u: start.x,
                        v: start.y,
                    },
                    end: ProjectedPoint2 { u: end.x, v: end.y },
                },
                SpatialProjectedGeometry::Circle { center, radius } => ProjectedGeometry::Circle {
                    center: ProjectedPoint2 {
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
                    center: ProjectedPoint2 {
                        u: center.x,
                        v: center.y,
                    },
                    radius: *radius,
                    start_angle: *start_angle,
                    end_angle: *end_angle,
                },
            };
            if primitive.geometry != expected_geometry {
                return Err(unsupported(format!(
                    "parametric Projection geometry differs from HLR edge {}",
                    edge.id
                )));
            }
            let source = projection.element_for_primitive(&id);
            if source != Some(&SpatialElementRef::Body) {
                return Err(source_mismatch(
                    Some(projection_view),
                    Some(id),
                    source.cloned(),
                    "parametric final B-Rep edge must reference the derived body",
                ));
            }
            let expected_class = match edge.visibility {
                scc_spatial::SpatialEdgeVisibility::Visible => VisibilityClass::Visible,
                scc_spatial::SpatialEdgeVisibility::Hidden => VisibilityClass::Hidden,
            };
            if visibility.class_for(projection_view, &ProjectionPrimitiveId(edge.id.clone()))
                != Some(expected_class)
            {
                return Err(unsupported(format!(
                    "Visibility classification differs from OCC HLR edge {}",
                    edge.id
                )));
            }
        }
    }
    Ok(())
}

fn validate_domain(spatial: &SpatialModel) -> Result<(), ProjectionConsistencyError> {
    validate_visibility_domain(spatial).map_err(map_visibility_error)?;

    // The resolved interval is the source of truth, and the stored face
    // witness must agree with the authoritative lowering convention:
    // Base => [0, D], Opposite => [T-D, T]. This validates provenance of the
    // resolved interval without re-lowering or consulting engineering intent.
    let z0 = spatial.body.z.min;
    let z1 = spatial.body.z.max;
    for (id, cut) in &spatial.cuts {
        let touches_base = approximately_equal(cut.z.min, z0);
        let touches_top = approximately_equal(cut.z.max, z1);
        match cut.kind {
            scc_spatial::SpatialCutKind::ThroughAll => {
                if !touches_base || !touches_top {
                    return Err(unsupported(format!(
                        "cut {id} is marked ThroughAll but its resolved interval is not the body interval"
                    )));
                }
            }
            scc_spatial::SpatialCutKind::Blind { start_face } => {
                let face_matches = match start_face {
                    scc_spatial::HoleStartFace::Base => touches_base && !touches_top,
                    scc_spatial::HoleStartFace::Opposite => !touches_base && touches_top,
                };
                if !face_matches {
                    return Err(unsupported(format!(
                        "cut {id} Blind start face disagrees with its resolved Z interval"
                    )));
                }
            }
        }
    }
    for (id, slot) in &spatial.slot_cuts {
        if slot.segments.len() != 4
            || !matches!(
                slot.segments.as_slice(),
                [
                    SpatialProfileSegment::Line { .. },
                    SpatialProfileSegment::Arc { .. },
                    SpatialProfileSegment::Line { .. },
                    SpatialProfileSegment::Arc { .. },
                ]
            )
        {
            return Err(unsupported(format!(
                "slot {id} must keep an exact line-arc-line-arc footprint; bounding-rectangle cuts are forbidden"
            )));
        }
    }
    Ok(())
}

fn validate_projection_structure(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<(), ProjectionConsistencyError> {
    projection
        .validate_integrity()
        .map_err(map_projection_error)?;
    projection
        .validate_against(spatial)
        .map_err(map_projection_error)?;
    Ok(())
}

fn validate_principal_views(
    projection: &ProjectionModel,
) -> Result<(), ProjectionConsistencyError> {
    if projection.views.len() != PRINCIPAL_VIEWS.len() {
        return Err(view_mismatch(
            None,
            None,
            "projection must contain exactly Front, Top, and Left views",
        ));
    }
    for kind in PRINCIPAL_VIEWS {
        if !projection.views.contains_key(&kind) {
            return Err(view_mismatch(
                Some(kind),
                None,
                "required principal view is missing",
            ));
        }
    }
    Ok(())
}

fn validate_source_map(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<(), ProjectionConsistencyError> {
    for kind in PRINCIPAL_VIEWS {
        let view = projection
            .views
            .get(&kind)
            .expect("principal views checked");
        for id in view.primitives.keys() {
            let source = projection
                .sources
                .element_for_primitive(id)
                .ok_or_else(|| {
                    source_mismatch(
                        Some(kind),
                        Some(id.clone()),
                        None,
                        "projection primitive has no source-map entry",
                    )
                })?;
            match source {
                SpatialElementRef::Body => {}
                SpatialElementRef::BodySegment { index } => {
                    let valid = matches!(
                        &spatial.body.footprint,
                        SpatialFootprint::Composite { segments }
                            if segments.iter().any(|segment| segment.segment_index() == *index)
                    );
                    if !valid {
                        return Err(source_mismatch(
                            Some(kind),
                            Some(id.clone()),
                            Some(source.clone()),
                            "source-map body segment does not exist in SpatialModel",
                        ));
                    }
                }
                SpatialElementRef::Cut { id: cut_id } => {
                    if !spatial.cuts.contains_key(cut_id) {
                        return Err(source_mismatch(
                            Some(kind),
                            Some(id.clone()),
                            Some(source.clone()),
                            "source-map cut does not exist in SpatialModel",
                        ));
                    }
                }
                SpatialElementRef::SlotCut { id: cut_id } => {
                    if !spatial.slot_cuts.contains_key(cut_id) {
                        return Err(source_mismatch(
                            Some(kind),
                            Some(id.clone()),
                            Some(source.clone()),
                            "source-map slot cut does not exist in SpatialModel",
                        ));
                    }
                }
                SpatialElementRef::SlotSegment { id: cut_id, index } => {
                    let valid = spatial.slot_cuts.get(cut_id).is_some_and(|slot| {
                        slot.segments
                            .iter()
                            .any(|segment| segment.segment_index() == *index)
                    });
                    if !valid {
                        return Err(source_mismatch(
                            Some(kind),
                            Some(id.clone()),
                            Some(source.clone()),
                            "source-map slot segment does not exist in SpatialModel",
                        ));
                    }
                }
            }
        }
    }

    // validate_integrity rejects orphan entries. Keep the explicit iteration
    // here as an auditable source-map gate with deterministic traversal.
    for (id, source) in projection.sources.iter() {
        if !projection
            .views
            .values()
            .any(|view| view.primitives.contains_key(id))
        {
            return Err(source_mismatch(
                None,
                Some(id.clone()),
                Some(source.clone()),
                "source-map entry has no projection primitive",
            ));
        }
    }
    Ok(())
}

fn validate_roles_and_cardinality(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<(), ProjectionConsistencyError> {
    for kind in PRINCIPAL_VIEWS {
        let view = projection
            .views
            .get(&kind)
            .expect("principal views checked");
        let mut counts: BTreeMap<SpatialElementRef, usize> = BTreeMap::new();

        for (id, primitive) in &view.primitives {
            let source = projection
                .sources
                .element_for_primitive(id)
                .expect("source map checked");
            *counts.entry(source.clone()).or_default() += 1;

            let role_ok = match source {
                SpatialElementRef::Body | SpatialElementRef::BodySegment { .. } => {
                    primitive.role == ProjectionPrimitiveRole::BodyBoundary
                }
                SpatialElementRef::Cut { .. }
                | SpatialElementRef::SlotCut { .. }
                | SpatialElementRef::SlotSegment { .. } => {
                    primitive.role == ProjectionPrimitiveRole::CutBoundary
                }
            };
            if !role_ok {
                return Err(source_mismatch(
                    Some(kind),
                    Some(id.clone()),
                    Some(source.clone()),
                    "primitive role does not match its source element",
                ));
            }

            let circular_front = matches!(spatial.body.footprint, SpatialFootprint::Circle { .. })
                && kind == PrincipalViewKind::Front;
            let composite_front =
                matches!(spatial.body.footprint, SpatialFootprint::Composite { .. })
                    && kind == PrincipalViewKind::Front;
            let shape_ok = match source {
                SpatialElementRef::Body => {
                    if circular_front {
                        matches!(primitive.geometry, ProjectedGeometry::Circle { .. })
                    } else if composite_front {
                        false
                    } else {
                        matches!(primitive.geometry, ProjectedGeometry::Segment { .. })
                    }
                }
                SpatialElementRef::BodySegment { .. } => {
                    composite_front
                        && matches!(
                            primitive.geometry,
                            ProjectedGeometry::Segment { .. } | ProjectedGeometry::Arc { .. }
                        )
                }
                SpatialElementRef::Cut { .. } => match kind {
                    PrincipalViewKind::Front => {
                        matches!(primitive.geometry, ProjectedGeometry::Circle { .. })
                    }
                    PrincipalViewKind::Top | PrincipalViewKind::Left => {
                        matches!(primitive.geometry, ProjectedGeometry::Segment { .. })
                    }
                },
                SpatialElementRef::SlotSegment { .. } => {
                    kind == PrincipalViewKind::Front
                        && matches!(
                            primitive.geometry,
                            ProjectedGeometry::Segment { .. } | ProjectedGeometry::Arc { .. }
                        )
                }
                SpatialElementRef::SlotCut { .. } => {
                    kind != PrincipalViewKind::Front
                        && matches!(primitive.geometry, ProjectedGeometry::Segment { .. })
                }
            };
            if !shape_ok {
                return Err(geometry_mismatch(
                    Some(kind),
                    Some(id.clone()),
                    Some(source.clone()),
                    "primitive geometry kind does not match the canonical source/view contract",
                ));
            }
        }

        let body_count = counts
            .iter()
            .filter(|(source, _)| {
                matches!(
                    source,
                    SpatialElementRef::Body | SpatialElementRef::BodySegment { .. }
                )
            })
            .map(|(_, count)| *count)
            .sum::<usize>();
        let expected_body_count =
            if matches!(spatial.body.footprint, SpatialFootprint::Circle { .. })
                && kind == PrincipalViewKind::Front
            {
                2
            } else if matches!(spatial.body.footprint, SpatialFootprint::Composite { .. })
                && kind == PrincipalViewKind::Front
            {
                2 * composite_segment_count(spatial)
            } else {
                8
            };
        if body_count != expected_body_count {
            return Err(view_mismatch(
                Some(kind),
                None,
                format!(
                    "body must contribute exactly {expected_body_count} primitives, found {body_count}"
                ),
            ));
        }

        if matches!(spatial.body.footprint, SpatialFootprint::Composite { .. })
            && kind == PrincipalViewKind::Front
        {
            if counts.contains_key(&SpatialElementRef::Body) {
                return Err(source_mismatch(
                    Some(kind),
                    None,
                    Some(SpatialElementRef::Body),
                    "Front composite profile edges must use BodySegment provenance",
                ));
            }
            if let SpatialFootprint::Composite { segments } = &spatial.body.footprint {
                for segment in segments {
                    let source = SpatialElementRef::BodySegment {
                        index: segment.segment_index(),
                    };
                    let found = counts.get(&source).copied().unwrap_or_default();
                    if found != 2 {
                        return Err(view_mismatch(
                            Some(kind),
                            None,
                            format!(
                                "body segment {} must contribute exactly 2 Front primitives, found {found}",
                                segment.segment_index()
                            ),
                        ));
                    }
                }
            }
        }

        for cut_id in spatial.cuts.keys() {
            let source = SpatialElementRef::Cut { id: cut_id.clone() };
            let expected = if kind == PrincipalViewKind::Front {
                2
            } else {
                4
            };
            let found = counts.get(&source).copied().unwrap_or_default();
            if found != expected {
                return Err(view_mismatch(
                    Some(kind),
                    None,
                    format!(
                        "cut {cut_id} must contribute exactly {expected} primitives, found {found}"
                    ),
                ));
            }
        }

        for (cut_id, slot) in &spatial.slot_cuts {
            if kind == PrincipalViewKind::Front {
                for segment in &slot.segments {
                    let source = SpatialElementRef::SlotSegment {
                        id: cut_id.clone(),
                        index: segment.segment_index(),
                    };
                    let found = counts.get(&source).copied().unwrap_or_default();
                    if found != 2 {
                        return Err(view_mismatch(
                            Some(kind),
                            None,
                            format!(
                                "slot segment {} must contribute exactly 2 Front primitives, found {found}",
                                segment.segment_index()
                            ),
                        ));
                    }
                }
            } else {
                let source = SpatialElementRef::SlotCut { id: cut_id.clone() };
                let found = counts.get(&source).copied().unwrap_or_default();
                if found != 4 {
                    return Err(view_mismatch(
                        Some(kind),
                        None,
                        format!("slot {cut_id} must contribute exactly 4 side primitives, found {found}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct ExpectedPrimitive {
    geometry: ProjectedGeometry,
    depth: ProjectionDepthRange,
}

fn validate_geometry_and_depth(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
) -> Result<(), ProjectionConsistencyError> {
    for kind in PRINCIPAL_VIEWS {
        let view = projection
            .views
            .get(&kind)
            .expect("principal views checked");

        match (kind, &spatial.body.footprint) {
            (PrincipalViewKind::Front, SpatialFootprint::Composite { segments }) => {
                let z0 = spatial.body.z.min;
                let z1 = spatial.body.z.max;
                let frame = frame_for(kind);
                for segment in segments {
                    let source = SpatialElementRef::BodySegment {
                        index: segment.segment_index(),
                    };
                    let expected = vec![
                        expected_profile_segment(frame, segment, z1),
                        expected_profile_segment(frame, segment, z0),
                    ];
                    validate_expected_group(kind, view, projection, &source, &expected)?;
                }
            }
            _ => {
                let body_source = SpatialElementRef::Body;
                let body_expected = expected_body(kind, spatial)?;
                validate_expected_group(kind, view, projection, &body_source, &body_expected)?;
            }
        }

        for (cut_id, cut) in &spatial.cuts {
            let source = SpatialElementRef::Cut { id: cut_id.clone() };
            let expected = expected_cut(kind, cut_id, cut)?;
            validate_expected_group(kind, view, projection, &source, &expected)?;
        }

        for (cut_id, slot) in &spatial.slot_cuts {
            match kind {
                PrincipalViewKind::Front => {
                    let frame = frame_for(kind);
                    for segment in &slot.segments {
                        let source = SpatialElementRef::SlotSegment {
                            id: cut_id.clone(),
                            index: segment.segment_index(),
                        };
                        let expected = vec![
                            expected_profile_segment(frame, segment, slot.z.max),
                            expected_profile_segment(frame, segment, slot.z.min),
                        ];
                        validate_expected_group(kind, view, projection, &source, &expected)?;
                    }
                }
                PrincipalViewKind::Top | PrincipalViewKind::Left => {
                    let source = SpatialElementRef::SlotCut { id: cut_id.clone() };
                    let expected = expected_slot_side(kind, slot)?;
                    validate_expected_group(kind, view, projection, &source, &expected)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_expected_group(
    kind: PrincipalViewKind,
    view: &scc_projection::ProjectionView,
    projection: &ProjectionModel,
    source: &SpatialElementRef,
    expected: &[ExpectedPrimitive],
) -> Result<(), ProjectionConsistencyError> {
    let actual: Vec<(&ProjectionPrimitiveId, &ProjectionPrimitive)> = view
        .primitives
        .iter()
        .filter_map(|(id, primitive)| {
            source_matches(source, projection.sources.element_for_primitive(id))
                .then_some((id, primitive))
        })
        .collect();

    if actual.len() != expected.len() {
        return Err(view_mismatch(
            Some(kind),
            None,
            format!(
                "source {source:?} has {} primitives but expected {}",
                actual.len(),
                expected.len()
            ),
        ));
    }

    let mut matched = vec![false; expected.len()];
    for (id, primitive) in actual {
        let geometry_candidates: Vec<usize> = expected
            .iter()
            .enumerate()
            .filter_map(|(index, wanted)| {
                (!matched[index] && geometry_equal(&primitive.geometry, &wanted.geometry))
                    .then_some(index)
            })
            .collect();

        if geometry_candidates.is_empty() {
            return Err(geometry_mismatch(
                Some(kind),
                Some(id.clone()),
                Some(source.clone()),
                "stored geometry does not match any source-derived candidate",
            ));
        }

        let depth_match = geometry_candidates
            .iter()
            .copied()
            .find(|index| depth_equal(primitive.depth, expected[*index].depth));
        let index = match depth_match {
            Some(index) => index,
            None => {
                return Err(depth_mismatch(
                    Some(kind),
                    Some(id.clone()),
                    Some(source.clone()),
                    "stored depth does not match the source-derived geometry candidate",
                ))
            }
        };
        matched[index] = true;
    }

    if matched.iter().any(|matched| !matched) {
        return Err(geometry_mismatch(
            Some(kind),
            None,
            Some(source.clone()),
            "a source-derived candidate is missing from the projection",
        ));
    }
    Ok(())
}

fn expected_body(
    kind: PrincipalViewKind,
    spatial: &SpatialModel,
) -> Result<Vec<ExpectedPrimitive>, ProjectionConsistencyError> {
    let (x0, x1, y0, y1, z0, z1) = body_coordinates(spatial)?;
    let frame = frame_for(kind);
    if matches!(spatial.body.footprint, SpatialFootprint::Circle { .. })
        && kind == PrincipalViewKind::Front
    {
        let (cx, cy, _) = match spatial.body.footprint {
            SpatialFootprint::Circle { center, radius } => (center.x, center.y, radius),
            SpatialFootprint::Rectangle { .. }
            | SpatialFootprint::Composite { .. }
            | SpatialFootprint::Parametric { .. } => {
                return Err(unsupported(
                    "legacy circular projection received a non-circular body",
                ));
            }
        };
        return Ok(vec![
            expected_circle(frame, point(cx, cy, z0), spatial_body_radius(spatial)?),
            expected_circle(frame, point(cx, cy, z1), spatial_body_radius(spatial)?),
        ]);
    }
    if kind == PrincipalViewKind::Front {
        if let SpatialFootprint::Composite { segments } = &spatial.body.footprint {
            let mut result = Vec::with_capacity(2 * segments.len());
            for z in [z1, z0] {
                for segment in segments {
                    result.push(expected_profile_segment(frame, segment, z));
                }
            }
            return Ok(result);
        }
    }
    let loops: [[PartLocalPoint3; 4]; 2] = match kind {
        PrincipalViewKind::Front => [
            [
                point(x0, y0, z1),
                point(x1, y0, z1),
                point(x1, y1, z1),
                point(x0, y1, z1),
            ],
            [
                point(x0, y0, z0),
                point(x1, y0, z0),
                point(x1, y1, z0),
                point(x0, y1, z0),
            ],
        ],
        PrincipalViewKind::Top => [
            [
                point(x0, y1, z1),
                point(x1, y1, z1),
                point(x1, y1, z0),
                point(x0, y1, z0),
            ],
            [
                point(x0, y0, z1),
                point(x1, y0, z1),
                point(x1, y0, z0),
                point(x0, y0, z0),
            ],
        ],
        PrincipalViewKind::Left => [
            [
                point(x0, y0, z1),
                point(x0, y1, z1),
                point(x0, y1, z0),
                point(x0, y0, z0),
            ],
            [
                point(x1, y0, z1),
                point(x1, y1, z1),
                point(x1, y1, z0),
                point(x1, y0, z0),
            ],
        ],
    };

    let mut result = Vec::with_capacity(8);
    for loop_points in loops {
        for edge in loop_points.windows(2) {
            result.push(expected_segment(frame, edge[0], edge[1]));
        }
        result.push(expected_segment(frame, loop_points[3], loop_points[0]));
    }
    Ok(result)
}

fn expected_slot_side(
    kind: PrincipalViewKind,
    slot: &SlotCut,
) -> Result<Vec<ExpectedPrimitive>, ProjectionConsistencyError> {
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    for segment in &slot.segments {
        let Some((min_x, max_x, min_y, max_y)) = segment.bounds() else {
            return Err(nonfinite(
                Some(kind),
                None,
                "slot segment bounds are not finite",
            ));
        };
        bounds = Some(match bounds {
            None => (min_x, max_x, min_y, max_y),
            Some((x0, x1, y0, y1)) => (x0.min(min_x), x1.max(max_x), y0.min(min_y), y1.max(max_y)),
        });
    }
    let Some((x0, x1, y0, y1)) = bounds else {
        return Err(nonfinite(Some(kind), None, "slot has no bounds"));
    };
    let z0 = slot.z.min;
    let z1 = slot.z.max;
    let frame = frame_for(kind);
    let points = match kind {
        PrincipalViewKind::Front => {
            return Err(view_mismatch(
                Some(kind),
                None,
                "slot silhouette is not a Front primitive",
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
    let mut result = Vec::with_capacity(4);
    for edge in points.windows(2) {
        result.push(expected_segment(frame, edge[0], edge[1]));
    }
    result.push(expected_segment(frame, points[3], points[0]));
    Ok(result)
}

fn expected_cut(
    kind: PrincipalViewKind,
    cut_id: &SpatialCutId,
    cut: &scc_spatial::CylindricalCut,
) -> Result<Vec<ExpectedPrimitive>, ProjectionConsistencyError> {
    let frame = frame_for(kind);
    let cx = cut.center.x;
    let cy = cut.center.y;
    let za = cut.z.min;
    let zb = cut.z.max;

    match kind {
        PrincipalViewKind::Front => Ok([
            expected_circle(frame, point(cx, cy, za), cut.radius),
            expected_circle(frame, point(cx, cy, zb), cut.radius),
        ]
        .into_iter()
        .collect()),
        PrincipalViewKind::Top => {
            let x_minus = finite_sub(cx, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} X extent is not finite"),
                )
            })?;
            let x_plus = finite_add(cx, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} X extent is not finite"),
                )
            })?;
            let y_minus = finite_sub(cy, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} Y extent is not finite"),
                )
            })?;
            let y_plus = finite_add(cy, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} Y extent is not finite"),
                )
            })?;

            Ok(vec![
                expected_segment_with_depth(
                    frame,
                    point(x_minus, cy, za),
                    point(x_plus, cy, za),
                    y_minus,
                    y_plus,
                ),
                expected_segment_with_depth(
                    frame,
                    point(x_minus, cy, zb),
                    point(x_plus, cy, zb),
                    y_minus,
                    y_plus,
                ),
                expected_segment_with_depth(
                    frame,
                    point(x_minus, cy, za),
                    point(x_minus, cy, zb),
                    cy,
                    cy,
                ),
                expected_segment_with_depth(
                    frame,
                    point(x_plus, cy, za),
                    point(x_plus, cy, zb),
                    cy,
                    cy,
                ),
            ])
        }
        PrincipalViewKind::Left => {
            let y_minus = finite_sub(cy, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} Y extent is not finite"),
                )
            })?;
            let y_plus = finite_add(cy, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} Y extent is not finite"),
                )
            })?;
            let x_plus = finite_add(cx, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} X extent is not finite"),
                )
            })?;
            let x_minus = finite_sub(cx, cut.radius).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} X extent is not finite"),
                )
            })?;
            let depth_min = finite_neg(x_plus).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} Left depth is not finite"),
                )
            })?;
            let depth_max = finite_neg(x_minus).ok_or_else(|| {
                nonfinite(
                    Some(kind),
                    None,
                    format!("cut {cut_id} Left depth is not finite"),
                )
            })?;

            Ok(vec![
                expected_segment_with_depth(
                    frame,
                    point(cx, y_minus, za),
                    point(cx, y_plus, za),
                    depth_min,
                    depth_max,
                ),
                expected_segment_with_depth(
                    frame,
                    point(cx, y_minus, zb),
                    point(cx, y_plus, zb),
                    depth_min,
                    depth_max,
                ),
                expected_segment_with_depth(
                    frame,
                    point(cx, y_minus, za),
                    point(cx, y_minus, zb),
                    -cx,
                    -cx,
                ),
                expected_segment_with_depth(
                    frame,
                    point(cx, y_plus, za),
                    point(cx, y_plus, zb),
                    -cx,
                    -cx,
                ),
            ])
        }
    }
}

fn spatial_body_radius(spatial: &SpatialModel) -> Result<f64, ProjectionConsistencyError> {
    match spatial.body.footprint {
        SpatialFootprint::Circle { radius, .. } => Ok(radius),
        SpatialFootprint::Rectangle { .. }
        | SpatialFootprint::Composite { .. }
        | SpatialFootprint::Parametric { .. } => Err(nonfinite(
            None,
            None,
            "circular body radius requested for unsupported body profile",
        )),
    }
}

fn body_coordinates(
    spatial: &SpatialModel,
) -> Result<(f64, f64, f64, f64, f64, f64), ProjectionConsistencyError> {
    let (x0, x1, y0, y1) = match &spatial.body.footprint {
        SpatialFootprint::Rectangle {
            origin,
            width,
            height,
        } => {
            let x1 = finite_add(origin.x, *width)
                .ok_or_else(|| nonfinite(None, None, "body X extent is not finite"))?;
            let y1 = finite_add(origin.y, *height)
                .ok_or_else(|| nonfinite(None, None, "body Y extent is not finite"))?;
            (origin.x, x1, origin.y, y1)
        }
        SpatialFootprint::Circle { center, radius } => {
            let x0 = finite_sub(center.x, *radius)
                .ok_or_else(|| nonfinite(None, None, "circular body X extent is not finite"))?;
            let x1 = finite_add(center.x, *radius)
                .ok_or_else(|| nonfinite(None, None, "circular body X extent is not finite"))?;
            let y0 = finite_sub(center.y, *radius)
                .ok_or_else(|| nonfinite(None, None, "circular body Y extent is not finite"))?;
            let y1 = finite_add(center.y, *radius)
                .ok_or_else(|| nonfinite(None, None, "circular body Y extent is not finite"))?;
            (x0, x1, y0, y1)
        }
        SpatialFootprint::Parametric { .. } => {
            return Err(unsupported(
                "legacy body coordinates requested for parametric evidence",
            ));
        }
        SpatialFootprint::Composite { segments } => {
            let mut bounds: Option<(f64, f64, f64, f64)> = None;
            for segment in segments {
                let (sx0, sx1, sy0, sy1) = segment.bounds().ok_or_else(|| {
                    nonfinite(None, None, "composite body segment bounds are not finite")
                })?;
                bounds = Some(match bounds {
                    None => (sx0, sx1, sy0, sy1),
                    Some((bx0, bx1, by0, by1)) => {
                        (bx0.min(sx0), bx1.max(sx1), by0.min(sy0), by1.max(sy1))
                    }
                });
            }
            bounds.ok_or_else(|| nonfinite(None, None, "composite body has no bounds"))?
        }
    };
    Ok((x0, x1, y0, y1, spatial.body.z.min, spatial.body.z.max))
}

fn expected_segment(
    frame: OrthographicFrame,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
) -> ExpectedPrimitive {
    let a = frame.project_point(start);
    let b = frame.project_point(end);
    ExpectedPrimitive {
        geometry: ProjectedGeometry::Segment {
            start: ProjectedPoint2 { u: a.u, v: a.v },
            end: ProjectedPoint2 { u: b.u, v: b.v },
        },
        depth: ProjectionDepthRange {
            min: a.depth.min(b.depth),
            max: a.depth.max(b.depth),
        },
    }
}

fn expected_segment_with_depth(
    frame: OrthographicFrame,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
    depth_min: f64,
    depth_max: f64,
) -> ExpectedPrimitive {
    let mut expected = expected_segment(frame, start, end);
    expected.depth = ProjectionDepthRange {
        min: depth_min,
        max: depth_max,
    };
    expected
}

fn expected_circle(
    frame: OrthographicFrame,
    center: PartLocalPoint3,
    radius: f64,
) -> ExpectedPrimitive {
    let p = frame.project_point(center);
    ExpectedPrimitive {
        geometry: ProjectedGeometry::Circle {
            center: ProjectedPoint2 { u: p.u, v: p.v },
            radius,
        },
        depth: ProjectionDepthRange {
            min: p.depth,
            max: p.depth,
        },
    }
}

fn expected_profile_segment(
    frame: OrthographicFrame,
    segment: &SpatialProfileSegment,
    z: f64,
) -> ExpectedPrimitive {
    match segment {
        SpatialProfileSegment::Line { start, end, .. } => {
            expected_segment(frame, point(start.x, start.y, z), point(end.x, end.y, z))
        }
        SpatialProfileSegment::Arc {
            start,
            end,
            center,
            radius,
            direction,
            ..
        } => expected_arc(
            frame,
            point(center.x, center.y, z),
            point(start.x, start.y, z),
            point(end.x, end.y, z),
            *radius,
            *direction,
        ),
    }
}

fn expected_arc(
    frame: OrthographicFrame,
    center: PartLocalPoint3,
    start: PartLocalPoint3,
    end: PartLocalPoint3,
    radius: f64,
    direction: SpatialArcDirection,
) -> ExpectedPrimitive {
    let projected_center = frame.project_point(center);
    let projected_start = frame.project_point(start);
    let projected_end = frame.project_point(end);
    let start_angle =
        (projected_start.v - projected_center.v).atan2(projected_start.u - projected_center.u);
    let end_angle =
        (projected_end.v - projected_center.v).atan2(projected_end.u - projected_center.u);
    let (start_angle, end_angle) = match direction {
        SpatialArcDirection::CounterClockwise => (start_angle, end_angle),
        SpatialArcDirection::Clockwise => (end_angle, start_angle),
    };
    ExpectedPrimitive {
        geometry: ProjectedGeometry::Arc {
            center: ProjectedPoint2 {
                u: projected_center.u,
                v: projected_center.v,
            },
            radius,
            start_angle,
            end_angle,
        },
        depth: ProjectionDepthRange {
            min: projected_center.depth,
            max: projected_center.depth,
        },
    }
}

fn validate_visibility_invariants(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
) -> Result<(), ProjectionConsistencyError> {
    for kind in PRINCIPAL_VIEWS {
        let view = projection
            .views
            .get(&kind)
            .expect("principal views checked");
        validate_body_visibility(
            kind,
            view,
            projection,
            visibility,
            matches!(spatial.body.footprint, SpatialFootprint::Circle { .. }),
        )?;
        for (cut_id, cut) in &spatial.cuts {
            match kind {
                PrincipalViewKind::Front => {
                    validate_front_cut_visibility(
                        kind,
                        view,
                        projection,
                        visibility,
                        spatial.body.z.max,
                        cut_id,
                        cut,
                    )?;
                }
                PrincipalViewKind::Top | PrincipalViewKind::Left => {
                    validate_side_cut_visibility(kind, view, projection, visibility, cut_id)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_body_visibility(
    kind: PrincipalViewKind,
    view: &scc_projection::ProjectionView,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
    circular_body: bool,
) -> Result<(), ProjectionConsistencyError> {
    let ids = ids_for_source(view, projection, &SpatialElementRef::Body);
    if circular_body && kind == PrincipalViewKind::Front {
        if ids.len() != 2
            || ids.iter().any(|id| {
                !matches!(
                    view.primitives[id].geometry,
                    ProjectedGeometry::Circle { .. }
                )
            })
        {
            return Err(visibility_mismatch(
                Some(kind),
                None,
                "circular Front body must contain two circle candidates",
            ));
        }
        if !geometry_equal(
            &view.primitives[&ids[0]].geometry,
            &view.primitives[&ids[1]].geometry,
        ) {
            return Err(visibility_mismatch(
                Some(kind),
                Some(ids[0].clone()),
                "circular Front body candidates must be coincident",
            ));
        }
        let first_depth = view.primitives[&ids[0]].depth.max;
        let second_depth = view.primitives[&ids[1]].depth.max;
        if approximately_equal(first_depth, second_depth) {
            return Err(visibility_mismatch(
                Some(kind),
                Some(ids[0].clone()),
                "circular body candidates do not have a unique depth winner",
            ));
        }
        let (winner, loser) = if first_depth > second_depth {
            (&ids[0], &ids[1])
        } else {
            (&ids[1], &ids[0])
        };
        expect_class(
            visibility,
            kind,
            winner,
            VisibilityClass::Visible,
            "nearest circular body candidate must be Visible",
        )?;
        expect_class(
            visibility,
            kind,
            loser,
            VisibilityClass::Occluded,
            "far circular body candidate must be Occluded",
        )?;
        return Ok(());
    }
    let mut remaining: BTreeSet<ProjectionPrimitiveId> = ids.iter().cloned().collect();

    while let Some(first) = remaining.iter().next().cloned() {
        remaining.remove(&first);
        let first_primitive = &view.primitives[&first];
        let group: Vec<ProjectionPrimitiveId> = remaining
            .iter()
            .filter(|id| geometry_equal(&first_primitive.geometry, &view.primitives[id].geometry))
            .cloned()
            .collect();

        if group.len() != 1 {
            return Err(visibility_mismatch(
                Some(kind),
                Some(first),
                "body geometry must have exactly one coincident near/far partner",
            ));
        }
        let second = group[0].clone();
        remaining.remove(&second);

        let first_depth = view.primitives[&first].depth.max;
        let second_depth = view.primitives[&second].depth.max;
        if approximately_equal(first_depth, second_depth) {
            return Err(visibility_mismatch(
                Some(kind),
                Some(first),
                "coincident body candidates do not have a unique depth winner",
            ));
        }

        let (winner, loser) = if first_depth > second_depth {
            (&first, &second)
        } else {
            (&second, &first)
        };
        expect_class(
            visibility,
            kind,
            winner,
            VisibilityClass::Visible,
            "nearest coincident body candidate must be Visible",
        )?;
        expect_class(
            visibility,
            kind,
            loser,
            VisibilityClass::Occluded,
            "far coincident body candidate must be Occluded",
        )?;
    }
    Ok(())
}

fn validate_front_cut_visibility(
    kind: PrincipalViewKind,
    view: &scc_projection::ProjectionView,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
    body_z_max: f64,
    cut_id: &SpatialCutId,
    cut: &scc_spatial::CylindricalCut,
) -> Result<(), ProjectionConsistencyError> {
    let source = SpatialElementRef::Cut { id: cut_id.clone() };
    let ids = ids_for_source(view, projection, &source);
    let first = ids[0].clone();
    let second = ids[1].clone();
    let first_depth = view.primitives[&first].depth.max;
    let second_depth = view.primitives[&second].depth.max;
    if approximately_equal(first_depth, second_depth) {
        return Err(visibility_mismatch(
            Some(kind),
            Some(first),
            "coincident cut circles do not have a unique depth winner",
        ));
    }
    let nearest_is_first = first_depth > second_depth;
    let nearest = if nearest_is_first {
        first.clone()
    } else {
        second.clone()
    };
    let farther = if nearest_is_first { second } else { first };
    let nearest_class = if approximately_equal(cut.z.max, body_z_max) {
        VisibilityClass::Visible
    } else {
        VisibilityClass::Hidden
    };
    expect_class(
        visibility,
        kind,
        &nearest,
        nearest_class,
        "nearest Front cut circle disagrees with the resolved top-face invariant",
    )?;
    expect_class(
        visibility,
        kind,
        &farther,
        VisibilityClass::Occluded,
        "farther Front cut circle must be Occluded",
    )?;
    Ok(())
}

fn validate_side_cut_visibility(
    kind: PrincipalViewKind,
    view: &scc_projection::ProjectionView,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
    cut_id: &SpatialCutId,
) -> Result<(), ProjectionConsistencyError> {
    let source = SpatialElementRef::Cut { id: cut_id.clone() };
    let body_ids = ids_for_source(view, projection, &SpatialElementRef::Body);
    let visible_body_ids: Vec<ProjectionPrimitiveId> = body_ids
        .into_iter()
        .filter(|id| visibility.class_for(kind, id) == Some(VisibilityClass::Visible))
        .collect();

    for id in ids_for_source(view, projection, &source) {
        let primitive = &view.primitives[&id];
        let expected = if primitive.depth.is_point() {
            VisibilityClass::Hidden
        } else {
            let contained = visible_body_ids.iter().any(|body_id| {
                segments_contained(&view.primitives[body_id].geometry, &primitive.geometry)
            });
            if contained {
                VisibilityClass::Occluded
            } else {
                VisibilityClass::Hidden
            }
        };
        expect_class(
            visibility,
            kind,
            &id,
            expected,
            "side-view cut class disagrees with tangent/rim occlusion invariant",
        )?;
    }
    Ok(())
}

fn expect_class(
    visibility: &VisibilityModel,
    kind: PrincipalViewKind,
    id: &ProjectionPrimitiveId,
    expected: VisibilityClass,
    detail: &str,
) -> Result<(), ProjectionConsistencyError> {
    let actual = visibility.class_for(kind, id).ok_or_else(|| {
        view_mismatch(
            Some(kind),
            Some(id.clone()),
            "visibility decision disappeared after coverage validation",
        )
    })?;
    if actual != expected {
        return Err(visibility_mismatch(
            Some(kind),
            Some(id.clone()),
            format!("{detail}; expected {expected:?}, found {actual:?}"),
        ));
    }
    Ok(())
}

fn ids_for_source(
    view: &scc_projection::ProjectionView,
    projection: &ProjectionModel,
    source: &SpatialElementRef,
) -> Vec<ProjectionPrimitiveId> {
    view.primitives
        .keys()
        .filter(|id| source_matches(source, projection.sources.element_for_primitive(id)))
        .cloned()
        .collect()
}

fn source_matches(requested: &SpatialElementRef, actual: Option<&SpatialElementRef>) -> bool {
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

fn composite_segment_count(spatial: &SpatialModel) -> usize {
    match &spatial.body.footprint {
        SpatialFootprint::Composite { segments } => segments.len(),
        _ => 0,
    }
}

fn geometry_equal(a: &ProjectedGeometry, b: &ProjectedGeometry) -> bool {
    match (a, b) {
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
            (point_equal(*a_start, *b_start) && point_equal(*a_end, *b_end))
                || (point_equal(*a_start, *b_end) && point_equal(*a_end, *b_start))
        }
        (
            ProjectedGeometry::Circle {
                center: a_center,
                radius: a_radius,
            },
            ProjectedGeometry::Circle {
                center: b_center,
                radius: b_radius,
            },
        ) => point_equal(*a_center, *b_center) && approximately_equal(*a_radius, *b_radius),
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
            point_equal(*a_center, *b_center)
                && approximately_equal(*a_radius, *b_radius)
                && normalized_angle_equal(*a_start, *b_start)
                && normalized_angle_equal(*a_end, *b_end)
        }
        _ => false,
    }
}

fn normalized_angle_equal(a: f64, b: f64) -> bool {
    let tau = std::f64::consts::TAU;
    let delta = (a.rem_euclid(tau) - b.rem_euclid(tau)).abs();
    delta <= scc_visibility::VISIBILITY_TOLERANCE_MM
        || tau - delta <= scc_visibility::VISIBILITY_TOLERANCE_MM
}

fn depth_equal(a: ProjectionDepthRange, b: ProjectionDepthRange) -> bool {
    approximately_equal(a.min, b.min) && approximately_equal(a.max, b.max)
}

fn point_equal(a: ProjectedPoint2, b: ProjectedPoint2) -> bool {
    approximately_equal(a.u, b.u) && approximately_equal(a.v, b.v)
}

fn segments_contained(container: &ProjectedGeometry, candidate: &ProjectedGeometry) -> bool {
    let (
        ProjectedGeometry::Segment { start: cs, end: ce },
        ProjectedGeometry::Segment { start: ps, end: pe },
    ) = (container, candidate)
    else {
        return false;
    };

    let container_horizontal = approximately_equal(cs.v, ce.v);
    let candidate_horizontal = approximately_equal(ps.v, pe.v);
    if container_horizontal && candidate_horizontal && approximately_equal(cs.v, ps.v) {
        return interval_contains(cs.u, ce.u, ps.u, pe.u);
    }

    let container_vertical = approximately_equal(cs.u, ce.u);
    let candidate_vertical = approximately_equal(ps.u, pe.u);
    if container_vertical && candidate_vertical && approximately_equal(cs.u, ps.u) {
        return interval_contains(cs.v, ce.v, ps.v, pe.v);
    }
    false
}

fn interval_contains(a: f64, b: f64, x: f64, y: f64) -> bool {
    let (min_a, max_a) = if a <= b { (a, b) } else { (b, a) };
    let (min_x, max_x) = if x <= y { (x, y) } else { (y, x) };
    (min_x > min_a || approximately_equal(min_x, min_a))
        && (max_x < max_a || approximately_equal(max_x, max_a))
}

fn frame_for(kind: PrincipalViewKind) -> OrthographicFrame {
    match kind {
        PrincipalViewKind::Front => OrthographicFrame::front(),
        PrincipalViewKind::Top => OrthographicFrame::top(),
        PrincipalViewKind::Left => OrthographicFrame::left(),
    }
}

fn point(x: f64, y: f64, z: f64) -> PartLocalPoint3 {
    PartLocalPoint3 { x, y, z }
}

fn approximately_equal(a: f64, b: f64) -> bool {
    approximately_equal_mm(a, b)
}

fn finite_add(a: f64, b: f64) -> Option<f64> {
    let value = a + b;
    value.is_finite().then_some(value)
}

fn finite_sub(a: f64, b: f64) -> Option<f64> {
    let value = a - b;
    value.is_finite().then_some(value)
}

fn finite_neg(value: f64) -> Option<f64> {
    let value = -value;
    value.is_finite().then_some(value)
}

fn map_projection_error(error: ProjectionError) -> ProjectionConsistencyError {
    match error {
        ProjectionError::MissingProjectionView { kind } => {
            view_mismatch(Some(kind), None, "projection view is missing")
        }
        ProjectionError::InvalidProjectionSource { id } => source_mismatch(
            None,
            Some(id),
            None,
            "projection source map does not resolve to SpatialModel",
        ),
        ProjectionError::InvalidProjectionDepthRange { min, max }
            if !min.is_finite() || !max.is_finite() =>
        {
            nonfinite(None, None, "projection depth contains a non-finite value")
        }
        ProjectionError::InvalidProjectionGeometry {
            reason: scc_projection::InvalidProjectionGeometryReason::NonFiniteCoordinate,
        } => nonfinite(
            None,
            None,
            "projection geometry contains a non-finite value",
        ),
        ProjectionError::InvalidProjectionBounds { bounds }
            if !bounds.min_u.is_finite()
                || !bounds.max_u.is_finite()
                || !bounds.min_v.is_finite()
                || !bounds.max_v.is_finite() =>
        {
            nonfinite(None, None, "projection bounds contain a non-finite value")
        }
        ProjectionError::InvalidSpatialModel {
            reason:
                scc_projection::InvalidSpatialModelReason::NonFiniteBodyValue
                | scc_projection::InvalidSpatialModelReason::NonFiniteCutValue
                | scc_projection::InvalidSpatialModelReason::BodyCoordinateOverflow,
        } => nonfinite(
            None,
            None,
            "projection preflight found a non-finite spatial state",
        ),
        other => invalid_input(ConsistencyModel::Projection, other.to_string()),
    }
}

fn map_visibility_error(error: VisibilityError) -> ProjectionConsistencyError {
    match error {
        VisibilityError::ProjectionInput { error } => map_projection_error(error),
        VisibilityError::UnsupportedVisibilityConfiguration {
            reason: scc_visibility::UnsupportedVisibilityReason::NumericOverflow { context },
        } => nonfinite(None, None, context),
        VisibilityError::UnsupportedVisibilityConfiguration { reason } => {
            unsupported(reason.to_string())
        }
        VisibilityError::AmbiguousCoincidentCandidates { view, ids } => visibility_mismatch(
            Some(view),
            ids.into_iter().next(),
            "visibility has no unique coincident depth winner",
        ),
        VisibilityError::MissingVisibilityDecision { view, id } => {
            view_mismatch(Some(view), Some(id), "visibility decision is missing")
        }
        VisibilityError::InvalidVisibilityDecision { view, id, reason } => {
            view_mismatch(view, id, reason)
        }
        VisibilityError::InvalidVisibilityInput { reason } => match reason {
            scc_visibility::InvalidVisibilityInputReason::NumericOverflow(detail) => {
                nonfinite(None, None, detail)
            }
            scc_visibility::InvalidVisibilityInputReason::RoleSourceMismatch(detail) => {
                source_mismatch(None, None, None, detail)
            }
            scc_visibility::InvalidVisibilityInputReason::UnexpectedGeometry(detail) => {
                geometry_mismatch(None, None, None, detail)
            }
            scc_visibility::InvalidVisibilityInputReason::UnexpectedPrimitiveCount(detail) => {
                view_mismatch(None, None, detail)
            }
            scc_visibility::InvalidVisibilityInputReason::MalformedSpatialModel(detail) => {
                if detail.contains("non-finite") || detail.contains("overflowed") {
                    nonfinite(None, None, detail)
                } else {
                    invalid_input(ConsistencyModel::Spatial, detail)
                }
            }
            scc_visibility::InvalidVisibilityInputReason::MalformedProjectionModel(detail) => {
                if detail.contains("non-finite") || detail.contains("overflowed") {
                    nonfinite(None, None, detail)
                } else {
                    invalid_input(ConsistencyModel::Projection, detail)
                }
            }
        },
    }
}

fn invalid_input(model: ConsistencyModel, detail: impl Into<String>) -> ProjectionConsistencyError {
    ProjectionConsistencyError::InvalidInputModel {
        model,
        detail: detail.into(),
    }
}

fn unsupported(detail: impl Into<String>) -> ProjectionConsistencyError {
    ProjectionConsistencyError::UnsupportedTopology {
        detail: detail.into(),
    }
}

fn view_mismatch(
    view: Option<PrincipalViewKind>,
    primitive: Option<ProjectionPrimitiveId>,
    detail: impl Into<String>,
) -> ProjectionConsistencyError {
    ProjectionConsistencyError::ViewOrPrimitiveMismatch {
        view,
        primitive,
        detail: detail.into(),
    }
}

fn source_mismatch(
    view: Option<PrincipalViewKind>,
    primitive: Option<ProjectionPrimitiveId>,
    source: Option<SpatialElementRef>,
    detail: impl Into<String>,
) -> ProjectionConsistencyError {
    ProjectionConsistencyError::SourceOrRoleMismatch {
        view,
        primitive,
        element: source,
        detail: detail.into(),
    }
}

fn geometry_mismatch(
    view: Option<PrincipalViewKind>,
    primitive: Option<ProjectionPrimitiveId>,
    source: Option<SpatialElementRef>,
    detail: impl Into<String>,
) -> ProjectionConsistencyError {
    ProjectionConsistencyError::GeometryMismatch {
        view,
        primitive,
        element: source,
        detail: detail.into(),
    }
}

fn depth_mismatch(
    view: Option<PrincipalViewKind>,
    primitive: Option<ProjectionPrimitiveId>,
    source: Option<SpatialElementRef>,
    detail: impl Into<String>,
) -> ProjectionConsistencyError {
    ProjectionConsistencyError::DepthMismatch {
        view,
        primitive,
        element: source,
        detail: detail.into(),
    }
}

fn visibility_mismatch(
    view: Option<PrincipalViewKind>,
    primitive: Option<ProjectionPrimitiveId>,
    detail: impl Into<String>,
) -> ProjectionConsistencyError {
    ProjectionConsistencyError::VisibilityMismatch {
        view,
        primitive,
        detail: detail.into(),
    }
}

fn nonfinite(
    view: Option<PrincipalViewKind>,
    primitive: Option<ProjectionPrimitiveId>,
    detail: impl Into<String>,
) -> ProjectionConsistencyError {
    ProjectionConsistencyError::NonFiniteNumericState {
        view,
        primitive,
        detail: detail.into(),
    }
}
