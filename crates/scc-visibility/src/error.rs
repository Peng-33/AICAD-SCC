//! Structured errors for the Phase 9.5 visibility sidecar.

use scc_core::error::{ErrorStage, StructuredError};
use scc_projection::{PrincipalViewKind, ProjectionError, ProjectionPrimitiveId};
use scc_spatial::SpatialCutId;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum VisibilityError {
    #[error("invalid visibility input: {reason}")]
    InvalidVisibilityInput {
        reason: InvalidVisibilityInputReason,
    },

    #[error("projection input failed visibility validation: {error}")]
    ProjectionInput {
        #[source]
        error: ProjectionError,
    },

    #[error("visibility configuration is unsupported: {reason}")]
    UnsupportedVisibilityConfiguration { reason: UnsupportedVisibilityReason },

    #[error("coincident candidates have ambiguous depth in {view:?}: {ids:?}")]
    AmbiguousCoincidentCandidates {
        view: PrincipalViewKind,
        ids: Vec<ProjectionPrimitiveId>,
    },

    #[error("visibility decision is missing for {id} in {view:?}")]
    MissingVisibilityDecision {
        view: PrincipalViewKind,
        id: ProjectionPrimitiveId,
    },

    #[error("visibility decision is invalid for {id:?} in {view:?}: {reason}")]
    InvalidVisibilityDecision {
        view: Option<PrincipalViewKind>,
        id: Option<ProjectionPrimitiveId>,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidVisibilityInputReason {
    #[error("spatial model is malformed: {0}")]
    MalformedSpatialModel(String),
    #[error("projection model is malformed: {0}")]
    MalformedProjectionModel(String),
    #[error("primitive role does not match its source: {0}")]
    RoleSourceMismatch(String),
    #[error("unexpected projected geometry shape: {0}")]
    UnexpectedGeometry(String),
    #[error("unexpected primitive count: {0}")]
    UnexpectedPrimitiveCount(String),
    #[error("non-finite or overflowing visibility comparison: {0}")]
    NumericOverflow(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnsupportedVisibilityReason {
    #[error("cut {id} is outside the rectangular body")]
    OutsideBodyCut { id: SpatialCutId },
    #[error("cut {id} is tangent to the rectangular body")]
    TangentBodyCut { id: SpatialCutId },
    #[error("cuts {first} and {second} overlap")]
    OverlappingCuts {
        first: SpatialCutId,
        second: SpatialCutId,
    },
    #[error("cuts {first} and {second} are tangent")]
    TangentCuts {
        first: SpatialCutId,
        second: SpatialCutId,
    },
    #[error("cuts {first} and {second} are coincident")]
    CoincidentCuts {
        first: SpatialCutId,
        second: SpatialCutId,
    },
    #[error("cut {id} has unsupported Z topology")]
    UnsupportedZTopology { id: SpatialCutId },
    #[error("visibility would require candidate splitting or clipping")]
    PartialCandidateClassification,
    #[error("visibility comparison overflowed for {context}")]
    NumericOverflow { context: String },
}

impl VisibilityError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidVisibilityInput { .. } | Self::ProjectionInput { .. } => "SCC-VIS-001",
            Self::UnsupportedVisibilityConfiguration { .. } => "SCC-VIS-002",
            Self::AmbiguousCoincidentCandidates { .. } => "SCC-VIS-003",
            Self::MissingVisibilityDecision { .. } => "SCC-VIS-004",
            Self::InvalidVisibilityDecision { .. } => "SCC-VIS-005",
        }
    }
}

impl From<ProjectionError> for VisibilityError {
    fn from(error: ProjectionError) -> Self {
        Self::ProjectionInput { error }
    }
}

impl From<VisibilityError> for StructuredError {
    fn from(error: VisibilityError) -> Self {
        let code = error.code();
        let mut structured = StructuredError::new(code, ErrorStage::Visibility, error.to_string());
        if let VisibilityError::ProjectionInput { error: source } = error {
            structured = structured.with_source(source.into());
        }
        structured
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::error::ErrorSeverity;

    #[test]
    fn error_codes_are_sequential_and_structured() {
        let values = [
            VisibilityError::InvalidVisibilityInput {
                reason: InvalidVisibilityInputReason::MalformedSpatialModel("bad".into()),
            },
            VisibilityError::UnsupportedVisibilityConfiguration {
                reason: UnsupportedVisibilityReason::PartialCandidateClassification,
            },
            VisibilityError::AmbiguousCoincidentCandidates {
                view: PrincipalViewKind::Front,
                ids: Vec::new(),
            },
            VisibilityError::MissingVisibilityDecision {
                view: PrincipalViewKind::Front,
                id: ProjectionPrimitiveId("id".into()),
            },
            VisibilityError::InvalidVisibilityDecision {
                view: None,
                id: None,
                reason: "bad".into(),
            },
        ];
        for (index, error) in values.iter().enumerate() {
            assert_eq!(error.code(), format!("SCC-VIS-{:03}", index + 1));
            let structured: StructuredError = error.clone().into();
            assert_eq!(structured.stage, ErrorStage::Visibility);
            assert_eq!(structured.severity, ErrorSeverity::Error);
            assert!(!structured.recoverable);
            structured.validate_contract().unwrap();
        }
    }
}
