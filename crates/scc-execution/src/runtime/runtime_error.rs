use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::runtime::ValidationError;
use thiserror::Error;

/// Whether an overlay target refers to an entity or a constraint.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayTargetKind {
    Entity,
    Constraint,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("entity not found: {id}")]
    EntityNotFound { id: String },
    #[error("constraint not found: {id}")]
    ConstraintNotFound { id: String },
    #[error("validation failed: {message}")]
    ValidationFailed { message: String },
    #[error("validation failed: {0}")]
    ValidationError(ValidationError),
    #[error("overlay target not found: {id}")]
    OverlayApplyFailed { id: String, kind: OverlayTargetKind },
    #[error("conflict detected: {message}")]
    ConflictDetected { message: String },
    #[error("failed to canonicalize {target}: {message}")]
    CanonicalizationFailed {
        target: &'static str,
        message: String,
    },
    #[error("constraint is frozen at L0 authority: {id}")]
    FrozenConstraint { id: String },
    #[error("constraint cannot be promoted to L0 authority: {id}")]
    CannotPromoteToL0 { id: String },
    #[error("negative value not allowed for parameter: {parameter}")]
    NegativeParameter { parameter: String },
}

impl From<RuntimeError> for StructuredError {
    fn from(err: RuntimeError) -> Self {
        match err {
            RuntimeError::EntityNotFound { id } => {
                StructuredError::new("SCC-EXEC-001", ErrorStage::Execution, "entity not found")
                    .with_entity_ids(vec![EntityId(id)])
                    .with_recoverable(true)
            }
            RuntimeError::ConstraintNotFound { id } => StructuredError::new(
                "SCC-EXEC-004",
                ErrorStage::Execution,
                "constraint not found",
            )
            .with_constraint_ids(vec![ConstraintId(id)])
            .with_recoverable(true),
            RuntimeError::ValidationFailed { message } => {
                StructuredError::new("SCC-EXEC-009", ErrorStage::Execution, message)
                    .with_recoverable(false)
            }
            RuntimeError::ValidationError(e) => {
                let mut structured: StructuredError = e.into();
                structured.recoverable = true;
                structured
            }
            RuntimeError::OverlayApplyFailed { id, kind } => {
                let mut structured = StructuredError::new(
                    "SCC-EXEC-005",
                    ErrorStage::Execution,
                    "overlay target not found",
                )
                .with_recoverable(true)
                .with_suggestion("ensure the overlay target exists in the graph state");
                match kind {
                    OverlayTargetKind::Entity => structured.entity_ids = Some(vec![EntityId(id)]),
                    OverlayTargetKind::Constraint => {
                        structured.constraint_ids = Some(vec![ConstraintId(id)])
                    }
                }
                structured
            }
            RuntimeError::ConflictDetected { message } => {
                StructuredError::new("SCC-EXEC-007", ErrorStage::Execution, message)
                    .with_recoverable(true)
            }
            RuntimeError::CanonicalizationFailed { target, message } => StructuredError::new(
                "SCC-EXEC-013",
                ErrorStage::Execution,
                format!("failed to canonicalize {target}: {message}"),
            )
            .with_path(target)
            .with_recoverable(false)
            .with_suggestion("ensure the execution input can be serialized deterministically"),
            RuntimeError::FrozenConstraint { id } => StructuredError::new(
                "SCC-EXEC-002",
                ErrorStage::Execution,
                "constraint is frozen at L0 authority",
            )
            .with_constraint_ids(vec![ConstraintId(id)])
            .with_recoverable(false)
            .with_suggestion("do not suppress, unsuppress, or re-authorize an L0 constraint"),
            RuntimeError::CannotPromoteToL0 { id } => StructuredError::new(
                "SCC-EXEC-011",
                ErrorStage::Execution,
                "constraint cannot be promoted to L0 authority",
            )
            .with_constraint_ids(vec![ConstraintId(id)])
            .with_recoverable(false),
            RuntimeError::NegativeParameter { parameter } => StructuredError::new(
                "SCC-EXEC-012",
                ErrorStage::Execution,
                "negative value not allowed for parameter",
            )
            .with_path(parameter)
            .with_recoverable(true)
            .with_suggestion("use a non-negative value"),
        }
    }
}

impl From<RuntimeError> for SccError {
    fn from(err: RuntimeError) -> Self {
        SccError::Structured(Box::new(err.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::error::ErrorStage;

    #[test]
    fn entity_not_found_structured_error_passes_contract() {
        let err = RuntimeError::EntityNotFound {
            id: "missing".into(),
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-EXEC-001");
        assert_eq!(structured.stage, ErrorStage::Execution);
        assert_eq!(
            structured.entity_ids,
            Some(vec![EntityId("missing".into())])
        );
        structured.validate_contract().unwrap();
    }

    #[test]
    fn validation_error_structured_error_passes_contract() {
        let err =
            RuntimeError::ValidationError(scc_core::runtime::ValidationError::MissingEntity {
                constraint_id: ConstraintId("c1".into()),
                entity_id: EntityId("e1".into()),
            });
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-EXEC-001");
        structured.validate_contract().unwrap();
    }

    #[test]
    fn runtime_error_converts_to_scc_error() {
        let err = RuntimeError::EntityNotFound {
            id: "missing".into(),
        };
        let scc_err: SccError = err.into();
        match scc_err {
            SccError::Structured(e) => {
                assert_eq!(e.code, "SCC-EXEC-001");
            }
            other => panic!("expected structured error, got {:?}", other),
        }
    }

    #[test]
    fn canonicalization_failure_preserves_structured_context() {
        let err = RuntimeError::CanonicalizationFailed {
            target: "base_ir",
            message: "serialization failed".into(),
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-EXEC-013");
        assert_eq!(structured.stage, ErrorStage::Execution);
        assert_eq!(structured.path.as_deref(), Some("base_ir"));
        assert!(!structured.recoverable);
        structured.validate_contract().unwrap();
    }

    #[test]
    fn frozen_constraint_error_preserves_constraint_context() {
        let structured: StructuredError = RuntimeError::FrozenConstraint { id: "l0".into() }.into();
        assert_eq!(structured.code, "SCC-EXEC-002");
        assert_eq!(
            structured.constraint_ids,
            Some(vec![ConstraintId("l0".into())])
        );
        structured.validate_contract().unwrap();
    }

    #[test]
    fn promote_to_l0_error_preserves_constraint_context() {
        let structured: StructuredError =
            RuntimeError::CannotPromoteToL0 { id: "l1".into() }.into();
        assert_eq!(structured.code, "SCC-EXEC-011");
        assert_eq!(
            structured.constraint_ids,
            Some(vec![ConstraintId("l1".into())])
        );
        structured.validate_contract().unwrap();
    }

    #[test]
    fn negative_parameter_error_preserves_parameter_context() {
        let structured: StructuredError = RuntimeError::NegativeParameter {
            parameter: "diameter".into(),
        }
        .into();
        assert_eq!(structured.code, "SCC-EXEC-012");
        assert_eq!(structured.path.as_deref(), Some("diameter"));
        structured.validate_contract().unwrap();
    }
}
