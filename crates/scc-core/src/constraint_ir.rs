use serde::{Deserialize, Serialize};

use crate::constraint::Constraint;
use crate::entity::Entity;
use crate::graph::DeclarationGraph;
use crate::ids::ConstraintId;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IrConstraintLevels {
    pub l0: Vec<ConstraintId>,
    pub l1: Vec<ConstraintId>,
    pub l2: Vec<ConstraintId>,
    pub l3: Vec<ConstraintId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrMetadata {
    pub constraint_levels: IrConstraintLevels,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintIr {
    pub entities: Vec<Entity>,
    pub constraints: Vec<Constraint>,
    pub graph: DeclarationGraph,
    pub metadata: IrMetadata,
}
