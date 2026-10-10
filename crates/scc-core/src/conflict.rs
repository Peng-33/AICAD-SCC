use serde::{Deserialize, Serialize};

use crate::ids::{ConstraintId, EntityId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictCategory {
    Structural,
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictType {
    InvalidEntity,
    MissingReference,
    CircularDependency,
    TypeMismatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinimalConflictSubgraph {
    pub conflicting_constraints: Vec<ConstraintId>,
    pub conflict_type: ConflictType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictFacts {
    pub affected_entities: Vec<EntityId>,
    pub affected_constraints: Vec<ConstraintId>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub category: ConflictCategory,
    pub conflict_type: ConflictType,
    pub mcs: MinimalConflictSubgraph,
    pub facts: ConflictFacts,
}
