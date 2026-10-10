//! Deterministic expression evaluation.
//!
//! Evaluates the Phase 1 [`Expression`] AST without `eval()`, shells, or any
//! dynamic code (ADR-016 Decision 3). Evaluation is a **derived** view: the
//! model keeps its source-of-truth values (expressions stay expressions), and
//! this module resolves them on demand in dependency order.
//!
//! Numeric safety: `NaN` / `±Infinity` are rejected, division by zero and
//! overflow-to-non-finite are detected, and [`ParameterValue::Unresolved`] is
//! propagated rather than guessed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::EngineeringError;
use crate::expression::Expression;
use crate::model::{EngineeringModel, Parameter};
use crate::parameter::ParameterValue;
use crate::ParameterId;

/// The result of evaluating a parameter or expression.
///
/// [`ResolvedValue::Unresolved`] is preserved end-to-end: it is never silently
/// converted to `0` or any other number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResolvedValue {
    Scalar(f64),
    Integer(i64),
    Boolean(bool),
    Choice(String),
    Unresolved,
}

impl ResolvedValue {
    /// Whether this value is deliberately unresolved.
    pub fn is_unresolved(&self) -> bool {
        matches!(self, ResolvedValue::Unresolved)
    }

    /// Numeric view used by arithmetic: `Ok(None)` propagates unresolved;
    /// `Err` rejects boolean/choice values used in arithmetic.
    pub fn as_f64(&self) -> Result<Option<f64>, EngineeringError> {
        match self {
            ResolvedValue::Scalar(value) => Ok(Some(*value)),
            ResolvedValue::Integer(value) => Ok(Some(*value as f64)),
            ResolvedValue::Unresolved => Ok(None),
            ResolvedValue::Boolean(_) | ResolvedValue::Choice(_) => {
                Err(EngineeringError::ExpressionTypeMismatch)
            }
        }
    }
}

/// Evaluate an expression against a resolver for parameter references.
pub fn evaluate_expression(
    expression: &Expression,
    resolve: &dyn Fn(&ParameterId) -> Result<ResolvedValue, EngineeringError>,
) -> Result<ResolvedValue, EngineeringError> {
    match expression {
        Expression::Constant { value } => {
            if !value.is_finite() {
                return Err(EngineeringError::NonFiniteValue {
                    what: "expression result",
                });
            }
            Ok(ResolvedValue::Scalar(*value))
        }
        Expression::Reference { parameter } => resolve(parameter),
        Expression::Add { left, right } => binary_op(left, right, resolve, |a, b| a + b),
        Expression::Subtract { left, right } => binary_op(left, right, resolve, |a, b| a - b),
        Expression::Multiply { left, right } => binary_op(left, right, resolve, |a, b| a * b),
        Expression::Divide { left, right } => {
            let left_value = evaluate_expression(left, resolve)?;
            let right_value = evaluate_expression(right, resolve)?;
            if left_value.is_unresolved() || right_value.is_unresolved() {
                return Ok(ResolvedValue::Unresolved);
            }
            let dividend = left_value.as_f64()?.expect("checked unresolved");
            let divisor = right_value.as_f64()?.expect("checked unresolved");
            if divisor == 0.0 {
                return Err(EngineeringError::ExpressionDivideByZero);
            }
            finish(dividend / divisor)
        }
    }
}

fn binary_op(
    left: &Expression,
    right: &Expression,
    resolve: &dyn Fn(&ParameterId) -> Result<ResolvedValue, EngineeringError>,
    op: fn(f64, f64) -> f64,
) -> Result<ResolvedValue, EngineeringError> {
    let left_value = evaluate_expression(left, resolve)?;
    let right_value = evaluate_expression(right, resolve)?;
    if left_value.is_unresolved() || right_value.is_unresolved() {
        return Ok(ResolvedValue::Unresolved);
    }
    let a = left_value.as_f64()?.expect("checked unresolved");
    let b = right_value.as_f64()?.expect("checked unresolved");
    finish(op(a, b))
}

fn finish(value: f64) -> Result<ResolvedValue, EngineeringError> {
    if !value.is_finite() {
        return Err(EngineeringError::NonFiniteValue {
            what: "expression result",
        });
    }
    Ok(ResolvedValue::Scalar(value))
}

/// Evaluate every parameter in deterministic dependency order.
pub fn evaluate_all(
    model: &EngineeringModel,
) -> Result<BTreeMap<ParameterId, ResolvedValue>, EngineeringError> {
    let graph = model.dependency_graph();
    let order = graph.topological_order()?;
    let mut resolved: BTreeMap<ParameterId, ResolvedValue> = BTreeMap::new();
    for id in order {
        let parameter =
            model
                .parameters
                .get(&id)
                .ok_or_else(|| EngineeringError::ParameterNotFound {
                    parameter_id: id.clone(),
                })?;
        let value = resolve_parameter(parameter, &resolved)?;
        resolved.insert(id, value);
    }
    Ok(resolved)
}

/// Evaluate a single parameter, following only its transitive dependencies.
pub fn evaluate_parameter(
    model: &EngineeringModel,
    id: &ParameterId,
) -> Result<ResolvedValue, EngineeringError> {
    if !model.parameters.contains_key(id) {
        return Err(EngineeringError::ParameterNotFound {
            parameter_id: id.clone(),
        });
    }
    let graph = model.dependency_graph();
    let mut needed = graph.transitive_dependencies(id);
    needed.insert(id.clone());
    let order = graph.topological_order_restricted(&needed)?;
    let mut resolved: BTreeMap<ParameterId, ResolvedValue> = BTreeMap::new();
    for pid in order {
        let parameter = model
            .parameters
            .get(&pid)
            .expect("needed parameter present");
        let value = resolve_parameter(parameter, &resolved)?;
        resolved.insert(pid, value);
    }
    Ok(resolved.get(id).cloned().expect("target parameter present"))
}

fn resolve_parameter(
    parameter: &Parameter,
    resolved: &BTreeMap<ParameterId, ResolvedValue>,
) -> Result<ResolvedValue, EngineeringError> {
    match &parameter.value {
        ParameterValue::Expression(expression) => evaluate_expression(expression, &|reference| {
            resolved.get(reference).cloned().ok_or_else(|| {
                EngineeringError::ExpressionReferenceMissing {
                    parameter_id: reference.clone(),
                }
            })
        }),
        ParameterValue::Scalar(value) => {
            if !value.is_finite() {
                return Err(EngineeringError::NonFiniteValue {
                    what: "parameter value",
                });
            }
            Ok(ResolvedValue::Scalar(*value))
        }
        ParameterValue::Integer(value) => Ok(ResolvedValue::Integer(*value)),
        ParameterValue::Boolean(value) => Ok(ResolvedValue::Boolean(*value)),
        ParameterValue::Choice(value) => Ok(ResolvedValue::Choice(value.clone())),
        ParameterValue::Unresolved => Ok(ResolvedValue::Unresolved),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::authority::AuthorityLevel;

    use crate::model::{Parameter, ParameterOrigin};

    fn reference(id: &str) -> Expression {
        Expression::Reference {
            parameter: ParameterId(id.into()),
        }
    }

    fn constant(value: f64) -> Expression {
        Expression::Constant { value }
    }

    fn eval_constant(value: f64) -> Result<ResolvedValue, EngineeringError> {
        evaluate_expression(&constant(value), &|_| unreachable!("no references"))
    }

    #[test]
    fn constant_and_reference_evaluate() {
        assert_eq!(eval_constant(42.0).unwrap(), ResolvedValue::Scalar(42.0));
        let expression = Expression::Reference {
            parameter: ParameterId("a".into()),
        };
        assert_eq!(
            evaluate_expression(&expression, &|_| Ok(ResolvedValue::Scalar(7.0))).unwrap(),
            ResolvedValue::Scalar(7.0)
        );
    }

    #[test]
    fn arithmetic_chain_evaluates_deterministically() {
        // (10 + 5) * 2 - 3 = 27
        let expression = Expression::Subtract {
            left: Box::new(Expression::Multiply {
                left: Box::new(Expression::Add {
                    left: Box::new(constant(10.0)),
                    right: Box::new(constant(5.0)),
                }),
                right: Box::new(constant(2.0)),
            }),
            right: Box::new(constant(3.0)),
        };
        assert_eq!(
            evaluate_expression(&expression, &|_| unreachable!("no references")).unwrap(),
            ResolvedValue::Scalar(27.0)
        );
    }

    #[test]
    fn division_by_zero_at_evaluation_is_rejected() {
        // A literal constant zero is rejected at construction by Expression
        // validation...
        let literal = Expression::Divide {
            left: Box::new(constant(10.0)),
            right: Box::new(constant(0.0)),
        };
        assert!(matches!(
            literal.validate("expression").unwrap_err(),
            EngineeringError::DivisionByConstantZero { .. }
        ));
        // ...whereas a *resolved* zero (via a parameter) is rejected here, at
        // evaluation time.
        let via_resolution = Expression::Divide {
            left: Box::new(constant(10.0)),
            right: Box::new(reference("zero")),
        };
        assert!(matches!(
            evaluate_expression(&via_resolution, &|_| Ok(ResolvedValue::Scalar(0.0))).unwrap_err(),
            EngineeringError::ExpressionDivideByZero
        ));
    }

    #[test]
    fn overflow_to_non_finite_is_rejected() {
        let expression = Expression::Multiply {
            left: Box::new(constant(1.0e200)),
            right: Box::new(constant(1.0e200)),
        };
        assert!(matches!(
            evaluate_expression(&expression, &|_| unreachable!("no references")).unwrap_err(),
            EngineeringError::NonFiniteValue { .. }
        ));
    }

    #[test]
    fn unresolved_reference_propagates_not_guessed() {
        let expression = Expression::Divide {
            left: Box::new(reference("a")),
            right: Box::new(constant(8.0)),
        };
        let resolved =
            evaluate_expression(&expression, &|_| Ok(ResolvedValue::Unresolved)).unwrap();
        assert_eq!(resolved, ResolvedValue::Unresolved);
    }

    #[test]
    fn boolean_or_choice_in_arithmetic_is_rejected() {
        let expression = Expression::Add {
            left: Box::new(reference("flag")),
            right: Box::new(constant(1.0)),
        };
        assert!(matches!(
            evaluate_expression(&expression, &|_| Ok(ResolvedValue::Boolean(true))).unwrap_err(),
            EngineeringError::ExpressionTypeMismatch
        ));
        let expression = Expression::Add {
            left: Box::new(reference("grade")),
            right: Box::new(constant(1.0)),
        };
        assert!(evaluate_expression(&expression, &|_| {
            Ok(ResolvedValue::Choice("fine".into()))
        })
        .is_err());
    }

    #[test]
    fn integer_reference_is_used_as_number_in_arithmetic() {
        let expression = Expression::Multiply {
            left: Box::new(reference("count")),
            right: Box::new(constant(2.0)),
        };
        assert_eq!(
            evaluate_expression(&expression, &|_| Ok(ResolvedValue::Integer(4))).unwrap(),
            ResolvedValue::Scalar(8.0)
        );
    }

    fn make_model(parameters: Vec<(&str, ParameterValue)>) -> EngineeringModel {
        let mut model = EngineeringModel::new();
        for (id, value) in parameters {
            model
                .create_parameter(Parameter::new(
                    ParameterId(id.into()),
                    value,
                    ParameterOrigin::User,
                    AuthorityLevel::L0,
                ))
                .unwrap();
        }
        model
    }

    #[test]
    fn evaluate_parameter_follows_dependencies() {
        // hole_diameter = plate_width / 8, plate_width = 80 → 10
        let mut model = make_model(vec![
            ("plate_width", ParameterValue::Scalar(80.0)),
            ("hole_diameter", ParameterValue::Unresolved),
        ]);
        let expression =
            Expression::divide_parameter_by_constant(ParameterId("plate_width".into()), 8.0);
        model
            .parameters
            .get_mut(&ParameterId("hole_diameter".into()))
            .unwrap()
            .value = ParameterValue::Expression(expression);
        assert_eq!(
            model
                .evaluate_parameter(&ParameterId("hole_diameter".into()))
                .unwrap(),
            ResolvedValue::Scalar(10.0)
        );
    }

    #[test]
    fn evaluate_all_returns_dependency_ordered_resolved_values() {
        // A=100, B=A/2, C=B+10, D=C*2
        let mut model = make_model(vec![
            ("a", ParameterValue::Scalar(100.0)),
            ("b", ParameterValue::Unresolved),
            ("c", ParameterValue::Unresolved),
            ("d", ParameterValue::Unresolved),
        ]);
        let mut set = |id: &str, expression: Expression| {
            model
                .parameters
                .get_mut(&ParameterId(id.into()))
                .unwrap()
                .value = ParameterValue::Expression(expression);
        };
        set(
            "b",
            Expression::divide_parameter_by_constant(ParameterId("a".into()), 2.0),
        );
        set(
            "c",
            Expression::Add {
                left: Box::new(reference("b")),
                right: Box::new(constant(10.0)),
            },
        );
        set(
            "d",
            Expression::Multiply {
                left: Box::new(reference("c")),
                right: Box::new(constant(2.0)),
            },
        );
        let resolved = model.evaluate_all().unwrap();
        assert_eq!(
            resolved.get(&ParameterId("a".into())).unwrap(),
            &ResolvedValue::Scalar(100.0)
        );
        assert_eq!(
            resolved.get(&ParameterId("b".into())).unwrap(),
            &ResolvedValue::Scalar(50.0)
        );
        assert_eq!(
            resolved.get(&ParameterId("c".into())).unwrap(),
            &ResolvedValue::Scalar(60.0)
        );
        assert_eq!(
            resolved.get(&ParameterId("d".into())).unwrap(),
            &ResolvedValue::Scalar(120.0)
        );
    }

    #[test]
    fn unresolved_parameter_stays_unresolved_in_evaluation() {
        let mut model = make_model(vec![
            ("b", ParameterValue::Unresolved),
            ("a", ParameterValue::Unresolved),
        ]);
        model
            .parameters
            .get_mut(&ParameterId("a".into()))
            .unwrap()
            .value = ParameterValue::Expression(Expression::divide_parameter_by_constant(
            ParameterId("b".into()),
            8.0,
        ));
        assert_eq!(
            model.evaluate_parameter(&ParameterId("a".into())).unwrap(),
            ResolvedValue::Unresolved
        );
    }

    #[test]
    fn evaluate_missing_parameter_errors() {
        let model = make_model(vec![]);
        assert!(matches!(
            model
                .evaluate_parameter(&ParameterId("missing".into()))
                .unwrap_err(),
            EngineeringError::ParameterNotFound { .. }
        ));
    }

    #[test]
    fn evaluate_in_a_cycle_reports_cycle_instead_of_looping() {
        let mut model = make_model(vec![
            ("a", ParameterValue::Unresolved),
            ("b", ParameterValue::Unresolved),
        ]);
        model
            .parameters
            .get_mut(&ParameterId("a".into()))
            .unwrap()
            .value = ParameterValue::Expression(Expression::Add {
            left: Box::new(reference("b")),
            right: Box::new(constant(1.0)),
        });
        model
            .parameters
            .get_mut(&ParameterId("b".into()))
            .unwrap()
            .value = ParameterValue::Expression(Expression::Add {
            left: Box::new(reference("a")),
            right: Box::new(constant(1.0)),
        });
        assert!(matches!(
            model.evaluate_all().unwrap_err(),
            EngineeringError::ExpressionCycle { .. }
        ));
    }
}
