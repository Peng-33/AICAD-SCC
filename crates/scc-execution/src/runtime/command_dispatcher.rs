use crate::execution_plan::RuntimeCommand;
use crate::runtime::runtime_error::RuntimeError;
use scc_core::ids::SnapshotId;
use scc_core::runtime::{ConstraintRuntimeState, GraphState, RuntimeConstraint, RuntimeEntity};

pub struct CommandDispatcher;

impl CommandDispatcher {
    pub fn dispatch(
        state: &mut GraphState,
        command: &RuntimeCommand,
    ) -> Result<Option<SnapshotId>, RuntimeError> {
        match command {
            RuntimeCommand::CreateEntity { id, kind } => {
                state.entities.insert(
                    id.clone(),
                    RuntimeEntity {
                        id: id.clone(),
                        kind: *kind,
                        parameters: Default::default(),
                    },
                );
                state.advance_step();
                Ok(Some(state.current_snapshot()))
            }

            RuntimeCommand::SetParameter {
                entity,
                name,
                value,
            } => {
                if let Some(ent) = state.entities.get_mut(entity) {
                    ent.parameters.insert(name.clone(), value.clone());
                    state.advance_step();
                    Ok(Some(state.current_snapshot()))
                } else {
                    Err(RuntimeError::EntityNotFound {
                        id: entity.0.clone(),
                    })
                }
            }

            RuntimeCommand::AddConstraint {
                id,
                kind,
                source,
                target,
                authority,
                anchor_role,
                parameters,
            } => {
                state.constraints.insert(
                    id.clone(),
                    RuntimeConstraint {
                        id: id.clone(),
                        kind: kind.clone(),
                        source: source.clone(),
                        target: target.clone(),
                        authority: *authority,
                        state: ConstraintRuntimeState::Active,
                        anchor_role: *anchor_role,
                        parameters: parameters.clone(),
                    },
                );
                state.advance_step();
                Ok(Some(state.current_snapshot()))
            }

            RuntimeCommand::ValidateGraph => {
                for constraint in state.constraints.values() {
                    match constraint.kind {
                        scc_core::constraint::ConstraintKind::ArrayRelation
                        | scc_core::constraint::ConstraintKind::InstanceOf => {}
                        _ => {
                            if !state.entities.contains_key(&constraint.source) {
                                return Err(RuntimeError::ValidationFailed {
                                    message: format!(
                                        "constraint {} references missing source entity {}",
                                        constraint.id, constraint.source
                                    ),
                                });
                            }
                            if !state.entities.contains_key(&constraint.target) {
                                return Err(RuntimeError::ValidationFailed {
                                    message: format!(
                                        "constraint {} references missing target entity {}",
                                        constraint.id, constraint.target
                                    ),
                                });
                            }
                        }
                    }
                }
                Ok(None)
            }

            RuntimeCommand::MarkSnapshot => Ok(None),
        }
    }
}
