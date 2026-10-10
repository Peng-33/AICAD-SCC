//! Provider-neutral tool argument values.
//!
//! Structured values support arrays and objects for bounded semantic payloads
//! such as topology references. Tool adapters still validate each domain
//! operation against a strict schema. Patterns remain high-level (`pcd`/`count`,
//! never expanded instance point arrays). See ADR-019 Decision 6.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A structured tool argument value.
///
/// Serde is untagged so a provider/LLM can emit plain JSON (`"p"`, `8`, `8.5`,
/// `true`, `null`). `Integer` is declared before `Number` so a JSON integer
/// (`8`) decodes as `Integer` and a decimal (`8.5`) as `Number` — preserving
/// the strict integer/number distinction required for `count`-style arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolValue {
    Integer(i64),
    Number(f64),
    String(String),
    Boolean(bool),
    Array(Vec<ToolValue>),
    Object(BTreeMap<String, ToolValue>),
    Null,
}

impl ToolValue {
    /// A human-readable type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            ToolValue::String(_) => "string",
            ToolValue::Number(_) => "number",
            ToolValue::Integer(_) => "integer",
            ToolValue::Boolean(_) => "boolean",
            ToolValue::Array(_) => "array",
            ToolValue::Object(_) => "object",
            ToolValue::Null => "null",
        }
    }
}

/// Construct a numeric tool value.
pub fn num(value: f64) -> ToolValue {
    ToolValue::Number(value)
}

/// Construct an integer tool value.
pub fn int(value: i64) -> ToolValue {
    ToolValue::Integer(value)
}

/// Construct a string tool value.
pub fn str_value(value: &str) -> ToolValue {
    ToolValue::String(value.into())
}

/// Construct a boolean tool value.
pub fn bool_value(value: bool) -> ToolValue {
    ToolValue::Boolean(value)
}

/// Construct an array tool value.
pub fn array(values: Vec<ToolValue>) -> ToolValue {
    ToolValue::Array(values)
}
