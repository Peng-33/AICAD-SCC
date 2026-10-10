//! Engineering parameter values.
//!
//! A parameter value may be a concrete scalar, a whole number, a boolean, a
//! selection from an enumerated list, a symbolic expression, or deliberately
//! unresolved. Keeping [`ParameterValue::Unresolved`] is a core AICAD
//! principle: missing critical design information must not be silently
//! guessed.

use serde::{Deserialize, Serialize};

use crate::error::EngineeringError;
use crate::expression::Expression;

/// The value of an engineering parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ParameterValue {
    /// A finite floating-point dimension (millimetres).
    Scalar(f64),
    /// A whole-number count or index.
    Integer(i64),
    /// A boolean flag.
    Boolean(bool),
    /// A selection from an enumerated list.
    Choice(String),
    /// A symbolic expression that must be resolved before use.
    Expression(Expression),
    /// Deliberately unresolved; downstream layers must not guess a value.
    Unresolved,
}

impl ParameterValue {
    /// Validate command-level legality of this value.
    ///
    /// `Scalar` must be finite; `Choice` must be non-empty; `Expression` must
    /// itself validate. `Integer`, `Boolean`, and `Unresolved` carry no
    /// command-level constraints.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        match self {
            ParameterValue::Scalar(value) => {
                if !value.is_finite() {
                    return Err(EngineeringError::NonFiniteValue {
                        what: "parameter value",
                    });
                }
            }
            ParameterValue::Choice(choice) => {
                if choice.trim().is_empty() {
                    return Err(EngineeringError::EmptyChoice {
                        what: "parameter value",
                    });
                }
            }
            ParameterValue::Expression(expression) => {
                expression.validate("parameter expression")?;
            }
            ParameterValue::Integer(_)
            | ParameterValue::Boolean(_)
            | ParameterValue::Unresolved => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParameterId;

    #[test]
    fn scalar_integer_and_unresolved_roundtrip() {
        let values = [
            ParameterValue::Scalar(5.0),
            ParameterValue::Integer(8),
            ParameterValue::Boolean(true),
            ParameterValue::Choice("fine".into()),
            ParameterValue::Unresolved,
        ];
        for value in values {
            let json = serde_json::to_string(&value).unwrap();
            let back: ParameterValue = serde_json::from_str(&json).unwrap();
            assert_eq!(value, back);
        }
    }

    #[test]
    fn scalar_serializes_as_tagged_human_readable_value() {
        let json = serde_json::to_string(&ParameterValue::Scalar(5.0)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "scalar");
        assert_eq!(value["value"], 5.0);
    }

    #[test]
    fn unresolved_serializes_without_guessing() {
        let json = serde_json::to_string(&ParameterValue::Unresolved).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "unresolved");
        assert!(value.get("value").is_none());
    }

    #[test]
    fn expression_value_roundtrips_as_nested_ast() {
        let value = ParameterValue::Expression(
            crate::expression::Expression::divide_parameter_by_constant(
                ParameterId("plate_width".into()),
                8.0,
            ),
        );
        let json = serde_json::to_string(&value).unwrap();
        let back: ParameterValue = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
        assert!(json.contains("\"type\":\"expression\""));
    }

    #[test]
    fn non_finite_scalar_is_rejected() {
        assert!(ParameterValue::Scalar(f64::NAN).validate().is_err());
        assert!(ParameterValue::Scalar(f64::INFINITY).validate().is_err());
        assert!(ParameterValue::Scalar(-f64::INFINITY).validate().is_err());
        assert!(ParameterValue::Scalar(-3.5).validate().is_ok());
    }

    #[test]
    fn empty_choice_is_rejected() {
        assert!(ParameterValue::Choice("".into()).validate().is_err());
        assert!(ParameterValue::Choice("  ".into()).validate().is_err());
        assert!(ParameterValue::Choice("coarse".into()).validate().is_ok());
    }

    #[test]
    fn unresolved_and_integer_validate_ok() {
        assert!(ParameterValue::Unresolved.validate().is_ok());
        assert!(ParameterValue::Integer(0).validate().is_ok());
        assert!(ParameterValue::Integer(-1).validate().is_ok());
    }
}
