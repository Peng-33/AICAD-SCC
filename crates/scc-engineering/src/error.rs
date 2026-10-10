//! Structured errors for the engineering command layer.
//!
//! Every [`EngineeringError`] converts losslessly to an
//! [`scc_core::error::StructuredError`] carrying `SCC-ENG-{NNN}` codes under
//! [`scc_core::error::ErrorStage::Engineering`], so the engineering layer
//! participates in the ADR-002 structured error contract without owning the
//! core error types.

use scc_core::error::{ErrorStage, StructuredError};

use crate::binding::FeatureParameterSlot;
use crate::{EngineeringConstraintId, FeatureId, ParameterId, PartId};

/// A command-level or model-level validation failure in the engineering layer.
///
/// Phase 1 variants (`SCC-ENG-001`..`008`) validate a single command's data.
/// Phase 2 variants (`SCC-ENG-009`..`024`) describe model-level failures raised
/// while applying a command to an [`crate::model::EngineeringModel`] or while
/// validating / evaluating the model.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineeringError {
    #[error("empty id for {what}")]
    EmptyId { what: &'static str },

    #[error(
        "id `{id}` for {what} is not a valid engineering identifier ([a-z][a-z0-9_-]{{0,63}})"
    )]
    InvalidId { what: &'static str, id: String },

    #[error("name must be non-empty and must not have surrounding whitespace")]
    EmptyName,

    #[error("{what} must be a finite number")]
    NonFiniteValue { what: &'static str },

    #[error("{what} must be positive, got {value}")]
    NonPositiveLength { what: &'static str, value: f64 },

    #[error("{what} must be at least 1, got {count}")]
    ZeroCount { what: &'static str, count: u32 },

    #[error("division by the constant zero in {what}")]
    DivisionByConstantZero { what: &'static str },

    #[error("choice value must not be empty in {what}")]
    EmptyChoice { what: &'static str },

    #[error("part already exists: {part_id}")]
    DuplicatePart { part_id: PartId },

    #[error("no part exists; apply CreatePart before modifying the model")]
    PartMissing,

    #[error("feature already exists: {feature_id}")]
    DuplicateFeature { feature_id: FeatureId },

    #[error("feature not found: {feature_id}")]
    FeatureNotFound { feature_id: FeatureId },

    #[error("feature {feature_id} is referenced by {referenced_by:?} and cannot be removed")]
    FeatureInUse {
        feature_id: FeatureId,
        referenced_by: Vec<String>,
    },

    #[error("constraint already exists: {constraint_id}")]
    DuplicateConstraint {
        constraint_id: EngineeringConstraintId,
    },

    #[error("constraint not found: {constraint_id}")]
    ConstraintNotFound {
        constraint_id: EngineeringConstraintId,
    },

    #[error("parameter already exists: {parameter_id}")]
    DuplicateParameter { parameter_id: ParameterId },

    #[error("parameter not found: {parameter_id}")]
    ParameterNotFound { parameter_id: ParameterId },

    #[error("dangling reference to `{target}` at {location}")]
    DanglingReference { target: String, location: String },

    #[error("expression references missing parameter {parameter_id}")]
    ExpressionReferenceMissing { parameter_id: ParameterId },

    #[error("parameter dependency cycle detected: {cycle:?}")]
    ExpressionCycle { cycle: Vec<String> },

    #[error("expression evaluates to division by zero")]
    ExpressionDivideByZero,

    #[error("expression uses a boolean or choice value in arithmetic")]
    ExpressionTypeMismatch,

    #[error("feature {feature_id} cannot be moved: {reason}")]
    UnsupportedMove {
        feature_id: FeatureId,
        reason: &'static str,
    },

    #[error("model validation failed: {message}")]
    ModelValidationFailed { message: String },

    #[error("feature {feature_id} ({feature_type}) cannot be lowered")]
    UnsupportedFeatureLowering {
        feature_id: FeatureId,
        feature_type: &'static str,
    },

    #[error("constraint {constraint_id} ({kind}) cannot be lowered")]
    UnsupportedConstraintLowering {
        constraint_id: EngineeringConstraintId,
        kind: &'static str,
    },

    #[error(
        "cannot lower feature {feature_id}: required parameter {parameter_id} for slot {slot} is Unresolved"
    )]
    UnresolvedRequiredParameter {
        feature_id: FeatureId,
        slot: FeatureParameterSlot,
        parameter_id: ParameterId,
    },

    #[error("invalid feature parameter binding: feature {feature_id}, slot {slot}: {reason}")]
    InvalidFeatureParameterBinding {
        feature_id: FeatureId,
        slot: FeatureParameterSlot,
        reason: String,
    },

    #[error(
        "cannot lower feature {feature_id}: required slot {slot} has no value (no binding and no literal)"
    )]
    MissingFeatureParameterBinding {
        feature_id: FeatureId,
        slot: FeatureParameterSlot,
    },

    #[error("lowering failed: {message}")]
    LoweringFailed { message: String },

    #[error(
        "spatial extent target {feature_id} is not a hole-bearing feature (Hole or HolePattern)"
    )]
    InvalidSpatialTarget { feature_id: FeatureId },

    #[error(
        "blind depth {depth} mm reaches or exceeds the part thickness {thickness} mm for feature {feature_id}"
    )]
    DepthExceedsThickness {
        feature_id: FeatureId,
        depth: f64,
        thickness: f64,
    },

    #[error("invalid dimensional specification: {reason}")]
    InvalidDimensionalSpecification { reason: String },

    #[error("invalid engineering profile: {reason}")]
    InvalidProfile { reason: String },
}

impl From<EngineeringError> for StructuredError {
    fn from(err: EngineeringError) -> Self {
        let code = match &err {
            EngineeringError::EmptyId { .. } => "SCC-ENG-001",
            EngineeringError::InvalidId { .. } => "SCC-ENG-002",
            EngineeringError::EmptyName => "SCC-ENG-003",
            EngineeringError::NonFiniteValue { .. } => "SCC-ENG-004",
            EngineeringError::NonPositiveLength { .. } => "SCC-ENG-005",
            EngineeringError::ZeroCount { .. } => "SCC-ENG-006",
            EngineeringError::DivisionByConstantZero { .. } => "SCC-ENG-007",
            EngineeringError::EmptyChoice { .. } => "SCC-ENG-008",
            EngineeringError::DuplicatePart { .. } => "SCC-ENG-009",
            EngineeringError::PartMissing => "SCC-ENG-010",
            EngineeringError::DuplicateFeature { .. } => "SCC-ENG-011",
            EngineeringError::FeatureNotFound { .. } => "SCC-ENG-012",
            EngineeringError::FeatureInUse { .. } => "SCC-ENG-013",
            EngineeringError::DuplicateConstraint { .. } => "SCC-ENG-014",
            EngineeringError::ConstraintNotFound { .. } => "SCC-ENG-015",
            EngineeringError::DuplicateParameter { .. } => "SCC-ENG-016",
            EngineeringError::ParameterNotFound { .. } => "SCC-ENG-017",
            EngineeringError::DanglingReference { .. } => "SCC-ENG-018",
            EngineeringError::ExpressionReferenceMissing { .. } => "SCC-ENG-019",
            EngineeringError::ExpressionCycle { .. } => "SCC-ENG-020",
            EngineeringError::ExpressionDivideByZero => "SCC-ENG-021",
            EngineeringError::ExpressionTypeMismatch => "SCC-ENG-022",
            EngineeringError::UnsupportedMove { .. } => "SCC-ENG-023",
            EngineeringError::ModelValidationFailed { .. } => "SCC-ENG-024",
            EngineeringError::UnsupportedFeatureLowering { .. } => "SCC-ENG-025",
            EngineeringError::UnsupportedConstraintLowering { .. } => "SCC-ENG-026",
            EngineeringError::UnresolvedRequiredParameter { .. } => "SCC-ENG-027",
            EngineeringError::InvalidFeatureParameterBinding { .. } => "SCC-ENG-028",
            EngineeringError::MissingFeatureParameterBinding { .. } => "SCC-ENG-029",
            EngineeringError::LoweringFailed { .. } => "SCC-ENG-030",
            EngineeringError::InvalidSpatialTarget { .. } => "SCC-ENG-031",
            EngineeringError::DepthExceedsThickness { .. } => "SCC-ENG-032",
            EngineeringError::InvalidDimensionalSpecification { .. } => "SCC-ENG-033",
            EngineeringError::InvalidProfile { .. } => "SCC-ENG-034",
        };
        StructuredError::new(code, ErrorStage::Engineering, err.to_string())
            .with_recoverable(true)
            .with_suggestion("correct the invalid command input before retrying")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::error::{ErrorSeverity, ErrorStage};

    #[test]
    fn engineering_error_converts_to_structured_error() {
        let err = EngineeringError::NonPositiveLength {
            what: "hole diameter",
            value: -10.0,
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-ENG-005");
        assert_eq!(structured.stage, ErrorStage::Engineering);
        assert_eq!(structured.severity, ErrorSeverity::Error);
        assert!(structured.recoverable);
        structured.validate_contract().unwrap();
    }

    #[test]
    fn every_error_code_passes_contract() {
        let cases = [
            EngineeringError::EmptyId { what: "part id" },
            EngineeringError::InvalidId {
                what: "feature id",
                id: "BAD".into(),
            },
            EngineeringError::EmptyName,
            EngineeringError::NonFiniteValue { what: "width" },
            EngineeringError::NonPositiveLength {
                what: "diameter",
                value: 0.0,
            },
            EngineeringError::ZeroCount {
                what: "hole count",
                count: 0,
            },
            EngineeringError::DivisionByConstantZero { what: "expression" },
            EngineeringError::EmptyChoice { what: "parameter" },
            EngineeringError::DuplicatePart {
                part_id: crate::PartId("plate".into()),
            },
            EngineeringError::PartMissing,
            EngineeringError::DuplicateFeature {
                feature_id: crate::FeatureId("f1".into()),
            },
            EngineeringError::FeatureNotFound {
                feature_id: crate::FeatureId("f1".into()),
            },
            EngineeringError::FeatureInUse {
                feature_id: crate::FeatureId("f1".into()),
                referenced_by: vec!["constraint c1".into()],
            },
            EngineeringError::DuplicateConstraint {
                constraint_id: crate::EngineeringConstraintId("c1".into()),
            },
            EngineeringError::ConstraintNotFound {
                constraint_id: crate::EngineeringConstraintId("c1".into()),
            },
            EngineeringError::DuplicateParameter {
                parameter_id: crate::ParameterId("width".into()),
            },
            EngineeringError::ParameterNotFound {
                parameter_id: crate::ParameterId("width".into()),
            },
            EngineeringError::DanglingReference {
                target: "feature".into(),
                location: "constraint c1".into(),
            },
            EngineeringError::ExpressionReferenceMissing {
                parameter_id: crate::ParameterId("width".into()),
            },
            EngineeringError::ExpressionCycle {
                cycle: vec!["a".into(), "b".into()],
            },
            EngineeringError::ExpressionDivideByZero,
            EngineeringError::ExpressionTypeMismatch,
            EngineeringError::UnsupportedMove {
                feature_id: crate::FeatureId("f1".into()),
                reason: "no position",
            },
            EngineeringError::ModelValidationFailed {
                message: "bad model".into(),
            },
            EngineeringError::UnsupportedFeatureLowering {
                feature_id: crate::FeatureId("f1".into()),
                feature_type: "Fillet",
            },
            EngineeringError::UnsupportedConstraintLowering {
                constraint_id: crate::EngineeringConstraintId("c1".into()),
                kind: "Concentric",
            },
            EngineeringError::UnresolvedRequiredParameter {
                feature_id: crate::FeatureId("f1".into()),
                slot: crate::binding::FeatureParameterSlot::Diameter,
                parameter_id: crate::ParameterId("d".into()),
            },
            EngineeringError::InvalidFeatureParameterBinding {
                feature_id: crate::FeatureId("f1".into()),
                slot: crate::binding::FeatureParameterSlot::StartAngle,
                reason: "slot not valid".into(),
            },
            EngineeringError::MissingFeatureParameterBinding {
                feature_id: crate::FeatureId("f1".into()),
                slot: crate::binding::FeatureParameterSlot::Width,
            },
            EngineeringError::LoweringFailed {
                message: "boom".into(),
            },
            EngineeringError::InvalidSpatialTarget {
                feature_id: crate::FeatureId("f1".into()),
            },
            EngineeringError::DepthExceedsThickness {
                feature_id: crate::FeatureId("f1".into()),
                depth: 10.0,
                thickness: 8.0,
            },
            EngineeringError::InvalidDimensionalSpecification {
                reason: "bad tolerance".into(),
            },
            EngineeringError::InvalidProfile {
                reason: "self-intersecting composite".into(),
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
    fn structured_error_message_is_human_readable() {
        let err = EngineeringError::NonPositiveLength {
            what: "hole diameter",
            value: -10.0,
        };
        let structured: StructuredError = err.into();
        assert!(structured.message.contains("hole diameter"));
        assert!(structured.message.contains("-10"));
    }
}
