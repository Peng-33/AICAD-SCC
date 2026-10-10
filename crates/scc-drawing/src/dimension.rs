//! Engineering drawing dimensions (Phase 8.5): what is measured and where the
//! number comes from.
//!
//! A [`DrawingDimension`] answers *"which engineering quantity does this
//! dimension express?"* — never *"where is the dimension line drawn?"* (Phase
//! 8.6 layout) and never *"how is it formatted?"* (`"Ø64"`, `"8×Ø10"` are
//! future formatter output; nothing here stores strings).
//!
//! Core rules (ADR-026):
//!
//! - **Typed source, not literal**: every number traces to a
//!   [`DimensionValueSource`] — a bound `ParameterId`, or (only when unbound)
//!   the owning feature's own slot. Never a bare `120.0`.
//! - **Binding wins**: if `Feature + Slot` has a parameter binding the source
//!   *must* be `Parameter`; referencing a stale feature literal instead is
//!   forbidden. This mirrors the Phase 3 lowering resolution order exactly.
//! - **Model values, not paper values**: resolved values are always real model
//!   engineering values (120 stays 120 at any [`crate::DrawingScale`]).
//! - **Per-feature, not per-primitive**: a `HolePattern` yields ONE structured
//!   Pattern dimension (count / diameter / PCD taken from engineering
//!   semantics), never one dimension per expanded circle and never
//!   geometry-derived numbers (`radius × 2`, measured PCD).
//! - `resolved` values are derived presentation caches recomputed on every
//!   build; the source is the provenance. No Drawing→Engineering write path.

use std::collections::BTreeSet;

use scc_core::ids::EntityId;
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::feature::{BaseProfile, EngineeringFeature, HolePattern};
use scc_engineering::model::EngineeringModel;
use scc_engineering::specification::{
    DecimalPlaces, DimensionalSpecification, DimensionalTolerance, EngineeringDimensionRef,
};
use scc_engineering::{FeatureId, HoleStartFace, ParameterId, ResolvedValue};
use scc_spatial::{SpatialCutKind, ZInterval};
use serde::{Deserialize, Serialize};

use crate::{DrawingError, DrawingView};

/// Strong-typed, deterministic identifier of a dimension.
///
/// Semantic and stable (e.g. `"plate.width"`, `"bolt_pattern.pattern"`),
/// derived from the target feature id — identity only. The typed source fields
/// carry the semantics; the id string is **never** parsed back.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingDimensionId(pub String);

impl std::fmt::Display for DrawingDimensionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Orientation of a linear dimension. Engineering semantics, not a layout
/// coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearAxis {
    Horizontal,
    Vertical,
}

/// A principal model-space axis used by parametric geometric dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionAxis3D {
    X,
    Y,
    Z,
}

/// Formal EngineeringModel v2 feature parameter selected by an associative
/// dimension. These values are declared by typed features, not read from an
/// SVG or transient kernel ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricDimensionParameter {
    ExtrudeDistance,
    /// Diameter of a circular Sketch → Extrude ADD profile.
    BossDiameter,
    HoleDiameter,
    HoleDepth,
    CounterboreDiameter,
    CounterboreDepth,
    FilletRadius,
    PatternCount,
    PatternSpacing,
    PatternRadius,
}

/// The deterministic reference edge used by Phase 11.3 hole locations.
/// This is drawing geometry, not a GD&T datum feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleDatum {
    LeftEdge,
    BottomEdge,
}

/// A typed field resolved from a formal hole-pattern semantic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternSemanticField {
    CenterX,
    CenterY,
    Count,
    Rows,
    Columns,
    SpacingX,
    SpacingY,
    PitchCircleDiameter,
    OffsetX,
    OffsetY,
}

/// Where a dimension's number comes from — strong-typed engineering source.
///
/// `Parameter` when the feature/slot has a binding (the only correct source
/// then); `FeatureSlot` only when no binding exists, referencing the owning
/// feature's own typed slot. A bare literal value is never a source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DimensionValueSource {
    /// A bound engineering parameter (drives the value through expressions).
    Parameter { parameter: ParameterId },
    /// A typed parameter on an EngineeringModel v2 parametric feature.
    EngineeringFeatureParameter {
        feature: FeatureId,
        parameter: ParametricDimensionParameter,
    },
    /// A body envelope value measured from the current validated rebuild.
    VerifiedBodyGeometry {
        model_sha256: String,
        axis: DimensionAxis3D,
    },
    /// The feature's own literal for a typed slot (no binding exists).
    FeatureSlot {
        feature: FeatureId,
        slot: FeatureParameterSlot,
    },
    /// A value measured from the already-resolved drawing/spatial geometry.
    /// This is the Phase 11.1 source for production three-view annotations:
    /// the drawing records the feature/entity provenance but never re-reads
    /// engineering parameters or re-lowers a cut.
    ResolvedGeometry {
        feature: FeatureId,
        entity: EntityId,
    },
    /// A resolved location from a hole/pattern instance to a typed body edge.
    ResolvedHoleLocation {
        feature: FeatureId,
        entity: EntityId,
        datum_feature: FeatureId,
        datum_entity: EntityId,
        datum: HoleDatum,
    },
    /// A resolved location of a pattern's center to a typed body edge.
    ResolvedPatternLocation {
        feature: FeatureId,
        representative_entity: EntityId,
        datum_feature: FeatureId,
        datum_entity: EntityId,
        datum: HoleDatum,
    },
    /// A resolved distance between two pattern instance centers.
    ResolvedPatternSpacing {
        feature: FeatureId,
        first_entity: EntityId,
        second_entity: EntityId,
        axis: LinearAxis,
    },
    /// A resolved value supplied by the formal pattern semantic, never reread
    /// or recomputed from instance geometry by the drawing layer.
    ResolvedPatternSemantic {
        feature: FeatureId,
        field: PatternSemanticField,
    },
    /// A blind-hole depth resolved from the SpatialModel Z witness. The
    /// drawing never evaluates the EngineeringModel depth expression itself.
    ResolvedHoleDepth {
        feature: FeatureId,
        entity: EntityId,
    },
}

/// The resolved engineering value of a dimension component.
///
/// Real model units — millimetres for lengths, a pure count for pattern
/// counts. This is a **derived cache** recomputed on every build, kept next to
/// its source so consumers can read the number without model access.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolvedDimensionValue {
    LengthMm { mm: f64 },
    Count { count: u32 },
}

impl ResolvedDimensionValue {
    /// Whether the cached value is semantically valid (finite positive length,
    /// or count ≥ 1).
    pub fn is_valid(&self) -> bool {
        match self {
            ResolvedDimensionValue::LengthMm { mm } => mm.is_finite() && *mm > 0.0,
            ResolvedDimensionValue::Count { count } => *count >= 1,
        }
    }
}

/// One measured quantity inside a Pattern dimension.
/// A validated derived copy of one authoritative engineering specification.
///
/// Drawing consumers may format this copy, but the typed reference remains
/// attached so stale, injected, or detached presentation metadata can be
/// rejected against the EngineeringModel before rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingDimensionSpecification {
    pub reference: EngineeringDimensionRef,
    pub value: DimensionalSpecification,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionComponent {
    pub source: DimensionValueSource,
    pub resolved: ResolvedDimensionValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specification: Option<DrawingDimensionSpecification>,
}

/// The typed extent carried by an engineering hole callout.
///
/// depth is a resolved SpatialModel witness, while side remains the
/// authoritative typed EngineeringModel identity. The side is never
/// recovered from the Z interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HoleCalloutExtent {
    Through,
    Blind {
        depth: DimensionComponent,
        side: HoleStartFace,
    },
}

/// One resolved cut witness attached to a hole callout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoleCalloutWitness {
    pub entity: EntityId,
    pub radius: f64,
    pub z: ZInterval,
    pub kind: SpatialCutKind,
}

/// Typed source/provenance for an engineering hole callout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoleCalloutProvenance {
    pub feature: FeatureId,
    pub representative_entity: EntityId,
    pub witnesses: Vec<HoleCalloutWitness>,
}

/// Engineering hole semantics after resolution, before formatting.
///
/// quantity is present only for a formal hole pattern and comes from its
/// typed pattern semantics. diameter comes from a resolved cut radius and
/// extent combines typed EngineeringModel identity with resolved spatial
/// witness values. No display string is stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoleCalloutSemantic {
    pub diameter: DimensionComponent,
    pub quantity: Option<DimensionComponent>,
    /// Whether the existing circular-pattern PCD semantic is required in the
    /// compact production callout. Circular base drawings enable this without
    /// creating a second PCD truth; legacy rectangular callouts keep their
    /// Phase 11 presentation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub show_pitch_circle_diameter: bool,
    pub extent: HoleCalloutExtent,
    pub provenance: HoleCalloutProvenance,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The engineering shape of a Pattern dimension, mirroring `HolePattern`
/// one-to-one. Every component carries its own typed source; counts come from
/// engineering slots — never from counting solved circles — and the PCD comes
/// from the pattern's semantics, never from measuring hole coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum PatternShape {
    Circular {
        count: DimensionComponent,
        pitch_circle_diameter: DimensionComponent,
        hole_diameter: DimensionComponent,
    },
    Grid {
        rows: DimensionComponent,
        columns: DimensionComponent,
        spacing_x: DimensionComponent,
        spacing_y: DimensionComponent,
        hole_diameter: DimensionComponent,
    },
    Corner {
        offset_x: DimensionComponent,
        offset_y: DimensionComponent,
        hole_diameter: DimensionComponent,
    },
}

impl PatternShape {
    /// All components in deterministic order.
    pub fn components(&self) -> Vec<&DimensionComponent> {
        match self {
            PatternShape::Circular {
                count,
                pitch_circle_diameter,
                hole_diameter,
            } => vec![count, pitch_circle_diameter, hole_diameter],
            PatternShape::Grid {
                rows,
                columns,
                spacing_x,
                spacing_y,
                hole_diameter,
            } => vec![rows, columns, spacing_x, spacing_y, hole_diameter],
            PatternShape::Corner {
                offset_x,
                offset_y,
                hole_diameter,
            } => vec![offset_x, offset_y, hole_diameter],
        }
    }
}

/// What kind of dimension this is.
///
/// MVP: `Linear` (horizontal/vertical), `Diameter`, and the structured
/// `Pattern`. `Radius` is defined for completeness but no current feature
/// produces a reliable radius semantic, so the builder never generates it.
/// Angular / Coordinate / Ordinate / ArcLength / Chamfer / Thread / Tolerance
/// are deliberately absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawingDimensionKind {
    Linear {
        axis: LinearAxis,
    },
    Diameter,
    /// An overall material-body diameter, distinct from a hole diameter.
    OverallDiameter,
    Radius,
    Pattern,
    SlotCallout,
}

/// What a dimension measures — always an engineering feature as a whole.
///
/// No fake edge/entity targets: layout later resolves
/// `Feature → DrawingPrimitive → geometry` for witness geometry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DrawingDimensionTarget {
    /// The complete validated material body.
    Body,
    Feature {
        feature: FeatureId,
    },
}

/// The value payload of a dimension.
///
/// Invariant enforced by validation: `kind == Pattern` ⟺ `value == Pattern`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "value", rename_all = "snake_case")]
pub enum DimensionValue {
    Measured {
        source: DimensionValueSource,
        resolved: ResolvedDimensionValue,
    },
    Pattern {
        shape: PatternShape,
    },
    /// A three-view engineering hole callout. pattern preserves the
    /// structured pattern semantics used by the existing Pattern dimension;
    /// the formatter reads extent/diameter/quantity only from semantic.
    HoleCallout {
        semantic: HoleCalloutSemantic,
        pattern: Option<PatternShape>,
    },
    SlotCallout {
        semantic: SlotCalloutSemantic,
    },
    /// EngineeringModel v2 associative Hole/Counterbore callout.
    ParametricHoleCallout {
        semantic: ParametricHoleCalloutSemantic,
    },
}

/// Typed through-slot callout. The formatted string is presentation only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotCalloutSemantic {
    pub overall_length: DimensionComponent,
    pub width: DimensionComponent,
    pub feature: FeatureId,
    pub entity: EntityId,
}

/// Formally resolved extent for a generic parametric hole callout. Depth is
/// retained as engineering semantics and checked against exact cylindrical
/// face evidence before the DrawingModel is returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParametricHoleExtent {
    Through,
    Blind { depth: DimensionComponent },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParametricHolePatternCallout {
    Linear {
        count: DimensionComponent,
        spacing: DimensionComponent,
    },
    Circular {
        count: DimensionComponent,
        radius: DimensionComponent,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricCounterboreCallout {
    pub diameter: DimensionComponent,
    pub depth: DimensionComponent,
}

/// Typed presentation value for an EngineeringModel v2 Hole or hole Pattern.
/// It is populated only after parameter-to-B-Rep witnesses agree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricHoleCalloutSemantic {
    pub feature: FeatureId,
    pub diameter: DimensionComponent,
    pub extent: ParametricHoleExtent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterbore: Option<ParametricCounterboreCallout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<ParametricHolePatternCallout>,
}

/// One engineering drawing dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingDimension {
    pub id: DrawingDimensionId,
    pub target: DrawingDimensionTarget,
    pub kind: DrawingDimensionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specification: Option<DrawingDimensionSpecification>,
    pub value: DimensionValue,
}

impl DrawingDimension {
    /// All value sources referenced anywhere in this dimension.
    pub fn sources(&self) -> Vec<&DimensionValueSource> {
        match &self.value {
            DimensionValue::Measured { source, .. } => vec![source],
            DimensionValue::Pattern { shape } => {
                shape.components().into_iter().map(|c| &c.source).collect()
            }
            DimensionValue::SlotCallout { semantic } => {
                vec![&semantic.overall_length.source, &semantic.width.source]
            }
            DimensionValue::HoleCallout { semantic, pattern } => {
                let mut sources = vec![&semantic.diameter.source];
                if let Some(quantity) = &semantic.quantity {
                    sources.push(&quantity.source);
                }
                if let HoleCalloutExtent::Blind { depth, .. } = &semantic.extent {
                    sources.push(&depth.source);
                }
                if let Some(shape) = pattern {
                    sources.extend(shape.components().into_iter().map(|c| &c.source));
                }
                sources
            }
            DimensionValue::ParametricHoleCallout { semantic } => {
                let mut sources = vec![&semantic.diameter.source];
                if let ParametricHoleExtent::Blind { depth } = &semantic.extent {
                    sources.push(&depth.source);
                }
                if let Some(counterbore) = &semantic.counterbore {
                    sources.push(&counterbore.diameter.source);
                    sources.push(&counterbore.depth.source);
                }
                if let Some(pattern) = &semantic.pattern {
                    match pattern {
                        ParametricHolePatternCallout::Linear { count, spacing } => {
                            sources.push(&count.source);
                            sources.push(&spacing.source);
                        }
                        ParametricHolePatternCallout::Circular { count, radius } => {
                            sources.push(&count.source);
                            sources.push(&radius.source);
                        }
                    }
                }
                sources
            }
        }
    }
}

/// Whether the feature/slot pair has a parameter binding on the model.
fn binding_of(
    model: &EngineeringModel,
    feature: &FeatureId,
    slot: FeatureParameterSlot,
) -> Option<ParameterId> {
    model
        .feature_parameters
        .get(feature)
        .and_then(|bindings| bindings.get(&slot))
        .cloned()
}

/// Copy one authoritative EngineeringModel specification into DrawingModel.
///
/// The copy is deliberately optional: absence means the dimension retains the
/// legacy nominal presentation. No formatted text is persisted here.
pub(crate) fn drawing_specification(
    model: &EngineeringModel,
    reference: EngineeringDimensionRef,
) -> Option<DrawingDimensionSpecification> {
    model
        .dimensional_specification(&reference)
        .cloned()
        .map(|value| DrawingDimensionSpecification { reference, value })
}

/// Convert a raw numeric into the right resolved type for the slot: count-like
/// slots carry positive whole counts; every other policy slot is a length in mm.
fn numeric(slot: FeatureParameterSlot, value: f64) -> Result<ResolvedDimensionValue, DrawingError> {
    match slot {
        FeatureParameterSlot::Count
        | FeatureParameterSlot::Rows
        | FeatureParameterSlot::Columns => {
            if !value.is_finite() || value < 1.0 || value.fract() != 0.0 {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!(
                        "slot {} must resolve to a positive whole count, got {}",
                        slot, value
                    ),
                });
            }
            Ok(ResolvedDimensionValue::Count {
                count: value as u32,
            })
        }
        _ => {
            if !value.is_finite() || value <= 0.0 {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!(
                        "slot {} must resolve to a positive length in mm, got {}",
                        slot, value
                    ),
                });
            }
            Ok(ResolvedDimensionValue::LengthMm { mm: value })
        }
    }
}

/// Resolve a raw number from a bound parameter, fail-closed on unresolved.
fn raw_from_parameter(
    model: &EngineeringModel,
    parameter: &ParameterId,
) -> Result<f64, DrawingError> {
    let evaluated = model.evaluate_parameter(parameter).map_err(|_| {
        DrawingError::UnresolvedDimensionValue {
            parameter: parameter.clone(),
        }
    })?;
    match evaluated {
        ResolvedValue::Scalar(v) => Ok(v),
        ResolvedValue::Integer(i) => Ok(i as f64),
        // Deliberately unresolved must never fall back to a feature literal
        // (or zero). Normally unreachable at drawing time: lowering already
        // fails earlier with SCC-ENG-027 for bound-but-unresolved slots.
        ResolvedValue::Unresolved => Err(DrawingError::UnresolvedDimensionValue {
            parameter: parameter.clone(),
        }),
        ResolvedValue::Boolean(_) | ResolvedValue::Choice(_) => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: format!("parameter {} is not numeric", parameter),
            })
        }
    }
}

/// The feature's own literal for a typed slot — the second resolution tier,
/// mirroring the Phase 3 lowering order right after the binding tier.
fn literal_for_slot(feature: &EngineeringFeature, slot: FeatureParameterSlot) -> Option<f64> {
    match (feature, slot) {
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Rectangle { width, .. },
                ..
            },
            FeatureParameterSlot::Width,
        ) => Some(*width),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Rectangle { height, .. },
                ..
            },
            FeatureParameterSlot::Height,
        ) => Some(*height),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Circle { center, .. },
                ..
            },
            FeatureParameterSlot::CenterX,
        ) => Some(center.x),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Circle { center, .. },
                ..
            },
            FeatureParameterSlot::CenterY,
        ) => Some(center.y),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Circle { diameter, .. },
                ..
            },
            FeatureParameterSlot::Diameter,
        ) => Some(*diameter),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::RoundedRectangle { center, .. },
                ..
            },
            FeatureParameterSlot::CenterX,
        ) => Some(center.x),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::RoundedRectangle { center, .. },
                ..
            },
            FeatureParameterSlot::CenterY,
        ) => Some(center.y),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::RoundedRectangle { width, .. },
                ..
            },
            FeatureParameterSlot::Width,
        ) => Some(*width),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::RoundedRectangle { height, .. },
                ..
            },
            FeatureParameterSlot::Height,
        ) => Some(*height),
        (
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::RoundedRectangle { corner_radius, .. },
                ..
            },
            FeatureParameterSlot::Radius,
        ) => Some(*corner_radius),
        (EngineeringFeature::Hole { diameter, .. }, FeatureParameterSlot::Diameter) => {
            Some(*diameter)
        }
        (
            EngineeringFeature::Slot {
                center,
                length,
                width,
                orientation_degrees,
                ..
            },
            slot,
        ) => match slot {
            FeatureParameterSlot::Length => Some(*length),
            FeatureParameterSlot::Width => Some(*width),
            FeatureParameterSlot::CenterX => Some(center.x),
            FeatureParameterSlot::CenterY => Some(center.y),
            FeatureParameterSlot::Orientation => Some(*orientation_degrees),
            _ => None,
        },
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Circular { count, .. },
                ..
            },
            FeatureParameterSlot::Count,
        ) => Some(f64::from(*count)),
        (
            EngineeringFeature::HolePattern {
                pattern:
                    HolePattern::Circular {
                        pitch_circle_diameter,
                        ..
                    },
                ..
            },
            FeatureParameterSlot::PitchCircleDiameter,
        ) => Some(*pitch_circle_diameter),
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Grid { rows, .. },
                ..
            },
            FeatureParameterSlot::Rows,
        ) => Some(f64::from(*rows)),
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Grid { columns, .. },
                ..
            },
            FeatureParameterSlot::Columns,
        ) => Some(f64::from(*columns)),
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Grid { spacing_x, .. },
                ..
            },
            FeatureParameterSlot::SpacingX,
        ) => Some(*spacing_x),
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Grid { spacing_y, .. },
                ..
            },
            FeatureParameterSlot::SpacingY,
        ) => Some(*spacing_y),
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Corner { offset_x, .. },
                ..
            },
            FeatureParameterSlot::OffsetX,
        ) => Some(*offset_x),
        (
            EngineeringFeature::HolePattern {
                pattern: HolePattern::Corner { offset_y, .. },
                ..
            },
            FeatureParameterSlot::OffsetY,
        ) => Some(*offset_y),
        // Every pattern nests its hole diameter in the shared HoleDefinition.
        (EngineeringFeature::HolePattern { pattern, .. }, FeatureParameterSlot::Diameter) => {
            match pattern {
                HolePattern::Circular { hole, .. }
                | HolePattern::Grid { hole, .. }
                | HolePattern::Corner { hole, .. } => Some(hole.diameter),
            }
        }
        // CenterX/CenterY have no literal tier (positions are not dimensions).
        _ => None,
    }
}

/// Resolve one component for `feature`/`slot`: binding first (fail-closed on
/// unresolved), otherwise the feature's own typed literal.
pub(crate) fn resolve_component(
    model: &EngineeringModel,
    feature: &FeatureId,
    slot: FeatureParameterSlot,
) -> Result<DimensionComponent, DrawingError> {
    if let Some(parameter) = binding_of(model, feature, slot) {
        let raw = raw_from_parameter(model, &parameter)?;
        return Ok(DimensionComponent {
            source: DimensionValueSource::Parameter { parameter },
            resolved: numeric(slot, raw)?,
            specification: drawing_specification(
                model,
                EngineeringDimensionRef::FeatureSlot {
                    feature: feature.clone(),
                    slot,
                },
            ),
        });
    }
    let target =
        model
            .features
            .get(feature)
            .ok_or_else(|| DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "feature {} does not exist in the engineering model",
                    feature
                ),
            })?;
    let literal =
        literal_for_slot(target, slot).ok_or_else(|| DrawingError::InvalidDrawingDimension {
            reason: format!("feature {} has no literal for slot {}", feature, slot),
        })?;
    Ok(DimensionComponent {
        source: DimensionValueSource::FeatureSlot {
            feature: feature.clone(),
            slot,
        },
        resolved: numeric(slot, literal)?,
        specification: drawing_specification(
            model,
            EngineeringDimensionRef::FeatureSlot {
                feature: feature.clone(),
                slot,
            },
        ),
    })
}

/// Deterministically generate the MVP dimension set from engineering features.
///
/// Conservative policy (ADR-026): BaseProfile → width + height linear;
/// Hole → diameter; HolePattern → ONE structured Pattern dimension;
/// ConstructionCircle and everything else → none. No automatic dimensions for
/// arbitrary parameters, constraints, or construction geometry.
pub fn generate_dimensions(
    model: &EngineeringModel,
) -> Result<Vec<DrawingDimension>, DrawingError> {
    fn measured(
        fid: &FeatureId,
        suffix: &str,
        kind: DrawingDimensionKind,
        component: DimensionComponent,
    ) -> DrawingDimension {
        DrawingDimension {
            id: DrawingDimensionId(format!("{}.{}", fid, suffix)),
            target: DrawingDimensionTarget::Feature {
                feature: fid.clone(),
            },
            kind,
            specification: component.specification,
            value: DimensionValue::Measured {
                source: component.source,
                resolved: component.resolved,
            },
        }
    }

    let mut out = Vec::new();
    for feature in model.features.values() {
        let fid = feature.id();
        match feature {
            EngineeringFeature::BaseProfile { profile, .. } => match profile {
                scc_engineering::feature::BaseProfile::Rectangle { .. }
                | scc_engineering::feature::BaseProfile::RoundedRectangle { .. } => {
                    out.push(measured(
                        fid,
                        "width",
                        DrawingDimensionKind::Linear {
                            axis: LinearAxis::Horizontal,
                        },
                        resolve_component(model, fid, FeatureParameterSlot::Width)?,
                    ));
                    out.push(measured(
                        fid,
                        "height",
                        DrawingDimensionKind::Linear {
                            axis: LinearAxis::Vertical,
                        },
                        resolve_component(model, fid, FeatureParameterSlot::Height)?,
                    ));
                    if matches!(
                        profile,
                        scc_engineering::feature::BaseProfile::RoundedRectangle { .. }
                    ) {
                        out.push(measured(
                            fid,
                            "corner_radius",
                            DrawingDimensionKind::Radius,
                            resolve_component(model, fid, FeatureParameterSlot::Radius)?,
                        ));
                    }
                }
                scc_engineering::feature::BaseProfile::Circle { .. } => {
                    out.push(measured(
                        fid,
                        "diameter",
                        DrawingDimensionKind::OverallDiameter,
                        resolve_component(model, fid, FeatureParameterSlot::Diameter)?,
                    ));
                }
                scc_engineering::feature::BaseProfile::Composite { .. }
                | scc_engineering::feature::BaseProfile::Polygon { .. } => {}
            },
            EngineeringFeature::Hole { .. } => {
                out.push(measured(
                    fid,
                    "diameter",
                    DrawingDimensionKind::Diameter,
                    resolve_component(model, fid, FeatureParameterSlot::Diameter)?,
                ));
            }
            EngineeringFeature::HolePattern { pattern, .. } => {
                let shape = match pattern {
                    HolePattern::Circular { .. } => PatternShape::Circular {
                        count: resolve_component(model, fid, FeatureParameterSlot::Count)?,
                        pitch_circle_diameter: resolve_component(
                            model,
                            fid,
                            FeatureParameterSlot::PitchCircleDiameter,
                        )?,
                        hole_diameter: resolve_component(
                            model,
                            fid,
                            FeatureParameterSlot::Diameter,
                        )?,
                    },
                    HolePattern::Grid { .. } => PatternShape::Grid {
                        rows: resolve_component(model, fid, FeatureParameterSlot::Rows)?,
                        columns: resolve_component(model, fid, FeatureParameterSlot::Columns)?,
                        spacing_x: resolve_component(model, fid, FeatureParameterSlot::SpacingX)?,
                        spacing_y: resolve_component(model, fid, FeatureParameterSlot::SpacingY)?,
                        hole_diameter: resolve_component(
                            model,
                            fid,
                            FeatureParameterSlot::Diameter,
                        )?,
                    },
                    HolePattern::Corner { .. } => PatternShape::Corner {
                        offset_x: resolve_component(model, fid, FeatureParameterSlot::OffsetX)?,
                        offset_y: resolve_component(model, fid, FeatureParameterSlot::OffsetY)?,
                        hole_diameter: resolve_component(
                            model,
                            fid,
                            FeatureParameterSlot::Diameter,
                        )?,
                    },
                };
                out.push(DrawingDimension {
                    id: DrawingDimensionId(format!("{}.pattern", fid)),
                    target: DrawingDimensionTarget::Feature {
                        feature: fid.clone(),
                    },
                    kind: DrawingDimensionKind::Pattern,
                    specification: None,
                    value: DimensionValue::Pattern { shape },
                });
            }
            // Construction circles get no automatic MVP dimension. Slot callouts
            // are inserted from spatial slot cuts, not inferred here. Fillet
            // never reaches solved geometry (fail closed upstream).
            EngineeringFeature::ConstructionCircle { .. }
            | EngineeringFeature::Slot { .. }
            | EngineeringFeature::Fillet { .. }
            | EngineeringFeature::Parametric { .. } => {}
        }
    }
    Ok(out)
}

/// Resolve one declared EngineeringModel v2 parameter without consulting
/// renderer or topology identifiers.
pub(crate) fn parametric_feature_parameter_value(
    model: &EngineeringModel,
    feature_id: &FeatureId,
    parameter: ParametricDimensionParameter,
) -> Option<f64> {
    use scc_engineering::parametric::{HoleKind, HoleTermination, ParametricFeature};
    let scc_engineering::feature::EngineeringFeature::Parametric {
        feature,
        suppressed: false,
        ..
    } = model.features.get(feature_id)?
    else {
        return None;
    };
    match (feature, parameter) {
        (
            ParametricFeature::Extrude {
                extent:
                    scc_engineering::ExtrudeExtent::Distance { length }
                    | scc_engineering::ExtrudeExtent::Symmetric { length },
                ..
            },
            ParametricDimensionParameter::ExtrudeDistance,
        ) => Some(*length),
        (
            ParametricFeature::Extrude {
                profile,
                operation: scc_engineering::feature::ExtrudeOperation::Add,
                direction,
                ..
            },
            ParametricDimensionParameter::BossDiameter,
        ) => circular_add_extrude_profile(model, profile, direction).map(|(_, diameter)| diameter),
        (ParametricFeature::Hole { .. }, ParametricDimensionParameter::HoleDiameter) => model
            .resolved_feature_parameter_value(feature_id, FeatureParameterSlot::Diameter)
            .ok(),
        (
            ParametricFeature::Hole {
                termination: HoleTermination::Blind { .. },
                ..
            },
            ParametricDimensionParameter::HoleDepth,
        ) => model
            .resolved_feature_parameter_value(feature_id, FeatureParameterSlot::BlindDepth)
            .ok(),
        (
            ParametricFeature::Hole {
                kind: HoleKind::Counterbore { .. },
                ..
            },
            ParametricDimensionParameter::CounterboreDiameter,
        ) => model
            .resolved_feature_parameter_value(feature_id, FeatureParameterSlot::CounterboreDiameter)
            .ok(),
        (
            ParametricFeature::Hole {
                kind: HoleKind::Counterbore { .. },
                ..
            },
            ParametricDimensionParameter::CounterboreDepth,
        ) => model
            .resolved_feature_parameter_value(feature_id, FeatureParameterSlot::CounterboreDepth)
            .ok(),
        (ParametricFeature::Fillet { radius, .. }, ParametricDimensionParameter::FilletRadius) => {
            Some(*radius)
        }
        (
            ParametricFeature::Pattern {
                definition: scc_engineering::PatternDefinition::Linear { count, .. },
                ..
            },
            ParametricDimensionParameter::PatternCount,
        ) => Some(f64::from(*count)),
        (
            ParametricFeature::Pattern {
                definition: scc_engineering::PatternDefinition::Linear { spacing, .. },
                ..
            },
            ParametricDimensionParameter::PatternSpacing,
        ) => Some(*spacing),
        (
            ParametricFeature::Pattern {
                definition: scc_engineering::PatternDefinition::Circular { count, .. },
                ..
            },
            ParametricDimensionParameter::PatternCount,
        ) => Some(f64::from(*count)),
        (
            ParametricFeature::Pattern {
                definition: scc_engineering::PatternDefinition::Circular { radius, .. },
                ..
            },
            ParametricDimensionParameter::PatternRadius,
        ) => Some(*radius),
        _ => None,
    }
}

pub(crate) fn circular_add_extrude_profile(
    model: &EngineeringModel,
    profile_feature: &FeatureId,
    direction: &scc_engineering::parametric::ExtrudeDirection,
) -> Option<(scc_engineering::parametric::SketchEntityId, f64)> {
    use scc_engineering::parametric::SketchCurveGeometry;

    let z_axis = match direction {
        scc_engineering::parametric::ExtrudeDirection::Normal { .. } => true,
        scc_engineering::parametric::ExtrudeDirection::Vector { value } => {
            value.x.abs() <= 1.0e-8 && value.y.abs() <= 1.0e-8 && value.z.abs() > 1.0e-8
        }
    };
    if !z_axis {
        return None;
    }

    let scc_engineering::feature::EngineeringFeature::Parametric {
        feature: scc_engineering::parametric::ParametricFeature::Sketch { support, profile },
        suppressed: false,
        ..
    } = model.features.get(profile_feature)?
    else {
        return None;
    };
    if !support_is_xy(model, support, 0) || !profile.inner_loops.is_empty() {
        return None;
    }
    let [entity] = profile.outer.entities.as_slice() else {
        return None;
    };
    let SketchCurveGeometry::Circle { radius, .. } = &entity.geometry else {
        return None;
    };
    (radius.is_finite() && *radius > 0.0).then(|| (entity.id.clone(), 2.0 * radius))
}

fn support_is_xy(
    model: &EngineeringModel,
    support: &scc_engineering::parametric::PlaneReference,
    depth: usize,
) -> bool {
    use scc_engineering::parametric::{DatumPlaneDefinition, PlaneReference, PrincipalPlane};

    if depth > 32 {
        return false;
    }
    match support {
        PlaneReference::Principal {
            plane: PrincipalPlane::XY,
        } => true,
        PlaneReference::Datum { feature } => {
            let Some(scc_engineering::feature::EngineeringFeature::Parametric {
                feature: scc_engineering::parametric::ParametricFeature::DatumPlane { definition },
                suppressed: false,
                ..
            }) = model.features.get(feature)
            else {
                return false;
            };
            match definition {
                DatumPlaneDefinition::Principal {
                    plane: PrincipalPlane::XY,
                } => true,
                DatumPlaneDefinition::Offset { base, .. } => support_is_xy(model, base, depth + 1),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Read the raw engineering number behind a source, fail-closed.
fn raw_value_from_source(
    model: &EngineeringModel,
    source: &DimensionValueSource,
) -> Result<f64, DrawingError> {
    match source {
        DimensionValueSource::Parameter { parameter } => raw_from_parameter(model, parameter),
        DimensionValueSource::EngineeringFeatureParameter { feature, parameter } => {
            parametric_feature_parameter_value(model, feature, *parameter).ok_or_else(|| {
                DrawingError::InvalidDrawingDimension {
                    reason: format!(
                        "feature {} does not expose parameter {:?}",
                        feature, parameter
                    ),
                }
            })
        }
        DimensionValueSource::VerifiedBodyGeometry { model_sha256, axis } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "body geometry {}:{:?} requires its validated SpatialModel witness",
                    model_sha256, axis
                ),
            })
        }
        DimensionValueSource::ResolvedGeometry { feature, entity } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "resolved geometry source {}:{} has no raw engineering value",
                    feature, entity
                ),
            })
        }
        DimensionValueSource::ResolvedHoleLocation {
            feature, entity, ..
        } => Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "resolved hole location source {}:{} has no raw engineering value",
                feature, entity
            ),
        }),
        DimensionValueSource::ResolvedPatternLocation {
            feature,
            representative_entity,
            ..
        } => Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "resolved pattern location source {}:{} has no raw engineering value",
                feature, representative_entity
            ),
        }),
        DimensionValueSource::ResolvedPatternSpacing { feature, .. } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "resolved pattern spacing source {} has no raw engineering value",
                    feature
                ),
            })
        }
        DimensionValueSource::ResolvedPatternSemantic { feature, field } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "resolved pattern semantic source {}:{:?} has no raw engineering value",
                    feature, field
                ),
            })
        }
        DimensionValueSource::ResolvedHoleDepth { feature, entity } => {
            Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "resolved hole depth source {}:{} has no raw engineering value",
                    feature, entity
                ),
            })
        }
        DimensionValueSource::FeatureSlot { feature, slot } => {
            let target = model.features.get(feature).ok_or_else(|| {
                DrawingError::InvalidDrawingDimension {
                    reason: format!("source feature {} does not exist", feature),
                }
            })?;
            literal_for_slot(target, *slot).ok_or_else(|| DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "source feature {} has no literal for slot {}",
                    feature, slot
                ),
            })
        }
    }
}

/// Whether a freshly read raw value still matches the cached resolution.
fn check_matches_cached(
    cached: &ResolvedDimensionValue,
    raw: f64,
    what: &str,
) -> Result<(), DrawingError> {
    let matches_value = match cached {
        ResolvedDimensionValue::LengthMm { mm } => raw.is_finite() && (raw - mm).abs() <= 1.0e-9,
        ResolvedDimensionValue::Count { count } => {
            raw.is_finite() && raw.fract() == 0.0 && raw >= 1.0 && raw as u32 == *count
        }
    };
    if matches_value {
        Ok(())
    } else {
        Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "{} cached value does not match its engineering source (cached {:?}, source {})",
                what, cached, raw
            ),
        })
    }
}

/// Validate one derived drawing specification against the authoritative
/// EngineeringModel entry. Formatted text is intentionally not part of this
/// check: it is recomputed by the shared formatter at render/layout time.
fn validate_drawing_specification(
    dimension_id: &DrawingDimensionId,
    model: &EngineeringModel,
    specification: Option<&DrawingDimensionSpecification>,
) -> Result<(), DrawingError> {
    let Some(specification) = specification else {
        return Ok(());
    };
    specification
        .reference
        .validate_for_model(model)
        .map_err(|error| DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} has an invalid specification target: {}",
                dimension_id, error
            ),
        })?;
    specification
        .value
        .validate()
        .map_err(|error| DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} has an invalid specification value: {}",
                dimension_id, error
            ),
        })?;
    let canonical = specification
        .value
        .clone()
        .canonicalize()
        .map_err(|error| DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} has a non-canonical specification: {}",
                dimension_id, error
            ),
        })?;
    if canonical != specification.value {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} specification is not in canonical form",
                dimension_id
            ),
        });
    }
    let Some(authoritative) = model
        .dimensional_specifications
        .get(&specification.reference)
    else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} carries a specification absent from EngineeringModel",
                dimension_id
            ),
        });
    };
    if authoritative != &specification.value {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} carries a stale specification for {}",
                dimension_id, specification.reference
            ),
        });
    }
    Ok(())
}

/// Validate the dimensions attached to a view, fail-closed.
///
/// Checks per dimension: the target feature exists; kind/value correlation
/// (`Pattern` ⟺ `DimensionValue::Pattern`); valid resolved caches; every
/// source re-resolves against the engineering model to exactly the cached
/// value (a tampered model cannot pass silently); `FeatureSlot` sources name a
/// slot that is valid for their feature; `Parameter` sources exist.
pub fn validate_dimension_integrity(
    view: &DrawingView,
    model: &EngineeringModel,
) -> Result<(), DrawingError> {
    for (dim_id, dimension) in &view.dimensions {
        validate_drawing_specification(dim_id, model, dimension.specification.as_ref())?;
        let DrawingDimensionTarget::Feature { feature } = &dimension.target else {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "dimension {} body target requires validated parametric spatial evidence",
                    dim_id
                ),
            });
        };
        if !model.features.contains_key(feature) {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "dimension {} targets feature {} which does not exist",
                    dim_id, feature
                ),
            });
        }
        let value_matches_kind = matches!(
            (&dimension.kind, &dimension.value),
            (
                DrawingDimensionKind::Linear { .. },
                DimensionValue::Measured { .. }
            ) | (
                DrawingDimensionKind::Diameter | DrawingDimensionKind::OverallDiameter,
                DimensionValue::Measured { .. }
            ) | (
                DrawingDimensionKind::Radius,
                DimensionValue::Measured { .. }
            ) | (
                DrawingDimensionKind::Pattern,
                DimensionValue::Pattern { .. }
            ) | (
                DrawingDimensionKind::Diameter,
                DimensionValue::HoleCallout { pattern: None, .. },
            ) | (
                DrawingDimensionKind::Pattern,
                DimensionValue::HoleCallout {
                    pattern: Some(_),
                    ..
                },
            ) | (
                DrawingDimensionKind::SlotCallout,
                DimensionValue::SlotCallout { .. },
            )
        );
        if !value_matches_kind {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!("dimension {} kind and value disagree", dim_id),
            });
        }

        let check_source_slot = |source: &DimensionValueSource| -> Result<(), DrawingError> {
            match source {
                DimensionValueSource::EngineeringFeatureParameter { feature, parameter } => {
                    if parametric_feature_parameter_value(model, feature, *parameter).is_none() {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "dimension {} has an unresolved feature parameter {:?} on {}",
                                dim_id, parameter, feature
                            ),
                        });
                    }
                }
                DimensionValueSource::VerifiedBodyGeometry { .. } => {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!("dimension {} body geometry must be validated with ParametricSpatialEvidence", dim_id),
                    });
                }
                DimensionValueSource::FeatureSlot { feature, slot } => {
                    let Some(f) = model.features.get(feature) else {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "dimension {} sources missing feature {}",
                                dim_id, feature
                            ),
                        });
                    };
                    if !FeatureParameterSlot::valid_for(f).contains(slot) {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "dimension {} sources slot {} which is not valid for feature {}",
                                dim_id, slot, feature
                            ),
                        });
                    }
                }
                DimensionValueSource::ResolvedGeometry { feature, .. }
                | DimensionValueSource::ResolvedPatternSemantic { feature, .. }
                | DimensionValueSource::ResolvedPatternSpacing { feature, .. }
                | DimensionValueSource::ResolvedPatternLocation { feature, .. }
                | DimensionValueSource::ResolvedHoleDepth { feature, .. } => {
                    if !model.features.contains_key(feature) {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "dimension {} sources missing feature {}",
                                dim_id, feature
                            ),
                        });
                    }
                }
                DimensionValueSource::ResolvedHoleLocation {
                    feature,
                    datum_feature,
                    ..
                } => {
                    if !model.features.contains_key(feature)
                        || !model.features.contains_key(datum_feature)
                    {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "dimension {} has an unresolved hole-location source",
                                dim_id
                            ),
                        });
                    }
                }
                DimensionValueSource::Parameter { .. } => {}
            }
            Ok(())
        };

        match &dimension.value {
            DimensionValue::Measured { source, resolved } => {
                check_source_slot(source)?;
                if !resolved.is_valid() {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!("dimension {} has an invalid resolved value", dim_id),
                    });
                }
                if !matches!(
                    source,
                    DimensionValueSource::ResolvedGeometry { .. }
                        | DimensionValueSource::ResolvedHoleLocation { .. }
                        | DimensionValueSource::ResolvedPatternSpacing { .. }
                        | DimensionValueSource::ResolvedPatternLocation { .. }
                        | DimensionValueSource::ResolvedPatternSemantic { .. }
                        | DimensionValueSource::ResolvedHoleDepth { .. }
                ) {
                    let raw = raw_value_from_source(model, source)?;
                    check_matches_cached(resolved, raw, &format!("dimension {}", dim_id))?;
                }
            }
            DimensionValue::Pattern { shape } => {
                for component in shape.components() {
                    validate_drawing_specification(
                        dim_id,
                        model,
                        component.specification.as_ref(),
                    )?;
                    check_source_slot(&component.source)?;
                    if !component.resolved.is_valid() {
                        return Err(DrawingError::InvalidDrawingDimension {
                            reason: format!(
                                "dimension {} has an invalid pattern component",
                                dim_id
                            ),
                        });
                    }
                    if !matches!(
                        component.source,
                        DimensionValueSource::ResolvedGeometry { .. }
                            | DimensionValueSource::ResolvedHoleLocation { .. }
                            | DimensionValueSource::ResolvedPatternSpacing { .. }
                            | DimensionValueSource::ResolvedPatternLocation { .. }
                            | DimensionValueSource::ResolvedPatternSemantic { .. }
                            | DimensionValueSource::ResolvedHoleDepth { .. }
                    ) {
                        let raw = raw_value_from_source(model, &component.source)?;
                        check_matches_cached(
                            &component.resolved,
                            raw,
                            &format!("dimension {}", dim_id),
                        )?;
                    }
                }
            }
            DimensionValue::HoleCallout { semantic, pattern } => {
                validate_hole_callout_semantic(dim_id, feature, semantic, pattern.as_ref(), model)?;
            }
            DimensionValue::ParametricHoleCallout { semantic } => {
                if semantic.feature != *feature {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!(
                            "parametric callout {} is detached from its target feature",
                            dim_id
                        ),
                    });
                }
                for source in dimension.sources() {
                    check_source_slot(source)?;
                }
                if !semantic.diameter.resolved.is_valid()
                    || matches!(semantic.extent, ParametricHoleExtent::Blind { ref depth } if !depth.resolved.is_valid())
                    || semantic.counterbore.as_ref().is_some_and(|cb| {
                        !cb.diameter.resolved.is_valid() || !cb.depth.resolved.is_valid()
                    })
                {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!(
                            "parametric callout {} contains an invalid component",
                            dim_id
                        ),
                    });
                }
            }
            DimensionValue::SlotCallout { semantic } => {
                if &semantic.feature != feature {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!("slot callout {} is detached from its feature", dim_id),
                    });
                }
                check_source_slot(&semantic.overall_length.source)?;
                check_source_slot(&semantic.width.source)?;
                if !semantic.overall_length.resolved.is_valid()
                    || !semantic.width.resolved.is_valid()
                {
                    return Err(DrawingError::InvalidDrawingDimension {
                        reason: format!("slot callout {} has an invalid resolved value", dim_id),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_hole_callout_semantic(
    dimension_id: &DrawingDimensionId,
    target_feature: &FeatureId,
    semantic: &HoleCalloutSemantic,
    pattern: Option<&PatternShape>,
    model: &EngineeringModel,
) -> Result<(), DrawingError> {
    validate_drawing_specification(
        dimension_id,
        model,
        semantic.diameter.specification.as_ref(),
    )?;
    if let Some(quantity) = &semantic.quantity {
        validate_drawing_specification(dimension_id, model, quantity.specification.as_ref())?;
    }
    if let HoleCalloutExtent::Blind { depth, .. } = &semantic.extent {
        validate_drawing_specification(dimension_id, model, depth.specification.as_ref())?;
    }
    if let Some(pattern) = pattern {
        for component in pattern.components() {
            validate_drawing_specification(dimension_id, model, component.specification.as_ref())?;
        }
    }
    let provenance = &semantic.provenance;
    if provenance.feature != *target_feature {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} callout provenance does not match its target feature",
                dimension_id
            ),
        });
    }
    if provenance.witnesses.is_empty() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} has no hole witnesses", dimension_id),
        });
    }
    if !model.features.contains_key(&provenance.feature) {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} callout source feature {} is missing",
                dimension_id, provenance.feature
            ),
        });
    }
    if !provenance
        .witnesses
        .iter()
        .any(|witness| witness.entity == provenance.representative_entity)
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} representative entity is not in its witnesses",
                dimension_id
            ),
        });
    }
    let mut witness_entities = BTreeSet::new();
    for witness in &provenance.witnesses {
        if !witness_entities.insert(witness.entity.clone()) {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!("dimension {} has duplicate hole witnesses", dimension_id),
            });
        }
        if !witness.radius.is_finite()
            || witness.radius <= 0.0
            || !witness.z.min.is_finite()
            || !witness.z.max.is_finite()
            || witness.z.max <= witness.z.min
        {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!("dimension {} has an invalid hole witness", dimension_id),
            });
        }
    }

    let DimensionValueSource::ResolvedGeometry {
        feature: diameter_feature,
        entity: diameter_entity,
    } = &semantic.diameter.source
    else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} diameter is not geometry-resolved",
                dimension_id
            ),
        });
    };
    if diameter_feature != &provenance.feature
        || diameter_entity != &provenance.representative_entity
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} diameter provenance is detached", dimension_id),
        });
    }
    let ResolvedDimensionValue::LengthMm { mm: diameter } = semantic.diameter.resolved else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} diameter is not a length", dimension_id),
        });
    };
    if !semantic.diameter.resolved.is_valid() {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} diameter is invalid", dimension_id),
        });
    }
    let representative = provenance
        .witnesses
        .iter()
        .find(|witness| witness.entity == provenance.representative_entity)
        .expect("representative witness checked above");
    if (diameter - 2.0 * representative.radius).abs() > 1.0e-6 {
        return Err(DrawingError::InvalidDimensionWitness {
            reason: format!(
                "dimension {} diameter disagrees with its cut witness",
                dimension_id
            ),
        });
    }

    match (pattern, &semantic.quantity) {
        (Some(PatternShape::Circular { count, .. }), Some(quantity)) => {
            if quantity.source
                != (DimensionValueSource::ResolvedPatternSemantic {
                    feature: provenance.feature.clone(),
                    field: PatternSemanticField::Count,
                })
                || quantity.resolved != count.resolved
                || !matches!(quantity.resolved, ResolvedDimensionValue::Count { .. })
                || !quantity.resolved.is_valid()
            {
                return Err(DrawingError::InvalidDrawingDimension {
                    reason: format!(
                        "dimension {} quantity disagrees with its pattern semantic",
                        dimension_id
                    ),
                });
            }
        }
        (Some(PatternShape::Grid { .. } | PatternShape::Corner { .. }), None) | (None, None) => {}
        _ => {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "dimension {} pattern/quantity correlation is invalid",
                    dimension_id
                ),
            })
        }
    }
    if let Some(shape) = pattern {
        match shape {
            PatternShape::Circular {
                count,
                pitch_circle_diameter,
                hole_diameter,
            } => {
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    count,
                    PatternSemanticField::Count,
                )?;
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    pitch_circle_diameter,
                    PatternSemanticField::PitchCircleDiameter,
                )?;
                validate_hole_callout_pattern_geometry(
                    dimension_id,
                    &provenance.feature,
                    hole_diameter,
                    &witness_entities,
                )?;
            }
            PatternShape::Grid {
                rows,
                columns,
                spacing_x,
                spacing_y,
                hole_diameter,
            } => {
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    rows,
                    PatternSemanticField::Rows,
                )?;
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    columns,
                    PatternSemanticField::Columns,
                )?;
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    spacing_x,
                    PatternSemanticField::SpacingX,
                )?;
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    spacing_y,
                    PatternSemanticField::SpacingY,
                )?;
                validate_hole_callout_pattern_geometry(
                    dimension_id,
                    &provenance.feature,
                    hole_diameter,
                    &witness_entities,
                )?;
            }
            PatternShape::Corner {
                offset_x,
                offset_y,
                hole_diameter,
            } => {
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    offset_x,
                    PatternSemanticField::OffsetX,
                )?;
                validate_hole_callout_pattern_semantic(
                    dimension_id,
                    &provenance.feature,
                    offset_y,
                    PatternSemanticField::OffsetY,
                )?;
                validate_hole_callout_pattern_geometry(
                    dimension_id,
                    &provenance.feature,
                    hole_diameter,
                    &witness_entities,
                )?;
            }
        }
    }

    if let HoleCalloutExtent::Blind { depth, .. } = &semantic.extent {
        let DimensionValueSource::ResolvedHoleDepth {
            feature: depth_feature,
            entity: depth_entity,
        } = &depth.source
        else {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!(
                    "dimension {} depth is not Spatial-witness resolved",
                    dimension_id
                ),
            });
        };
        if depth_feature != &provenance.feature || depth_entity != &provenance.representative_entity
        {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!("dimension {} depth provenance is detached", dimension_id),
            });
        }
        if !matches!(depth.resolved, ResolvedDimensionValue::LengthMm { .. })
            || !depth.resolved.is_valid()
        {
            return Err(DrawingError::InvalidDrawingDimension {
                reason: format!("dimension {} depth is invalid", dimension_id),
            });
        }
    }
    Ok(())
}

fn validate_hole_callout_pattern_semantic(
    dimension_id: &DrawingDimensionId,
    feature: &FeatureId,
    component: &DimensionComponent,
    field: PatternSemanticField,
) -> Result<(), DrawingError> {
    if component.source
        != (DimensionValueSource::ResolvedPatternSemantic {
            feature: feature.clone(),
            field,
        })
        || !component.resolved.is_valid()
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} pattern component is not typed as {:?}",
                dimension_id, field
            ),
        });
    }
    Ok(())
}

fn validate_hole_callout_pattern_geometry(
    dimension_id: &DrawingDimensionId,
    feature: &FeatureId,
    component: &DimensionComponent,
    witness_entities: &BTreeSet<EntityId>,
) -> Result<(), DrawingError> {
    let DimensionValueSource::ResolvedGeometry {
        feature: source_feature,
        entity,
    } = &component.source
    else {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} pattern diameter is not geometry-resolved",
                dimension_id
            ),
        });
    };
    if source_feature != feature || !witness_entities.contains(entity) {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!(
                "dimension {} pattern diameter provenance is detached",
                dimension_id
            ),
        });
    }
    if !matches!(component.resolved, ResolvedDimensionValue::LengthMm { .. })
        || !component.resolved.is_valid()
    {
        return Err(DrawingError::InvalidDrawingDimension {
            reason: format!("dimension {} pattern diameter is invalid", dimension_id),
        });
    }
    Ok(())
}

/// Pure, deterministic dimension formatting (Phase 8.8 + Phase 11.5): the
/// ephemeral *display* string for a dimension. This is presentation only —
/// the string is never stored back into a `DrawingDimension`, never
/// authoritative, and changes automatically when the underlying resolved
/// value or typed specification changes.
///
/// `AICAD_INTERNAL_V1` is an explicit internal presentation contract. It is
/// intentionally not a claim of GB/ISO/ASME compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingFormattingProfileId {
    AicadInternalV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToleranceStyle {
    SingleLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingFormattingProfile {
    pub id: DrawingFormattingProfileId,
    /// `None` preserves the pre-11.5 adaptive nominal formatter when no
    /// explicit precision is attached to a dimension.
    pub nominal_decimal_places: Option<DecimalPlaces>,
    /// Default precision for tolerance values when a specification does not
    /// carry its own precision.
    pub tolerance_decimal_places: DecimalPlaces,
    pub diameter_symbol: char,
    pub depth_symbol: char,
    pub multiplication_symbol: char,
    pub tolerance_style: ToleranceStyle,
}

/// Frozen Phase 11.5 drawing profile. This is an internal deterministic
/// profile, not a standards-compliance mode.
pub const AICAD_INTERNAL_V1: DrawingFormattingProfile = DrawingFormattingProfile {
    id: DrawingFormattingProfileId::AicadInternalV1,
    nominal_decimal_places: None,
    tolerance_decimal_places: DecimalPlaces(2),
    diameter_symbol: '\u{00D8}',
    depth_symbol: '\u{21A7}',
    multiplication_symbol: '\u{00D7}',
    tolerance_style: ToleranceStyle::SingleLine,
};

/// The legacy adaptive number formatter. Its `format!("{value:.12}")`
/// behavior is deliberately retained for no-tolerance/no-precision output so
/// Phase 11.4 goldens remain byte-identical.
fn num(value: f64) -> String {
    scc_geometry::svg::number(value)
}

/// Format a finite number with exactly the requested decimal places. Rust's
/// current fixed-point formatter is the rounding authority for Phase 11.5.
fn fixed_number(value: f64, places: DecimalPlaces) -> String {
    let mut rendered = format!("{value:.precision$}", precision = places.0 as usize);
    if rendered.starts_with("-0")
        && rendered[1..]
            .chars()
            .all(|character| character == '0' || character == '.')
    {
        rendered.remove(0);
    }
    rendered
}

fn formatted_number(
    value: f64,
    precision: Option<DecimalPlaces>,
    profile: &DrawingFormattingProfile,
) -> String {
    let precision = precision.or(profile.nominal_decimal_places);
    match precision {
        Some(places) => fixed_number(value, places),
        None => num(value),
    }
}

fn formatted_tolerance(
    tolerance: &DimensionalTolerance,
    precision: Option<DecimalPlaces>,
    profile: &DrawingFormattingProfile,
) -> String {
    let places = precision.unwrap_or(profile.tolerance_decimal_places);
    match tolerance {
        DimensionalTolerance::None => String::new(),
        DimensionalTolerance::Symmetric { plus_minus } => match profile.tolerance_style {
            ToleranceStyle::SingleLine => {
                format!(" ±{}", fixed_number(*plus_minus, places))
            }
        },
        DimensionalTolerance::Bilateral { upper, lower } => match profile.tolerance_style {
            ToleranceStyle::SingleLine => format!(
                " +{}/-{}",
                fixed_number(*upper, places),
                fixed_number(lower.abs(), places)
            ),
        },
    }
}

fn formatted_measurement(
    value: f64,
    specification: Option<&DrawingDimensionSpecification>,
    profile: &DrawingFormattingProfile,
) -> String {
    let precision = specification.and_then(|item| item.value.precision);
    let tolerance = specification
        .map(|item| &item.value.tolerance)
        .unwrap_or(&DimensionalTolerance::None);
    format!(
        "{}{}",
        formatted_number(value, precision, profile),
        formatted_tolerance(tolerance, precision, profile)
    )
}

fn formatted_component(
    component: &DimensionComponent,
    profile: &DrawingFormattingProfile,
) -> String {
    let ResolvedDimensionValue::LengthMm { mm } = component.resolved else {
        return String::new();
    };
    if !mm.is_finite() || mm <= 0.0 {
        return String::new();
    }
    formatted_measurement(mm, component.specification.as_ref(), profile)
}

fn count_text(component: &DimensionComponent) -> String {
    match component.resolved {
        ResolvedDimensionValue::Count { count } => count.to_string(),
        ResolvedDimensionValue::LengthMm { mm } => num(mm),
    }
}

pub fn format_dimension(dimension: &DrawingDimension) -> String {
    format_dimension_with_profile(dimension, &AICAD_INTERNAL_V1)
}

/// Format a dimension using an explicit frozen profile. This is useful for
/// deterministic tests and future profile negotiation without introducing a
/// second layout engine.
pub fn format_dimension_with_profile(
    dimension: &DrawingDimension,
    profile: &DrawingFormattingProfile,
) -> String {
    match (&dimension.kind, &dimension.value) {
        (
            DrawingDimensionKind::Diameter | DrawingDimensionKind::OverallDiameter,
            DimensionValue::Measured {
                resolved: ResolvedDimensionValue::LengthMm { mm },
                ..
            },
        ) => format!(
            "{}{}",
            profile.diameter_symbol,
            formatted_measurement(*mm, dimension.specification.as_ref(), profile)
        ),
        (
            DrawingDimensionKind::Diameter,
            DimensionValue::HoleCallout {
                semantic,
                pattern: None,
            },
        ) => format_hole_callout_with_profile(semantic, None, profile),
        (
            DrawingDimensionKind::Diameter | DrawingDimensionKind::Pattern,
            DimensionValue::ParametricHoleCallout { semantic },
        ) => format_parametric_hole_callout(semantic, profile),
        (
            DrawingDimensionKind::Radius,
            DimensionValue::Measured {
                resolved: ResolvedDimensionValue::LengthMm { mm },
                ..
            },
        ) => format!(
            "R{}",
            formatted_measurement(*mm, dimension.specification.as_ref(), profile)
        ),
        (
            DrawingDimensionKind::Linear { .. },
            DimensionValue::Measured {
                resolved: ResolvedDimensionValue::LengthMm { mm },
                ..
            },
        ) => formatted_measurement(*mm, dimension.specification.as_ref(), profile),
        (DrawingDimensionKind::SlotCallout, DimensionValue::SlotCallout { semantic }) => {
            format_slot_callout(semantic, profile)
        }
        (DrawingDimensionKind::Pattern, DimensionValue::Pattern { shape }) => {
            format_pattern_dimension(shape, profile)
        }
        (
            DrawingDimensionKind::Pattern,
            DimensionValue::HoleCallout {
                semantic,
                pattern: Some(pattern),
            },
        ) => format_hole_callout_with_profile(semantic, Some(pattern), profile),
        _ => String::new(),
    }
}

fn format_slot_callout(
    semantic: &SlotCalloutSemantic,
    profile: &DrawingFormattingProfile,
) -> String {
    let (
        ResolvedDimensionValue::LengthMm { mm: overall_length },
        ResolvedDimensionValue::LengthMm { mm: width },
    ) = (semantic.overall_length.resolved, semantic.width.resolved)
    else {
        return String::new();
    };
    if !overall_length.is_finite() || !width.is_finite() || overall_length <= width || width <= 0.0
    {
        return String::new();
    }
    format!(
        "SLOT {} {} {} THRU",
        formatted_measurement(
            overall_length,
            semantic.overall_length.specification.as_ref(),
            profile
        ),
        profile.multiplication_symbol,
        formatted_measurement(width, semantic.width.specification.as_ref(), profile)
    )
}

/// Format typed hole semantics only at the presentation boundary.
pub fn format_hole_callout(
    semantic: &HoleCalloutSemantic,
    pattern: Option<&PatternShape>,
) -> String {
    format_hole_callout_with_profile(semantic, pattern, &AICAD_INTERNAL_V1)
}

fn format_hole_callout_with_profile(
    semantic: &HoleCalloutSemantic,
    pattern: Option<&PatternShape>,
    profile: &DrawingFormattingProfile,
) -> String {
    let ResolvedDimensionValue::LengthMm { mm: diameter } = semantic.diameter.resolved else {
        return String::new();
    };
    if !diameter.is_finite() || diameter <= 0.0 {
        return String::new();
    }
    let quantity = match pattern {
        None => String::new(),
        Some(PatternShape::Circular { .. }) => match &semantic.quantity {
            Some(quantity) => match quantity.resolved {
                ResolvedDimensionValue::Count { count } if count >= 1 => {
                    format!("{}{} ", count, profile.multiplication_symbol)
                }
                _ => return String::new(),
            },
            None => return String::new(),
        },
        Some(PatternShape::Grid { rows, columns, .. }) => {
            let (
                ResolvedDimensionValue::Count { count: rows },
                ResolvedDimensionValue::Count { count: columns },
            ) = (rows.resolved, columns.resolved)
            else {
                return String::new();
            };
            if rows < 1 || columns < 1 {
                return String::new();
            }
            format!("{}{}{} ", rows, profile.multiplication_symbol, columns)
        }
        Some(PatternShape::Corner { .. }) => {
            format!("4{} ", profile.multiplication_symbol)
        }
    };
    let extent = match &semantic.extent {
        HoleCalloutExtent::Through => "THRU".to_string(),
        HoleCalloutExtent::Blind { depth, side } => {
            let ResolvedDimensionValue::LengthMm { mm: depth_mm } = depth.resolved else {
                return String::new();
            };
            if !depth_mm.is_finite() || depth_mm <= 0.0 {
                return String::new();
            }
            let side = match side {
                HoleStartFace::Base => "BASE",
                HoleStartFace::Opposite => "OPP",
            };
            format!(
                "{}{} {}",
                profile.depth_symbol,
                formatted_component(depth, profile),
                side
            )
        }
    };
    let mut rendered = format!(
        "{}{}{} {}",
        quantity,
        profile.diameter_symbol,
        formatted_component(&semantic.diameter, profile),
        extent
    );
    // Phase 11.4 intentionally omitted PCD from hole callouts. Preserve that
    // no-tolerance contract; expose PCD in the callout only when the
    // EngineeringModel carries a PCD precision/tolerance specification.
    if let Some(PatternShape::Circular {
        pitch_circle_diameter,
        ..
    }) = pattern
    {
        if semantic.show_pitch_circle_diameter || pitch_circle_diameter.specification.is_some() {
            rendered.push_str(" PCD ");
            rendered.push(profile.diameter_symbol);
            rendered.push_str(&formatted_component(pitch_circle_diameter, profile));
        }
    }
    rendered
}

fn format_parametric_hole_callout(
    semantic: &ParametricHoleCalloutSemantic,
    profile: &DrawingFormattingProfile,
) -> String {
    let ResolvedDimensionValue::LengthMm { mm: diameter } = semantic.diameter.resolved else {
        return String::new();
    };
    if !diameter.is_finite() || diameter <= 0.0 {
        return String::new();
    }
    let mut output = String::new();
    if let Some(pattern) = &semantic.pattern {
        let count = match pattern {
            ParametricHolePatternCallout::Linear { count, .. }
            | ParametricHolePatternCallout::Circular { count, .. } => match count.resolved {
                ResolvedDimensionValue::Count { count } if count > 0 => count,
                _ => return String::new(),
            },
        };
        output.push_str(&format!("{}{} ", count, profile.multiplication_symbol));
    }
    output.push(profile.diameter_symbol);
    output.push_str(&formatted_component(&semantic.diameter, profile));
    match &semantic.extent {
        ParametricHoleExtent::Through => output.push_str(" THRU"),
        ParametricHoleExtent::Blind { depth } => {
            let ResolvedDimensionValue::LengthMm { mm } = depth.resolved else {
                return String::new();
            };
            if !mm.is_finite() || mm <= 0.0 {
                return String::new();
            }
            output.push(' ');
            output.push(profile.depth_symbol);
            output.push_str(&formatted_component(depth, profile));
        }
    }
    if let Some(counterbore) = &semantic.counterbore {
        let ResolvedDimensionValue::LengthMm { mm: dia } = counterbore.diameter.resolved else {
            return String::new();
        };
        let ResolvedDimensionValue::LengthMm { mm: depth } = counterbore.depth.resolved else {
            return String::new();
        };
        if !dia.is_finite() || dia <= 0.0 || !depth.is_finite() || depth <= 0.0 {
            return String::new();
        }
        output.push_str(" ⌴ ");
        output.push(profile.diameter_symbol);
        output.push_str(&formatted_component(&counterbore.diameter, profile));
        output.push(' ');
        output.push(profile.multiplication_symbol);
        output.push(' ');
        output.push_str(&formatted_component(&counterbore.depth, profile));
    }
    if let Some(pattern) = &semantic.pattern {
        match pattern {
            ParametricHolePatternCallout::Linear { spacing, .. } => {
                output.push_str(" P=");
                output.push_str(&formatted_component(spacing, profile));
            }
            ParametricHolePatternCallout::Circular { radius, .. } => {
                let ResolvedDimensionValue::LengthMm { mm } = radius.resolved else {
                    return String::new();
                };
                if !mm.is_finite() || mm <= 0.0 {
                    return String::new();
                }
                output.push_str(" PCD ");
                output.push(profile.diameter_symbol);
                output.push_str(&num(mm * 2.0));
            }
        }
    }
    output
}

fn format_pattern_dimension(shape: &PatternShape, profile: &DrawingFormattingProfile) -> String {
    match shape {
        PatternShape::Circular {
            count,
            pitch_circle_diameter,
            hole_diameter,
        } => format!(
            "{}{}{}{} PCD {}{}",
            count_text(count),
            profile.multiplication_symbol,
            profile.diameter_symbol,
            formatted_component(hole_diameter, profile),
            profile.diameter_symbol,
            formatted_component(pitch_circle_diameter, profile)
        ),
        PatternShape::Grid {
            rows,
            columns,
            spacing_x,
            spacing_y,
            hole_diameter,
        } => format!(
            "{}{}{} {}{} X{} Y{}",
            count_text(rows),
            profile.multiplication_symbol,
            count_text(columns),
            profile.diameter_symbol,
            formatted_component(hole_diameter, profile),
            formatted_component(spacing_x, profile),
            formatted_component(spacing_y, profile)
        ),
        PatternShape::Corner {
            offset_x,
            offset_y,
            hole_diameter,
        } => format!(
            "4{}{}{} OFF X{} Y{}",
            profile.multiplication_symbol,
            profile.diameter_symbol,
            formatted_component(hole_diameter, profile),
            formatted_component(offset_x, profile),
            formatted_component(offset_y, profile)
        ),
    }
}
