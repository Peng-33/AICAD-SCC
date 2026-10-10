//! The unified engineering operation protocol.
//!
//! Every upper-layer entry point (DSL, tool calling, natural language, UI,
//! mouse editing) must eventually produce an [`EngineeringCommand`]. A command
//! expresses *intent* only: it never computes geometry, never mutates runtime
//! state, and never depends on LLM / HTTP / UI infrastructure. It serializes
//! deterministically and carries its own command-level validation.

use scc_core::authority::AuthorityLevel;
use serde::{Deserialize, Serialize};

use crate::binding::{FeatureParameterReference, FeatureParameterSlot};
use crate::error::EngineeringError;
use crate::expression::Expression;
use crate::feature::{DrawingProperty, EngineeringConstraint, EngineeringFeature};
use crate::model::ParameterOrigin;
use crate::parameter::ParameterValue;
use crate::spatial::{HoleExtent, SourcedLength};
use crate::specification::{DimensionalSpecification, EngineeringDimensionRef};
use crate::{EngineeringConstraintId, FeatureId, ParameterId, PartId, Point2D};

/// A single high-level engineering operation.
///
/// Serialized with an internal `op` tag so documents read as
/// `{"op": "set_parameter", "target": ..., "value": ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EngineeringCommand {
    /// Create an empty named part.
    CreatePart { id: PartId, name: String },
    /// Add a feature to the current design.
    AddFeature { feature: EngineeringFeature },
    /// Replace a feature's typed parameters while preserving its stable ID.
    ReplaceFeature { feature: EngineeringFeature },
    /// Suppress or unsuppress one generic-history node without deleting it.
    SetFeatureSuppressed { target: FeatureId, suppressed: bool },
    /// Move a feature before another history node, or to the end when absent.
    ReorderFeature {
        target: FeatureId,
        before: Option<FeatureId>,
    },
    /// Remove a feature by id.
    RemoveFeature { target: FeatureId },
    /// Set a parameter to a concrete value or to `Unresolved`.
    SetParameter {
        target: ParameterId,
        value: ParameterValue,
    },
    /// Set one schema-backed feature slot through a stable typed reference.
    /// An unbound supported v2 Hole slot may create its deterministic parameter
    /// identity and binding as part of the same atomic command.
    SetFeatureParameter {
        target: FeatureParameterReference,
        value: ParameterValue,
    },
    /// Bind a parameter to a symbolic expression.
    SetExpression {
        target: ParameterId,
        expression: Expression,
    },
    /// Declare a new parameter (Phase 4: needed so a parameterized model can
    /// be built from an empty state purely through commands).
    DeclareParameter {
        id: ParameterId,
        value: ParameterValue,
        origin: ParameterOrigin,
        authority: AuthorityLevel,
    },
    /// Bind one typed feature slot to an existing parameter (Phase 4; routes
    /// through the Phase 3 strong-typed [`FeatureParameterSlot`] validation).
    BindFeatureParameter {
        feature: FeatureId,
        slot: FeatureParameterSlot,
        parameter: ParameterId,
    },
    /// Add a high-level engineering constraint between features.
    AddConstraint { constraint: EngineeringConstraint },
    /// Remove a constraint by id.
    RemoveConstraint { id: EngineeringConstraintId },
    /// Move a feature to an explicit position.
    MoveFeature {
        target: FeatureId,
        position: Point2D,
    },
    /// Change a drawing presentation property.
    SetDrawingProperty { property: DrawingProperty },
    /// Declare (or clear with `None`) the part thickness (Phase 9.1).
    SetPartThickness { source: Option<SourcedLength> },
    /// Declare (or clear with `None`) the extent of a hole-bearing feature
    /// (Phase 9.1). The target must be a `Hole` or `HolePattern`.
    SetHoleExtent {
        target: FeatureId,
        extent: Option<HoleExtent>,
    },
    /// Set or clear one typed engineering dimensional specification (Phase
    /// 11.5). The target is never a free-form string and `None` clears it.
    SetDimensionalSpecification {
        target: EngineeringDimensionRef,
        specification: Option<DimensionalSpecification>,
    },
}

impl EngineeringCommand {
    /// Command-level validation.
    ///
    /// Rejects only data that is obviously illegal *for the command itself*:
    /// empty/invalid IDs, empty names, non-finite numbers, non-positive
    /// dimensions, zero pattern counts, empty choices, and division by a
    /// constant zero. Geometric or cross-feature conflicts (for example a hole
    /// that is larger than the base profile) are the responsibility of the
    /// engineering-model / semantic validator in a later phase.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        match self {
            EngineeringCommand::CreatePart { id, name } => {
                crate::validate_identifier("part id", &id.0)?;
                if !crate::is_valid_name(name) {
                    return Err(EngineeringError::EmptyName);
                }
            }
            EngineeringCommand::AddFeature { feature }
            | EngineeringCommand::ReplaceFeature { feature } => feature.validate()?,
            EngineeringCommand::SetFeatureSuppressed { target, .. } => {
                crate::validate_identifier("feature id", &target.0)?;
            }
            EngineeringCommand::ReorderFeature { target, before } => {
                crate::validate_identifier("feature id", &target.0)?;
                if let Some(before) = before {
                    crate::validate_identifier("feature id", &before.0)?;
                }
            }
            EngineeringCommand::RemoveFeature { target } => {
                crate::validate_identifier("feature id", &target.0)?;
            }
            EngineeringCommand::SetParameter { target, value } => {
                crate::validate_identifier("parameter id", &target.0)?;
                value.validate()?;
            }
            EngineeringCommand::SetFeatureParameter { target, value } => {
                crate::validate_identifier("part id", &target.part_id.0)?;
                crate::validate_identifier("feature id", &target.feature_id.0)?;
                crate::validate_identifier("parameter id", &target.parameter_id.0)?;
                value.validate()?;
            }
            EngineeringCommand::SetExpression { target, expression } => {
                crate::validate_identifier("parameter id", &target.0)?;
                expression.validate("expression")?;
            }
            EngineeringCommand::DeclareParameter { id, value, .. } => {
                crate::validate_identifier("parameter id", &id.0)?;
                value.validate()?;
            }
            EngineeringCommand::BindFeatureParameter {
                feature, parameter, ..
            } => {
                crate::validate_identifier("feature id", &feature.0)?;
                crate::validate_identifier("parameter id", &parameter.0)?;
            }
            EngineeringCommand::AddConstraint { constraint } => constraint.validate()?,
            EngineeringCommand::RemoveConstraint { id } => {
                crate::validate_identifier("constraint id", &id.0)?;
            }
            EngineeringCommand::MoveFeature { target, position } => {
                crate::validate_identifier("feature id", &target.0)?;
                crate::finite_point(*position, "move position")?;
            }
            EngineeringCommand::SetDrawingProperty { property } => property.validate()?,
            EngineeringCommand::SetPartThickness { source } => {
                if let Some(SourcedLength::Parameter { id }) = source {
                    // Same identifier rule as every other parameter reference:
                    // a malformed id is an InvalidId, not a later
                    // ParameterNotFound (SCC-ENG-017).
                    crate::validate_identifier("parameter id", &id.0)?;
                }
                if let Some(SourcedLength::Literal { value }) = source {
                    crate::positive("part thickness", *value)?;
                }
                // Clearing carries no constraint.
            }
            EngineeringCommand::SetHoleExtent { target, extent } => {
                crate::validate_identifier("feature id", &target.0)?;
                if let Some(HoleExtent::Blind {
                    depth: SourcedLength::Literal { value },
                    ..
                }) = extent
                {
                    crate::positive("blind hole depth", *value)?;
                }
            }
            EngineeringCommand::SetDimensionalSpecification {
                target,
                specification,
            } => {
                target.validate_identifiers()?;
                if let Some(specification) = specification {
                    specification.clone().canonicalize()?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::Expression;
    use crate::feature::{HoleDefinition, HolePattern, ProjectionMethod};
    use crate::parameter::ParameterValue;
    use crate::specification::{DecimalPlaces, DimensionalTolerance};

    fn feature_id(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    fn parameter_id(id: &str) -> ParameterId {
        ParameterId(id.into())
    }

    fn roundtrip(command: &EngineeringCommand) -> EngineeringCommand {
        let json = serde_json::to_string(command).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn create_part_constructs_validates_and_serializes() {
        let command = EngineeringCommand::CreatePart {
            id: PartId("plate".into()),
            name: "plate".into(),
        };
        assert!(command.validate().is_ok());

        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("\"op\":\"create_part\""));
        assert!(json.contains("\"id\":\"plate\""));
        assert!(json.contains("\"name\":\"plate\""));
        assert_eq!(roundtrip(&command), command);
    }

    #[test]
    fn create_part_rejects_empty_id_and_bad_name() {
        assert!(matches!(
            EngineeringCommand::CreatePart {
                id: PartId("".into()),
                name: "plate".into(),
            }
            .validate()
            .unwrap_err(),
            EngineeringError::EmptyId { .. }
        ));

        assert!(matches!(
            EngineeringCommand::CreatePart {
                id: PartId("plate".into()),
                name: "  ".into(),
            }
            .validate()
            .unwrap_err(),
            EngineeringError::EmptyName
        ));

        assert!(EngineeringCommand::CreatePart {
            id: PartId("Plate".into()),
            name: "plate".into(),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn add_feature_base_profile_roundtrips() {
        let command = EngineeringCommand::AddFeature {
            feature: crate::feature::EngineeringFeature::BaseProfile {
                id: feature_id("plate"),
                name: "plate".into(),
                profile: crate::BaseProfile::Rectangle {
                    width: 80.0,
                    height: 80.0,
                },
            },
        };
        assert!(command.validate().is_ok());
        assert_eq!(roundtrip(&command), command);
    }

    #[test]
    fn add_feature_hole_roundtrips() {
        let command = EngineeringCommand::AddFeature {
            feature: crate::feature::EngineeringFeature::Hole {
                id: feature_id("center_hole"),
                diameter: 100.0,
            },
        };
        assert!(command.validate().is_ok());
        assert_eq!(roundtrip(&command), command);
    }

    #[test]
    fn add_feature_hole_pattern_roundtrips() {
        let command = EngineeringCommand::AddFeature {
            feature: crate::feature::EngineeringFeature::HolePattern {
                id: feature_id("bolt_circle"),
                pattern: HolePattern::Circular {
                    center: Point2D::new(0.0, 0.0),
                    pitch_circle_diameter: 126.0,
                    count: 8,
                    start_angle: 0.0,
                    hole: HoleDefinition { diameter: 10.0 },
                },
            },
        };
        assert!(command.validate().is_ok());
        assert_eq!(roundtrip(&command), command);

        // The serialized pattern remains parametric (count 8), not 8 holes.
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&command).unwrap()).unwrap();
        let pattern = &value["feature"];
        assert_eq!(pattern["type"], "hole_pattern");
        assert_eq!(pattern["pattern"]["count"], 8);
        assert!(pattern["pattern"].get("holes").is_none());
    }

    #[test]
    fn set_parameter_scalar_integer_unresolved() {
        for value in [
            ParameterValue::Scalar(160.0),
            ParameterValue::Integer(4),
            ParameterValue::Unresolved,
        ] {
            let command = EngineeringCommand::SetParameter {
                target: parameter_id("outer_diameter"),
                value,
            };
            assert!(command.validate().is_ok());
            assert_eq!(roundtrip(&command), command);
        }
    }

    #[test]
    fn set_parameter_rejects_non_finite_value_and_empty_target() {
        assert!(EngineeringCommand::SetParameter {
            target: parameter_id("d"),
            value: ParameterValue::Scalar(f64::NAN),
        }
        .validate()
        .is_err());

        assert!(matches!(
            EngineeringCommand::SetParameter {
                target: parameter_id(""),
                value: ParameterValue::Unresolved,
            }
            .validate()
            .unwrap_err(),
            EngineeringError::EmptyId { .. }
        ));
    }

    #[test]
    fn set_expression_plate_width_over_eight() {
        let command = EngineeringCommand::SetExpression {
            target: parameter_id("hole_diameter"),
            expression: Expression::divide_parameter_by_constant(parameter_id("plate_width"), 8.0),
        };
        assert!(command.validate().is_ok());

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&command).unwrap()).unwrap();
        assert_eq!(value["op"], "set_expression");
        assert_eq!(value["target"], "hole_diameter");
        assert_eq!(value["expression"]["op"], "divide");
        assert_eq!(value["expression"]["left"]["parameter"], "plate_width");
        assert_eq!(value["expression"]["right"]["value"], 8.0);

        assert_eq!(roundtrip(&command), command);
    }

    #[test]
    fn set_expression_rejects_division_by_constant_zero() {
        let command = EngineeringCommand::SetExpression {
            target: parameter_id("hole_diameter"),
            expression: Expression::divide_parameter_by_constant(parameter_id("plate_width"), 0.0),
        };
        assert!(matches!(
            command.validate().unwrap_err(),
            EngineeringError::DivisionByConstantZero { .. }
        ));
    }

    #[test]
    fn add_and_remove_constraint_roundtrip() {
        let add = EngineeringCommand::AddConstraint {
            constraint: crate::feature::EngineeringConstraint::Concentric {
                id: EngineeringConstraintId("conc_1".into()),
                source: feature_id("center_hole"),
                target: feature_id("bolt_circle"),
            },
        };
        assert!(add.validate().is_ok());
        assert_eq!(roundtrip(&add), add);

        let remove = EngineeringCommand::RemoveConstraint {
            id: EngineeringConstraintId("conc_1".into()),
        };
        assert!(remove.validate().is_ok());
        assert_eq!(roundtrip(&remove), remove);
        assert!(remove.validate().is_ok());
    }

    #[test]
    fn remove_feature_validates_id() {
        let command = EngineeringCommand::RemoveFeature {
            target: feature_id("center_hole"),
        };
        assert!(command.validate().is_ok());
        assert_eq!(roundtrip(&command), command);

        assert!(EngineeringCommand::RemoveFeature {
            target: feature_id(""),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn move_feature_roundtrips_and_rejects_non_finite_position() {
        let command = EngineeringCommand::MoveFeature {
            target: feature_id("center_hole"),
            position: Point2D::new(40.0, 40.0),
        };
        assert!(command.validate().is_ok());
        assert_eq!(roundtrip(&command), command);

        assert!(EngineeringCommand::MoveFeature {
            target: feature_id("center_hole"),
            position: Point2D::new(f64::NAN, 0.0),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn set_drawing_property_roundtrips() {
        let command = EngineeringCommand::SetDrawingProperty {
            property: crate::feature::DrawingProperty::ProjectionMethod {
                method: ProjectionMethod::ThirdAngle,
            },
        };
        assert!(command.validate().is_ok());
        assert_eq!(roundtrip(&command), command);

        let scale = EngineeringCommand::SetDrawingProperty {
            property: crate::feature::DrawingProperty::Scale { value: 2.0 },
        };
        assert!(scale.validate().is_ok());

        assert!(EngineeringCommand::SetDrawingProperty {
            property: crate::feature::DrawingProperty::Scale { value: 0.0 },
        }
        .validate()
        .is_err());
    }

    #[test]
    fn all_commands_serde_roundtrip() {
        let commands = vec![
            EngineeringCommand::CreatePart {
                id: PartId("plate".into()),
                name: "plate".into(),
            },
            EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::Hole {
                    id: feature_id("h1"),
                    diameter: 10.0,
                },
            },
            EngineeringCommand::RemoveFeature {
                target: feature_id("h1"),
            },
            EngineeringCommand::SetParameter {
                target: parameter_id("d"),
                value: ParameterValue::Scalar(10.0),
            },
            EngineeringCommand::SetExpression {
                target: parameter_id("d"),
                expression: Expression::divide_parameter_by_constant(parameter_id("w"), 8.0),
            },
            EngineeringCommand::DeclareParameter {
                id: parameter_id("plate_width"),
                value: ParameterValue::Scalar(120.0),
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
            EngineeringCommand::BindFeatureParameter {
                feature: feature_id("plate"),
                slot: crate::binding::FeatureParameterSlot::Width,
                parameter: parameter_id("plate_width"),
            },
            EngineeringCommand::AddConstraint {
                constraint: crate::feature::EngineeringConstraint::Fixed {
                    id: EngineeringConstraintId("c1".into()),
                    target: feature_id("plate"),
                },
            },
            EngineeringCommand::RemoveConstraint {
                id: EngineeringConstraintId("c1".into()),
            },
            EngineeringCommand::MoveFeature {
                target: feature_id("h1"),
                position: Point2D::new(1.0, 2.0),
            },
            EngineeringCommand::SetDrawingProperty {
                property: crate::feature::DrawingProperty::ShowCenterlines { enabled: true },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 8.0 }),
            },
            EngineeringCommand::SetPartThickness { source: None },
            EngineeringCommand::SetHoleExtent {
                target: feature_id("center_hole"),
                extent: Some(crate::spatial::HoleExtent::ThroughAll),
            },
            EngineeringCommand::SetHoleExtent {
                target: feature_id("center_hole"),
                extent: Some(crate::spatial::HoleExtent::Blind {
                    start_face: crate::spatial::HoleStartFace::Base,
                    depth: SourcedLength::Literal { value: 4.0 },
                }),
            },
            EngineeringCommand::SetHoleExtent {
                target: feature_id("center_hole"),
                extent: None,
            },
            EngineeringCommand::SetDimensionalSpecification {
                target: EngineeringDimensionRef::FeatureSlot {
                    feature: feature_id("h1"),
                    slot: FeatureParameterSlot::Diameter,
                },
                specification: Some(
                    DimensionalSpecification::new(
                        DimensionalTolerance::symmetric(0.05).unwrap(),
                        Some(DecimalPlaces(2)),
                    )
                    .unwrap(),
                ),
            },
        ];
        for command in commands {
            assert!(command.validate().is_ok());
            assert_eq!(roundtrip(&command), command);
        }
    }

    #[test]
    fn serialization_is_deterministic() {
        let command = EngineeringCommand::SetExpression {
            target: parameter_id("hole_diameter"),
            expression: Expression::divide_parameter_by_constant(parameter_id("plate_width"), 8.0),
        };
        let a = serde_json::to_string_pretty(&command).unwrap();
        let b = serde_json::to_string_pretty(&command).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            serde_json::to_string(&command).unwrap(),
            serde_json::to_string(&command).unwrap()
        );
    }

    #[test]
    fn repeated_construction_is_stable() {
        let build = || EngineeringCommand::AddFeature {
            feature: crate::feature::EngineeringFeature::HolePattern {
                id: feature_id("bolt_circle"),
                pattern: HolePattern::Circular {
                    center: Point2D::new(0.0, 0.0),
                    pitch_circle_diameter: 126.0,
                    count: 8,
                    start_angle: 0.0,
                    hole: HoleDefinition { diameter: 10.0 },
                },
            },
        };
        assert_eq!(
            serde_json::to_string(&build()).unwrap(),
            serde_json::to_string(&build()).unwrap()
        );
    }

    #[test]
    fn invalid_add_feature_is_rejected_at_command_level() {
        let command = EngineeringCommand::AddFeature {
            feature: crate::feature::EngineeringFeature::Hole {
                id: feature_id("h1"),
                diameter: -10.0,
            },
        };
        assert!(matches!(
            command.validate().unwrap_err(),
            EngineeringError::NonPositiveLength { .. }
        ));
    }

    #[test]
    fn set_part_thickness_validates_literal_and_roundtrips() {
        let set = |value: f64| EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value }),
        };

        assert!(set(8.0).validate().is_ok());
        assert_eq!(roundtrip(&set(8.0)), set(8.0));

        // Non-finite and non-positive literals are rejected at the command
        // level (same 004/005 semantics as every other dimension).
        for bad in [f64::NAN, f64::INFINITY, 0.0, -2.0] {
            assert!(
                set(bad).validate().is_err(),
                "thickness {} must be rejected",
                bad
            );
        }

        // Clearing is always legal; parameter-driven thickness carries no
        // numeric constraint at this level.
        assert!(EngineeringCommand::SetPartThickness { source: None }
            .validate()
            .is_ok());
        assert!(EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Parameter {
                id: parameter_id("thickness")
            })
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn set_part_thickness_rejects_malformed_parameter_id_as_invalid_id() {
        let command = EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Parameter {
                id: parameter_id("Bad_Id"),
            }),
        };
        assert!(matches!(
            command.validate().unwrap_err(),
            EngineeringError::InvalidId { .. }
        ));
    }

    #[test]
    fn set_hole_extent_validates_and_roundtrips() {
        let blind = |depth: f64| EngineeringCommand::SetHoleExtent {
            target: feature_id("center_hole"),
            extent: Some(crate::spatial::HoleExtent::Blind {
                start_face: crate::spatial::HoleStartFace::Opposite,
                depth: SourcedLength::Literal { value: depth },
            }),
        };

        assert!(blind(4.0).validate().is_ok());
        assert_eq!(roundtrip(&blind(4.0)), blind(4.0));

        // Blind literal depths obey the same finite/positive rules.
        for bad in [f64::NAN, 0.0, -1.0] {
            assert!(
                blind(bad).validate().is_err(),
                "depth {} must be rejected",
                bad
            );
        }

        // The target must be a valid feature id.
        assert!(matches!(
            EngineeringCommand::SetHoleExtent {
                target: feature_id(""),
                extent: None,
            }
            .validate()
            .unwrap_err(),
            EngineeringError::EmptyId { .. }
        ));

        // ThroughAll and clearing round-trip too.
        let through_all = EngineeringCommand::SetHoleExtent {
            target: feature_id("center_hole"),
            extent: Some(crate::spatial::HoleExtent::ThroughAll),
        };
        assert!(through_all.validate().is_ok());
        assert_eq!(roundtrip(&through_all), through_all);
    }

    #[test]
    fn spatial_commands_serialize_with_expected_tags() {
        let value: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Parameter {
                    id: parameter_id("t"),
                }),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["op"], "set_part_thickness");
        assert_eq!(value["source"]["source"], "parameter");
        assert_eq!(value["source"]["id"], "t");

        let value: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&EngineeringCommand::SetHoleExtent {
                target: feature_id("h1"),
                extent: Some(crate::spatial::HoleExtent::Blind {
                    start_face: crate::spatial::HoleStartFace::Base,
                    depth: SourcedLength::Literal { value: 4.0 },
                }),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["op"], "set_hole_extent");
        assert_eq!(value["target"], "h1");
        assert_eq!(value["extent"]["type"], "blind");
        assert_eq!(value["extent"]["start_face"], "base");
        assert_eq!(value["extent"]["depth"]["source"], "literal");
        assert_eq!(value["extent"]["depth"]["value"], 4.0);
    }
}
