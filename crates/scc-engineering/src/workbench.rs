//! A derived, read-only workbench view for the UI (Phase 7, ADR-021).
//!
//! `EngineeringWorkbenchView` is the single source of what the UI workbench
//! renders. It is **derived** from the [`EngineeringModel`] on demand — never
//! stored, never writable, and not a second source of truth. The UI never
//! computes derived values or dependency lists itself; it only renders this
//! view and sends edits back as tool calls.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::binding::{feature_field_slots, FeatureParameterReference, FeatureParameterSlot};
use crate::evaluator::ResolvedValue;
use crate::feature::{EngineeringConstraint, EngineeringFeature};
use crate::model::{EngineeringModel, ParameterOrigin};
use crate::parameter::ParameterValue;

/// A parameter row the workbench can render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchParameter {
    pub id: String,
    /// Resolved display value, `UNRESOLVED`, or the canonical expression for
    /// derived parameters.
    pub value: String,
    pub origin: String,
    /// Whether the UI may edit this parameter directly (derived values are
    /// read-only; AI assumptions are disabled).
    pub editable: bool,
    /// Canonical formula, present for derived parameters.
    pub expression: Option<String>,
    /// Parameter ids this parameter depends on (from the real dependency
    /// graph, never parsed from text).
    pub dependencies: Vec<String>,
}

/// One typed slot of a feature, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchSlot {
    pub slot: String,
    pub label: String,
    pub display_value: String,
    /// `parameter:<id>`, `literal`, `unresolved`, or `default`.
    pub source: String,
    pub parameter: Option<String>,
    pub editable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_reference: Option<FeatureParameterReference>,
}

/// A feature row with its slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchFeature {
    pub id: String,
    pub feature_type: String,
    pub slots: Vec<WorkbenchSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topology_references: Vec<WorkbenchTopologyReference>,
}

/// Semantic topology provenance shown beside the consuming feature. Resolution
/// is reported by the UI only after the matching model revision rebuilds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchTopologyReference {
    pub source_feature_id: String,
    pub role: String,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// A constraint row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchConstraint {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub target: String,
}

/// A drawing property row (data only; no drawing engine).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchDrawingProperty {
    pub kind: String,
    pub value: String,
}

/// Counts for the Review panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchSummary {
    pub confirmed: usize,
    pub derived: usize,
    pub default: usize,
    pub unresolved: usize,
    pub warnings: usize,
}

/// The derived workbench view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineeringWorkbenchView {
    pub part: Option<crate::context::ContextPart>,
    pub features: Vec<WorkbenchFeature>,
    pub parameters: Vec<WorkbenchParameter>,
    pub constraints: Vec<WorkbenchConstraint>,
    pub unresolved: Vec<String>,
    pub drawing: Vec<WorkbenchDrawingProperty>,
    pub summary: WorkbenchSummary,
}

impl EngineeringModel {
    /// Derive the read-only workbench view (deterministic ordering).
    pub fn workbench_view(&self) -> EngineeringWorkbenchView {
        let resolved = self.evaluate_all().ok();
        let graph = self.dependency_graph();

        let mut parameters = Vec::new();
        let mut unresolved = Vec::new();
        let mut summary = WorkbenchSummary {
            confirmed: 0,
            derived: 0,
            default: 0,
            unresolved: 0,
            warnings: 0,
        };
        for (id, parameter) in &self.parameters {
            let origin = origin_name(parameter.origin);
            let is_unresolved = matches!(parameter.value, ParameterValue::Unresolved)
                || matches!(
                    resolved.as_ref().and_then(|m| m.get(id)),
                    Some(ResolvedValue::Unresolved)
                );
            let value = if is_unresolved {
                "UNRESOLVED".to_string()
            } else {
                parameter_display(resolved.as_ref().and_then(|m| m.get(id)))
            };
            let (expression, dependencies) = match &parameter.value {
                ParameterValue::Expression(e) => {
                    let deps: Vec<String> = graph.depends_on(id).into_iter().map(|d| d.0).collect();
                    (Some(e.to_string()), deps)
                }
                _ => (None, Vec::new()),
            };
            let editable = matches!(
                parameter.origin,
                ParameterOrigin::User | ParameterOrigin::Default
            ) && !is_unresolved;
            if is_unresolved {
                unresolved.push(id.0.clone());
                summary.unresolved += 1;
                summary.warnings += 1;
            } else {
                match parameter.origin {
                    ParameterOrigin::User => summary.confirmed += 1,
                    ParameterOrigin::Derived => summary.derived += 1,
                    ParameterOrigin::Default => summary.default += 1,
                    ParameterOrigin::AiAssumption => {}
                }
            }
            parameters.push(WorkbenchParameter {
                id: id.0.clone(),
                value,
                origin: origin.to_string(),
                editable,
                expression,
                dependencies,
            });
        }

        let mut features = Vec::new();
        for id in self.ordered_feature_ids() {
            if let Some(feature) = self.features.get(&id) {
                features.push(workbench_feature(feature, self, resolved.as_ref()));
            }
        }

        let mut constraints = Vec::new();
        for constraint in self.constraints.values() {
            let (source, target) = constraint_reference_summary(constraint);
            constraints.push(WorkbenchConstraint {
                id: constraint.id().0.clone(),
                kind: constraint_kind_name(constraint).to_string(),
                source,
                target,
            });
        }

        let mut drawing = Vec::new();
        for (kind, property) in &self.drawing.properties {
            drawing.push(WorkbenchDrawingProperty {
                kind: format!("{:?}", kind).to_lowercase(),
                value: drawing_value(property),
            });
        }

        EngineeringWorkbenchView {
            part: self.part.as_ref().map(|p| crate::context::ContextPart {
                id: p.id.0.clone(),
                name: p.name.clone(),
            }),
            features,
            parameters,
            constraints,
            unresolved,
            drawing,
            summary,
        }
    }
}

fn workbench_feature(
    feature: &EngineeringFeature,
    model: &EngineeringModel,
    resolved: Option<&BTreeMap<crate::ParameterId, ResolvedValue>>,
) -> WorkbenchFeature {
    let literals = feature_field_slots(feature);
    let bindings = model.feature_parameters.get(feature.id());
    let mut slots = Vec::new();
    for slot in FeatureParameterSlot::valid_for(feature) {
        let bound = bindings.and_then(|m| m.get(&slot));
        let (display, source, parameter, editable) = if let Some(parameter_id) = bound {
            let resolved_value = resolved
                .and_then(|m| m.get(parameter_id))
                .map(resolved_display)
                .unwrap_or_else(|| "UNRESOLVED".to_string());
            let param = model.parameters.get(parameter_id);
            let is_unresolved = resolved_value == "UNRESOLVED";
            let editable = param.is_some_and(|p| {
                matches!(p.origin, ParameterOrigin::User | ParameterOrigin::Default)
                    && !is_unresolved
            });
            (
                resolved_value,
                format!("parameter:{}", parameter_id.0),
                Some(parameter_id.0.clone()),
                editable,
            )
        } else if let Some(literal) = literals.get(&slot) {
            (format!("{}", literal), "literal".to_string(), None, false)
        } else {
            ("0".to_string(), "default".to_string(), None, false)
        };
        let capability = FeatureParameterSlot::capability_for(feature, slot);
        let parameter_reference =
            capability.and_then(|_| model.feature_parameter_reference(feature.id(), slot).ok());
        let capability_editable = capability.is_some()
            && if bound.is_none() {
                true
            } else {
                editable
                    && model
                        .resolved_feature_parameter_value(feature.id(), slot)
                        .is_ok()
            };
        slots.push(WorkbenchSlot {
            slot: slot.to_string(),
            label: slot.to_string(),
            display_value: display,
            source,
            parameter,
            editable: if capability.is_some() {
                capability_editable
            } else {
                editable
            },
            unit: capability
                .map(|value| value.expected_type.unit().to_string())
                .or_else(|| feature_parameter_unit(slot).map(str::to_string)),
            parameter_reference,
        });
    }
    if let EngineeringFeature::Parametric { feature, .. } = feature {
        match feature {
            crate::parametric::ParametricFeature::Path { support, segments } => {
                slots.push(readonly_slot("support", format!("{support:?}")));
                slots.push(readonly_slot(
                    "path_segment_count",
                    segments.len().to_string(),
                ));
                slots.push(readonly_slot(
                    "path_segment_ids",
                    segments
                        .iter()
                        .map(|segment| segment.id.0.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
            crate::parametric::ParametricFeature::Loft {
                sections,
                operation,
                mode,
                orientation_policy,
            } => {
                slots.push(readonly_slot(
                    "section_sketch_refs",
                    sections
                        .iter()
                        .map(|id| id.0.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                slots.push(readonly_slot(
                    "operation",
                    format!("{operation:?}").to_lowercase(),
                ));
                slots.push(readonly_slot(
                    "loft_mode",
                    format!("{mode:?}").to_lowercase(),
                ));
                slots.push(readonly_slot(
                    "orientation_policy",
                    format!("{orientation_policy:?}").to_lowercase(),
                ));
            }
            crate::parametric::ParametricFeature::Sweep {
                profile,
                path,
                operation,
                direction,
                profile_origin,
                orientation,
            } => {
                slots.push(readonly_slot("profile", profile.0.clone()));
                slots.push(readonly_slot("path", path.0.clone()));
                slots.push(readonly_slot(
                    "operation",
                    format!("{operation:?}").to_lowercase(),
                ));
                slots.push(readonly_slot(
                    "direction",
                    format!("{direction:?}").to_lowercase(),
                ));
                slots.push(readonly_slot(
                    "orientation",
                    match orientation {
                        crate::parametric::SweepOrientation::FixedBinormal => {
                            "fixed_binormal".to_string()
                        }
                    },
                ));
                slots.push(readonly_slot(
                    "profile_origin",
                    format!("({:.6}, {:.6}) mm", profile_origin.x, profile_origin.y),
                ));
            }
            _ => {}
        }
    }
    WorkbenchFeature {
        id: feature.id().0.clone(),
        feature_type: feature_type_name(feature).to_string(),
        slots,
        topology_references: workbench_topology_references(feature),
    }
}

fn workbench_topology_references(feature: &EngineeringFeature) -> Vec<WorkbenchTopologyReference> {
    use crate::parametric::{
        GeneratedFaceSelector as Selector, ParametricFeature as Parametric, PlaneReference,
    };
    let mut references = Vec::new();
    let mut append = |source: &crate::FeatureId, selector: &Selector, relation: &str| {
        let (role, identity) = match selector {
            Selector::SweepStartCap => ("sweep_start_cap", None),
            Selector::SweepEndCap => ("sweep_end_cap", None),
            Selector::SweepLateral { profile_entity_id } => {
                ("sweep_lateral", Some(profile_entity_id.0.clone()))
            }
            Selector::LoftStartCap => ("loft_start_cap", None),
            Selector::LoftEndCap => ("loft_end_cap", None),
            Selector::LoftLateral {
                start_section_id,
                end_section_id,
                start_entity_id,
                end_entity_id,
            } => (
                "loft_lateral",
                Some(format!(
                    "{}:{} → {}:{}",
                    start_section_id.0, start_entity_id.0, end_section_id.0, end_entity_id.0
                )),
            ),
        };
        references.push(WorkbenchTopologyReference {
            source_feature_id: source.0.clone(),
            role: role.to_string(),
            relation: relation.to_string(),
            identity,
            status: "awaiting_rebuild".to_string(),
            diagnostic: None,
        });
    };
    if let EngineeringFeature::Parametric { feature, .. } = feature {
        match feature {
            Parametric::Hole {
                support: PlaneReference::GeneratedFace { reference },
                ..
            } => {
                append(
                    &reference.source_feature_id,
                    &reference.selector,
                    "support_plane",
                );
            }
            Parametric::Fillet { edge_refs, .. } | Parametric::Chamfer { edge_refs, .. } => {
                for edge in edge_refs {
                    if let crate::parametric::SemanticEdgeSelector::BetweenGeneratedFaces {
                        first,
                        second,
                    } = &edge.selector
                    {
                        append(
                            &first.source_feature_id,
                            &first.selector,
                            "intersection_edge",
                        );
                        append(
                            &second.source_feature_id,
                            &second.selector,
                            "intersection_edge",
                        );
                    }
                }
            }
            _ => {}
        }
    }
    references.sort_by(|a, b| {
        (&a.source_feature_id, &a.role, &a.identity).cmp(&(
            &b.source_feature_id,
            &b.role,
            &b.identity,
        ))
    });
    references.dedup();
    references
}

fn readonly_slot(slot: &str, display_value: String) -> WorkbenchSlot {
    WorkbenchSlot {
        slot: slot.to_string(),
        label: slot.to_string(),
        display_value,
        source: "literal".to_string(),
        parameter: None,
        editable: false,
        unit: None,
        parameter_reference: None,
    }
}

fn parameter_display(resolved: Option<&ResolvedValue>) -> String {
    resolved_display(resolved.unwrap_or(&ResolvedValue::Unresolved))
}

fn resolved_display(value: &ResolvedValue) -> String {
    match value {
        ResolvedValue::Scalar(v) => format!("{}", v),
        ResolvedValue::Integer(i) => format!("{}", i),
        ResolvedValue::Boolean(b) => format!("{}", b),
        ResolvedValue::Choice(c) => c.clone(),
        ResolvedValue::Unresolved => "UNRESOLVED".to_string(),
    }
}

fn feature_parameter_unit(slot: FeatureParameterSlot) -> Option<&'static str> {
    use FeatureParameterSlot as Slot;
    match slot {
        Slot::CenterX
        | Slot::CenterY
        | Slot::Width
        | Slot::Height
        | Slot::OffsetX
        | Slot::OffsetY
        | Slot::Diameter
        | Slot::BlindDepth
        | Slot::CounterboreDiameter
        | Slot::CounterboreDepth
        | Slot::CountersinkMouthDiameter
        | Slot::Radius
        | Slot::Length
        | Slot::SpacingX
        | Slot::SpacingY
        | Slot::PitchCircleDiameter => Some("mm"),
        Slot::Orientation | Slot::StartAngle | Slot::CountersinkIncludedAngleDegrees => Some("deg"),
        Slot::Count | Slot::Rows | Slot::Columns => None,
    }
}

fn origin_name(origin: ParameterOrigin) -> &'static str {
    match origin {
        ParameterOrigin::User => "user",
        ParameterOrigin::Derived => "derived",
        ParameterOrigin::Default => "default",
        ParameterOrigin::AiAssumption => "ai_assumption",
    }
}

fn feature_type_name(feature: &EngineeringFeature) -> &'static str {
    match feature {
        EngineeringFeature::BaseProfile { .. } => "base_profile",
        EngineeringFeature::Hole { .. } => "hole",
        EngineeringFeature::HolePattern { pattern, .. } => match pattern {
            crate::feature::HolePattern::Circular { .. } => "hole_pattern:circular",
            crate::feature::HolePattern::Grid { .. } => "hole_pattern:grid",
            crate::feature::HolePattern::Corner { .. } => "hole_pattern:corner",
        },
        EngineeringFeature::Slot { .. } => "slot",
        EngineeringFeature::Fillet { .. } => "fillet",
        EngineeringFeature::ConstructionCircle { .. } => "construction_circle",
        EngineeringFeature::Parametric { feature, .. } => match feature {
            crate::feature::ParametricFeature::DatumPlane { .. } => "datum_plane",
            crate::feature::ParametricFeature::DatumAxis { .. } => "datum_axis",
            crate::feature::ParametricFeature::Sketch { .. } => "sketch",
            crate::feature::ParametricFeature::Path { .. } => "path",
            crate::feature::ParametricFeature::Sweep { .. } => "sweep",
            crate::feature::ParametricFeature::Loft { .. } => "loft",
            crate::feature::ParametricFeature::Extrude { .. } => "extrude",
            crate::feature::ParametricFeature::Revolve { .. } => "revolve",
            crate::feature::ParametricFeature::Hole { .. } => "hole",
            crate::feature::ParametricFeature::Pattern { .. } => "pattern",
            crate::feature::ParametricFeature::Fillet { .. } => "fillet",
            crate::feature::ParametricFeature::Chamfer { .. } => "chamfer",
        },
    }
}

fn constraint_kind_name(constraint: &EngineeringConstraint) -> &'static str {
    use EngineeringConstraint as C;
    match constraint {
        C::Symmetric { .. } => "symmetric",
        C::Aligned { .. } => "aligned",
        C::Distance { .. } => "distance",
        C::Tangent { .. } => "tangent",
        C::Concentric { .. } => "concentric",
        C::Fixed { .. } => "fixed",
    }
}

fn constraint_reference_summary(constraint: &EngineeringConstraint) -> (String, String) {
    use EngineeringConstraint as C;
    match constraint {
        C::Fixed { target, .. } => (target.0.clone(), target.0.clone()),
        C::Symmetric { source, target, .. }
        | C::Aligned { source, target, .. }
        | C::Distance { source, target, .. }
        | C::Tangent { source, target, .. }
        | C::Concentric { source, target, .. } => (source.0.clone(), target.0.clone()),
    }
}

fn drawing_value(property: &crate::feature::DrawingProperty) -> String {
    use crate::feature::DrawingProperty as D;
    match property {
        D::Scale { value } => format!("{}", value),
        D::ProjectionMethod { method } => format!("{:?}", method).to_lowercase(),
        D::ShowCenterlines { enabled } => format!("{}", enabled),
        D::ShowConstructionGeometry { enabled } => format!("{}", enabled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::EngineeringCommand;
    use crate::executor::CommandExecutor;
    use crate::{FeatureId, ParameterId, PartId};

    fn fixture_model() -> EngineeringModel {
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
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("plate_width".into()),
                value: crate::parameter::ParameterValue::Scalar(120.0),
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("plate_height".into()),
                value: crate::parameter::ParameterValue::Scalar(80.0),
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("edge_offset".into()),
                value: crate::parameter::ParameterValue::Unresolved,
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: ParameterId("hole_diameter".into()),
                value: crate::parameter::ParameterValue::Scalar(10.0),
                origin: crate::model::ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::BaseProfile {
                    id: FeatureId("main".into()),
                    name: "main".into(),
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
                feature: crate::feature::EngineeringFeature::HolePattern {
                    id: FeatureId("mounting".into()),
                    pattern: crate::feature::HolePattern::Corner {
                        offset_x: 15.0,
                        offset_y: 15.0,
                        hole: crate::feature::HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("main".into()),
                slot: crate::binding::FeatureParameterSlot::Width,
                parameter: ParameterId("plate_width".into()),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("mounting".into()),
                slot: crate::binding::FeatureParameterSlot::OffsetX,
                parameter: ParameterId("edge_offset".into()),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::BindFeatureParameter {
                feature: FeatureId("mounting".into()),
                slot: crate::binding::FeatureParameterSlot::Diameter,
                parameter: ParameterId("hole_diameter".into()),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: ParameterId("hole_diameter".into()),
                expression: crate::expression::Expression::divide_parameter_by_constant(
                    ParameterId("plate_height".into()),
                    8.0,
                ),
            },
        )
        .unwrap();
        model
    }

    #[test]
    fn workbench_exposes_generated_topology_source_role_and_pending_status() {
        use crate::parametric::{
            EdgeReference, GeneratedFaceSelector, ParametricFeature, SemanticEdgeQualifier,
            SemanticEdgeSelector, SemanticGeneratedFaceQualifier, SemanticGeneratedFaceReference,
        };
        let cap = SemanticGeneratedFaceReference {
            source_feature_id: FeatureId("sweep_body".into()),
            selector: GeneratedFaceSelector::SweepEndCap,
            qualifier: SemanticGeneratedFaceQualifier::default(),
        };
        let side = SemanticGeneratedFaceReference {
            source_feature_id: FeatureId("sweep_body".into()),
            selector: GeneratedFaceSelector::SweepLateral {
                profile_entity_id: crate::SketchEntityId("profile_edge".into()),
            },
            qualifier: SemanticGeneratedFaceQualifier::default(),
        };
        let feature = EngineeringFeature::Parametric {
            id: FeatureId("rim_fillet".into()),
            name: "rim_fillet".into(),
            suppressed: false,
            feature: ParametricFeature::Fillet {
                edge_refs: vec![EdgeReference {
                    source_feature_id: FeatureId("sweep_body".into()),
                    selector: SemanticEdgeSelector::BetweenGeneratedFaces {
                        first: cap,
                        second: side,
                    },
                    qualifier: SemanticEdgeQualifier::default(),
                }],
                radius: 0.5,
            },
        };
        let references = workbench_topology_references(&feature);
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].source_feature_id, "sweep_body");
        assert!(references.iter().any(|item| item.role == "sweep_end_cap"));
        assert!(references
            .iter()
            .any(|item| item.role == "sweep_lateral"
                && item.identity.as_deref() == Some("profile_edge")));
        assert!(references
            .iter()
            .all(|item| item.relation == "intersection_edge" && item.status == "awaiting_rebuild"));
    }

    #[test]
    fn workbench_view_marks_direct_and_derived_and_unresolved() {
        let view = fixture_model().workbench_view();
        let width = view
            .parameters
            .iter()
            .find(|p| p.id == "plate_width")
            .unwrap();
        assert_eq!(width.value, "120");
        assert_eq!(width.origin, "user");
        assert!(width.editable);
        assert!(width.expression.is_none());

        let diameter = view
            .parameters
            .iter()
            .find(|p| p.id == "hole_diameter")
            .unwrap();
        assert_eq!(diameter.origin, "derived");
        assert_eq!(diameter.expression.as_deref(), Some("(plate_height / 8)"));
        assert_eq!(diameter.dependencies, vec!["plate_height"]);
        assert!(!diameter.editable, "derived parameters are read-only");

        let offset = view
            .parameters
            .iter()
            .find(|p| p.id == "edge_offset")
            .unwrap();
        assert_eq!(offset.value, "UNRESOLVED");
        assert!(view.unresolved.contains(&"edge_offset".to_string()));
        assert_eq!(view.summary.unresolved, 1);
        assert_eq!(view.summary.confirmed, 2);
        assert_eq!(view.summary.derived, 1);
    }

    #[test]
    fn workbench_features_expose_slots_and_bindings() {
        let view = fixture_model().workbench_view();
        let main = view.features.iter().find(|f| f.id == "main").unwrap();
        assert_eq!(main.feature_type, "base_profile");
        let width = main.slots.iter().find(|s| s.slot == "width").unwrap();
        assert_eq!(width.source, "parameter:plate_width");
        assert!(width.editable);
        let mounting = view.features.iter().find(|f| f.id == "mounting").unwrap();
        let offset_x = mounting
            .slots
            .iter()
            .find(|s| s.slot == "offset_x")
            .unwrap();
        assert_eq!(offset_x.display_value, "UNRESOLVED");
        assert!(!offset_x.editable);
        let diameter = mounting
            .slots
            .iter()
            .find(|s| s.slot == "diameter")
            .unwrap();
        assert_eq!(diameter.source, "parameter:hole_diameter");
        assert!(
            !diameter.editable,
            "a slot bound to a derived parameter is read-only"
        );
    }

    #[test]
    fn path_and_sweep_slots_are_visible_and_read_only() {
        use crate::parametric::{
            ParametricFeature, PlaneReference, PrincipalPlane, SketchCurveGeometry, SketchEntity,
            SketchEntityId, SketchLoop, SketchProfile, SweepDirection, SweepOperation,
            SweepOrientation,
        };
        use crate::Point2D;
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId("pipe".into()),
                name: "pipe".into(),
            },
        )
        .unwrap();
        let support = PlaneReference::Principal {
            plane: PrincipalPlane::XY,
        };
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::Parametric {
                    id: FeatureId("profile".into()),
                    name: "profile".into(),
                    feature: ParametricFeature::Sketch {
                        support: support.clone(),
                        profile: SketchProfile {
                            outer: SketchLoop::circle(
                                "profile-circle",
                                Point2D::new(0.0, 0.0),
                                2.0,
                            ),
                            inner_loops: Vec::new(),
                        },
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::Parametric {
                    id: FeatureId("path".into()),
                    name: "path".into(),
                    feature: ParametricFeature::Path {
                        support: support.clone(),
                        segments: vec![SketchEntity {
                            id: SketchEntityId("path-line".into()),
                            geometry: SketchCurveGeometry::Line {
                                start: Point2D::new(0.0, 0.0),
                                end: Point2D::new(20.0, 0.0),
                            },
                        }],
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::Parametric {
                    id: FeatureId("sweep".into()),
                    name: "sweep".into(),
                    feature: ParametricFeature::Sweep {
                        profile: FeatureId("profile".into()),
                        path: FeatureId("path".into()),
                        operation: SweepOperation::Add,
                        direction: SweepDirection::Forward,
                        profile_origin: Point2D::new(0.0, 0.0),
                        orientation: SweepOrientation::FixedBinormal,
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();

        let view = model.workbench_view();
        let path = view
            .features
            .iter()
            .find(|feature| feature.id == "path")
            .unwrap();
        assert_eq!(path.feature_type, "path");
        assert_eq!(
            path.slots
                .iter()
                .find(|slot| slot.slot == "path_segment_count")
                .unwrap()
                .display_value,
            "1"
        );
        let sweep = view
            .features
            .iter()
            .find(|feature| feature.id == "sweep")
            .unwrap();
        assert_eq!(sweep.feature_type, "sweep");
        for (slot_name, expected) in [
            ("profile", "profile"),
            ("path", "path"),
            ("operation", "add"),
            ("direction", "forward"),
            ("orientation", "fixed_binormal"),
        ] {
            let slot = sweep
                .slots
                .iter()
                .find(|slot| slot.slot == slot_name)
                .unwrap();
            assert_eq!(slot.display_value, expected);
            assert!(!slot.editable);
        }
    }

    #[test]
    fn parametric_hole_slots_expose_typed_edit_targets_and_mm_units() {
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
                feature: crate::feature::EngineeringFeature::Parametric {
                    id: FeatureId("center_hole".into()),
                    name: "center_hole".into(),
                    feature: crate::parametric::ParametricFeature::Hole {
                        support: crate::parametric::PlaneReference::Principal {
                            plane: crate::parametric::PrincipalPlane::XY,
                        },
                        position: crate::Point2D::new(0.0, 0.0),
                        diameter: 10.0,
                        reversed: true,
                        termination: crate::parametric::HoleTermination::Blind { depth: 6.0 },
                        kind: crate::parametric::HoleKind::Simple,
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();

        let view = model.workbench_view();
        let hole = view
            .features
            .iter()
            .find(|feature| feature.id == "center_hole")
            .unwrap();
        assert_eq!(hole.feature_type, "hole");
        let diameter = hole
            .slots
            .iter()
            .find(|slot| slot.slot == "diameter")
            .unwrap();
        assert_eq!(diameter.display_value, "10");
        assert_eq!(diameter.unit.as_deref(), Some("mm"));
        assert!(diameter.editable);
        let reference = diameter.parameter_reference.as_ref().unwrap();
        assert_eq!(reference.part_id.0, "mounting_plate");
        assert_eq!(reference.feature_id.0, "center_hole");
        assert_eq!(reference.slot, FeatureParameterSlot::Diameter);
        assert_eq!(
            reference.expected_type,
            crate::binding::ExpectedParameterType::LengthMm
        );
        assert!(reference.parameter_id.0.starts_with("fp_"));

        let depth = hole
            .slots
            .iter()
            .find(|slot| slot.slot == "blind_depth")
            .unwrap();
        assert_eq!(depth.display_value, "6");
        assert_eq!(depth.unit.as_deref(), Some("mm"));
        assert!(depth.editable);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(
            json["features"][0]["slots"][0]["parameter_reference"]["expected_type"],
            "length_mm"
        );
    }

    #[test]
    fn workbench_view_is_deterministic_and_serializes() {
        let model = fixture_model();
        let a = model.workbench_view();
        let b = model.workbench_view();
        assert_eq!(a, b);
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("UNRESOLVED"));
        assert!(json.contains("editable"));
        // Not a second model: no mutation API on the view.
        let _ = b.summary;
    }

    #[test]
    fn source_map_inverse_maps_entity_to_feature() {
        use crate::lowering::lower_engineering_model;
        let model = fixture_model();
        // edge_offset is unresolved → lowering fails; use a resolved copy.
        let mut resolved = model;
        CommandExecutor::apply(
            &mut resolved,
            &EngineeringCommand::SetParameter {
                target: ParameterId("edge_offset".into()),
                value: crate::parameter::ParameterValue::Scalar(15.0),
            },
        )
        .unwrap();
        let lowering = lower_engineering_model(&resolved).unwrap();
        let inverse = lowering.source_map.entity_to_feature();
        assert_eq!(
            inverse.get(&scc_core::ids::EntityId("main".into())),
            Some(&FeatureId("main".into()))
        );
        // The corner template maps to the pattern feature.
        assert_eq!(
            inverse.get(&scc_core::ids::EntityId("mounting".into())),
            Some(&FeatureId("mounting".into()))
        );
    }
}
