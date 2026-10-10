use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::{ConstraintId, EntityId};
use thiserror::Error;

/// Whether a mutation target refers to an entity or a constraint.
#[derive(Debug, Clone, PartialEq)]
pub enum MutationTargetKind {
    Entity,
    Constraint,
}

#[derive(Debug, Error)]
pub enum MutationError {
    #[error("target not found: {id}")]
    TargetNotFound {
        id: String,
        kind: MutationTargetKind,
    },
    #[error("cannot mutate frozen L0 constraint: {id}")]
    FrozenConstraint { id: String },
    #[error("cannot promote constraint to L0: {id}")]
    CannotPromoteToL0 { id: String },
    #[error("negative value not allowed for parameter")]
    NegativeValue,
    #[error("mutation validation failed: {message}")]
    ValidationFailed { message: String },
}

impl From<MutationError> for StructuredError {
    fn from(err: MutationError) -> Self {
        match err {
            MutationError::TargetNotFound { id, kind } => {
                let mut structured = StructuredError::new(
                    "SCC-EXEC-001",
                    ErrorStage::Execution,
                    "mutation target not found",
                )
                .with_recoverable(true)
                .with_suggestion("ensure the target entity or constraint exists");
                match kind {
                    MutationTargetKind::Entity => structured.entity_ids = Some(vec![EntityId(id)]),
                    MutationTargetKind::Constraint => {
                        structured.constraint_ids = Some(vec![ConstraintId(id)])
                    }
                }
                structured
            }
            MutationError::FrozenConstraint { id } => StructuredError::new(
                "SCC-EXEC-002",
                ErrorStage::Execution,
                "cannot mutate frozen L0 constraint",
            )
            .with_constraint_ids(vec![ConstraintId(id)])
            .with_recoverable(true)
            .with_suggestion("L0 constraints are immutable; modify a higher-level intent instead"),
            MutationError::CannotPromoteToL0 { id } => StructuredError::new(
                "SCC-EXEC-011",
                ErrorStage::Execution,
                "cannot promote constraint to L0",
            )
            .with_constraint_ids(vec![ConstraintId(id)])
            .with_recoverable(true)
            .with_suggestion("L0 authority cannot be assigned through mutation"),
            MutationError::NegativeValue => StructuredError::new(
                "SCC-EXEC-012",
                ErrorStage::Execution,
                "negative value not allowed for parameter",
            )
            .with_recoverable(true)
            .with_suggestion("use a non-negative value"),
            MutationError::ValidationFailed { message } => {
                StructuredError::new("SCC-EXEC-010", ErrorStage::Execution, message)
                    .with_recoverable(false)
            }
        }
    }
}

impl From<MutationError> for SccError {
    fn from(err: MutationError) -> Self {
        SccError::Structured(Box::new(err.into()))
    }
}
