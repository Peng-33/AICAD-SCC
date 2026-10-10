//! Structured errors for the provider-neutral tool layer.
//!
//! Every [`ToolError`] converts to an [`scc_core::error::StructuredError`]
//! carrying `SCC-TOOL-{NNN}` codes under
//! [`scc_core::error::ErrorStage::Tool`]. Engineering-layer errors are **not**
//! flattened into a generic tool code: `CommandApplicationFailed` keeps its
//! source [`EngineeringError`] as the structured error's `source` chain.

use scc_core::error::{ErrorStage, StructuredError};
use scc_engineering::error::EngineeringError;

/// A provider-neutral tool-layer failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool `{name}`")]
    UnknownTool { name: String },

    #[error("tool `{tool}` is missing required argument `{argument}`")]
    MissingArgument { tool: String, argument: String },

    #[error("tool `{tool}` received unexpected argument `{argument}`")]
    UnexpectedArgument { tool: String, argument: String },

    #[error(
        "tool `{tool}` argument `{argument}` has wrong type: expected {expected}, got {received}"
    )]
    InvalidArgumentType {
        tool: String,
        argument: String,
        expected: String,
        received: String,
    },

    #[error("tool `{tool}` argument `{argument}` is invalid: {message}")]
    InvalidArgumentValue {
        tool: String,
        argument: String,
        message: String,
    },

    #[error("tool `{tool}` received duplicate argument `{argument}`")]
    DuplicateArgument { tool: String, argument: String },

    #[error("tool `{tool}` is not supported by AICAD Tool v1")]
    UnsupportedTool { tool: String },

    #[error("adapter failed for tool `{tool}`: {message}")]
    AdapterFailed { tool: String, message: String },

    #[error("tool `{tool}` expression parse failed: {message}")]
    ExpressionParseFailed { tool: String, message: String },

    #[error("tool `{tool}` command application failed: {source}")]
    CommandApplicationFailed {
        tool: String,
        source: EngineeringError,
    },
}

impl ToolError {
    /// The tool this error refers to, when known.
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            ToolError::UnknownTool { name } => Some(name),
            ToolError::MissingArgument { tool, .. }
            | ToolError::UnexpectedArgument { tool, .. }
            | ToolError::InvalidArgumentType { tool, .. }
            | ToolError::InvalidArgumentValue { tool, .. }
            | ToolError::DuplicateArgument { tool, .. }
            | ToolError::UnsupportedTool { tool }
            | ToolError::AdapterFailed { tool, .. }
            | ToolError::ExpressionParseFailed { tool, .. }
            | ToolError::CommandApplicationFailed { tool, .. } => Some(tool),
        }
    }
}

impl From<ToolError> for StructuredError {
    fn from(err: ToolError) -> Self {
        let code = match &err {
            ToolError::UnknownTool { .. } => "SCC-TOOL-001",
            ToolError::MissingArgument { .. } => "SCC-TOOL-002",
            ToolError::UnexpectedArgument { .. } => "SCC-TOOL-003",
            ToolError::InvalidArgumentType { .. } => "SCC-TOOL-004",
            ToolError::InvalidArgumentValue { .. } => "SCC-TOOL-005",
            ToolError::DuplicateArgument { .. } => "SCC-TOOL-006",
            ToolError::UnsupportedTool { .. } => "SCC-TOOL-007",
            ToolError::AdapterFailed { .. } => "SCC-TOOL-008",
            ToolError::ExpressionParseFailed { .. } => "SCC-TOOL-009",
            ToolError::CommandApplicationFailed { .. } => "SCC-TOOL-010",
        };
        let mut structured = StructuredError::new(code, ErrorStage::Tool, err.to_string())
            .with_recoverable(true)
            .with_suggestion("fix the tool call before retrying");
        if let ToolError::CommandApplicationFailed { source, .. } = &err {
            structured = structured.with_source(StructuredError::from(source.clone()));
        }
        structured
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_error_code_passes_contract() {
        let cases = [
            ToolError::UnknownTool { name: "x".into() },
            ToolError::MissingArgument {
                tool: "t".into(),
                argument: "a".into(),
            },
            ToolError::UnexpectedArgument {
                tool: "t".into(),
                argument: "a".into(),
            },
            ToolError::InvalidArgumentType {
                tool: "t".into(),
                argument: "a".into(),
                expected: "number".into(),
                received: "string".into(),
            },
            ToolError::InvalidArgumentValue {
                tool: "t".into(),
                argument: "a".into(),
                message: "bad".into(),
            },
            ToolError::DuplicateArgument {
                tool: "t".into(),
                argument: "a".into(),
            },
            ToolError::UnsupportedTool { tool: "t".into() },
            ToolError::AdapterFailed {
                tool: "t".into(),
                message: "bad".into(),
            },
            ToolError::ExpressionParseFailed {
                tool: "t".into(),
                message: "bad".into(),
            },
            ToolError::CommandApplicationFailed {
                tool: "t".into(),
                source: EngineeringError::FeatureNotFound {
                    feature_id: scc_engineering::FeatureId("f".into()),
                },
            },
        ];
        for err in cases {
            let structured: StructuredError = err.into();
            assert!(
                structured.validate_contract().is_ok(),
                "{} must pass contract",
                structured.code
            );
        }
    }

    #[test]
    fn command_application_failure_keeps_engineering_source() {
        let err = ToolError::CommandApplicationFailed {
            tool: "remove_feature".into(),
            source: EngineeringError::FeatureNotFound {
                feature_id: scc_engineering::FeatureId("f1".into()),
            },
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-TOOL-010");
        let source = structured.source.as_ref().expect("source chain preserved");
        assert_eq!(source.code, "SCC-ENG-012");
    }
}
