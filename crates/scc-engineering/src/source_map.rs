//! Strongly typed Engineering → SCC source mapping.
//!
//! Maps lowered SCC entities and constraints back to their engineering source.
//! The mapping is strong-typed (ADR-017 Decision 7): provenance is never
//! recovered by parsing entity id strings. Deterministic id naming is preserved
//! for readability, but the source map is the only mechanism.

use std::collections::{BTreeMap, BTreeSet};

use scc_core::ids::{ConstraintId, EntityId};

use crate::{EngineeringConstraintId, FeatureId};

/// Semantic identity of one lowered profile segment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProfileSegmentSemantic {
    CompositeSegment,
    RoundedCorner { corner: RoundedCorner },
    SlotSegment,
    PolygonEdge { index: u32 },
}

/// Stable corner identity for generated rounded rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundedCorner {
    BottomRight,
    TopRight,
    TopLeft,
    BottomLeft,
}

/// Typed provenance from one SCC entity to its source profile segment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProfileSegmentSource {
    pub feature: FeatureId,
    pub segment_index: u32,
    pub semantic: ProfileSegmentSemantic,
}

/// Which SCC entities and constraints an engineering feature/constraint lowered
/// into.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct EngineeringSourceMap {
    feature_entities: BTreeMap<FeatureId, BTreeSet<EntityId>>,
    constraint_ids: BTreeMap<EngineeringConstraintId, BTreeSet<ConstraintId>>,
    pattern_arrays: BTreeMap<FeatureId, String>,
    profile_segments: BTreeMap<EntityId, ProfileSegmentSource>,
}

impl EngineeringSourceMap {
    /// An empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `feature` lowered into `entity`.
    pub fn record_feature_entity(&mut self, feature: FeatureId, entity: EntityId) {
        self.feature_entities
            .entry(feature)
            .or_default()
            .insert(entity);
    }

    /// Record that `constraint` lowered into the SCC constraint `constraint`.
    pub fn record_constraint(
        &mut self,
        constraint: EngineeringConstraintId,
        constraint_id: ConstraintId,
    ) {
        self.constraint_ids
            .entry(constraint)
            .or_default()
            .insert(constraint_id);
    }

    /// Record the array pseudo-entity a pattern feature lowers into.
    pub fn record_pattern_array(&mut self, feature: FeatureId, array_id: String) {
        self.pattern_arrays.insert(feature, array_id);
    }

    /// Record typed provenance for one lowered line or arc segment.
    pub fn record_profile_segment(&mut self, entity: EntityId, source: ProfileSegmentSource) {
        self.profile_segments.insert(entity, source);
    }

    /// Look up typed profile-segment provenance.
    pub fn profile_segment(&self, entity: &EntityId) -> Option<&ProfileSegmentSource> {
        self.profile_segments.get(entity)
    }

    /// All typed profile-segment mappings.
    pub fn profile_segments(&self) -> &BTreeMap<EntityId, ProfileSegmentSource> {
        &self.profile_segments
    }

    /// All SCC entities that `feature` lowered into.
    pub fn entities_for(&self, feature: &FeatureId) -> BTreeSet<EntityId> {
        self.feature_entities
            .get(feature)
            .cloned()
            .unwrap_or_default()
    }

    /// The SCC constraint ids that `constraint` lowered into (deterministic
    /// prediction from the SCC lowering counter, valid when the proposal's
    /// relations are emitted before any dimension/array/anchor).
    pub fn constraint_ids_for(
        &self,
        constraint: &EngineeringConstraintId,
    ) -> BTreeSet<ConstraintId> {
        self.constraint_ids
            .get(constraint)
            .cloned()
            .unwrap_or_default()
    }

    /// The array pseudo-entity id a pattern feature lowered into, if any.
    pub fn pattern_array(&self, feature: &FeatureId) -> Option<&str> {
        self.pattern_arrays.get(feature).map(String::as_str)
    }

    /// Whether any entity has been recorded for the feature.
    pub fn contains_feature(&self, feature: &FeatureId) -> bool {
        self.feature_entities.contains_key(feature)
    }

    /// Inverse map: SCC entity → engineering feature. Derived here so the UI
    /// never recovers provenance by parsing entity id strings (ADR-021
    /// Decision 7).
    pub fn entity_to_feature(&self) -> BTreeMap<EntityId, FeatureId> {
        let mut inverse = BTreeMap::new();
        for (feature, entities) in &self.feature_entities {
            for entity in entities {
                inverse.insert(entity.clone(), feature.clone());
            }
        }
        inverse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_queries_feature_entities() {
        let mut map = EngineeringSourceMap::new();
        let feature = FeatureId("bolt_circle".into());
        assert!(!map.contains_feature(&feature));
        map.record_feature_entity(feature.clone(), EntityId("bolt_circle".into()));
        map.record_feature_entity(feature.clone(), EntityId("bolt_circle_array[0]".into()));
        assert!(map.contains_feature(&feature));
        assert_eq!(map.entities_for(&feature).len(), 2);
        assert_eq!(map.entities_for(&FeatureId("ghost".into())).len(), 0);
    }

    #[test]
    fn records_constraint_ids() {
        let mut map = EngineeringSourceMap::new();
        let cid = EngineeringConstraintId("c1".into());
        map.record_constraint(cid.clone(), ConstraintId("c_1".into()));
        assert_eq!(map.constraint_ids_for(&cid).len(), 1);
        assert_eq!(
            map.constraint_ids_for(&EngineeringConstraintId("ghost".into()))
                .len(),
            0
        );
    }
}
