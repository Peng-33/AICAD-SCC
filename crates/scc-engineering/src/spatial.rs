//! Engineering spatial semantics (Phase 9.1, ADR-032).
//!
//! This module stores **spatial intent** — plate thickness and per-hole
//! extents — as data on the [`EngineeringModel`], plus a deterministic
//! readiness check over that data. It performs no geometry computation and no
//! lowering: like the rest of the engineering layer, it describes *what the
//! design means*, never how it is solved or rendered.
//!
//! Design rules (ADR-032):
//! - Every `Hole` / `HolePattern` must declare an extent before any
//!   spatial consumer runs (`MissingHoleExtent`, fail-closed).
//! - A blind depth may not reach or exceed the plate thickness
//!   (`InvalidBlindDepth`); parameter-mediated conflicts are caught by the
//!   readiness check, not guessed at by consumers.
//! - Features without spatial meaning in this phase (`Fillet`) are
//!   reported as [`SpatialIssue::UnsupportedForSpatial`] instead of being
//!   silently ignored.
//! - The readiness report is deterministically ordered: identical models
//!   always produce identical issue sequences.
//!
//! # Canonical Part Local Frame (domain contract)
//!
//! All spatial data below is interpreted in one canonical part-local frame:
//! +X and +Y follow the existing `EngineeringModel` / `GeometryModel` axes
//! (right / up); +Z is perpendicular to the primary sketch plane, pointing out
//! of the material. The primary sketch plane is XY at `z = 0` — the **Base**
//! face ([`HoleStartFace::Base`]); the **Opposite** face lies at
//! `z = part_thickness` ([`HoleStartFace::Opposite`]). The plate solid is the
//! `BaseProfile` extruded along +Z over `[0, thickness]`; the extrusion
//! direction is fixed (`PositiveZ`) as a 2.5-D domain contract. Future
//! projection consumers may only use this convention, never redefine it;
//! the terms Base/Opposite deliberately avoid Front/Back to stay clear of
//! future projection-view names.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::evaluator::ResolvedValue;
use crate::feature::EngineeringFeature;
use crate::model::EngineeringModel;
use crate::{EngineeringError, FeatureId, ParameterId};

/// A length value together with its provenance: either a literal dimension or
/// a reference to a model parameter (resolved at evaluation time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SourcedLength {
    /// A concrete dimension in millimetres.
    Literal { value: f64 },
    /// Driven by an existing model parameter.
    Parameter { id: ParameterId },
}

/// The face from which a blind hole starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleStartFace {
    /// The face the feature was placed on (the base face).
    Base,
    /// The opposite face of the part.
    Opposite,
}

/// How deep a hole goes through the part.
///
/// `ThroughAll` needs no thickness; `Blind` carries a start face plus a depth
/// that must stay strictly inside the material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoleExtent {
    /// The hole passes entirely through the part.
    ThroughAll,
    /// The hole stops after `depth` millimetres from `start_face`.
    Blind {
        start_face: HoleStartFace,
        depth: SourcedLength,
    },
}

/// Spatial intent attached to the whole part (Phase 9.1 scope: thickness and
/// hole extents only — no positions, no draft, no pockets).
///
/// Stored on [`EngineeringModel::spatial`] with `#[serde(default,
/// skip_serializing_if = "PartSpatialDefinition::is_empty")]` so Phase-8
/// documents deserialize unchanged and empty definitions never appear in
/// serialized output (byte-level back compatibility).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartSpatialDefinition {
    /// Nominal plate thickness, if declared.
    pub thickness: Option<SourcedLength>,
    /// Extent declaration per hole-bearing feature id.
    pub hole_extents: BTreeMap<FeatureId, HoleExtent>,
}

impl PartSpatialDefinition {
    /// Whether no spatial intent is stored at all.
    pub fn is_empty(&self) -> bool {
        self.thickness.is_none() && self.hole_extents.is_empty()
    }
}

/// One problem found by [`EngineeringModel::validate_spatial_readiness`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpatialIssue {
    /// No thickness is declared although spatial data requires one.
    MissingThickness,
    /// The declared literal thickness is non-finite or non-positive.
    InvalidThickness { value: f64 },
    /// A `Hole` / `HolePattern` has no extent entry (fail-closed).
    MissingHoleExtent { feature: FeatureId },
    /// A blind depth resolves to a value at or beyond the plate thickness
    /// (or to an invalid literal).
    InvalidBlindDepth { feature: FeatureId, depth: f64 },
    /// A parameter referenced by a sourced length cannot be resolved to a
    /// finite positive number.
    UnresolvableParameter { parameter: ParameterId },
    /// The feature exists but has no spatial meaning in this phase
    /// (`Slot`, `Fillet`); fail-closed instead of silently ignored.
    UnsupportedForSpatial { feature: FeatureId },
    /// An extent entry targets a feature id that does not exist (or is not a
    /// hole-bearing feature).
    UnknownHoleExtentTarget { feature: FeatureId },
}

impl SpatialIssue {
    /// Stable category order used by the deterministic report sort.
    fn discriminant(&self) -> u8 {
        match self {
            SpatialIssue::MissingThickness => 0,
            SpatialIssue::InvalidThickness { .. } => 1,
            SpatialIssue::MissingHoleExtent { .. } => 2,
            SpatialIssue::InvalidBlindDepth { .. } => 3,
            SpatialIssue::UnresolvableParameter { .. } => 4,
            SpatialIssue::UnsupportedForSpatial { .. } => 5,
            SpatialIssue::UnknownHoleExtentTarget { .. } => 6,
        }
    }

    /// The affected feature, when the issue is about one; secondary sort key.
    fn sort_feature(&self) -> Option<&FeatureId> {
        match self {
            SpatialIssue::MissingThickness | SpatialIssue::InvalidThickness { .. } => None,
            SpatialIssue::UnresolvableParameter { .. } => None,
            SpatialIssue::MissingHoleExtent { feature }
            | SpatialIssue::InvalidBlindDepth { feature, .. }
            | SpatialIssue::UnsupportedForSpatial { feature }
            | SpatialIssue::UnknownHoleExtentTarget { feature } => Some(feature),
        }
    }
}

/// Deterministic result of the spatial readiness check. `issues` is sorted by
/// `(category, feature)` so identical models always yield identical reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialReadiness {
    /// `true` exactly when there are no issues.
    pub ready: bool,
    /// Deterministically ordered issues (empty when ready).
    pub issues: Vec<SpatialIssue>,
}

/// Resolve a parameter reference to a finite, strictly positive number of
/// millimetres. Any evaluation failure, boolean/choice value, unresolved
/// state, or non-positive/non-finite outcome maps to `Err(parameter)`.
fn resolve_parameter_mm(model: &EngineeringModel, id: &ParameterId) -> Result<f64, ParameterId> {
    let resolved = model.evaluate_parameter(id).map_err(|_| id.clone())?;
    match resolved {
        ResolvedValue::Scalar(value) if value.is_finite() && value > 0.0 => Ok(value),
        other => match other.as_f64() {
            Ok(Some(value)) if value.is_finite() && value > 0.0 => Ok(value),
            _ => Err(id.clone()),
        },
    }
}

/// Single owner of the eager literal-vs-literal blind-depth invariant
/// (ADR-032 Decision 5, as amended): a blind depth must be strictly below
/// the plate thickness; equality rejects. Shared by `SetHoleExtent` and
/// `SetPartThickness` so both command directions enforce one rule.
///
/// Non-finite inputs are deliberately not rejected here (`NaN >= t` is
/// `false`, so they fall through); they can only enter the model via direct
/// in-process construction and are reported by
/// `validate_spatial_readiness` as `InvalidThickness` / `InvalidBlindDepth`.
pub(crate) fn check_blind_depth_against_thickness(
    feature_id: &FeatureId,
    depth: f64,
    thickness: f64,
) -> Result<(), EngineeringError> {
    if depth >= thickness {
        return Err(EngineeringError::DepthExceedsThickness {
            feature_id: feature_id.clone(),
            depth,
            thickness,
        });
    }
    Ok(())
}

fn sort_issues_deterministically(issues: &mut [SpatialIssue]) {
    // Stable sort: equal keys keep collection order, which is itself
    // deterministic (BTreeMap iteration), so the total order is stable across
    // runs for identical inputs.
    issues.sort_by(|a, b| {
        (a.discriminant(), a.sort_feature()).cmp(&(b.discriminant(), b.sort_feature()))
    });
}

impl EngineeringModel {
    /// Check whether the model's spatial semantics are complete and
    /// self-consistent (Phase 9.1, ADR-032).
    ///
    /// Checks performed:
    /// - thickness present, valid, and resolvable (when parameter-driven);
    /// - every extent target exists and is a `Hole` / `HolePattern`;
    /// - blind depths resolve and stay strictly below the thickness
    ///   (cross-check skipped when thickness is missing/unresolvable — that
    ///   failure is reported separately);
    /// - every `Hole` / `HolePattern` in the model declares an extent
    ///   (fail-closed);
    /// - every `Fillet` in the model is reported as unsupported.
    ///
    /// Purely read-only and deterministic: identical models produce identical
    /// reports.
    pub fn validate_spatial_readiness(&self) -> SpatialReadiness {
        let mut issues: Vec<SpatialIssue> = Vec::new();

        // Thickness.
        let mut resolved_thickness: Option<f64> = None;
        match &self.spatial.thickness {
            None => issues.push(SpatialIssue::MissingThickness),
            Some(SourcedLength::Literal { value }) => {
                if value.is_finite() && *value > 0.0 {
                    resolved_thickness = Some(*value);
                } else {
                    issues.push(SpatialIssue::InvalidThickness { value: *value });
                }
            }
            Some(SourcedLength::Parameter { id }) => match resolve_parameter_mm(self, id) {
                Ok(value) => resolved_thickness = Some(value),
                Err(parameter) => issues.push(SpatialIssue::UnresolvableParameter { parameter }),
            },
        }

        // Declared hole extents.
        for (feature_id, extent) in &self.spatial.hole_extents {
            let is_hole_bearing = match self.features.get(feature_id) {
                None => {
                    issues.push(SpatialIssue::UnknownHoleExtentTarget {
                        feature: feature_id.clone(),
                    });
                    continue;
                }
                Some(feature) => matches!(
                    feature,
                    EngineeringFeature::Hole { .. } | EngineeringFeature::HolePattern { .. }
                ),
            };
            if !is_hole_bearing {
                issues.push(SpatialIssue::UnknownHoleExtentTarget {
                    feature: feature_id.clone(),
                });
                continue;
            }
            if let HoleExtent::Blind { depth, .. } = extent {
                match depth {
                    SourcedLength::Literal { value } => {
                        let valid = value.is_finite() && *value > 0.0;
                        let exceeds = valid && resolved_thickness.is_some_and(|t| *value >= t);
                        if !valid || exceeds {
                            issues.push(SpatialIssue::InvalidBlindDepth {
                                feature: feature_id.clone(),
                                depth: *value,
                            });
                        }
                    }
                    SourcedLength::Parameter { id } => match resolve_parameter_mm(self, id) {
                        Ok(depth_value) => {
                            if resolved_thickness.is_some_and(|t| depth_value >= t) {
                                issues.push(SpatialIssue::InvalidBlindDepth {
                                    feature: feature_id.clone(),
                                    depth: depth_value,
                                });
                            }
                        }
                        Err(parameter) => {
                            issues.push(SpatialIssue::UnresolvableParameter { parameter })
                        }
                    },
                }
            }
        }

        // Model features: required extents and unsupported kinds.
        for (id, feature) in &self.features {
            match feature {
                EngineeringFeature::Hole { .. } | EngineeringFeature::HolePattern { .. } => {
                    if !self.spatial.hole_extents.contains_key(id) {
                        issues.push(SpatialIssue::MissingHoleExtent {
                            feature: id.clone(),
                        });
                    }
                }
                EngineeringFeature::Fillet { .. } => {
                    issues.push(SpatialIssue::UnsupportedForSpatial {
                        feature: id.clone(),
                    });
                }
                EngineeringFeature::Parametric {
                    suppressed: false, ..
                } => {
                    issues.push(SpatialIssue::UnsupportedForSpatial {
                        feature: id.clone(),
                    });
                }
                EngineeringFeature::Slot { .. }
                | EngineeringFeature::BaseProfile { .. }
                | EngineeringFeature::ConstructionCircle { .. }
                | EngineeringFeature::Parametric {
                    suppressed: true, ..
                } => {}
            }
        }

        sort_issues_deterministically(&mut issues);
        let ready = issues.is_empty();
        SpatialReadiness { ready, issues }
    }

    /// Read-only helper for downstream spatial consumers (Phase 9.2
    /// `scc-spatial`): resolve a sourced length to a finite, strictly positive
    /// number of millimetres using the single-sourced Phase 9.1 resolution
    /// rule (the same rule `validate_spatial_readiness` applies). Returns
    /// `None` for non-finite/non-positive literals and for parameters that
    /// cannot be resolved to such a value. Never mutates the model.
    pub fn resolve_sourced_length_mm(&self, source: &SourcedLength) -> Option<f64> {
        match source {
            SourcedLength::Literal { value } if value.is_finite() && *value > 0.0 => Some(*value),
            SourcedLength::Literal { .. } => None,
            SourcedLength::Parameter { id } => resolve_parameter_mm(self, id).ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::EngineeringCommand;
    use crate::executor::CommandExecutor;
    use crate::expression::Expression;
    use crate::model::{Parameter, ParameterOrigin};
    use crate::parameter::ParameterValue;
    use crate::{FeatureId, ParameterId, PartId};
    use scc_core::authority::AuthorityLevel;

    fn fid(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    fn pid(id: &str) -> ParameterId {
        ParameterId(id.into())
    }

    fn add_parameter(model: &mut EngineeringModel, id: &str, value: ParameterValue) {
        model
            .create_parameter(Parameter::new(
                pid(id),
                value,
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
    }

    /// Plate (120×80) + one hole + one corner hole pattern, no spatial data.
    fn plate_model() -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId("plate".into()),
                name: "plate".into(),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: fid("plate"),
                    name: "plate".into(),
                    profile: crate::BaseProfile::Rectangle {
                        width: 120.0,
                        height: 80.0,
                    },
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: fid("center_hole"),
                    diameter: 10.0,
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("corner_holes"),
                    pattern: crate::feature::HolePattern::Corner {
                        offset_x: 15.0,
                        offset_y: 15.0,
                        hole: crate::feature::HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        model
    }

    fn set_thickness(model: &mut EngineeringModel, source: SourcedLength) {
        CommandExecutor::apply(
            model,
            &EngineeringCommand::SetPartThickness {
                source: Some(source),
            },
        )
        .unwrap();
    }

    fn set_extent(model: &mut EngineeringModel, target: &str, extent: HoleExtent) {
        CommandExecutor::apply(
            model,
            &EngineeringCommand::SetHoleExtent {
                target: fid(target),
                extent: Some(extent),
            },
        )
        .unwrap();
    }

    fn fully_declared(thickness: f64, blind_depths: &[(&str, f64)]) -> EngineeringModel {
        let mut model = plate_model();
        set_thickness(&mut model, SourcedLength::Literal { value: thickness });
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        for (id, depth) in blind_depths {
            // Written directly so readiness tests can observe conflicts the
            // command executor would already have rejected atomically.
            model.spatial.hole_extents.insert(
                fid(id),
                HoleExtent::Blind {
                    start_face: HoleStartFace::Base,
                    depth: SourcedLength::Literal { value: *depth },
                },
            );
        }
        model
    }

    #[test]
    fn fully_declared_model_is_ready() {
        let model = fully_declared(8.0, &[("corner_holes", 4.0)]);
        let readiness = model.validate_spatial_readiness();
        assert!(readiness.ready, "unexpected issues: {:?}", readiness.issues);
        assert!(readiness.issues.is_empty());
    }

    #[test]
    fn missing_thickness_is_reported() {
        let mut model = plate_model();
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        let readiness = model.validate_spatial_readiness();
        assert!(!readiness.ready);
        assert_eq!(readiness.issues, vec![SpatialIssue::MissingThickness]);
    }

    #[test]
    fn missing_hole_extents_are_reported_fail_closed() {
        let mut model = plate_model();
        set_thickness(&mut model, SourcedLength::Literal { value: 8.0 });
        let readiness = model.validate_spatial_readiness();
        assert!(!readiness.ready);
        assert_eq!(
            readiness.issues,
            vec![
                SpatialIssue::MissingHoleExtent {
                    feature: fid("center_hole")
                },
                SpatialIssue::MissingHoleExtent {
                    feature: fid("corner_holes")
                },
            ]
        );
    }

    #[test]
    fn fillet_is_unsupported_for_spatial_while_slot_is_ready() {
        let mut model = fully_declared(8.0, &[]);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Slot {
                    id: fid("slot_1"),
                    center: crate::Point2D::new(0.0, 0.0),
                    length: 20.0,
                    width: 6.0,
                    orientation_degrees: 0.0,
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Fillet {
                    id: fid("fillet_1"),
                    radius: 3.0,
                },
            },
        )
        .unwrap();
        let readiness = model.validate_spatial_readiness();
        assert!(!readiness.ready);
        assert_eq!(
            readiness.issues,
            vec![SpatialIssue::UnsupportedForSpatial {
                feature: fid("fillet_1")
            }]
        );
    }

    #[test]
    fn blind_depth_within_thickness_from_base_face_is_ready() {
        // Base face: depth measured into [0, T]; 4 mm into an 8 mm plate.
        let model = fully_declared(8.0, &[("corner_holes", 4.0)]);
        assert!(model.validate_spatial_readiness().ready);
    }

    #[test]
    fn blind_opposite_face_semantics_still_capped_by_thickness() {
        // The readiness layer checks depth ≤ T regardless of start face; the
        // geometric interpretation of the opposite face belongs to later
        // layers. Depth 6 into T=8 is fine from either face.
        let mut model = plate_model();
        set_thickness(&mut model, SourcedLength::Literal { value: 8.0 });
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        model.spatial.hole_extents.insert(
            fid("corner_holes"),
            HoleExtent::Blind {
                start_face: HoleStartFace::Opposite,
                depth: SourcedLength::Literal { value: 6.0 },
            },
        );
        assert!(model.validate_spatial_readiness().ready);

        // …but a depth reaching the full thickness is rejected from the
        // opposite face too.
        model.spatial.hole_extents.insert(
            fid("corner_holes"),
            HoleExtent::Blind {
                start_face: HoleStartFace::Opposite,
                depth: SourcedLength::Literal { value: 8.0 },
            },
        );
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::InvalidBlindDepth {
                feature: fid("corner_holes"),
                depth: 8.0,
            }]
        );
    }

    #[test]
    fn blind_depth_equal_to_thickness_is_rejected() {
        let model = fully_declared(8.0, &[("corner_holes", 8.0)]);
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::InvalidBlindDepth {
                feature: fid("corner_holes"),
                depth: 8.0,
            }]
        );
    }

    #[test]
    fn parameter_bound_thickness_resolves() {
        let mut model = plate_model();
        add_parameter(&mut model, "thickness", ParameterValue::Scalar(10.0));
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        set_thickness(
            &mut model,
            SourcedLength::Parameter {
                id: pid("thickness"),
            },
        );
        assert!(model.validate_spatial_readiness().ready);
    }

    #[test]
    fn derived_parameter_thickness_resolves_through_dependency() {
        let mut model = plate_model();
        add_parameter(&mut model, "width", ParameterValue::Scalar(120.0));
        add_parameter(&mut model, "thickness", ParameterValue::Unresolved);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("thickness"),
                expression: Expression::divide_parameter_by_constant(pid("width"), 12.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&pid("thickness")).unwrap(),
            ResolvedValue::Scalar(10.0)
        );
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        set_thickness(
            &mut model,
            SourcedLength::Parameter {
                id: pid("thickness"),
            },
        );
        assert!(model.validate_spatial_readiness().ready);
    }

    #[test]
    fn dependency_change_can_invalidate_blind_depth() {
        // T=10, D=8 ok; retarget thickness to 6 via a derived chain → invalid.
        let mut model = plate_model();
        add_parameter(&mut model, "thickness", ParameterValue::Scalar(10.0));
        add_parameter(&mut model, "blind_depth", ParameterValue::Scalar(8.0));
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(
            &mut model,
            "corner_holes",
            HoleExtent::Blind {
                start_face: HoleStartFace::Base,
                depth: SourcedLength::Parameter {
                    id: pid("blind_depth"),
                },
            },
        );
        set_thickness(
            &mut model,
            SourcedLength::Parameter {
                id: pid("thickness"),
            },
        );
        assert!(model.validate_spatial_readiness().ready);

        // Change the thickness parameter to 6: the same 8 mm blind depth now
        // exceeds the plate.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: pid("thickness"),
                value: ParameterValue::Scalar(6.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::InvalidBlindDepth {
                feature: fid("corner_holes"),
                depth: 8.0,
            }]
        );
    }

    #[test]
    fn unresolvable_parameters_are_reported() {
        // Unknown parameter for thickness (written directly: the executor
        // would already have rejected this command with SCC-ENG-017).
        let mut model = plate_model();
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        model.spatial.thickness = Some(SourcedLength::Parameter { id: pid("missing") });
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::UnresolvableParameter {
                parameter: pid("missing")
            }]
        );

        // Unresolved parameter for a blind depth.
        let mut model = fully_declared(8.0, &[]);
        add_parameter(&mut model, "d", ParameterValue::Unresolved);
        set_extent(
            &mut model,
            "corner_holes",
            HoleExtent::Blind {
                start_face: HoleStartFace::Base,
                depth: SourcedLength::Parameter { id: pid("d") },
            },
        );
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::UnresolvableParameter {
                parameter: pid("d")
            }]
        );

        // Boolean parameter used as a thickness is also unresolvable.
        let mut model = plate_model();
        add_parameter(&mut model, "flag", ParameterValue::Boolean(true));
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        set_thickness(&mut model, SourcedLength::Parameter { id: pid("flag") });
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::UnresolvableParameter {
                parameter: pid("flag")
            }]
        );
    }

    #[test]
    fn invalid_literal_thickness_is_reported_defensively() {
        let mut model = plate_model();
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
        // NaN thickness can only enter through direct construction (commands
        // reject it); the readiness check still reports it defensively.
        model.spatial.thickness = Some(SourcedLength::Literal { value: f64::NAN });
        let issues = model.validate_spatial_readiness().issues;
        assert_eq!(issues.len(), 1);
        assert!(matches!(issues[0], SpatialIssue::InvalidThickness { value } if value.is_nan()));
    }

    #[test]
    fn derived_thickness_rewiring_propagates_into_readiness() {
        let mut model = plate_model();
        add_parameter(&mut model, "base", ParameterValue::Scalar(5.0));
        add_parameter(&mut model, "plate_thickness", ParameterValue::Unresolved);

        let times = |parameter: &str, factor: f64| Expression::Multiply {
            left: Box::new(Expression::Reference {
                parameter: pid(parameter),
            }),
            right: Box::new(Expression::Constant { value: factor }),
        };

        // plate_thickness = base * 2 → 10 mm; a literal 8 mm blind depth fits.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("plate_thickness"),
                expression: times("base", 2.0),
            },
        )
        .unwrap();
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(
            &mut model,
            "corner_holes",
            HoleExtent::Blind {
                start_face: HoleStartFace::Base,
                depth: SourcedLength::Literal { value: 8.0 },
            },
        );
        set_thickness(
            &mut model,
            SourcedLength::Parameter {
                id: pid("plate_thickness"),
            },
        );
        assert_eq!(
            model.evaluate_parameter(&pid("plate_thickness")).unwrap(),
            ResolvedValue::Scalar(10.0)
        );
        assert!(model.validate_spatial_readiness().ready);

        // Re-wire the derived chain (base * 3 → thickness stays parameter-
        // driven but resolves differently); then change base so the resolved
        // thickness drops to 8 and the same literal depth hits the limit.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("plate_thickness"),
                expression: times("base", 3.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&pid("plate_thickness")).unwrap(),
            ResolvedValue::Scalar(15.0)
        );
        assert!(model.validate_spatial_readiness().ready);

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: pid("base"),
                value: ParameterValue::Scalar(4.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&pid("plate_thickness")).unwrap(),
            ResolvedValue::Scalar(12.0)
        );
        assert!(model.validate_spatial_readiness().ready);

        // base * 2 again: thickness 8 == depth 8 → readiness fails closed.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("plate_thickness"),
                expression: times("base", 2.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::InvalidBlindDepth {
                feature: fid("corner_holes"),
                depth: 8.0,
            }]
        );
    }

    #[test]
    fn literal_blind_depth_against_parameter_thickness_follows_parameter_change() {
        let mut model = plate_model();
        add_parameter(&mut model, "t", ParameterValue::Scalar(10.0));
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        set_extent(
            &mut model,
            "corner_holes",
            HoleExtent::Blind {
                start_face: HoleStartFace::Base,
                depth: SourcedLength::Literal { value: 8.0 },
            },
        );
        set_thickness(&mut model, SourcedLength::Parameter { id: pid("t") });
        assert!(model.validate_spatial_readiness().ready);

        // The literal depth never moves, but the parameter-driven thickness
        // shrinking below it invalidates the combination.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: pid("t"),
                value: ParameterValue::Scalar(6.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::InvalidBlindDepth {
                feature: fid("corner_holes"),
                depth: 8.0,
            }]
        );
    }

    #[test]
    fn unknown_extent_target_is_reported() {
        let mut model = fully_declared(8.0, &[]);
        model
            .spatial
            .hole_extents
            .insert(fid("ghost"), HoleExtent::ThroughAll);
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::UnknownHoleExtentTarget {
                feature: fid("ghost")
            }]
        );

        // An extent on a non-hole feature (base profile) is equally unknown.
        let mut model = fully_declared(8.0, &[]);
        model
            .spatial
            .hole_extents
            .insert(fid("plate"), HoleExtent::ThroughAll);
        assert_eq!(
            model.validate_spatial_readiness().issues,
            vec![SpatialIssue::UnknownHoleExtentTarget {
                feature: fid("plate")
            }]
        );
    }

    #[test]
    fn readiness_report_is_deterministic() {
        let build = || {
            let mut model = fully_declared(8.0, &[("corner_holes", 9.0)]);
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::Fillet {
                        id: fid("fillet_1"),
                        radius: 2.0,
                    },
                },
            )
            .unwrap();
            model
        };
        let first = build().validate_spatial_readiness();
        let second = build().validate_spatial_readiness();
        assert!(!first.ready);
        assert_eq!(first, second);
    }
}
