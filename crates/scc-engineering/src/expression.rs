//! Minimal, deterministic expression AST.
//!
//! An expression is a tree of constants, parameter references, and binary
//! arithmetic. It is stored as a structured tree and is **never** evaluated
//! with `eval()` or any dynamic code; values are resolved later by the
//! engineering model / lowerer (Phase 2+).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::EngineeringError;
use crate::ParameterId;

/// An arithmetic expression over parameters.
///
/// Example: `hole_diameter = plate_width / 8` is represented as
/// `Divide(Reference(plate_width), Constant(8))`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Expression {
    Constant {
        value: f64,
    },
    Reference {
        parameter: ParameterId,
    },
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Subtract {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Multiply {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Divide {
        left: Box<Expression>,
        right: Box<Expression>,
    },
}

impl std::fmt::Display for Expression {
    /// Canonical parenthesized form, used by the LLM context view and future
    /// canonical DSL formatting. Deterministic for identical ASTs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expression::Constant { value } => write!(f, "{}", value),
            Expression::Reference { parameter } => write!(f, "{}", parameter.0),
            Expression::Add { left, right } => write!(f, "({} + {})", left, right),
            Expression::Subtract { left, right } => write!(f, "({} - {})", left, right),
            Expression::Multiply { left, right } => write!(f, "({} * {})", left, right),
            Expression::Divide { left, right } => write!(f, "({} / {})", left, right),
        }
    }
}

impl Expression {
    /// Validate structural legality of the expression.
    ///
    /// Only obviously invalid data is rejected at this layer: non-finite
    /// constants, empty/invalid parameter references, and division by a
    /// constant zero. Semantic conflicts (e.g. a reference that resolves to
    /// zero at evaluation time) belong to the engineering-model / semantic
    /// validator in a later phase.
    pub fn validate(&self, what: &'static str) -> Result<(), EngineeringError> {
        match self {
            Expression::Constant { value } => {
                if !value.is_finite() {
                    return Err(EngineeringError::NonFiniteValue { what });
                }
            }
            Expression::Reference { parameter } => {
                crate::validate_identifier("parameter id", &parameter.0)?;
            }
            Expression::Add { left, right }
            | Expression::Subtract { left, right }
            | Expression::Multiply { left, right } => {
                left.validate(what)?;
                right.validate(what)?;
            }
            Expression::Divide { left, right } => {
                left.validate(what)?;
                right.validate(what)?;
                if let Expression::Constant { value } = &**right {
                    if *value == 0.0 {
                        return Err(EngineeringError::DivisionByConstantZero { what });
                    }
                }
            }
        }
        Ok(())
    }

    /// `parameter / constant` is the most common engineering expression shape
    /// (e.g. `plate_width / 8`); convenience constructor for callers and tests.
    pub fn divide_parameter_by_constant(parameter: ParameterId, divisor: f64) -> Expression {
        Expression::Divide {
            left: Box::new(Expression::Reference { parameter }),
            right: Box::new(Expression::Constant { value: divisor }),
        }
    }

    /// The set of parameters this expression references (transitively).
    ///
    /// Used by the parameter dependency graph and by expression validation.
    /// Parameter references are treated as opaque keys; the logic never splits
    /// the id string to derive meaning.
    pub fn referenced_parameters(&self) -> BTreeSet<ParameterId> {
        let mut out = BTreeSet::new();
        self.collect_references(&mut out);
        out
    }

    fn collect_references(&self, out: &mut BTreeSet<ParameterId>) {
        match self {
            Expression::Constant { .. } => {}
            Expression::Reference { parameter } => {
                out.insert(parameter.clone());
            }
            Expression::Add { left, right }
            | Expression::Subtract { left, right }
            | Expression::Multiply { left, right }
            | Expression::Divide { left, right } => {
                left.collect_references(out);
                right.collect_references(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression_roundtrip(expression: &Expression) -> Expression {
        let json = serde_json::to_string(expression).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn plate_width_over_eight_builds_stable_ast() {
        let expression =
            Expression::divide_parameter_by_constant(ParameterId("plate_width".into()), 8.0);
        assert!(expression.validate("hole_diameter").is_ok());

        let json = serde_json::to_string(&expression).unwrap();
        assert!(json.contains("\"op\":\"divide\""));
        assert!(json.contains("\"op\":\"reference\""));
        assert!(json.contains("\"parameter\":\"plate_width\""));
        assert!(json.contains("\"op\":\"constant\""));
        assert!(json.contains("\"value\":8.0"));
    }

    #[test]
    fn expression_serde_roundtrip() {
        let expression = Expression::Divide {
            left: Box::new(Expression::Subtract {
                left: Box::new(Expression::Reference {
                    parameter: ParameterId("plate_width".into()),
                }),
                right: Box::new(Expression::Constant { value: 2.0 }),
            }),
            right: Box::new(Expression::Constant { value: 8.0 }),
        };
        assert_eq!(expression_roundtrip(&expression), expression);
    }

    #[test]
    fn division_by_constant_zero_is_rejected() {
        let expression =
            Expression::divide_parameter_by_constant(ParameterId("plate_width".into()), 0.0);
        assert!(matches!(
            expression.validate("hole_diameter").unwrap_err(),
            EngineeringError::DivisionByConstantZero { .. }
        ));
    }

    #[test]
    fn division_by_reference_is_not_rejected_at_command_level() {
        // A divisor that *may* resolve to zero is a semantic concern for the
        // model / evaluator, not for command construction.
        let expression = Expression::Divide {
            left: Box::new(Expression::Constant { value: 10.0 }),
            right: Box::new(Expression::Reference {
                parameter: ParameterId("other".into()),
            }),
        };
        assert!(expression.validate("expr").is_ok());
    }

    #[test]
    fn non_finite_constant_is_rejected() {
        let expression = Expression::Constant { value: f64::NAN };
        assert!(matches!(
            expression.validate("expr").unwrap_err(),
            EngineeringError::NonFiniteValue { .. }
        ));
        let inf = Expression::Constant {
            value: f64::INFINITY,
        };
        assert!(inf.validate("expr").is_err());
    }

    #[test]
    fn invalid_parameter_reference_is_rejected() {
        let expression = Expression::Reference {
            parameter: ParameterId("".into()),
        };
        assert!(matches!(
            expression.validate("expr").unwrap_err(),
            EngineeringError::EmptyId { .. }
        ));
    }

    #[test]
    fn expression_serialization_is_deterministic() {
        let expression =
            Expression::divide_parameter_by_constant(ParameterId("plate_width".into()), 8.0);
        let a = serde_json::to_string(&expression).unwrap();
        let b = serde_json::to_string(&expression).unwrap();
        assert_eq!(a, b);
    }
}
