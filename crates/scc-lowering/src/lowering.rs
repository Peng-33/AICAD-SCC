use std::collections::{BTreeMap, BTreeSet};

use crate::error::{validate_finite_parameters, LoweringError};
use scc_core::array::PatternNode;
use scc_core::authority::{AuthorityLevel, ConstraintOrigin};
use scc_core::constraint::{
    AnchorRole, Constraint, ConstraintKind, PARAM_ARRAY_CIRCULAR, PARAM_ARRAY_COLS,
    PARAM_ARRAY_COL_SPACING, PARAM_ARRAY_COUNT, PARAM_ARRAY_DIRECTION_X, PARAM_ARRAY_DIRECTION_Y,
    PARAM_ARRAY_GRID, PARAM_ARRAY_LINEAR, PARAM_ARRAY_ROWS, PARAM_ARRAY_ROW_SPACING,
    PARAM_ARRAY_SPACING, PARAM_AXIS_X, PARAM_AXIS_Y, PARAM_DIAMETER, PARAM_END_ANGLE, PARAM_HEIGHT,
    PARAM_LENGTH, PARAM_OFFSET, PARAM_RADIUS, PARAM_START_ANGLE, PARAM_VALUE, PARAM_WIDTH, PARAM_X,
    PARAM_X1, PARAM_X2, PARAM_Y, PARAM_Y1, PARAM_Y2,
};
use scc_core::constraint_context::ConstraintContext;
use scc_core::constraint_ir::{ConstraintIr, IrConstraintLevels, IrMetadata};
use scc_core::entity::{Entity, EntityKind};
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::provenance::DerivationChain;
use scc_core::semantic_ir::{SemanticAnchor, SemanticDimension, SemanticIr, SemanticRelation};

fn make_context(origin: ConstraintOrigin, authority: AuthorityLevel) -> ConstraintContext {
    ConstraintContext {
        origin,
        authority,
        derivation: DerivationChain,
        version_hint: None,
    }
}

fn lower_array_parameters(pattern: &PatternNode) -> BTreeMap<String, f64> {
    let mut parameters = BTreeMap::new();
    match pattern {
        PatternNode::Linear {
            count,
            spacing,
            direction,
        } => {
            parameters.insert(PARAM_ARRAY_LINEAR.into(), 1.0);
            parameters.insert(PARAM_ARRAY_COUNT.into(), f64::from(*count));
            parameters.insert(PARAM_ARRAY_SPACING.into(), *spacing);
            let direction_key = match direction.as_deref() {
                None | Some("x") => PARAM_ARRAY_DIRECTION_X,
                Some("y") => PARAM_ARRAY_DIRECTION_Y,
                Some(_) => return parameters,
            };
            parameters.insert(direction_key.into(), 1.0);
        }
        PatternNode::Circular { count, radius, .. } => {
            parameters.insert(PARAM_ARRAY_CIRCULAR.into(), 1.0);
            parameters.insert(PARAM_ARRAY_COUNT.into(), f64::from(*count));
            parameters.insert(PARAM_RADIUS.into(), *radius);
        }
        PatternNode::Grid {
            rows,
            cols,
            row_spacing,
            col_spacing,
        } => {
            parameters.insert(PARAM_ARRAY_GRID.into(), 1.0);
            parameters.insert(PARAM_ARRAY_ROWS.into(), f64::from(*rows));
            parameters.insert(PARAM_ARRAY_COLS.into(), f64::from(*cols));
            parameters.insert(PARAM_ARRAY_ROW_SPACING.into(), *row_spacing);
            parameters.insert(PARAM_ARRAY_COL_SPACING.into(), *col_spacing);
        }
    }
    parameters
}

fn lower_relation(rel: &SemanticRelation, counter: &mut usize) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    *counter += 1;

    match rel {
        SemanticRelation::CenteredIn {
            subject,
            reference,
            axis,
        } => {
            let mut parameters = BTreeMap::new();
            match axis.as_deref() {
                None | Some("both") => {
                    parameters.insert(PARAM_AXIS_X.into(), 1.0);
                    parameters.insert(PARAM_AXIS_Y.into(), 1.0);
                }
                Some("x") => {
                    parameters.insert(PARAM_AXIS_X.into(), 1.0);
                }
                Some("y") => {
                    parameters.insert(PARAM_AXIS_Y.into(), 1.0);
                }
                Some(other) => {
                    parameters.insert(format!("unsupported_axis:{}", other), 1.0);
                }
            }
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::CenterOf,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters,
            });
        }
        SemanticRelation::Inside { subject, reference } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::On,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::Outside { subject, reference } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::On,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L2),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::ParallelTo { subject, reference } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Parallel,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::PerpendicularTo { subject, reference } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Perpendicular,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::TangentTo { subject, reference } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Tangent,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::SymmetricAbout {
            subject, reference, ..
        } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Symmetric,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::AlignedWith { subject, reference } => {
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Align,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            });
        }
        SemanticRelation::DistanceFrom {
            subject,
            reference,
            value,
        } => {
            let mut parameters = BTreeMap::new();
            parameters.insert(PARAM_VALUE.into(), *value);
            constraints.push(Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Distance,
                source: subject.clone(),
                target: reference.clone(),
                context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L1),
                anchor_role: None,
                parameters,
            });
        }
    }

    constraints
}

fn lower_dimension(
    dim: &SemanticDimension,
    counter: &mut usize,
    entity_kind: Option<EntityKind>,
    axis_occurrences: &mut BTreeMap<(EntityId, char), usize>,
    angle_occurrences: &mut BTreeMap<EntityId, usize>,
) -> Vec<Constraint> {
    *counter += 1;
    let (target, value, parameter_name) = match dim {
        SemanticDimension::Diameter { target, value } => (target, *value, PARAM_DIAMETER),
        SemanticDimension::Radius { target, value } => (target, *value, PARAM_RADIUS),
        SemanticDimension::Width { target, value } => (target, *value, PARAM_WIDTH),
        SemanticDimension::Height { target, value } => (target, *value, PARAM_HEIGHT),
        SemanticDimension::Length { target, value } => (target, *value, PARAM_LENGTH),
        SemanticDimension::Offset { target, value } => {
            if entity_kind == Some(EntityKind::Arc) {
                let occurrence = angle_occurrences.entry(target.clone()).or_default();
                let parameter = match *occurrence {
                    0 => PARAM_START_ANGLE,
                    1 => PARAM_END_ANGLE,
                    _ => PARAM_OFFSET,
                };
                *occurrence += 1;
                (target, *value, parameter)
            } else {
                (target, *value, PARAM_OFFSET)
            }
        }
        SemanticDimension::X { target, value } => {
            let occurrence = axis_occurrences.entry((target.clone(), 'x')).or_default();
            let parameter = match entity_kind {
                Some(EntityKind::Line) => match *occurrence {
                    0 => PARAM_X1,
                    1 => PARAM_X2,
                    _ => PARAM_X,
                },
                Some(EntityKind::Arc) => match *occurrence {
                    0 => PARAM_X,
                    1 => PARAM_X1,
                    2 => PARAM_X2,
                    _ => PARAM_X,
                },
                _ => PARAM_X,
            };
            *occurrence += 1;
            (target, *value, parameter)
        }
        SemanticDimension::Y { target, value } => {
            let occurrence = axis_occurrences.entry((target.clone(), 'y')).or_default();
            let parameter = match entity_kind {
                Some(EntityKind::Line) => match *occurrence {
                    0 => PARAM_Y1,
                    1 => PARAM_Y2,
                    _ => PARAM_Y,
                },
                Some(EntityKind::Arc) => match *occurrence {
                    0 => PARAM_Y,
                    1 => PARAM_Y1,
                    2 => PARAM_Y2,
                    _ => PARAM_Y,
                },
                _ => PARAM_Y,
            };
            *occurrence += 1;
            (target, *value, parameter)
        }
    };

    let mut parameters = BTreeMap::new();
    parameters.insert(PARAM_VALUE.into(), value);
    parameters.insert(parameter_name.into(), value);

    vec![Constraint {
        id: ConstraintId(format!("c_{}", counter)),
        kind: ConstraintKind::Distance,
        source: target.clone(),
        target: target.clone(),
        context: make_context(ConstraintOrigin::UserSpecified, AuthorityLevel::L1),
        anchor_role: None,
        parameters,
    }]
}

fn lower_anchor(anchor: &SemanticAnchor, counter: &mut usize) -> (Vec<Entity>, Vec<Constraint>) {
    *counter += 1;

    let mut entities = Vec::new();
    let mut constraints = Vec::new();

    let constraint = match anchor {
        SemanticAnchor::OriginPoint { id } => {
            entities.push(Entity {
                id: id.clone(),
                kind: EntityKind::Point,
            });
            Constraint {
                id: ConstraintId(format!("c_{}", counter)),
                kind: ConstraintKind::Fixed,
                source: id.clone(),
                target: id.clone(),
                context: make_context(ConstraintOrigin::UserSpecified, AuthorityLevel::L0),
                anchor_role: Some(AnchorRole::OriginPoint),
                parameters: BTreeMap::new(),
            }
        }
        SemanticAnchor::FixedCenter { entity, .. } => Constraint {
            id: ConstraintId(format!("c_{}", counter)),
            kind: ConstraintKind::Fixed,
            source: entity.clone(),
            target: entity.clone(),
            context: make_context(ConstraintOrigin::UserSpecified, AuthorityLevel::L0),
            anchor_role: Some(AnchorRole::FixedCenter),
            parameters: BTreeMap::new(),
        },
        SemanticAnchor::ReferenceEdge { entity, .. } => Constraint {
            id: ConstraintId(format!("c_{}", counter)),
            kind: ConstraintKind::Datum,
            source: entity.clone(),
            target: entity.clone(),
            context: make_context(ConstraintOrigin::UserSpecified, AuthorityLevel::L0),
            anchor_role: Some(AnchorRole::ReferenceEdge),
            parameters: BTreeMap::new(),
        },
        SemanticAnchor::DatumLine { entity, .. } => Constraint {
            id: ConstraintId(format!("c_{}", counter)),
            kind: ConstraintKind::Datum,
            source: entity.clone(),
            target: entity.clone(),
            context: make_context(ConstraintOrigin::UserSpecified, AuthorityLevel::L0),
            anchor_role: Some(AnchorRole::DatumLine),
            parameters: BTreeMap::new(),
        },
    };

    constraints.push(constraint);
    (entities, constraints)
}

pub fn lower(ir: &SemanticIr) -> ConstraintIr {
    let mut entities = ir.entities.clone();
    let mut constraints = Vec::new();
    let mut counter = 0usize;
    let entity_kinds: BTreeMap<EntityId, EntityKind> = ir
        .entities
        .iter()
        .map(|entity| (entity.id.clone(), entity.kind))
        .collect();
    let mut axis_occurrences = BTreeMap::new();
    let mut angle_occurrences = BTreeMap::new();

    for rel in &ir.relations {
        constraints.extend(lower_relation(rel, &mut counter));
    }

    for dim in &ir.dimensions {
        let target = match dim {
            SemanticDimension::Diameter { target, .. }
            | SemanticDimension::Radius { target, .. }
            | SemanticDimension::Width { target, .. }
            | SemanticDimension::Height { target, .. }
            | SemanticDimension::Length { target, .. }
            | SemanticDimension::Offset { target, .. }
            | SemanticDimension::X { target, .. }
            | SemanticDimension::Y { target, .. } => target,
        };
        constraints.extend(lower_dimension(
            dim,
            &mut counter,
            entity_kinds.get(target).copied(),
            &mut axis_occurrences,
            &mut angle_occurrences,
        ));
    }

    for array in &ir.arrays {
        counter += 1;
        constraints.push(Constraint {
            id: ConstraintId(format!("c_{}", counter)),
            kind: ConstraintKind::ArrayRelation,
            source: array.element.clone(),
            target: EntityId(array.id.0.clone()),
            context: make_context(ConstraintOrigin::DerivedFromIntent, AuthorityLevel::L2),
            anchor_role: None,
            parameters: lower_array_parameters(&array.pattern),
        });

        counter += 1;
        constraints.push(Constraint {
            id: ConstraintId(format!("c_{}", counter)),
            kind: ConstraintKind::InstanceOf,
            source: EntityId(array.id.0.clone()),
            target: array.element.clone(),
            context: make_context(ConstraintOrigin::CompilerGenerated, AuthorityLevel::L3),
            anchor_role: None,
            parameters: BTreeMap::new(),
        });
    }

    for anchor in &ir.anchors {
        let (anchor_entities, anchor_constraints) = lower_anchor(anchor, &mut counter);
        entities.extend(anchor_entities);
        constraints.extend(anchor_constraints);
    }

    let mut metadata = IrMetadata {
        constraint_levels: IrConstraintLevels::default(),
    };

    for c in &constraints {
        match c.context.authority {
            AuthorityLevel::L0 => metadata.constraint_levels.l0.push(c.id.clone()),
            AuthorityLevel::L1 => metadata.constraint_levels.l1.push(c.id.clone()),
            AuthorityLevel::L2 => metadata.constraint_levels.l2.push(c.id.clone()),
            AuthorityLevel::L3 => metadata.constraint_levels.l3.push(c.id.clone()),
        }
    }

    ConstraintIr {
        entities,
        constraints,
        graph: ir.graph.clone(),
        metadata,
    }
}

/// Lower a [`SemanticIr`] to a [`ConstraintIr`], preserving structured failures.
///
/// The legacy [`lower`] function remains infallible and unchanged; `try_lower`
/// adds validation that the lowered IR is well-formed, deterministic, and
/// obeys the authority-level invariants required by the execution layer.
pub fn try_lower(ir: &SemanticIr) -> Result<ConstraintIr, LoweringError> {
    let constraint_ir = lower(ir);
    validate_entity_kinds(&constraint_ir)?;
    validate_constraint_references(&constraint_ir)?;
    validate_constraint_id_uniqueness(&constraint_ir)?;
    validate_authority_assignments(&constraint_ir)?;
    validate_finite_parameters(&constraint_ir)?;
    Ok(constraint_ir)
}

fn validate_entity_kinds(ir: &ConstraintIr) -> Result<(), LoweringError> {
    for entity in &ir.entities {
        if !is_supported_entity_kind(entity.kind) {
            return Err(LoweringError::UnsupportedEntityKind {
                entity_id: entity.id.clone(),
                kind: format!("{:?}", entity.kind).to_lowercase(),
            });
        }
    }
    Ok(())
}

fn is_supported_entity_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Point
            | EntityKind::Line
            | EntityKind::Circle
            | EntityKind::Arc
            | EntityKind::Rectangle
            | EntityKind::Polygon
    )
}

fn validate_constraint_references(ir: &ConstraintIr) -> Result<(), LoweringError> {
    let entity_ids: BTreeSet<&EntityId> = ir.entities.iter().map(|e| &e.id).collect();
    for constraint in &ir.constraints {
        // ArrayRelation and InstanceOf reference array pseudo-entities that are
        // not required to appear in the entity list.
        if matches!(
            constraint.kind,
            ConstraintKind::ArrayRelation | ConstraintKind::InstanceOf
        ) {
            continue;
        }
        if !entity_ids.contains(&constraint.source) {
            return Err(LoweringError::InvalidEntityReference {
                entity_id: constraint.source.clone(),
            });
        }
        if !entity_ids.contains(&constraint.target) {
            return Err(LoweringError::InvalidEntityReference {
                entity_id: constraint.target.clone(),
            });
        }
    }
    Ok(())
}

fn validate_constraint_id_uniqueness(ir: &ConstraintIr) -> Result<(), LoweringError> {
    let mut seen = BTreeSet::new();
    for constraint in &ir.constraints {
        if !seen.insert(constraint.id.clone()) {
            return Err(LoweringError::ConstraintIdCollision {
                constraint_id: constraint.id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_authority_assignments(ir: &ConstraintIr) -> Result<(), LoweringError> {
    for constraint in &ir.constraints {
        let expected = match constraint.kind {
            ConstraintKind::Fixed | ConstraintKind::Datum => AuthorityLevel::L0,
            ConstraintKind::ArrayRelation => AuthorityLevel::L2,
            ConstraintKind::InstanceOf => AuthorityLevel::L3,
            ConstraintKind::On => {
                // `On` is used for both `Inside` (L1) and `Outside` (L2) relations.
                if constraint.context.authority == AuthorityLevel::L2 {
                    continue;
                }
                AuthorityLevel::L1
            }
            _ => AuthorityLevel::L1,
        };
        if constraint.context.authority != expected {
            return Err(LoweringError::InvalidAuthorityAssignment {
                constraint_id: constraint.id.clone(),
                authority: constraint.context.authority,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::array::{ArrayNode, PatternNode};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;
    use scc_core::ids::{ArrayId, EntityId};

    fn make_ir() -> SemanticIr {
        SemanticIr {
            entities: vec![
                Entity {
                    id: EntityId("rect_1".into()),
                    kind: EntityKind::Rectangle,
                },
                Entity {
                    id: EntityId("circle_1".into()),
                    kind: EntityKind::Circle,
                },
            ],
            relations: vec![SemanticRelation::CenteredIn {
                subject: EntityId("circle_1".into()),
                reference: EntityId("rect_1".into()),
                axis: Some("both".into()),
            }],
            dimensions: vec![SemanticDimension::Diameter {
                target: EntityId("circle_1".into()),
                value: 20.0,
            }],
            arrays: vec![ArrayNode {
                id: ArrayId("hole_array".into()),
                pattern: PatternNode::Linear {
                    count: 4,
                    spacing: 20.0,
                    direction: None,
                },
                element: EntityId("circle_1".into()),
            }],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        }
    }

    #[test]
    fn lowers_relations_to_constraints() {
        let ir = make_ir();
        let result = lower(&ir);
        assert!(result
            .constraints
            .iter()
            .any(|c| c.kind == ConstraintKind::CenterOf));
    }

    #[test]
    fn lowers_dimensions_to_distance_constraints() {
        let ir = make_ir();
        let result = lower(&ir);
        let distance_count = result
            .constraints
            .iter()
            .filter(|c| c.kind == ConstraintKind::Distance)
            .count();
        assert_eq!(distance_count, 1);
    }

    #[test]
    fn lowers_arrays_to_array_relation_and_instance_of() {
        let ir = make_ir();
        let result = lower(&ir);
        assert!(result
            .constraints
            .iter()
            .any(|c| c.kind == ConstraintKind::ArrayRelation));
        assert!(result
            .constraints
            .iter()
            .any(|c| c.kind == ConstraintKind::InstanceOf));

        let array = result
            .constraints
            .iter()
            .find(|constraint| constraint.kind == ConstraintKind::ArrayRelation)
            .unwrap();
        assert_eq!(array.parameters.get(PARAM_ARRAY_LINEAR), Some(&1.0));
        assert_eq!(array.parameters.get(PARAM_ARRAY_COUNT), Some(&4.0));
        assert_eq!(array.parameters.get(PARAM_ARRAY_SPACING), Some(&20.0));
        assert_eq!(array.parameters.get(PARAM_ARRAY_DIRECTION_X), Some(&1.0));
    }

    #[test]
    fn populates_constraint_levels() {
        let ir = make_ir();
        let result = lower(&ir);
        assert!(!result.metadata.constraint_levels.l1.is_empty());
        assert!(!result.metadata.constraint_levels.l2.is_empty());
        assert!(!result.metadata.constraint_levels.l3.is_empty());
    }

    #[test]
    fn constraint_contexts_have_correct_origin() {
        let ir = make_ir();
        let result = lower(&ir);
        let center = result
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::CenterOf)
            .unwrap();
        assert_eq!(center.context.origin, ConstraintOrigin::DerivedFromIntent);
        assert_eq!(center.context.authority, AuthorityLevel::L1);

        let array_rel = result
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::InstanceOf)
            .unwrap();
        assert_eq!(
            array_rel.context.origin,
            ConstraintOrigin::CompilerGenerated
        );
        assert_eq!(array_rel.context.authority, AuthorityLevel::L3);
    }

    // ── P0-1 regression: distance value preserved to ConstraintIR ─

    #[test]
    fn distance_value_survives_through_to_constraint_parameters() {
        let ir = SemanticIr {
            entities: vec![
                Entity {
                    id: EntityId("a".into()),
                    kind: EntityKind::Circle,
                },
                Entity {
                    id: EntityId("b".into()),
                    kind: EntityKind::Circle,
                },
            ],
            relations: vec![SemanticRelation::DistanceFrom {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
                value: 123.456,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };
        let result = lower(&ir);
        let dist_constraint = result
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::Distance)
            .expect("distance constraint must exist");
        assert_eq!(
            dist_constraint.parameters.get(PARAM_VALUE),
            Some(&123.456),
            "distance value must be preserved in constraint parameters"
        );
    }

    #[test]
    fn distance_zero_value_is_preserved_explicitly() {
        let ir = SemanticIr {
            entities: vec![
                Entity {
                    id: EntityId("x".into()),
                    kind: EntityKind::Point,
                },
                Entity {
                    id: EntityId("y".into()),
                    kind: EntityKind::Point,
                },
            ],
            relations: vec![SemanticRelation::DistanceFrom {
                subject: EntityId("x".into()),
                reference: EntityId("y".into()),
                value: 0.0,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };
        let result = lower(&ir);
        let dist = result
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::Distance)
            .unwrap();
        assert_eq!(dist.parameters.get(PARAM_VALUE), Some(&0.0));
    }

    // ── P0 regression: dimension values preserved through lowering ─

    fn make_ir_with_dimension(dim: SemanticDimension) -> SemanticIr {
        SemanticIr {
            entities: vec![Entity {
                id: EntityId("target".into()),
                kind: EntityKind::Circle,
            }],
            relations: vec![],
            dimensions: vec![dim],
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        }
    }

    fn assert_dimension_value_preserved(dim: SemanticDimension, expected: f64) {
        let ir = make_ir_with_dimension(dim);
        let result = lower(&ir);
        let constraint = result
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::Distance)
            .expect("dimension must lower to a Distance constraint");
        assert_eq!(
            constraint.parameters.get(PARAM_VALUE),
            Some(&expected),
            "dimension value {} must be preserved in parameters[\"{}\"]",
            expected,
            PARAM_VALUE
        );
    }

    #[test]
    fn diameter_value_is_preserved() {
        assert_dimension_value_preserved(
            SemanticDimension::Diameter {
                target: EntityId("target".into()),
                value: 20.0,
            },
            20.0,
        );
    }

    #[test]
    fn radius_value_is_preserved() {
        assert_dimension_value_preserved(
            SemanticDimension::Radius {
                target: EntityId("target".into()),
                value: 15.5,
            },
            15.5,
        );
    }

    #[test]
    fn width_value_is_preserved() {
        assert_dimension_value_preserved(
            SemanticDimension::Width {
                target: EntityId("target".into()),
                value: 100.0,
            },
            100.0,
        );
    }

    #[test]
    fn height_value_is_preserved() {
        assert_dimension_value_preserved(
            SemanticDimension::Height {
                target: EntityId("target".into()),
                value: 50.0,
            },
            50.0,
        );
    }

    #[test]
    fn length_value_is_preserved() {
        assert_dimension_value_preserved(
            SemanticDimension::Length {
                target: EntityId("target".into()),
                value: 200.0,
            },
            200.0,
        );
    }

    #[test]
    fn offset_value_is_preserved() {
        assert_dimension_value_preserved(
            SemanticDimension::Offset {
                target: EntityId("target".into()),
                value: 5.0,
            },
            5.0,
        );
    }

    // ── Anchor lowering: RED-phase tests demonstrating semantic loss ─

    #[test]
    fn lowers_origin_point_anchor_to_point_entity_and_fixed_constraint() {
        let ir = SemanticIr {
            entities: vec![],
            relations: vec![],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![scc_core::semantic_ir::SemanticAnchor::OriginPoint {
                id: EntityId("origin".into()),
            }],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let result = lower(&ir);

        assert!(
            result
                .entities
                .iter()
                .any(|e| e.id.0 == "origin" && e.kind == EntityKind::Point),
            "origin_point anchor must create a Point entity"
        );

        let constraints_json = serde_json::to_string(&result.constraints).unwrap();
        assert!(
            constraints_json.contains("Fixed"),
            "origin_point anchor must lower to a Fixed constraint, got: {}",
            constraints_json
        );
    }

    #[test]
    fn lowers_fixed_center_anchor_to_fixed_constraint() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("circle_1".into()),
                kind: EntityKind::Circle,
            }],
            relations: vec![],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![scc_core::semantic_ir::SemanticAnchor::FixedCenter {
                id: EntityId("anchor_1".into()),
                entity: EntityId("circle_1".into()),
            }],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let result = lower(&ir);
        let constraints_json = serde_json::to_string(&result.constraints).unwrap();
        assert!(
            constraints_json.contains("Fixed"),
            "fixed_center anchor must lower to a Fixed constraint, got: {}",
            constraints_json
        );
    }

    #[test]
    fn lowers_reference_edge_anchor_to_datum_constraint() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("line_1".into()),
                kind: EntityKind::Line,
            }],
            relations: vec![],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![scc_core::semantic_ir::SemanticAnchor::ReferenceEdge {
                id: EntityId("anchor_1".into()),
                entity: EntityId("line_1".into()),
            }],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let result = lower(&ir);
        let constraints_json = serde_json::to_string(&result.constraints).unwrap();
        assert!(
            constraints_json.contains("Datum"),
            "reference_edge anchor must lower to a Datum constraint, got: {}",
            constraints_json
        );
    }

    #[test]
    fn lowers_datum_line_anchor_to_datum_constraint() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("line_1".into()),
                kind: EntityKind::Line,
            }],
            relations: vec![],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![scc_core::semantic_ir::SemanticAnchor::DatumLine {
                id: EntityId("anchor_1".into()),
                entity: EntityId("line_1".into()),
            }],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let result = lower(&ir);
        let constraints_json = serde_json::to_string(&result.constraints).unwrap();
        assert!(
            constraints_json.contains("Datum"),
            "datum_line anchor must lower to a Datum constraint, got: {}",
            constraints_json
        );
    }

    #[test]
    fn anchor_constraints_are_pinned_at_l0() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("circle_1".into()),
                kind: EntityKind::Circle,
            }],
            relations: vec![],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![scc_core::semantic_ir::SemanticAnchor::FixedCenter {
                id: EntityId("anchor_1".into()),
                entity: EntityId("circle_1".into()),
            }],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let result = lower(&ir);
        assert!(
            !result.metadata.constraint_levels.l0.is_empty(),
            "anchor constraints must be pinned at L0"
        );
    }

    #[test]
    fn try_lower_rejects_non_finite_dimension_value() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("target".into()),
                kind: EntityKind::Circle,
            }],
            relations: vec![],
            dimensions: vec![SemanticDimension::Diameter {
                target: EntityId("target".into()),
                value: f64::NAN,
            }],
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let err = try_lower(&ir).unwrap_err();
        assert_eq!(err.code(), "SCC-LOW-006");
        let structured: scc_core::error::StructuredError = err.into();
        structured.validate_contract().unwrap();
    }

    #[test]
    fn try_lower_rejects_invalid_entity_reference() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("a".into()),
                kind: EntityKind::Circle,
            }],
            relations: vec![SemanticRelation::DistanceFrom {
                subject: EntityId("a".into()),
                reference: EntityId("missing".into()),
                value: 10.0,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };

        let err = try_lower(&ir).unwrap_err();
        assert_eq!(err.code(), "SCC-LOW-004");
        let structured: scc_core::error::StructuredError = err.into();
        structured.validate_contract().unwrap();
    }

    #[test]
    fn try_lower_rejects_invalid_authority_assignment() {
        let ir = make_ir();

        let mut bad = lower(&ir);
        // Mutate an array relation authority to violate the L2 invariant.
        if let Some(c) = bad
            .constraints
            .iter_mut()
            .find(|c| c.kind == ConstraintKind::ArrayRelation)
        {
            c.context.authority = AuthorityLevel::L1;
        }

        // Manually invoke authority validation on the mutated IR.
        let result = validate_authority_assignments(&bad);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "SCC-LOW-007");
        let structured: scc_core::error::StructuredError = err.into();
        structured.validate_contract().unwrap();
    }

    #[test]
    fn x_y_position_dimensions_lower_to_entity_coordinate_parameters() {
        let ir = SemanticIr {
            entities: vec![Entity {
                id: EntityId("hole".into()),
                kind: EntityKind::Circle,
            }],
            relations: vec![],
            dimensions: vec![
                SemanticDimension::X {
                    target: EntityId("hole".into()),
                    value: -45.0,
                },
                SemanticDimension::Y {
                    target: EntityId("hole".into()),
                    value: -25.0,
                },
            ],
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        };
        let result = lower(&ir);
        let find = |parameter: &str| {
            result.constraints.iter().find(|c| {
                c.kind == ConstraintKind::Distance
                    && c.source == EntityId("hole".into())
                    && c.target == EntityId("hole".into())
                    && c.parameters.contains_key(parameter)
            })
        };
        let x = find(PARAM_X).expect("x dimension must lower to an x parameter");
        assert_eq!(x.parameters.get(PARAM_X), Some(&-45.0));
        assert_eq!(x.parameters.get(PARAM_VALUE), Some(&-45.0));
        let y = find(PARAM_Y).expect("y dimension must lower to a y parameter");
        assert_eq!(y.parameters.get(PARAM_Y), Some(&-25.0));
    }

    #[test]
    fn try_lower_accepts_well_formed_ir() {
        let ir = make_ir();
        let result = try_lower(&ir);
        assert!(result.is_ok());
        let constraint_ir = result.unwrap();
        assert!(!constraint_ir.constraints.is_empty());
    }

    #[test]
    fn lowering_is_deterministic_over_many_runs() {
        use sha2::{Digest, Sha256};

        let ir = make_ir();
        let first = lower(&ir);
        for _ in 0..100 {
            let candidate = lower(&ir);
            assert_eq!(
                candidate.constraints, first.constraints,
                "constraint order must be deterministic"
            );
            assert_eq!(
                candidate.metadata, first.metadata,
                "metadata must be deterministic"
            );
        }

        let canonical = serde_json::to_string(&first).unwrap();
        let hash = Sha256::digest(canonical.as_bytes());
        let mut prev = hash;
        for _ in 0..10 {
            let canonical = serde_json::to_string(&lower(&ir)).unwrap();
            let hash = Sha256::digest(canonical.as_bytes());
            assert_eq!(hash, prev, "canonical JSON SHA-256 must be stable");
            prev = hash;
        }
    }
}
