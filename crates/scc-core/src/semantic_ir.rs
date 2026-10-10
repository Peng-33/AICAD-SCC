use serde::{Deserialize, Serialize};

use crate::array::ArrayNode;
use crate::entity::Entity;
use crate::graph::DeclarationGraph;
use crate::ids::EntityId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticRelation {
    CenteredIn {
        subject: EntityId,
        reference: EntityId,
        axis: Option<String>,
    },
    Inside {
        subject: EntityId,
        reference: EntityId,
    },
    Outside {
        subject: EntityId,
        reference: EntityId,
    },
    ParallelTo {
        subject: EntityId,
        reference: EntityId,
    },
    PerpendicularTo {
        subject: EntityId,
        reference: EntityId,
    },
    TangentTo {
        subject: EntityId,
        reference: EntityId,
    },
    SymmetricAbout {
        subject: EntityId,
        reference: EntityId,
        axis: Option<String>,
    },
    AlignedWith {
        subject: EntityId,
        reference: EntityId,
    },
    DistanceFrom {
        subject: EntityId,
        reference: EntityId,
        value: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticDimension {
    Diameter {
        target: EntityId,
        value: f64,
    },
    Radius {
        target: EntityId,
        value: f64,
    },
    Width {
        target: EntityId,
        value: f64,
    },
    Height {
        target: EntityId,
        value: f64,
    },
    Length {
        target: EntityId,
        value: f64,
    },
    Offset {
        target: EntityId,
        value: f64,
    },
    /// Explicit X coordinate of an entity's position. Lowered to the `x`
    /// parameter of a `Distance(source == target)` constraint so the execution
    /// layer stores it on the entity (Phase 3 engineering lowering).
    X {
        target: EntityId,
        value: f64,
    },
    /// Explicit Y coordinate of an entity's position (see [`Self::X`]).
    Y {
        target: EntityId,
        value: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticAnchor {
    OriginPoint { id: EntityId },
    FixedCenter { id: EntityId, entity: EntityId },
    ReferenceEdge { id: EntityId, entity: EntityId },
    DatumLine { id: EntityId, entity: EntityId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticIr {
    pub entities: Vec<Entity>,
    pub relations: Vec<SemanticRelation>,
    pub dimensions: Vec<SemanticDimension>,
    pub arrays: Vec<ArrayNode>,
    pub anchors: Vec<SemanticAnchor>,
    pub graph: DeclarationGraph,
}
