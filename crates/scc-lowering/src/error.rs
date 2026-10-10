use scc_core::authority::AuthorityLevel;
use scc_core::constraint::PARAM_VALUE;
use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::{ConstraintId, EntityId};

/// A structured error produced while lowering [`SemanticIr`] to [`ConstraintIr`].
#[derive(Debug, Clone, PartialEq)]
pub enum LoweringError {
    UnsupportedEntityKind {
        entity_id: EntityId,
        kind: String,
    },
    UnsupportedRelation {
        relation_kind: String,
    },
    UnsupportedDimension {
        dimension_kind: String,
    },
    InvalidEntityReference {
        entity_id: EntityId,
    },
    ConstraintIdCollision {
        constraint_id: ConstraintId,
    },
    NonFiniteParameter {
        constraint_id: ConstraintId,
        parameter: String,
        value: f64,
    },
    InvalidAuthorityAssignment {
        constraint_id: ConstraintId,
        authority: AuthorityLevel,
    },
    AnchorLoweringFailure {
        anchor_id: EntityId,
        reason: String,
    },
}

impl LoweringError {
    pub fn code(&self) -> &'static str {
        match self {
            LoweringError::UnsupportedEntityKind { .. } => "SCC-LOW-001",
            LoweringError::UnsupportedRelation { .. } => "SCC-LOW-002",
            LoweringError::UnsupportedDimension { .. } => "SCC-LOW-003",
            LoweringError::InvalidEntityReference { .. } => "SCC-LOW-004",
            LoweringError::ConstraintIdCollision { .. } => "SCC-LOW-005",
            LoweringError::NonFiniteParameter { .. } => "SCC-LOW-006",
            LoweringError::InvalidAuthorityAssignment { .. } => "SCC-LOW-007",
            LoweringError::AnchorLoweringFailure { .. } => "SCC-LOW-008",
        }
    }
}

impl From<LoweringError> for StructuredError {
    fn from(err: LoweringError) -> Self {
        let code = err.code();
        match err {
            LoweringError::UnsupportedEntityKind { entity_id, kind: _ } => {
                StructuredError::new(code, ErrorStage::Lowering, "unsupported entity kind")
                    .with_entity_ids(vec![entity_id.clone()])
                    .with_suggestion("use a supported entity kind")
            }
            LoweringError::UnsupportedRelation { relation_kind: _ } => {
                StructuredError::new(code, ErrorStage::Lowering, "unsupported relation kind")
                    .with_suggestion("use a supported relation kind")
            }
            LoweringError::UnsupportedDimension { dimension_kind: _ } => {
                StructuredError::new(code, ErrorStage::Lowering, "unsupported dimension kind")
                    .with_suggestion("use a supported dimension kind")
            }
            LoweringError::InvalidEntityReference { entity_id } => StructuredError::new(
                code,
                ErrorStage::Lowering,
                "constraint references missing entity",
            )
            .with_entity_ids(vec![entity_id])
            .with_suggestion("ensure the referenced entity exists in the semantic IR"),
            LoweringError::ConstraintIdCollision { constraint_id } => {
                StructuredError::new(code, ErrorStage::Lowering, "constraint id collision")
                    .with_constraint_ids(vec![constraint_id])
                    .with_recoverable(false)
            }
            LoweringError::NonFiniteParameter {
                constraint_id,
                parameter,
                value,
            } => StructuredError::new(code, ErrorStage::Lowering, "non-finite parameter value")
                .with_constraint_ids(vec![constraint_id.clone()])
                .with_path(format!(
                    "/constraints/{}/parameters/{}",
                    constraint_id, parameter
                ))
                .with_suggestion(format!("replace {} with a finite numeric value", value)),
            LoweringError::InvalidAuthorityAssignment {
                constraint_id,
                authority,
            } => StructuredError::new(code, ErrorStage::Lowering, "invalid authority assignment")
                .with_constraint_ids(vec![constraint_id])
                .with_suggestion(format!("review authority level {:?}", authority)),
            LoweringError::AnchorLoweringFailure { anchor_id, reason } => {
                StructuredError::new(code, ErrorStage::Lowering, reason)
                    .with_entity_ids(vec![anchor_id])
                    .with_suggestion("check anchor definition and referenced entity")
            }
        }
    }
}

impl From<LoweringError> for SccError {
    fn from(err: LoweringError) -> Self {
        SccError::Structured(Box::new(err.into()))
    }
}

/// Validate that all numeric parameters in a lowered [`ConstraintIr`] are finite.
pub fn validate_finite_parameters(
    ir: &scc_core::constraint_ir::ConstraintIr,
) -> Result<(), LoweringError> {
    for constraint in &ir.constraints {
        if let Some(value) = constraint.parameters.get(PARAM_VALUE) {
            if !value.is_finite() {
                return Err(LoweringError::NonFiniteParameter {
                    constraint_id: constraint.id.clone(),
                    parameter: PARAM_VALUE.into(),
                    value: *value,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::constraint_ir::ConstraintIr;

    #[test]
    fn lowering_error_contract_validation() {
        let err = LoweringError::NonFiniteParameter {
            constraint_id: ConstraintId("c_1".into()),
            parameter: PARAM_VALUE.into(),
            value: f64::NAN,
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-LOW-006");
        structured.validate_contract().unwrap();
    }

    #[test]
    fn finite_parameters_pass_validation() {
        let ir = ConstraintIr {
            entities: vec![],
            constraints: vec![],
            graph: scc_core::graph::DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: scc_core::constraint_ir::IrMetadata {
                constraint_levels: scc_core::constraint_ir::IrConstraintLevels::default(),
            },
        };
        assert!(validate_finite_parameters(&ir).is_ok());
    }
}
