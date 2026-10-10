//! Structured errors for the 2.5-D spatial lowering layer.
//!
//! Every [`SpatialError`] converts losslessly to an
//! [`scc_core::error::StructuredError`] carrying `SCC-SPAT-{NNN}` codes under
//! [`scc_core::error::ErrorStage::Spatial`], so the spatial layer participates
//! in the ADR-002 structured error contract without owning the core error
//! types (ADR-033 Decision 11).
//!
//! All spatial failures are fail-closed (`recoverable: false`): no spatial
//! model is ever partially assembled or silently repaired.

use scc_core::error::{ErrorStage, StructuredError};
use scc_core::ids::EntityId;
use scc_engineering::{FeatureId, SpatialIssue};
use scc_geometry::GeometryPrimitive;
use thiserror::Error;

/// Why a base profile could not become the spatial body.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidBaseReason {
    /// The base profile has no solved entity in geometry (or none mapped).
    NoSolvedEntity,
    /// The solved base primitive is not a Rectangle.
    NotRectangle,
    /// The solved base primitive is not a Circle.
    NotCircle,
    /// The base profile solved into more than one entity.
    NotUnique,
    /// A footprint coordinate or dimension is not finite.
    NonFiniteValue,
    /// A footprint dimension is zero or negative.
    NonPositiveValue,
    /// A new typed profile cannot be lowered until its engineering core
    /// representation is present.
    UnsupportedProfile,
}

/// Why a mapped entity could not be accepted as spatial source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidSpatialSourceReason {
    /// The mapped entity does not exist in the solved geometry.
    NotInGeometry,
    /// The entity is mapped under a different engineering feature.
    ForeignFeature,
}

/// Why a solved geometry primitive is not a valid spatial cut.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidGeometryReason {
    /// The primitive is not a `Circle`; carries the kind actually found.
    NotCircle { found: PrimitiveKind },
    /// A circle center coordinate is not finite.
    NonFiniteCenter,
    /// The circle radius is not finite.
    NonFiniteRadius,
    /// The circle radius is zero or negative.
    NonPositiveRadius,
    /// A through slot leaves or intersects the body outline.
    SlotOutsideBody,
    /// A cylindrical hole is outside or crosses the body outline.
    HoleOutsideBody,
}

/// The kind of a solved geometry primitive (fail-closed reporting only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    Point,
    Line,
    Circle,
    Arc,
    Rectangle,
    Polygon,
}

impl PrimitiveKind {
    /// The kind of a solved geometry primitive. Mirrors
    /// `scc_geometry::GeometryPrimitive` losslessly; a future variant there
    /// is a compile error here (fail-closed by construction).
    pub(crate) fn of(primitive: &GeometryPrimitive) -> Self {
        match primitive {
            GeometryPrimitive::Point { .. } => PrimitiveKind::Point,
            GeometryPrimitive::Line { .. } => PrimitiveKind::Line,
            GeometryPrimitive::Circle { .. } => PrimitiveKind::Circle,
            GeometryPrimitive::Arc { .. } => PrimitiveKind::Arc,
            GeometryPrimitive::Rectangle { .. } => PrimitiveKind::Rectangle,
            GeometryPrimitive::Polygon { .. } => PrimitiveKind::Polygon,
        }
    }
}

/// A failure while lowering engineering intent + solved geometry into a
/// [`crate::SpatialModel`] (Phase 9.2, ADR-033).
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SpatialError {
    /// The engineering model is not spatially ready (`SCC-SPAT-001`). The
    /// readiness issues are reported verbatim, deterministically ordered.
    #[error("{count} engineering spatial readiness issue(s) block lowering")]
    EngineeringNotSpatiallyReady {
        count: usize,
        issues: Vec<SpatialIssue>,
    },

    /// The model has no `BaseProfile` feature (`SCC-SPAT-002`).
    #[error("no BaseProfile feature exists; the spatial body has no representation")]
    MissingBaseRepresentation,

    /// The `BaseProfile` cannot become the spatial body (`SCC-SPAT-003`).
    #[error("base profile {feature} cannot become the spatial body: {reason:?}")]
    InvalidBaseRepresentation {
        feature: FeatureId,
        reason: InvalidBaseReason,
    },

    /// A `Hole` has no mapped solved entity (`SCC-SPAT-004`).
    #[error("hole {feature} has no mapped solved geometry entity")]
    MissingSpatialSource { feature: FeatureId },

    /// A mapped entity cannot be accepted as spatial source (`SCC-SPAT-005`).
    #[error("entity {entity} cannot be a spatial source of feature {feature}: {reason:?}")]
    InvalidSpatialSource {
        feature: FeatureId,
        entity: EntityId,
        reason: InvalidSpatialSourceReason,
    },

    /// A solved geometry primitive is not a valid spatial cut
    /// (`SCC-SPAT-006`).
    #[error("entity {entity} is not a valid spatial cut geometry: {reason:?}")]
    InvalidSpatialGeometry {
        entity: EntityId,
        reason: InvalidGeometryReason,
    },

    /// Engineering truth and solved geometry disagree on a length
    /// (`SCC-SPAT-007`).
    #[error(
        "witness mismatch for feature {feature} on entity {entity}: \
         engineering {engineering_value} mm vs geometry {geometry_value} mm"
    )]
    SpatialWitnessMismatch {
        feature: FeatureId,
        entity: EntityId,
        engineering_value: f64,
        geometry_value: f64,
    },

    /// A hole-bearing feature solved into a different number of instances
    /// than its intent declares (`SCC-SPAT-008`).
    #[error("feature {feature} declares {expected} hole(s) but {found} solved instance(s) exist")]
    PatternCardinalityMismatch {
        feature: FeatureId,
        expected: usize,
        found: usize,
    },

    /// A feature kind has no spatial meaning in this phase (`SCC-SPAT-009`).
    #[error("feature {feature} has no spatial meaning in this phase")]
    UnsupportedSpatialFeature { feature: FeatureId },

    /// A blind depth reached or exceeded the plate thickness while lowering
    /// (`SCC-SPAT-010`). Defensive: readiness normally rejects this earlier;
    /// the lowering never converts an over-deep blind hole into `ThroughAll`.
    #[error(
        "blind depth {depth} mm reaches or exceeds plate thickness {thickness} mm \
         for feature {feature}"
    )]
    InvalidBlindDepthDefensive {
        feature: FeatureId,
        depth: f64,
        thickness: f64,
    },

    /// The plate thickness is missing or unresolvable while lowering
    /// (`SCC-SPAT-011`). Defensive: readiness normally rejects this earlier.
    #[error("part thickness is missing or unresolvable; the spatial body cannot be built")]
    InvalidThicknessDefensive,

    /// A blind depth cannot be resolved to a finite positive length while
    /// lowering (`SCC-SPAT-012`). Defensive: readiness normally catches this.
    #[error("blind depth of feature {feature} cannot be resolved to a finite positive length")]
    InvalidBlindDepthResolutionDefensive { feature: FeatureId },

    /// A Z interval is invalid (non-finite, empty, or below the base plane)
    /// (`SCC-SPAT-013`).
    #[error("invalid Z interval [{z_min}, {z_max}]")]
    InvalidSpatialInterval { z_min: f64, z_max: f64 },

    /// A 3-D bounds envelope is invalid (`SCC-SPAT-014`).
    #[error("invalid spatial bounds min {min:?} / max {max:?}")]
    InvalidSpatialBounds { min: [f64; 3], max: [f64; 3] },

    /// A solved geometry entity is claimed by no spatial element
    /// (`SCC-SPAT-015`). Fail-closed claim integrity (ADR-033 Decision 6).
    #[error("geometry entity {entity} is claimed by no spatial element")]
    UnmappedGeometryEntity { entity: EntityId },

    /// The diameter of a hole-bearing feature cannot be resolved to a finite
    /// positive length while lowering (`SCC-SPAT-016`). Defensive: readiness
    /// checks extents and thickness but not diameters, so an unresolved
    /// bound diameter reaches this gate — never silently re-solved.
    #[error("diameter of feature {feature} cannot be resolved to a finite positive length")]
    InvalidHoleDiameterDefensive { feature: FeatureId },
}

impl SpatialError {
    /// The stable `SCC-SPAT-{NNN}` code of this error.
    pub fn code(&self) -> &'static str {
        match self {
            SpatialError::EngineeringNotSpatiallyReady { .. } => "SCC-SPAT-001",
            SpatialError::MissingBaseRepresentation => "SCC-SPAT-002",
            SpatialError::InvalidBaseRepresentation { .. } => "SCC-SPAT-003",
            SpatialError::MissingSpatialSource { .. } => "SCC-SPAT-004",
            SpatialError::InvalidSpatialSource { .. } => "SCC-SPAT-005",
            SpatialError::InvalidSpatialGeometry { .. } => "SCC-SPAT-006",
            SpatialError::SpatialWitnessMismatch { .. } => "SCC-SPAT-007",
            SpatialError::PatternCardinalityMismatch { .. } => "SCC-SPAT-008",
            SpatialError::UnsupportedSpatialFeature { .. } => "SCC-SPAT-009",
            SpatialError::InvalidBlindDepthDefensive { .. } => "SCC-SPAT-010",
            SpatialError::InvalidThicknessDefensive => "SCC-SPAT-011",
            SpatialError::InvalidBlindDepthResolutionDefensive { .. } => "SCC-SPAT-012",
            SpatialError::InvalidSpatialInterval { .. } => "SCC-SPAT-013",
            SpatialError::InvalidSpatialBounds { .. } => "SCC-SPAT-014",
            SpatialError::UnmappedGeometryEntity { .. } => "SCC-SPAT-015",
            SpatialError::InvalidHoleDiameterDefensive { .. } => "SCC-SPAT-016",
        }
    }
}

impl From<SpatialError> for StructuredError {
    fn from(error: SpatialError) -> Self {
        // Fail-closed by contract: severity Error, recoverable false, no
        // extra context (ADR-033 Decision 11).
        StructuredError::new(error.code(), ErrorStage::Spatial, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_engineering::FeatureId;

    fn fid(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    fn entity(id: &str) -> EntityId {
        EntityId(id.into())
    }

    fn all_variants() -> Vec<SpatialError> {
        vec![
            SpatialError::EngineeringNotSpatiallyReady {
                count: 1,
                issues: vec![SpatialIssue::MissingThickness],
            },
            SpatialError::MissingBaseRepresentation,
            SpatialError::InvalidBaseRepresentation {
                feature: fid("plate"),
                reason: InvalidBaseReason::NotRectangle,
            },
            SpatialError::MissingSpatialSource {
                feature: fid("hole"),
            },
            SpatialError::InvalidSpatialSource {
                feature: fid("hole"),
                entity: entity("hole"),
                reason: InvalidSpatialSourceReason::ForeignFeature,
            },
            SpatialError::InvalidSpatialGeometry {
                entity: entity("hole"),
                reason: InvalidGeometryReason::NonPositiveRadius,
            },
            SpatialError::SpatialWitnessMismatch {
                feature: fid("hole"),
                entity: entity("hole"),
                engineering_value: 10.0,
                geometry_value: 12.0,
            },
            SpatialError::PatternCardinalityMismatch {
                feature: fid("pattern"),
                expected: 8,
                found: 7,
            },
            SpatialError::UnsupportedSpatialFeature {
                feature: fid("slot"),
            },
            SpatialError::InvalidBlindDepthDefensive {
                feature: fid("hole"),
                depth: 10.0,
                thickness: 8.0,
            },
            SpatialError::InvalidThicknessDefensive,
            SpatialError::InvalidBlindDepthResolutionDefensive {
                feature: fid("hole"),
            },
            SpatialError::InvalidSpatialInterval {
                z_min: 5.0,
                z_max: 5.0,
            },
            SpatialError::InvalidSpatialBounds {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
            },
            SpatialError::UnmappedGeometryEntity {
                entity: entity("ghost"),
            },
            SpatialError::InvalidHoleDiameterDefensive {
                feature: fid("hole"),
            },
        ]
    }

    #[test]
    fn error_codes_are_distinct_sequential_and_complete() {
        let errors = all_variants();
        assert_eq!(errors.len(), 16, "every code must have a variant");
        for (index, error) in errors.iter().enumerate() {
            assert_eq!(
                error.code(),
                format!("SCC-SPAT-{:03}", index + 1),
                "variant {index} must carry the sequential code"
            );
        }
        let mut codes: Vec<&str> = errors.iter().map(|e| e.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 16, "codes must be distinct");
    }

    #[test]
    fn every_variant_passes_contract_and_converts() {
        for error in all_variants() {
            let structured: StructuredError = error.clone().into();
            assert_eq!(structured.code, error.code());
            assert_eq!(structured.stage, ErrorStage::Spatial);
            assert_eq!(structured.severity, scc_core::error::ErrorSeverity::Error);
            assert!(!structured.recoverable, "spatial errors are fail-closed");
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
            InvalidBaseReason::NoSolvedEntity,
            InvalidBaseReason::NotRectangle,
            InvalidBaseReason::NotUnique,
            InvalidBaseReason::NonFiniteValue,
            InvalidBaseReason::NonPositiveValue,
            InvalidBaseReason::UnsupportedProfile,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, serde_json::to_string(&reason).unwrap());
            assert_eq!(
                serde_json::from_str::<InvalidBaseReason>(&json).unwrap(),
                reason
            );
        }
        for reason in [
            InvalidSpatialSourceReason::NotInGeometry,
            InvalidSpatialSourceReason::ForeignFeature,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(
                serde_json::from_str::<InvalidSpatialSourceReason>(&json).unwrap(),
                reason
            );
        }
        for reason in [
            InvalidGeometryReason::NotCircle {
                found: PrimitiveKind::Rectangle,
            },
            InvalidGeometryReason::NonFiniteCenter,
            InvalidGeometryReason::NonFiniteRadius,
            InvalidGeometryReason::NonPositiveRadius,
            InvalidGeometryReason::SlotOutsideBody,
            InvalidGeometryReason::HoleOutsideBody,
            InvalidGeometryReason::HoleOutsideBody,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(
                serde_json::from_str::<InvalidGeometryReason>(&json).unwrap(),
                reason
            );
        }
    }

    #[test]
    fn primitive_kind_mirrors_geometry_primitives() {
        use scc_geometry::{GeometryPrimitive, Point2};
        let point = Point2 { x: 0.0, y: 0.0 };
        assert_eq!(
            PrimitiveKind::of(&GeometryPrimitive::Point { position: point }),
            PrimitiveKind::Point
        );
        assert_eq!(
            PrimitiveKind::of(&GeometryPrimitive::Line {
                start: point,
                end: point,
            }),
            PrimitiveKind::Line
        );
        assert_eq!(
            PrimitiveKind::of(&GeometryPrimitive::Circle {
                center: point,
                radius: 1.0,
            }),
            PrimitiveKind::Circle
        );
        assert_eq!(
            PrimitiveKind::of(&GeometryPrimitive::Arc {
                center: point,
                radius: 1.0,
                start_angle: 0.0,
                end_angle: 1.0,
            }),
            PrimitiveKind::Arc
        );
        assert_eq!(
            PrimitiveKind::of(&GeometryPrimitive::Rectangle {
                origin: point,
                width: 1.0,
                height: 1.0,
            }),
            PrimitiveKind::Rectangle
        );
        assert_eq!(
            PrimitiveKind::of(&GeometryPrimitive::Polygon {
                vertices: vec![point],
            }),
            PrimitiveKind::Polygon
        );
    }
}
