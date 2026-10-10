use crate::constraint_context::ConstraintContext;
use crate::ids::{ConstraintId, EntityId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstraintKind {
    CenterOf,
    Distance,
    Tangent,
    Parallel,
    Perpendicular,
    On,
    Symmetric,
    Align,
    ArrayRelation,
    InstanceOf,
    /// Unary constraint: the source entity is pinned/fixed in space.
    /// Represented with `source == target`.
    Fixed,
    /// Unary constraint: the source entity serves as a reference/datum.
    /// Represented with `source == target`.
    Datum,
}

/// Preserves the proposal-level meaning of an anchor after lowering.
///
/// `ConstraintKind` continues to describe solver behavior (`Fixed` or `Datum`),
/// while this optional role distinguishes coordinate semantics without breaking
/// legacy IR and GraphState documents that do not contain the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorRole {
    OriginPoint,
    FixedCenter,
    ReferenceEdge,
    DatumLine,
}

/// Standard key for numeric constraint parameters stored in [`Constraint::parameters`].
pub const PARAM_VALUE: &str = "value";
pub const PARAM_DIAMETER: &str = "diameter";
pub const PARAM_RADIUS: &str = "radius";
pub const PARAM_WIDTH: &str = "width";
pub const PARAM_HEIGHT: &str = "height";
pub const PARAM_LENGTH: &str = "length";
pub const PARAM_OFFSET: &str = "offset";
pub const PARAM_AXIS_X: &str = "axis_x";
pub const PARAM_AXIS_Y: &str = "axis_y";
pub const PARAM_ARRAY_LINEAR: &str = "array_linear";
pub const PARAM_ARRAY_CIRCULAR: &str = "array_circular";
pub const PARAM_ARRAY_GRID: &str = "array_grid";
pub const PARAM_ARRAY_COUNT: &str = "array_count";
pub const PARAM_ARRAY_SPACING: &str = "array_spacing";
pub const PARAM_ARRAY_DIRECTION_X: &str = "array_direction_x";
pub const PARAM_ARRAY_DIRECTION_Y: &str = "array_direction_y";
pub const PARAM_ARRAY_ROWS: &str = "array_rows";
pub const PARAM_ARRAY_COLS: &str = "array_cols";
pub const PARAM_ARRAY_ROW_SPACING: &str = "array_row_spacing";
pub const PARAM_ARRAY_COL_SPACING: &str = "array_col_spacing";
/// Entity coordinate parameter written by the execution layer from an
/// `X` position dimension (lowered as `Distance(source == target)`).
pub const PARAM_X: &str = "x";
/// Entity coordinate parameter written by the execution layer from a
/// `Y` position dimension.
pub const PARAM_Y: &str = "y";
/// Endpoint coordinates for an exact line/profile segment.
pub const PARAM_X1: &str = "x1";
pub const PARAM_Y1: &str = "y1";
pub const PARAM_X2: &str = "x2";
pub const PARAM_Y2: &str = "y2";
/// Directed angular bounds for an exact arc/profile segment.
pub const PARAM_START_ANGLE: &str = "start_angle";
pub const PARAM_END_ANGLE: &str = "end_angle";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub id: ConstraintId,
    pub kind: ConstraintKind,
    pub source: EntityId,
    pub target: EntityId,
    pub context: ConstraintContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_role: Option<AnchorRole>,
    #[serde(default)]
    pub parameters: BTreeMap<String, f64>,
}
