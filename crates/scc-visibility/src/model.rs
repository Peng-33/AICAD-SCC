//! The minimal semantic sidecar produced by the visibility classifier.

use std::collections::{BTreeMap, BTreeSet};

use scc_projection::{PrincipalViewKind, ProjectionModel, ProjectionPrimitiveId};
use serde::{Deserialize, Serialize};

use crate::error::VisibilityError;

/// The three semantic outcomes of one raw projection candidate.
///
/// Presentation such as solid or dashed strokes belongs to a later drawing
/// phase. Occluded means that this raw candidate contributes no independent
/// final stroke; it does not claim that every point is physically covered by
/// material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityClass {
    Visible,
    Hidden,
    Occluded,
}

/// The decision for one existing projection primitive id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityDecision {
    pub class: VisibilityClass,
}

/// Decisions for one frozen principal view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityView {
    pub decisions: BTreeMap<ProjectionPrimitiveId, VisibilityDecision>,
}

/// The derived, read-only visibility sidecar.
///
/// Geometry, depth, source, feature, and entity data remain exclusively in
/// the input projection/spatial models. Serde is transport/debug support; a
/// deserialized value must pass validate_against before use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityModel {
    pub views: BTreeMap<PrincipalViewKind, VisibilityView>,
}

impl VisibilityModel {
    /// Verify exact view and primitive coverage against a projection model.
    /// This is intentionally explicit rather than a serde trust boundary.
    pub fn validate_against(&self, projection: &ProjectionModel) -> Result<(), VisibilityError> {
        let expected = [
            PrincipalViewKind::Front,
            PrincipalViewKind::Top,
            PrincipalViewKind::Left,
        ];
        if self.views.len() != expected.len() {
            return Err(VisibilityError::InvalidVisibilityDecision {
                view: None,
                id: None,
                reason: format!("expected exactly {} principal views", expected.len()),
            });
        }

        for kind in expected {
            let projection_view = projection.views.get(&kind).ok_or_else(|| {
                VisibilityError::InvalidVisibilityDecision {
                    view: Some(kind),
                    id: None,
                    reason: "projection view is missing".into(),
                }
            })?;
            let visibility_view =
                self.views
                    .get(&kind)
                    .ok_or(VisibilityError::MissingVisibilityDecision {
                        view: kind,
                        id: ProjectionPrimitiveId(format!("{kind:?}:view")),
                    })?;

            if visibility_view.decisions.len() != projection_view.primitives.len() {
                return Err(VisibilityError::InvalidVisibilityDecision {
                    view: Some(kind),
                    id: None,
                    reason: "decision count does not equal projection primitive count".into(),
                });
            }

            for id in projection_view.primitives.keys() {
                if !visibility_view.decisions.contains_key(id) {
                    return Err(VisibilityError::MissingVisibilityDecision {
                        view: kind,
                        id: id.clone(),
                    });
                }
            }
            for id in visibility_view.decisions.keys() {
                if !projection_view.primitives.contains_key(id) {
                    return Err(VisibilityError::InvalidVisibilityDecision {
                        view: Some(kind),
                        id: Some(id.clone()),
                        reason: "orphan decision is not present in projection".into(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Query one decision without exposing or copying projection geometry.
    pub fn class_for(
        &self,
        view: PrincipalViewKind,
        id: &ProjectionPrimitiveId,
    ) -> Option<VisibilityClass> {
        self.views
            .get(&view)
            .and_then(|view| view.decisions.get(id))
            .map(|decision| decision.class)
    }

    /// Return all ids in one class, in deterministic id order.
    pub fn ids_for(
        &self,
        view: PrincipalViewKind,
        class: VisibilityClass,
    ) -> BTreeSet<ProjectionPrimitiveId> {
        self.views
            .get(&view)
            .map(|view| {
                view.decisions
                    .iter()
                    .filter_map(|(id, decision)| (decision.class == class).then_some(id.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn visible_ids(&self, view: PrincipalViewKind) -> BTreeSet<ProjectionPrimitiveId> {
        self.ids_for(view, VisibilityClass::Visible)
    }

    pub fn hidden_ids(&self, view: PrincipalViewKind) -> BTreeSet<ProjectionPrimitiveId> {
        self.ids_for(view, VisibilityClass::Hidden)
    }

    pub fn occluded_ids(&self, view: PrincipalViewKind) -> BTreeSet<ProjectionPrimitiveId> {
        self.ids_for(view, VisibilityClass::Occluded)
    }
}
