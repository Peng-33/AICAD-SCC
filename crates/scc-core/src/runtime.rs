use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::authority::AuthorityLevel;
use crate::constraint::{AnchorRole, ConstraintKind};
use crate::entity::EntityKind;
use crate::error::{ErrorStage, StructuredError};
use crate::ids::EntityId;
use crate::ids::{ConstraintId, SnapshotId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Numeric(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEntity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub parameters: BTreeMap<String, ParameterValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstraintRuntimeState {
    Active,
    Suppressed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeConstraint {
    pub id: ConstraintId,
    pub kind: ConstraintKind,
    pub source: EntityId,
    pub target: EntityId,
    pub authority: AuthorityLevel,
    pub state: ConstraintRuntimeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_role: Option<AnchorRole>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub snapshot: SnapshotId,
    pub command_summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub root: u64,
    pub events: Vec<TraceEvent>,
}

impl ExecutionTrace {
    pub fn new(root: u64) -> Self {
        Self {
            root,
            events: Vec::new(),
        }
    }

    pub fn record(&mut self, snapshot: SnapshotId, message: String) {
        self.events.push(TraceEvent {
            snapshot,
            command_summary: message,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphState {
    pub root: u64,
    pub step: u64,
    pub entities: BTreeMap<EntityId, RuntimeEntity>,
    pub constraints: BTreeMap<ConstraintId, RuntimeConstraint>,
}

/// Structured validation failure for a [`GraphState`].
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    MissingEntity {
        constraint_id: ConstraintId,
        entity_id: EntityId,
    },
    MissingConstraint {
        constraint_id: ConstraintId,
    },
    OrphanEntity {
        entity_id: EntityId,
    },
}

impl ValidationError {
    pub fn code(&self) -> &'static str {
        match self {
            ValidationError::MissingEntity { .. } => "SCC-EXEC-001",
            ValidationError::MissingConstraint { .. } => "SCC-EXEC-004",
            ValidationError::OrphanEntity { .. } => "SCC-EXEC-003",
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingEntity {
                constraint_id,
                entity_id,
            } => write!(
                f,
                "constraint {} references missing entity {}",
                constraint_id, entity_id
            ),
            ValidationError::MissingConstraint { constraint_id } => {
                write!(
                    f,
                    "constraint {} references missing constraint",
                    constraint_id
                )
            }
            ValidationError::OrphanEntity { entity_id } => {
                write!(
                    f,
                    "entity {} is not referenced by any constraint",
                    entity_id
                )
            }
        }
    }
}

impl From<ValidationError> for StructuredError {
    fn from(err: ValidationError) -> Self {
        let code = err.code();
        match err {
            ValidationError::MissingEntity {
                constraint_id,
                entity_id,
            } => StructuredError::new(
                code,
                ErrorStage::Execution,
                "constraint references missing entity",
            )
            .with_entity_ids(vec![entity_id.clone()])
            .with_constraint_ids(vec![constraint_id])
            .with_recoverable(true)
            .with_suggestion("ensure the referenced entity exists in the graph state"),
            ValidationError::MissingConstraint { constraint_id } => {
                StructuredError::new(code, ErrorStage::Execution, "constraint reference missing")
                    .with_constraint_ids(vec![constraint_id])
                    .with_recoverable(true)
                    .with_suggestion("ensure the referenced constraint exists")
            }
            ValidationError::OrphanEntity { entity_id } => StructuredError::new(
                code,
                ErrorStage::Execution,
                "entity is not referenced by any constraint",
            )
            .with_entity_ids(vec![entity_id])
            .with_recoverable(true)
            .with_suggestion("add a constraint that references this entity or remove it"),
        }
    }
}

impl GraphState {
    pub fn new(root: u64) -> Self {
        Self {
            root,
            step: 0,
            entities: BTreeMap::new(),
            constraints: BTreeMap::new(),
        }
    }

    pub fn current_snapshot(&self) -> SnapshotId {
        SnapshotId(format!("{}:{}", self.root, self.step))
    }

    pub fn advance_step(&mut self) {
        self.step += 1;
    }

    /// Validates referential integrity and basic consistency of the state.
    /// Every constraint must reference entities that exist in the state,
    /// with array-derived constraints validated against their element side.
    pub fn validate_detailed(&self) -> Result<(), ValidationError> {
        let entity_ids: BTreeSet<&EntityId> = self.entities.keys().collect();
        for constraint in self.constraints.values() {
            match constraint.kind {
                ConstraintKind::ArrayRelation => {
                    if !entity_ids.contains(&constraint.source) {
                        return Err(ValidationError::MissingEntity {
                            constraint_id: constraint.id.clone(),
                            entity_id: constraint.source.clone(),
                        });
                    }
                }
                ConstraintKind::InstanceOf => {
                    if !entity_ids.contains(&constraint.target) {
                        return Err(ValidationError::MissingEntity {
                            constraint_id: constraint.id.clone(),
                            entity_id: constraint.target.clone(),
                        });
                    }
                }
                _ => {
                    if !entity_ids.contains(&constraint.source) {
                        return Err(ValidationError::MissingEntity {
                            constraint_id: constraint.id.clone(),
                            entity_id: constraint.source.clone(),
                        });
                    }
                    if !entity_ids.contains(&constraint.target) {
                        return Err(ValidationError::MissingEntity {
                            constraint_id: constraint.id.clone(),
                            entity_id: constraint.target.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Legacy v0.1.0 validation wrapper returning a string message.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_detailed()
            .map_err(|e| format!("{}: {}", e.code(), self.format_validation_error(e)))
    }

    /// Validates the final state, including that every entity is referenced by
    /// at least one constraint (no orphan nodes).
    pub fn validate_final_detailed(&self) -> Result<(), ValidationError> {
        self.validate_detailed()?;
        let mut referenced: BTreeSet<&EntityId> = BTreeSet::new();
        for constraint in self.constraints.values() {
            match constraint.kind {
                ConstraintKind::ArrayRelation => {
                    referenced.insert(&constraint.source);
                }
                ConstraintKind::InstanceOf => {
                    referenced.insert(&constraint.target);
                }
                _ => {
                    referenced.insert(&constraint.source);
                    referenced.insert(&constraint.target);
                }
            }
        }
        for entity_id in self.entities.keys() {
            if !referenced.contains(entity_id) {
                return Err(ValidationError::OrphanEntity {
                    entity_id: entity_id.clone(),
                });
            }
        }
        Ok(())
    }

    /// Legacy v0.1.0 final validation wrapper returning a string message.
    pub fn validate_final(&self) -> Result<(), String> {
        self.validate_final_detailed()
            .map_err(|e| format!("{}: {}", e.code(), self.format_validation_error(e)))
    }

    fn format_validation_error(&self, err: ValidationError) -> String {
        match err {
            ValidationError::MissingEntity {
                constraint_id,
                entity_id,
            } => format!(
                "constraint {} references missing entity {}",
                constraint_id, entity_id
            ),
            ValidationError::MissingConstraint { constraint_id } => {
                format!("constraint {} references missing constraint", constraint_id)
            }
            ValidationError::OrphanEntity { entity_id } => {
                format!("entity {} is not referenced by any constraint", entity_id)
            }
        }
    }
}
