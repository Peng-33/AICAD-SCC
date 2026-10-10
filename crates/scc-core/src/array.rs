use serde::{Deserialize, Serialize};

use crate::ids::{ArrayId, EntityId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatternNode {
    Linear {
        count: u32,
        spacing: f64,
        direction: Option<String>,
    },
    Circular {
        count: u32,
        radius: f64,
        center: Option<String>,
    },
    Grid {
        rows: u32,
        cols: u32,
        row_spacing: f64,
        col_spacing: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrayNode {
    pub id: ArrayId,
    pub pattern: PatternNode,
    pub element: EntityId,
}
