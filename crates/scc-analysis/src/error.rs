use scc_core::conflict::{
    Conflict, ConflictCategory, ConflictFacts, ConflictType, MinimalConflictSubgraph,
};
use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::{ArrayId, EntityId};

/// A structured semantic diagnostic produced by `scc-analysis`.
///
/// These are distinct from [`Conflict`]: an `AnalysisError` means the input is
/// not safe to analyze or an analysis invariant failed, whereas a `Conflict`
/// represents a business-level contradiction that can be reported to the user.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisError {
    /// A relation or dimension references an entity that does not exist.
    MissingEntityReference { entity_id: EntityId, path: String },
    /// Entities are used in a relation that is incompatible with their kinds.
    TypeMismatch {
        entity_ids: Vec<EntityId>,
        path: String,
        message: String,
    },
    /// The declaration graph contains a cycle.
    CircularDependency { path: Vec<String> },
    /// The declaration graph or semantic IR is structurally invalid.
    InvalidDeclarationGraph { message: String },
    /// An array references a non-existent element entity.
    InvalidArrayReference {
        array_id: ArrayId,
        element_id: EntityId,
        index: usize,
    },
    /// An anchor references a non-existent entity.
    InvalidAnchorReference {
        anchor_id: EntityId,
        entity_id: EntityId,
        index: usize,
    },
}

impl AnalysisError {
    pub fn code(&self) -> &'static str {
        match self {
            AnalysisError::MissingEntityReference { .. } => "SCC-ANAL-001",
            AnalysisError::TypeMismatch { .. } => "SCC-ANAL-002",
            AnalysisError::CircularDependency { .. } => "SCC-ANAL-003",
            AnalysisError::InvalidDeclarationGraph { .. } => "SCC-ANAL-004",
            AnalysisError::InvalidArrayReference { .. } => "SCC-ANAL-005",
            AnalysisError::InvalidAnchorReference { .. } => "SCC-ANAL-006",
        }
    }
}

impl From<AnalysisError> for StructuredError {
    fn from(err: AnalysisError) -> Self {
        let code = err.code();
        match err {
            AnalysisError::MissingEntityReference { entity_id, path } => StructuredError::new(
                code,
                ErrorStage::Analysis,
                "referenced entity does not exist",
            )
            .with_path(path)
            .with_entity_ids(vec![entity_id])
            .with_recoverable(true)
            .with_suggestion("declare the referenced entity before using it"),
            AnalysisError::TypeMismatch {
                entity_ids,
                path,
                message,
            } => StructuredError::new(code, ErrorStage::Analysis, message)
                .with_path(path)
                .with_entity_ids(entity_ids)
                .with_recoverable(true)
                .with_suggestion("check entity kinds against the relation requirements"),
            AnalysisError::CircularDependency { path } => {
                let ids: Vec<EntityId> = path.iter().map(|s| EntityId(s.clone())).collect();
                StructuredError::new(code, ErrorStage::Analysis, "circular dependency detected")
                    .with_entity_ids(ids)
                    .with_recoverable(true)
                    .with_suggestion("break the cycle by removing or reordering relations")
            }
            AnalysisError::InvalidDeclarationGraph { message } => {
                StructuredError::new(code, ErrorStage::Analysis, message).with_recoverable(false)
            }
            AnalysisError::InvalidArrayReference {
                array_id: _,
                element_id,
                index,
            } => StructuredError::new(
                code,
                ErrorStage::Analysis,
                "array references missing element",
            )
            .with_path(format!("/arrays/{}/element", index))
            .with_entity_ids(vec![element_id])
            .with_recoverable(true)
            .with_suggestion("ensure the array element entity exists"),
            AnalysisError::InvalidAnchorReference {
                anchor_id,
                entity_id,
                index,
            } => StructuredError::new(
                code,
                ErrorStage::Analysis,
                "anchor references missing entity",
            )
            .with_path(format!("/anchors/{}/entity", index))
            .with_entity_ids(vec![entity_id, anchor_id])
            .with_recoverable(true)
            .with_suggestion("ensure the anchor entity exists"),
        }
    }
}

impl From<AnalysisError> for SccError {
    fn from(err: AnalysisError) -> Self {
        SccError::Structured(Box::new(err.into()))
    }
}

impl AnalysisError {
    /// Convert this error to the legacy [`Conflict`] representation used by the
    /// v0.1.0 `analyze()` API.
    pub fn into_conflict(self) -> Conflict {
        match self {
            AnalysisError::MissingEntityReference { entity_id, .. } => conflict(
                ConflictType::MissingReference,
                vec![entity_id.clone()],
                format!("relation references undefined entity: {}", entity_id.0),
            ),
            AnalysisError::TypeMismatch {
                entity_ids,
                message,
                ..
            } => conflict(ConflictType::TypeMismatch, entity_ids, message),
            AnalysisError::CircularDependency { path } => {
                let entities: Vec<EntityId> = path.iter().map(|s| EntityId(s.clone())).collect();
                conflict(
                    ConflictType::CircularDependency,
                    entities.clone(),
                    format!("circular dependency detected: {}", path.join(" -> ")),
                )
            }
            AnalysisError::InvalidDeclarationGraph { message } => {
                conflict(ConflictType::InvalidEntity, vec![], message)
            }
            AnalysisError::InvalidArrayReference {
                array_id,
                element_id,
                ..
            } => conflict(
                ConflictType::MissingReference,
                vec![element_id.clone()],
                format!(
                    "array {} references undefined element: {}",
                    array_id.0, element_id.0
                ),
            ),
            AnalysisError::InvalidAnchorReference {
                anchor_id,
                entity_id,
                ..
            } => conflict(
                ConflictType::MissingReference,
                vec![entity_id.clone(), anchor_id.clone()],
                format!(
                    "anchor {} references undefined entity: {}",
                    anchor_id.0, entity_id.0
                ),
            ),
        }
    }
}

fn conflict<T: IntoIterator<Item = EntityId>>(
    conflict_type: ConflictType,
    affected_entities: T,
    message: String,
) -> Conflict {
    let affected_entities: Vec<EntityId> = affected_entities.into_iter().collect();
    Conflict {
        category: ConflictCategory::Structural,
        conflict_type,
        mcs: MinimalConflictSubgraph {
            conflicting_constraints: vec![],
            conflict_type,
        },
        facts: ConflictFacts {
            affected_entities,
            affected_constraints: vec![],
            message,
        },
    }
}

/// Multiple semantic analysis errors returned together.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisErrors {
    pub errors: Vec<AnalysisError>,
}

impl AnalysisErrors {
    /// Convert all errors to legacy [`Conflict`] values, preserving order.
    pub fn into_conflicts(self) -> Vec<Conflict> {
        self.errors.into_iter().map(|e| e.into_conflict()).collect()
    }
}

impl From<AnalysisErrors> for SccError {
    fn from(errors: AnalysisErrors) -> Self {
        let structured: Vec<StructuredError> = errors
            .errors
            .into_iter()
            .map(StructuredError::from)
            .collect();
        match structured.len() {
            0 => SccError::Structured(Box::new(StructuredError::new(
                "SCC-ANAL-007",
                ErrorStage::Analysis,
                "internal analysis invariant failed",
            ))),
            1 => SccError::Structured(Box::new(structured.into_iter().next().unwrap())),
            _ => SccError::StructuredBatch(structured),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_entity_reference_structured_error_passes_contract() {
        let err = AnalysisError::MissingEntityReference {
            entity_id: EntityId("missing".into()),
            path: "/relations/0/reference".into(),
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-ANAL-001");
        assert_eq!(structured.path, Some("/relations/0/reference".into()));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn type_mismatch_structured_error_passes_contract() {
        let err = AnalysisError::TypeMismatch {
            entity_ids: vec![EntityId("a".into()), EntityId("b".into())],
            path: "/relations/0".into(),
            message: "parallel relation requires line entities".into(),
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-ANAL-002");
        structured.validate_contract().unwrap();
    }

    #[test]
    fn circular_dependency_structured_error_passes_contract() {
        let err = AnalysisError::CircularDependency {
            path: vec!["a".into(), "b".into(), "a".into()],
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-ANAL-003");
        structured.validate_contract().unwrap();
    }

    #[test]
    fn multiple_analysis_errors_are_preserved_in_scc_error() {
        let errors = AnalysisErrors {
            errors: vec![
                AnalysisError::MissingEntityReference {
                    entity_id: EntityId("missing-a".into()),
                    path: "/relations/0/subject".into(),
                },
                AnalysisError::MissingEntityReference {
                    entity_id: EntityId("missing-b".into()),
                    path: "/relations/0/reference".into(),
                },
            ],
        };

        match SccError::from(errors) {
            SccError::StructuredBatch(errors) => {
                assert_eq!(errors.len(), 2);
                assert_eq!(errors[0].path.as_deref(), Some("/relations/0/subject"));
                assert_eq!(errors[1].path.as_deref(), Some("/relations/0/reference"));
            }
            other => panic!("expected structured batch, got {other:?}"),
        }
    }
}
