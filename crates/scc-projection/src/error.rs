//! Structured errors for the orthographic projection layer.
//!
//! Every [`ProjectionError`] converts losslessly to an
//! [`scc_core::error::StructuredError`] carrying `SCC-PROJ-{NNN}` codes under
//! [`scc_core::error::ErrorStage::Projection`], so the projection layer
//! participates in the ADR-002 structured error contract without owning the
//! core error types (ADR-034 Decision 20, mirroring the ADR-033 pattern).
//!
//! All projection failures are fail-closed (`recoverable: false`): no
//! projection model is ever partially assembled or silently repaired.

use scc_core::error::{ErrorStage, StructuredError};
use thiserror::Error;

use crate::frame::PrincipalViewKind;
use crate::model::{ProjectionBounds2D, ProjectionPrimitiveId};

/// A failure while building or validating a [`crate::ProjectionModel`]
/// (Phase 9.3, ADR-034). All failures are fail-closed: no projection model
/// is ever partially assembled or silently repaired.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ProjectionError {
    /// A primitive was staged for a view that was never added
    /// (`SCC-PROJ-001`).
    #[error("projection view {kind:?} was not added before use")]
    MissingProjectionView {
        /// The view kind that was used without being added.
        kind: PrincipalViewKind,
    },

    /// A projected primitive is geometrically invalid: non-finite
    /// coordinate, zero-length segment, or non-positive radius
    /// (`SCC-PROJ-002`).
    #[error("projected geometry is invalid: {reason:?}")]
    InvalidProjectionGeometry {
        /// Why the geometry was rejected.
        reason: InvalidProjectionGeometryReason,
    },

    /// A depth interval is invalid (non-finite or `min > max`)
    /// (`SCC-PROJ-003`). `min == max` is legal and never rejected.
    #[error("invalid projection depth range [{min}, {max}]")]
    InvalidProjectionDepthRange {
        /// The rejected interval minimum (mm).
        min: f64,
        /// The rejected interval maximum (mm).
        max: f64,
    },

    /// A view bounds envelope is invalid (non-finite)
    /// (`SCC-PROJ-004`).
    #[error("invalid projection bounds {bounds:?}")]
    InvalidProjectionBounds {
        /// The rejected envelope.
        bounds: ProjectionBounds2D,
    },

    /// A primitive references a spatial element that does not exist in the
    /// referenced `SpatialModel` (`SCC-PROJ-005`). Fail-closed source
    /// integrity; never a synthetic fallback.
    #[error(
        "projection primitive {id} references a spatial element \
         missing from the spatial model"
    )]
    InvalidProjectionSource {
        /// The primitive whose source reference could not be resolved.
        id: ProjectionPrimitiveId,
    },

    /// A primitive id collides with an existing primitive in the same view
    /// (`SCC-PROJ-006`). Reachable because callers may supply arbitrary ids.
    #[error("projection primitive id {id} already exists in view {kind:?}")]
    DuplicateProjectionPrimitive {
        /// The colliding primitive id.
        id: ProjectionPrimitiveId,
        /// The view where the collision occurred.
        kind: PrincipalViewKind,
    },

    /// The input spatial model is malformed or internally inconsistent
    /// (`SCC-PROJ-007`). This is an additive Phase 9.4 preflight gate: the
    /// spatial schema is serde-decodable, so projection cannot assume that
    /// deserialization ran its constructors.
    #[error("spatial model is invalid: {reason:?}")]
    InvalidSpatialModel {
        /// The first deterministic spatial-model integrity failure.
        reason: InvalidSpatialModelReason,
    },
}

/// Why a projected primitive is invalid.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidProjectionGeometryReason {
    /// A coordinate (x/y/z/u/v/depth/radius) is not finite.
    NonFiniteCoordinate,
    /// A segment's start and end are identical (zero length is rejected).
    ZeroLengthSegment,
    /// A circle or arc radius is zero or negative.
    NonPositiveRadius,
    /// An arc sweep is zero, near zero, a full turn, or near a full turn.
    InvalidArcSweep,
    /// A deserialized frame does not match its canonical view kind.
    NonCanonicalFrame,
}

/// Why a [`scc_spatial::SpatialModel`] failed the Phase 9.4 preflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidSpatialModelReason {
    /// The body footprint is not the supported rectangular representation.
    NonRectangularBody,
    /// A body coordinate, dimension, or interval endpoint is not finite.
    NonFiniteBodyValue,
    /// A body width or height is not strictly positive.
    NonPositiveBodyDimension,
    /// The body interval is not finite, starts at zero, and strictly grows.
    InvalidBodyZInterval,
    /// Adding the rectangle origin and its dimensions overflowed.
    BodyCoordinateOverflow,
    /// The stored 3-D bounds are non-finite or non-strict.
    InvalidSpatialBounds,
    /// The stored bounds do not exactly witness the body footprint and Z
    /// interval.
    SpatialBoundsMismatch,
    /// A cut center, radius, or interval endpoint is not finite.
    NonFiniteCutValue,
    /// A cut radius is not strictly positive.
    NonPositiveCutRadius,
    /// A cut interval is empty or starts below the base plane.
    InvalidCutZInterval,
    /// A cut interval is not contained in the body Z interval.
    CutOutsideBodyZ,
}

impl ProjectionError {
    /// The stable `SCC-PROJ-{NNN}` code of this error.
    pub fn code(&self) -> &'static str {
        match self {
            ProjectionError::MissingProjectionView { .. } => "SCC-PROJ-001",
            ProjectionError::InvalidProjectionGeometry { .. } => "SCC-PROJ-002",
            ProjectionError::InvalidProjectionDepthRange { .. } => "SCC-PROJ-003",
            ProjectionError::InvalidProjectionBounds { .. } => "SCC-PROJ-004",
            ProjectionError::InvalidProjectionSource { .. } => "SCC-PROJ-005",
            ProjectionError::DuplicateProjectionPrimitive { .. } => "SCC-PROJ-006",
            ProjectionError::InvalidSpatialModel { .. } => "SCC-PROJ-007",
        }
    }
}

impl From<ProjectionError> for StructuredError {
    fn from(error: ProjectionError) -> Self {
        // Fail-closed by contract: severity Error, recoverable false, no
        // extra context (ADR-033 Decision 11 pattern).
        StructuredError::new(error.code(), ErrorStage::Projection, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> ProjectionPrimitiveId {
        ProjectionPrimitiveId(value.into())
    }

    fn all_variants() -> Vec<ProjectionError> {
        vec![
            ProjectionError::MissingProjectionView {
                kind: PrincipalViewKind::Front,
            },
            ProjectionError::InvalidProjectionGeometry {
                reason: InvalidProjectionGeometryReason::ZeroLengthSegment,
            },
            ProjectionError::InvalidProjectionDepthRange { min: 5.0, max: 1.0 },
            ProjectionError::InvalidProjectionBounds {
                bounds: ProjectionBounds2D {
                    min_u: 0.0,
                    max_u: 0.0,
                    min_v: 0.0,
                    max_v: 0.0,
                },
            },
            ProjectionError::InvalidProjectionSource {
                id: id("front:body:p:0000"),
            },
            ProjectionError::DuplicateProjectionPrimitive {
                id: id("front:body:p:0000"),
                kind: PrincipalViewKind::Top,
            },
            ProjectionError::InvalidSpatialModel {
                reason: InvalidSpatialModelReason::InvalidBodyZInterval,
            },
        ]
    }

    #[test]
    fn error_codes_are_distinct_sequential_and_complete() {
        let errors = all_variants();
        assert_eq!(errors.len(), 7, "every code must have a variant");
        for (index, error) in errors.iter().enumerate() {
            assert_eq!(
                error.code(),
                format!("SCC-PROJ-{:03}", index + 1),
                "variant {index} must carry the sequential code"
            );
        }
        let mut codes: Vec<&str> = errors.iter().map(|e| e.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 7, "codes must be distinct");
    }

    #[test]
    fn every_variant_passes_contract_and_converts() {
        for error in all_variants() {
            let structured: StructuredError = error.clone().into();
            assert_eq!(structured.code, error.code());
            assert_eq!(structured.stage, ErrorStage::Projection);
            assert_eq!(structured.severity, scc_core::error::ErrorSeverity::Error);
            assert!(!structured.recoverable, "projection errors are fail-closed");
            assert_eq!(structured.path, None);
            assert_eq!(structured.entity_ids, None);
            assert_eq!(structured.constraint_ids, None);
            assert_eq!(structured.source, None);
            assert_eq!(structured.suggestion, None);
            assert_eq!(structured.details, None);
            structured
                .validate_contract()
                .unwrap_or_else(|e| panic!("{} must pass contract: {e:?}", error.code()));
        }
    }

    #[test]
    fn reason_enums_roundtrip_through_serde() {
        for reason in [
            InvalidProjectionGeometryReason::NonFiniteCoordinate,
            InvalidProjectionGeometryReason::ZeroLengthSegment,
            InvalidProjectionGeometryReason::NonPositiveRadius,
            InvalidProjectionGeometryReason::InvalidArcSweep,
            InvalidProjectionGeometryReason::NonCanonicalFrame,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(
                serde_json::from_str::<InvalidProjectionGeometryReason>(&json).unwrap(),
                reason
            );
        }
    }
}
