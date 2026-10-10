//! Converts a [`DslDocument`] AST into `EngineeringCommand` values.
//!
//! The builder is the only bridge between the DSL AST and the engineering
//! layer (ADR-018 Decision 1). It never mutates a model; it only produces
//! commands, which are applied later by `CommandExecutor` (decision 7).
//!
//! References are validated **in declaration order**: a parameter, feature, or
//! base profile must be declared before it is used, which matches how
//! `CommandExecutor::apply_all` executes the commands.

use std::collections::BTreeSet;

use scc_core::authority::AuthorityLevel;
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::command::EngineeringCommand;
use scc_engineering::expression::Expression as EngExpression;
use scc_engineering::feature::{
    DrawingProperty, EngineeringConstraint, EngineeringFeature, HoleDefinition, HolePattern,
    ProjectionMethod,
};
use scc_engineering::model::ParameterOrigin;
use scc_engineering::parameter::ParameterValue;
use scc_engineering::{EngineeringConstraintId, FeatureId, ParameterId, PartId, Point2D};
use scc_engineering::{HoleExtent, HoleStartFace, SourcedLength};

use crate::ast::*;
use crate::error::DslError;
use crate::token::SourceSpan;

/// Build `EngineeringCommand` values from a parsed DSL document.
pub fn build_commands(document: &DslDocument) -> Result<Vec<EngineeringCommand>, DslError> {
    let mut builder = CommandBuilder::default();
    let mut commands = Vec::new();
    for statement in &document.statements {
        commands.extend(builder.build_statement(statement)?);
    }
    Ok(commands)
}

/// Parse and build in one step: `source → Vec<EngineeringCommand>`.
pub fn compile(source: &str) -> Result<Vec<EngineeringCommand>, DslError> {
    build_commands(&crate::parser::parse_document(source)?)
}

/// An error while applying a compiled document to a model.
#[derive(Debug)]
pub enum ApplyError {
    /// DSL lexing/parsing/building failure.
    Dsl(DslError),
    /// A command failed to apply (the whole document is rejected atomically).
    Engineering(scc_engineering::error::EngineeringError),
}

/// Compile `source` and apply the whole document to `model` atomically
/// (working-copy + commit via [`scc_engineering::executor::CommandExecutor`]).
pub fn apply_document(
    model: &mut scc_engineering::model::EngineeringModel,
    source: &str,
) -> Result<Vec<EngineeringCommand>, ApplyError> {
    let commands = compile(source).map_err(ApplyError::Dsl)?;
    scc_engineering::executor::CommandExecutor::apply_all(model, &commands)
        .map_err(ApplyError::Engineering)?;
    Ok(commands)
}

struct CommandBuilder {
    declared_parameters: BTreeSet<String>,
    declared_features: BTreeSet<String>,
    declared_bases: BTreeSet<String>,
    current_span: SourceSpan,
}

impl Default for CommandBuilder {
    fn default() -> Self {
        Self {
            declared_parameters: BTreeSet::new(),
            declared_features: BTreeSet::new(),
            declared_bases: BTreeSet::new(),
            current_span: SourceSpan::new(
                crate::token::Position::new(0, 0),
                crate::token::Position::new(0, 0),
            ),
        }
    }
}

impl CommandBuilder {
    fn build_statement(
        &mut self,
        statement: &DslStatement,
    ) -> Result<Vec<EngineeringCommand>, DslError> {
        self.current_span = statement.span;
        match &statement.kind {
            DslStatementKind::Part(part) => {
                let id = part.id.clone();
                self.declared_features.insert(id.clone());
                Ok(vec![EngineeringCommand::CreatePart {
                    id: PartId(id.clone()),
                    name: part.display_name.clone().unwrap_or_else(|| id.clone()),
                }])
            }
            DslStatementKind::Thickness(thickness) => {
                Ok(vec![EngineeringCommand::SetPartThickness {
                    source: Some(SourcedLength::Literal {
                        value: thickness.value,
                    }),
                }])
            }
            DslStatementKind::HoleExtent(extent) => self.build_hole_extent(extent),
            DslStatementKind::Parameter(parameter) => self.build_parameter(parameter),
            DslStatementKind::BaseRect(base) => {
                let id = base.id.clone();
                self.declared_features.insert(id.clone());
                self.declared_bases.insert(id.clone());
                Ok(vec![EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::BaseProfile {
                        id: FeatureId(id.clone()),
                        name: id.clone(),
                        profile: scc_engineering::BaseProfile::Rectangle {
                            width: base.width,
                            height: base.height,
                        },
                    },
                }])
            }
            DslStatementKind::BaseCircle(base) => {
                let id = base.id.clone();
                self.declared_features.insert(id.clone());
                self.declared_bases.insert(id.clone());
                let mut commands = Vec::new();
                self.declare_center(&mut commands, &id, base.center.0, base.center.1);
                commands.push(EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::BaseProfile {
                        id: FeatureId(id.clone()),
                        name: id.clone(),
                        profile: scc_engineering::BaseProfile::Circle {
                            center: Point2D::new(base.center.0, base.center.1),
                            diameter: base.diameter,
                        },
                    },
                });
                self.bind_center(&mut commands, &id);
                Ok(commands)
            }
            DslStatementKind::BaseRoundedRect(base) => {
                let id = base.id.clone();
                self.declared_features.insert(id.clone());
                self.declared_bases.insert(id.clone());
                let mut commands = Vec::new();
                self.declare_center(&mut commands, &id, base.center.0, base.center.1);
                commands.push(EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::BaseProfile {
                        id: FeatureId(id.clone()),
                        name: id.clone(),
                        profile: scc_engineering::BaseProfile::RoundedRectangle {
                            center: Point2D::new(base.center.0, base.center.1),
                            width: base.width,
                            height: base.height,
                            corner_radius: base.radius,
                        },
                    },
                });
                self.bind_center(&mut commands, &id);
                Ok(commands)
            }
            DslStatementKind::BasePolygon(base) => {
                let id = base.id.clone();
                self.declared_features.insert(id.clone());
                self.declared_bases.insert(id.clone());
                let vertices = base
                    .vertices
                    .iter()
                    .map(|(x, y)| Point2D::new(*x, *y))
                    .collect::<Vec<_>>();
                let vertices =
                    scc_engineering::profile::canonicalize_polygon(&vertices).map_err(|error| {
                        DslError::CommandBuildFailed {
                            message: error.to_string(),
                        }
                    })?;
                Ok(vec![EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::BaseProfile {
                        id: FeatureId(id.clone()),
                        name: id,
                        profile: scc_engineering::BaseProfile::Polygon { vertices },
                    },
                }])
            }
            DslStatementKind::Hole(hole) => self.build_hole(hole),
            DslStatementKind::Slot(slot) => self.build_slot(slot),
            DslStatementKind::HolePattern(pattern) => self.build_hole_pattern(pattern),
            DslStatementKind::ConstructionCircle(cc) => self.build_construction_circle(cc),
            DslStatementKind::Binding(binding) => self.build_binding(binding),
            DslStatementKind::Constraint(constraint) => self.build_constraint(constraint),
            DslStatementKind::Drawing(drawing) => self.build_drawing(drawing),
        }
    }

    fn build_parameter(
        &mut self,
        parameter: &DslParameter,
    ) -> Result<Vec<EngineeringCommand>, DslError> {
        let (value, origin) = match &parameter.value {
            DslParameterValue::Direct(value) => {
                (ParameterValue::Scalar(*value), ParameterOrigin::User)
            }
            DslParameterValue::Unresolved => (ParameterValue::Unresolved, ParameterOrigin::User),
            DslParameterValue::Expression(expression) => {
                for reference in expression.referenced_parameters() {
                    if !self.declared_parameters.contains(&reference) {
                        return Err(self.unknown_parameter(&reference));
                    }
                }
                (
                    ParameterValue::Expression(convert_expression(expression)?),
                    ParameterOrigin::Derived,
                )
            }
        };
        self.declared_parameters.insert(parameter.id.clone());
        Ok(vec![EngineeringCommand::DeclareParameter {
            id: ParameterId(parameter.id.clone()),
            value,
            origin,
            authority: AuthorityLevel::L0,
        }])
    }

    fn build_slot(&mut self, slot: &DslSlotFeature) -> Result<Vec<EngineeringCommand>, DslError> {
        let id = slot.id.clone();
        self.declared_features.insert(id.clone());
        Ok(vec![EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Slot {
                id: FeatureId(id),
                center: Point2D::new(slot.center.0, slot.center.1),
                length: slot.length,
                width: slot.width,
                orientation_degrees: slot.angle,
            },
        }])
    }

    fn build_hole(&mut self, hole: &DslHole) -> Result<Vec<EngineeringCommand>, DslError> {
        let id = hole.id.clone();
        self.declared_features.insert(id.clone());
        let mut commands = Vec::new();
        // Phase 1 `Hole` has no position field: `CENTER` desugars to implicit
        // center parameters bound to the `CenterX`/`CenterY` slots.
        if let Some((x, y)) = hole.center {
            self.declare_center(&mut commands, &id, x, y);
        }
        commands.push(EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId(id.clone()),
                diameter: hole.diameter,
            },
        });
        if hole.center.is_some() {
            self.bind_center(&mut commands, &id);
        }
        Ok(commands)
    }

    fn build_construction_circle(
        &mut self,
        cc: &DslConstructionCircle,
    ) -> Result<Vec<EngineeringCommand>, DslError> {
        let id = cc.id.clone();
        self.declared_features.insert(id.clone());
        let mut commands = Vec::new();
        if let Some((x, y)) = cc.center {
            self.declare_center(&mut commands, &id, x, y);
        }
        commands.push(EngineeringCommand::AddFeature {
            feature: EngineeringFeature::ConstructionCircle {
                id: FeatureId(id.clone()),
                diameter: cc.diameter,
            },
        });
        if cc.center.is_some() {
            self.bind_center(&mut commands, &id);
        }
        Ok(commands)
    }

    fn build_hole_pattern(
        &mut self,
        pattern: &DslHolePattern,
    ) -> Result<Vec<EngineeringCommand>, DslError> {
        let id = pattern.id.clone();
        self.declared_features.insert(id.clone());
        let fields = &pattern.fields;
        let mut commands = Vec::new();

        let hole = HoleDefinition {
            diameter: fields.diameter.ok_or_else(|| self.missing("DIA", &id))?,
        };
        match pattern.kind {
            DslPatternKind::Circular => {
                // `CENTER` is the feature's native center field.
                let center = fields.center.unwrap_or((0.0, 0.0));
                let pcd = fields.pcd.ok_or_else(|| self.missing("PCD", &id))?;
                let count =
                    positive_count(fields.count.ok_or_else(|| self.missing("COUNT", &id))?)?;
                let start_angle = fields.start_angle.unwrap_or(0.0);
                commands.push(EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::HolePattern {
                        id: FeatureId(id.clone()),
                        pattern: HolePattern::Circular {
                            center: Point2D::new(center.0, center.1),
                            pitch_circle_diameter: pcd,
                            count,
                            start_angle,
                            hole,
                        },
                    },
                });
            }
            DslPatternKind::Grid => {
                let mut bind_center_later = false;
                if let Some((x, y)) = fields.center {
                    self.declare_center(&mut commands, &id, x, y);
                    bind_center_later = true;
                }
                let rows = positive_count(fields.rows.ok_or_else(|| self.missing("ROWS", &id))?)?;
                let columns =
                    positive_count(fields.columns.ok_or_else(|| self.missing("COLUMNS", &id))?)?;
                let spacing_x = fields
                    .spacing_x
                    .ok_or_else(|| self.missing("SPACING_X", &id))?;
                let spacing_y = fields
                    .spacing_y
                    .ok_or_else(|| self.missing("SPACING_Y", &id))?;
                commands.push(EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::HolePattern {
                        id: FeatureId(id.clone()),
                        pattern: HolePattern::Grid {
                            rows,
                            columns,
                            spacing_x,
                            spacing_y,
                            hole,
                        },
                    },
                });
                if bind_center_later {
                    self.bind_center(&mut commands, &id);
                }
            }
            DslPatternKind::Corner => {
                if fields.center.is_some() {
                    return Err(self.invalid_field(
                        "HOLE_PATTERN CORNER",
                        "CENTER",
                        "corner patterns are positioned by offsets, not a center",
                    ));
                }
                if let Some(base) = &fields.base {
                    if !self.declared_bases.contains(base) {
                        return Err(self.invalid_binding(format!(
                            "base profile `{}` must be declared before the corner pattern",
                            base
                        )));
                    }
                }
                let offset_x = fields
                    .offset_x
                    .ok_or_else(|| self.missing("OFFSET_X", &id))?;
                let offset_y = fields
                    .offset_y
                    .ok_or_else(|| self.missing("OFFSET_Y", &id))?;
                commands.push(EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::HolePattern {
                        id: FeatureId(id.clone()),
                        pattern: HolePattern::Corner {
                            offset_x,
                            offset_y,
                            hole,
                        },
                    },
                });
            }
        }
        Ok(commands)
    }

    fn build_hole_extent(
        &mut self,
        declaration: &DslHoleExtent,
    ) -> Result<Vec<EngineeringCommand>, DslError> {
        if !self.declared_features.contains(&declaration.target) {
            return Err(self.invalid_binding(format!(
                "hole feature {} must be declared before its extent",
                declaration.target
            )));
        }
        let extent = match declaration.extent {
            DslHoleExtentKind::ThroughAll => HoleExtent::ThroughAll,
            DslHoleExtentKind::BlindBase(depth) => HoleExtent::Blind {
                start_face: HoleStartFace::Base,
                depth: SourcedLength::Literal { value: depth },
            },
            DslHoleExtentKind::BlindOpposite(depth) => HoleExtent::Blind {
                start_face: HoleStartFace::Opposite,
                depth: SourcedLength::Literal { value: depth },
            },
        };
        Ok(vec![EngineeringCommand::SetHoleExtent {
            target: FeatureId(declaration.target.clone()),
            extent: Some(extent),
        }])
    }
    fn build_binding(&mut self, binding: &DslBinding) -> Result<Vec<EngineeringCommand>, DslError> {
        if !self.declared_features.contains(&binding.feature) {
            return Err(self.invalid_binding(format!(
                "feature `{}` must be declared before it is bound",
                binding.feature
            )));
        }
        if !self.declared_parameters.contains(&binding.parameter) {
            return Err(self.unknown_parameter(&binding.parameter));
        }
        Ok(vec![EngineeringCommand::BindFeatureParameter {
            feature: FeatureId(binding.feature.clone()),
            slot: binding.slot.to_engineering(),
            parameter: ParameterId(binding.parameter.clone()),
        }])
    }

    fn build_constraint(
        &mut self,
        constraint: &DslConstraint,
    ) -> Result<Vec<EngineeringCommand>, DslError> {
        for feature in [&constraint.subject, &constraint.reference] {
            if !self.declared_features.contains(feature) {
                return Err(self.invalid_binding(format!(
                    "feature `{}` must be declared before it is constrained",
                    feature
                )));
            }
        }
        let id = EngineeringConstraintId(constraint.id.clone());
        let engineering = match constraint.kind {
            DslConstraintKind::Fixed => EngineeringConstraint::Fixed {
                id,
                target: FeatureId(constraint.subject.clone()),
            },
            DslConstraintKind::Symmetric => EngineeringConstraint::Symmetric {
                id,
                source: FeatureId(constraint.subject.clone()),
                target: FeatureId(constraint.reference.clone()),
            },
            DslConstraintKind::Aligned => EngineeringConstraint::Aligned {
                id,
                source: FeatureId(constraint.subject.clone()),
                target: FeatureId(constraint.reference.clone()),
            },
            DslConstraintKind::Tangent => EngineeringConstraint::Tangent {
                id,
                source: FeatureId(constraint.subject.clone()),
                target: FeatureId(constraint.reference.clone()),
            },
            DslConstraintKind::Distance => EngineeringConstraint::Distance {
                id,
                source: FeatureId(constraint.subject.clone()),
                target: FeatureId(constraint.reference.clone()),
                value: constraint.value.ok_or_else(|| {
                    self.invalid_binding("DISTANCE requires a numeric value".into())
                })?,
            },
        };
        Ok(vec![EngineeringCommand::AddConstraint {
            constraint: engineering,
        }])
    }

    fn build_drawing(&mut self, drawing: &DslDrawing) -> Result<Vec<EngineeringCommand>, DslError> {
        let property = match drawing {
            DslDrawing::Scale(value) => DrawingProperty::Scale { value: *value },
            DslDrawing::ShowCenterlines(enabled) => {
                DrawingProperty::ShowCenterlines { enabled: *enabled }
            }
            DslDrawing::ShowConstructionGeometry(enabled) => {
                DrawingProperty::ShowConstructionGeometry { enabled: *enabled }
            }
            DslDrawing::ProjectionMethod(method) => DrawingProperty::ProjectionMethod {
                method: match method {
                    DslProjectionMethod::FirstAngle => ProjectionMethod::FirstAngle,
                    DslProjectionMethod::ThirdAngle => ProjectionMethod::ThirdAngle,
                },
            },
        };
        Ok(vec![EngineeringCommand::SetDrawingProperty { property }])
    }

    // -- CENTER desugaring helpers --

    fn declare_center(&mut self, commands: &mut Vec<EngineeringCommand>, id: &str, x: f64, y: f64) {
        let cx = format!("{}_center_x", id);
        let cy = format!("{}_center_y", id);
        self.declared_parameters.insert(cx.clone());
        self.declared_parameters.insert(cy.clone());
        commands.push(EngineeringCommand::DeclareParameter {
            id: ParameterId(cx),
            value: ParameterValue::Scalar(x),
            origin: ParameterOrigin::User,
            authority: AuthorityLevel::L0,
        });
        commands.push(EngineeringCommand::DeclareParameter {
            id: ParameterId(cy),
            value: ParameterValue::Scalar(y),
            origin: ParameterOrigin::User,
            authority: AuthorityLevel::L0,
        });
    }

    fn bind_center(&self, commands: &mut Vec<EngineeringCommand>, id: &str) {
        commands.push(EngineeringCommand::BindFeatureParameter {
            feature: FeatureId(id.into()),
            slot: FeatureParameterSlot::CenterX,
            parameter: ParameterId(format!("{}_center_x", id)),
        });
        commands.push(EngineeringCommand::BindFeatureParameter {
            feature: FeatureId(id.into()),
            slot: FeatureParameterSlot::CenterY,
            parameter: ParameterId(format!("{}_center_y", id)),
        });
    }

    // -- error helpers --

    fn unknown_parameter(&self, parameter: &str) -> DslError {
        DslError::UnknownParameterReference {
            parameter: parameter.into(),
            line: self.current_span.start.line,
            column: self.current_span.start.column,
        }
    }

    fn invalid_binding(&self, message: String) -> DslError {
        DslError::InvalidBinding {
            message,
            line: self.current_span.start.line,
            column: self.current_span.start.column,
        }
    }

    fn missing(&self, field: &str, id: &str) -> DslError {
        DslError::MissingRequiredField {
            feature: id.into(),
            missing: field.into(),
            line: self.current_span.start.line,
            column: self.current_span.start.column,
        }
    }

    fn invalid_field(&self, feature: &str, field: &str, message: &str) -> DslError {
        DslError::InvalidFeatureField {
            feature: feature.into(),
            field: field.into(),
            message: message.into(),
            line: self.current_span.start.line,
            column: self.current_span.start.column,
        }
    }
}

fn positive_count(value: f64) -> Result<u32, DslError> {
    if value.is_finite() && value >= 1.0 && value.fract() == 0.0 && value <= f64::from(u32::MAX) {
        Ok(value as u32)
    } else {
        Err(DslError::CommandBuildFailed {
            message: format!("count must be a positive integer, got {}", value),
        })
    }
}

/// Convert a DSL expression AST to the Phase 2 engineering `Expression` AST.
///
/// Exposed for the Phase 5 tool layer (`set_expression` reuses the DSL
/// expression parser); never eval's text.
pub fn expression_to_engineering(expression: &DslExpression) -> Result<EngExpression, DslError> {
    convert_expression(expression)
}

fn convert_expression(expression: &DslExpression) -> Result<EngExpression, DslError> {
    Ok(match expression {
        DslExpression::Number(value) => EngExpression::Constant { value: *value },
        DslExpression::Reference(id) => EngExpression::Reference {
            parameter: ParameterId(id.clone()),
        },
        DslExpression::Binary { left, op, right } => {
            let left = Box::new(convert_expression(left)?);
            let right = Box::new(convert_expression(right)?);
            match op {
                DslBinaryOp::Add => EngExpression::Add { left, right },
                DslBinaryOp::Subtract => EngExpression::Subtract { left, right },
                DslBinaryOp::Multiply => EngExpression::Multiply { left, right },
                DslBinaryOp::Divide => EngExpression::Divide { left, right },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(source: &str) -> Result<Vec<EngineeringCommand>, DslError> {
        compile(source)
    }

    #[test]
    fn slot_builds_typed_engineering_feature() {
        let commands =
            build("AICAD 1\nPART p\nSLOT s1\n  CENTER 20 10\n  LENGTH 40\n  WIDTH 12\n  ANGLE 0\n")
                .unwrap();
        assert!(commands.iter().any(|command| matches!(
            command,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Slot {
                    length,
                    width,
                    orientation_degrees,
                    center,
                    ..
                }
            } if *length == 40.0
                && *width == 12.0
                && *orientation_degrees == 0.0
                && center.x == 20.0
                && center.y == 10.0
        )));
        assert!(!commands.iter().any(|command| matches!(
            command,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile { .. }
            }
        )));
    }

    #[test]
    fn center_on_hole_desugars_to_implicit_center_binding() {
        let commands = build("AICAD 1\nPART p\nHOLE h\n  CENTER 40 30\n  DIA 10\n").unwrap();
        assert!(commands.iter().any(|c| matches!(
            c,
            EngineeringCommand::DeclareParameter { id, value, .. }
                if id.0 == "h_center_x" && *value == ParameterValue::Scalar(40.0)
        )));
        assert!(commands.iter().any(|c| matches!(
            c,
            EngineeringCommand::BindFeatureParameter { feature, slot, parameter }
                if feature.0 == "h" && *slot == FeatureParameterSlot::CenterX && parameter.0 == "h_center_x"
        )));
        // The hole feature itself is still ONE Hole.
        assert!(commands.iter().any(|c| matches!(
            c,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole { .. }
            }
        )));
    }

    #[test]
    fn unknown_parameter_reference_in_expression_is_rejected() {
        // §58: missing_width is never declared.
        let err = build("AICAD 1\nPARAM hole_diameter := missing_width / 8\n").unwrap_err();
        assert!(matches!(
            err,
            DslError::UnknownParameterReference { parameter, .. } if parameter == "missing_width"
        ));
    }

    #[test]
    fn unknown_parameter_reference_in_bind_is_rejected() {
        let err = build(
            "AICAD 1\nPART p\nBASE_RECT main\n  WIDTH 10\n  HEIGHT 10\nBIND main WIDTH <- ghost\n",
        )
        .unwrap_err();
        assert!(matches!(err, DslError::UnknownParameterReference { .. }));
    }

    #[test]
    fn bind_to_undeclared_feature_is_rejected() {
        let err = build("AICAD 1\nPART p\nBIND ghost WIDTH <- x\n").unwrap_err();
        assert!(matches!(err, DslError::InvalidBinding { .. }));
    }

    #[test]
    fn corner_base_must_be_declared_first() {
        let err = build(
            "AICAD 1\nPART p\nHOLE_PATTERN mounting CORNER\n  BASE main\n  OFFSET_X 15\n  OFFSET_Y 15\n  DIA 10\n",
        )
        .unwrap_err();
        assert!(matches!(err, DslError::InvalidBinding { .. }));
    }

    #[test]
    fn invalid_slot_binding_flows_to_engineering_error_on_apply() {
        // §59: a Hole has no PCD slot; the strong-typed check runs in the
        // engineering layer (SCC-ENG-028), not in the DSL.
        let source = "AICAD 1\nPART p\nPARAM x = 1\nHOLE h\n  DIA 10\nBIND h PCD <- x\n";
        let mut model = scc_engineering::model::EngineeringModel::new();
        let result = apply_document(&mut model, source);
        assert!(matches!(
            result,
            Err(ApplyError::Engineering(
                scc_engineering::error::EngineeringError::InvalidFeatureParameterBinding { .. }
            ))
        ));
        // The whole document is rejected atomically: no part, no feature.
        assert!(model.part.is_none());
        assert!(model.features.is_empty());
    }

    #[test]
    fn whole_document_applies_atomically() {
        // §44: a failing command leaves the model completely unchanged.
        let source =
            "AICAD 1\nPART p\nBASE_RECT main\n  WIDTH 10\n  HEIGHT 10\nBIND main WIDTH <- ghost\n";
        let mut model = scc_engineering::model::EngineeringModel::new();
        let result = apply_document(&mut model, source);
        assert!(result.is_err());
        assert!(model.part.is_none(), "part must not be committed");
    }

    #[test]
    fn derived_parameter_produces_expression_command() {
        let commands =
            build("AICAD 1\nPARAM plate_width = 120\nPARAM x := plate_width / 8\n").unwrap();
        let derived = commands
            .iter()
            .find(|c| matches!(c, EngineeringCommand::DeclareParameter { id, .. } if id.0 == "x"))
            .unwrap();
        match derived {
            EngineeringCommand::DeclareParameter {
                value: ParameterValue::Expression(_),
                origin: ParameterOrigin::Derived,
                ..
            } => {}
            other => panic!("expected derived expression parameter, got {:?}", other),
        }
    }
}
