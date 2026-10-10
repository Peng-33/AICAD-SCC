use crate::mutation::editable_subgraph::EditableSubgraph;
use crate::mutation::mutation_error::{MutationError, MutationTargetKind};
use crate::overlay_view::MutationOp;
use scc_core::authority::AuthorityLevel;
#[cfg(test)]
use scc_core::authority::ConstraintOrigin;
use scc_core::constraint_ir::ConstraintIr;
#[cfg(test)]
use std::collections::BTreeMap;

pub struct MutationGuard;

impl MutationGuard {
    pub fn validate(op: &MutationOp, ir: &ConstraintIr) -> Result<(), MutationError> {
        let subgraph = EditableSubgraph::from_ir(ir);

        match op {
            MutationOp::UpdateParameter {
                target_id, value, ..
            } => {
                if !subgraph.editable_entities.contains(target_id) {
                    return Err(MutationError::TargetNotFound {
                        id: target_id.clone(),
                        kind: MutationTargetKind::Entity,
                    });
                }
                if *value < 0.0 {
                    return Err(MutationError::NegativeValue);
                }
            }

            MutationOp::SuppressConstraint { target_id } => {
                if !subgraph.editable_constraints.contains(target_id) {
                    if !ir.constraints.iter().any(|c| c.id.0 == *target_id) {
                        return Err(MutationError::TargetNotFound {
                            id: target_id.clone(),
                            kind: MutationTargetKind::Constraint,
                        });
                    }
                    return Err(MutationError::FrozenConstraint {
                        id: target_id.clone(),
                    });
                }
            }

            MutationOp::UnsuppressConstraint { target_id } => {
                if !subgraph.editable_constraints.contains(target_id) {
                    if !ir.constraints.iter().any(|c| c.id.0 == *target_id) {
                        return Err(MutationError::TargetNotFound {
                            id: target_id.clone(),
                            kind: MutationTargetKind::Constraint,
                        });
                    }
                    return Err(MutationError::FrozenConstraint {
                        id: target_id.clone(),
                    });
                }
            }

            MutationOp::ChangeAuthority {
                target_id,
                new_authority,
            } => {
                if !subgraph.editable_constraints.contains(target_id) {
                    if !ir.constraints.iter().any(|c| c.id.0 == *target_id) {
                        return Err(MutationError::TargetNotFound {
                            id: target_id.clone(),
                            kind: MutationTargetKind::Constraint,
                        });
                    }
                    return Err(MutationError::FrozenConstraint {
                        id: target_id.clone(),
                    });
                }
                if *new_authority == AuthorityLevel::L0 {
                    return Err(MutationError::CannotPromoteToL0 {
                        id: target_id.clone(),
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::constraint::{Constraint, ConstraintKind};
    use scc_core::constraint_context::ConstraintContext;
    use scc_core::constraint_ir::{ConstraintIr, IrConstraintLevels, IrMetadata};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::provenance::DerivationChain;

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
            entities: vec![Entity {
                id: EntityId("hole".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![
                Constraint {
                    id: ConstraintId("c_1".into()),
                    kind: ConstraintKind::CenterOf,
                    source: EntityId("hole".into()),
                    target: EntityId("plate".into()),
                    context: make_context(AuthorityLevel::L1),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
                Constraint {
                    id: ConstraintId("c_0".into()),
                    kind: ConstraintKind::On,
                    source: EntityId("hole".into()),
                    target: EntityId("plate".into()),
                    context: make_context(AuthorityLevel::L0),
                    anchor_role: None,
                    parameters: BTreeMap::new(),
                },
            ],
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
    fn control_layer_rejects_l0_mutation() {
        let ir = test_ir();
        let op = MutationOp::SuppressConstraint {
            target_id: "c_0".into(),
        };
        let result = MutationGuard::validate(&op, &ir);
        assert!(result.is_err());
    }

    #[test]
    fn control_layer_allows_l1_mutation() {
        let ir = test_ir();
        let op = MutationOp::SuppressConstraint {
            target_id: "c_1".into(),
        };
        let result = MutationGuard::validate(&op, &ir);
        assert!(result.is_ok());
    }

    #[test]
    fn control_layer_rejects_unknown_target() {
        let ir = test_ir();
        let op = MutationOp::UpdateParameter {
            target_id: "nonexistent".into(),
            parameter: "x".into(),
            value: 1.0,
        };
        let result = MutationGuard::validate(&op, &ir);
        assert!(result.is_err());
    }

    #[test]
    fn control_layer_rejects_promote_to_l0() {
        let ir = test_ir();
        let op = MutationOp::ChangeAuthority {
            target_id: "c_1".into(),
            new_authority: AuthorityLevel::L0,
        };
        let result = MutationGuard::validate(&op, &ir);
        assert!(result.is_err());
    }

    #[test]
    fn control_layer_rejects_negative_value() {
        let ir = test_ir();
        let op = MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: -5.0,
        };
        let result = MutationGuard::validate(&op, &ir);
        assert!(result.is_err());
    }
}
