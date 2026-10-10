//! Tool execution: adapt + apply atomically.
//!
//! The executor never mutates a model directly; it always routes through
//! [`CommandExecutor::apply_all`] (working-copy + commit). Both a single
//! composite tool and a batch of tool calls are all-or-nothing.

use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::model::EngineeringModel;

use crate::adapter::adapt_tool_call;
use crate::call::ToolCall;
use crate::error::ToolError;

/// The result of applying a tool call (or batch): the commands that were
/// committed. It never carries a copy of the [`EngineeringModel`].
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub applied_commands: Vec<EngineeringCommand>,
}

/// Adapt and atomically apply one tool call to the model.
pub fn apply_tool_call(
    model: &mut EngineeringModel,
    call: &ToolCall,
) -> Result<ToolResult, ToolError> {
    let commands = adapt_tool_call(call)?;
    CommandExecutor::apply_all(model, &commands).map_err(|source| {
        ToolError::CommandApplicationFailed {
            tool: call.name.0.clone(),
            source,
        }
    })?;
    Ok(ToolResult {
        applied_commands: commands,
    })
}

/// Adapt and atomically apply a batch of tool calls.
///
/// All calls are adapted first (any adaptation failure rejects the batch before
/// any change), then all commands are applied in one working-copy + commit.
pub fn apply_tool_calls_atomically(
    model: &mut EngineeringModel,
    calls: &[ToolCall],
) -> Result<Vec<ToolResult>, ToolError> {
    let mut per_call: Vec<Vec<EngineeringCommand>> = Vec::with_capacity(calls.len());
    let mut all_commands = Vec::new();
    for call in calls {
        let commands = adapt_tool_call(call)?;
        all_commands.extend(commands.clone());
        per_call.push(commands);
    }
    CommandExecutor::apply_all(model, &all_commands).map_err(|source| {
        ToolError::CommandApplicationFailed {
            tool: "batch".into(),
            source,
        }
    })?;
    Ok(per_call
        .into_iter()
        .map(|applied_commands| ToolResult { applied_commands })
        .collect())
}
