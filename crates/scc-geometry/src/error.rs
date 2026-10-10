use scc_core::constraint::ConstraintKind;
use scc_core::entity::EntityKind;
use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::{ConstraintId, EntityId};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GeometryError {
    #[error("graph state is not valid for geometry solving: {message}")]
    InvalidGraph { message: String },

    #[error("entity {entity_id} is missing required geometry parameter {parameter}")]
    MissingParameter {
        entity_id: EntityId,
        parameter: String,
    },

    #[error("entity {entity_id} has invalid geometry parameter {parameter}: {reason}")]
    InvalidParameter {
        entity_id: EntityId,
        parameter: String,
        reason: String,
    },

    #[error("entity {entity_id} has unsupported geometry kind {kind:?}")]
    UnsupportedEntityKind {
        entity_id: EntityId,
        kind: EntityKind,
    },

    #[error("constraint {constraint_id} has unsupported geometry kind {kind:?}")]
    UnsupportedConstraint {
        constraint_id: ConstraintId,
        kind: ConstraintKind,
    },

    #[error("entity {entity_id} has conflicting geometry parameters: {reason}")]
    ConflictingParameters { entity_id: EntityId, reason: String },

    #[error(
        "geometry is underconstrained with {remaining_dof} remaining degrees of freedom: {reason}"
    )]
    Underconstrained {
        entity_ids: Vec<EntityId>,
        remaining_dof: u32,
        reason: String,
    },
}

impl GeometryError {
    pub fn code(&self) -> &'static str {
        match self {
            GeometryError::InvalidGraph { .. } => "SCC-GEOM-001",
            GeometryError::MissingParameter { .. } => "SCC-GEOM-002",
            GeometryError::InvalidParameter { .. } => "SCC-GEOM-003",
            GeometryError::UnsupportedEntityKind { .. } => "SCC-GEOM-004",
            GeometryError::UnsupportedConstraint { .. } => "SCC-GEOM-005",
            GeometryError::ConflictingParameters { .. } => "SCC-GEOM-006",
            GeometryError::Underconstrained { .. } => "SCC-GEOM-007",
        }
    }
}

impl From<GeometryError> for StructuredError {
    fn from(error: GeometryError) -> Self {
        let code = error.code();
        match error {
            GeometryError::InvalidGraph { message } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                "graph state is not valid for geometry solving",
            )
            .with_recoverable(true)
            .with_suggestion(message),
            GeometryError::MissingParameter {
                entity_id,
                parameter,
            } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                format!("missing required geometry parameter {}", parameter),
            )
            .with_entity_ids(vec![entity_id])
            .with_recoverable(true)
            .with_suggestion("provide the parameter through proposal dimensions or a mutation"),
            GeometryError::InvalidParameter {
                entity_id,
                parameter,
                reason,
            } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                format!("invalid geometry parameter {}: {}", parameter, reason),
            )
            .with_entity_ids(vec![entity_id])
            .with_recoverable(true),
            GeometryError::UnsupportedEntityKind { entity_id, kind } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                format!(
                    "entity kind {:?} is not supported by the current solver",
                    kind
                ),
            )
            .with_entity_ids(vec![entity_id])
            .with_recoverable(true),
            GeometryError::UnsupportedConstraint {
                constraint_id,
                kind,
            } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                format!(
                    "constraint kind {:?} is not supported by the current solver",
                    kind
                ),
            )
            .with_constraint_ids(vec![constraint_id])
            .with_recoverable(true),
            GeometryError::ConflictingParameters { entity_id, reason } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                format!("conflicting geometry parameters: {}", reason),
            )
            .with_entity_ids(vec![entity_id])
            .with_recoverable(true),
            GeometryError::Underconstrained {
                entity_ids,
                remaining_dof,
                reason,
            } => StructuredError::new(
                code,
                ErrorStage::Geometry,
                format!(
                    "geometry is underconstrained with {} remaining degrees of freedom: {}",
                    remaining_dof, reason
                ),
            )
            .with_entity_ids(entity_ids)
            .with_detail(
                "remaining_degrees_of_freedom",
                serde_json::json!(remaining_dof),
            )
            .with_recoverable(true)
            .with_suggestion(
                "add independent positioning constraints or explicit coordinates until the remaining degrees of freedom reach zero",
            ),
        }
    }
}

impl From<GeometryError> for SccError {
    fn from(error: GeometryError) -> Self {
        SccError::Structured(Box::new(error.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_error_uses_valid_structured_contract() {
        let errors = [
            GeometryError::InvalidGraph {
                message: "missing reference".into(),
            },
            GeometryError::MissingParameter {
                entity_id: EntityId("circle".into()),
                parameter: "radius".into(),
            },
            GeometryError::InvalidParameter {
                entity_id: EntityId("circle".into()),
                parameter: "radius".into(),
                reason: "must be positive".into(),
            },
            GeometryError::UnsupportedEntityKind {
                entity_id: EntityId("polygon".into()),
                kind: EntityKind::Polygon,
            },
            GeometryError::UnsupportedConstraint {
                constraint_id: ConstraintId("c1".into()),
                kind: ConstraintKind::Tangent,
            },
            GeometryError::ConflictingParameters {
                entity_id: EntityId("circle".into()),
                reason: "radius and diameter disagree".into(),
            },
            GeometryError::Underconstrained {
                entity_ids: vec![EntityId("point".into())],
                remaining_dof: 1,
                reason: "distance fixes radius but not direction".into(),
            },
        ];

        for error in errors {
            let structured: StructuredError = error.into();
            assert!(structured.validate_contract().is_ok());
            assert_eq!(structured.stage, ErrorStage::Geometry);
        }
    }
}
