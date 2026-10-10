use serde::{Deserialize, Serialize};

use crate::ids::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Point,
    Line,
    Circle,
    Arc,
    Rectangle,
    Polygon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
}
