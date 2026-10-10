//! Provider-neutral Tool Calling Layer (Phase 5).
//!
//! A second controlled front end for [`scc_engineering::command::EngineeringCommand`],
//! parallel to the DSL. A tool call is transport-level structured data that is
//! validated, adapted to `EngineeringCommand` values, and applied atomically by
//! `CommandExecutor::apply_all` — it never mutates an
//! [`scc_engineering::model::EngineeringModel`] directly and never depends on
//! any LLM provider SDK.
//!
//! ```text
//! ToolCall → Argument Validation → ToolAdapter → EngineeringCommand[] → apply_all → EngineeringModel
//! ```

pub mod adapter;
pub mod call;
pub mod definition;
pub mod error;
pub mod executor;
pub mod registry;
pub mod validator;
pub mod value;

pub use adapter::adapt_tool_call;
pub use call::ToolCall;
pub use definition::{
    ToolDefinition, ToolName, ToolParameter, ToolParameterConstraints, ToolParameterType,
};
pub use error::ToolError;
pub use executor::{apply_tool_call, apply_tool_calls_atomically, ToolResult};
pub use registry::ToolRegistry;
pub use value::{array, bool_value, int, num, str_value, ToolValue};
