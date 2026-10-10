use scc_core::authority::AuthorityLevel;
use scc_core::constraint_ir::ConstraintIr;
use serde::{Deserialize, Serialize};

pub type MutationPatch = Vec<MutationOp>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MutationOp {
    UpdateParameter {
        target_id: String,
        parameter: String,
        value: f64,
    },
    SuppressConstraint {
        target_id: String,
    },
    UnsuppressConstraint {
        target_id: String,
    },
    ChangeAuthority {
        target_id: String,
        new_authority: AuthorityLevel,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ConstraintIrWithOverlay<'a> {
    pub base_ir: &'a ConstraintIr,
    pub overlay: &'a [MutationOp],
}
impl MutationOp {
    /// Returns a short, human-readable description of the overlay mutation.
    pub fn summary(&self) -> String {
        match self {
            MutationOp::UpdateParameter {
                target_id,
                parameter,
                value,
            } => {
                format!("set parameter {parameter} of {target_id} to {value}")
            }
            MutationOp::SuppressConstraint { target_id } => {
                format!("suppress constraint {target_id}")
            }
            MutationOp::UnsuppressConstraint { target_id } => {
                format!("unsuppress constraint {target_id}")
            }
            MutationOp::ChangeAuthority {
                target_id,
                new_authority,
            } => {
                format!("change authority of constraint {target_id} to {new_authority:?}")
            }
        }
    }
}
