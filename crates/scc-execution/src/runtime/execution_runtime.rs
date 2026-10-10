use crate::execution_plan::ExecutionPlan;
use crate::runtime::execution_trace::ExecutionTraceRecorder;
use crate::runtime::graph_state_machine::GraphStateMachine;
use crate::runtime::overlay_applier::OverlayApplier;
use crate::runtime::runtime_error::RuntimeError;
use scc_core::runtime::{ExecutionTrace, GraphState};

pub struct RuntimeOutput {
    pub final_state: GraphState,
    pub trace: ExecutionTrace,
}

pub struct ExecutionRuntime;

impl ExecutionRuntime {
    pub fn run(plan: ExecutionPlan) -> Result<RuntimeOutput, RuntimeError> {
        let mut state = GraphState::new(plan.root);
        let mut trace = ExecutionTrace::new(plan.root);

        GraphStateMachine::apply_commands(&mut state, &plan.commands, &mut trace)?;

        let mut candidate = state.clone();
        let overlay_events = OverlayApplier::apply(&mut candidate, &plan.overlay)?;
        candidate
            .validate_detailed()
            .map_err(RuntimeError::ValidationError)?;
        state = candidate;
        ExecutionTraceRecorder::record_overlay_events(&mut trace, overlay_events);

        Ok(RuntimeOutput {
            final_state: state,
            trace,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::driver;
    use crate::overlay_view::MutationOp;
    use scc_core::authority::{AuthorityLevel, ConstraintOrigin};
    use scc_core::constraint::{Constraint, ConstraintKind};
    use scc_core::constraint_context::ConstraintContext;
    use scc_core::constraint_ir::{ConstraintIr, IrConstraintLevels, IrMetadata};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::provenance::DerivationChain;
    use std::collections::BTreeMap;

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
            constraints: vec![],
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
    fn runtime_trace_determinism() {
        let ir = test_ir();
        let patch = vec![MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: 12.0,
        }];

        let out1 = driver::execute(&ir, &patch).unwrap();
        let out2 = driver::execute(&ir, &patch).unwrap();

        let trace1 = serde_json::to_string(&out1.trace).unwrap();
        let trace2 = serde_json::to_string(&out2.trace).unwrap();
        assert_eq!(trace1, trace2);

        let state1 = serde_json::to_string(&out1.final_state).unwrap();
        let state2 = serde_json::to_string(&out2.final_state).unwrap();
        assert_eq!(state1, state2);
    }

    fn make_context() -> ConstraintContext {
        ConstraintContext {
            origin: ConstraintOrigin::UserSpecified,
            authority: AuthorityLevel::L1,
            derivation: DerivationChain,
            version_hint: None,
        }
    }

    #[test]
    fn normal_entity_with_constraints_validates() {
        let ir = ConstraintIr {
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
                id: ConstraintId("c1".into()),
                kind: ConstraintKind::Distance,
                source: EntityId("plate".into()),
                target: EntityId("hole".into()),
                context: make_context(),
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
        };
        let out = driver::execute(&ir, &vec![]).unwrap();
        assert!(out
            .final_state
            .entities
            .contains_key(&EntityId("plate".into())));
        assert!(out
            .final_state
            .entities
            .contains_key(&EntityId("hole".into())));
        assert_eq!(out.final_state.constraints.len(), 1);
    }

    #[test]
    fn initial_orphan_entity_is_allowed() {
        let ir = ConstraintIr {
            entities: vec![Entity {
                id: EntityId("orphan".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        };
        let out = driver::execute(&ir, &vec![]).unwrap();
        assert!(out
            .final_state
            .entities
            .contains_key(&EntityId("orphan".into())));
        assert_eq!(out.final_state.constraints.len(), 0);
    }

    #[test]
    fn overlay_adds_orphan_entity_not_referenced() {
        let ir = test_ir();
        let patch = vec![MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: 10.0,
        }];
        let out = driver::execute(&ir, &patch).unwrap();
        assert!(out
            .final_state
            .entities
            .contains_key(&EntityId("plate".into())));
        assert!(out
            .final_state
            .entities
            .contains_key(&EntityId("hole".into())));
        let hole = &out.final_state.entities[&EntityId("hole".into())];
        assert_eq!(
            hole.parameters.get("diameter"),
            Some(&scc_core::runtime::ParameterValue::Numeric(10.0))
        );
    }

    #[test]
    fn dangling_reference_is_rejected() {
        let ir = ConstraintIr {
            entities: vec![],
            constraints: vec![Constraint {
                id: ConstraintId("c_bad".into()),
                kind: ConstraintKind::Distance,
                source: EntityId("ghost".into()),
                target: EntityId("phantom".into()),
                context: make_context(),
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
        };
        let result = driver::execute(&ir, &vec![]);
        assert!(result.is_err(), "expected error for dangling reference");
    }

    #[test]
    fn validate_final_external_call_detects_orphans() {
        use scc_core::entity::EntityKind;
        use scc_core::runtime::{GraphState, RuntimeEntity};
        use std::collections::BTreeMap;
        let mut state = GraphState::new(42);
        state.entities.insert(
            EntityId("alone".into()),
            RuntimeEntity {
                id: EntityId("alone".into()),
                kind: EntityKind::Circle,
                parameters: BTreeMap::new(),
            },
        );
        let result = state.validate_final();
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("not referenced by any constraint"));
    }

    #[test]
    fn deterministic_with_orphan_entity() {
        let ir = test_ir();
        let patch = vec![MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: 10.0,
        }];
        let out1 = driver::execute(&ir, &patch).unwrap();
        let out2 = driver::execute(&ir, &patch).unwrap();
        assert_eq!(
            serde_json::to_string(&out1.final_state).unwrap(),
            serde_json::to_string(&out2.final_state).unwrap(),
        );
        assert_eq!(
            serde_json::to_string(&out1.trace).unwrap(),
            serde_json::to_string(&out2.trace).unwrap(),
        );
        assert_eq!(out1.final_state.root, out2.final_state.root);
    }

    #[test]
    fn runtime_deterministic_root() {
        let ir = test_ir();
        let patch = vec![MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: 12.0,
        }];

        let out1 = driver::execute(&ir, &patch).unwrap();
        let out2 = driver::execute(&ir, &patch).unwrap();

        assert_eq!(out1.final_state.root, out2.final_state.root);
    }
}
