//! AICAD engineering-layer command protocol and design state.
//!
//! [`command::EngineeringCommand`] is the unified operation protocol that every
//! upper-layer entry point (DSL, tool calling, natural language, UI, mouse
//! editing) must eventually produce. [`model::EngineeringModel`] is the single
//! authoritative engineering state: it remembers one part, its features,
//! parameters, constraints, feature→parameter bindings, and drawing properties,
//! and deterministically resolves parameter expressions.
//!
//! This crate performs **no** geometry computation, **no** lowering to SCC IR,
//! and **no** mutation of runtime [`scc_core`] state. It depends only on
//! `scc-core` so that it stays upstream of the geometry and execution layers.

pub mod binding;
pub mod command;
pub mod context;
pub mod dependency;
pub mod error;
pub mod evaluator;
pub mod executor;
pub mod expression;
pub mod feature;
pub mod lowering;
pub mod model;
pub mod parameter;
pub mod parametric;
pub mod profile;
pub mod slot;
pub mod snapshot;
pub mod source_map;
pub mod spatial;
pub mod specification;
pub mod workbench;

pub use evaluator::ResolvedValue;
pub use feature::BaseProfile;
pub use model::{DrawingState, EngineeringModel, Parameter, ParameterOrigin, PartDefinition};
pub use parametric::{
    AxisReference, DatumAxisDefinition, DatumPlaneDefinition, EdgeReference, ExtrudeDirection,
    ExtrudeExtent, ExtrudeOperation, GeneratedFaceSelector, HoleKind, HoleTermination, LoftMode,
    LoftOperation, LoftOrientationPolicy, ParametricFeature, PatternDefinition, PatternPlacement,
    PlanarFaceQualifier, PlanarFaceSelector, PlaneReference, PrincipalAxis, PrincipalPlane,
    ProfileEdgeSide, RevolveAngle, RevolveOperation, RevolveOrientation, SemanticEdgeCurveKind,
    SemanticEdgeQualifier, SemanticEdgeSelector, SemanticGeneratedFaceQualifier,
    SemanticGeneratedFaceReference, SemanticPlanarFaceReference, SemanticSurfaceKind,
    SketchCurveGeometry, SketchEntity, SketchEntityEndpoint, SketchEntityId, SketchLoop,
    SketchProfile, SweepDirection, SweepOperation, SweepOrientation, Vector3D,
};
pub use profile::{ArcDirection, ProfileSegment, PROFILE_TOLERANCE};
pub use spatial::{
    HoleExtent, HoleStartFace, PartSpatialDefinition, SourcedLength, SpatialIssue, SpatialReadiness,
};
pub use specification::{
    DecimalPlaces, DimensionAxis, DimensionalSpecification, DimensionalTolerance,
    EngineeringDimensionRef,
};

use error::EngineeringError;
use serde::{Deserialize, Serialize};

/// Stable identifier for an engineering part.
///
/// IDs are deterministic user/upstream-supplied strings (per the
/// generic-feature-graph spec: `[a-z][a-z0-9_-]{0,63}`); they are never
/// auto-generated random UUIDs. The `String`-newtype pattern mirrors
/// [`scc_core::ids::EntityId`], but is a distinct type because a part is not a
/// 2-D sketch entity.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PartId(pub String);

/// Stable identifier for an engineering feature.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FeatureId(pub String);

/// Stable identifier for an engineering parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ParameterId(pub String);

/// Stable identifier for an engineering constraint.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EngineeringConstraintId(pub String);

impl std::fmt::Display for PartId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for FeatureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for ParameterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for EngineeringConstraintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A 2-D point in the engineering layer, expressed in millimetres.
///
/// Kept as a dedicated engineering value type rather than reusing
/// `scc_geometry::Point2` so that the engineering layer depends only on
/// `scc-core` and stays upstream of geometry (no reverse dependency).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    /// Construct a finite point.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Default for Point2D {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// Whether `id` satisfies the deterministic identifier rule
/// `[a-z][a-z0-9_-]{0,63}` (generic-feature-graph spec).
pub(crate) fn is_valid_identifier(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// A name must be non-empty and free of surrounding whitespace.
pub(crate) fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.trim() == name
}

/// Validate an identifier, distinguishing the empty case from other violations.
pub(crate) fn validate_identifier(what: &'static str, id: &str) -> Result<(), EngineeringError> {
    if id.is_empty() {
        return Err(EngineeringError::EmptyId { what });
    }
    if !is_valid_identifier(id) {
        return Err(EngineeringError::InvalidId {
            what,
            id: id.to_owned(),
        });
    }
    Ok(())
}

/// A point is only legal when both coordinates are finite.
pub(crate) fn finite_point(point: Point2D, what: &'static str) -> Result<(), EngineeringError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(EngineeringError::NonFiniteValue { what });
    }
    Ok(())
}

/// A length/dimension must be finite and strictly positive.
pub(crate) fn positive(what: &'static str, value: f64) -> Result<(), EngineeringError> {
    if !value.is_finite() {
        return Err(EngineeringError::NonFiniteValue { what });
    }
    if value <= 0.0 {
        return Err(EngineeringError::NonPositiveLength { what, value });
    }
    Ok(())
}

/// A pattern count must be at least one.
pub(crate) fn at_least_one(what: &'static str, count: u32) -> Result<(), EngineeringError> {
    if count == 0 {
        return Err(EngineeringError::ZeroCount { what, count });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_equality_ordering_and_display() {
        let a = FeatureId("a".into());
        let b = FeatureId("b".into());
        let a2 = FeatureId("a".into());
        assert_eq!(a, a2);
        assert_ne!(a, b);
        assert!(a < b);
        assert_eq!(a.to_string(), "a");
    }

    #[test]
    fn id_serde_roundtrip_is_plain_string() {
        let id = PartId("plate".into());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"plate\"");
        let back: PartId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn ids_are_deterministic_and_sorted_for_btreemap() {
        use std::collections::BTreeSet;
        let mut set = BTreeSet::new();
        set.insert(ParameterId("width".into()));
        set.insert(ParameterId("length".into()));
        let keys: Vec<_> = set.iter().map(|p| p.0.clone()).collect();
        assert_eq!(keys, vec!["length", "width"]);
    }

    #[test]
    fn identifier_rule_accepts_spec_valid_ids() {
        for id in ["plate", "center_hole", "hole-1", "a"] {
            assert!(is_valid_identifier(id), "expected {id} to be valid");
        }
    }

    #[test]
    fn identifier_rule_rejects_invalid_ids() {
        for id in ["", "Plate", "1plate", "has space", "a_b/c", &"a".repeat(65)] {
            assert!(!is_valid_identifier(id), "expected {id} to be rejected");
        }
    }

    #[test]
    fn validate_identifier_distinguishes_empty() {
        assert_eq!(
            validate_identifier("test id", "").unwrap_err(),
            EngineeringError::EmptyId { what: "test id" }
        );
        assert!(matches!(
            validate_identifier("test id", "BAD").unwrap_err(),
            EngineeringError::InvalidId { .. }
        ));
    }

    #[test]
    fn positive_rejects_non_finite_and_non_positive() {
        assert!(positive("d", 1.0).is_ok());
        assert!(matches!(
            positive("d", f64::NAN).unwrap_err(),
            EngineeringError::NonFiniteValue { .. }
        ));
        assert!(matches!(
            positive("d", f64::INFINITY).unwrap_err(),
            EngineeringError::NonFiniteValue { .. }
        ));
        assert!(matches!(
            positive("d", -10.0).unwrap_err(),
            EngineeringError::NonPositiveLength { .. }
        ));
        assert!(positive("d", 0.0).is_err());
    }

    #[test]
    fn point2d_must_be_finite() {
        assert!(finite_point(Point2D::new(0.0, 0.0), "move position").is_ok());
        assert!(finite_point(Point2D::new(f64::NAN, 0.0), "move position").is_err());
        assert!(finite_point(Point2D::new(0.0, f64::INFINITY), "move position").is_err());
    }
}
