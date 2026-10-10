//! Strongly typed feature→parameter slot binding.
//!
//! A plain `FeatureId → Set<ParameterId>` relation cannot say whether
//! `plate_width` drives a `BaseProfile`'s `width` or `height`. Phase 3
//! introduces [`FeatureParameterSlot`]: each bind names the exact feature field
//! a parameter drives, the slot is validated against the feature type at bind
//! time, and lowering resolves values without ever guessing from a parameter id
//! string (ADR-017 Decision 4).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::feature::{BaseProfile, EngineeringFeature, HolePattern};
use crate::parametric::{HoleKind, HoleTermination, ParametricFeature};
use crate::{FeatureId, ParameterId, PartId};

/// Physical value type expected by a supported feature parameter capability.
///
/// This is intentionally distinct from ParameterValue: a binding declares
/// the engineering meaning and unit expected at the feature boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedParameterType {
    LengthMm,
    AngleDegrees,
}

impl ExpectedParameterType {
    pub const fn unit(self) -> &'static str {
        match self {
            Self::LengthMm => "mm",
            Self::AngleDegrees => "deg",
        }
    }
}

/// Stable, typed address for editing one bound feature parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureParameterReference {
    pub part_id: PartId,
    pub feature_id: FeatureId,
    pub slot: FeatureParameterSlot,
    pub parameter_id: ParameterId,
    pub expected_type: ExpectedParameterType,
}

/// Deterministic capability metadata for one editable feature slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureParameterCapability {
    pub expected_type: ExpectedParameterType,
}

/// A typed field of an engineering feature that can be driven by a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureParameterSlot {
    /// `BaseProfile`/`Slot` width.
    Width,
    /// `BaseProfile` height.
    Height,
    /// Center X coordinate (holes, patterns, construction circles).
    CenterX,
    /// Center Y coordinate.
    CenterY,
    /// Circle/hole diameter.
    Diameter,
    /// Depth of a Hole whose termination is explicitly Blind.
    BlindDepth,
    /// Diameter of a supported Counterbore Hole kind.
    CounterboreDiameter,
    /// Depth of a supported Counterbore Hole kind.
    CounterboreDepth,
    /// Mouth diameter of a typed Countersink Hole.
    CountersinkMouthDiameter,
    /// Included cone angle in degrees of a typed Countersink Hole.
    CountersinkIncludedAngleDegrees,
    /// `Fillet` radius.
    Radius,
    /// `Slot` overall length (end to end).
    Length,
    /// `Slot` orientation in degrees.
    Orientation,
    /// Pattern hole count.
    Count,
    /// Circular pattern pitch circle diameter.
    PitchCircleDiameter,
    /// Circular pattern start angle.
    StartAngle,
    /// Grid pattern row count.
    Rows,
    /// Grid pattern column count.
    Columns,
    /// Grid pattern x spacing.
    SpacingX,
    /// Grid pattern y spacing.
    SpacingY,
    /// Corner pattern x offset.
    OffsetX,
    /// Corner pattern y offset.
    OffsetY,
}

impl FeatureParameterSlot {
    /// Parse a stable slot name (the serde snake_case spelling used by the DSL
    /// and the tool layer) into a strong-typed slot.
    pub fn from_name(name: &str) -> Option<FeatureParameterSlot> {
        use FeatureParameterSlot::*;
        Some(match name {
            "width" => Width,
            "height" => Height,
            "center_x" => CenterX,
            "center_y" => CenterY,
            "diameter" => Diameter,
            "blind_depth" => BlindDepth,
            "counterbore_diameter" => CounterboreDiameter,
            "counterbore_depth" => CounterboreDepth,
            "countersink_mouth_diameter" => CountersinkMouthDiameter,
            "countersink_included_angle_degrees" => CountersinkIncludedAngleDegrees,
            "radius" => Radius,
            "length" => Length,
            "orientation" => Orientation,
            "count" => Count,
            "pcd" => PitchCircleDiameter,
            "start_angle" => StartAngle,
            "rows" => Rows,
            "columns" => Columns,
            "spacing_x" => SpacingX,
            "spacing_y" => SpacingY,
            "offset_x" => OffsetX,
            "offset_y" => OffsetY,
            _ => return None,
        })
    }
}

impl std::fmt::Display for FeatureParameterSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use FeatureParameterSlot::*;
        let name = match self {
            Width => "width",
            Height => "height",
            CenterX => "center_x",
            CenterY => "center_y",
            Diameter => "diameter",
            BlindDepth => "blind_depth",
            CounterboreDiameter => "counterbore_diameter",
            CounterboreDepth => "counterbore_depth",
            CountersinkMouthDiameter => "countersink_mouth_diameter",
            CountersinkIncludedAngleDegrees => "countersink_included_angle_degrees",
            Radius => "radius",
            Length => "length",
            Orientation => "orientation",
            Count => "count",
            PitchCircleDiameter => "pitch_circle_diameter",
            StartAngle => "start_angle",
            Rows => "rows",
            Columns => "columns",
            SpacingX => "spacing_x",
            SpacingY => "spacing_y",
            OffsetX => "offset_x",
            OffsetY => "offset_y",
        };
        f.write_str(name)
    }
}

impl FeatureParameterSlot {
    /// The slots that are meaningful for a given feature type.
    ///
    /// Binding any other slot to the feature is rejected at bind time so the
    /// system never becomes a loose "any slot on any feature" binding.
    pub fn valid_for(feature: &EngineeringFeature) -> BTreeSet<FeatureParameterSlot> {
        use FeatureParameterSlot::*;
        match feature {
            EngineeringFeature::BaseProfile { profile, .. } => match profile {
                BaseProfile::Rectangle { .. } => BTreeSet::from([Width, Height]),
                BaseProfile::Circle { .. } => BTreeSet::from([CenterX, CenterY, Diameter]),
                BaseProfile::RoundedRectangle { .. } => {
                    BTreeSet::from([CenterX, CenterY, Width, Height, Radius])
                }
                BaseProfile::Composite { .. } | BaseProfile::Polygon { .. } => BTreeSet::new(),
            },
            EngineeringFeature::Hole { .. } => BTreeSet::from([CenterX, CenterY, Diameter]),
            EngineeringFeature::HolePattern { pattern, .. } => match pattern {
                HolePattern::Circular { .. } => BTreeSet::from([
                    CenterX,
                    CenterY,
                    Diameter,
                    Count,
                    PitchCircleDiameter,
                    StartAngle,
                ]),
                HolePattern::Grid { .. } => BTreeSet::from([
                    CenterX, CenterY, Rows, Columns, SpacingX, SpacingY, Diameter,
                ]),
                HolePattern::Corner { .. } => BTreeSet::from([OffsetX, OffsetY, Diameter]),
            },
            EngineeringFeature::Slot { .. } => {
                BTreeSet::from([Length, Width, CenterX, CenterY, Orientation])
            }
            EngineeringFeature::Fillet { .. } => BTreeSet::from([Radius]),
            EngineeringFeature::ConstructionCircle { .. } => {
                BTreeSet::from([CenterX, CenterY, Diameter])
            }
            EngineeringFeature::Parametric {
                feature:
                    ParametricFeature::Hole {
                        termination, kind, ..
                    },
                ..
            } => {
                let mut slots = BTreeSet::from([Diameter]);
                if matches!(termination, HoleTermination::Blind { .. }) {
                    slots.insert(BlindDepth);
                }
                match kind {
                    HoleKind::Counterbore { .. } => {
                        slots.insert(CounterboreDiameter);
                        slots.insert(CounterboreDepth);
                    }
                    HoleKind::Countersink { .. } => {
                        slots.insert(CountersinkMouthDiameter);
                        slots.insert(CountersinkIncludedAngleDegrees);
                    }
                    HoleKind::Simple => {}
                }
                slots
            }
            EngineeringFeature::Parametric { .. } => BTreeSet::new(),
        }
    }

    /// Return the closed editable capability schema for a feature slot.
    /// G6A.3 exposes only millimetre-valued v2 Hole dimensions.
    pub fn capability_for(
        feature: &EngineeringFeature,
        slot: FeatureParameterSlot,
    ) -> Option<FeatureParameterCapability> {
        if !matches!(
            feature,
            EngineeringFeature::Parametric {
                feature: ParametricFeature::Hole { .. },
                ..
            }
        ) || !Self::valid_for(feature).contains(&slot)
        {
            return None;
        }
        Some(FeatureParameterCapability {
            expected_type: if slot == FeatureParameterSlot::CountersinkIncludedAngleDegrees {
                ExpectedParameterType::AngleDegrees
            } else {
                ExpectedParameterType::LengthMm
            },
        })
    }
}

/// A deterministic parameter identity used when the Workbench edits an
/// unbound, schema-supported slot. The binding remains in EngineeringModel;
/// this helper only derives its stable ID from model identity.
pub fn stable_feature_parameter_id(
    part_id: &PartId,
    feature_id: &FeatureId,
    slot: FeatureParameterSlot,
) -> ParameterId {
    // Fixed FNV-1a variant for cross-process/cross-restart stability. A
    // collision is checked against the model and fails closed at command time.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in part_id
        .0
        .bytes()
        .chain([0])
        .chain(feature_id.0.bytes())
        .chain([0])
        .chain(slot.to_string().bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    ParameterId(format!("fp_{hash:016x}"))
}

/// The literal slot values embedded in a feature (its initial defaults).
///
/// Slots with no literal field (e.g. a pattern's implicit center) are absent
/// and default to `0.0` during lowering unless bound to a parameter.
pub fn feature_field_slots(feature: &EngineeringFeature) -> BTreeMap<FeatureParameterSlot, f64> {
    use FeatureParameterSlot::*;
    match feature {
        EngineeringFeature::BaseProfile { profile, .. } => match profile {
            BaseProfile::Rectangle { width, height } => {
                BTreeMap::from([(Width, *width), (Height, *height)])
            }
            BaseProfile::Circle { center, diameter } => BTreeMap::from([
                (CenterX, center.x),
                (CenterY, center.y),
                (Diameter, *diameter),
            ]),
            BaseProfile::RoundedRectangle {
                center,
                width,
                height,
                corner_radius,
            } => BTreeMap::from([
                (CenterX, center.x),
                (CenterY, center.y),
                (Width, *width),
                (Height, *height),
                (Radius, *corner_radius),
            ]),
            BaseProfile::Composite { .. } | BaseProfile::Polygon { .. } => BTreeMap::new(),
        },
        EngineeringFeature::Hole { diameter, .. } => BTreeMap::from([(Diameter, *diameter)]),
        EngineeringFeature::HolePattern { pattern, .. } => pattern_field_slots(pattern),
        EngineeringFeature::Slot {
            center,
            length,
            width,
            orientation_degrees,
            ..
        } => BTreeMap::from([
            (Length, *length),
            (Width, *width),
            (CenterX, center.x),
            (CenterY, center.y),
            (Orientation, *orientation_degrees),
        ]),
        EngineeringFeature::Fillet { radius, .. } => BTreeMap::from([(Radius, *radius)]),
        EngineeringFeature::ConstructionCircle { diameter, .. } => {
            BTreeMap::from([(Diameter, *diameter)])
        }
        EngineeringFeature::Parametric {
            feature:
                ParametricFeature::Hole {
                    diameter,
                    termination,
                    kind,
                    ..
                },
            ..
        } => {
            let mut values = BTreeMap::from([(Diameter, *diameter)]);
            if let HoleTermination::Blind { depth } = termination {
                values.insert(BlindDepth, *depth);
            }
            match kind {
                HoleKind::Counterbore { diameter, depth } => {
                    values.insert(CounterboreDiameter, *diameter);
                    values.insert(CounterboreDepth, *depth);
                }
                HoleKind::Countersink {
                    diameter,
                    included_angle_degrees,
                } => {
                    values.insert(CountersinkMouthDiameter, *diameter);
                    values.insert(CountersinkIncludedAngleDegrees, *included_angle_degrees);
                }
                HoleKind::Simple => {}
            }
            values
        }
        EngineeringFeature::Parametric { .. } => BTreeMap::new(),
    }
}

fn pattern_field_slots(pattern: &HolePattern) -> BTreeMap<FeatureParameterSlot, f64> {
    use FeatureParameterSlot::*;
    match pattern {
        HolePattern::Circular {
            center,
            pitch_circle_diameter,
            count,
            start_angle,
            hole,
        } => BTreeMap::from([
            (CenterX, center.x),
            (CenterY, center.y),
            (PitchCircleDiameter, *pitch_circle_diameter),
            (Count, f64::from(*count)),
            (StartAngle, *start_angle),
            (Diameter, hole.diameter),
        ]),
        HolePattern::Grid {
            rows,
            columns,
            spacing_x,
            spacing_y,
            hole,
        } => BTreeMap::from([
            (Rows, f64::from(*rows)),
            (Columns, f64::from(*columns)),
            (SpacingX, *spacing_x),
            (SpacingY, *spacing_y),
            (Diameter, hole.diameter),
        ]),
        HolePattern::Corner {
            offset_x,
            offset_y,
            hole,
        } => BTreeMap::from([
            (OffsetX, *offset_x),
            (OffsetY, *offset_y),
            (Diameter, hole.diameter),
        ]),
    }
}
