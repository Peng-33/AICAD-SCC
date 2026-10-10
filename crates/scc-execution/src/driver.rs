use crate::ir_patcher::IrPatcher;
use crate::mutation::mutation_guard::MutationGuard;
use crate::overlay_canonicalizer::OverlayCanonicalizer;
use crate::overlay_view::MutationPatch;
use crate::plan_generator::PlanGenerator;
use crate::runtime::execution_runtime::ExecutionRuntime;
use crate::RuntimeOutput;
use scc_core::constraint_ir::ConstraintIr;
use scc_core::error::SccError;

pub fn execute(ir: &ConstraintIr, patch: &MutationPatch) -> Result<RuntimeOutput, SccError> {
    for op in patch {
        MutationGuard::validate(op, ir).map_err(SccError::from)?;
    }

    let view = IrPatcher::patch(ir, patch);
    let root = OverlayCanonicalizer::canonicalize(&view)?;
    let plan = PlanGenerator::generate(&view, root)?;
    let output = ExecutionRuntime::run(plan)?;
    Ok(output)
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
    use scc_core::graph::DeclarationGraph;
    use scc_core::graph::{GraphNode, NodeKind};
    use scc_core::ids::ConstraintId;
    use scc_core::ids::EntityId;
    use scc_core::provenance::DerivationChain;
    use std::collections::BTreeMap;

    fn empty_ir() -> ConstraintIr {
        ConstraintIr {
            entities: vec![],
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

    fn make_context() -> ConstraintContext {
        ConstraintContext {
            origin: ConstraintOrigin::UserSpecified,
            authority: AuthorityLevel::L1,
            derivation: DerivationChain,
            version_hint: None,
        }
    }

    fn ir_with_array() -> ConstraintIr {
        ConstraintIr {
            entities: vec![Entity {
                id: EntityId("hole".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![
                Constraint {
                    id: ConstraintId("c_array".into()),
                    kind: ConstraintKind::ArrayRelation,
                    source: EntityId("hole".into()),
                    target: EntityId("hole_array".into()),
                    context: make_context(),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
                Constraint {
                    id: ConstraintId("c_instance".into()),
                    kind: ConstraintKind::InstanceOf,
                    source: EntityId("hole_array".into()),
                    target: EntityId("hole".into()),
                    context: make_context(),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
            ],
            graph: DeclarationGraph {
                nodes: vec![
                    GraphNode {
                        id: "hole".into(),
                        node_kind: NodeKind::Entity,
                    },
                    GraphNode {
                        id: "hole_array".into(),
                        node_kind: NodeKind::Array,
                    },
                ],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        }
    }

    #[test]
    fn execute_with_empty_patch() {
        let ir = empty_ir();
        let patch: MutationPatch = vec![];
        let output = execute(&ir, &patch).unwrap();
        assert_eq!(output.final_state.root, output.trace.root);
    }

    #[test]
    fn execute_with_overlay() {
        let ir = ConstraintIr {
            entities: vec![Entity {
                id: EntityId("circle_1".into()),
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
        let patch = vec![MutationOp::UpdateParameter {
            target_id: "circle_1".into(),
            parameter: "diameter".into(),
            value: 20.0,
        }];
        let output = execute(&ir, &patch).unwrap();

        let entity = output
            .final_state
            .entities
            .get(&EntityId("circle_1".into()))
            .unwrap();
        assert_eq!(
            entity.parameters.get("diameter"),
            Some(&scc_core::runtime::ParameterValue::Numeric(20.0))
        );
    }

    #[test]
    fn ir_patcher_does_not_alter_ir() {
        let ir = empty_ir();
        let patch = vec![MutationOp::UpdateParameter {
            target_id: "x".into(),
            parameter: "y".into(),
            value: 1.0,
        }];
        let original_bytes = serde_json::to_vec(&ir).unwrap();

        let _view = IrPatcher::patch(&ir, &patch);

        let after_bytes = serde_json::to_vec(&ir).unwrap();
        assert_eq!(original_bytes, after_bytes);
    }

    #[test]
    fn graph_state_includes_array_derived_constraints() {
        let ir = ir_with_array();
        let output = execute(&ir, &vec![]).unwrap();

        let array_constraint = output
            .final_state
            .constraints
            .get(&ConstraintId("c_array".into()))
            .unwrap();
        assert_eq!(array_constraint.kind, ConstraintKind::ArrayRelation);
        assert_eq!(array_constraint.source, EntityId("hole".into()));
        assert_eq!(array_constraint.target, EntityId("hole_array".into()));

        let instance_constraint = output
            .final_state
            .constraints
            .get(&ConstraintId("c_instance".into()))
            .unwrap();
        assert_eq!(instance_constraint.kind, ConstraintKind::InstanceOf);
        assert_eq!(instance_constraint.source, EntityId("hole_array".into()));
        assert_eq!(instance_constraint.target, EntityId("hole".into()));

        assert!(output
            .final_state
            .entities
            .contains_key(&EntityId("hole".into())));
    }

    #[test]
    fn execution_is_deterministic_with_array_constraints() {
        let ir = ir_with_array();
        let patch: Vec<MutationOp> = vec![];

        let out1 = execute(&ir, &patch).unwrap();
        let out2 = execute(&ir, &patch).unwrap();

        let state1 = serde_json::to_string(&out1.final_state).unwrap();
        let state2 = serde_json::to_string(&out2.final_state).unwrap();
        assert_eq!(state1, state2);

        let trace1 = serde_json::to_string(&out1.trace).unwrap();
        let trace2 = serde_json::to_string(&out2.trace).unwrap();
        assert_eq!(trace1, trace2);
    }
}
