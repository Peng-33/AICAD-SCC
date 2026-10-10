use crate::execution_plan::{ExecutionPlan, RuntimeCommand};
use crate::overlay_view::ConstraintIrWithOverlay;
use scc_core::constraint::{ConstraintKind, PARAM_VALUE};
use scc_core::error::SccError;
use scc_core::graph::{DeclarationGraph, EdgeRelation};
use scc_core::ids::EntityId;
use scc_core::runtime::ParameterValue;
use std::collections::{BTreeMap, BTreeSet};

pub struct PlanGenerator;

impl PlanGenerator {
    pub fn generate(
        view: &ConstraintIrWithOverlay<'_>,
        root: u64,
    ) -> Result<ExecutionPlan, SccError> {
        let topo_order = compute_topological_order(&view.base_ir.graph)?;
        let topo_index: BTreeMap<EntityId, usize> = topo_order
            .iter()
            .enumerate()
            .map(|(idx, id)| (id.clone(), idx))
            .collect();

        let mut entities: Vec<_> = view.base_ir.entities.iter().collect();
        entities.sort_by_key(|e| topo_index.get(&e.id).copied().unwrap_or(usize::MAX));

        let mut constraints: Vec<_> = view.base_ir.constraints.iter().collect();
        constraints.sort_by(|a, b| {
            let a_idx = topo_index.get(&a.source).copied().unwrap_or(usize::MAX);
            let b_idx = topo_index.get(&b.source).copied().unwrap_or(usize::MAX);
            a_idx.cmp(&b_idx).then_with(|| a.id.0.cmp(&b.id.0))
        });

        let mut commands = Vec::new();

        for entity in entities {
            commands.push(RuntimeCommand::CreateEntity {
                id: entity.id.clone(),
                kind: entity.kind,
            });
        }

        for constraint in constraints {
            if constraint.kind == ConstraintKind::Distance && constraint.source == constraint.target
            {
                for (name, value) in &constraint.parameters {
                    if name == PARAM_VALUE {
                        continue;
                    }
                    commands.push(RuntimeCommand::SetParameter {
                        entity: constraint.source.clone(),
                        name: name.clone(),
                        value: ParameterValue::Numeric(*value),
                    });
                }
            }

            commands.push(RuntimeCommand::AddConstraint {
                id: constraint.id.clone(),
                kind: constraint.kind.clone(),
                source: constraint.source.clone(),
                target: constraint.target.clone(),
                authority: constraint.context.authority,
                anchor_role: constraint.anchor_role,
                parameters: constraint.parameters.clone(),
            });
        }

        commands.push(RuntimeCommand::ValidateGraph);
        commands.push(RuntimeCommand::MarkSnapshot);

        Ok(ExecutionPlan {
            root,
            commands,
            overlay: view.overlay.to_vec(),
        })
    }
}

/// Computes a deterministic topological order over the declaration graph.
///
/// Raw graph edges point from dependent to dependency (e.g. subject -> reference
/// for Dependency, array -> element for Ownership). This function reverses those
/// edges for Kahn's algorithm so that dependencies are emitted before dependents.
///
/// Tie-breaking between simultaneously available nodes is deterministic: nodes are
/// ordered by their string ID via a `BTreeSet`.
fn compute_topological_order(graph: &DeclarationGraph) -> Result<Vec<EntityId>, SccError> {
    let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for node in &graph.nodes {
        in_degree.entry(node.id.clone()).or_insert(0);
        adjacency.entry(node.id.clone()).or_default();
    }

    for edge in &graph.edges {
        if !matches!(
            edge.relation,
            EdgeRelation::Dependency | EdgeRelation::Ownership
        ) {
            continue;
        }

        // Raw edge: from (dependent) -> to (dependency).
        // Reversed edge: to (dependency) -> from (dependent).
        // Dependency must be processed first.
        let from = &edge.from;
        let to = &edge.to;

        *in_degree.entry(from.clone()).or_insert(0) += 1;
        adjacency.entry(to.clone()).or_default().push(from.clone());
    }

    let mut ready: BTreeSet<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();

    while let Some(id) = ready.pop_first() {
        order.push(EntityId(id.clone()));

        if let Some(neighbors) = adjacency.get(&id) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.insert(neighbor.clone());
                    }
                }
            }
        }
    }

    if order.len() != in_degree.len() {
        return Err(SccError::Structured(Box::new(
            scc_core::error::StructuredError::new(
                "SCC-EXEC-006",
                scc_core::error::ErrorStage::Execution,
                "declaration graph contains a cycle",
            )
            .with_recoverable(true)
            .with_suggestion("remove cyclic dependencies before execution"),
        )));
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_view::MutationOp;
    use scc_core::authority::{AuthorityLevel, ConstraintOrigin};
    use scc_core::constraint::{Constraint, ConstraintKind};
    use scc_core::constraint_context::ConstraintContext;
    use scc_core::constraint_ir::{ConstraintIr, IrConstraintLevels, IrMetadata};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::{DeclarationGraph, EdgeRelation, GraphEdge, GraphNode, NodeKind};
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::provenance::DerivationChain;
    use std::collections::BTreeMap;

    fn make_context(authority: AuthorityLevel) -> ConstraintContext {
        ConstraintContext {
            origin: ConstraintOrigin::UserSpecified,
            authority,
            derivation: DerivationChain,
            version_hint: None,
        }
    }

    fn test_ir() -> ConstraintIr {
        ConstraintIr {
            entities: vec![
                Entity {
                    id: EntityId("plate".into()),
                    kind: EntityKind::Rectangle,
                },
                Entity {
                    id: EntityId("hole".into()),
                    kind: EntityKind::Circle,
                },
            ],
            constraints: vec![Constraint {
                id: ConstraintId("c_1".into()),
                kind: ConstraintKind::CenterOf,
                source: EntityId("hole".into()),
                target: EntityId("plate".into()),
                context: make_context(AuthorityLevel::L1),
                anchor_role: None,
                parameters: BTreeMap::new(),
            }],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        }
    }

    #[test]
    fn plan_generator_ignores_overlay_semantics() {
        let ir = test_ir();
        let empty_patch: Vec<MutationOp> = vec![];
        let overlay_patch = vec![
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "diameter".into(),
                value: 12.0,
            },
            MutationOp::SuppressConstraint {
                target_id: "c_1".into(),
            },
        ];

        let view_empty = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &empty_patch,
        };
        let view_overlay = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &overlay_patch,
        };

        let plan_empty = PlanGenerator::generate(&view_empty, 42).unwrap();
        let plan_overlay = PlanGenerator::generate(&view_overlay, 42).unwrap();

        assert_eq!(plan_empty.commands, plan_overlay.commands);
        assert_eq!(plan_empty.root, plan_overlay.root);

        assert!(plan_empty.overlay.is_empty());
        assert_eq!(plan_overlay.overlay.len(), 2);
    }

    #[test]
    fn plan_generator_produces_create_entity_commands() {
        let ir = test_ir();
        let patch: Vec<MutationOp> = vec![];
        let view = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &patch,
        };

        let plan = PlanGenerator::generate(&view, 100).unwrap();

        let create_count = plan
            .commands
            .iter()
            .filter(|c| matches!(c, RuntimeCommand::CreateEntity { .. }))
            .count();
        assert_eq!(create_count, 2);

        let add_constraint_count = plan
            .commands
            .iter()
            .filter(|c| matches!(c, RuntimeCommand::AddConstraint { .. }))
            .count();
        assert_eq!(add_constraint_count, 1);
    }

    #[test]
    fn plan_generator_copies_overlay_verbatim() {
        let ir = test_ir();
        let patch = vec![MutationOp::ChangeAuthority {
            target_id: "c_1".into(),
            new_authority: AuthorityLevel::L2,
        }];
        let view = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &patch,
        };

        let plan = PlanGenerator::generate(&view, 0).unwrap();

        assert_eq!(plan.overlay.len(), 1);
        assert!(matches!(
            &plan.overlay[0],
            MutationOp::ChangeAuthority { target_id, new_authority: AuthorityLevel::L2 } if target_id == "c_1"
        ));
    }

    #[test]
    fn plan_generator_orders_entities_topologically() {
        // Dependency graph: a -> b -> c (a depends on b, b depends on c).
        // Input order is [a, b, c], but topological order should place
        // dependencies before dependents: [c, b, a].
        let ir = chained_dependency_ir();
        let patch: Vec<MutationOp> = vec![];
        let view = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &patch,
        };

        let plan = PlanGenerator::generate(&view, 0).unwrap();

        let create_order: Vec<String> = plan
            .commands
            .iter()
            .filter_map(|c| match c {
                RuntimeCommand::CreateEntity { id, .. } => Some(id.0.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(
            create_order,
            vec!["c".to_string(), "b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn plan_generator_orders_constraints_topologically() {
        // Dependency graph: a -> b -> c.
        // Constraint c_ab (a centered_in b) must be added AFTER c_bc (b centered_in c)
        // because a's position depends on b, which depends on c.
        let ir = chained_dependency_ir();
        let patch: Vec<MutationOp> = vec![];
        let view = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &patch,
        };

        let plan = PlanGenerator::generate(&view, 0).unwrap();

        let constraint_order: Vec<String> = plan
            .commands
            .iter()
            .filter_map(|c| match c {
                RuntimeCommand::AddConstraint { id, .. } => Some(id.0.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(
            constraint_order,
            vec!["c_bc".to_string(), "c_ab".to_string()]
        );
    }

    #[test]
    fn plan_generator_deterministically_orders_fan_in_dependencies() {
        // Fan-in: both a and b depend on c.
        // c must come first. a and b become available simultaneously; tie-breaking
        // must be deterministic (by entity ID), so order is [c, a, b].
        let ir = fan_in_dependency_ir();
        let patch: Vec<MutationOp> = vec![];
        let view = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &patch,
        };

        let plan = PlanGenerator::generate(&view, 0).unwrap();

        let create_order: Vec<String> = plan
            .commands
            .iter()
            .filter_map(|c| match c {
                RuntimeCommand::CreateEntity { id, .. } => Some(id.0.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(
            create_order,
            vec!["c".to_string(), "a".to_string(), "b".to_string()]
        );

        // Constraints c_ac and c_bc both reference c; their source entities a and b
        // are tied, so constraint order is deterministic by constraint ID:
        // c_ac before c_bc.
        let constraint_order: Vec<String> = plan
            .commands
            .iter()
            .filter_map(|c| match c {
                RuntimeCommand::AddConstraint { id, .. } => Some(id.0.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(
            constraint_order,
            vec!["c_ac".to_string(), "c_bc".to_string()]
        );
    }

    #[test]
    fn topological_sort_helper_chain() {
        let graph = DeclarationGraph {
            nodes: vec![
                GraphNode {
                    id: "a".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "b".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "c".into(),
                    node_kind: NodeKind::Entity,
                },
            ],
            edges: vec![
                GraphEdge {
                    from: "a".into(),
                    to: "b".into(),
                    relation: EdgeRelation::Dependency,
                },
                GraphEdge {
                    from: "b".into(),
                    to: "c".into(),
                    relation: EdgeRelation::Dependency,
                },
            ],
        };

        let order = compute_topological_order(&graph).unwrap();
        let ids: Vec<String> = order.into_iter().map(|id| id.0).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn topological_sort_helper_diamond() {
        // a depends on b and c; b and c both depend on d.
        let graph = DeclarationGraph {
            nodes: vec![
                GraphNode {
                    id: "a".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "b".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "c".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "d".into(),
                    node_kind: NodeKind::Entity,
                },
            ],
            edges: vec![
                GraphEdge {
                    from: "a".into(),
                    to: "b".into(),
                    relation: EdgeRelation::Dependency,
                },
                GraphEdge {
                    from: "a".into(),
                    to: "c".into(),
                    relation: EdgeRelation::Dependency,
                },
                GraphEdge {
                    from: "b".into(),
                    to: "d".into(),
                    relation: EdgeRelation::Dependency,
                },
                GraphEdge {
                    from: "c".into(),
                    to: "d".into(),
                    relation: EdgeRelation::Dependency,
                },
            ],
        };

        let order = compute_topological_order(&graph).unwrap();
        let ids: Vec<String> = order.into_iter().map(|id| id.0).collect();

        // d must be first; a must be last. b and c are tied and ordered by ID.
        assert_eq!(ids[0], "d");
        assert_eq!(ids[3], "a");
        assert!(ids[1] == "b" && ids[2] == "c");
    }

    #[test]
    fn topological_sort_helper_disconnected_nodes() {
        let graph = DeclarationGraph {
            nodes: vec![
                GraphNode {
                    id: "z".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "a".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "m".into(),
                    node_kind: NodeKind::Entity,
                },
            ],
            edges: vec![],
        };

        let order = compute_topological_order(&graph).unwrap();
        let ids: Vec<String> = order.into_iter().map(|id| id.0).collect();
        assert_eq!(ids, vec!["a", "m", "z"]);
    }

    fn chained_dependency_ir() -> ConstraintIr {
        ConstraintIr {
            entities: vec![
                Entity {
                    id: EntityId("a".into()),
                    kind: EntityKind::Circle,
                },
                Entity {
                    id: EntityId("b".into()),
                    kind: EntityKind::Circle,
                },
                Entity {
                    id: EntityId("c".into()),
                    kind: EntityKind::Circle,
                },
            ],
            constraints: vec![
                Constraint {
                    id: ConstraintId("c_ab".into()),
                    kind: ConstraintKind::CenterOf,
                    source: EntityId("a".into()),
                    target: EntityId("b".into()),
                    context: make_context(AuthorityLevel::L1),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
                Constraint {
                    id: ConstraintId("c_bc".into()),
                    kind: ConstraintKind::CenterOf,
                    source: EntityId("b".into()),
                    target: EntityId("c".into()),
                    context: make_context(AuthorityLevel::L1),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
            ],
            graph: DeclarationGraph {
                nodes: vec![
                    GraphNode {
                        id: "a".into(),
                        node_kind: NodeKind::Entity,
                    },
                    GraphNode {
                        id: "b".into(),
                        node_kind: NodeKind::Entity,
                    },
                    GraphNode {
                        id: "c".into(),
                        node_kind: NodeKind::Entity,
                    },
                ],
                edges: vec![
                    GraphEdge {
                        from: "a".into(),
                        to: "b".into(),
                        relation: EdgeRelation::Dependency,
                    },
                    GraphEdge {
                        from: "b".into(),
                        to: "c".into(),
                        relation: EdgeRelation::Dependency,
                    },
                ],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        }
    }

    fn fan_in_dependency_ir() -> ConstraintIr {
        ConstraintIr {
            entities: vec![
                Entity {
                    id: EntityId("a".into()),
                    kind: EntityKind::Circle,
                },
                Entity {
                    id: EntityId("b".into()),
                    kind: EntityKind::Circle,
                },
                Entity {
                    id: EntityId("c".into()),
                    kind: EntityKind::Circle,
                },
            ],
            constraints: vec![
                Constraint {
                    id: ConstraintId("c_ac".into()),
                    kind: ConstraintKind::CenterOf,
                    source: EntityId("a".into()),
                    target: EntityId("c".into()),
                    context: make_context(AuthorityLevel::L1),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
                Constraint {
                    id: ConstraintId("c_bc".into()),
                    kind: ConstraintKind::CenterOf,
                    source: EntityId("b".into()),
                    target: EntityId("c".into()),
                    context: make_context(AuthorityLevel::L1),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
            ],
            graph: DeclarationGraph {
                nodes: vec![
                    GraphNode {
                        id: "a".into(),
                        node_kind: NodeKind::Entity,
                    },
                    GraphNode {
                        id: "b".into(),
                        node_kind: NodeKind::Entity,
                    },
                    GraphNode {
                        id: "c".into(),
                        node_kind: NodeKind::Entity,
                    },
                ],
                edges: vec![
                    GraphEdge {
                        from: "a".into(),
                        to: "c".into(),
                        relation: EdgeRelation::Dependency,
                    },
                    GraphEdge {
                        from: "b".into(),
                        to: "c".into(),
                        relation: EdgeRelation::Dependency,
                    },
                ],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        }
    }
}
