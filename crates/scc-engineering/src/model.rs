//! The engineering model: the single authoritative state of a design.
//!
//! [`EngineeringModel`] is the only true engineering state in this layer
//! (ADR-016 Decision 1). All state changes arrive through
//! [`crate::command::EngineeringCommand`] via
//! [`crate::executor::CommandExecutor`] (Decision 2); derived structures such
//! as the parameter dependency graph, resolved values, and validation reports
//! are computed on demand and are never a second source of truth.
//!
//! The model stores **intent**, not geometry: hole patterns remain patterns,
//! engineering constraints remain engineering constraints, and nothing here is
//! lowered to SemanticIR / ConstraintIR / GeometryModel (Phase 3+).

use std::collections::BTreeMap;

use scc_core::authority::AuthorityLevel;
use serde::{Deserialize, Serialize};

use crate::binding::{
    stable_feature_parameter_id, ExpectedParameterType, FeatureParameterReference,
    FeatureParameterSlot,
};
use crate::dependency::ParameterDependencyGraph;
use crate::error::EngineeringError;
use crate::evaluator::ResolvedValue;
use crate::feature::{
    DrawingProperty, DrawingPropertyKind, EngineeringConstraint, EngineeringFeature,
};
use crate::parameter::ParameterValue;
use crate::spatial::PartSpatialDefinition;
use crate::specification::{DimensionalSpecification, EngineeringDimensionRef};
use crate::{EngineeringConstraintId, FeatureId, ParameterId, PartId};

/// Identity of the single part managed by a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartDefinition {
    pub id: PartId,
    pub name: String,
}

/// Where a parameter's current value came from.
///
/// User-supplied values and derived values must never be indistinguishable
/// (AICAD requirement): the origin is stored explicitly on every parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterOrigin {
    /// Explicitly set by a user or upstream input.
    User,
    /// Computed from an expression over other parameters.
    Derived,
    /// A built-in default.
    Default,
    /// Supplied by an AI assumption (kept explicit for later review; never
    /// treated as user truth).
    AiAssumption,
}

/// A named, driving parameter of the design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub id: ParameterId,
    pub value: ParameterValue,
    pub origin: ParameterOrigin,
    pub authority: AuthorityLevel,
}

impl Parameter {
    /// Construct a parameter. Callers must validate `value` and the id before
    /// inserting it into a model.
    pub fn new(
        id: ParameterId,
        value: ParameterValue,
        origin: ParameterOrigin,
        authority: AuthorityLevel,
    ) -> Self {
        Self {
            id,
            value,
            origin,
            authority,
        }
    }
}

/// Drawing presentation state. Data-only: no projection, dimension layout, or
/// rendering lives here (Phase 2 stores properties only, per §12.9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingState {
    /// At most one property per kind, ordered for deterministic output.
    pub properties: BTreeMap<DrawingPropertyKind, DrawingProperty>,
}

impl DrawingState {
    /// An empty drawing state.
    pub fn new() -> Self {
        Self {
            properties: BTreeMap::new(),
        }
    }

    /// Set (or replace) a drawing property, keyed by its kind.
    pub fn set(&mut self, property: DrawingProperty) {
        let kind = property.kind();
        self.properties.insert(kind, property);
    }

    /// Read the stored property of the given kind, if any.
    pub fn get(&self, kind: DrawingPropertyKind) -> Option<&DrawingProperty> {
        self.properties.get(&kind)
    }
}

impl Default for DrawingState {
    fn default() -> Self {
        Self::new()
    }
}

/// The single authoritative state of an engineering design.
///
/// All collections are [`BTreeMap`]/[`BTreeSet`] for deterministic iteration
/// and serialization; no random UUIDs, no timestamps, and no `HashMap` in the
/// persisted core. Identical command sequences produce identical models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineeringModel {
    /// `None` until `CreatePart` is applied. A second `CreatePart` is rejected
    /// rather than silently replacing the existing part.
    pub part: Option<PartDefinition>,
    pub features: BTreeMap<FeatureId, EngineeringFeature>,
    /// Explicit feature history for generic parametric models. Legacy projects
    /// omit this field and retain their prior (non-historical) interpretation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_order: Vec<FeatureId>,
    pub parameters: BTreeMap<ParameterId, Parameter>,
    pub constraints: BTreeMap<EngineeringConstraintId, EngineeringConstraint>,
    /// Explicit, verifiable feature→slot→parameter binding. Each bound
    /// parameter drives one typed field ([`FeatureParameterSlot`]) of the
    /// feature; every bound id must exist and every slot must be valid for the
    /// feature type. Upgraded from a plain `FeatureId → Set<ParameterId>` in
    /// Phase 3 because the set form could not express which field a parameter
    /// drives (ADR-016 note / ADR-017 Decision 4).
    pub feature_parameters: BTreeMap<FeatureId, BTreeMap<FeatureParameterSlot, ParameterId>>,
    pub drawing: DrawingState,
    /// Spatial intent (Phase 9.1, ADR-032): plate thickness and per-hole
    /// extents. Optional data with an empty default; absent from serialized
    /// output while empty so Phase-8 documents round-trip byte-identically.
    #[serde(default, skip_serializing_if = "PartSpatialDefinition::is_empty")]
    pub spatial: PartSpatialDefinition,
    /// Typed dimensional requirements (Phase 11.5). This is engineering
    /// source truth; drawing formatting receives only a validated derived
    /// copy. Empty maps are omitted so legacy Project v1 JSON remains stable.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "crate::specification::dimensional_specification_map"
    )]
    pub dimensional_specifications: BTreeMap<EngineeringDimensionRef, DimensionalSpecification>,
}

impl EngineeringModel {
    /// An empty model with no part yet.
    pub fn new() -> Self {
        Self {
            part: None,
            features: BTreeMap::new(),
            feature_order: Vec::new(),
            parameters: BTreeMap::new(),
            constraints: BTreeMap::new(),
            feature_parameters: BTreeMap::new(),
            drawing: DrawingState::new(),
            spatial: PartSpatialDefinition::default(),
            dimensional_specifications: BTreeMap::new(),
        }
    }

    /// Return stable feature IDs in rebuild order. Legacy v1 models did not
    /// persist ordering, so their deterministic compatibility order is the
    /// existing BTreeMap key order. Callers must validate a model before using
    /// this list as an executable history.
    pub fn ordered_feature_ids(&self) -> Vec<FeatureId> {
        if self.feature_order.is_empty() {
            self.features.keys().cloned().collect()
        } else {
            self.feature_order.clone()
        }
    }

    /// Create a parameter (setup API; there is no `CreateParameter` command in
    /// Phase 2 — commands only modify existing parameters). Rejects duplicate
    /// ids and invalid values.
    pub fn create_parameter(&mut self, parameter: Parameter) -> Result<(), EngineeringError> {
        crate::validate_identifier("parameter id", &parameter.id.0)?;
        parameter.value.validate()?;
        if self.parameters.contains_key(&parameter.id) {
            return Err(EngineeringError::DuplicateParameter {
                parameter_id: parameter.id.clone(),
            });
        }
        self.parameters.insert(parameter.id.clone(), parameter);
        Ok(())
    }

    /// Bind one typed field (`slot`) of an existing feature to an existing
    /// parameter. The feature, the parameter, and the slot/feature-type
    /// compatibility are all verified, so every reference is always resolvable
    /// and the system never guesses meaning from a parameter id string.
    pub fn bind_feature_parameter(
        &mut self,
        feature: FeatureId,
        slot: FeatureParameterSlot,
        parameter: ParameterId,
    ) -> Result<(), EngineeringError> {
        let feature_value =
            self.features
                .get(&feature)
                .ok_or_else(|| EngineeringError::FeatureNotFound {
                    feature_id: feature.clone(),
                })?;
        if !FeatureParameterSlot::valid_for(feature_value).contains(&slot) {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: feature.clone(),
                slot,
                reason: format!("slot {} is not valid for this feature type", slot),
            });
        }
        if !self.parameters.contains_key(&parameter) {
            return Err(EngineeringError::ParameterNotFound {
                parameter_id: parameter,
            });
        }
        let mut candidate = self.clone();
        candidate
            .feature_parameters
            .entry(feature)
            .or_default()
            .insert(slot, parameter);
        candidate.validate_feature_parameter_bindings()?;
        *self = candidate;
        Ok(())
    }

    /// Resolve the stable typed edit address exposed to the Workbench. An
    /// unbound slot receives a deterministic proposed parameter ID; the edit
    /// command decides atomically whether it may create that binding.
    pub fn feature_parameter_reference(
        &self,
        feature_id: &FeatureId,
        slot: FeatureParameterSlot,
    ) -> Result<FeatureParameterReference, EngineeringError> {
        let part = self.part.as_ref().ok_or(EngineeringError::PartMissing)?;
        let feature =
            self.features
                .get(feature_id)
                .ok_or_else(|| EngineeringError::FeatureNotFound {
                    feature_id: feature_id.clone(),
                })?;
        let capability = FeatureParameterSlot::capability_for(feature, slot).ok_or_else(|| {
            EngineeringError::InvalidFeatureParameterBinding {
                feature_id: feature_id.clone(),
                slot,
                reason: format!("slot {} is not an editable parameter capability", slot),
            }
        })?;
        let parameter_id = self
            .feature_parameters
            .get(feature_id)
            .and_then(|bindings| bindings.get(&slot))
            .cloned()
            .unwrap_or_else(|| stable_feature_parameter_id(&part.id, feature_id, slot));
        Ok(FeatureParameterReference {
            part_id: part.id.clone(),
            feature_id: feature_id.clone(),
            slot,
            parameter_id,
            expected_type: capability.expected_type,
        })
    }

    /// Resolve one numeric feature slot, honoring a typed parameter binding.
    /// The capability type is checked before a value reaches geometry lowering.
    pub fn resolved_feature_parameter_value(
        &self,
        feature_id: &FeatureId,
        slot: FeatureParameterSlot,
    ) -> Result<f64, EngineeringError> {
        let feature =
            self.features
                .get(feature_id)
                .ok_or_else(|| EngineeringError::FeatureNotFound {
                    feature_id: feature_id.clone(),
                })?;
        if !FeatureParameterSlot::valid_for(feature).contains(&slot) {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: feature_id.clone(),
                slot,
                reason: format!("slot {} is not valid for this feature type", slot),
            });
        }
        let binding = self
            .feature_parameters
            .get(feature_id)
            .and_then(|bindings| bindings.get(&slot));
        let value = if let Some(parameter_id) = binding {
            match self.evaluate_parameter(parameter_id)? {
                ResolvedValue::Scalar(value) => value,
                ResolvedValue::Integer(value)
                    if FeatureParameterSlot::capability_for(feature, slot).is_none() =>
                {
                    value as f64
                }
                ResolvedValue::Unresolved => {
                    return Err(EngineeringError::UnresolvedRequiredParameter {
                        feature_id: feature_id.clone(),
                        slot,
                        parameter_id: parameter_id.clone(),
                    });
                }
                other => {
                    return Err(EngineeringError::InvalidFeatureParameterBinding {
                        feature_id: feature_id.clone(),
                        slot,
                        reason: format!(
                            "parameter {} has incompatible resolved value type {:?}",
                            parameter_id, other
                        ),
                    });
                }
            }
        } else {
            crate::binding::feature_field_slots(feature)
                .get(&slot)
                .copied()
                .ok_or_else(|| EngineeringError::MissingFeatureParameterBinding {
                    feature_id: feature_id.clone(),
                    slot,
                })?
        };
        if !value.is_finite() {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: feature_id.clone(),
                slot,
                reason: "resolved value must be finite".into(),
            });
        }
        if let Some(capability) = FeatureParameterSlot::capability_for(feature, slot) {
            match capability.expected_type {
                ExpectedParameterType::LengthMm if value <= 0.0 => {
                    return Err(EngineeringError::InvalidFeatureParameterBinding {
                        feature_id: feature_id.clone(),
                        slot,
                        reason: "length_mm parameter must be positive".into(),
                    });
                }
                ExpectedParameterType::LengthMm => {}
                ExpectedParameterType::AngleDegrees if !(0.0 < value && value < 180.0) => {
                    return Err(EngineeringError::InvalidFeatureParameterBinding {
                        feature_id: feature_id.clone(),
                        slot,
                        reason: "angle_degrees parameter must be between 0 and 180 degrees".into(),
                    });
                }
                ExpectedParameterType::AngleDegrees => {}
            }
        }
        Ok(value)
    }

    /// Validate resolved values and cross-field constraints for all
    /// capability-backed bindings, including values changed through legacy
    /// SetParameter/SetExpression commands. An unresolved draft remains a
    /// valid EngineeringModel state; geometry lowering rejects it before any
    /// Drawing or STEP result is produced.
    pub(crate) fn validate_feature_parameter_bindings(&self) -> Result<(), EngineeringError> {
        for (feature_id, bindings) in &self.feature_parameters {
            let Some(feature) = self.features.get(feature_id) else {
                continue;
            };
            for slot in bindings.keys() {
                if FeatureParameterSlot::capability_for(feature, *slot).is_some() {
                    self.resolved_feature_parameter_value_if_resolved(feature_id, *slot)?;
                }
            }
        }

        for (feature_id, feature) in &self.features {
            let crate::feature::EngineeringFeature::Parametric {
                feature:
                    crate::parametric::ParametricFeature::Hole {
                        termination, kind, ..
                    },
                ..
            } = feature
            else {
                continue;
            };
            let diameter = self.resolved_feature_parameter_value_if_resolved(
                feature_id,
                FeatureParameterSlot::Diameter,
            )?;
            let blind_depth = match termination {
                crate::parametric::HoleTermination::Blind { .. } => self
                    .resolved_feature_parameter_value_if_resolved(
                        feature_id,
                        FeatureParameterSlot::BlindDepth,
                    )?,
                crate::parametric::HoleTermination::ThroughAll => None,
            };
            if let crate::parametric::HoleKind::Counterbore { .. } = kind {
                let cb_diameter = self.resolved_feature_parameter_value_if_resolved(
                    feature_id,
                    FeatureParameterSlot::CounterboreDiameter,
                )?;
                let cb_depth = self.resolved_feature_parameter_value_if_resolved(
                    feature_id,
                    FeatureParameterSlot::CounterboreDepth,
                )?;
                if matches!((cb_diameter, diameter), (Some(cb), Some(main)) if cb <= main) {
                    return Err(EngineeringError::InvalidFeatureParameterBinding {
                        feature_id: feature_id.clone(),
                        slot: FeatureParameterSlot::CounterboreDiameter,
                        reason: "counterbore diameter must exceed the main Hole diameter".into(),
                    });
                }
                if matches!((cb_depth, blind_depth), (Some(cb), Some(blind)) if cb > blind) {
                    return Err(EngineeringError::InvalidFeatureParameterBinding {
                        feature_id: feature_id.clone(),
                        slot: FeatureParameterSlot::CounterboreDepth,
                        reason: "counterbore depth cannot exceed the blind Hole depth".into(),
                    });
                }
            }
            if let crate::parametric::HoleKind::Countersink { .. } = kind {
                let mouth_diameter = self.resolved_feature_parameter_value_if_resolved(
                    feature_id,
                    FeatureParameterSlot::CountersinkMouthDiameter,
                )?;
                let included_angle = self.resolved_feature_parameter_value_if_resolved(
                    feature_id,
                    FeatureParameterSlot::CountersinkIncludedAngleDegrees,
                )?;
                if matches!((mouth_diameter, diameter), (Some(mouth), Some(main)) if mouth <= main)
                {
                    return Err(EngineeringError::InvalidFeatureParameterBinding {
                        feature_id: feature_id.clone(),
                        slot: FeatureParameterSlot::CountersinkMouthDiameter,
                        reason: "countersink mouth diameter must exceed the main Hole diameter"
                            .into(),
                    });
                }
                if let (Some(mouth), Some(angle)) = (mouth_diameter, included_angle) {
                    let main = diameter.ok_or_else(|| {
                        EngineeringError::InvalidFeatureParameterBinding {
                            feature_id: feature_id.clone(),
                            slot: FeatureParameterSlot::Diameter,
                            reason: "main Hole diameter is unresolved".into(),
                        }
                    })?;
                    let depth = (mouth - main) / (2.0 * (angle.to_radians() / 2.0).tan());
                    if !depth.is_finite() || depth <= 1.0e-7 {
                        return Err(EngineeringError::InvalidFeatureParameterBinding {
                            feature_id: feature_id.clone(),
                            slot: FeatureParameterSlot::CountersinkIncludedAngleDegrees,
                            reason: "derived countersink depth must exceed OCP tolerance".into(),
                        });
                    }
                    if matches!(blind_depth, Some(blind) if depth > blind + 1.0e-8) {
                        return Err(EngineeringError::InvalidFeatureParameterBinding {
                            feature_id: feature_id.clone(),
                            slot: FeatureParameterSlot::CountersinkIncludedAngleDegrees,
                            reason: "derived countersink depth cannot exceed blind-hole depth"
                                .into(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn resolved_feature_parameter_value_if_resolved(
        &self,
        feature_id: &FeatureId,
        slot: FeatureParameterSlot,
    ) -> Result<Option<f64>, EngineeringError> {
        let parameter_id = self
            .feature_parameters
            .get(feature_id)
            .and_then(|bindings| bindings.get(&slot));
        if let Some(parameter_id) = parameter_id {
            if matches!(
                self.evaluate_parameter(parameter_id)?,
                ResolvedValue::Unresolved
            ) {
                return Ok(None);
            }
        }
        self.resolved_feature_parameter_value(feature_id, slot)
            .map(Some)
    }

    /// Build the parameter dependency graph from the current parameters
    /// (derived data, never persisted).
    pub fn dependency_graph(&self) -> ParameterDependencyGraph {
        ParameterDependencyGraph::from_parameters(&self.parameters)
    }

    /// Deterministically evaluate one parameter, following its dependencies.
    pub fn evaluate_parameter(&self, id: &ParameterId) -> Result<ResolvedValue, EngineeringError> {
        crate::evaluator::evaluate_parameter(self, id)
    }

    /// Deterministically evaluate every parameter in dependency order.
    pub fn evaluate_all(&self) -> Result<BTreeMap<ParameterId, ResolvedValue>, EngineeringError> {
        crate::evaluator::evaluate_all(self)
    }

    /// Set or clear one typed dimensional specification.
    ///
    /// The API boundary canonicalizes zero tolerance to `None`, validates the
    /// typed target against the current engineering model, and omits the
    /// default empty specification. Tolerance and precision never modify
    /// feature geometry or spatial intent.
    pub fn set_dimensional_specification(
        &mut self,
        target: EngineeringDimensionRef,
        specification: Option<DimensionalSpecification>,
    ) -> Result<(), EngineeringError> {
        target.validate_for_model(self)?;
        let Some(specification) = specification else {
            self.dimensional_specifications.remove(&target);
            return Ok(());
        };
        let specification = specification.canonicalize()?;
        if specification.is_default() {
            self.dimensional_specifications.remove(&target);
        } else {
            self.dimensional_specifications
                .insert(target, specification);
        }
        Ok(())
    }

    /// Return the authoritative typed specification for `target`, if any.
    pub fn dimensional_specification(
        &self,
        target: &EngineeringDimensionRef,
    ) -> Option<&DimensionalSpecification> {
        self.dimensional_specifications.get(target)
    }

    /// Model-level validation (Phase 2 scope only).
    ///
    /// Checks: a part exists; every feature/parameter/constraint id is unique
    /// (guaranteed by map keys) and consistent with its key; feature ids are
    /// valid and self-consistent; constraint feature references exist;
    /// feature→parameter bindings resolve; expression references exist; the
    /// parameter dependency graph is acyclic; and all determined numeric values
    /// are finite.
    ///
    /// It deliberately does **not** check geometric facts (hole inside plate,
    /// profile intersection, overconstraint) — those belong to a later
    /// semantic/lowering stage.
    pub fn validate_model(&self) -> Result<(), EngineeringError> {
        if self.part.is_none() {
            return Err(EngineeringError::ModelValidationFailed {
                message: "model has no part; apply CreatePart first".into(),
            });
        }

        for (key, feature) in &self.features {
            if key != feature.id() {
                return Err(EngineeringError::ModelValidationFailed {
                    message: format!(
                        "feature map key {} does not match feature id {}",
                        key,
                        feature.id()
                    ),
                });
            }
            feature
                .validate()
                .map_err(|e| EngineeringError::ModelValidationFailed {
                    message: format!("feature {} is invalid: {}", key, e),
                })?;
        }

        self.validate_feature_history()?;

        for constraint in self.constraints.values() {
            for feature in constraint.referenced_features() {
                if !self.features.contains_key(feature) {
                    return Err(EngineeringError::DanglingReference {
                        target: feature.0.clone(),
                        location: format!("constraint {}", constraint.id()),
                    });
                }
            }
        }

        for (feature, bindings) in &self.feature_parameters {
            let feature_value =
                self.features
                    .get(feature)
                    .ok_or_else(|| EngineeringError::DanglingReference {
                        target: feature.0.clone(),
                        location: "feature_parameters".into(),
                    })?;
            for (slot, parameter) in bindings {
                if !FeatureParameterSlot::valid_for(feature_value).contains(slot) {
                    return Err(EngineeringError::ModelValidationFailed {
                        message: format!(
                            "feature {} has an invalid binding: slot {} is not valid for this feature type",
                            feature, slot
                        ),
                    });
                }
                if !self.parameters.contains_key(parameter) {
                    return Err(EngineeringError::DanglingReference {
                        target: parameter.0.clone(),
                        location: format!("feature_parameters of {}", feature),
                    });
                }
            }
        }

        for (key, parameter) in &self.parameters {
            if key != &parameter.id {
                return Err(EngineeringError::ModelValidationFailed {
                    message: format!(
                        "parameter map key {} does not match parameter id {}",
                        key, parameter.id
                    ),
                });
            }
            parameter
                .value
                .validate()
                .map_err(|e| EngineeringError::ModelValidationFailed {
                    message: format!("parameter {} has invalid value: {}", key, e),
                })?;
            if let ParameterValue::Expression(expression) = &parameter.value {
                for reference in expression.referenced_parameters() {
                    if !self.parameters.contains_key(&reference) {
                        return Err(EngineeringError::ExpressionReferenceMissing {
                            parameter_id: reference,
                        });
                    }
                }
            }
        }

        for (target, specification) in &self.dimensional_specifications {
            target.validate_for_model(self)?;
            specification.validate()?;
            let canonical = specification.clone().canonicalize()?;
            if canonical != *specification {
                return Err(EngineeringError::InvalidDimensionalSpecification {
                    reason: format!(
                        "dimensional specification {} is not in canonical form",
                        target
                    ),
                });
            }
            if specification.is_default() {
                return Err(EngineeringError::InvalidDimensionalSpecification {
                    reason: format!(
                        "dimensional specification {} is empty and must be omitted",
                        target
                    ),
                });
            }
        }

        self.validate_feature_parameter_bindings()?;

        let graph = self.dependency_graph();
        if let Some(cycle) = graph.find_cycle() {
            return Err(EngineeringError::ExpressionCycle {
                cycle: cycle.iter().map(|id| id.0.clone()).collect(),
            });
        }

        Ok(())
    }

    fn validate_feature_history(&self) -> Result<(), EngineeringError> {
        let has_parametric = self.features.values().any(crate::parametric::is_parametric);
        if has_parametric && self.feature_order.is_empty() {
            return Err(EngineeringError::ModelValidationFailed {
                message: "parametric features require an explicit ordered feature history".into(),
            });
        }
        if has_parametric
            && self
                .features
                .values()
                .any(|feature| !crate::parametric::is_parametric(feature))
        {
            return Err(EngineeringError::ModelValidationFailed {
                message: "legacy geometry features cannot be mixed with the G1 parametric history; migrate explicitly first".into(),
            });
        }
        if has_parametric && !self.spatial.is_empty() {
            return Err(EngineeringError::ModelValidationFailed {
                message: "legacy part spatial intent cannot coexist with the G1 parametric history; represent extents through ordered features".into(),
            });
        }

        if self.feature_order.is_empty() {
            return Ok(());
        }
        let mut seen = BTreeMap::<FeatureId, usize>::new();
        for (index, id) in self.feature_order.iter().enumerate() {
            if !self.features.contains_key(id) {
                return Err(EngineeringError::ModelValidationFailed {
                    message: format!("feature history references missing feature {id}"),
                });
            }
            if seen.insert(id.clone(), index).is_some() {
                return Err(EngineeringError::ModelValidationFailed {
                    message: format!("feature history contains duplicate id {id}"),
                });
            }
        }
        if seen.len() != self.features.len() {
            return Err(EngineeringError::ModelValidationFailed {
                message: "feature history must list every feature exactly once".into(),
            });
        }

        for (index, id) in self.feature_order.iter().enumerate() {
            let Some(EngineeringFeature::Parametric {
                feature,
                suppressed,
                ..
            }) = self.features.get(id)
            else {
                continue;
            };
            let is_loft = matches!(feature, crate::parametric::ParametricFeature::Loft { .. });
            for reference in feature.referenced_features() {
                let Some(reference_index) = seen.get(reference).copied() else {
                    let message = if is_loft {
                        format!("LOFT_SECTION_REFERENCE_NOT_FOUND: Loft feature {id} references missing Sketch {reference}")
                    } else {
                        format!("feature {id} references missing feature {reference}")
                    };
                    return Err(EngineeringError::ModelValidationFailed { message });
                };
                if reference_index >= index {
                    let message = if is_loft {
                        format!("LOFT_SECTION_ORDER_INVALID: Loft feature {id} must reference an earlier Sketch; {reference} is not earlier")
                    } else {
                        format!("feature {id} must reference an earlier feature; {reference} is not earlier")
                    };
                    return Err(EngineeringError::ModelValidationFailed { message });
                }
                let source = self
                    .features
                    .get(reference)
                    .expect("history index is a feature key");
                let valid_source = match feature {
                    crate::parametric::ParametricFeature::DatumPlane {
                        definition:
                            crate::parametric::DatumPlaneDefinition::Offset {
                                base: crate::parametric::PlaneReference::Datum { .. },
                                ..
                            },
                    }
                    | crate::parametric::ParametricFeature::Sketch {
                        support: crate::parametric::PlaneReference::Datum { .. },
                        ..
                    }
                    | crate::parametric::ParametricFeature::Path {
                        support: crate::parametric::PlaneReference::Datum { .. },
                        ..
                    } => matches!(
                        source,
                        EngineeringFeature::Parametric {
                            feature: crate::parametric::ParametricFeature::DatumPlane { .. },
                            ..
                        }
                    ),
                    crate::parametric::ParametricFeature::DatumPlane {
                        definition:
                            crate::parametric::DatumPlaneDefinition::PlanarFaceReference { .. },
                    } => matches!(
                        source,
                        EngineeringFeature::Parametric {
                            feature: crate::parametric::ParametricFeature::Extrude { .. },
                            ..
                        }
                    ),
                    crate::parametric::ParametricFeature::Sketch {
                        support: crate::parametric::PlaneReference::PlanarFace { .. },
                        ..
                    }
                    | crate::parametric::ParametricFeature::Hole {
                        support: crate::parametric::PlaneReference::PlanarFace { .. },
                        ..
                    } => matches!(
                        source,
                        EngineeringFeature::Parametric {
                            feature: crate::parametric::ParametricFeature::Extrude { .. }
                                | crate::parametric::ParametricFeature::Loft { .. },
                            ..
                        }
                    ),
                    crate::parametric::ParametricFeature::Hole {
                        support: crate::parametric::PlaneReference::GeneratedFace { reference },
                        ..
                    } => matches!(
                        (&reference.selector, source),
                        (
                            crate::parametric::GeneratedFaceSelector::SweepStartCap
                                | crate::parametric::GeneratedFaceSelector::SweepEndCap,
                            EngineeringFeature::Parametric {
                                feature: crate::parametric::ParametricFeature::Sweep { .. },
                                ..
                            },
                        ) | (
                            crate::parametric::GeneratedFaceSelector::LoftStartCap
                                | crate::parametric::GeneratedFaceSelector::LoftEndCap,
                            EngineeringFeature::Parametric {
                                feature: crate::parametric::ParametricFeature::Loft { .. },
                                ..
                            },
                        )
                    ),
                    crate::parametric::ParametricFeature::Loft { .. } => matches!(
                        source,
                        EngineeringFeature::Parametric {
                            feature: crate::parametric::ParametricFeature::Sketch { .. },
                            ..
                        }
                    ),
                    crate::parametric::ParametricFeature::Sweep { profile, path, .. } => {
                        if reference == profile {
                            matches!(
                                source,
                                EngineeringFeature::Parametric {
                                    feature: crate::parametric::ParametricFeature::Sketch { .. },
                                    ..
                                }
                            )
                        } else if reference == path {
                            matches!(
                                source,
                                EngineeringFeature::Parametric {
                                    feature: crate::parametric::ParametricFeature::Path { .. },
                                    ..
                                }
                            )
                        } else {
                            false
                        }
                    }
                    crate::parametric::ParametricFeature::Path {
                        support: crate::parametric::PlaneReference::Principal { .. },
                        ..
                    } => false,
                    crate::parametric::ParametricFeature::Extrude { .. } => {
                        matches!(
                            source,
                            EngineeringFeature::Parametric {
                                feature: crate::parametric::ParametricFeature::Sketch { .. },
                                ..
                            }
                        )
                    }
                    crate::parametric::ParametricFeature::Revolve { profile, axis, .. } => {
                        if reference == profile {
                            matches!(
                                source,
                                EngineeringFeature::Parametric {
                                    feature: crate::parametric::ParametricFeature::Sketch { .. },
                                    ..
                                }
                            )
                        } else {
                            match axis {
                                crate::parametric::AxisReference::DatumAxis { feature }
                                    if reference == feature =>
                                {
                                    matches!(
                                        source,
                                        EngineeringFeature::Parametric {
                                            feature:
                                                crate::parametric::ParametricFeature::DatumAxis { .. },
                                            ..
                                        }
                                    )
                                }
                                _ => false,
                            }
                        }
                    }
                    crate::parametric::ParametricFeature::Pattern {
                        seed_feature_id,
                        definition,
                    } => {
                        if reference == seed_feature_id {
                            matches!(
                                source,
                                EngineeringFeature::Parametric {
                                    feature: crate::parametric::ParametricFeature::Extrude { .. }
                                        | crate::parametric::ParametricFeature::Hole { .. },
                                    ..
                                }
                            )
                        } else {
                            match definition {
                                crate::parametric::PatternDefinition::ExplicitPlacement {
                                    reference_plane:
                                        crate::parametric::PlaneReference::Datum { feature },
                                    ..
                                } if reference == feature => matches!(
                                    source,
                                    EngineeringFeature::Parametric {
                                        feature: crate::parametric::ParametricFeature::DatumPlane { .. },
                                        ..
                                    }
                                ),
                                _ => false,
                            }
                        }
                    }
                    crate::parametric::ParametricFeature::Fillet { .. }
                    | crate::parametric::ParametricFeature::Chamfer { .. } => matches!(
                        source,
                        EngineeringFeature::Parametric {
                            feature: crate::parametric::ParametricFeature::Extrude { .. }
                                | crate::parametric::ParametricFeature::Revolve { .. }
                                | crate::parametric::ParametricFeature::Sweep { .. }
                                | crate::parametric::ParametricFeature::Loft { .. }
                                | crate::parametric::ParametricFeature::Hole { .. }
                                | crate::parametric::ParametricFeature::Pattern { .. }
                                | crate::parametric::ParametricFeature::Fillet { .. }
                                | crate::parametric::ParametricFeature::Chamfer { .. },
                            ..
                        }
                    ),
                    _ => true,
                };
                if !valid_source {
                    let message = if is_loft {
                        format!("LOFT_SECTION_REFERENCE_INVALID: Loft feature {id} requires an earlier Sketch reference, not {reference}")
                    } else {
                        format!("feature {id} has an incompatible reference to {reference}")
                    };
                    return Err(EngineeringError::ModelValidationFailed { message });
                }
                if !*suppressed
                    && matches!(
                        source,
                        EngineeringFeature::Parametric {
                            suppressed: true,
                            ..
                        }
                    )
                {
                    let message = if is_loft {
                        format!("LOFT_SECTION_REFERENCE_NOT_FOUND: Loft feature {id} references suppressed Sketch {reference}")
                    } else {
                        format!("active feature {id} depends on suppressed feature {reference}")
                    };
                    return Err(EngineeringError::ModelValidationFailed { message });
                }
            }
        }
        Ok(())
    }
}

impl Default for EngineeringModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::EngineeringCommand;
    use crate::evaluator::ResolvedValue;
    use crate::executor::CommandExecutor;
    use crate::expression::Expression;
    use crate::feature::{DrawingProperty, HoleDefinition, HolePattern, ProjectionMethod};
    use crate::specification::{
        DecimalPlaces, DimensionalSpecification, DimensionalTolerance, EngineeringDimensionRef,
    };
    use crate::Point2D;

    fn pid(id: &str) -> ParameterId {
        ParameterId(id.into())
    }

    fn fid(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    fn reference(id: &str) -> Expression {
        Expression::Reference {
            parameter: ParameterId(id.into()),
        }
    }

    fn constant(value: f64) -> Expression {
        Expression::Constant { value }
    }

    fn user_parameter(id: &str, value: ParameterValue) -> Parameter {
        Parameter::new(
            ParameterId(id.into()),
            value,
            ParameterOrigin::User,
            AuthorityLevel::L0,
        )
    }

    fn with_parameters(ids: &[(&str, ParameterValue)]) -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId("p".into()),
                name: "p".into(),
            },
        )
        .unwrap();
        for (id, value) in ids {
            model
                .create_parameter(user_parameter(id, value.clone()))
                .unwrap();
        }
        model
    }

    /// §30 acceptance fixture: mounting plate with a corner hole pattern.
    fn mounting_plate_model() -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId("mounting_plate".into()),
                name: "mounting_plate".into(),
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
                feature: EngineeringFeature::HolePattern {
                    id: fid("corner_holes"),
                    pattern: HolePattern::Corner {
                        offset_x: 15.0,
                        offset_y: 15.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        model
    }

    // ---- §30 fixture ----

    #[test]
    fn mounting_plate_fixture_is_represented_as_a_pattern() {
        let model = mounting_plate_model();
        model.validate_model().unwrap();

        assert_eq!(model.part.as_ref().unwrap().name, "mounting_plate");
        match &model.features[&fid("plate")] {
            EngineeringFeature::BaseProfile {
                profile: crate::BaseProfile::Rectangle { width, height },
                ..
            } => {
                assert_eq!(*width, 120.0);
                assert_eq!(*height, 80.0);
            }
            _ => panic!("expected base profile"),
        }
        match &model.features[&fid("corner_holes")] {
            EngineeringFeature::HolePattern { pattern, .. } => {
                // The pattern stays pattern-level: a Corner description, never
                // an array of four holes (Phase 1 Corner expresses a single
                // corner hole; the four-corner layout is a Phase 3 lowering
                // concern).
                match pattern {
                    HolePattern::Corner {
                        offset_x,
                        offset_y,
                        hole,
                    } => {
                        assert_eq!(*offset_x, 15.0);
                        assert_eq!(*offset_y, 15.0);
                        assert_eq!(hole.diameter, 10.0);
                    }
                    _ => panic!("expected corner pattern"),
                }
            }
            _ => panic!("expected hole pattern"),
        }
    }

    // ---- §31 parameter modification ----

    #[test]
    fn parameter_modification_propagates_to_derived_value() {
        let mut model = with_parameters(&[
            ("plate_width", ParameterValue::Scalar(120.0)),
            ("offset", ParameterValue::Scalar(15.0)),
            ("right_hole_x", ParameterValue::Unresolved),
        ]);
        let right_hole_x = pid("right_hole_x");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: right_hole_x.clone(),
                expression: Expression::Subtract {
                    left: Box::new(reference("plate_width")),
                    right: Box::new(reference("offset")),
                },
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&right_hole_x).unwrap(),
            ResolvedValue::Scalar(105.0)
        );

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: pid("plate_width"),
                value: ParameterValue::Scalar(160.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&right_hole_x).unwrap(),
            ResolvedValue::Scalar(145.0)
        );
    }

    // ---- §32 expression chain ----

    #[test]
    fn expression_chain_propagates_on_source_change() {
        let mut model = with_parameters(&[
            ("a", ParameterValue::Scalar(100.0)),
            ("b", ParameterValue::Unresolved),
            ("c", ParameterValue::Unresolved),
            ("d", ParameterValue::Unresolved),
        ]);
        let mut set = |id: &str, expression: Expression| {
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetExpression {
                    target: pid(id),
                    expression,
                },
            )
            .unwrap();
        };
        set("b", Expression::divide_parameter_by_constant(pid("a"), 2.0));
        set(
            "c",
            Expression::Add {
                left: Box::new(reference("b")),
                right: Box::new(constant(10.0)),
            },
        );
        set(
            "d",
            Expression::Multiply {
                left: Box::new(reference("c")),
                right: Box::new(constant(2.0)),
            },
        );

        let values = model.evaluate_all().unwrap();
        let scalar = |id: &str| values.get(&pid(id)).unwrap();
        assert_eq!(scalar("a"), &ResolvedValue::Scalar(100.0));
        assert_eq!(scalar("b"), &ResolvedValue::Scalar(50.0));
        assert_eq!(scalar("c"), &ResolvedValue::Scalar(60.0));
        assert_eq!(scalar("d"), &ResolvedValue::Scalar(120.0));

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: pid("a"),
                value: ParameterValue::Scalar(80.0),
            },
        )
        .unwrap();
        let values = model.evaluate_all().unwrap();
        let scalar = |id: &str| values.get(&pid(id)).unwrap();
        assert_eq!(scalar("b"), &ResolvedValue::Scalar(40.0));
        assert_eq!(scalar("c"), &ResolvedValue::Scalar(50.0));
        assert_eq!(scalar("d"), &ResolvedValue::Scalar(100.0));
    }

    // ---- §22 Unresolved propagation ----

    #[test]
    fn unresolved_is_saved_and_stays_unresolved_when_evaluated() {
        // hole.offset_x = Unresolved → model accepts it, evaluation stays
        // unresolved, and it is never silently coerced to 0.
        let mut model = with_parameters(&[
            ("hole_offset_x", ParameterValue::Unresolved),
            ("derived", ParameterValue::Unresolved),
        ]);
        model.validate_model().unwrap();
        assert_eq!(
            model.evaluate_parameter(&pid("hole_offset_x")).unwrap(),
            ResolvedValue::Unresolved
        );

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("derived"),
                expression: Expression::divide_parameter_by_constant(pid("hole_offset_x"), 2.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&pid("derived")).unwrap(),
            ResolvedValue::Unresolved
        );
    }

    // ---- create_parameter / bind ----

    #[test]
    fn create_parameter_rejects_duplicate_and_invalid() {
        let mut model = EngineeringModel::new();
        model
            .create_parameter(user_parameter("width", ParameterValue::Scalar(1.0)))
            .unwrap();
        assert!(matches!(
            model
                .create_parameter(user_parameter("width", ParameterValue::Scalar(2.0)))
                .unwrap_err(),
            EngineeringError::DuplicateParameter { .. }
        ));
        assert!(model
            .create_parameter(user_parameter("bad id", ParameterValue::Scalar(1.0)))
            .is_err());
        assert!(model
            .create_parameter(user_parameter("nan", ParameterValue::Scalar(f64::NAN)))
            .is_err());
    }

    #[test]
    fn bind_feature_parameter_verifies_existence_and_slot_validity() {
        let mut model = mounting_plate_model();
        assert!(matches!(
            model
                .bind_feature_parameter(
                    fid("ghost"),
                    FeatureParameterSlot::Width,
                    pid("plate_width")
                )
                .unwrap_err(),
            EngineeringError::FeatureNotFound { .. }
        ));
        assert!(matches!(
            model
                .bind_feature_parameter(fid("plate"), FeatureParameterSlot::Width, pid("ghost"))
                .unwrap_err(),
            EngineeringError::ParameterNotFound { .. }
        ));
        // An invalid slot for the feature type is rejected at bind time.
        assert!(matches!(
            model
                .bind_feature_parameter(
                    fid("plate"),
                    FeatureParameterSlot::StartAngle,
                    pid("plate_width")
                )
                .unwrap_err(),
            EngineeringError::InvalidFeatureParameterBinding { .. }
        ));
        // Create the parameter, then a valid slot binding verifies and succeeds.
        model
            .create_parameter(user_parameter("plate_width", ParameterValue::Scalar(120.0)))
            .unwrap();
        model
            .bind_feature_parameter(
                fid("plate"),
                FeatureParameterSlot::Width,
                pid("plate_width"),
            )
            .unwrap();
        assert_eq!(
            model.feature_parameters[&fid("plate")][&FeatureParameterSlot::Width],
            pid("plate_width")
        );
    }

    // ---- validate_model ----

    #[test]
    fn validate_model_requires_a_part() {
        let mut model = EngineeringModel::new();
        model
            .create_parameter(user_parameter("width", ParameterValue::Scalar(1.0)))
            .unwrap();
        assert!(matches!(
            model.validate_model().unwrap_err(),
            EngineeringError::ModelValidationFailed { .. }
        ));
    }

    #[test]
    fn validate_model_detects_dangling_binding() {
        let mut model = with_parameters(&[("width", ParameterValue::Scalar(1.0))]);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: fid("plate"),
                    name: "plate".into(),
                    profile: crate::BaseProfile::Rectangle {
                        width: 10.0,
                        height: 10.0,
                    },
                },
            },
        )
        .unwrap();
        // A binding to a missing parameter is a dangling reference.
        model
            .feature_parameters
            .entry(fid("plate"))
            .or_default()
            .insert(FeatureParameterSlot::Width, pid("ghost"));
        assert!(matches!(
            model.validate_model().unwrap_err(),
            EngineeringError::DanglingReference { .. }
        ));
    }

    #[test]
    fn validate_model_detects_expression_cycle() {
        let mut model = with_parameters(&[
            ("a", ParameterValue::Unresolved),
            ("b", ParameterValue::Unresolved),
        ]);
        model.parameters.get_mut(&pid("a")).unwrap().value =
            ParameterValue::Expression(Expression::Add {
                left: Box::new(reference("b")),
                right: Box::new(constant(1.0)),
            });
        model.parameters.get_mut(&pid("b")).unwrap().value =
            ParameterValue::Expression(Expression::Add {
                left: Box::new(reference("a")),
                right: Box::new(constant(1.0)),
            });
        assert!(matches!(
            model.validate_model().unwrap_err(),
            EngineeringError::ExpressionCycle { .. }
        ));
    }

    #[test]
    fn validate_model_reports_expression_reference_missing() {
        let mut model = with_parameters(&[("a", ParameterValue::Unresolved)]);
        model.parameters.get_mut(&pid("a")).unwrap().value =
            ParameterValue::Expression(Expression::divide_parameter_by_constant(pid("ghost"), 8.0));
        assert!(matches!(
            model.validate_model().unwrap_err(),
            EngineeringError::ExpressionReferenceMissing { .. }
        ));
    }

    #[test]
    fn dimensional_specifications_are_typed_and_do_not_change_geometry() {
        let mut model = mounting_plate_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetPartThickness {
                source: Some(crate::spatial::SourcedLength::Literal { value: 10.0 }),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: fid("corner_holes"),
                extent: Some(crate::spatial::HoleExtent::Blind {
                    start_face: crate::spatial::HoleStartFace::Base,
                    depth: crate::spatial::SourcedLength::Literal { value: 4.0 },
                }),
            },
        )
        .unwrap();

        let width = EngineeringDimensionRef::FeatureSlot {
            feature: fid("plate"),
            slot: FeatureParameterSlot::Width,
        };
        let diameter = EngineeringDimensionRef::FeatureSlot {
            feature: fid("corner_holes"),
            slot: FeatureParameterSlot::Diameter,
        };
        let thickness = EngineeringDimensionRef::PartThickness;
        let depth = EngineeringDimensionRef::HoleDepth {
            feature: fid("corner_holes"),
        };
        let symmetric = DimensionalSpecification::new(
            DimensionalTolerance::symmetric(0.10).unwrap(),
            Some(DecimalPlaces(2)),
        )
        .unwrap();
        let bilateral = DimensionalSpecification::new(
            DimensionalTolerance::bilateral(0.20, -0.05).unwrap(),
            Some(DecimalPlaces(2)),
        )
        .unwrap();
        for (target, specification) in [
            (width.clone(), symmetric.clone()),
            (diameter, symmetric.clone()),
            (thickness, bilateral),
            (depth, symmetric),
        ] {
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetDimensionalSpecification {
                    target,
                    specification: Some(specification),
                },
            )
            .unwrap();
        }
        assert_eq!(model.dimensional_specifications.len(), 4);
        model.validate_model().unwrap();

        let with_spec = crate::lowering::lower_and_solve(&model).unwrap().1;
        let mut nominal = model.clone();
        nominal.dimensional_specifications.clear();
        let without_spec = crate::lowering::lower_and_solve(&nominal).unwrap().1;
        assert_eq!(with_spec, without_spec);

        model
            .set_dimensional_specification(width.clone(), Some(DimensionalSpecification::default()))
            .unwrap();
        assert!(!model.dimensional_specifications.contains_key(&width));

        let count_command = EngineeringCommand::SetDimensionalSpecification {
            target: EngineeringDimensionRef::FeatureSlot {
                feature: fid("corner_holes"),
                slot: FeatureParameterSlot::Count,
            },
            specification: Some(
                DimensionalSpecification::new(DimensionalTolerance::symmetric(0.1).unwrap(), None)
                    .unwrap(),
            ),
        };
        assert!(matches!(
            CommandExecutor::apply(&mut model, &count_command).unwrap_err(),
            EngineeringError::InvalidDimensionalSpecification { .. }
        ));

        let through_depth = EngineeringDimensionRef::HoleDepth {
            feature: fid("corner_holes"),
        };
        model
            .set_dimensional_specification(through_depth.clone(), None)
            .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: fid("corner_holes"),
                extent: Some(crate::spatial::HoleExtent::ThroughAll),
            },
        )
        .unwrap();
        assert!(model
            .set_dimensional_specification(
                through_depth,
                Some(
                    DimensionalSpecification::new(
                        DimensionalTolerance::symmetric(0.10).unwrap(),
                        Some(DecimalPlaces(2)),
                    )
                    .unwrap(),
                ),
            )
            .is_err());

        model.dimensional_specifications.insert(
            width,
            DimensionalSpecification {
                tolerance: DimensionalTolerance::Symmetric { plus_minus: 0.0 },
                precision: None,
            },
        );
        assert!(matches!(
            model.validate_model().unwrap_err(),
            EngineeringError::InvalidDimensionalSpecification { .. }
        ));
    }

    // ---- §37 serde round-trip ----

    #[test]
    fn model_serde_roundtrip_equals_original() {
        let model = mounting_plate_model();
        let json = serde_json::to_string_pretty(&model).unwrap();
        let back: EngineeringModel = serde_json::from_str(&json).unwrap();
        assert_eq!(model, back);

        // Deterministic output: the same model serializes to the same bytes.
        let again = serde_json::to_string_pretty(&model).unwrap();
        assert_eq!(json, again);
    }

    #[test]
    fn model_with_parameters_serde_roundtrips() {
        let mut model = with_parameters(&[
            ("plate_width", ParameterValue::Scalar(120.0)),
            (
                "right_hole_x",
                ParameterValue::Expression(Expression::Subtract {
                    left: Box::new(reference("plate_width")),
                    right: Box::new(reference("offset")),
                }),
            ),
            ("offset", ParameterValue::Unresolved),
        ]);
        model.drawing.set(DrawingProperty::ProjectionMethod {
            method: ProjectionMethod::ThirdAngle,
        });
        let back: EngineeringModel =
            serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
        assert_eq!(model, back);
    }

    // ---- §36 determinism ----

    #[test]
    fn identical_command_sequence_produces_identical_model() {
        let build = || {
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
                            width: 80.0,
                            height: 80.0,
                        },
                    },
                },
            )
            .unwrap();
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::HolePattern {
                        id: fid("bolt_circle"),
                        pattern: HolePattern::Circular {
                            center: Point2D::new(0.0, 0.0),
                            pitch_circle_diameter: 126.0,
                            count: 8,
                            start_angle: 0.0,
                            hole: HoleDefinition { diameter: 10.0 },
                        },
                    },
                },
            )
            .unwrap();
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetDrawingProperty {
                    property: DrawingProperty::ShowCenterlines { enabled: true },
                },
            )
            .unwrap();
            model
        };

        let reference = build();
        let reference_json = serde_json::to_string(&reference).unwrap();
        for _ in 0..100 {
            let model = build();
            assert_eq!(model, reference);
            assert_eq!(serde_json::to_string(&model).unwrap(), reference_json);
        }
    }

    /// Phase-8 shape model JSON (no `spatial` key): part, features (hole +
    /// circular pattern), parameter, empty constraints, feature→slot binding,
    /// and drawing state. Field order matches the canonical serialization.
    const LEGACY_MODEL_JSON: &str = concat!(
        r#"{"part":{"id":"legacy_plate","name":"Legacy Plate"},"#,
        r#""features":{"#,
        r#""bolt_circle":{"type":"hole_pattern","id":"bolt_circle","pattern":{"type":"circular","center":{"x":60.0,"y":40.0},"pitch_circle_diameter":90.0,"count":6,"start_angle":0.0,"hole":{"diameter":8.0}}},"#,
        r#""center_hole":{"type":"hole","id":"center_hole","diameter":10.0},"#,
        r#""plate":{"type":"base_profile","id":"plate","name":"plate","width":120.0,"height":80.0}},"#,
        r#""parameters":{"w":{"id":"w","value":{"type":"scalar","value":120.0},"origin":"user","authority":"L0"}},"#,
        r#""constraints":{},"feature_parameters":{"plate":{"width":"w"}},"#,
        r#""drawing":{"properties":{}}}"#
    );

    #[test]
    fn phase8_legacy_json_deserializes_with_empty_spatial_and_roundtrips() {
        let model: EngineeringModel = serde_json::from_str(LEGACY_MODEL_JSON).unwrap();
        assert!(model.spatial.is_empty());
        assert_eq!(model.part.as_ref().unwrap().id.0, "legacy_plate");
        assert_eq!(model.features.len(), 3);

        // The `spatial` key must stay absent while the definition is empty:
        // 100 compact re-serializations are byte-identical to the input.
        for _ in 0..100 {
            let output = serde_json::to_string(&model).unwrap();
            assert_eq!(output, LEGACY_MODEL_JSON);
        }
    }
}
