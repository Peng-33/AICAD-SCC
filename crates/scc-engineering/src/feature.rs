//! Engineering feature, hole pattern, constraint, and drawing-property
//! definitions.
//!
//! These are *intent* types only: they describe what a feature is, never how
//! it is lowered to geometry. No coordinate computation, array expansion,
//! constraint solving, projection, or drawing happens in this crate.

use serde::{Deserialize, Serialize};

use crate::error::EngineeringError;
use crate::{EngineeringConstraintId, FeatureId, Point2D};

pub use crate::parametric::{
    AxisReference, DatumAxisDefinition, DatumPlaneDefinition, EdgeReference, ExtrudeDirection,
    ExtrudeExtent, ExtrudeOperation, GeneratedFaceSelector, HoleKind, HoleTermination,
    ParametricFeature, PatternDefinition, PatternPlacement, PlanarFaceQualifier,
    PlanarFaceSelector, PlaneReference, PrincipalAxis, PrincipalPlane, ProfileEdgeSide,
    RevolveAngle, RevolveOperation, RevolveOrientation, SemanticEdgeCurveKind,
    SemanticEdgeQualifier, SemanticEdgeSelector, SemanticGeneratedFaceQualifier,
    SemanticGeneratedFaceReference, SemanticPlanarFaceReference, SemanticSurfaceKind,
    SketchCurveGeometry, SketchEntity, SketchEntityEndpoint, SketchEntityId, SketchLoop,
    SketchProfile, Vector3D,
};
pub use crate::profile::{ArcDirection, BaseProfile, ProfileSegment};

/// A feature in an engineering design.
///
/// Phase 1 intentionally implements a small subset (base profile, holes,
/// pattern, slot, fillet, construction circle). The enum is extensible: later
/// phases add variants without changing existing ones. Each feature carries a
/// stable, deterministic [`FeatureId`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// Parametric stores the ordered feature payload inline. Boxing that variant
// would change the public model type only to satisfy a size lint.
#[allow(clippy::large_enum_variant)]
pub enum EngineeringFeature {
    /// The closed base profile of a part with nominal width and height (mm).
    BaseProfile {
        id: FeatureId,
        name: String,
        #[serde(flatten)]
        profile: BaseProfile,
    },
    /// A single round hole.
    Hole { id: FeatureId, diameter: f64 },
    /// A repeated pattern of holes expressed parametrically (never expanded).
    HolePattern { id: FeatureId, pattern: HolePattern },
    /// A through oblong slot (material-removal cut, never a base profile).
    ///
    /// `length` is overall end-to-end length, not center-distance.
    Slot {
        id: FeatureId,
        #[serde(default)]
        center: Point2D,
        /// Overall length (end to end).
        length: f64,
        width: f64,
        /// Canonicalized to `[0, 180)` on ingest.
        #[serde(default)]
        orientation_degrees: f64,
    },
    /// A radiused edge treatment.
    Fillet { id: FeatureId, radius: f64 },
    /// Reference-only circle (not material geometry).
    ConstructionCircle { id: FeatureId, diameter: f64 },
    /// A node in the generic parametric feature history. Its stable ID is
    /// shared with the existing feature map; the optional model history only
    /// supplies execution order. Spatial/manufacturing lowerings must reject
    /// unsupported nodes rather than silently ignoring them.
    Parametric {
        id: FeatureId,
        name: String,
        feature: ParametricFeature,
        #[serde(default)]
        suppressed: bool,
    },
}

impl EngineeringFeature {
    /// The stable identifier of this feature.
    pub fn id(&self) -> &FeatureId {
        match self {
            EngineeringFeature::BaseProfile { id, .. }
            | EngineeringFeature::Hole { id, .. }
            | EngineeringFeature::HolePattern { id, .. }
            | EngineeringFeature::Slot { id, .. }
            | EngineeringFeature::Fillet { id, .. }
            | EngineeringFeature::ConstructionCircle { id, .. }
            | EngineeringFeature::Parametric { id, .. } => id,
        }
    }

    /// Command-level validation of this feature's own data.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        crate::validate_identifier("feature id", &self.id().0)?;
        match self {
            EngineeringFeature::BaseProfile { name, profile, .. } => {
                if !crate::is_valid_name(name) {
                    return Err(EngineeringError::EmptyName);
                }
                match profile {
                    BaseProfile::Rectangle { width, height } => {
                        crate::positive("base profile width", *width)?;
                        crate::positive("base profile height", *height)?;
                    }
                    BaseProfile::Circle { center, diameter } => {
                        crate::finite_point(*center, "circular base profile center")?;
                        crate::positive("circular base profile diameter", *diameter)?;
                    }
                    BaseProfile::RoundedRectangle { .. }
                    | BaseProfile::Composite { .. }
                    | BaseProfile::Polygon { .. } => {
                        profile.validate()?;
                    }
                }
            }
            EngineeringFeature::Hole { diameter, .. } => {
                crate::positive("hole diameter", *diameter)?;
            }
            EngineeringFeature::HolePattern { pattern, .. } => pattern.validate()?,
            EngineeringFeature::Slot {
                center,
                length,
                width,
                orientation_degrees,
                ..
            } => {
                crate::finite_point(*center, "slot center")?;
                crate::slot::validate_slot_dimensions(*length, *width)?;
                crate::slot::canonicalize_orientation_degrees(*orientation_degrees)?;
            }
            EngineeringFeature::Fillet { radius, .. } => {
                crate::positive("fillet radius", *radius)?;
            }
            EngineeringFeature::ConstructionCircle { diameter, .. } => {
                crate::positive("construction circle diameter", *diameter)?;
            }
            EngineeringFeature::Parametric { name, feature, .. } => {
                if !crate::is_valid_name(name) {
                    return Err(EngineeringError::EmptyName);
                }
                feature.validate()?;
            }
        }
        Ok(())
    }
}

/// A hole pattern expressed as a **parametric pattern**, never as an array of
/// holes. This is a hard architectural requirement: lowering (Phase 3) expands
/// the pattern into concrete hole positions; this layer keeps it symbolic so
/// the design stays editable and deterministic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HolePattern {
    /// Holes evenly distributed on a pitch circle diameter (PCD).
    Circular {
        center: Point2D,
        /// Diameter of the circle passing through every hole center (PCD).
        pitch_circle_diameter: f64,
        count: u32,
        /// Angular offset of the first hole from the +X axis (degrees).
        start_angle: f64,
        hole: HoleDefinition,
    },
    /// Holes laid out on a rectangular grid.
    Grid {
        rows: u32,
        columns: u32,
        spacing_x: f64,
        spacing_y: f64,
        hole: HoleDefinition,
    },
    /// A single hole positioned by corner offsets.
    Corner {
        offset_x: f64,
        offset_y: f64,
        hole: HoleDefinition,
    },
}

/// The repeating hole definition shared by every pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoleDefinition {
    pub diameter: f64,
}

impl HolePattern {
    /// Command-level validation of the pattern parameters.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        match self {
            HolePattern::Circular {
                center,
                pitch_circle_diameter,
                count,
                start_angle,
                hole,
            } => {
                crate::finite_point(*center, "hole pattern center")?;
                crate::positive("pitch circle diameter", *pitch_circle_diameter)?;
                crate::at_least_one("circular hole pattern count", *count)?;
                if !start_angle.is_finite() {
                    return Err(EngineeringError::NonFiniteValue {
                        what: "hole pattern start angle",
                    });
                }
                hole.validate()?;
            }
            HolePattern::Grid {
                rows,
                columns,
                spacing_x,
                spacing_y,
                hole,
            } => {
                crate::at_least_one("grid row count", *rows)?;
                crate::at_least_one("grid column count", *columns)?;
                crate::positive("grid x spacing", *spacing_x)?;
                crate::positive("grid y spacing", *spacing_y)?;
                hole.validate()?;
            }
            HolePattern::Corner {
                offset_x,
                offset_y,
                hole,
            } => {
                if !offset_x.is_finite() || !offset_y.is_finite() {
                    return Err(EngineeringError::NonFiniteValue {
                        what: "corner hole offset",
                    });
                }
                hole.validate()?;
            }
        }
        Ok(())
    }
}

impl HoleDefinition {
    /// Command-level validation of the repeating hole.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        crate::positive("hole diameter", self.diameter)
    }
}

/// A high-level engineering constraint between features.
///
/// This layer expresses *intent* (for example "these two holes are
/// concentric"). The mapping to [`scc_core::constraint::ConstraintKind`]
/// happens during lowering (Phase 3); this crate deliberately does not reuse
/// the solver vocabulary, because engineering constraints target features
/// rather than 2-D entities and include concepts (e.g. concentricity) with no
/// direct solver equivalent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineeringConstraint {
    /// `source` is symmetric to `target` about a shared axis.
    Symmetric {
        id: EngineeringConstraintId,
        source: FeatureId,
        target: FeatureId,
    },
    /// `source` is aligned with `target`.
    Aligned {
        id: EngineeringConstraintId,
        source: FeatureId,
        target: FeatureId,
    },
    /// `source` is a fixed distance from `target`.
    Distance {
        id: EngineeringConstraintId,
        source: FeatureId,
        target: FeatureId,
        value: f64,
    },
    /// `source` is tangent to `target`.
    Tangent {
        id: EngineeringConstraintId,
        source: FeatureId,
        target: FeatureId,
    },
    /// `source` and `target` share the same centre.
    Concentric {
        id: EngineeringConstraintId,
        source: FeatureId,
        target: FeatureId,
    },
    /// Unary constraint: `target` is pinned/fixed in place.
    Fixed {
        id: EngineeringConstraintId,
        target: FeatureId,
    },
}

impl EngineeringConstraint {
    /// The stable identifier of this constraint.
    pub fn id(&self) -> &EngineeringConstraintId {
        match self {
            EngineeringConstraint::Symmetric { id, .. }
            | EngineeringConstraint::Aligned { id, .. }
            | EngineeringConstraint::Distance { id, .. }
            | EngineeringConstraint::Tangent { id, .. }
            | EngineeringConstraint::Concentric { id, .. }
            | EngineeringConstraint::Fixed { id, .. } => id,
        }
    }

    /// Command-level validation of the constraint's own data.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        crate::validate_identifier("constraint id", &self.id().0)?;
        match self {
            EngineeringConstraint::Symmetric { source, target, .. }
            | EngineeringConstraint::Aligned { source, target, .. }
            | EngineeringConstraint::Distance { source, target, .. }
            | EngineeringConstraint::Tangent { source, target, .. }
            | EngineeringConstraint::Concentric { source, target, .. } => {
                crate::validate_identifier("constraint source feature id", &source.0)?;
                crate::validate_identifier("constraint target feature id", &target.0)?;
            }
            EngineeringConstraint::Fixed { target, .. } => {
                crate::validate_identifier("constraint target feature id", &target.0)?;
            }
        }
        if let EngineeringConstraint::Distance { value, .. } = self {
            crate::positive("constraint distance", *value)?;
        }
        Ok(())
    }

    /// The feature ids this constraint references, in (source, target) order
    /// (for `Fixed`, the single `target`).
    pub fn referenced_features(&self) -> Vec<&FeatureId> {
        match self {
            EngineeringConstraint::Symmetric { source, target, .. }
            | EngineeringConstraint::Aligned { source, target, .. }
            | EngineeringConstraint::Distance { source, target, .. }
            | EngineeringConstraint::Tangent { source, target, .. }
            | EngineeringConstraint::Concentric { source, target, .. } => {
                vec![source, target]
            }
            EngineeringConstraint::Fixed { target, .. } => vec![target],
        }
    }

    /// Whether this constraint references the given feature.
    pub fn references_feature(&self, feature: &FeatureId) -> bool {
        self.referenced_features().contains(&feature)
    }
}

/// A drawing presentation property. Data-only: no projection, dimension
/// layout, or SVG rendering happens in this layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DrawingProperty {
    /// Drawing scale (e.g. `2.0` = double size).
    Scale { value: f64 },
    /// Standard projection convention.
    ProjectionMethod { method: ProjectionMethod },
    /// Whether centerlines are shown.
    ShowCenterlines { enabled: bool },
    /// Whether construction geometry is shown.
    ShowConstructionGeometry { enabled: bool },
}

/// Standard projection convention for a 2-D drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMethod {
    FirstAngle,
    ThirdAngle,
}

/// Stable kind of a drawing property, used as the ordered key of the drawing
/// state so at most one property of each kind is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingPropertyKind {
    Scale,
    ProjectionMethod,
    ShowCenterlines,
    ShowConstructionGeometry,
}

impl DrawingProperty {
    /// Command-level validation of the property value.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        match self {
            DrawingProperty::Scale { value } => crate::positive("drawing scale", *value),
            DrawingProperty::ProjectionMethod { .. }
            | DrawingProperty::ShowCenterlines { .. }
            | DrawingProperty::ShowConstructionGeometry { .. } => Ok(()),
        }
    }

    /// The stable kind of this property.
    pub fn kind(&self) -> DrawingPropertyKind {
        match self {
            DrawingProperty::Scale { .. } => DrawingPropertyKind::Scale,
            DrawingProperty::ProjectionMethod { .. } => DrawingPropertyKind::ProjectionMethod,
            DrawingProperty::ShowCenterlines { .. } => DrawingPropertyKind::ShowCenterlines,
            DrawingProperty::ShowConstructionGeometry { .. } => {
                DrawingPropertyKind::ShowConstructionGeometry
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature_id(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    #[test]
    fn hole_pattern_is_a_pattern_not_an_array() {
        let pattern = HolePattern::Circular {
            center: Point2D::new(0.0, 0.0),
            pitch_circle_diameter: 126.0,
            count: 8,
            start_angle: 0.0,
            hole: HoleDefinition { diameter: 10.0 },
        };
        assert!(pattern.validate().is_ok());

        // The representation must be parametric, never a Vec<Hole>.
        let json = serde_json::to_string(&pattern).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "circular");
        assert_eq!(value["count"], 8);
        assert_eq!(value["pitch_circle_diameter"], 126.0);
        assert_eq!(value["hole"]["diameter"], 10.0);
        assert!(value.get("holes").is_none());
    }

    #[test]
    fn circular_pattern_zero_count_is_rejected() {
        let pattern = HolePattern::Circular {
            center: Point2D::new(0.0, 0.0),
            pitch_circle_diameter: 126.0,
            count: 0,
            start_angle: 0.0,
            hole: HoleDefinition { diameter: 10.0 },
        };
        assert!(matches!(
            pattern.validate().unwrap_err(),
            EngineeringError::ZeroCount { .. }
        ));
    }

    #[test]
    fn pattern_negative_diameter_is_rejected() {
        let pattern = HolePattern::Circular {
            center: Point2D::new(0.0, 0.0),
            pitch_circle_diameter: 126.0,
            count: 8,
            start_angle: 0.0,
            hole: HoleDefinition { diameter: -10.0 },
        };
        assert!(matches!(
            pattern.validate().unwrap_err(),
            EngineeringError::NonPositiveLength { .. }
        ));
    }

    #[test]
    fn grid_pattern_validates_and_roundtrips() {
        let pattern = HolePattern::Grid {
            rows: 2,
            columns: 3,
            spacing_x: 40.0,
            spacing_y: 40.0,
            hole: HoleDefinition { diameter: 6.0 },
        };
        assert!(pattern.validate().is_ok());
        let back: HolePattern =
            serde_json::from_str(&serde_json::to_string(&pattern).unwrap()).unwrap();
        assert_eq!(pattern, back);
        assert!(matches!(pattern, HolePattern::Grid { .. }));
    }

    #[test]
    fn grid_pattern_zero_rows_is_rejected() {
        let pattern = HolePattern::Grid {
            rows: 0,
            columns: 3,
            spacing_x: 40.0,
            spacing_y: 40.0,
            hole: HoleDefinition { diameter: 6.0 },
        };
        assert!(pattern.validate().is_err());
    }

    #[test]
    fn corner_pattern_offsets_are_finite_only() {
        let pattern = HolePattern::Corner {
            offset_x: 15.0,
            offset_y: 15.0,
            hole: HoleDefinition { diameter: 5.0 },
        };
        assert!(pattern.validate().is_ok());

        let nan = HolePattern::Corner {
            offset_x: f64::NAN,
            offset_y: 15.0,
            hole: HoleDefinition { diameter: 5.0 },
        };
        assert!(nan.validate().is_err());
    }

    #[test]
    fn feature_construction_and_validation() {
        let cases: Vec<(EngineeringFeature, bool)> = vec![
            (
                EngineeringFeature::BaseProfile {
                    id: feature_id("plate"),
                    name: "plate".into(),
                    profile: BaseProfile::Rectangle {
                        width: 80.0,
                        height: 80.0,
                    },
                },
                true,
            ),
            (
                EngineeringFeature::Hole {
                    id: feature_id("center_hole"),
                    diameter: 100.0,
                },
                true,
            ),
            (
                EngineeringFeature::HolePattern {
                    id: feature_id("bolt_circle"),
                    pattern: HolePattern::Circular {
                        center: Point2D::new(0.0, 0.0),
                        pitch_circle_diameter: 126.0,
                        count: 8,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
                true,
            ),
            (
                EngineeringFeature::Slot {
                    id: feature_id("slot_1"),
                    center: Point2D::new(0.0, 0.0),
                    length: 30.0,
                    width: 8.0,
                    orientation_degrees: 0.0,
                },
                true,
            ),
            (
                EngineeringFeature::Fillet {
                    id: feature_id("f1"),
                    radius: 5.0,
                },
                true,
            ),
            (
                EngineeringFeature::ConstructionCircle {
                    id: feature_id("cc_1"),
                    diameter: 60.0,
                },
                true,
            ),
        ];
        for (feature, expect_ok) in cases {
            assert_eq!(
                feature.validate().is_ok(),
                expect_ok,
                "feature: {feature:?}"
            );
            // Every feature exposes its stable id.
            assert!(!feature.id().0.is_empty());
        }
    }

    #[test]
    fn feature_invalid_id_is_rejected() {
        let feature = EngineeringFeature::Hole {
            id: feature_id(""),
            diameter: 10.0,
        };
        assert!(matches!(
            feature.validate().unwrap_err(),
            EngineeringError::EmptyId { .. }
        ));
    }

    #[test]
    fn feature_negative_hole_diameter_is_rejected() {
        let feature = EngineeringFeature::Hole {
            id: feature_id("h1"),
            diameter: -10.0,
        };
        assert!(matches!(
            feature.validate().unwrap_err(),
            EngineeringError::NonPositiveLength { .. }
        ));
    }

    #[test]
    fn feature_nan_diameter_is_rejected() {
        let feature = EngineeringFeature::Hole {
            id: feature_id("h1"),
            diameter: f64::NAN,
        };
        assert!(feature.validate().is_err());
    }

    #[test]
    fn base_profile_empty_name_is_rejected() {
        let feature = EngineeringFeature::BaseProfile {
            id: feature_id("plate"),
            name: "  ".into(),
            profile: BaseProfile::Rectangle {
                width: 80.0,
                height: 80.0,
            },
        };
        assert!(matches!(
            feature.validate().unwrap_err(),
            EngineeringError::EmptyName
        ));
    }

    #[test]
    fn engineering_constraint_validates_and_roundtrips() {
        let constraints = vec![
            EngineeringConstraint::Concentric {
                id: EngineeringConstraintId("conc_1".into()),
                source: feature_id("center_hole"),
                target: feature_id("bolt_circle"),
            },
            EngineeringConstraint::Distance {
                id: EngineeringConstraintId("dist_1".into()),
                source: feature_id("hole_a"),
                target: feature_id("hole_b"),
                value: 42.0,
            },
            EngineeringConstraint::Fixed {
                id: EngineeringConstraintId("fix_1".into()),
                target: feature_id("plate"),
            },
            EngineeringConstraint::Symmetric {
                id: EngineeringConstraintId("sym_1".into()),
                source: feature_id("hole_a"),
                target: feature_id("hole_b"),
            },
            EngineeringConstraint::Aligned {
                id: EngineeringConstraintId("aln_1".into()),
                source: feature_id("hole_a"),
                target: feature_id("hole_b"),
            },
            EngineeringConstraint::Tangent {
                id: EngineeringConstraintId("tan_1".into()),
                source: feature_id("circle_a"),
                target: feature_id("line_a"),
            },
        ];
        for constraint in constraints {
            assert!(constraint.validate().is_ok());
            let back: EngineeringConstraint =
                serde_json::from_str(&serde_json::to_string(&constraint).unwrap()).unwrap();
            assert_eq!(constraint, back);
            assert!(!constraint.id().0.is_empty());
        }
    }

    #[test]
    fn constraint_negative_distance_is_rejected() {
        let constraint = EngineeringConstraint::Distance {
            id: EngineeringConstraintId("dist_1".into()),
            source: feature_id("a"),
            target: feature_id("b"),
            value: -1.0,
        };
        assert!(constraint.validate().is_err());
    }

    #[test]
    fn constraint_serializes_as_tagged_op() {
        let constraint = EngineeringConstraint::Distance {
            id: EngineeringConstraintId("dist_1".into()),
            source: feature_id("hole_a"),
            target: feature_id("hole_b"),
            value: 42.0,
        };
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&constraint).unwrap()).unwrap();
        assert_eq!(value["type"], "distance");
        assert_eq!(value["value"], 42.0);
    }

    #[test]
    fn drawing_property_validates_and_roundtrips() {
        let properties = vec![
            DrawingProperty::Scale { value: 2.0 },
            DrawingProperty::ProjectionMethod {
                method: ProjectionMethod::ThirdAngle,
            },
            DrawingProperty::ShowCenterlines { enabled: true },
            DrawingProperty::ShowConstructionGeometry { enabled: false },
        ];
        for property in properties {
            assert!(property.validate().is_ok());
            let back: DrawingProperty =
                serde_json::from_str(&serde_json::to_string(&property).unwrap()).unwrap();
            assert_eq!(property, back);
        }

        let scale: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&DrawingProperty::Scale { value: 2.0 }).unwrap(),
        )
        .unwrap();
        assert_eq!(scale["type"], "scale");
    }

    #[test]
    fn drawing_property_non_positive_scale_is_rejected() {
        assert!(DrawingProperty::Scale { value: 0.0 }.validate().is_err());
        assert!(DrawingProperty::Scale { value: -1.0 }.validate().is_err());
        assert!(DrawingProperty::Scale { value: f64::NAN }
            .validate()
            .is_err());
    }

    #[test]
    fn projection_method_serializes_as_snake_case() {
        let json = serde_json::to_string(&ProjectionMethod::FirstAngle).unwrap();
        assert_eq!(json, "\"first_angle\"");
    }
}
