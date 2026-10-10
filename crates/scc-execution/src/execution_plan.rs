use scc_core::authority::AuthorityLevel;
use scc_core::constraint::{AnchorRole, ConstraintKind};
use scc_core::entity::EntityKind;
use scc_core::ids::ConstraintId;
use scc_core::ids::EntityId;
use scc_core::runtime::ParameterValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::overlay_view::MutationOp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeCommand {
    CreateEntity {
        id: EntityId,
        kind: EntityKind,
    },
    SetParameter {
        entity: EntityId,
        name: String,
        value: ParameterValue,
    },
    AddConstraint {
        id: ConstraintId,
        kind: ConstraintKind,
        source: EntityId,
        target: EntityId,
        authority: AuthorityLevel,
        anchor_role: Option<AnchorRole>,
        parameters: BTreeMap<String, f64>,
    },
    ValidateGraph,
    MarkSnapshot,
}

impl RuntimeCommand {
    /// Returns a short, human-readable description of the command for execution traces.
    pub fn summary(&self) -> String {
        match self {
            RuntimeCommand::CreateEntity { id, kind } => {
                format!("create entity {id} ({kind:?})")
            }
            RuntimeCommand::SetParameter {
                entity,
                name,
                value,
            } => {
                format!("set parameter {name} of {entity} to {value:?}")
            }
            RuntimeCommand::AddConstraint {
                id,
                kind,
                source,
                target,
                authority,
                anchor_role: _,
                parameters: _,
            } => {
                format!(
                    "add constraint {id} ({kind:?}) from {source} to {target} at authority {authority:?}"
                )
            }
            RuntimeCommand::ValidateGraph => "validate graph".to_string(),
            RuntimeCommand::MarkSnapshot => "mark snapshot".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub root: u64,
    pub commands: Vec<RuntimeCommand>,
    pub overlay: Vec<MutationOp>,
}
