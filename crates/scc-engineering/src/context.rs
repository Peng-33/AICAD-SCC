//! A derived, read-only LLM context view over an [`EngineeringModel`].
//!
//! `EngineeringContextView` is a compact, deterministic summary of the model
//! intended as LLM context for Edit intent (ADR-020 Decision 11). It is
//! **derived** on demand from the model — never stored, never a second source
//! of truth, and cannot modify the CAD. It deliberately excludes
//! `ConstraintIR`, `GraphState`, SCC internal ids, `ExecutionTrace`, and
//! Proposal internals.

use serde::{Deserialize, Serialize};

use crate::evaluator::ResolvedValue;
use crate::feature::EngineeringFeature;
use crate::model::{EngineeringModel, ParameterOrigin};
use crate::parameter::ParameterValue;

/// A feature summary line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeature {
    pub id: String,
    pub feature_type: String,
}

/// A parameter summary line, showing its current value (or expression, or
/// `UNRESOLVED`) and its origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextParameter {
    pub id: String,
    pub value: String,
    pub origin: String,
}

/// A constraint summary line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextConstraint {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub target: String,
}

/// The part identity summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPart {
    pub id: String,
    pub name: String,
}

/// The derived, read-only context view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineeringContextView {
    pub part: Option<ContextPart>,
    pub features: Vec<ContextFeature>,
    pub parameters: Vec<ContextParameter>,
    pub constraints: Vec<ContextConstraint>,
    pub unresolved: Vec<String>,
}

impl EngineeringModel {
    /// Derive the read-only context view (deterministic ordering).
    pub fn context_view(&self) -> EngineeringContextView {
        let resolved = self.evaluate_all().ok();

        let part = self.part.as_ref().map(|p| ContextPart {
            id: p.id.0.clone(),
            name: p.name.clone(),
        });

        let mut features = Vec::new();
        for id in self.ordered_feature_ids() {
            let Some(feature) = self.features.get(&id) else {
                continue;
            };
            features.push(ContextFeature {
                id: feature.id().0.clone(),
                feature_type: feature_type_name(feature).to_string(),
            });
        }

        let mut parameters = Vec::new();
        let mut unresolved = Vec::new();
        for (id, parameter) in &self.parameters {
            let value = match (&parameter.value, resolved.as_ref().and_then(|m| m.get(id))) {
                (ParameterValue::Unresolved, _) => "UNRESOLVED".to_string(),
                (_, Some(ResolvedValue::Scalar(v))) => format!("{}", v),
                (_, Some(ResolvedValue::Integer(i))) => format!("{}", i),
                (_, Some(ResolvedValue::Boolean(b))) => format!("{}", b),
                (_, Some(ResolvedValue::Choice(c))) => c.clone(),
                (_, Some(ResolvedValue::Unresolved)) => "UNRESOLVED".to_string(),
                (ParameterValue::Expression(e), _) => format!("{}", e),
                _ => "?".to_string(),
            };
            if value == "UNRESOLVED" {
                unresolved.push(id.0.clone());
            }
            parameters.push(ContextParameter {
                id: id.0.clone(),
                value,
                origin: origin_name(parameter.origin).to_string(),
            });
        }

        let mut constraints = Vec::new();
        for constraint in self.constraints.values() {
            let (source, target) = constraint_reference_summary(constraint);
            constraints.push(ContextConstraint {
                id: constraint.id().0.clone(),
                kind: constraint_kind_name(constraint).to_string(),
                source,
                target,
            });
        }

        EngineeringContextView {
            part,
            features,
            parameters,
            constraints,
            unresolved,
        }
    }
}

fn feature_type_name(feature: &EngineeringFeature) -> &'static str {
    match feature {
        EngineeringFeature::BaseProfile { .. } => "base_profile",
        EngineeringFeature::Hole { .. } => "hole",
        EngineeringFeature::HolePattern { pattern, .. } => match pattern {
            crate::feature::HolePattern::Circular { .. } => "hole_pattern:circular",
            crate::feature::HolePattern::Grid { .. } => "hole_pattern:grid",
            crate::feature::HolePattern::Corner { .. } => "hole_pattern:corner",
        },
        EngineeringFeature::Slot { .. } => "slot",
        EngineeringFeature::Fillet { .. } => "fillet",
        EngineeringFeature::ConstructionCircle { .. } => "construction_circle",
        EngineeringFeature::Parametric { feature, .. } => match feature {
            crate::parametric::ParametricFeature::DatumPlane { .. } => "datum_plane",
            crate::parametric::ParametricFeature::DatumAxis { .. } => "datum_axis",
            crate::parametric::ParametricFeature::Sketch { .. } => "sketch",
            crate::parametric::ParametricFeature::Path { .. } => "path",
            crate::parametric::ParametricFeature::Sweep { .. } => "sweep",
            crate::parametric::ParametricFeature::Loft { .. } => "loft",
            crate::parametric::ParametricFeature::Extrude { .. } => "extrude",
            crate::parametric::ParametricFeature::Revolve { .. } => "revolve",
            crate::parametric::ParametricFeature::Hole { .. } => "hole",
            crate::parametric::ParametricFeature::Pattern { .. } => "pattern",
            crate::parametric::ParametricFeature::Fillet { .. } => "fillet",
            crate::parametric::ParametricFeature::Chamfer { .. } => "chamfer",
        },
    }
}

fn origin_name(origin: ParameterOrigin) -> &'static str {
    match origin {
        ParameterOrigin::User => "user",
        ParameterOrigin::Derived => "derived",
        ParameterOrigin::Default => "default",
        ParameterOrigin::AiAssumption => "ai_assumption",
    }
}

fn constraint_kind_name(constraint: &crate::feature::EngineeringConstraint) -> &'static str {
    use crate::feature::EngineeringConstraint as C;
    match constraint {
        C::Symmetric { .. } => "symmetric",
        C::Aligned { .. } => "aligned",
        C::Distance { .. } => "distance",
        C::Tangent { .. } => "tangent",
        C::Concentric { .. } => "concentric",
        C::Fixed { .. } => "fixed",
    }
}

fn constraint_reference_summary(
    constraint: &crate::feature::EngineeringConstraint,
) -> (String, String) {
    use crate::feature::EngineeringConstraint as C;
    match constraint {
        C::Fixed { target, .. } => (target.0.clone(), target.0.clone()),
        C::Symmetric { source, target, .. }
        | C::Aligned { source, target, .. }
        | C::Distance { source, target, .. }
        | C::Tangent { source, target, .. }
        | C::Concentric { source, target, .. } => (source.0.clone(), target.0.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::EngineeringCommand;
    use crate::executor::CommandExecutor;
    use crate::{FeatureId, ParameterId, PartId};

    fn sample_model() -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId("mounting_plate".into()),
                name: "mounting_plate".into(),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::BaseProfile {
                    id: FeatureId("main".into()),
                    name: "main".into(),
                    profile: crate::BaseProfile::Rectangle {
                        width: 120.0,
                        height: 80.0,
                    },
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("plate_width".into()),
                value: crate::parameter::ParameterValue::Scalar(120.0),
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("hole_diameter".into()),
                value: crate::parameter::ParameterValue::Unresolved,
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
        model
    }

    #[test]
    fn context_view_is_deterministic_and_read_only() {
        let model = sample_model();
        let view = model.context_view();
        let view2 = model.context_view();
        assert_eq!(view, view2);
        // Read-only: the model is unchanged.
        assert_eq!(model.part.as_ref().unwrap().name, "mounting_plate");

        assert_eq!(view.part.as_ref().unwrap().id, "mounting_plate");
        assert!(view
            .features
            .iter()
            .any(|f| f.id == "main" && f.feature_type == "base_profile"));
        let width = view
            .parameters
            .iter()
            .find(|p| p.id == "plate_width")
            .unwrap();
        assert_eq!(width.value, "120");
        assert_eq!(width.origin, "user");
        let unresolved = view
            .parameters
            .iter()
            .find(|p| p.id == "hole_diameter")
            .unwrap();
        assert_eq!(unresolved.value, "UNRESOLVED");
        assert!(view.unresolved.contains(&"hole_diameter".to_string()));
        // No SCC internals in the view.
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("constraint_ir"));
        assert!(!json.contains("graph_state"));
        assert!(!json.contains("execution_trace"));
    }

    #[test]
    fn expression_display_is_canonical() {
        use crate::expression::Expression;
        let expr = Expression::divide_parameter_by_constant(ParameterId("plate_width".into()), 8.0);
        assert_eq!(expr.to_string(), "(plate_width / 8)");
    }
}
