//! Strong-typed drawing source provenance (Phase 8.1 + Phase 8.2).
//!
//! [`DrawingSourceMap`] maps each [`DrawingPrimitiveId`] back to the solved
//! geometry [`EntityId`] it was derived from. Composed with
//! [`scc_engineering::source_map::EngineeringSourceMap`]'s `entity_to_feature`
//! inverse, this yields the full chain
//!
//! ```text
//! DrawingPrimitiveId → EntityId → FeatureId
//! ```
//!
//! Provenance is **never** recovered by parsing id strings (no prefix, no
//! `contains`, no `split("_")`, no regex) — id parsing is forbidden by the
//! same discipline as ADR-017 Decision 7 / ADR-021 Decision 7.
//!
//! Phase 8.2 formalizes the semantics of [`DrawingSource`]: every
//! geometry-backed primitive is [`DrawingSource::Engineering`] (feature +
//! entity). [`DrawingSource::Synthetic`] is **only** for drawing-engine-created
//! primitives (future: extension lines, arrows, center marks, view/sheet
//! borders) and is **never** a fallback for missing provenance — a solved
//! geometry entity without an engineering feature fails closed
//! (`SCC-DRW-003`).

use std::collections::{BTreeMap, BTreeSet};

use scc_core::ids::EntityId;
use scc_engineering::feature::EngineeringFeature;
use scc_engineering::source_map::EngineeringSourceMap;
use scc_engineering::FeatureId;
use scc_geometry::GeometryModel;
use serde::{Deserialize, Serialize};

use crate::model::DrawingModel;
use crate::{DrawingError, DrawingPrimitiveId};

/// Strong-typed map from each drawing primitive to its originating geometry
/// entity. Deterministic (`BTreeMap` by primitive id).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DrawingSourceMap {
    primitive_entities: BTreeMap<DrawingPrimitiveId, EntityId>,
    /// Additive provenance for primitives lowered directly from the frozen
    /// Phase 9 projection artifact. Omitted when empty so legacy documents
    /// retain their exact serialized shape.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    projected_primitives: BTreeMap<DrawingPrimitiveId, scc_projection::ProjectionPrimitiveId>,
    /// Section-derived boundary, hatch and cutting-plane primitives. Their
    /// provenance is the validated EngineeringModel rebuild digest and a
    /// stable semantic location in its sampled OCP section.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    section_primitives: BTreeMap<DrawingPrimitiveId, SectionPrimitiveSource>,
}

impl DrawingSourceMap {
    /// An empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `primitive` was derived from geometry entity `entity`.
    pub fn record(&mut self, primitive: DrawingPrimitiveId, entity: EntityId) {
        self.primitive_entities.insert(primitive, entity);
    }

    /// Record that `primitive` was lowered from an opaque projection
    /// primitive. The projection id is stored verbatim; it is never parsed.
    pub fn record_projected(
        &mut self,
        primitive: DrawingPrimitiveId,
        projected: scc_projection::ProjectionPrimitiveId,
    ) {
        self.projected_primitives.insert(primitive, projected);
    }

    pub fn record_section(
        &mut self,
        primitive: DrawingPrimitiveId,
        source: SectionPrimitiveSource,
    ) {
        self.section_primitives.insert(primitive, source);
    }

    pub fn section_for_primitive(
        &self,
        primitive: &DrawingPrimitiveId,
    ) -> Option<&SectionPrimitiveSource> {
        self.section_primitives.get(primitive)
    }

    pub fn section_iter(
        &self,
    ) -> impl Iterator<Item = (&DrawingPrimitiveId, &SectionPrimitiveSource)> {
        self.section_primitives.iter()
    }

    /// The projection primitive that produced a drawing primitive, if any.
    pub fn projected_for_primitive(
        &self,
        primitive: &DrawingPrimitiveId,
    ) -> Option<&scc_projection::ProjectionPrimitiveId> {
        self.projected_primitives.get(primitive)
    }

    /// All projected provenance entries in deterministic order.
    pub fn projected_iter(
        &self,
    ) -> impl Iterator<Item = (&DrawingPrimitiveId, &scc_projection::ProjectionPrimitiveId)> {
        self.projected_primitives.iter()
    }

    /// The geometry entity a drawing primitive was derived from, if any.
    ///
    /// `None` means the primitive id is unknown to this map (not a
    /// provenance failure — use [`DrawingSourceMap::feature_for_primitive`]
    /// for the fail-closed feature lookup).
    pub fn entity_for_primitive(&self, primitive: &DrawingPrimitiveId) -> Option<&EntityId> {
        self.primitive_entities.get(primitive)
    }

    /// Alias of [`DrawingSourceMap::entity_for_primitive`] (kept for
    /// compatibility with the Phase 8.1 API).
    pub fn entity_for(&self, primitive: &DrawingPrimitiveId) -> Option<&EntityId> {
        self.entity_for_primitive(primitive)
    }

    /// Return the typed projected provenance for a primitive, if present.
    pub fn projected_source_for_primitive(
        &self,
        primitive: &DrawingPrimitiveId,
    ) -> Option<DrawingSource> {
        self.projected_for_primitive(primitive)
            .map(|projected| DrawingSource::Projected {
                primitive: projected.clone(),
            })
    }

    /// All `(primitive, entity)` pairs in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (&DrawingPrimitiveId, &EntityId)> + '_ {
        self.primitive_entities.iter()
    }

    /// Whether every primitive in `model` has a provenance entry.
    ///
    /// Phase 8.2 builds every primitive from a geometry entity and records it
    /// here (the fail-closed [`crate::builder::build_drawing`] guarantees
    /// coverage), so the sets match by construction; this is a cheap integrity
    /// assertion for callers and tests.
    pub fn covers_all(model: &DrawingModel) -> bool {
        model
            .sheets
            .values()
            .flat_map(|sheet| sheet.views.values())
            .all(|view| {
                view.primitives.keys().all(|prim| {
                    model.source_map.entity_for_primitive(prim).is_some()
                        || model.source_map.projected_for_primitive(prim).is_some()
                        || model.source_map.section_for_primitive(prim).is_some()
                })
            })
    }

    /// Derived inverse index: `EntityId → BTreeSet<DrawingPrimitiveId>`.
    ///
    /// One geometry entity can legitimately feed several drawing primitives
    /// (e.g. a future multi-view drawing). Derived on demand, deterministic.
    pub fn entity_index(&self) -> BTreeMap<EntityId, BTreeSet<DrawingPrimitiveId>> {
        let mut index = BTreeMap::new();
        for (primitive, entity) in &self.primitive_entities {
            index
                .entry(entity.clone())
                .or_insert_with(BTreeSet::new)
                .insert(primitive.clone());
        }
        index
    }

    /// All drawing primitives derived from a given geometry entity.
    pub fn primitives_for_entity(&self, entity: &EntityId) -> BTreeSet<DrawingPrimitiveId> {
        self.entity_index().get(entity).cloned().unwrap_or_default()
    }

    /// The engineering feature a drawing primitive presents, fail-closed.
    ///
    /// - `Ok(None)`: the primitive id is unknown to this map.
    /// - `Err(MissingSourceMapping)`: the primitive's entity has no engineering
    ///   feature (an integrity violation — never silently treated as synthetic).
    /// - `Ok(Some(feature))`: the originating feature.
    pub fn feature_for_primitive(
        &self,
        primitive: &DrawingPrimitiveId,
        engineering_source_map: &EngineeringSourceMap,
    ) -> Result<Option<FeatureId>, DrawingError> {
        let Some(entity) = self.entity_for_primitive(primitive) else {
            return Ok(None);
        };
        let feature = engineering_source_map
            .entity_to_feature()
            .get(entity)
            .cloned()
            .ok_or_else(|| DrawingError::MissingSourceMapping {
                entity_id: entity.clone(),
            })?;
        Ok(Some(feature))
    }

    /// All drawing primitives whose entity belongs to the given engineering
    /// feature (e.g. the 8 circles of a circular `HolePattern`). Stable
    /// `BTreeSet`, deterministic.
    pub fn primitives_for_feature(
        &self,
        feature: &FeatureId,
        engineering_source_map: &EngineeringSourceMap,
    ) -> BTreeSet<DrawingPrimitiveId> {
        let index = self.entity_index();
        engineering_source_map
            .entities_for(feature)
            .iter()
            .filter_map(|entity| index.get(entity))
            .flatten()
            .cloned()
            .collect()
    }

    /// Classify each drawing primitive's origin, fail-closed.
    ///
    /// Composes this map (primitive → entity) with the engineering source map
    /// (entity → feature). Every entry is [`DrawingSource::Engineering`]
    /// (feature + entity). [`DrawingSource::Synthetic`] is **never** produced
    /// here: a solved geometry entity with no engineering feature is a
    /// fail-closed [`DrawingError::MissingSourceMapping`], never a synthetic
    /// fallback.
    /// Classify projected primitives as typed `DrawingSource::Projected`
    /// values. Legacy engineering entries are intentionally not mixed into
    /// this method.
    pub fn projected_sources(&self) -> BTreeMap<DrawingPrimitiveId, DrawingSource> {
        self.projected_primitives
            .iter()
            .map(|(drawing, projected)| {
                (
                    drawing.clone(),
                    DrawingSource::Projected {
                        primitive: projected.clone(),
                    },
                )
            })
            .collect()
    }

    pub fn drawing_sources(
        &self,
        engineering_source_map: &EngineeringSourceMap,
    ) -> Result<BTreeMap<DrawingPrimitiveId, DrawingSource>, DrawingError> {
        let entity_to_feature = engineering_source_map.entity_to_feature();
        let mut out = BTreeMap::new();
        for (primitive, entity) in &self.primitive_entities {
            let feature = entity_to_feature.get(entity).ok_or_else(|| {
                DrawingError::MissingSourceMapping {
                    entity_id: entity.clone(),
                }
            })?;
            out.insert(
                primitive.clone(),
                DrawingSource::Engineering {
                    feature: feature.clone(),
                    entity: entity.clone(),
                },
            );
        }
        Ok(out)
    }
}

/// The origin of a drawing primitive: which engineering source it presents.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DrawingSource {
    /// Derived from a solved geometry entity that maps to an engineering
    /// feature. Carries both the feature and the originating entity.
    Engineering {
        feature: FeatureId,
        entity: EntityId,
    },
    /// Created by the drawing engine itself (no engineering entity).
    ///
    /// Phase 8.2 never produces this from the geometry→drawing builder; it is
    /// reserved for future drawing-engine-created geometry (dimension
    /// extension lines, arrows, leaders, centerlines, center marks, view/sheet
    /// borders).
    Synthetic { kind: SyntheticDrawingKind },
    /// Lowered directly from one opaque projection primitive.
    Projected {
        /// The exact projection id; this value is never interpreted here.
        primitive: scc_projection::ProjectionPrimitiveId,
    },
    /// Derived from the validated read-only OCP section of a model digest.
    Section { source: SectionPrimitiveSource },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SectionPrimitiveSource {
    pub model_sha256: String,
    pub origin: SectionPrimitiveOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SectionPrimitiveOrigin {
    BoundarySegment { loop_index: u32, segment_index: u32 },
    HatchSegment { hatch_index: u32 },
    CuttingPlaneSegment { axis: crate::SectionAxis },
}

/// The kind of drawing-engine-created synthetic source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntheticDrawingKind {
    /// Reserved catch-all for drawing-engine-created primitives. Specific kinds
    /// (dimension extension line, arrow, leader, centerline, center mark,
    /// view/sheet border) are added when the corresponding Drawing Engine
    /// phase arrives.
    DrawingEngineGenerated,
}

/// Validate the integrity of a drawing's source provenance, fail-closed.
///
/// Checks, for every primitive in the drawing:
/// - it has a source-map entry (no orphan primitives);
/// - its source-map entry refers to a primitive that actually exists;
/// - its originating entity exists in the solved geometry model;
/// - that entity maps to an engineering feature (never silently synthetic);
/// - that feature exists in the engineering model.
///
/// Duplicate/conflicting primitive→entity mappings are structurally prevented
/// (a `BTreeMap` key is a single primitive id). Violations return
/// `SCC-DRW-006`; a missing entity→feature mapping returns `SCC-DRW-003`.
pub fn validate_drawing_source_integrity(
    drawing: &DrawingModel,
    geometry: &GeometryModel,
    engineering_source_map: &EngineeringSourceMap,
    features: &BTreeMap<FeatureId, EngineeringFeature>,
) -> Result<(), DrawingError> {
    // 1. Every model primitive has a source-map entry.
    for (sheet_id, sheet) in &drawing.sheets {
        for (view_id, view) in &sheet.views {
            for prim_id in view.primitives.keys() {
                if drawing.source_map.entity_for_primitive(prim_id).is_none()
                    && drawing
                        .source_map
                        .projected_for_primitive(prim_id)
                        .is_none()
                    && drawing.source_map.section_for_primitive(prim_id).is_none()
                {
                    return Err(DrawingError::InvalidSourceIntegrity {
                        reason: format!(
                            "primitive {} in view {} of sheet {} has no source-map entry",
                            prim_id, view_id, sheet_id
                        ),
                    });
                }
            }
        }
    }

    let entity_to_feature = engineering_source_map.entity_to_feature();
    for (prim_id, entity) in drawing.source_map.iter() {
        // 2. The map key must refer to a primitive that exists in the model.
        let exists_in_model = drawing.sheets.values().any(|sheet| {
            sheet
                .views
                .values()
                .any(|view| view.primitives.contains_key(prim_id))
        });
        if !exists_in_model {
            return Err(DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "source map references primitive {} that does not exist in the drawing model",
                    prim_id
                ),
            });
        }
        // 3. The entity must exist in the solved geometry.
        if !geometry.entities.contains_key(entity) {
            return Err(DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "primitive {} maps to entity {} that does not exist in the solved geometry",
                    prim_id, entity
                ),
            });
        }
        // 4. The entity must map to an engineering feature (never synthetic).
        let feature =
            entity_to_feature
                .get(entity)
                .ok_or_else(|| DrawingError::MissingSourceMapping {
                    entity_id: entity.clone(),
                })?;
        // 5. The feature must exist in the engineering model.
        if !features.contains_key(feature) {
            return Err(DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "primitive {} maps to entity {} → feature {} which does not exist in the engineering model",
                    prim_id, entity, feature
                ),
            });
        }
    }

    // Section geometry is sourced by a validated read-only OCP rebuild
    // identified by its EngineeringModel digest, not a transient face/edge
    // ordinal, so it has a separate typed provenance contract.
    for (prim_id, source) in drawing.source_map.section_iter() {
        let owner = drawing
            .sheets
            .values()
            .flat_map(|sheet| sheet.views.values())
            .find(|view| view.primitives.contains_key(prim_id))
            .ok_or_else(|| DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "section provenance references primitive {} absent from DrawingModel",
                    prim_id
                ),
            })?;
        if source.model_sha256.len() != 64
            || !source
                .model_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "section primitive {} has an invalid EngineeringModel digest",
                    prim_id
                ),
            });
        }
        let valid_owner = match source.origin {
            SectionPrimitiveOrigin::BoundarySegment { .. }
            | SectionPrimitiveOrigin::HatchSegment { .. } => {
                owner.kind == crate::DrawingViewKind::SectionAA
            }
            SectionPrimitiveOrigin::CuttingPlaneSegment { .. } => {
                matches!(
                    owner.kind,
                    crate::DrawingViewKind::Front | crate::DrawingViewKind::Top
                )
            }
        };
        if !valid_owner {
            return Err(DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "section primitive {} is attached to an incompatible view",
                    prim_id
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prim(id: &str) -> DrawingPrimitiveId {
        DrawingPrimitiveId(id.into())
    }

    fn ent(id: &str) -> EntityId {
        EntityId(id.into())
    }

    #[test]
    fn record_and_forward_query() {
        let mut map = DrawingSourceMap::new();
        map.record(prim("p1"), ent("e1"));
        map.record(prim("p2"), ent("e2"));
        assert_eq!(map.entity_for_primitive(&prim("p1")), Some(&ent("e1")));
        assert_eq!(map.entity_for_primitive(&prim("ghost")), None);
        assert_eq!(map.iter().count(), 2);
    }

    #[test]
    fn entity_index_and_inverse_query() {
        let mut map = DrawingSourceMap::new();
        map.record(prim("p1"), ent("e1"));
        map.record(prim("p2"), ent("e1"));
        map.record(prim("p3"), ent("e2"));
        assert_eq!(
            map.primitives_for_entity(&ent("e1")),
            BTreeSet::from([prim("p1"), prim("p2")])
        );
        assert_eq!(map.primitives_for_entity(&ent("ghost")), BTreeSet::new());
    }

    #[test]
    fn synthetic_kind_is_expressible() {
        let source = DrawingSource::Synthetic {
            kind: SyntheticDrawingKind::DrawingEngineGenerated,
        };
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(
            json,
            r#"{"type":"synthetic","kind":"drawing_engine_generated"}"#
        );
        let back: DrawingSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, back);
    }
}
