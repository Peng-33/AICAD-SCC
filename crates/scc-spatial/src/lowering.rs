//! Deterministic lowering from engineering spatial intent plus solved
//! geometry into a read-only [`crate::SpatialModel`] (Phase 9.2, ADR-033).
//!
//! `lower_spatial` is a pure, side-effect-free function of its inputs: no
//! network, no time, no randomness, no global state, no mutation. It re-reads
//! the authoritative sources on every call — the [`EngineeringModel`] (intent),
//! the [`GeometryModel`] (solved positions), and the
//! [`EngineeringSourceMap`] (provenance) — and never caches derived values
//! (ADR-033 Decision 3).
//!
//! Order of gates (fail-closed, ADR-033 Decision 2):
//! 1. the single-owner engineering readiness check (`scc-engineering`,
//!    ADR-032) — never re-implemented here;
//! 2. thickness resolution (single-sourced Phase 9.1 rule);
//! 3. exactly one `BaseProfile`, solved into exactly one `Rectangle`;
//! 4. per hole-bearing feature: cardinality, per-entity circle validation,
//!    provenance, diameter witness, and Z-interval rules;
//! 5. claim integrity: every solved geometry entity is claimed exactly once;
//! 6. bounds derived from the body footprint only.
//!
//! Pattern templates: for array-backed patterns the solver removes the
//! template entity, so the source map contains one more entry than the solved
//! geometry (`{feature}` template + `{feature}_array[i]` instances). The
//! lowering works on `solved_entities` — the deterministic intersection of
//! the source-map entries with the geometry keys — and treats template-only
//! entries as expected, claim-free artifacts (ADR-033 Decision 5, as amended).

use std::collections::{BTreeMap, BTreeSet};

use scc_core::ids::EntityId;
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::feature::{
    ArcDirection, BaseProfile, EngineeringFeature, HolePattern, ProfileSegment,
};
use scc_engineering::source_map::{EngineeringSourceMap, ProfileSegmentSemantic};
use scc_engineering::{
    EngineeringModel, FeatureId, HoleExtent, HoleStartFace, Point2D, SourcedLength,
};
use scc_geometry::{GeometryModel, GeometryPrimitive, Point2};

use crate::error::{
    InvalidBaseReason, InvalidGeometryReason, InvalidSpatialSourceReason, PrimitiveKind,
    SpatialError,
};
use crate::model::{
    Bounds3D, CylindricalCut, SpatialArcDirection, SpatialBaseProfile, SpatialBody, SpatialCutId,
    SpatialCutKind, SpatialFootprint, SpatialModel, SpatialModelSource, SpatialProfileSegment,
    ZInterval,
};

/// Tolerance (mm) for the diameter witness between engineering intent and
/// solved geometry (ADR-033 Decision 7). Exactly at the tolerance passes;
/// beyond it fails.
pub(crate) const SPATIAL_LENGTH_TOLERANCE: f64 = 1.0e-9;

/// Read-only lowering inputs: the three authoritative sources.
#[derive(Debug, Clone, Copy)]
pub struct SpatialInputs<'a> {
    /// The authoritative engineering design state (intent + spatial data).
    pub engineering: &'a EngineeringModel,
    /// The authoritative solved geometry (positions).
    pub geometry: &'a GeometryModel,
    /// The authoritative feature → entity provenance map.
    pub source_map: &'a EngineeringSourceMap,
}

/// Lower engineering spatial intent plus solved geometry into a
/// [`SpatialModel`]. Pure and deterministic: identical inputs always produce
/// an identical model (byte-stable serialization).
pub fn lower_spatial(inputs: SpatialInputs<'_>) -> Result<SpatialModel, SpatialError> {
    let SpatialInputs {
        engineering,
        geometry,
        source_map,
    } = inputs;

    // 1. Single-owner readiness gate (ADR-032) — never re-implemented here.
    let readiness = engineering.validate_spatial_readiness();
    if !readiness.ready {
        return Err(SpatialError::EngineeringNotSpatiallyReady {
            count: readiness.issues.len(),
            issues: readiness.issues,
        });
    }

    // 2. Thickness: re-resolved every run via the single-sourced Phase 9.1
    //    rule. Readiness guarantees presence and resolvability, so failure
    //    here is defensive only.
    let thickness = engineering
        .spatial
        .thickness
        .as_ref()
        .and_then(|source| engineering.resolve_sourced_length_mm(source))
        .ok_or(SpatialError::InvalidThicknessDefensive)?;

    // 3. Exactly one BaseProfile, in deterministic (BTreeMap) order.
    let bases: Vec<&EngineeringFeature> = engineering
        .features
        .values()
        .filter(|feature| matches!(feature, EngineeringFeature::BaseProfile { .. }))
        .collect();
    let base = match bases.as_slice() {
        [] => return Err(SpatialError::MissingBaseRepresentation),
        [only] => *only,
        [first, ..] => {
            return Err(SpatialError::InvalidBaseRepresentation {
                feature: first.id().clone(),
                reason: InvalidBaseReason::NotUnique,
            })
        }
    };
    let base_id = base.id().clone();

    // 4. The body: a legacy rectangle/circle has one solved entity; a typed
    // rounded/composite profile lowers to one ordered solved entity per
    // profile segment. The first entity remains the compatibility
    // representative on SpatialBody; every segment entity is claimed below.
    let profile = match base {
        EngineeringFeature::BaseProfile { profile, .. } => profile,
        _ => unreachable!("base list only contains BaseProfile features"),
    };
    let base_solved = solved_entities(source_map, geometry, &base_id);
    let (body_entity, body_entities, footprint) = match profile {
        BaseProfile::Rectangle { .. } => {
            let body_entity = unique_base_entity(&base_id, &base_solved)?;
            let footprint = match base_primitive(geometry, &body_entity) {
                Some(GeometryPrimitive::Rectangle {
                    origin,
                    width,
                    height,
                }) => SpatialFootprint::Rectangle {
                    origin: *origin,
                    width: *width,
                    height: *height,
                },
                None => {
                    return Err(SpatialError::InvalidSpatialSource {
                        feature: base_id.clone(),
                        entity: body_entity,
                        reason: InvalidSpatialSourceReason::NotInGeometry,
                    });
                }
                Some(_) => {
                    return Err(SpatialError::InvalidBaseRepresentation {
                        feature: base_id.clone(),
                        reason: InvalidBaseReason::NotRectangle,
                    });
                }
            };
            (body_entity.clone(), vec![body_entity], footprint)
        }
        BaseProfile::Circle { .. } => {
            let body_entity = unique_base_entity(&base_id, &base_solved)?;
            let footprint = match base_primitive(geometry, &body_entity) {
                Some(GeometryPrimitive::Circle { center, radius }) => SpatialFootprint::Circle {
                    center: *center,
                    radius: *radius,
                },
                None => {
                    return Err(SpatialError::InvalidSpatialSource {
                        feature: base_id.clone(),
                        entity: body_entity,
                        reason: InvalidSpatialSourceReason::NotInGeometry,
                    });
                }
                Some(_) => {
                    return Err(SpatialError::InvalidBaseRepresentation {
                        feature: base_id.clone(),
                        reason: InvalidBaseReason::NotCircle,
                    });
                }
            };
            (body_entity.clone(), vec![body_entity], footprint)
        }
        BaseProfile::RoundedRectangle { .. }
        | BaseProfile::Composite { .. }
        | BaseProfile::Polygon { .. } => {
            let profile_segments =
                profile
                    .segments()
                    .map_err(|_| SpatialError::InvalidBaseRepresentation {
                        feature: base_id.clone(),
                        reason: InvalidBaseReason::UnsupportedProfile,
                    })?;
            let solved = match profile {
                BaseProfile::Composite { .. } => ordered_composite_entities(
                    source_map,
                    geometry,
                    &base_id,
                    profile_segments.len(),
                )?,
                BaseProfile::Polygon { .. } => ordered_polygon_entities(
                    source_map,
                    geometry,
                    &base_id,
                    profile_segments.len(),
                )?,
                BaseProfile::RoundedRectangle { .. } => base_solved.into_iter().collect(),
                _ => unreachable!("matched profile variants above"),
            };
            if solved.len() != profile_segments.len() || solved.is_empty() {
                return Err(SpatialError::InvalidBaseRepresentation {
                    feature: base_id.clone(),
                    reason: InvalidBaseReason::UnsupportedProfile,
                });
            }
            let segments = lower_profile_segments(&base_id, &profile_segments, &solved, geometry)?;
            let body_entity = solved[0].clone();
            (
                body_entity,
                solved,
                SpatialFootprint::Composite { segments },
            )
        }
    };
    let profile = match (profile, &footprint) {
        (
            BaseProfile::Rectangle { .. },
            SpatialFootprint::Rectangle {
                origin,
                width,
                height,
            },
        ) => SpatialBaseProfile::Rectangle {
            origin: *origin,
            width: *width,
            height: *height,
        },
        (BaseProfile::Circle { .. }, SpatialFootprint::Circle { center, radius }) => {
            SpatialBaseProfile::Circle {
                center: *center,
                radius: *radius,
            }
        }
        (
            BaseProfile::RoundedRectangle {
                center,
                width,
                height,
                corner_radius,
            },
            SpatialFootprint::Composite { .. },
        ) => SpatialBaseProfile::RoundedRectangle {
            center: Point2 {
                x: center.x,
                y: center.y,
            },
            width: *width,
            height: *height,
            corner_radius: *corner_radius,
        },
        (BaseProfile::Composite { .. }, SpatialFootprint::Composite { .. }) => {
            SpatialBaseProfile::Composite
        }
        (BaseProfile::Polygon { .. }, SpatialFootprint::Composite { .. }) => {
            SpatialBaseProfile::Polygon
        }
        _ => {
            return Err(SpatialError::InvalidBaseRepresentation {
                feature: base_id.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            });
        }
    };
    match &footprint {
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
                return Err(SpatialError::InvalidBaseRepresentation {
                    feature: base_id.clone(),
                    reason: InvalidBaseReason::NonFiniteValue,
                });
            }
            if *width <= 0.0 || *height <= 0.0 {
                return Err(SpatialError::InvalidBaseRepresentation {
                    feature: base_id.clone(),
                    reason: InvalidBaseReason::NonPositiveValue,
                });
            }
        }
        SpatialFootprint::Circle { center, radius } => {
            if !center.x.is_finite() || !center.y.is_finite() || !radius.is_finite() {
                return Err(SpatialError::InvalidBaseRepresentation {
                    feature: base_id.clone(),
                    reason: InvalidBaseReason::NonFiniteValue,
                });
            }
            if *radius <= 0.0 {
                return Err(SpatialError::InvalidBaseRepresentation {
                    feature: base_id.clone(),
                    reason: InvalidBaseReason::NonPositiveValue,
                });
            }
        }
        SpatialFootprint::Composite { .. } => {
            validate_composite_footprint(&footprint).map_err(|_| {
                SpatialError::InvalidBaseRepresentation {
                    feature: base_id.clone(),
                    reason: InvalidBaseReason::UnsupportedProfile,
                }
            })?;
        }
        SpatialFootprint::Parametric { .. } => {
            return Err(SpatialError::InvalidBaseRepresentation {
                feature: base_id.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            });
        }
    }

    let body = SpatialBody {
        footprint,
        profile,
        z: ZInterval::new(0.0, thickness)?,
        source_feature: base_id.clone(),
        source_entity: body_entity,
    };

    // 6. Every remaining feature, in deterministic (BTreeMap) order.
    let inverse = source_map.entity_to_feature();
    let mut cuts = BTreeMap::new();
    let mut slot_cuts = BTreeMap::new();
    let mut construction_circles = BTreeMap::new();

    for (feature_id, feature) in &engineering.features {
        match feature {
            EngineeringFeature::BaseProfile { .. } => continue,

            EngineeringFeature::ConstructionCircle { .. } => {
                // Reference-only circles: validated as circles, provenance
                // cross-checked, and recorded for integrity. No witness in
                // Phase 9.2 (ADR-033 Decision 8, policy B).
                for entity in source_map.entities_for(feature_id) {
                    cut_circle(geometry, feature_id, &entity)?;
                    if inverse.get(&entity) != Some(feature_id) {
                        return Err(SpatialError::InvalidSpatialSource {
                            feature: feature_id.clone(),
                            entity: entity.clone(),
                            reason: InvalidSpatialSourceReason::ForeignFeature,
                        });
                    }
                    construction_circles.insert(entity, feature_id.clone());
                }
            }

            EngineeringFeature::Hole { diameter, .. } => {
                // A single hole maps to exactly one solved entity. An empty
                // raw mapping means no provenance at all; a single mapped
                // entity absent from geometry is `NotInGeometry` (amendment
                // §4-A rule 2); more than one mapped entity breaks the
                // cardinality contract.
                let mapped = source_map.entities_for(feature_id);
                let entity = match mapped.len() {
                    0 => {
                        return Err(SpatialError::MissingSpatialSource {
                            feature: feature_id.clone(),
                        })
                    }
                    1 => mapped.iter().next().expect("len checked").clone(),
                    found => {
                        return Err(SpatialError::PatternCardinalityMismatch {
                            feature: feature_id.clone(),
                            expected: 1,
                            found,
                        })
                    }
                };
                let (center, radius) = cut_circle(geometry, feature_id, &entity)?;
                if inverse.get(&entity) != Some(feature_id) {
                    return Err(SpatialError::InvalidSpatialSource {
                        feature: feature_id.clone(),
                        entity: entity.clone(),
                        reason: InvalidSpatialSourceReason::ForeignFeature,
                    });
                }
                // Defensive gate: readiness checks extents and thickness but
                // not diameters, so an unresolved bound diameter reaches here
                // and fails closed (SCC-SPAT-016) — never silently re-solved.
                let engineering_diameter = resolved_diameter(engineering, feature_id, *diameter)
                    .ok_or(SpatialError::InvalidHoleDiameterDefensive {
                        feature: feature_id.clone(),
                    })?;
                witness_or_err(feature_id, &entity, engineering_diameter, radius)?;
                if !hole_strictly_inside_body(&body.footprint, center, radius) {
                    return Err(SpatialError::InvalidSpatialGeometry {
                        entity: entity.clone(),
                        reason: InvalidGeometryReason::HoleOutsideBody,
                    });
                }
                let (kind, z) = extent_to_interval(engineering, feature_id, thickness)?;
                cuts.insert(
                    allocate_cut_id(feature_id, 0),
                    CylindricalCut {
                        center,
                        radius,
                        z,
                        kind,
                        source_feature: feature_id.clone(),
                        source_entity: entity,
                    },
                );
            }

            EngineeringFeature::HolePattern { pattern, .. } => {
                // Cardinality is checked against the *solved* instance count:
                // template-only source-map entries are expected, claim-free
                // artifacts of array expansion (amendment §4-A rule 1/2).
                let expected = expected_pattern_count(pattern);
                let solved = solved_entities(source_map, geometry, feature_id);
                let found = solved.len();
                if found != expected {
                    return Err(SpatialError::PatternCardinalityMismatch {
                        feature: feature_id.clone(),
                        expected,
                        found,
                    });
                }
                let hole_diameter = match pattern {
                    HolePattern::Circular { hole, .. }
                    | HolePattern::Grid { hole, .. }
                    | HolePattern::Corner { hole, .. } => hole.diameter,
                };
                let engineering_diameter =
                    resolved_diameter(engineering, feature_id, hole_diameter).ok_or(
                        SpatialError::InvalidHoleDiameterDefensive {
                            feature: feature_id.clone(),
                        },
                    )?;
                let (kind, z) = extent_to_interval(engineering, feature_id, thickness)?;
                for (index, entity) in solved.iter().enumerate() {
                    let (center, radius) = cut_circle(geometry, feature_id, entity)?;
                    if inverse.get(entity) != Some(feature_id) {
                        return Err(SpatialError::InvalidSpatialSource {
                            feature: feature_id.clone(),
                            entity: entity.clone(),
                            reason: InvalidSpatialSourceReason::ForeignFeature,
                        });
                    }
                    witness_or_err(feature_id, entity, engineering_diameter, radius)?;
                    cuts.insert(
                        allocate_cut_id(feature_id, index),
                        CylindricalCut {
                            center,
                            radius,
                            z,
                            kind,
                            source_feature: feature_id.clone(),
                            source_entity: entity.clone(),
                        },
                    );
                }
            }

            EngineeringFeature::Slot {
                center,
                length,
                width,
                orientation_degrees,
                ..
            } => {
                let source_segments = scc_engineering::slot::slot_profile_segments(
                    *center,
                    *length,
                    *width,
                    *orientation_degrees,
                )
                .map_err(|_| SpatialError::InvalidBaseRepresentation {
                    feature: feature_id.clone(),
                    reason: InvalidBaseReason::UnsupportedProfile,
                })?;
                let solved: Vec<EntityId> = solved_entities(source_map, geometry, feature_id)
                    .into_iter()
                    .collect();
                if solved.len() != source_segments.len() {
                    return Err(SpatialError::PatternCardinalityMismatch {
                        feature: feature_id.clone(),
                        expected: source_segments.len(),
                        found: solved.len(),
                    });
                }
                let segments =
                    lower_profile_segments(feature_id, &source_segments, &solved, geometry)?;
                if !slot_strictly_inside_body(&body.footprint, &segments) {
                    return Err(SpatialError::InvalidSpatialGeometry {
                        entity: solved[0].clone(),
                        reason: InvalidGeometryReason::SlotOutsideBody,
                    });
                }
                slot_cuts.insert(
                    allocate_cut_id(feature_id, 0),
                    crate::model::SlotCut {
                        center: Point2 {
                            x: center.x,
                            y: center.y,
                        },
                        overall_length: *length,
                        width: *width,
                        orientation_degrees: *orientation_degrees,
                        segments,
                        z: ZInterval::new(0.0, thickness)?,
                        kind: SpatialCutKind::ThroughAll,
                        source_feature: feature_id.clone(),
                    },
                );
            }

            EngineeringFeature::Fillet { .. } => {
                return Err(SpatialError::UnsupportedSpatialFeature {
                    feature: feature_id.clone(),
                })
            }
            EngineeringFeature::Parametric {
                suppressed: true, ..
            } => {}
            EngineeringFeature::Parametric { .. } => {
                return Err(SpatialError::UnsupportedSpatialFeature {
                    feature: feature_id.clone(),
                })
            }
        }
    }

    // 7. Claim integrity: every solved geometry entity is claimed exactly
    //    once (body, cut, or construction circle). Source-map entries absent
    //    from geometry impose no claim (amendment §4-A rule 3).
    let mut claimed: BTreeSet<EntityId> = BTreeSet::new();
    claimed.extend(body_entities);
    claimed.extend(cuts.values().map(|cut| cut.source_entity.clone()));
    claimed.extend(slot_cuts.values().flat_map(|cut| {
        cut.segments
            .iter()
            .map(SpatialProfileSegment::source_entity)
            .cloned()
    }));
    claimed.extend(construction_circles.keys().cloned());
    for entity in geometry.entities.keys() {
        if !claimed.contains(entity) {
            return Err(SpatialError::UnmappedGeometryEntity {
                entity: entity.clone(),
            });
        }
    }

    // 8. Bounds from the body footprint ONLY — cuts never expand them
    //    (ADR-033 Decision 8).
    let bounds = match &body.footprint {
        SpatialFootprint::Rectangle {
            origin,
            width,
            height,
        } => Bounds3D::new(
            [origin.x, origin.y, 0.0],
            [origin.x + width, origin.y + height, thickness],
        )?,
        SpatialFootprint::Circle { center, radius } => Bounds3D::new(
            [center.x - radius, center.y - radius, 0.0],
            [center.x + radius, center.y + radius, thickness],
        )?,
        SpatialFootprint::Parametric { .. } => {
            return Err(SpatialError::InvalidBaseRepresentation {
                feature: body.source_feature.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            });
        }
        SpatialFootprint::Composite { segments } => {
            let (min_x, max_x, min_y, max_y) = composite_bounds(segments).ok_or_else(|| {
                SpatialError::InvalidBaseRepresentation {
                    feature: body.source_feature.clone(),
                    reason: InvalidBaseReason::UnsupportedProfile,
                }
            })?;
            Bounds3D::new([min_x, min_y, 0.0], [max_x, max_y, thickness])?
        }
    };

    // 9. Assemble. Cuts/construction circles are already deterministic
    //    BTreeMaps; the record points at the geometry generation used.
    Ok(SpatialModel {
        body,
        cuts,
        slot_cuts,
        bounds,
        construction_circles,
        source: SpatialModelSource {
            geometry_source_root: geometry.source_root,
            geometry_source_step: geometry.source_step,
        },
    })
}

/// The solved entities of one feature: the deterministic intersection of the
/// source-map entries with the geometry keys (`BTreeSet` order). Used for
/// every cardinality count and per-entity iteration so array-template
/// artifacts never reach the spatial model.
fn solved_entities(
    source_map: &EngineeringSourceMap,
    geometry: &GeometryModel,
    feature: &FeatureId,
) -> BTreeSet<EntityId> {
    source_map
        .entities_for(feature)
        .into_iter()
        .filter(|entity| geometry.entities.contains_key(entity))
        .collect()
}

/// Resolve Polygon edges in upstream typed provenance order. Entity labels
/// are deterministic but lexical order is not canonical: segment 10 sorts
/// before segment 2.
fn ordered_polygon_entities(
    source_map: &EngineeringSourceMap,
    geometry: &GeometryModel,
    feature: &FeatureId,
    expected_count: usize,
) -> Result<Vec<EntityId>, SpatialError> {
    let mut indexed = Vec::new();
    for entity in source_map.entities_for(feature) {
        let source = source_map.profile_segment(&entity).ok_or_else(|| {
            SpatialError::InvalidBaseRepresentation {
                feature: feature.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            }
        })?;
        if source.feature != *feature
            || !matches!(
                &source.semantic,
                ProfileSegmentSemantic::PolygonEdge { index }
                    if *index == source.segment_index
            )
        {
            return Err(SpatialError::InvalidBaseRepresentation {
                feature: feature.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            });
        }
        if !geometry.entities.contains_key(&entity) {
            return Err(SpatialError::InvalidSpatialSource {
                feature: feature.clone(),
                entity,
                reason: InvalidSpatialSourceReason::NotInGeometry,
            });
        }
        indexed.push((source.segment_index, entity));
    }
    indexed.sort_by_key(|(index, _)| *index);
    if indexed.len() != expected_count
        || indexed
            .iter()
            .enumerate()
            .any(|(expected, (actual, _))| *actual as usize != expected)
    {
        return Err(SpatialError::InvalidBaseRepresentation {
            feature: feature.clone(),
            reason: InvalidBaseReason::UnsupportedProfile,
        });
    }
    Ok(indexed.into_iter().map(|(_, entity)| entity).collect())
}

/// Resolve Composite segments in the ordered typed source-map sequence.
/// Entity labels are deterministic but lexical order is not source order once
/// the segment index reaches two digits.
fn ordered_composite_entities(
    source_map: &EngineeringSourceMap,
    geometry: &GeometryModel,
    feature: &FeatureId,
    expected_count: usize,
) -> Result<Vec<EntityId>, SpatialError> {
    let mut indexed = Vec::new();
    for entity in source_map.entities_for(feature) {
        let source = source_map.profile_segment(&entity).ok_or_else(|| {
            SpatialError::InvalidBaseRepresentation {
                feature: feature.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            }
        })?;
        if source.feature != *feature || source.semantic != ProfileSegmentSemantic::CompositeSegment
        {
            return Err(SpatialError::InvalidBaseRepresentation {
                feature: feature.clone(),
                reason: InvalidBaseReason::UnsupportedProfile,
            });
        }
        if !geometry.entities.contains_key(&entity) {
            return Err(SpatialError::InvalidSpatialSource {
                feature: feature.clone(),
                entity,
                reason: InvalidSpatialSourceReason::NotInGeometry,
            });
        }
        indexed.push((source.segment_index, entity));
    }
    indexed.sort_by_key(|(index, _)| *index);
    if indexed.len() != expected_count
        || indexed
            .iter()
            .enumerate()
            .any(|(expected, (actual, _))| *actual as usize != expected)
    {
        return Err(SpatialError::InvalidBaseRepresentation {
            feature: feature.clone(),
            reason: InvalidBaseReason::UnsupportedProfile,
        });
    }
    Ok(indexed.into_iter().map(|(_, entity)| entity).collect())
}

fn unique_base_entity(
    feature: &FeatureId,
    solved: &BTreeSet<EntityId>,
) -> Result<EntityId, SpatialError> {
    match solved.len() {
        0 => Err(SpatialError::InvalidBaseRepresentation {
            feature: feature.clone(),
            reason: InvalidBaseReason::NoSolvedEntity,
        }),
        1 => Ok(solved.iter().next().expect("len checked").clone()),
        _ => Err(SpatialError::InvalidBaseRepresentation {
            feature: feature.clone(),
            reason: InvalidBaseReason::NotUnique,
        }),
    }
}

fn lower_profile_segments(
    feature: &FeatureId,
    source_segments: &[ProfileSegment],
    entities: &[EntityId],
    geometry: &GeometryModel,
) -> Result<Vec<SpatialProfileSegment>, SpatialError> {
    const TOLERANCE: f64 = 1.0e-9;
    source_segments
        .iter()
        .zip(entities)
        .enumerate()
        .map(|(index, (source, entity))| {
            let primitive = geometry
                .entities
                .get(entity)
                .map(|entry| &entry.primitive)
                .ok_or_else(|| SpatialError::InvalidSpatialSource {
                    feature: feature.clone(),
                    entity: entity.clone(),
                    reason: InvalidSpatialSourceReason::NotInGeometry,
                })?;
            let segment_index =
                u32::try_from(index).map_err(|_| SpatialError::InvalidBaseRepresentation {
                    feature: feature.clone(),
                    reason: InvalidBaseReason::UnsupportedProfile,
                })?;
            match (source, primitive) {
                (
                    ProfileSegment::Line { start, end },
                    GeometryPrimitive::Line {
                        start: solved_start,
                        end: solved_end,
                    },
                ) if close_source_point(*start, *solved_start, TOLERANCE)
                    && close_source_point(*end, *solved_end, TOLERANCE) =>
                {
                    Ok(SpatialProfileSegment::Line {
                        start: *solved_start,
                        end: *solved_end,
                        source_entity: entity.clone(),
                        segment_index,
                    })
                }
                (
                    ProfileSegment::Arc {
                        start,
                        end,
                        center,
                        radius,
                        direction,
                    },
                    GeometryPrimitive::Arc {
                        center: solved_center,
                        radius: solved_radius,
                        start_angle,
                        end_angle,
                    },
                ) => {
                    let solved_start =
                        point_on_circle(*solved_center, *solved_radius, *start_angle);
                    let solved_end = point_on_circle(*solved_center, *solved_radius, *end_angle);
                    let (source_start_witness, source_end_witness) = match direction {
                        ArcDirection::CounterClockwise => (solved_start, solved_end),
                        ArcDirection::Clockwise => (solved_end, solved_start),
                    };
                    if !close_source_point(*start, source_start_witness, TOLERANCE)
                        || !close_source_point(*end, source_end_witness, TOLERANCE)
                        || !close_source_point(*center, *solved_center, TOLERANCE)
                        || !witness_matches(*radius, *solved_radius)
                    {
                        return Err(SpatialError::InvalidBaseRepresentation {
                            feature: feature.clone(),
                            reason: InvalidBaseReason::UnsupportedProfile,
                        });
                    }
                    Ok(SpatialProfileSegment::Arc {
                        start: Point2 {
                            x: start.x,
                            y: start.y,
                        },
                        end: Point2 { x: end.x, y: end.y },
                        center: *solved_center,
                        radius: *solved_radius,
                        direction: match direction {
                            ArcDirection::Clockwise => SpatialArcDirection::Clockwise,
                            ArcDirection::CounterClockwise => SpatialArcDirection::CounterClockwise,
                        },
                        source_entity: entity.clone(),
                        segment_index,
                    })
                }
                _ => Err(SpatialError::InvalidBaseRepresentation {
                    feature: feature.clone(),
                    reason: InvalidBaseReason::UnsupportedProfile,
                }),
            }
        })
        .collect()
}

fn close_point(a: Point2, b: Point2, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

fn close_source_point(a: Point2D, b: Point2, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

fn point_on_circle(center: Point2, radius: f64, angle: f64) -> Point2 {
    Point2 {
        x: center.x + radius * angle.cos(),
        y: center.y + radius * angle.sin(),
    }
}

fn validate_composite_footprint(footprint: &SpatialFootprint) -> Result<(), InvalidBaseReason> {
    let SpatialFootprint::Composite { segments } = footprint else {
        return Ok(());
    };
    if segments.is_empty() || segments.iter().any(|segment| !segment.is_valid()) {
        return Err(InvalidBaseReason::UnsupportedProfile);
    }
    const TOLERANCE: f64 = 1.0e-9;
    for index in 0..segments.len() {
        let end = segments[index].end();
        let next = segments[(index + 1) % segments.len()].start();
        if !close_point(end, next, TOLERANCE) {
            return Err(InvalidBaseReason::UnsupportedProfile);
        }
    }
    composite_bounds(segments)
        .filter(|(min_x, max_x, min_y, max_y)| max_x > min_x && max_y > min_y)
        .map(|_| ())
        .ok_or(InvalidBaseReason::UnsupportedProfile)
}

fn composite_bounds(segments: &[SpatialProfileSegment]) -> Option<(f64, f64, f64, f64)> {
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    for segment in segments {
        let (min_x, max_x, min_y, max_y) = segment.bounds()?;
        bounds = Some(match bounds {
            Some((old_min_x, old_max_x, old_min_y, old_max_y)) => (
                old_min_x.min(min_x),
                old_max_x.max(max_x),
                old_min_y.min(min_y),
                old_max_y.max(max_y),
            ),
            None => (min_x, max_x, min_y, max_y),
        });
    }
    bounds
}

fn hole_strictly_inside_body(footprint: &SpatialFootprint, center: Point2, radius: f64) -> bool {
    const TOLERANCE: f64 = 1.0e-9;
    match footprint {
        SpatialFootprint::Rectangle { .. } | SpatialFootprint::Circle { .. } => true,
        SpatialFootprint::Composite { segments }
            if segments
                .iter()
                .all(|segment| matches!(segment, SpatialProfileSegment::Line { .. })) =>
        {
            point_in_composite(center, segments)
                && segments.iter().all(|segment| {
                    let SpatialProfileSegment::Line { start, end, .. } = segment else {
                        return true;
                    };
                    distance_to_segment(center, *start, *end) > radius + TOLERANCE
                })
        }
        SpatialFootprint::Composite { .. } => true,
        SpatialFootprint::Parametric { .. } => false,
    }
}

fn distance_to_segment(point: Point2, start: Point2, end: Point2) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= 1.0e-9 * 1.0e-9 {
        return (point.x - start.x).hypot(point.y - start.y);
    }
    let t =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    let projection = Point2 {
        x: start.x + t * dx,
        y: start.y + t * dy,
    };
    (point.x - projection.x).hypot(point.y - projection.y)
}

fn slot_strictly_inside_body(footprint: &SpatialFootprint, slot: &[SpatialProfileSegment]) -> bool {
    const TOLERANCE: f64 = 1.0e-9;
    match footprint {
        SpatialFootprint::Rectangle {
            origin,
            width,
            height,
        } => slot.iter().all(|segment| {
            extrema(segment).iter().all(|point| {
                point.x > origin.x + TOLERANCE
                    && point.x < origin.x + width - TOLERANCE
                    && point.y > origin.y + TOLERANCE
                    && point.y < origin.y + height - TOLERANCE
            })
        }),
        SpatialFootprint::Circle { center, radius } => slot.iter().all(|segment| {
            extrema(segment).iter().all(|point| {
                let distance = (point.x - center.x).hypot(point.y - center.y);
                distance < radius - TOLERANCE
            })
        }),
        SpatialFootprint::Composite { segments } => {
            slot.iter().all(|segment| {
                extrema(segment)
                    .iter()
                    .all(|point| point_in_composite(*point, segments))
            }) && !slot_touches_composite(slot, segments)
        }
        SpatialFootprint::Parametric { .. } => false,
    }
}

fn extrema(segment: &SpatialProfileSegment) -> Vec<Point2> {
    match segment {
        SpatialProfileSegment::Line { start, end, .. } => vec![*start, *end],
        SpatialProfileSegment::Arc {
            start,
            end,
            center,
            radius,
            direction,
            ..
        } => {
            let mut points = vec![*start, *end];
            let start_angle = (start.y - center.y).atan2(start.x - center.x);
            let end_angle = (end.y - center.y).atan2(end.x - center.x);
            for angle in [
                0.0,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
                3.0 * std::f64::consts::FRAC_PI_2,
            ] {
                if angle_on_arc(start_angle, end_angle, *direction, angle) {
                    points.push(Point2 {
                        x: center.x + radius * angle.cos(),
                        y: center.y + radius * angle.sin(),
                    });
                }
            }
            points
        }
    }
}

fn angle_on_arc(start: f64, end: f64, direction: SpatialArcDirection, candidate: f64) -> bool {
    let sweep = match direction {
        SpatialArcDirection::CounterClockwise => (end - start).rem_euclid(std::f64::consts::TAU),
        SpatialArcDirection::Clockwise => (start - end).rem_euclid(std::f64::consts::TAU),
    };
    if sweep <= 0.0 || sweep >= std::f64::consts::TAU {
        return false;
    }
    let delta = match direction {
        SpatialArcDirection::CounterClockwise => (candidate.rem_euclid(std::f64::consts::TAU)
            - start.rem_euclid(std::f64::consts::TAU))
        .rem_euclid(std::f64::consts::TAU),
        SpatialArcDirection::Clockwise => (start.rem_euclid(std::f64::consts::TAU)
            - candidate.rem_euclid(std::f64::consts::TAU))
        .rem_euclid(std::f64::consts::TAU),
    };
    delta <= sweep
}

fn point_in_composite(point: Point2, segments: &[SpatialProfileSegment]) -> bool {
    const TOLERANCE: f64 = 1.0e-9;
    let mut winding = 0_i32;
    for segment in segments {
        let start = segment.start();
        let end = segment.end();
        let upward = start.y <= point.y && end.y > point.y;
        let downward = start.y > point.y && end.y <= point.y;
        if !upward && !downward {
            continue;
        }
        let dy = end.y - start.y;
        if dy.abs() <= TOLERANCE {
            continue;
        }
        let x = start.x + (point.y - start.y) * (end.x - start.x) / dy;
        if x > point.x + TOLERANCE {
            winding += if upward { 1 } else { -1 };
        }
    }
    winding != 0
}

fn slot_touches_composite(slot: &[SpatialProfileSegment], body: &[SpatialProfileSegment]) -> bool {
    slot.iter().any(|left| {
        body.iter().any(|right| {
            let (lx0, lx1, ly0, ly1) = match left.bounds() {
                Some(bounds) => bounds,
                None => return true,
            };
            let (rx0, rx1, ry0, ry1) = match right.bounds() {
                Some(bounds) => bounds,
                None => return true,
            };
            lx1 >= rx0 && rx1 >= lx0 && ly1 >= ry0 && ry1 >= ly0
        })
    })
}

/// The solved primitive of one entity, if it exists in geometry.
fn base_primitive<'a>(
    geometry: &'a GeometryModel,
    entity: &EntityId,
) -> Option<&'a GeometryPrimitive> {
    geometry.entities.get(entity).map(|entry| &entry.primitive)
}

/// Validate that `entity` is a finite, strictly positive solved circle and
/// return its center and radius. Also used for construction circles (no
/// witness there).
fn cut_circle(
    geometry: &GeometryModel,
    feature: &FeatureId,
    entity: &EntityId,
) -> Result<(Point2, f64), SpatialError> {
    let geometry_entity =
        geometry
            .entities
            .get(entity)
            .ok_or_else(|| SpatialError::InvalidSpatialSource {
                feature: feature.clone(),
                entity: entity.clone(),
                reason: InvalidSpatialSourceReason::NotInGeometry,
            })?;
    let (center, radius) = match &geometry_entity.primitive {
        GeometryPrimitive::Circle { center, radius } => (*center, *radius),
        other => {
            return Err(SpatialError::InvalidSpatialGeometry {
                entity: entity.clone(),
                reason: InvalidGeometryReason::NotCircle {
                    found: PrimitiveKind::of(other),
                },
            })
        }
    };
    if !center.x.is_finite() || !center.y.is_finite() {
        return Err(SpatialError::InvalidSpatialGeometry {
            entity: entity.clone(),
            reason: InvalidGeometryReason::NonFiniteCenter,
        });
    }
    if !radius.is_finite() {
        return Err(SpatialError::InvalidSpatialGeometry {
            entity: entity.clone(),
            reason: InvalidGeometryReason::NonFiniteRadius,
        });
    }
    if radius <= 0.0 {
        return Err(SpatialError::InvalidSpatialGeometry {
            entity: entity.clone(),
            reason: InvalidGeometryReason::NonPositiveRadius,
        });
    }
    Ok((center, radius))
}

/// The engineering diameter of a hole-bearing feature, resolved with the
/// single-sourced rule: a bound `Diameter` parameter wins over the embedded
/// literal (ADR-017 Decision 3), via
/// [`EngineeringModel::resolve_sourced_length_mm`].
fn resolved_diameter(
    engineering: &EngineeringModel,
    feature: &FeatureId,
    literal: f64,
) -> Option<f64> {
    match engineering
        .feature_parameters
        .get(feature)
        .and_then(|bindings| bindings.get(&FeatureParameterSlot::Diameter))
    {
        Some(parameter) => engineering.resolve_sourced_length_mm(&SourcedLength::Parameter {
            id: parameter.clone(),
        }),
        None => engineering.resolve_sourced_length_mm(&SourcedLength::Literal { value: literal }),
    }
}

/// Check the diameter witness: `|engineering − 2·radius|` must stay within
/// [`SPATIAL_LENGTH_TOLERANCE`].
fn witness_or_err(
    feature: &FeatureId,
    entity: &EntityId,
    engineering_diameter: f64,
    radius: f64,
) -> Result<(), SpatialError> {
    let geometry_diameter = 2.0 * radius;
    if !witness_matches(engineering_diameter, geometry_diameter) {
        return Err(SpatialError::SpatialWitnessMismatch {
            feature: feature.clone(),
            entity: entity.clone(),
            engineering_value: engineering_diameter,
            geometry_value: geometry_diameter,
        });
    }
    Ok(())
}

/// Whether two lengths agree within [`SPATIAL_LENGTH_TOLERANCE`]. Exactly at
/// the tolerance passes.
fn witness_matches(engineering_value: f64, geometry_value: f64) -> bool {
    (engineering_value - geometry_value).abs() <= SPATIAL_LENGTH_TOLERANCE
}

/// Convert a readiness-guaranteed hole extent into the cut kind plus its
/// validated Z interval. `ThroughAll` spans `[0, T]`; a blind depth `D` spans
/// `[0, D]` from the base face or `[T − D, T]` from the opposite face. A
/// resolved depth at or beyond the thickness is rejected — never converted
/// to `ThroughAll` (ADR-033 Decision 6).
fn extent_to_interval(
    engineering: &EngineeringModel,
    feature: &FeatureId,
    thickness: f64,
) -> Result<(SpatialCutKind, ZInterval), SpatialError> {
    let extent = engineering
        .spatial
        .hole_extents
        .get(feature)
        .expect("readiness guarantees a hole extent entry for hole-bearing features");
    match extent {
        HoleExtent::ThroughAll => Ok((SpatialCutKind::ThroughAll, ZInterval::new(0.0, thickness)?)),
        HoleExtent::Blind { start_face, depth } => {
            let resolved = engineering
                .resolve_sourced_length_mm(depth)
                .ok_or_else(|| SpatialError::InvalidBlindDepthResolutionDefensive {
                    feature: feature.clone(),
                })?;
            Ok((
                SpatialCutKind::Blind {
                    start_face: *start_face,
                },
                cut_z_interval(feature, *start_face, resolved, thickness)?,
            ))
        }
    }
}

/// The Z interval of a blind cut: `D < T` strictly; `Base` starts at the
/// base plane, `Opposite` ends at the opposite plane.
fn cut_z_interval(
    feature: &FeatureId,
    start_face: HoleStartFace,
    depth: f64,
    thickness: f64,
) -> Result<ZInterval, SpatialError> {
    if depth >= thickness {
        return Err(SpatialError::InvalidBlindDepthDefensive {
            feature: feature.clone(),
            depth,
            thickness,
        });
    }
    match start_face {
        HoleStartFace::Base => ZInterval::new(0.0, depth),
        HoleStartFace::Opposite => ZInterval::new(thickness - depth, thickness),
    }
}

/// The number of solved instances a pattern intent declares.
fn expected_pattern_count(pattern: &HolePattern) -> usize {
    match pattern {
        HolePattern::Circular { count, .. } => *count as usize,
        HolePattern::Grid { rows, columns, .. } => (*rows as usize) * (*columns as usize),
        HolePattern::Corner { .. } => 4,
    }
}

/// Allocate the deterministic cut id `{feature_id}:cut:{index:04}`
/// (ADR-033 Decision 9). Opaque: never parsed back for meaning.
pub(crate) fn allocate_cut_id(feature: &FeatureId, index: usize) -> SpatialCutId {
    SpatialCutId(format!("{}:cut:{:04}", feature.0, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fid(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    #[test]
    fn cut_z_interval_through_all_spans_the_plate() {
        // ThroughAll is the plain base-plane interval [0, T].
        let z = ZInterval::new(0.0, 10.0).unwrap();
        assert_eq!((z.min, z.max), (0.0, 10.0));
    }

    #[test]
    fn cut_z_interval_blind_base_starts_at_zero() {
        let z = cut_z_interval(&fid("h"), HoleStartFace::Base, 4.0, 10.0).unwrap();
        assert_eq!((z.min, z.max), (0.0, 4.0));
    }

    #[test]
    fn cut_z_interval_blind_opposite_ends_at_thickness() {
        let z = cut_z_interval(&fid("h"), HoleStartFace::Opposite, 4.0, 10.0).unwrap();
        assert_eq!((z.min, z.max), (6.0, 10.0));
    }

    #[test]
    fn cut_z_interval_rejects_depth_equal_to_thickness() {
        assert_eq!(
            cut_z_interval(&fid("h"), HoleStartFace::Base, 10.0, 10.0),
            Err(SpatialError::InvalidBlindDepthDefensive {
                feature: fid("h"),
                depth: 10.0,
                thickness: 10.0,
            })
        );
    }

    #[test]
    fn cut_z_interval_rejects_depth_beyond_thickness() {
        assert_eq!(
            cut_z_interval(&fid("h"), HoleStartFace::Opposite, 12.5, 10.0),
            Err(SpatialError::InvalidBlindDepthDefensive {
                feature: fid("h"),
                depth: 12.5,
                thickness: 10.0,
            })
        );
    }

    #[test]
    fn witness_boundary_one_nanometre_passes() {
        // The tolerance is inclusive: a difference at (the nearest f64 to)
        // 1e-9 passes. `1.0 − 1e-9` keeps the computed difference just below
        // the tolerance in IEEE-754; `1.0 + 1e-9` rounds a hair above it, so
        // the subtraction side is the honest at-tolerance case.
        assert!(witness_matches(1.0, 1.0 - 1.0e-9));
    }

    #[test]
    fn witness_boundary_two_nanometres_fails() {
        assert!(!witness_matches(1.0, 1.0 - 2.0e-9));
        assert!(!witness_matches(1.0, 1.0 + 2.0e-9));
        assert!(!witness_matches(1.0, 3.0));
    }

    #[test]
    fn expected_pattern_count_covers_all_pattern_kinds() {
        use scc_engineering::feature::HoleDefinition;
        let hole = HoleDefinition { diameter: 10.0 };
        assert_eq!(
            expected_pattern_count(&HolePattern::Circular {
                center: scc_engineering::Point2D::new(0.0, 0.0),
                pitch_circle_diameter: 100.0,
                count: 8,
                start_angle: 0.0,
                hole: hole.clone(),
            }),
            8
        );
        assert_eq!(
            expected_pattern_count(&HolePattern::Grid {
                rows: 2,
                columns: 3,
                spacing_x: 10.0,
                spacing_y: 10.0,
                hole: hole.clone(),
            }),
            6
        );
        assert_eq!(
            expected_pattern_count(&HolePattern::Corner {
                offset_x: 15.0,
                offset_y: 15.0,
                hole,
            }),
            4
        );
    }

    #[test]
    fn allocate_cut_id_is_deterministic_and_padded() {
        assert_eq!(
            allocate_cut_id(&fid("bolt_pattern"), 0).to_string(),
            "bolt_pattern:cut:0000"
        );
        assert_eq!(
            allocate_cut_id(&fid("bolt_pattern"), 7).to_string(),
            "bolt_pattern:cut:0007"
        );
        assert_eq!(
            allocate_cut_id(&fid("bolt_pattern"), 12).to_string(),
            "bolt_pattern:cut:0012"
        );
        // Determinism: same inputs, same id, every time.
        for index in 0..16 {
            assert_eq!(
                allocate_cut_id(&fid("f"), index),
                allocate_cut_id(&fid("f"), index)
            );
        }
    }
}
