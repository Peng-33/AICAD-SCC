//! A provider-neutral tool call.
//!
//! A [`ToolCall`] is transport-level structured data: a tool name plus a flat
//! object of arguments. It never carries an [`EngineeringModel`], IR, or any
//! provider/model/token/conversation fields.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::definition::ToolName;
use crate::error::ToolError;
use crate::value::ToolValue;

/// One provider-neutral tool invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: ToolName,
    /// Arguments keyed by argument name; `BTreeMap` keeps iteration
    /// deterministic and structurally prevents duplicate argument names.
    pub arguments: BTreeMap<String, ToolValue>,
}

impl ToolCall {
    /// Construct a tool call from a pre-built argument map.
    pub fn new(name: impl Into<String>, arguments: BTreeMap<String, ToolValue>) -> Self {
        Self {
            name: ToolName(name.into()),
            arguments,
        }
    }

    /// Construct a tool call from a pair list, rejecting duplicate argument
    /// names (`SCC-TOOL-006`) instead of letting the last value win.
    pub fn from_pairs(
        name: impl Into<String>,
        pairs: Vec<(String, ToolValue)>,
    ) -> Result<Self, ToolError> {
        let name = ToolName(name.into());
        let mut arguments = BTreeMap::new();
        for (key, value) in pairs {
            if arguments.insert(key.clone(), value).is_some() {
                return Err(ToolError::DuplicateArgument {
                    tool: name.0.clone(),
                    argument: key,
                });
            }
        }
        Ok(Self { name, arguments })
    }
}
