//! Tool-call argument validation.
//!
//! The validator is an *early rejection gate*: it rejects malformed calls
//! (unknown/missing arguments, wrong types, obvious value violations) before
//! any command is built. Full engineering rules stay in the engineering layer
//! — the validator does not duplicate "hole inside plate" or overconstraint
//! checks (ADR-019 Decision 5).

use std::collections::BTreeMap;

use crate::definition::{ToolDefinition, ToolParameterType};
use crate::error::ToolError;
use crate::value::ToolValue;

/// Validate a tool call's arguments against its definition.
pub fn validate_arguments(
    definition: &ToolDefinition,
    arguments: &BTreeMap<String, ToolValue>,
) -> Result<(), ToolError> {
    let tool = definition.name.0.clone();

    // Unknown arguments are rejected, never ignored (important for LLM
    // constraint, e.g. `color = "red"` must fail).
    for name in arguments.keys() {
        if !definition.parameters.iter().any(|p| &p.name == name) {
            return Err(ToolError::UnexpectedArgument {
                tool: tool.clone(),
                argument: name.clone(),
            });
        }
    }

    for parameter in &definition.parameters {
        let Some(value) = arguments.get(&parameter.name) else {
            if parameter.required {
                return Err(ToolError::MissingArgument {
                    tool: tool.clone(),
                    argument: parameter.name.clone(),
                });
            }
            continue;
        };
        validate_value(&tool, parameter, value)?;
    }
    Ok(())
}

fn validate_value(
    tool: &str,
    parameter: &crate::definition::ToolParameter,
    value: &ToolValue,
) -> Result<(), ToolError> {
    let name = &parameter.name;
    let type_error = |expected: &str| ToolError::InvalidArgumentType {
        tool: tool.into(),
        argument: name.clone(),
        expected: expected.into(),
        received: value.type_name().into(),
    };

    match parameter.param_type {
        ToolParameterType::String => {
            let ToolValue::String(s) = value else {
                return Err(type_error("string"));
            };
            if let Some(values) = &parameter
                .constraints
                .as_ref()
                .and_then(|c| c.enum_values.as_ref())
            {
                if !values.iter().any(|v| v == s) {
                    return Err(ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: name.clone(),
                        message: format!("value `{}` is not one of {:?}", s, values),
                    });
                }
            }
        }
        ToolParameterType::Number => {
            // A JSON integer is a valid number; decimals stay `Number`.
            let n = match value {
                ToolValue::Number(n) => *n,
                ToolValue::Integer(i) => *i as f64,
                _ => return Err(type_error("number")),
            };
            if !n.is_finite() {
                return Err(ToolError::InvalidArgumentValue {
                    tool: tool.into(),
                    argument: name.clone(),
                    message: format!("value {} must be finite", n),
                });
            }
            check_numeric_constraints(tool, name, n, &parameter.constraints)?;
        }
        ToolParameterType::Integer => {
            let ToolValue::Integer(i) = value else {
                return Err(type_error("integer"));
            };
            let n = *i as f64;
            if let Some(constraints) = &parameter.constraints {
                if let Some(min) = constraints.minimum {
                    if n < min {
                        return Err(below_min(tool, name, *i, min));
                    }
                }
            }
        }
        ToolParameterType::Boolean => {
            if !matches!(value, ToolValue::Boolean(_)) {
                return Err(type_error("boolean"));
            }
        }
        ToolParameterType::Array => {
            if !matches!(value, ToolValue::Array(_)) {
                return Err(type_error("array"));
            }
        }
        ToolParameterType::Any => {
            // Type is validated contextually by the adapter.
        }
    }
    Ok(())
}

fn check_numeric_constraints(
    tool: &str,
    name: &str,
    value: f64,
    constraints: &Option<crate::definition::ToolParameterConstraints>,
) -> Result<(), ToolError> {
    let Some(constraints) = constraints else {
        return Ok(());
    };
    if let Some(min) = constraints.minimum {
        if value < min {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: name.into(),
                message: format!("value {} must be at least {}", value, min),
            });
        }
    }
    if let Some(max) = constraints.maximum {
        if value > max {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: name.into(),
                message: format!("value {} must be at most {}", value, max),
            });
        }
    }
    if let Some(ex) = constraints.exclusive_minimum {
        if value <= ex {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: name.into(),
                message: format!("value {} must be greater than {}", value, ex),
            });
        }
    }
    Ok(())
}

fn below_min(tool: &str, name: &str, value: i64, min: f64) -> ToolError {
    ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: name.into(),
        message: format!("value {} must be at least {}", value, min),
    }
}
