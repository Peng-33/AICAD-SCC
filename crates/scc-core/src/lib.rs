pub mod array;
pub mod authority;
pub mod conflict;
pub mod constraint;
pub mod constraint_context;
pub mod constraint_ir;
pub mod entity;
pub mod error;
pub mod graph;
pub mod ids;
pub mod provenance;
pub mod runtime;
pub mod schema_envelope;
pub mod semantic_ir;

#[cfg(test)]
mod tests {
    use super::*;
    use authority::{AuthorityLevel, ConstraintOrigin};
    use constraint::{Constraint, ConstraintKind, PARAM_VALUE};
    use constraint_context::ConstraintContext;
    use entity::{Entity, EntityKind};
    use graph::{DeclarationGraph, EdgeRelation, GraphEdge, GraphNode, NodeKind};
    use ids::{ArrayId, ConstraintId, EntityId, SnapshotId};
    use provenance::DerivationChain;
    use runtime::{
        ConstraintRuntimeState, ExecutionTrace, GraphState, ParameterValue, RuntimeConstraint,
        RuntimeEntity,
    };
    use std::collections::BTreeMap;

    #[test]
    fn entity_id_equality_and_ordering() {
        let a = EntityId("a".into());
        let b = EntityId("b".into());
        let a2 = EntityId("a".into());
        assert_eq!(a, a2);
        assert_ne!(a, b);
        assert!(a < b);
        assert!(a <= a2);
    }

    #[test]
    fn constraint_id_equality_and_ordering() {
        let x = ConstraintId("c1".into());
        let y = ConstraintId("c2".into());
        let x2 = ConstraintId("c1".into());
        assert_eq!(x, x2);
        assert_ne!(x, y);
        assert!(x < y);
    }

    #[test]
    fn snapshot_id_equality_and_ordering() {
        let s1 = SnapshotId("0:0".into());
        let s2 = SnapshotId("0:1".into());
        let s1_dup = SnapshotId("0:0".into());
        assert_eq!(s1, s1_dup);
        assert!(s1 < s2);
    }

    #[test]
    fn array_id_display() {
        let id = ArrayId("arr_1".into());
        assert_eq!(format!("{}", id.0), "arr_1");
    }

    #[test]
    fn entity_serializes_roundtrip() {
        let e = Entity {
            id: EntityId("p1".into()),
            kind: EntityKind::Point,
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Entity = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn constraint_serializes_with_parameters() {
        let mut params = BTreeMap::new();
        params.insert(PARAM_VALUE.to_string(), 42.0);
        let c = Constraint {
            id: ConstraintId("dist_1".into()),
            kind: ConstraintKind::Distance,
            source: EntityId("a".into()),
            target: EntityId("b".into()),
            context: ConstraintContext {
                origin: ConstraintOrigin::UserSpecified,
                authority: AuthorityLevel::L0,
                derivation: DerivationChain,
                version_hint: None,
            },
            anchor_role: None,
            parameters: params,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
        assert!(json.contains("\"value\""));
    }

    #[test]
    fn entity_kind_copy_and_equality() {
        let k1 = EntityKind::Circle;
        let k2 = EntityKind::Circle;
        let k3 = EntityKind::Line;
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn authority_level_ordering() {
        assert!(AuthorityLevel::L0 < AuthorityLevel::L1);
        assert!(AuthorityLevel::L1 < AuthorityLevel::L2);
        assert!(AuthorityLevel::L2 < AuthorityLevel::L3);
    }

    #[test]
    fn constraint_context_default() {
        let ctx = ConstraintContext {
            origin: ConstraintOrigin::UserSpecified,
            authority: AuthorityLevel::L0,
            derivation: DerivationChain,
            version_hint: None,
        };
        let back: ConstraintContext =
            serde_json::from_str(&serde_json::to_string(&ctx).unwrap()).unwrap();
        assert_eq!(ctx, back);
    }

    #[test]
    fn declaration_graph_stable_ordering() {
        let g = DeclarationGraph {
            nodes: vec![
                GraphNode {
                    id: "b".into(),
                    node_kind: NodeKind::Entity,
                },
                GraphNode {
                    id: "a".into(),
                    node_kind: NodeKind::Entity,
                },
            ],
            edges: vec![
                GraphEdge {
                    from: "b".into(),
                    to: "a".into(),
                    relation: EdgeRelation::Dependency,
                },
                GraphEdge {
                    from: "a".into(),
                    to: "b".into(),
                    relation: EdgeRelation::Dependency,
                },
            ],
        };
        let json = serde_json::to_string_pretty(&g).unwrap();
        let back: DeclarationGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
        assert_eq!(back.nodes[0].id, "b");
        assert_eq!(back.edges[0].from, "b");
    }

    #[test]
    fn edge_relation_copy_and_equality() {
        assert_eq!(EdgeRelation::Ownership, EdgeRelation::Ownership);
        assert_ne!(EdgeRelation::Ownership, EdgeRelation::Dependency);
    }

    #[test]
    fn graph_state_starts_empty() {
        let state = GraphState::new(123);
        assert_eq!(state.root, 123);
        assert_eq!(state.step, 0);
        assert!(state.entities.is_empty());
        assert!(state.constraints.is_empty());
    }

    #[test]
    fn graph_state_snapshot_and_advance() {
        let mut state = GraphState::new(7);
        assert_eq!(state.current_snapshot().0, "7:0");
        state.advance_step();
        assert_eq!(state.step, 1);
        assert_eq!(state.current_snapshot().0, "7:1");
    }

    #[test]
    fn runtime_entity_owns_sorted_parameters() {
        let mut params = BTreeMap::new();
        params.insert("z".to_string(), ParameterValue::Numeric(1.0));
        params.insert("a".to_string(), ParameterValue::Numeric(2.0));
        let re = RuntimeEntity {
            id: EntityId("e1".into()),
            kind: EntityKind::Rectangle,
            parameters: params,
        };
        let keys: Vec<_> = re.parameters.keys().cloned().collect();
        assert_eq!(keys, vec!["a", "z"]);
    }

    #[test]
    fn runtime_constraint_serializes_roundtrip() {
        let rc = RuntimeConstraint {
            id: ConstraintId("c1".into()),
            kind: ConstraintKind::Parallel,
            source: EntityId("l1".into()),
            target: EntityId("l2".into()),
            authority: AuthorityLevel::L1,
            state: ConstraintRuntimeState::Active,
            anchor_role: None,
            parameters: BTreeMap::new(),
        };
        let json = serde_json::to_string(&rc).unwrap();
        let back: RuntimeConstraint = serde_json::from_str(&json).unwrap();
        assert_eq!(rc, back);
    }

    #[test]
    fn graph_state_insertion_is_deterministic() {
        let mut state = GraphState::new(0);
        for id in ["z", "a", "m"] {
            state.entities.insert(
                EntityId(id.into()),
                RuntimeEntity {
                    id: EntityId(id.into()),
                    kind: EntityKind::Point,
                    parameters: BTreeMap::new(),
                },
            );
        }
        let keys: Vec<_> = state.entities.keys().map(|e| e.0.clone()).collect();
        assert_eq!(keys, vec!["a", "m", "z"]);
    }

    #[test]
    fn execution_trace_records_events() {
        let mut trace = ExecutionTrace::new(42);
        assert_eq!(trace.root, 42);
        assert!(trace.events.is_empty());
        trace.record(SnapshotId("42:0".into()), "create entity a".into());
        trace.record(SnapshotId("42:1".into()), "apply constraint c".into());
        assert_eq!(trace.events.len(), 2);
        assert_eq!(trace.events[0].command_summary, "create entity a");
    }

    #[test]
    fn execution_trace_serializes_roundtrip() {
        let mut trace = ExecutionTrace::new(1);
        trace.record(SnapshotId("1:0".into()), "step 0".into());
        let json = serde_json::to_string_pretty(&trace).unwrap();
        let back: ExecutionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(trace, back);
    }

    #[test]
    fn parameter_value_numeric_roundtrip() {
        let p = ParameterValue::Numeric(42.0);
        let json = serde_json::to_string(&p).unwrap();
        let back: ParameterValue = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn serialization_order_is_deterministic() {
        let mut state = GraphState::new(0);
        for (id, kind) in [
            ("c1", EntityKind::Circle),
            ("l1", EntityKind::Line),
            ("p1", EntityKind::Point),
        ] {
            state.entities.insert(
                EntityId(id.into()),
                RuntimeEntity {
                    id: EntityId(id.into()),
                    kind,
                    parameters: BTreeMap::new(),
                },
            );
        }
        state.constraints.insert(
            ConstraintId("c_a".into()),
            RuntimeConstraint {
                id: ConstraintId("c_a".into()),
                kind: ConstraintKind::Distance,
                source: EntityId("p1".into()),
                target: EntityId("c1".into()),
                authority: AuthorityLevel::L0,
                state: ConstraintRuntimeState::Active,
                anchor_role: None,
                parameters: BTreeMap::new(),
            },
        );
        state.constraints.insert(
            ConstraintId("c_b".into()),
            RuntimeConstraint {
                id: ConstraintId("c_b".into()),
                kind: ConstraintKind::Parallel,
                source: EntityId("l1".into()),
                target: EntityId("l1".into()),
                authority: AuthorityLevel::L1,
                state: ConstraintRuntimeState::Suppressed,
                anchor_role: None,
                parameters: BTreeMap::new(),
            },
        );
        let run1 = serde_json::to_string_pretty(&state).unwrap();
        let run2 = serde_json::to_string_pretty(&state).unwrap();
        assert_eq!(run1, run2);
        assert!(run1.find("\"c_a\"").unwrap() < run1.find("\"c_b\"").unwrap());
        assert!(run1.find("\"c1\"").unwrap() < run1.find("\"l1\"").unwrap());
    }

    #[test]
    fn btreemap_insertion_order_does_not_affect_output() {
        let mut state_a = GraphState::new(0);
        let mut state_b = GraphState::new(0);
        for id in ["z", "a"] {
            state_a.entities.insert(
                EntityId(id.into()),
                RuntimeEntity {
                    id: EntityId(id.into()),
                    kind: EntityKind::Point,
                    parameters: BTreeMap::new(),
                },
            );
        }
        for id in ["a", "z"] {
            state_b.entities.insert(
                EntityId(id.into()),
                RuntimeEntity {
                    id: EntityId(id.into()),
                    kind: EntityKind::Point,
                    parameters: BTreeMap::new(),
                },
            );
        }
        assert_eq!(
            serde_json::to_string_pretty(&state_a).unwrap(),
            serde_json::to_string_pretty(&state_b).unwrap()
        );
    }

    #[test]
    fn conflict_serializes_roundtrip() {
        use conflict::{
            Conflict, ConflictCategory, ConflictFacts, ConflictType, MinimalConflictSubgraph,
        };
        let c = Conflict {
            category: ConflictCategory::Structural,
            conflict_type: ConflictType::MissingReference,
            mcs: MinimalConflictSubgraph {
                conflicting_constraints: vec![ConstraintId("c1".into())],
                conflict_type: ConflictType::MissingReference,
            },
            facts: ConflictFacts {
                affected_entities: vec![EntityId("e1".into())],
                affected_constraints: vec![ConstraintId("c1".into())],
                message: "missing ref".into(),
            },
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Conflict = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn scc_error_display() {
        let e = error::SccError::Execution {
            message: "disk full".into(),
        };
        assert!(e.to_string().contains("disk full"));
    }
}
