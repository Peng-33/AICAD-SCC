//! Provider-neutral tool definitions and parameter schemas.
//!
//! These types are deliberately free of any vendor-specific fields (no
//! provider, no model, no token limits): a Phase 6 provider adapter can map
//! this internal schema to OpenAI/local/other tool schemas.

use serde::{Deserialize, Serialize};

/// A stable tool name (`[a-z][a-z0-9_-]{0,63}` in the standard set).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ToolName(pub String);

impl std::fmt::Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The schema-level type of a tool argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolParameterType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    /// No schema-level type constraint; the tool adapter validates the value
    /// against its context (used only for `set_drawing_property`'s `value`).
    Any,
}

/// Optional value constraints applied by the validator as an early rejection
/// gate. The engineering layer remains the source of truth for full rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolParameterConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// One parameter of a tool definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub required: bool,
    pub param_type: ToolParameterType,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<ToolParameterConstraints>,
}

impl ToolParameter {
    pub fn string(name: &str, required: bool, description: &str) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::String,
            description: description.into(),
            constraints: None,
        }
    }

    pub fn string_enum(name: &str, required: bool, description: &str, values: &[&str]) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::String,
            description: description.into(),
            constraints: Some(ToolParameterConstraints {
                minimum: None,
                maximum: None,
                exclusive_minimum: None,
                enum_values: Some(values.iter().map(|s| s.to_string()).collect()),
            }),
        }
    }

    pub fn number(
        name: &str,
        required: bool,
        description: &str,
        exclusive_minimum: Option<f64>,
    ) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::Number,
            description: description.into(),
            constraints: Some(ToolParameterConstraints {
                minimum: None,
                maximum: None,
                exclusive_minimum,
                enum_values: None,
            }),
        }
    }

    pub fn integer(name: &str, required: bool, description: &str, minimum: Option<f64>) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::Integer,
            description: description.into(),
            constraints: Some(ToolParameterConstraints {
                minimum,
                maximum: None,
                exclusive_minimum: None,
                enum_values: None,
            }),
        }
    }

    pub fn boolean(name: &str, required: bool, description: &str) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::Boolean,
            description: description.into(),
            constraints: None,
        }
    }

    pub fn array(name: &str, required: bool, description: &str) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::Array,
            description: description.into(),
            constraints: None,
        }
    }

    pub fn any(name: &str, required: bool, description: &str) -> Self {
        Self {
            name: name.into(),
            required,
            param_type: ToolParameterType::Any,
            description: description.into(),
            constraints: None,
        }
    }
}

/// A provider-neutral tool definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
}

impl ToolDefinition {
    /// The parameter with the given name, if declared.
    pub fn parameter(&self, name: &str) -> Option<&ToolParameter> {
        self.parameters.iter().find(|p| p.name == name)
    }
}
