//! Typed engineering dimensional specifications (Phase 11.5).
//!
//! A dimensional specification is engineering source truth: it describes the
//! manufacturing requirement attached to one typed nominal dimension. It is
//! deliberately separate from feature geometry, solved spatial state, and
//! drawing presentation. Drawing consumers receive a validated derived copy;
//! they never own or mutate this data.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::binding::FeatureParameterSlot;
use crate::feature::EngineeringFeature;
use crate::model::EngineeringModel;
use crate::spatial::HoleExtent;
use crate::{EngineeringError, FeatureId};

/// Axis used by derived hole-location specifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionAxis {
    X,
    Y,
}

/// A typed nominal dimension that may carry a tolerance and/or precision.
///
/// Feature slots cover dimensions already represented by the engineering
/// feature model (width, height, diameter, spacing, PCD, and offsets). The
/// remaining dimensions are derived from typed engineering/spatial semantics.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineeringDimensionRef {
    FeatureSlot {
        feature: FeatureId,
        slot: FeatureParameterSlot,
    },
    PartThickness,
    HoleLocation {
        feature: FeatureId,
        axis: DimensionAxis,
    },
    HoleDepth {
        feature: FeatureId,
    },
}

impl EngineeringDimensionRef {
    /// Validate identifier syntax without requiring the referenced model to be
    /// present. This is the command-level gate.
    pub fn validate_identifiers(&self) -> Result<(), EngineeringError> {
        match self {
            Self::FeatureSlot { feature, .. }
            | Self::HoleLocation { feature, .. }
            | Self::HoleDepth { feature } => {
                crate::validate_identifier("dimension feature id", &feature.0)
            }
            Self::PartThickness => Ok(()),
        }
    }

    /// Validate that the typed target is meaningful for this model.
    pub fn validate_for_model(&self, model: &EngineeringModel) -> Result<(), EngineeringError> {
        self.validate_identifiers()?;
        match self {
            Self::PartThickness => Ok(()),
            Self::HoleLocation { feature, .. } => {
                let Some(target) = model.features.get(feature) else {
                    return Err(EngineeringError::FeatureNotFound {
                        feature_id: feature.clone(),
                    });
                };
                if matches!(
                    target,
                    EngineeringFeature::Hole { .. } | EngineeringFeature::HolePattern { .. }
                ) {
                    Ok(())
                } else {
                    Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!("{} is only valid for a hole or hole pattern", self),
                    })
                }
            }
            Self::HoleDepth { feature } => {
                let Some(target) = model.features.get(feature) else {
                    return Err(EngineeringError::FeatureNotFound {
                        feature_id: feature.clone(),
                    });
                };
                if !matches!(
                    target,
                    EngineeringFeature::Hole { .. } | EngineeringFeature::HolePattern { .. }
                ) {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!("{} is only valid for a hole or hole pattern", self),
                    });
                }
                match model.spatial.hole_extents.get(feature) {
                    Some(HoleExtent::Blind { .. }) => Ok(()),
                    Some(HoleExtent::ThroughAll) => {
                        Err(EngineeringError::InvalidDimensionalSpecification {
                            reason: format!(
                                "{} cannot carry a depth tolerance for a ThroughAll hole",
                                self
                            ),
                        })
                    }
                    None => Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!("{} requires a declared blind HoleExtent", self),
                    }),
                }
            }
            Self::FeatureSlot { feature, slot } => {
                let Some(target) = model.features.get(feature) else {
                    return Err(EngineeringError::FeatureNotFound {
                        feature_id: feature.clone(),
                    });
                };
                if !FeatureParameterSlot::valid_for(target).contains(slot) {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!("slot {} is not valid for feature {}", slot, feature),
                    });
                }
                if matches!(
                    slot,
                    FeatureParameterSlot::Count
                        | FeatureParameterSlot::Rows
                        | FeatureParameterSlot::Columns
                ) {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!(
                            "{} is an integer quantity and cannot carry tolerance or precision",
                            self
                        ),
                    });
                }
                if matches!(
                    slot,
                    FeatureParameterSlot::CenterX
                        | FeatureParameterSlot::CenterY
                        | FeatureParameterSlot::StartAngle
                        | FeatureParameterSlot::Orientation
                ) {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!(
                            "{} is not a supported dimensional annotation target",
                            self
                        ),
                    });
                }
                Ok(())
            }
        }
    }

    /// Whether the target belongs to a feature, used for cleanup on removal.
    pub fn references_feature(&self, feature: &FeatureId) -> bool {
        match self {
            Self::FeatureSlot {
                feature: target, ..
            }
            | Self::HoleLocation {
                feature: target, ..
            }
            | Self::HoleDepth { feature: target } => target == feature,
            Self::PartThickness => false,
        }
    }
}

impl fmt::Display for EngineeringDimensionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FeatureSlot { feature, slot } => write!(f, "feature {} slot {}", feature, slot),
            Self::PartThickness => f.write_str("part thickness"),
            Self::HoleLocation { feature, axis } => {
                write!(f, "hole feature {} location {:?}", feature, axis)
            }
            Self::HoleDepth { feature } => write!(f, "hole feature {} depth", feature),
        }
    }
}

/// Typed tolerance attached to a nominal engineering dimension.
///
/// Bilateral values are signed deviations: `upper >= 0` and `lower <= 0`.
/// Zero-valued symmetric/bilateral input is accepted at the API boundary and
/// canonicalized to [`DimensionalTolerance::None`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DimensionalTolerance {
    #[default]
    None,
    Symmetric {
        plus_minus: f64,
    },
    Bilateral {
        upper: f64,
        lower: f64,
    },
}

impl DimensionalTolerance {
    /// Construct and canonicalize a symmetric tolerance.
    pub fn symmetric(plus_minus: f64) -> Result<Self, EngineeringError> {
        Self::Symmetric { plus_minus }.canonicalize()
    }

    /// Construct and canonicalize a signed bilateral tolerance.
    pub fn bilateral(upper: f64, lower: f64) -> Result<Self, EngineeringError> {
        Self::Bilateral { upper, lower }.canonicalize()
    }

    /// Validate raw shape/sign/finite constraints.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        match self {
            Self::None => Ok(()),
            Self::Symmetric { plus_minus } => {
                if !plus_minus.is_finite() || *plus_minus < 0.0 {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!(
                            "symmetric tolerance must be finite and non-negative, got {}",
                            plus_minus
                        ),
                    });
                }
                Ok(())
            }
            Self::Bilateral { upper, lower } => {
                if !upper.is_finite() || !lower.is_finite() {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: "bilateral tolerance must be finite".into(),
                    });
                }
                if *upper < 0.0 || *lower > 0.0 {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!("bilateral tolerance uses signed deviations with upper >= 0 and lower <= 0, got {}/{}", upper, lower),
                    });
                }
                if upper < lower {
                    return Err(EngineeringError::InvalidDimensionalSpecification {
                        reason: format!(
                            "bilateral tolerance upper {} is below lower {}",
                            upper, lower
                        ),
                    });
                }
                Ok(())
            }
        }
    }

    /// Canonicalize zero tolerance to `None`.
    pub fn canonicalize(self) -> Result<Self, EngineeringError> {
        self.validate()?;
        Ok(match self {
            Self::Symmetric { plus_minus: 0.0 } => Self::None,
            Self::Bilateral {
                upper: 0.0,
                lower: 0.0,
            } => Self::None,
            other => other,
        })
    }

    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

/// Fixed decimal precision for engineering presentation.
///
/// Significant figures, scientific notation, and fractional inches are not
/// part of Phase 11.5. The allowed range is deliberately `0..=4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DecimalPlaces(pub u8);

impl DecimalPlaces {
    pub const MAX: u8 = 4;

    pub fn new(value: u8) -> Result<Self, EngineeringError> {
        let places = Self(value);
        places.validate()?;
        Ok(places)
    }

    pub fn validate(self) -> Result<(), EngineeringError> {
        if self.0 <= Self::MAX {
            Ok(())
        } else {
            Err(EngineeringError::InvalidDimensionalSpecification {
                reason: format!(
                    "decimal places must be between 0 and {}, got {}",
                    Self::MAX,
                    self.0
                ),
            })
        }
    }
}

/// The complete typed specification for one nominal dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionalSpecification {
    pub tolerance: DimensionalTolerance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<DecimalPlaces>,
}

impl Default for DimensionalSpecification {
    fn default() -> Self {
        Self {
            tolerance: DimensionalTolerance::None,
            precision: None,
        }
    }
}

impl DimensionalSpecification {
    pub fn new(
        tolerance: DimensionalTolerance,
        precision: Option<DecimalPlaces>,
    ) -> Result<Self, EngineeringError> {
        let specification = Self {
            tolerance,
            precision,
        };
        specification.canonicalize()
    }

    pub fn validate(&self) -> Result<(), EngineeringError> {
        self.tolerance.validate()?;
        if let Some(precision) = self.precision {
            precision.validate()?;
        }
        Ok(())
    }

    /// Validate then apply the zero-tolerance canonical form.
    pub fn canonicalize(self) -> Result<Self, EngineeringError> {
        let tolerance = self.tolerance.canonicalize()?;
        if let Some(precision) = self.precision {
            precision.validate()?;
        }
        Ok(Self {
            tolerance,
            precision: self.precision,
        })
    }

    pub fn is_default(&self) -> bool {
        self.tolerance.is_none() && self.precision.is_none()
    }
}

/// JSON helper for a typed-key map. JSON object keys must be strings, so the
/// deterministic `BTreeMap` is represented as an ordered array of typed
/// `(EngineeringDimensionRef, DimensionalSpecification)` pairs on the wire.
pub(crate) mod dimensional_specification_map {
    use super::*;

    pub fn serialize<S>(
        map: &BTreeMap<EngineeringDimensionRef, DimensionalSpecification>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        map.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<EngineeringDimensionRef, DimensionalSpecification>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries =
            Vec::<(EngineeringDimensionRef, DimensionalSpecification)>::deserialize(deserializer)?;
        let mut map = BTreeMap::new();
        for (target, specification) in entries {
            if map.insert(target, specification).is_some() {
                return Err(serde::de::Error::custom(
                    "duplicate engineering dimensional specification target",
                ));
            }
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_tolerance_is_canonicalized_to_none() {
        assert_eq!(
            DimensionalTolerance::symmetric(0.0).unwrap(),
            DimensionalTolerance::None
        );
        assert_eq!(
            DimensionalTolerance::bilateral(0.0, -0.0).unwrap(),
            DimensionalTolerance::None
        );
    }

    #[test]
    fn tolerance_validation_rejects_non_finite_negative_and_wrong_sign_values() {
        assert!(DimensionalTolerance::symmetric(-0.01).is_err());
        assert!(DimensionalTolerance::symmetric(f64::NAN).is_err());
        assert!(DimensionalTolerance::symmetric(f64::INFINITY).is_err());
        assert!(DimensionalTolerance::bilateral(-0.01, -0.02).is_err());
        assert!(DimensionalTolerance::bilateral(0.01, 0.02).is_err());
        assert!(DimensionalTolerance::bilateral(0.01, f64::NAN).is_err());
        assert!(DimensionalTolerance::bilateral(0.01, f64::INFINITY).is_err());
        assert!(DimensionalTolerance::Bilateral {
            upper: -0.01,
            lower: 0.02,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn bilateral_tolerance_uses_signed_deviations() {
        assert_eq!(
            DimensionalTolerance::bilateral(0.20, -0.05).unwrap(),
            DimensionalTolerance::Bilateral {
                upper: 0.20,
                lower: -0.05,
            }
        );
        assert!(DimensionalTolerance::Bilateral {
            upper: -0.01,
            lower: -0.05,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn decimal_places_is_frozen_to_zero_through_four() {
        for value in 0..=4 {
            assert!(DecimalPlaces::new(value).is_ok());
        }
        assert!(DecimalPlaces::new(5).is_err());
    }

    #[test]
    fn specification_and_typed_target_roundtrip_deterministically() {
        let target = EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: FeatureParameterSlot::Width,
        };
        let specification = DimensionalSpecification::new(
            DimensionalTolerance::bilateral(0.20, -0.05).unwrap(),
            Some(DecimalPlaces(2)),
        )
        .unwrap();
        let json = serde_json::to_string(&(target.clone(), specification.clone())).unwrap();
        assert_eq!(
            serde_json::from_str::<(EngineeringDimensionRef, DimensionalSpecification)>(&json)
                .unwrap(),
            (target, specification)
        );
    }
}
