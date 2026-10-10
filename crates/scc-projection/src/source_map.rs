//! The provenance map of a [`crate::ProjectionModel`] (Phase 9.3, ADR-034).
//!
//! One spatial element legitimately produces MANY projected primitives per
//! view (a cut → up to 4 segments per front-like view, or 1 circle in top),
//! so provenance lives in a separate map stored inside the model (the
//! `DrawingSourceMap` pattern) rather than being embedded per primitive
//! (ADR-034 Decision 12). The reverse index is derived on demand — never
//! stored, so there is no second index to drift.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::{ProjectionPrimitiveId, SpatialElementRef};

/// The provenance of every projected primitive: primitive id → the spatial
/// element it came from. Stored inside [`crate::ProjectionModel`] so a
/// serialized model is self-contained (no parallel artifact to lose).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectionSourceMap {
    primitive_sources: BTreeMap<ProjectionPrimitiveId, SpatialElementRef>,
}

impl ProjectionSourceMap {
    /// Record the source of one primitive. Called by the builder at insert
    /// time; provenance is atomic with the primitive, never back-filled.
    pub(crate) fn record(&mut self, primitive: ProjectionPrimitiveId, source: SpatialElementRef) {
        self.primitive_sources.insert(primitive, source);
    }

    /// Whether a primitive id already has a global provenance entry.
    pub(crate) fn contains_primitive(&self, primitive: &ProjectionPrimitiveId) -> bool {
        self.primitive_sources.contains_key(primitive)
    }

    /// The spatial element one primitive came from, if known.
    pub fn element_for_primitive(
        &self,
        primitive: &ProjectionPrimitiveId,
    ) -> Option<&SpatialElementRef> {
        self.primitive_sources.get(primitive)
    }

    /// Deterministic (id-ascending) iteration over all provenance entries.
    pub fn iter(&self) -> impl Iterator<Item = (&ProjectionPrimitiveId, &SpatialElementRef)> {
        self.primitive_sources.iter()
    }

    /// The element → primitives index, derived on demand — NEVER stored (no
    /// second index to drift, ADR-034 Decision 12).
    pub fn element_index(&self) -> BTreeMap<SpatialElementRef, BTreeSet<ProjectionPrimitiveId>> {
        let mut index: BTreeMap<SpatialElementRef, BTreeSet<ProjectionPrimitiveId>> =
            BTreeMap::new();
        for (primitive, element) in &self.primitive_sources {
            index
                .entry(element.clone())
                .or_default()
                .insert(primitive.clone());
        }
        index
    }

    /// All primitives derived from one spatial element, in id-ascending
    /// order. A scan over a small map at call time — cheap, deterministic,
    /// zero drift.
    pub fn primitives_for_element(
        &self,
        element: &SpatialElementRef,
    ) -> BTreeSet<ProjectionPrimitiveId> {
        self.primitive_sources
            .iter()
            .filter(|(_, source)| {
                *source == element
                    || matches!(
                        (element, *source),
                        (
                            SpatialElementRef::Body,
                            SpatialElementRef::BodySegment { .. }
                        )
                    )
            })
            .map(|(primitive, _)| primitive.clone())
            .collect()
    }

    /// Whether every primitive recorded in `present` has a provenance entry
    /// (and vice versa: no orphan provenance for unknown primitives).
    pub(crate) fn covers_exactly(
        &self,
        present: impl Iterator<Item = ProjectionPrimitiveId>,
    ) -> bool {
        let mut staged = BTreeSet::new();
        let mut present_count = 0usize;
        for primitive in present {
            present_count += 1;
            staged.insert(primitive);
        }
        if present_count != staged.len() || staged.len() != self.primitive_sources.len() {
            return false;
        }
        for primitive in self.primitive_sources.keys() {
            if !staged.remove(primitive) {
                return false;
            }
        }
        staged.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(value: &str) -> ProjectionPrimitiveId {
        ProjectionPrimitiveId(value.into())
    }

    fn sample() -> ProjectionSourceMap {
        let mut map = ProjectionSourceMap::default();
        map.record(pid("front:body:p:0000"), SpatialElementRef::Body);
        map.record(
            pid("top:a_hole:cut:0000:p:0000"),
            SpatialElementRef::Cut {
                id: scc_spatial::SpatialCutId("a_hole:cut:0000".into()),
            },
        );
        map.record(
            pid("left:a_hole:cut:0000:p:0000"),
            SpatialElementRef::Cut {
                id: scc_spatial::SpatialCutId("a_hole:cut:0000".into()),
            },
        );
        map
    }

    #[test]
    fn element_for_primitive_finds_recorded_sources() {
        let map = sample();
        assert_eq!(
            map.element_for_primitive(&pid("front:body:p:0000")),
            Some(&SpatialElementRef::Body)
        );
        assert_eq!(map.element_for_primitive(&pid("ghost")), None);
    }

    #[test]
    fn element_index_is_derived_on_demand() {
        let map = sample();
        let index = map.element_index();
        assert_eq!(index.len(), 2, "body + one cut");
        let cut = SpatialElementRef::Cut {
            id: scc_spatial::SpatialCutId("a_hole:cut:0000".into()),
        };
        assert_eq!(index[&cut].len(), 2);
        assert_eq!(map.primitives_for_element(&cut).len(), 2);
        assert_eq!(
            map.primitives_for_element(&SpatialElementRef::Body).len(),
            1
        );
    }

    #[test]
    fn iter_is_deterministic_and_complete() {
        let map = sample();
        let ids: Vec<&ProjectionPrimitiveId> = map.iter().map(|(id, _)| id).collect();
        assert_eq!(ids.len(), 3);
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "iter must be id-ascending (BTreeMap order)");
    }

    #[test]
    fn coverage_check_matches_exactly() {
        let map = sample();
        assert!(map.covers_exactly(
            [
                pid("front:body:p:0000"),
                pid("top:a_hole:cut:0000:p:0000"),
                pid("left:a_hole:cut:0000:p:0000")
            ]
            .into_iter()
        ));
        assert!(!map.covers_exactly([pid("front:body:p:0000")].into_iter()));
    }
}
