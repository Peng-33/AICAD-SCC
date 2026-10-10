//! Applies [`EngineeringCommand`] values to an [`EngineeringModel`].
//!
//! The executor is the only path through which commands mutate the model
//! (ADR-016 Decision 2). Each command is applied **atomically** using a
//! validate-before-write strategy: every validation that can fail runs before
//! the model is touched, so a failed command leaves the model completely
//! unchanged. Batches use a working-copy-plus-commit strategy (all-or-nothing).

use scc_core::authority::AuthorityLevel;

use crate::binding::{
    stable_feature_parameter_id, ExpectedParameterType, FeatureParameterReference,
    FeatureParameterSlot,
};
use crate::command::EngineeringCommand;
use crate::error::EngineeringError;
use crate::expression::Expression;
use crate::feature::{DrawingProperty, EngineeringConstraint, EngineeringFeature, HolePattern};
use crate::model::{EngineeringModel, ParameterOrigin, PartDefinition};
use crate::parameter::ParameterValue;
use crate::{EngineeringConstraintId, FeatureId, ParameterId, Point2D};

/// Stateless command executor.
pub struct CommandExecutor;

impl CommandExecutor {
    /// Apply one command to the model atomically.
    ///
    /// The command's own data validation runs first, then the model-level
    /// checks; only when every check passes is the model mutated.
    pub fn apply(
        model: &mut EngineeringModel,
        command: &EngineeringCommand,
    ) -> Result<(), EngineeringError> {
        command.validate()?;
        match command {
            EngineeringCommand::CreatePart { id, name } => create_part(model, id, name),
            EngineeringCommand::AddFeature { feature } => add_feature(model, feature),
            EngineeringCommand::ReplaceFeature { feature } => replace_feature(model, feature),
            EngineeringCommand::SetFeatureSuppressed { target, suppressed } => {
                set_feature_suppressed(model, target, *suppressed)
            }
            EngineeringCommand::ReorderFeature { target, before } => {
                reorder_feature(model, target, before.as_ref())
            }
            EngineeringCommand::RemoveFeature { target } => remove_feature(model, target),
            EngineeringCommand::SetParameter { target, value } => {
                set_parameter(model, target, value)
            }
            EngineeringCommand::SetFeatureParameter { target, value } => {
                set_feature_parameter(model, target, value)
            }
            EngineeringCommand::SetExpression { target, expression } => {
                set_expression(model, target, expression)
            }
            EngineeringCommand::DeclareParameter {
                id,
                value,
                origin,
                authority,
            } => declare_parameter(model, id, value, origin, authority),
            EngineeringCommand::BindFeatureParameter {
                feature,
                slot,
                parameter,
            } => bind_feature_parameter(model, feature, slot, parameter),
            EngineeringCommand::AddConstraint { constraint } => add_constraint(model, constraint),
            EngineeringCommand::RemoveConstraint { id } => remove_constraint(model, id),
            EngineeringCommand::MoveFeature { target, position } => {
                move_feature(model, target, position)
            }
            EngineeringCommand::SetDrawingProperty { property } => {
                set_drawing_property(model, property)
            }
            EngineeringCommand::SetPartThickness { source } => set_part_thickness(model, source),
            EngineeringCommand::SetHoleExtent { target, extent } => {
                set_hole_extent(model, target, extent)
            }
            EngineeringCommand::SetDimensionalSpecification {
                target,
                specification,
            } => model.set_dimensional_specification(target.clone(), specification.clone()),
        }
    }

    /// Apply a batch of commands all-or-nothing.
    ///
    /// Works on a clone of the model and commits it only if every command
    /// succeeds; on any failure the original model is untouched.
    pub fn apply_all(
        model: &mut EngineeringModel,
        commands: &[EngineeringCommand],
    ) -> Result<(), EngineeringError> {
        let mut working = model.clone();
        for command in commands {
            Self::apply(&mut working, command)?;
        }
        *model = working;
        Ok(())
    }
}

fn create_part(
    model: &mut EngineeringModel,
    id: &crate::PartId,
    name: &str,
) -> Result<(), EngineeringError> {
    if model.part.is_some() {
        return Err(EngineeringError::DuplicatePart {
            part_id: id.clone(),
        });
    }
    model.part = Some(PartDefinition {
        id: id.clone(),
        name: name.to_owned(),
    });
    Ok(())
}

fn add_feature(
    model: &mut EngineeringModel,
    feature: &EngineeringFeature,
) -> Result<(), EngineeringError> {
    if model.part.is_none() {
        return Err(EngineeringError::PartMissing);
    }
    let feature = canonicalize_feature(feature)?;
    feature.validate()?;
    let id = feature.id().clone();
    let incoming_is_parametric = crate::parametric::is_parametric(&feature);
    if !model.features.is_empty()
        && model
            .features
            .values()
            .any(crate::parametric::is_parametric)
            != incoming_is_parametric
    {
        return Err(EngineeringError::ModelValidationFailed {
            message: "legacy geometry features and G1 parametric features cannot be mixed; migrate explicitly first".into(),
        });
    }
    if model.features.contains_key(&id) {
        return Err(EngineeringError::DuplicateFeature { feature_id: id });
    }
    if incoming_is_parametric || !model.feature_order.is_empty() {
        if model.feature_order.is_empty() {
            model.feature_order.extend(model.features.keys().cloned());
        }
        model.feature_order.push(id.clone());
    }
    model.features.insert(id.clone(), feature);
    if incoming_is_parametric {
        if let Err(error) = model.validate_model() {
            model.features.remove(&id);
            model.feature_order.retain(|feature_id| feature_id != &id);
            return Err(error);
        }
    }
    Ok(())
}

fn replace_feature(
    model: &mut EngineeringModel,
    feature: &EngineeringFeature,
) -> Result<(), EngineeringError> {
    let feature = canonicalize_feature(feature)?;
    feature.validate()?;
    let id = feature.id().clone();
    if !model.features.contains_key(&id) {
        return Err(EngineeringError::FeatureNotFound { feature_id: id });
    }
    let mut candidate = model.clone();
    candidate.features.insert(id, feature);
    if candidate
        .features
        .values()
        .any(crate::parametric::is_parametric)
    {
        if candidate.feature_order.is_empty() {
            candidate.feature_order = candidate.features.keys().cloned().collect();
        }
    } else {
        candidate.feature_order.clear();
    }
    candidate.validate_model()?;
    *model = candidate;
    Ok(())
}

fn set_feature_suppressed(
    model: &mut EngineeringModel,
    target: &FeatureId,
    suppressed: bool,
) -> Result<(), EngineeringError> {
    let mut candidate = model.clone();
    match candidate.features.get_mut(target) {
        Some(EngineeringFeature::Parametric {
            suppressed: state, ..
        }) => *state = suppressed,
        Some(_) => {
            return Err(EngineeringError::ModelValidationFailed {
                message: "suppression is currently supported only for G1 parametric history nodes"
                    .into(),
            });
        }
        None => {
            return Err(EngineeringError::FeatureNotFound {
                feature_id: target.clone(),
            })
        }
    }
    candidate.validate_model()?;
    *model = candidate;
    Ok(())
}

fn reorder_feature(
    model: &mut EngineeringModel,
    target: &FeatureId,
    before: Option<&FeatureId>,
) -> Result<(), EngineeringError> {
    if !model.features.contains_key(target) {
        return Err(EngineeringError::FeatureNotFound {
            feature_id: target.clone(),
        });
    }
    if !model
        .features
        .values()
        .any(crate::parametric::is_parametric)
    {
        return Err(EngineeringError::ModelValidationFailed {
            message: "explicit reorder is available only for G1 parametric history".into(),
        });
    }
    if let Some(before) = before {
        if !model.features.contains_key(before) {
            return Err(EngineeringError::FeatureNotFound {
                feature_id: before.clone(),
            });
        }
    }
    let mut candidate = model.clone();
    let mut order = candidate.ordered_feature_ids();
    order.retain(|id| id != target);
    let index = before
        .and_then(|before| order.iter().position(|id| id == before))
        .unwrap_or(order.len());
    order.insert(index, target.clone());
    candidate.feature_order = order;
    candidate.validate_model()?;
    *model = candidate;
    Ok(())
}

fn canonicalize_feature(
    feature: &EngineeringFeature,
) -> Result<EngineeringFeature, EngineeringError> {
    match feature {
        EngineeringFeature::Slot {
            id,
            center,
            length,
            width,
            orientation_degrees,
        } => Ok(EngineeringFeature::Slot {
            id: id.clone(),
            center: *center,
            length: *length,
            width: *width,
            orientation_degrees: crate::slot::canonicalize_orientation_degrees(
                *orientation_degrees,
            )?,
        }),
        EngineeringFeature::BaseProfile {
            id,
            name,
            profile: crate::BaseProfile::Polygon { vertices },
        } => Ok(EngineeringFeature::BaseProfile {
            id: id.clone(),
            name: name.clone(),
            profile: crate::BaseProfile::Polygon {
                vertices: crate::profile::canonicalize_polygon(vertices)?,
            },
        }),
        other => Ok(other.clone()),
    }
}

fn remove_feature(
    model: &mut EngineeringModel,
    target: &FeatureId,
) -> Result<(), EngineeringError> {
    if !model.features.contains_key(target) {
        return Err(EngineeringError::FeatureNotFound {
            feature_id: target.clone(),
        });
    }
    // Reject when anything references the feature (Phase 2 policy: reject,
    // never cascade-delete, per ADR-016 Decision 7).
    let mut referenced_by: Vec<String> = Vec::new();
    for constraint in model.constraints.values() {
        if constraint.references_feature(target) {
            referenced_by.push(format!("constraint {}", constraint.id()));
        }
    }
    for (feature_id, feature) in &model.features {
        if feature_id == target {
            continue;
        }
        if let EngineeringFeature::Parametric { feature, .. } = feature {
            if feature.referenced_features().contains(&target) {
                referenced_by.push(format!("parametric feature {feature_id}"));
            }
        }
    }
    if !referenced_by.is_empty() {
        return Err(EngineeringError::FeatureInUse {
            feature_id: target.clone(),
            referenced_by,
        });
    }
    model.features.remove(target);
    model
        .feature_order
        .retain(|feature_id| feature_id != target);
    if !model
        .features
        .values()
        .any(crate::parametric::is_parametric)
    {
        model.feature_order.clear();
    }
    model.feature_parameters.remove(target);
    // Phase 9.1: keep the spatial invariant — a removed feature leaves no
    // dangling extent entry behind.
    model.spatial.hole_extents.remove(target);
    model
        .dimensional_specifications
        .retain(|dimension, _| !dimension.references_feature(target));
    Ok(())
}

fn set_parameter(
    model: &mut EngineeringModel,
    target: &ParameterId,
    value: &ParameterValue,
) -> Result<(), EngineeringError> {
    value.validate()?;
    let mut candidate = model.clone();
    let parameter = candidate.parameters.get_mut(target).ok_or_else(|| {
        EngineeringError::ParameterNotFound {
            parameter_id: target.clone(),
        }
    })?;
    // SetParameter overwrites to a direct value with User origin and clears any
    // expression relation (the expression is replaced, never kept alongside).
    parameter.value = value.clone();
    parameter.origin = ParameterOrigin::User;
    candidate.validate_feature_parameter_bindings()?;
    *model = candidate;
    Ok(())
}

fn set_feature_parameter(
    model: &mut EngineeringModel,
    target: &FeatureParameterReference,
    value: &ParameterValue,
) -> Result<(), EngineeringError> {
    let mut candidate = model.clone();
    let part = candidate
        .part
        .as_ref()
        .ok_or(EngineeringError::PartMissing)?;
    if part.id != target.part_id {
        return Err(EngineeringError::InvalidFeatureParameterBinding {
            feature_id: target.feature_id.clone(),
            slot: target.slot,
            reason: format!(
                "stale part reference: expected {}, current part is {}",
                target.part_id, part.id
            ),
        });
    }
    let feature = candidate.features.get(&target.feature_id).ok_or_else(|| {
        EngineeringError::FeatureNotFound {
            feature_id: target.feature_id.clone(),
        }
    })?;
    let capability =
        FeatureParameterSlot::capability_for(feature, target.slot).ok_or_else(|| {
            EngineeringError::InvalidFeatureParameterBinding {
                feature_id: target.feature_id.clone(),
                slot: target.slot,
                reason: format!(
                    "slot {} is not an editable parameter capability for this feature",
                    target.slot
                ),
            }
        })?;
    if capability.expected_type != target.expected_type {
        return Err(EngineeringError::InvalidFeatureParameterBinding {
            feature_id: target.feature_id.clone(),
            slot: target.slot,
            reason: format!(
                "stale parameter type reference: expected {:?}, current schema requires {:?}",
                target.expected_type, capability.expected_type
            ),
        });
    }
    match (target.expected_type, value) {
        (ExpectedParameterType::LengthMm, ParameterValue::Scalar(value))
            if value.is_finite() && *value > 0.0 => {}
        (ExpectedParameterType::LengthMm, ParameterValue::Scalar(_)) => {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: target.feature_id.clone(),
                slot: target.slot,
                reason: "length_mm parameter must be finite and positive".into(),
            });
        }
        (ExpectedParameterType::LengthMm, other) => {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: target.feature_id.clone(),
                slot: target.slot,
                reason: format!(
                    "expected a scalar length in millimetres, got {}",
                    parameter_value_kind(other)
                ),
            });
        }
        (ExpectedParameterType::AngleDegrees, ParameterValue::Scalar(value))
            if value.is_finite() && 0.0 < *value && *value < 180.0 => {}
        (ExpectedParameterType::AngleDegrees, ParameterValue::Scalar(_)) => {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: target.feature_id.clone(),
                slot: target.slot,
                reason: "angle_degrees parameter must be between 0 and 180 degrees".into(),
            });
        }
        (ExpectedParameterType::AngleDegrees, other) => {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: target.feature_id.clone(),
                slot: target.slot,
                reason: format!(
                    "expected a scalar angle in degrees, got {}",
                    parameter_value_kind(other)
                ),
            });
        }
    }

    let bound_parameter = candidate
        .feature_parameters
        .get(&target.feature_id)
        .and_then(|bindings| bindings.get(&target.slot))
        .cloned();
    match bound_parameter {
        Some(parameter_id) if parameter_id == target.parameter_id => {
            let parameter = candidate.parameters.get(&parameter_id).ok_or_else(|| {
                EngineeringError::ParameterNotFound {
                    parameter_id: parameter_id.clone(),
                }
            })?;
            if !matches!(
                &parameter.value,
                ParameterValue::Scalar(_)
                    | ParameterValue::Expression(_)
                    | ParameterValue::Unresolved
            ) {
                return Err(EngineeringError::InvalidFeatureParameterBinding {
                    feature_id: target.feature_id.clone(),
                    slot: target.slot,
                    reason: format!(
                        "bound parameter {} is not a scalar length in millimetres",
                        parameter_id
                    ),
                });
            }
            if !matches!(
                parameter.origin,
                ParameterOrigin::User | ParameterOrigin::Default
            ) {
                return Err(EngineeringError::InvalidFeatureParameterBinding {
                    feature_id: target.feature_id.clone(),
                    slot: target.slot,
                    reason: format!("bound parameter {} is not directly editable", parameter_id),
                });
            }
            let parameter = candidate
                .parameters
                .get_mut(&parameter_id)
                .expect("validated parameter exists");
            parameter.value = value.clone();
            parameter.origin = ParameterOrigin::User;
        }
        Some(parameter_id) => {
            return Err(EngineeringError::InvalidFeatureParameterBinding {
                feature_id: target.feature_id.clone(),
                slot: target.slot,
                reason: format!(
                    "stale parameter reference: this slot is bound to {}, not {}",
                    parameter_id, target.parameter_id
                ),
            });
        }
        None => {
            let expected_id =
                stable_feature_parameter_id(&target.part_id, &target.feature_id, target.slot);
            if target.parameter_id != expected_id {
                return Err(EngineeringError::InvalidFeatureParameterBinding {
                    feature_id: target.feature_id.clone(),
                    slot: target.slot,
                    reason: format!(
                        "unbound slot requires its stable parameter identity {}, not {}",
                        expected_id, target.parameter_id
                    ),
                });
            }
            if candidate.parameters.contains_key(&target.parameter_id) {
                return Err(EngineeringError::InvalidFeatureParameterBinding {
                    feature_id: target.feature_id.clone(),
                    slot: target.slot,
                    reason: format!(
                        "stable parameter identity {} already exists outside this slot",
                        target.parameter_id
                    ),
                });
            }
            candidate.parameters.insert(
                target.parameter_id.clone(),
                crate::model::Parameter::new(
                    target.parameter_id.clone(),
                    value.clone(),
                    ParameterOrigin::User,
                    AuthorityLevel::L0,
                ),
            );
            candidate
                .feature_parameters
                .entry(target.feature_id.clone())
                .or_default()
                .insert(target.slot, target.parameter_id.clone());
        }
    }

    candidate.validate_model()?;
    *model = candidate;
    Ok(())
}

fn parameter_value_kind(value: &ParameterValue) -> &'static str {
    match value {
        ParameterValue::Scalar(_) => "scalar",
        ParameterValue::Integer(_) => "integer",
        ParameterValue::Boolean(_) => "boolean",
        ParameterValue::Choice(_) => "choice",
        ParameterValue::Expression(_) => "expression",
        ParameterValue::Unresolved => "unresolved",
    }
}

fn set_expression(
    model: &mut EngineeringModel,
    target: &ParameterId,
    expression: &Expression,
) -> Result<(), EngineeringError> {
    expression.validate("expression")?;
    if !model.parameters.contains_key(target) {
        return Err(EngineeringError::ParameterNotFound {
            parameter_id: target.clone(),
        });
    }
    for reference in expression.referenced_parameters() {
        if !model.parameters.contains_key(&reference) {
            return Err(EngineeringError::ExpressionReferenceMissing {
                parameter_id: reference,
            });
        }
    }
    // Cycle check on the *would-be* graph before writing anything.
    let mut graph = model.dependency_graph();
    graph.set_dependencies(target.clone(), expression.referenced_parameters());
    if let Some(cycle) = graph.find_cycle() {
        return Err(EngineeringError::ExpressionCycle {
            cycle: cycle.iter().map(|id| id.0.clone()).collect(),
        });
    }
    let mut candidate = model.clone();
    let parameter = candidate
        .parameters
        .get_mut(target)
        .expect("existence checked above");
    parameter.value = ParameterValue::Expression(expression.clone());
    parameter.origin = ParameterOrigin::Derived;
    candidate.validate_feature_parameter_bindings()?;
    *model = candidate;
    Ok(())
}

fn declare_parameter(
    model: &mut EngineeringModel,
    id: &ParameterId,
    value: &ParameterValue,
    origin: &crate::model::ParameterOrigin,
    authority: &scc_core::authority::AuthorityLevel,
) -> Result<(), EngineeringError> {
    // Delegates to the model's create_parameter, which validates the id, the
    // value, and rejects duplicates atomically (validate-before-write).
    model.create_parameter(crate::model::Parameter::new(
        id.clone(),
        value.clone(),
        *origin,
        *authority,
    ))
}

fn bind_feature_parameter(
    model: &mut EngineeringModel,
    feature: &FeatureId,
    slot: &crate::binding::FeatureParameterSlot,
    parameter: &ParameterId,
) -> Result<(), EngineeringError> {
    // Delegates to the model's bind_feature_parameter, which validates the
    // feature exists, the parameter exists, and the slot is valid for the
    // feature type (Phase 3 strong-typed binding).
    model.bind_feature_parameter(feature.clone(), *slot, parameter.clone())
}

fn add_constraint(
    model: &mut EngineeringModel,
    constraint: &EngineeringConstraint,
) -> Result<(), EngineeringError> {
    constraint.validate()?;
    let id = constraint.id().clone();
    if model.constraints.contains_key(&id) {
        return Err(EngineeringError::DuplicateConstraint { constraint_id: id });
    }
    for feature in constraint.referenced_features() {
        if !model.features.contains_key(feature) {
            return Err(EngineeringError::FeatureNotFound {
                feature_id: feature.clone(),
            });
        }
    }
    model.constraints.insert(id, constraint.clone());
    Ok(())
}

fn remove_constraint(
    model: &mut EngineeringModel,
    id: &EngineeringConstraintId,
) -> Result<(), EngineeringError> {
    if !model.constraints.contains_key(id) {
        return Err(EngineeringError::ConstraintNotFound {
            constraint_id: id.clone(),
        });
    }
    model.constraints.remove(id);
    Ok(())
}

fn move_feature(
    model: &mut EngineeringModel,
    target: &FeatureId,
    position: &Point2D,
) -> Result<(), EngineeringError> {
    if !model.features.contains_key(target) {
        return Err(EngineeringError::FeatureNotFound {
            feature_id: target.clone(),
        });
    }
    let feature = model
        .features
        .get(target)
        .expect("existence checked above")
        .clone();
    match &feature {
        EngineeringFeature::Slot {
            id,
            length,
            width,
            orientation_degrees,
            ..
        } => {
            model.features.insert(
                id.clone(),
                EngineeringFeature::Slot {
                    id: id.clone(),
                    center: *position,
                    length: *length,
                    width: *width,
                    orientation_degrees: *orientation_degrees,
                },
            );
            Ok(())
        }
        EngineeringFeature::HolePattern { id, pattern } => {
            let new_pattern = match pattern {
                HolePattern::Circular {
                    center: _,
                    pitch_circle_diameter,
                    count,
                    start_angle,
                    hole,
                } => HolePattern::Circular {
                    center: *position,
                    pitch_circle_diameter: *pitch_circle_diameter,
                    count: *count,
                    start_angle: *start_angle,
                    hole: hole.clone(),
                },
                _ => {
                    return Err(EngineeringError::UnsupportedMove {
                        feature_id: target.clone(),
                        reason: "only a circular hole pattern has a movable center in this phase",
                    });
                }
            };
            model.features.insert(
                id.clone(),
                EngineeringFeature::HolePattern {
                    id: id.clone(),
                    pattern: new_pattern,
                },
            );
            Ok(())
        }
        _ => Err(EngineeringError::UnsupportedMove {
            feature_id: target.clone(),
            reason: "feature has no movable position/center in this phase",
        }),
    }
}

fn set_drawing_property(
    model: &mut EngineeringModel,
    property: &DrawingProperty,
) -> Result<(), EngineeringError> {
    property.validate()?;
    model.drawing.set(property.clone());
    Ok(())
}

fn set_part_thickness(
    model: &mut EngineeringModel,
    source: &Option<crate::spatial::SourcedLength>,
) -> Result<(), EngineeringError> {
    if model.part.is_none() {
        return Err(EngineeringError::PartMissing);
    }
    if model
        .features
        .values()
        .any(crate::parametric::is_parametric)
    {
        return Err(EngineeringError::ModelValidationFailed {
            message: "SetPartThickness is a legacy spatial command; use ordered Extrude features in a G1 parametric model".into(),
        });
    }
    if let Some(crate::spatial::SourcedLength::Literal { value }) = source {
        crate::positive("part thickness", *value)?;
    }
    if let Some(crate::spatial::SourcedLength::Parameter { id }) = source {
        if !model.parameters.contains_key(id) {
            return Err(EngineeringError::ParameterNotFound {
                parameter_id: id.clone(),
            });
        }
    }
    // Symmetric eager cross-check (ADR-032 Decision 5, as amended): a new
    // literal thickness may not strand an existing literal blind depth.
    // Parameter-backed depths stay deferred to readiness. BTreeMap order
    // makes the reported offender deterministic.
    if let Some(crate::spatial::SourcedLength::Literal { value: thickness }) = source {
        for (feature_id, extent) in &model.spatial.hole_extents {
            if let crate::spatial::HoleExtent::Blind {
                depth: crate::spatial::SourcedLength::Literal { value },
                ..
            } = extent
            {
                crate::spatial::check_blind_depth_against_thickness(
                    feature_id, *value, *thickness,
                )?;
            }
        }
    }
    // All checks passed; write.
    model.spatial.thickness = source.clone();
    Ok(())
}

fn set_hole_extent(
    model: &mut EngineeringModel,
    target: &FeatureId,
    extent: &Option<crate::spatial::HoleExtent>,
) -> Result<(), EngineeringError> {
    let feature = model
        .features
        .get(target)
        .ok_or_else(|| EngineeringError::FeatureNotFound {
            feature_id: target.clone(),
        })?;
    if !matches!(
        feature,
        EngineeringFeature::Hole { .. } | EngineeringFeature::HolePattern { .. }
    ) {
        return Err(EngineeringError::InvalidSpatialTarget {
            feature_id: target.clone(),
        });
    }
    if let Some(crate::spatial::HoleExtent::Blind {
        depth: crate::spatial::SourcedLength::Literal { value },
        ..
    }) = extent
    {
        crate::positive("blind hole depth", *value)?;
        if let Some(crate::spatial::SourcedLength::Literal { value: thickness }) =
            model.spatial.thickness
        {
            crate::spatial::check_blind_depth_against_thickness(target, *value, thickness)?;
        }
    }
    // All checks passed; write.
    match extent {
        Some(extent) => {
            model
                .spatial
                .hole_extents
                .insert(target.clone(), extent.clone());
        }
        None => {
            model.spatial.hole_extents.remove(target);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{ExpectedParameterType, FeatureParameterReference};
    use crate::feature::{DrawingPropertyKind, HoleDefinition, ProjectionMethod};
    use crate::model::{EngineeringModel, Parameter, ParameterOrigin};
    use crate::parametric::{
        HoleKind, HoleTermination, ParametricFeature, PlaneReference, PrincipalPlane,
    };
    use scc_core::authority::AuthorityLevel;

    fn part() -> EngineeringCommand {
        EngineeringCommand::CreatePart {
            id: crate::PartId("plate".into()),
            name: "plate".into(),
        }
    }

    fn base_profile() -> EngineeringCommand {
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: crate::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        }
    }

    fn circular_pattern() -> EngineeringCommand {
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("bolt_circle".into()),
                pattern: HolePattern::Circular {
                    center: Point2D::new(0.0, 0.0),
                    pitch_circle_diameter: 126.0,
                    count: 8,
                    start_angle: 0.0,
                    hole: HoleDefinition { diameter: 10.0 },
                },
            },
        }
    }

    fn add_parameter(model: &mut EngineeringModel, id: &str, value: ParameterValue) -> ParameterId {
        let parameter_id = ParameterId(id.into());
        model
            .create_parameter(Parameter::new(
                parameter_id.clone(),
                value,
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
        parameter_id
    }

    fn fixture_model() -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(&mut model, &part()).unwrap();
        CommandExecutor::apply(&mut model, &base_profile()).unwrap();
        CommandExecutor::apply(&mut model, &circular_pattern()).unwrap();
        model
    }

    fn parametric_hole_model(termination: HoleTermination, kind: HoleKind) -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(&mut model, &part()).unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Parametric {
                    id: FeatureId("center_hole".into()),
                    name: "center_hole".into(),
                    feature: ParametricFeature::Hole {
                        support: PlaneReference::Principal {
                            plane: PrincipalPlane::XY,
                        },
                        position: Point2D::new(0.0, 0.0),
                        diameter: 10.0,
                        reversed: true,
                        termination,
                        kind,
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();
        model
    }

    fn typed_hole_target(
        model: &EngineeringModel,
        slot: FeatureParameterSlot,
    ) -> FeatureParameterReference {
        model
            .feature_parameter_reference(&FeatureId("center_hole".into()), slot)
            .unwrap()
    }

    #[test]
    fn typed_hole_parameter_edit_binds_stable_targets_and_updates_only_selected_slot() {
        let mut model =
            parametric_hole_model(HoleTermination::Blind { depth: 6.0 }, HoleKind::Simple);
        model.validate_model().unwrap();
        let original_depth_target = typed_hole_target(&model, FeatureParameterSlot::BlindDepth);
        assert_eq!(
            original_depth_target,
            typed_hole_target(&model, FeatureParameterSlot::BlindDepth)
        );
        assert_eq!(
            original_depth_target.expected_type,
            ExpectedParameterType::LengthMm
        );
        assert_eq!(original_depth_target.part_id.0, "plate");
        assert_eq!(original_depth_target.feature_id.0, "center_hole");

        let diameter_target = typed_hole_target(&model, FeatureParameterSlot::Diameter);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureParameter {
                target: diameter_target.clone(),
                value: ParameterValue::Scalar(12.0),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureParameter {
                target: original_depth_target.clone(),
                value: ParameterValue::Scalar(8.0),
            },
        )
        .unwrap();

        assert_eq!(
            model
                .resolved_feature_parameter_value(
                    &FeatureId("center_hole".into()),
                    FeatureParameterSlot::Diameter
                )
                .unwrap(),
            12.0
        );
        assert_eq!(
            model
                .resolved_feature_parameter_value(
                    &FeatureId("center_hole".into()),
                    FeatureParameterSlot::BlindDepth
                )
                .unwrap(),
            8.0
        );
        assert_ne!(
            diameter_target.parameter_id,
            original_depth_target.parameter_id
        );
        assert_eq!(
            model.feature_parameters[&FeatureId("center_hole".into())]
                [&FeatureParameterSlot::Diameter],
            diameter_target.parameter_id
        );
        assert_eq!(
            model.feature_parameters[&FeatureId("center_hole".into())]
                [&FeatureParameterSlot::BlindDepth],
            original_depth_target.parameter_id
        );
        assert_eq!(model.parameters.len(), 2);
        model.validate_model().unwrap();

        let round_trip: EngineeringModel =
            serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
        assert_eq!(round_trip, model);
    }

    #[test]
    fn typed_hole_parameter_targets_fail_closed_and_leave_model_unchanged() {
        let mut model =
            parametric_hole_model(HoleTermination::Blind { depth: 6.0 }, HoleKind::Simple);
        let original = model.clone();
        let target = typed_hole_target(&model, FeatureParameterSlot::Diameter);
        let mut bad_targets = vec![target.clone()];
        bad_targets[0].part_id = crate::PartId("other_part".into());
        let mut stale = target.clone();
        stale.parameter_id = ParameterId("stale_parameter".into());
        bad_targets.push(stale);
        let mut unsupported = target.clone();
        unsupported.slot = FeatureParameterSlot::CounterboreDiameter;
        bad_targets.push(unsupported);
        for bad_target in bad_targets {
            assert!(CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetFeatureParameter {
                    target: bad_target,
                    value: ParameterValue::Scalar(12.0),
                },
            )
            .is_err());
            assert_eq!(model, original);
        }
        for invalid_value in [
            ParameterValue::Scalar(0.0),
            ParameterValue::Scalar(-1.0),
            ParameterValue::Integer(12),
        ] {
            assert!(CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetFeatureParameter {
                    target: target.clone(),
                    value: invalid_value,
                },
            )
            .is_err());
            assert_eq!(model, original);
        }
    }

    #[test]
    fn typed_countersink_parameter_edits_preserve_units_and_reject_invalid_candidates_atomically() {
        let mut model = parametric_hole_model(
            HoleTermination::Blind { depth: 8.0 },
            HoleKind::Countersink {
                diameter: 14.0,
                included_angle_degrees: 90.0,
            },
        );
        let mouth_target =
            typed_hole_target(&model, FeatureParameterSlot::CountersinkMouthDiameter);
        let angle_target = typed_hole_target(
            &model,
            FeatureParameterSlot::CountersinkIncludedAngleDegrees,
        );
        assert_eq!(mouth_target.expected_type, ExpectedParameterType::LengthMm);
        assert_eq!(
            angle_target.expected_type,
            ExpectedParameterType::AngleDegrees
        );
        assert_eq!(angle_target.expected_type.unit(), "deg");

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureParameter {
                target: mouth_target.clone(),
                value: ParameterValue::Scalar(16.0),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureParameter {
                target: angle_target.clone(),
                value: ParameterValue::Scalar(120.0),
            },
        )
        .unwrap();
        assert_eq!(
            model
                .resolved_feature_parameter_value(
                    &FeatureId("center_hole".into()),
                    FeatureParameterSlot::CountersinkMouthDiameter
                )
                .unwrap(),
            16.0
        );
        assert_eq!(
            model
                .resolved_feature_parameter_value(
                    &FeatureId("center_hole".into()),
                    FeatureParameterSlot::CountersinkIncludedAngleDegrees
                )
                .unwrap(),
            120.0
        );
        let original = model.clone();
        for (target, value) in [
            (mouth_target, ParameterValue::Scalar(10.0)),
            (angle_target, ParameterValue::Scalar(180.0)),
        ] {
            assert!(CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetFeatureParameter { target, value }
            )
            .is_err());
            assert_eq!(
                model, original,
                "invalid Countersink edits must not alter the source EngineeringModel"
            );
        }
    }

    #[test]
    fn legacy_set_parameter_cannot_bypass_bound_hole_type_or_positive_length_checks() {
        let mut model =
            parametric_hole_model(HoleTermination::Blind { depth: 6.0 }, HoleKind::Simple);
        let diameter_target = typed_hole_target(&model, FeatureParameterSlot::Diameter);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureParameter {
                target: diameter_target.clone(),
                value: ParameterValue::Scalar(12.0),
            },
        )
        .unwrap();
        let parameter_id = diameter_target.parameter_id;
        let original = model.clone();
        for invalid_value in [ParameterValue::Integer(13), ParameterValue::Scalar(-1.0)] {
            assert!(CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetParameter {
                    target: parameter_id.clone(),
                    value: invalid_value,
                },
            )
            .is_err());
            assert_eq!(model, original);
        }
    }

    #[test]
    fn typed_parameter_edit_does_not_coerce_mismatched_existing_parameters() {
        let mut model =
            parametric_hole_model(HoleTermination::Blind { depth: 6.0 }, HoleKind::Simple);
        let integer_id = ParameterId("integer_diameter".into());
        model
            .create_parameter(Parameter::new(
                integer_id.clone(),
                ParameterValue::Integer(10),
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
        model
            .feature_parameters
            .entry(FeatureId("center_hole".into()))
            .or_default()
            .insert(FeatureParameterSlot::Diameter, integer_id);
        let original = model.clone();
        let target = typed_hole_target(&model, FeatureParameterSlot::Diameter);
        assert!(CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureParameter {
                target,
                value: ParameterValue::Scalar(12.0),
            },
        )
        .is_err());
        assert_eq!(
            model, original,
            "a wrong-typed bound value must not be silently converted"
        );

        let mut bind_model =
            parametric_hole_model(HoleTermination::Blind { depth: 6.0 }, HoleKind::Simple);
        let integer_id = ParameterId("integer_diameter".into());
        bind_model
            .create_parameter(Parameter::new(
                integer_id.clone(),
                ParameterValue::Integer(10),
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
        assert!(bind_model
            .bind_feature_parameter(
                FeatureId("center_hole".into()),
                FeatureParameterSlot::Diameter,
                integer_id,
            )
            .is_err());
        assert!(bind_model.feature_parameters.is_empty());
    }

    #[test]
    fn hole_parameter_capabilities_match_termination_and_counterbore_kind() {
        let blind = parametric_hole_model(HoleTermination::Blind { depth: 6.0 }, HoleKind::Simple);
        assert!(blind
            .feature_parameter_reference(
                &FeatureId("center_hole".into()),
                FeatureParameterSlot::BlindDepth
            )
            .is_ok());
        assert!(blind
            .feature_parameter_reference(
                &FeatureId("center_hole".into()),
                FeatureParameterSlot::CounterboreDepth
            )
            .is_err());

        let through = parametric_hole_model(HoleTermination::ThroughAll, HoleKind::Simple);
        assert!(through
            .feature_parameter_reference(
                &FeatureId("center_hole".into()),
                FeatureParameterSlot::BlindDepth
            )
            .is_err());

        let counterbore = parametric_hole_model(
            HoleTermination::Blind { depth: 8.0 },
            HoleKind::Counterbore {
                diameter: 14.0,
                depth: 4.0,
            },
        );
        for slot in [
            FeatureParameterSlot::Diameter,
            FeatureParameterSlot::BlindDepth,
            FeatureParameterSlot::CounterboreDiameter,
            FeatureParameterSlot::CounterboreDepth,
        ] {
            assert!(counterbore
                .feature_parameter_reference(&FeatureId("center_hole".into()), slot)
                .is_ok());
        }
        assert!(counterbore.validate_model().is_ok());

        let countersink = parametric_hole_model(
            HoleTermination::Blind { depth: 8.0 },
            HoleKind::Countersink {
                diameter: 14.0,
                included_angle_degrees: 90.0,
            },
        );
        assert!(countersink
            .feature_parameter_reference(
                &FeatureId("center_hole".into()),
                FeatureParameterSlot::CountersinkMouthDiameter,
            )
            .is_ok());
        assert_eq!(
            countersink
                .feature_parameter_reference(
                    &FeatureId("center_hole".into()),
                    FeatureParameterSlot::CountersinkIncludedAngleDegrees,
                )
                .unwrap()
                .expected_type,
            ExpectedParameterType::AngleDegrees
        );
        assert!(countersink
            .feature_parameter_reference(
                &FeatureId("center_hole".into()),
                FeatureParameterSlot::CounterboreDiameter,
            )
            .is_err());
        assert!(countersink.validate_model().is_ok());
    }

    // ---- CreatePart ----

    #[test]
    fn create_part_then_duplicate_is_rejected() {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(&mut model, &part()).unwrap();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &part()).unwrap_err(),
            EngineeringError::DuplicatePart { .. }
        ));
        assert_eq!(
            model.part.as_ref().unwrap().id,
            crate::PartId("plate".into())
        );
    }

    #[test]
    fn add_feature_without_part_is_rejected() {
        let mut model = EngineeringModel::new();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &base_profile()).unwrap_err(),
            EngineeringError::PartMissing
        ));
        assert!(model.features.is_empty());
    }

    // ---- AddFeature / RemoveFeature ----

    #[test]
    fn duplicate_feature_is_rejected() {
        let mut model = fixture_model();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &circular_pattern()).unwrap_err(),
            EngineeringError::DuplicateFeature { .. }
        ));
    }

    #[test]
    fn remove_missing_feature_is_rejected() {
        let mut model = fixture_model();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::RemoveFeature {
                    target: FeatureId("nope".into()),
                },
            )
            .unwrap_err(),
            EngineeringError::FeatureNotFound { .. }
        ));
    }

    #[test]
    fn remove_referenced_feature_is_rejected() {
        // §34: a circular pattern referenced by a Concentric constraint.
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddConstraint {
                constraint: EngineeringConstraint::Concentric {
                    id: EngineeringConstraintId("conc_1".into()),
                    source: FeatureId("bolt_circle".into()),
                    target: FeatureId("plate".into()),
                },
            },
        )
        .unwrap();
        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::RemoveFeature {
                target: FeatureId("plate".into()),
            },
        )
        .unwrap_err();
        assert!(matches!(err, EngineeringError::FeatureInUse { .. }));
        match err {
            EngineeringError::FeatureInUse { referenced_by, .. } => {
                assert_eq!(referenced_by, vec!["constraint conc_1"])
            }
            _ => unreachable!(),
        }
        assert!(model.features.contains_key(&FeatureId("plate".into())));
    }

    #[test]
    fn remove_unreferenced_feature_succeeds() {
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::RemoveFeature {
                target: FeatureId("bolt_circle".into()),
            },
        )
        .unwrap();
        assert!(!model
            .features
            .contains_key(&FeatureId("bolt_circle".into())));
        // Removing the base profile leaves the pattern removed too; the part
        // remains.
        assert!(model.part.is_some());
    }

    // ---- SetParameter / SetExpression ----

    #[test]
    fn set_parameter_on_missing_parameter_is_rejected() {
        let mut model = EngineeringModel::new();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetParameter {
                    target: ParameterId("width".into()),
                    value: ParameterValue::Scalar(120.0),
                },
            )
            .unwrap_err(),
            EngineeringError::ParameterNotFound { .. }
        ));
    }

    #[test]
    fn set_parameter_nan_is_rejected_and_model_unchanged() {
        // §35 atomicity: a failing SetParameter leaves the value untouched.
        let mut model = EngineeringModel::new();
        add_parameter(&mut model, "width", ParameterValue::Scalar(120.0));
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetParameter {
                    target: ParameterId("width".into()),
                    value: ParameterValue::Scalar(f64::NAN),
                },
            )
            .unwrap_err(),
            EngineeringError::NonFiniteValue { .. }
        ));
        assert_eq!(
            model.parameters[&ParameterId("width".into())].value,
            ParameterValue::Scalar(120.0)
        );
    }

    #[test]
    fn set_parameter_overwrites_expression_to_user_scalar() {
        let mut model = EngineeringModel::new();
        add_parameter(&mut model, "plate_width", ParameterValue::Scalar(80.0));
        let target = add_parameter(&mut model, "hole_diameter", ParameterValue::Unresolved);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: target.clone(),
                expression: Expression::divide_parameter_by_constant(
                    ParameterId("plate_width".into()),
                    8.0,
                ),
            },
        )
        .unwrap();
        assert_eq!(model.parameters[&target].origin, ParameterOrigin::Derived);
        assert!(matches!(
            model.parameters[&target].value,
            ParameterValue::Expression(_)
        ));

        // Overwrite with a direct value → origin becomes User, expression gone.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: target.clone(),
                value: ParameterValue::Scalar(12.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.parameters[&target].value,
            ParameterValue::Scalar(12.0)
        );
        assert_eq!(model.parameters[&target].origin, ParameterOrigin::User);
    }

    #[test]
    fn set_expression_rejects_missing_reference() {
        let mut model = EngineeringModel::new();
        let target = add_parameter(&mut model, "hole_diameter", ParameterValue::Unresolved);
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetExpression {
                    target: target.clone(),
                    expression: Expression::divide_parameter_by_constant(
                        ParameterId("missing".into()),
                        8.0,
                    ),
                },
            )
            .unwrap_err(),
            EngineeringError::ExpressionReferenceMissing { .. }
        ));
        // Model unchanged.
        assert!(matches!(
            model.parameters[&target].value,
            ParameterValue::Unresolved
        ));
    }

    #[test]
    fn set_expression_cycle_is_rejected_without_pollution() {
        // §33: A = B + 1, then B = A + 1 must fail and leave the model intact.
        let mut model = EngineeringModel::new();
        let a = add_parameter(&mut model, "a", ParameterValue::Unresolved);
        let b = add_parameter(&mut model, "b", ParameterValue::Unresolved);

        let plus_one = |reference: &str| Expression::Add {
            left: Box::new(Expression::Reference {
                parameter: ParameterId(reference.into()),
            }),
            right: Box::new(Expression::Constant { value: 1.0 }),
        };
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: a.clone(),
                expression: plus_one("b"),
            },
        )
        .unwrap();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetExpression {
                    target: b.clone(),
                    expression: plus_one("a"),
                },
            )
            .unwrap_err(),
            EngineeringError::ExpressionCycle { .. }
        ));
        // a still references b, b still unresolved.
        assert!(matches!(
            model.parameters[&a].value,
            ParameterValue::Expression(_)
        ));
        assert!(matches!(
            model.parameters[&b].value,
            ParameterValue::Unresolved
        ));
        assert_eq!(model.parameters[&a].origin, ParameterOrigin::Derived);
        assert_eq!(model.parameters[&b].origin, ParameterOrigin::User);
    }

    #[test]
    fn expression_propagation_after_parameter_change() {
        // §18: hole_diameter = plate_width / 8; 80 → 10, then 96 → 12.
        let mut model = EngineeringModel::new();
        let width = add_parameter(&mut model, "plate_width", ParameterValue::Scalar(80.0));
        let diameter = add_parameter(&mut model, "hole_diameter", ParameterValue::Unresolved);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: diameter.clone(),
                expression: Expression::divide_parameter_by_constant(
                    ParameterId("plate_width".into()),
                    8.0,
                ),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&diameter).unwrap(),
            crate::evaluator::ResolvedValue::Scalar(10.0)
        );

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: width,
                value: ParameterValue::Scalar(96.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&diameter).unwrap(),
            crate::evaluator::ResolvedValue::Scalar(12.0)
        );
    }

    // ---- DeclareParameter / BindFeatureParameter (Phase 4) ----

    #[test]
    fn declare_parameter_creates_parameter_via_command() {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("plate_width".into()),
                value: ParameterValue::Scalar(120.0),
                origin: crate::model::ParameterOrigin::User,
                authority: AuthorityLevel::L0,
            },
        )
        .unwrap();
        assert!(model
            .parameters
            .contains_key(&ParameterId("plate_width".into())));
        assert_eq!(
            model.parameters[&ParameterId("plate_width".into())].value,
            ParameterValue::Scalar(120.0)
        );
    }

    #[test]
    fn declare_parameter_duplicate_or_invalid_is_atomic() {
        let mut model = EngineeringModel::new();
        let declare = || EngineeringCommand::DeclareParameter {
            id: ParameterId("plate_width".into()),
            value: ParameterValue::Scalar(120.0),
            origin: crate::model::ParameterOrigin::User,
            authority: AuthorityLevel::L0,
        };
        CommandExecutor::apply(&mut model, &declare()).unwrap();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &declare()).unwrap_err(),
            EngineeringError::DuplicateParameter { .. }
        ));
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::DeclareParameter {
                    id: ParameterId("nan".into()),
                    value: ParameterValue::Scalar(f64::NAN),
                    origin: crate::model::ParameterOrigin::User,
                    authority: AuthorityLevel::L0,
                },
            )
            .unwrap_err(),
            EngineeringError::NonFiniteValue { .. }
        ));
        assert_eq!(model.parameters.len(), 1);
    }

    #[test]
    fn bind_feature_parameter_command_binds_slot() {
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("plate_width".into()),
                value: ParameterValue::Scalar(120.0),
                origin: crate::model::ParameterOrigin::User,
                authority: AuthorityLevel::L0,
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("plate".into()),
                slot: crate::binding::FeatureParameterSlot::Width,
                parameter: ParameterId("plate_width".into()),
            },
        )
        .unwrap();
        assert_eq!(
            model.feature_parameters[&FeatureId("plate".into())]
                [&crate::binding::FeatureParameterSlot::Width],
            ParameterId("plate_width".into())
        );
    }

    #[test]
    fn bind_feature_parameter_invalid_slot_is_atomic() {
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("x".into()),
                value: ParameterValue::Scalar(1.0),
                origin: crate::model::ParameterOrigin::User,
                authority: AuthorityLevel::L0,
            },
        )
        .unwrap();
        // Hole (fixture has no hole, but bind to the plate base with an
        // invalid slot for a rectangle).
        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("plate".into()),
                slot: crate::binding::FeatureParameterSlot::StartAngle,
                parameter: ParameterId("x".into()),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            EngineeringError::InvalidFeatureParameterBinding { .. }
        ));
        assert!(model.feature_parameters.is_empty());
    }

    // ---- Constraints ----

    #[test]
    fn add_constraint_with_missing_feature_is_rejected() {
        let mut model = fixture_model();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::AddConstraint {
                    constraint: EngineeringConstraint::Fixed {
                        id: EngineeringConstraintId("c1".into()),
                        target: FeatureId("ghost".into()),
                    },
                },
            )
            .unwrap_err(),
            EngineeringError::FeatureNotFound { .. }
        ));
    }

    #[test]
    fn duplicate_and_missing_constraint() {
        let mut model = fixture_model();
        let constraint = || EngineeringCommand::AddConstraint {
            constraint: EngineeringConstraint::Fixed {
                id: EngineeringConstraintId("c1".into()),
                target: FeatureId("plate".into()),
            },
        };
        CommandExecutor::apply(&mut model, &constraint()).unwrap();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &constraint()).unwrap_err(),
            EngineeringError::DuplicateConstraint { .. }
        ));

        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::RemoveConstraint {
                    id: EngineeringConstraintId("ghost".into()),
                },
            )
            .unwrap_err(),
            EngineeringError::ConstraintNotFound { .. }
        ));
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::RemoveConstraint {
                id: EngineeringConstraintId("c1".into()),
            },
        )
        .unwrap();
        assert!(model.constraints.is_empty());
    }

    // ---- MoveFeature ----

    #[test]
    fn move_feature_updates_circular_pattern_center() {
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::MoveFeature {
                target: FeatureId("bolt_circle".into()),
                position: Point2D::new(40.0, 30.0),
            },
        )
        .unwrap();
        let feature = &model.features[&FeatureId("bolt_circle".into())];
        match feature {
            EngineeringFeature::HolePattern { pattern, .. } => match pattern {
                HolePattern::Circular { center, .. } => {
                    assert_eq!(*center, Point2D::new(40.0, 30.0));
                }
                _ => panic!("expected circular pattern"),
            },
            _ => panic!("expected hole pattern"),
        }
    }

    #[test]
    fn move_feature_on_feature_without_position_is_rejected() {
        let mut model = fixture_model();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::MoveFeature {
                    target: FeatureId("plate".into()),
                    position: Point2D::new(1.0, 1.0),
                },
            )
            .unwrap_err(),
            EngineeringError::UnsupportedMove { .. }
        ));
    }

    // ---- SetDrawingProperty ----

    #[test]
    fn set_drawing_property_updates_drawing_state() {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetDrawingProperty {
                property: DrawingProperty::ProjectionMethod {
                    method: ProjectionMethod::ThirdAngle,
                },
            },
        )
        .unwrap();
        assert!(matches!(
            model.drawing.get(DrawingPropertyKind::ProjectionMethod),
            Some(DrawingProperty::ProjectionMethod { .. })
        ));

        // Replacing the same kind is idempotent (one entry per kind).
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetDrawingProperty {
                property: DrawingProperty::ProjectionMethod {
                    method: ProjectionMethod::FirstAngle,
                },
            },
        )
        .unwrap();
        assert_eq!(model.drawing.properties.len(), 1);

        // Non-positive scale is rejected and the state is unchanged.
        let before = model.drawing.clone();
        assert!(CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetDrawingProperty {
                property: DrawingProperty::Scale { value: 0.0 },
            },
        )
        .is_err());
        assert_eq!(model.drawing, before);
    }

    // ---- Batch atomicity ----

    #[test]
    fn apply_all_is_all_or_nothing() {
        let mut model = fixture_model();
        let before = model.clone();
        let commands = vec![
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("h1".into()),
                    diameter: 10.0,
                },
            },
            // Failing command (feature already exists) → whole batch rejected.
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("h1".into()),
                    diameter: 5.0,
                },
            },
        ];
        assert!(CommandExecutor::apply_all(&mut model, &commands).is_err());
        assert_eq!(model, before, "failed batch must not change the model");
    }

    #[test]
    fn apply_all_commits_when_all_succeed() {
        let mut model = fixture_model();
        let commands = vec![
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("h1".into()),
                    diameter: 10.0,
                },
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Fillet {
                    id: FeatureId("f1".into()),
                    radius: 5.0,
                },
            },
        ];
        CommandExecutor::apply_all(&mut model, &commands).unwrap();
        assert!(model.features.contains_key(&FeatureId("h1".into())));
        assert!(model.features.contains_key(&FeatureId("f1".into())));
    }

    // ---- SetPartThickness / SetHoleExtent (Phase 9.1) ----

    use crate::spatial::{HoleExtent, HoleStartFace, SourcedLength};

    fn set_thickness_command(value: f64) -> EngineeringCommand {
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value }),
        }
    }

    fn blind_extent(depth: f64) -> Option<HoleExtent> {
        Some(HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: depth },
        })
    }

    #[test]
    fn set_part_thickness_stores_literal() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(8.0)).unwrap();
        assert_eq!(
            model.spatial.thickness,
            Some(SourcedLength::Literal { value: 8.0 })
        );
    }

    #[test]
    fn set_part_thickness_rejects_non_finite_and_non_positive() {
        let mut model = fixture_model();
        for bad in [f64::NAN, f64::INFINITY, 0.0, -2.0] {
            let err = CommandExecutor::apply(&mut model, &set_thickness_command(bad)).unwrap_err();
            assert!(
                matches!(
                    err,
                    EngineeringError::NonFiniteValue { .. }
                        | EngineeringError::NonPositiveLength { .. }
                ),
                "thickness {} must be rejected, got {:?}",
                bad,
                err
            );
        }
        assert_eq!(model.spatial.thickness, None);
    }

    #[test]
    fn set_part_thickness_rejects_unknown_parameter() {
        let mut model = fixture_model();
        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Parameter {
                    id: ParameterId("missing".into()),
                }),
            },
        )
        .unwrap_err();
        assert!(matches!(err, EngineeringError::ParameterNotFound { .. }));
        assert_eq!(model.spatial.thickness, None);
    }

    #[test]
    fn set_part_thickness_rejects_malformed_parameter_id_before_lookup() {
        // A malformed parameter id must surface as InvalidId at command
        // validation, never as a misleading SCC-ENG-017 lookup miss.
        let mut model = fixture_model();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetPartThickness {
                    source: Some(SourcedLength::Parameter {
                        id: ParameterId("Bad_Id".into()),
                    }),
                },
            )
            .unwrap_err(),
            EngineeringError::InvalidId { .. }
        ));
        assert_eq!(model.spatial.thickness, None);
    }

    #[test]
    fn set_part_thickness_accepts_known_parameter() {
        let mut model = fixture_model();
        add_parameter(&mut model, "t", ParameterValue::Scalar(10.0));
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Parameter {
                    id: ParameterId("t".into()),
                }),
            },
        )
        .unwrap();
        assert_eq!(
            model.spatial.thickness,
            Some(SourcedLength::Parameter {
                id: ParameterId("t".into())
            })
        );
    }

    #[test]
    fn set_part_thickness_without_part_is_rejected() {
        let mut model = EngineeringModel::new();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &set_thickness_command(8.0)).unwrap_err(),
            EngineeringError::PartMissing
        ));
        assert!(model.spatial.is_empty());
    }

    #[test]
    fn set_hole_extent_on_hole_and_pattern_stores_one_entry_each() {
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        )
        .unwrap();
        // A pattern stays a single feature; its extent stays a single entry.
        assert_eq!(model.spatial.hole_extents.len(), 1);
        assert_eq!(
            model
                .spatial
                .hole_extents
                .get(&FeatureId("bolt_circle".into())),
            Some(&HoleExtent::ThroughAll)
        );

        // A literal hole also gets an extent.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("center_hole".into()),
                    diameter: 10.0,
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("center_hole".into()),
                extent: blind_extent(4.0),
            },
        )
        .unwrap();
        assert_eq!(model.spatial.hole_extents.len(), 2);
    }

    #[test]
    fn set_hole_extent_on_base_profile_is_invalid_target() {
        let mut model = fixture_model();
        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("plate".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        )
        .unwrap_err();
        assert!(matches!(err, EngineeringError::InvalidSpatialTarget { .. }));
        assert!(model.spatial.hole_extents.is_empty());
    }

    #[test]
    fn set_hole_extent_on_unknown_feature_is_rejected() {
        let mut model = fixture_model();
        assert!(matches!(
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetHoleExtent {
                    target: FeatureId("ghost".into()),
                    extent: Some(HoleExtent::ThroughAll),
                },
            )
            .unwrap_err(),
            EngineeringError::FeatureNotFound { .. }
        ));
    }

    #[test]
    fn literal_blind_depth_conflict_is_atomically_rejected() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(8.0)).unwrap();
        let before = model.clone();

        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(8.0), // depth == thickness → reject
            },
        )
        .unwrap_err();
        match &err {
            EngineeringError::DepthExceedsThickness {
                feature_id,
                depth,
                thickness,
            } => {
                assert_eq!(feature_id.0, "bolt_circle");
                assert_eq!(*depth, 8.0);
                assert_eq!(*thickness, 8.0);
            }
            other => panic!("expected DepthExceedsThickness, got {:?}", other),
        }
        // Field-by-field atomicity: nothing changed anywhere.
        assert_eq!(model, before);

        // Depth below thickness succeeds.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(4.0),
            },
        )
        .unwrap();
        assert_eq!(
            model
                .spatial
                .hole_extents
                .get(&FeatureId("bolt_circle".into())),
            blind_extent(4.0).as_ref()
        );
    }

    #[test]
    fn parameter_blind_depth_defers_conflict_to_readiness() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(8.0)).unwrap();
        add_parameter(&mut model, "d", ParameterValue::Scalar(20.0));
        // No eager rejection: the conflict is parameter-mediated.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: Some(HoleExtent::Blind {
                    start_face: HoleStartFace::Opposite,
                    depth: SourcedLength::Parameter {
                        id: ParameterId("d".into()),
                    },
                }),
            },
        )
        .unwrap();
        let readiness = model.validate_spatial_readiness();
        assert!(!readiness.ready);
        assert!(readiness.issues.iter().any(|issue| matches!(
            issue,
            crate::spatial::SpatialIssue::InvalidBlindDepth { depth, .. } if *depth == 20.0
        )));
    }

    #[test]
    fn setting_thickness_below_existing_literal_blind_depth_rejected() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(10.0)).unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap();
        let before = model.clone();
        let before_json = serde_json::to_vec(&model).unwrap();

        let err = CommandExecutor::apply(&mut model, &set_thickness_command(4.0)).unwrap_err();
        match &err {
            EngineeringError::DepthExceedsThickness {
                feature_id,
                depth,
                thickness,
            } => {
                assert_eq!(feature_id.0, "bolt_circle");
                assert_eq!(*depth, 6.0);
                assert_eq!(*thickness, 4.0);
            }
            other => panic!("expected DepthExceedsThickness, got {:?}", other),
        }
        // Atomicity: the rejected command leaves the model PartialEq-identical
        // and its serialization byte-identical.
        assert_eq!(model, before);
        assert_eq!(serde_json::to_vec(&model).unwrap(), before_json);
    }

    #[test]
    fn setting_blind_depth_above_existing_thickness_rejected() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(4.0)).unwrap();
        let before = model.clone();

        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            EngineeringError::DepthExceedsThickness { .. }
        ));
        // Atomicity: nothing changed.
        assert_eq!(model, before);
    }

    #[test]
    fn thickness_blind_equality_rejected_in_both_orders() {
        // (i) Thickness first, then an equal blind depth.
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(6.0)).unwrap();
        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            EngineeringError::DepthExceedsThickness { .. }
        ));

        // (ii) Blind depth first (no thickness yet → Ok), then an equal
        // thickness — the symmetric direction also rejects equality.
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap();
        assert!(matches!(
            CommandExecutor::apply(&mut model, &set_thickness_command(6.0)).unwrap_err(),
            EngineeringError::DepthExceedsThickness { .. }
        ));
        // The rejected write left both spatial values untouched.
        assert_eq!(model.spatial.thickness, None);
        assert_eq!(
            model
                .spatial
                .hole_extents
                .get(&FeatureId("bolt_circle".into())),
            blind_extent(6.0).as_ref()
        );
    }

    #[test]
    fn thickness_change_that_stays_valid_succeeds() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(10.0)).unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap();

        CommandExecutor::apply(&mut model, &set_thickness_command(8.0)).unwrap();
        assert_eq!(
            model.spatial.thickness,
            Some(SourcedLength::Literal { value: 8.0 })
        );
        let readiness = model.validate_spatial_readiness();
        assert!(readiness.ready);
        assert!(!readiness.issues.iter().any(|issue| matches!(
            issue,
            crate::spatial::SpatialIssue::InvalidBlindDepth { .. }
                | crate::spatial::SpatialIssue::InvalidThickness { .. }
        )));
    }

    #[test]
    fn derived_thickness_rederived_on_every_readiness_call() {
        let mut model = fixture_model();
        add_parameter(&mut model, "base", ParameterValue::Scalar(5.0));
        let t = add_parameter(&mut model, "t", ParameterValue::Unresolved);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: t.clone(),
                expression: Expression::Multiply {
                    left: Box::new(Expression::Reference {
                        parameter: ParameterId("base".into()),
                    }),
                    right: Box::new(Expression::Constant { value: 2.0 }),
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Parameter { id: t }),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(8.0),
            },
        )
        .unwrap();
        // base = 5 → thickness 10 > 8: ready.
        assert!(model.validate_spatial_readiness().ready);

        // SetParameter never scans spatial; the drift (base = 3 → thickness
        // 6 < 8) is detected at the next readiness call.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: ParameterId("base".into()),
                value: ParameterValue::Scalar(3.0),
            },
        )
        .unwrap();
        let readiness = model.validate_spatial_readiness();
        assert!(!readiness.ready);
        assert!(readiness.issues.iter().any(|issue| matches!(
            issue,
            crate::spatial::SpatialIssue::InvalidBlindDepth { depth, .. } if *depth == 8.0
        )));
    }

    #[test]
    fn thickness_conflict_reports_first_feature_in_btree_order() {
        let mut model = fixture_model();
        for (id, depth) in [("a_hole", 7.0), ("b_hole", 6.0)] {
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::Hole {
                        id: FeatureId(id.into()),
                        diameter: 5.0,
                    },
                },
            )
            .unwrap();
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::SetHoleExtent {
                    target: FeatureId(id.into()),
                    extent: blind_extent(depth),
                },
            )
            .unwrap();
        }
        CommandExecutor::apply(&mut model, &set_thickness_command(10.0)).unwrap();

        let err = CommandExecutor::apply(&mut model, &set_thickness_command(5.0)).unwrap_err();
        match &err {
            EngineeringError::DepthExceedsThickness { feature_id, .. } => {
                assert_eq!(feature_id.0, "a_hole");
            }
            other => panic!("expected DepthExceedsThickness, got {:?}", other),
        }
    }

    #[test]
    fn clearing_paths_unaffected_by_conflicts() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(10.0)).unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap();

        // Clearing the thickness is allowed even though a blind depth exists.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetPartThickness { source: None },
        )
        .unwrap();
        assert_eq!(model.spatial.thickness, None);

        // Clearing the extent is allowed in another fixture too.
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(10.0)).unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(6.0),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: None,
            },
        )
        .unwrap();
        assert!(!model
            .spatial
            .hole_extents
            .contains_key(&FeatureId("bolt_circle".into())));
    }

    #[test]
    fn clearing_spatial_values_via_none() {
        let mut model = fixture_model();
        CommandExecutor::apply(&mut model, &set_thickness_command(8.0)).unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        )
        .unwrap();
        assert!(!model.spatial.is_empty());

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetPartThickness { source: None },
        )
        .unwrap();
        assert_eq!(model.spatial.thickness, None);

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: None,
            },
        )
        .unwrap();
        assert!(model.spatial.hole_extents.is_empty());
        assert!(model.spatial.is_empty());
    }

    #[test]
    fn remove_feature_cleans_up_its_extent_entry() {
        let mut model = fixture_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        )
        .unwrap();
        assert!(model
            .spatial
            .hole_extents
            .contains_key(&FeatureId("bolt_circle".into())));

        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::RemoveFeature {
                target: FeatureId("bolt_circle".into()),
            },
        )
        .unwrap();
        assert!(!model
            .features
            .contains_key(&FeatureId("bolt_circle".into())));
        assert!(!model
            .spatial
            .hole_extents
            .contains_key(&FeatureId("bolt_circle".into())));
    }

    #[test]
    fn spatial_batch_snapshot_semantics() {
        // apply_all is all-or-nothing: a failing SetHoleExtent rolls the
        // whole batch back, including earlier successful commands.
        let mut model = fixture_model();
        let before = model.clone();
        let commands = vec![
            set_thickness_command(8.0),
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(9.0), // conflicts with T=8 → fail
            },
        ];
        assert!(CommandExecutor::apply_all(&mut model, &commands).is_err());
        assert_eq!(model, before, "failed batch must leave the model untouched");

        // The same batch with a valid depth commits atomically.
        let commands = vec![
            set_thickness_command(8.0),
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("bolt_circle".into()),
                extent: blind_extent(4.0),
            },
        ];
        CommandExecutor::apply_all(&mut model, &commands).unwrap();
        assert_eq!(
            model.spatial.thickness,
            Some(SourcedLength::Literal { value: 8.0 })
        );
        assert!(model
            .spatial
            .hole_extents
            .contains_key(&FeatureId("bolt_circle".into())));
    }
}
