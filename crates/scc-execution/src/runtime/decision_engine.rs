use crate::execution_plan::RuntimeCommand;
use crate::runtime::runtime_error::RuntimeError;
use scc_core::constraint::ConstraintKind;
use scc_core::runtime::{GraphState, ValidationError};

/// Decision produced by the runtime decision engine for each execution step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Execute,
    Conflict,
}

/// Evaluates each planned command against the current GraphState before dispatch.
pub struct DecisionEngine;

impl DecisionEngine {
    /// Evaluates `command` against `state` and returns the decision.
    pub fn evaluate(
        state: &GraphState,
        command: &RuntimeCommand,
    ) -> Result<Decision, RuntimeError> {
        match command {
            RuntimeCommand::CreateEntity { id, .. } => {
                if state.entities.contains_key(id) {
                    Ok(Decision::Conflict)
                } else {
                    Ok(Decision::Execute)
                }
            }
            RuntimeCommand::AddConstraint {
                id,
                kind,
                source,
                target,
                ..
            } => {
                if state.constraints.contains_key(id) {
                    Ok(Decision::Conflict)
                } else {
                    match kind {
                        ConstraintKind::ArrayRelation => {
                            if !state.entities.contains_key(source) {
                                return Err(RuntimeError::ValidationError(
                                    ValidationError::MissingEntity {
                                        constraint_id: id.clone(),
                                        entity_id: source.clone(),
                                    },
                                ));
                            }
                            Ok(Decision::Execute)
                        }
                        ConstraintKind::InstanceOf => {
                            if !state.entities.contains_key(target) {
                                return Err(RuntimeError::ValidationError(
                                    ValidationError::MissingEntity {
                                        constraint_id: id.clone(),
                                        entity_id: target.clone(),
                                    },
                                ));
                            }
                            Ok(Decision::Execute)
                        }
                        _ => {
                            if !state.entities.contains_key(source) {
                                return Err(RuntimeError::ValidationError(
                                    ValidationError::MissingEntity {
                                        constraint_id: id.clone(),
                                        entity_id: source.clone(),
                                    },
                                ));
                            }
                            if !state.entities.contains_key(target) {
                                return Err(RuntimeError::ValidationError(
                                    ValidationError::MissingEntity {
                                        constraint_id: id.clone(),
                                        entity_id: target.clone(),
                                    },
                                ));
                            }
                            Ok(Decision::Execute)
                        }
                    }
                }
            }
            RuntimeCommand::SetParameter { entity, .. } => {
                if state.entities.contains_key(entity) {
                    Ok(Decision::Execute)
                } else {
                    Err(RuntimeError::EntityNotFound {
                        id: entity.0.clone(),
                    })
                }
            }
            RuntimeCommand::ValidateGraph => Ok(Decision::Execute),
            RuntimeCommand::MarkSnapshot => Ok(Decision::Execute),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::authority::AuthorityLevel;
    use scc_core::constraint::ConstraintKind;
    use scc_core::entity::EntityKind;
    use scc_core::error::StructuredError;
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::runtime::{
        ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
    };

    fn state_with_entity(id: &str) -> GraphState {
        let mut state = GraphState::new(0);
        let id = EntityId(id.into());
        state.entities.insert(
            id.clone(),
            RuntimeEntity {
                id,
                kind: EntityKind::Point,
                parameters: Default::default(),
            },
        );
        state
    }

    fn add_constraint_with_kind(
        kind: ConstraintKind,
        source: &str,
        target: &str,
    ) -> RuntimeCommand {
        RuntimeCommand::AddConstraint {
            id: ConstraintId("constraint".into()),
            kind,
            source: EntityId(source.into()),
            target: EntityId(target.into()),
            authority: AuthorityLevel::L1,
            anchor_role: None,
            parameters: Default::default(),
        }
    }

    fn add_constraint(source: &str, target: &str) -> RuntimeCommand {
        add_constraint_with_kind(ConstraintKind::Distance, source, target)
    }

    fn assert_constraint_missing_entity(error: RuntimeError, expected_entity: &str) {
        let structured: StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-001");
        assert_eq!(
            structured.entity_ids,
            Some(vec![EntityId(expected_entity.into())])
        );
        assert_eq!(
            structured.constraint_ids,
            Some(vec![ConstraintId("constraint".into())])
        );
        structured.validate_contract().unwrap();
    }

    #[test]
    fn missing_constraint_source_returns_entity_not_found() {
        let state = state_with_entity("target");
        let error =
            DecisionEngine::evaluate(&state, &add_constraint("source", "target")).unwrap_err();
        assert_constraint_missing_entity(error, "source");
    }

    #[test]
    fn missing_constraint_target_returns_entity_not_found() {
        let state = state_with_entity("source");
        let error =
            DecisionEngine::evaluate(&state, &add_constraint("source", "target")).unwrap_err();
        assert_constraint_missing_entity(error, "target");
    }

    #[test]
    fn two_missing_constraint_entities_report_source_first() {
        let state = GraphState::new(0);
        let error =
            DecisionEngine::evaluate(&state, &add_constraint("source", "target")).unwrap_err();
        assert_constraint_missing_entity(error, "source");
    }

    #[test]
    fn array_relation_requires_only_its_source_entity() {
        let state = state_with_entity("source");
        let command =
            add_constraint_with_kind(ConstraintKind::ArrayRelation, "source", "array-pseudo-node");
        assert_eq!(
            DecisionEngine::evaluate(&state, &command).unwrap(),
            Decision::Execute
        );

        let missing_source = add_constraint_with_kind(
            ConstraintKind::ArrayRelation,
            "missing",
            "array-pseudo-node",
        );
        let error = DecisionEngine::evaluate(&state, &missing_source).unwrap_err();
        assert_constraint_missing_entity(error, "missing");
    }

    #[test]
    fn instance_of_requires_only_its_target_entity() {
        let state = state_with_entity("target");
        let command =
            add_constraint_with_kind(ConstraintKind::InstanceOf, "array-pseudo-node", "target");
        assert_eq!(
            DecisionEngine::evaluate(&state, &command).unwrap(),
            Decision::Execute
        );

        let missing_target =
            add_constraint_with_kind(ConstraintKind::InstanceOf, "array-pseudo-node", "missing");
        let error = DecisionEngine::evaluate(&state, &missing_target).unwrap_err();
        assert_constraint_missing_entity(error, "missing");
    }

    #[test]
    fn missing_parameter_target_returns_stable_structured_error() {
        let state = GraphState::new(0);
        let command = RuntimeCommand::SetParameter {
            entity: EntityId("missing".into()),
            name: "radius".into(),
            value: ParameterValue::Numeric(2.0),
        };
        let error = DecisionEngine::evaluate(&state, &command).unwrap_err();
        let structured: StructuredError = error.into();

        assert_eq!(structured.code, "SCC-EXEC-001");
        assert_eq!(
            structured.entity_ids,
            Some(vec![EntityId("missing".into())])
        );
        structured.validate_contract().unwrap();
    }

    #[test]
    fn duplicate_entity_remains_a_runtime_conflict() {
        let state = state_with_entity("entity");
        let command = RuntimeCommand::CreateEntity {
            id: EntityId("entity".into()),
            kind: EntityKind::Point,
        };

        assert_eq!(
            DecisionEngine::evaluate(&state, &command).unwrap(),
            Decision::Conflict
        );
    }

    #[test]
    fn duplicate_constraint_remains_a_runtime_conflict() {
        let mut state = state_with_entity("entity");
        let id = ConstraintId("constraint".into());
        state.constraints.insert(
            id.clone(),
            RuntimeConstraint {
                id,
                kind: ConstraintKind::Distance,
                source: EntityId("entity".into()),
                target: EntityId("entity".into()),
                authority: AuthorityLevel::L1,
                state: ConstraintRuntimeState::Active,
                anchor_role: None,
                parameters: Default::default(),
            },
        );
        let command = add_constraint("entity", "entity");

        assert_eq!(
            DecisionEngine::evaluate(&state, &command).unwrap(),
            Decision::Conflict
        );
    }
}
