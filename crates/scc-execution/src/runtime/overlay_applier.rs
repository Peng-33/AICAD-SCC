use crate::overlay_view::MutationOp;
use crate::runtime::runtime_error::{OverlayTargetKind, RuntimeError};
use scc_core::authority::AuthorityLevel;
use scc_core::ids::SnapshotId;
use scc_core::runtime::{ConstraintRuntimeState, GraphState, ParameterValue};

pub struct OverlayApplier;

impl OverlayApplier {
    /// Apply a sequence of mutation operations to a GraphState.
    ///
    /// Semantics: all-or-nothing transactional apply.
    /// A candidate state is cloned at the start; all operations run on the
    /// candidate. If every operation succeeds the original state is replaced.
    /// On any failure the candidate is discarded, leaving the original state
    /// unchanged.
    pub fn apply(
        state: &mut GraphState,
        overlay: &[MutationOp],
    ) -> Result<Vec<(SnapshotId, String)>, RuntimeError> {
        let mut candidate = state.clone();
        let mut events = Vec::new();

        for op in overlay {
            match op {
                MutationOp::UpdateParameter {
                    target_id,
                    parameter,
                    value,
                } => {
                    if *value < 0.0 {
                        return Err(RuntimeError::NegativeParameter {
                            parameter: parameter.clone(),
                        });
                    }
                    let eid = scc_core::ids::EntityId(target_id.clone());
                    if let Some(entity) = candidate.entities.get_mut(&eid) {
                        entity
                            .parameters
                            .insert(parameter.clone(), ParameterValue::Numeric(*value));
                        candidate.advance_step();
                    } else {
                        return Err(RuntimeError::OverlayApplyFailed {
                            id: target_id.clone(),
                            kind: OverlayTargetKind::Entity,
                        });
                    }
                }

                MutationOp::SuppressConstraint { target_id } => {
                    let cid = scc_core::ids::ConstraintId(target_id.clone());
                    if let Some(constraint) = candidate.constraints.get_mut(&cid) {
                        if constraint.authority == AuthorityLevel::L0 {
                            return Err(RuntimeError::FrozenConstraint {
                                id: target_id.clone(),
                            });
                        }
                        constraint.state = ConstraintRuntimeState::Suppressed;
                        candidate.advance_step();
                    } else {
                        return Err(RuntimeError::OverlayApplyFailed {
                            id: target_id.clone(),
                            kind: OverlayTargetKind::Constraint,
                        });
                    }
                }

                MutationOp::UnsuppressConstraint { target_id } => {
                    let cid = scc_core::ids::ConstraintId(target_id.clone());
                    if let Some(constraint) = candidate.constraints.get_mut(&cid) {
                        if constraint.authority == AuthorityLevel::L0 {
                            return Err(RuntimeError::FrozenConstraint {
                                id: target_id.clone(),
                            });
                        }
                        constraint.state = ConstraintRuntimeState::Active;
                        candidate.advance_step();
                    } else {
                        return Err(RuntimeError::OverlayApplyFailed {
                            id: target_id.clone(),
                            kind: OverlayTargetKind::Constraint,
                        });
                    }
                }

                MutationOp::ChangeAuthority {
                    target_id,
                    new_authority,
                } => {
                    let cid = scc_core::ids::ConstraintId(target_id.clone());
                    if let Some(constraint) = candidate.constraints.get_mut(&cid) {
                        if constraint.authority == AuthorityLevel::L0 {
                            return Err(RuntimeError::FrozenConstraint {
                                id: target_id.clone(),
                            });
                        }
                        if *new_authority == AuthorityLevel::L0 {
                            return Err(RuntimeError::CannotPromoteToL0 {
                                id: target_id.clone(),
                            });
                        }
                        constraint.authority = *new_authority;
                        candidate.advance_step();
                    } else {
                        return Err(RuntimeError::OverlayApplyFailed {
                            id: target_id.clone(),
                            kind: OverlayTargetKind::Constraint,
                        });
                    }
                }
            }

            events.push((candidate.current_snapshot(), op.summary()));
        }

        *state = candidate;
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::authority::AuthorityLevel;
    use scc_core::constraint::ConstraintKind;
    use scc_core::entity::EntityKind;
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::runtime::{ConstraintRuntimeState, RuntimeConstraint, RuntimeEntity};

    fn setup_state() -> GraphState {
        let mut state = GraphState::new(42);
        state.entities.insert(
            EntityId("hole".into()),
            RuntimeEntity {
                id: EntityId("hole".into()),
                kind: EntityKind::Circle,
                parameters: Default::default(),
            },
        );
        state.constraints.insert(
            ConstraintId("c_1".into()),
            RuntimeConstraint {
                id: ConstraintId("c_1".into()),
                kind: ConstraintKind::CenterOf,
                source: EntityId("hole".into()),
                target: EntityId("plate".into()),
                authority: AuthorityLevel::L1,
                state: ConstraintRuntimeState::Active,
                anchor_role: None,
                parameters: Default::default(),
            },
        );
        state
    }

    #[test]
    fn runtime_applies_update_parameter() {
        let mut state = setup_state();
        let overlay = vec![MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: 12.0,
        }];

        let result = OverlayApplier::apply(&mut state, &overlay);
        assert!(result.is_ok());

        let entity = state.entities.get(&EntityId("hole".into())).unwrap();
        assert_eq!(
            entity.parameters.get("diameter"),
            Some(&ParameterValue::Numeric(12.0))
        );
    }

    #[test]
    fn runtime_rejects_negative_parameter_without_mutating_state() {
        let mut state = setup_state();
        let before = state.clone();
        let overlay = vec![MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: -1.0,
        }];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-012");
        assert_eq!(state, before);
    }

    #[test]
    fn runtime_applies_suppress_constraint() {
        let mut state = setup_state();
        let overlay = vec![MutationOp::SuppressConstraint {
            target_id: "c_1".into(),
        }];

        let result = OverlayApplier::apply(&mut state, &overlay);
        assert!(result.is_ok());

        let constraint = state.constraints.get(&ConstraintId("c_1".into())).unwrap();
        assert_eq!(constraint.state, ConstraintRuntimeState::Suppressed);
    }

    #[test]
    fn runtime_applies_change_authority() {
        let mut state = setup_state();
        let overlay = vec![MutationOp::ChangeAuthority {
            target_id: "c_1".into(),
            new_authority: AuthorityLevel::L2,
        }];

        let result = OverlayApplier::apply(&mut state, &overlay);
        assert!(result.is_ok());

        let constraint = state.constraints.get(&ConstraintId("c_1".into())).unwrap();
        assert_eq!(constraint.authority, AuthorityLevel::L2);
    }

    #[test]
    fn overlay_applier_increments_step() {
        let mut state = setup_state();
        assert_eq!(state.step, 0);

        let overlay = vec![
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "diameter".into(),
                value: 10.0,
            },
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "width".into(),
                value: 20.0,
            },
        ];

        OverlayApplier::apply(&mut state, &overlay).unwrap();
        assert_eq!(state.step, 2);
    }

    #[test]
    fn overlay_applier_fails_on_missing_entity() {
        let mut state = setup_state();
        let overlay = vec![MutationOp::UpdateParameter {
            target_id: "nonexistent".into(),
            parameter: "x".into(),
            value: 1.0,
        }];

        let result = OverlayApplier::apply(&mut state, &overlay);
        assert!(result.is_err());
    }

    fn mark_constraint_l0(state: &mut GraphState) {
        state
            .constraints
            .get_mut(&ConstraintId("c_1".into()))
            .unwrap()
            .authority = AuthorityLevel::L0;
    }

    #[test]
    fn runtime_rejects_suppressing_l0_constraint_without_mutating_state() {
        let mut state = setup_state();
        mark_constraint_l0(&mut state);
        let before = state.clone();
        let overlay = vec![MutationOp::SuppressConstraint {
            target_id: "c_1".into(),
        }];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        assert_eq!(state, before);
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-002");
        assert_eq!(
            structured.constraint_ids,
            Some(vec![ConstraintId("c_1".into())])
        );
    }

    #[test]
    fn runtime_rejects_unsuppressing_l0_constraint_without_mutating_state() {
        let mut state = setup_state();
        mark_constraint_l0(&mut state);
        let before = state.clone();
        let overlay = vec![MutationOp::UnsuppressConstraint {
            target_id: "c_1".into(),
        }];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        assert_eq!(state, before);
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-002");
    }

    #[test]
    fn runtime_rejects_reauthorizing_l0_constraint_without_mutating_state() {
        let mut state = setup_state();
        mark_constraint_l0(&mut state);
        let before = state.clone();
        let overlay = vec![MutationOp::ChangeAuthority {
            target_id: "c_1".into(),
            new_authority: AuthorityLevel::L2,
        }];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        assert_eq!(state, before);
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-002");
    }

    #[test]
    fn runtime_rejects_promoting_constraint_to_l0_without_mutating_state() {
        let mut state = setup_state();
        let before = state.clone();
        let overlay = vec![MutationOp::ChangeAuthority {
            target_id: "c_1".into(),
            new_authority: AuthorityLevel::L0,
        }];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        assert_eq!(state, before);
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-011");
    }

    #[test]
    fn overlay_applier_commits_all_on_full_success() {
        let mut state = setup_state();
        let overlay = vec![
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "diameter".into(),
                value: 12.0,
            },
            MutationOp::SuppressConstraint {
                target_id: "c_1".into(),
            },
            MutationOp::ChangeAuthority {
                target_id: "c_1".into(),
                new_authority: AuthorityLevel::L2,
            },
        ];

        let events = OverlayApplier::apply(&mut state, &overlay).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(state.step, 3);

        let entity = state.entities.get(&EntityId("hole".into())).unwrap();
        assert_eq!(
            entity.parameters.get("diameter"),
            Some(&ParameterValue::Numeric(12.0))
        );

        let constraint = state.constraints.get(&ConstraintId("c_1".into())).unwrap();
        assert_eq!(constraint.state, ConstraintRuntimeState::Suppressed);
        assert_eq!(constraint.authority, AuthorityLevel::L2);
    }

    #[test]
    fn overlay_applier_rolls_back_on_partial_failure() {
        let mut state = setup_state();
        let before = state.clone();
        let overlay = vec![
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "diameter".into(),
                value: 12.0,
            },
            MutationOp::UpdateParameter {
                target_id: "missing".into(),
                parameter: "x".into(),
                value: 1.0,
            },
        ];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-005");
        assert_eq!(state, before);
        assert_eq!(state.step, 0);
    }

    #[test]
    fn overlay_applier_rolls_back_negative_parameter_on_partial_failure() {
        let mut state = setup_state();
        let before = state.clone();
        let overlay = vec![
            MutationOp::SuppressConstraint {
                target_id: "c_1".into(),
            },
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "diameter".into(),
                value: -1.0,
            },
        ];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-012");
        assert_eq!(state, before);

        let constraint = state.constraints.get(&ConstraintId("c_1".into())).unwrap();
        assert_eq!(constraint.state, ConstraintRuntimeState::Active);
    }

    #[test]
    fn overlay_applier_rolls_back_l0_violation_on_partial_failure() {
        let mut state = setup_state();
        let before = state.clone();
        let overlay = vec![
            MutationOp::UpdateParameter {
                target_id: "hole".into(),
                parameter: "diameter".into(),
                value: 12.0,
            },
            MutationOp::ChangeAuthority {
                target_id: "c_1".into(),
                new_authority: AuthorityLevel::L0,
            },
        ];

        let error = OverlayApplier::apply(&mut state, &overlay).unwrap_err();
        let structured: scc_core::error::StructuredError = error.into();
        assert_eq!(structured.code, "SCC-EXEC-011");
        assert_eq!(state, before);
        assert_eq!(state.step, 0);

        let entity = state.entities.get(&EntityId("hole".into())).unwrap();
        assert_eq!(entity.parameters.get("diameter"), None);
    }

    #[test]
    fn overlay_applier_empty_overlay_is_noop() {
        let mut state = setup_state();
        let before = state.clone();
        let overlay = vec![];

        let events = OverlayApplier::apply(&mut state, &overlay).unwrap();
        assert!(events.is_empty());
        assert_eq!(state, before);
    }
}
