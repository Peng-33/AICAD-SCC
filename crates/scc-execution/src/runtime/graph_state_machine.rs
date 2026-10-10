use crate::execution_plan::RuntimeCommand;
use crate::runtime::command_dispatcher::CommandDispatcher;
use crate::runtime::decision_engine::{Decision, DecisionEngine};
use crate::runtime::runtime_error::RuntimeError;
use scc_core::runtime::{ExecutionTrace, GraphState};

pub struct GraphStateMachine;

impl GraphStateMachine {
    pub fn apply_commands(
        state: &mut GraphState,
        commands: &[RuntimeCommand],
        trace: &mut ExecutionTrace,
    ) -> Result<(), RuntimeError> {
        for command in commands {
            let mut candidate = state.clone();
            let decision = DecisionEngine::evaluate(&candidate, command)?;

            match decision {
                Decision::Execute => {
                    let result = CommandDispatcher::dispatch(&mut candidate, command)?;
                    if let Some(new_snapshot) = result {
                        trace.record(new_snapshot, command.summary());
                    } else {
                        trace.record(candidate.current_snapshot(), command.summary());
                    }
                    candidate
                        .validate_detailed()
                        .map_err(RuntimeError::ValidationError)?;
                    *state = candidate;
                }
                Decision::Conflict => {
                    return Err(RuntimeError::ConflictDetected {
                        message: format!("conflict detected for command: {}", command.summary()),
                    });
                }
            }
        }
        state
            .validate_detailed()
            .map_err(RuntimeError::ValidationError)?;
        Ok(())
    }
}
