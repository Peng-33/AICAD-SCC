use std::collections::{BTreeMap, BTreeSet};

use scc_core::constraint::{AnchorRole, ConstraintKind, PARAM_AXIS_X, PARAM_AXIS_Y, PARAM_VALUE};
use scc_core::entity::EntityKind;
use scc_core::ids::EntityId;
use scc_core::runtime::{
    ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
};

use crate::error::GeometryError;
use crate::model::Point2;

const POSITION_EPSILON: f64 = 1.0e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn parameter(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Axis::X => PARAM_AXIS_X,
            Axis::Y => PARAM_AXIS_Y,
        }
    }

    fn select(self, point: Point2) -> f64 {
        match self {
            Axis::X => point.x,
            Axis::Y => point.y,
        }
    }
}

pub(crate) struct PositionResolver<'a> {
    state: &'a GraphState,
    coordinates: BTreeMap<(EntityId, Axis), f64>,
    resolving: BTreeSet<(EntityId, Axis)>,
}

impl<'a> PositionResolver<'a> {
    pub(crate) fn new(state: &'a GraphState) -> Self {
        Self {
            state,
            coordinates: BTreeMap::new(),
            resolving: BTreeSet::new(),
        }
    }

    /// Returns the coordinate used by the serialized primitive.
    ///
    /// For rectangles this is the lower-left origin. For points, circles, and
    /// arcs it is their geometric center/position.
    pub(crate) fn primitive_position(
        &mut self,
        entity: &RuntimeEntity,
    ) -> Result<Point2, GeometryError> {
        let center = self.geometric_center(&entity.id)?;
        if entity.kind == EntityKind::Rectangle {
            let width = positive_parameter(entity, "width")?;
            let height = positive_parameter(entity, "height")?;
            Ok(Point2 {
                x: center.x - width / 2.0,
                y: center.y - height / 2.0,
            })
        } else {
            Ok(center)
        }
    }

    fn geometric_center(&mut self, entity_id: &EntityId) -> Result<Point2, GeometryError> {
        Ok(Point2 {
            x: self.geometric_coordinate(entity_id, Axis::X)?,
            y: self.geometric_coordinate(entity_id, Axis::Y)?,
        })
    }

    fn geometric_coordinate(
        &mut self,
        entity_id: &EntityId,
        axis: Axis,
    ) -> Result<f64, GeometryError> {
        let key = (entity_id.clone(), axis);
        if let Some(value) = self.coordinates.get(&key) {
            return Ok(*value);
        }
        if !self.resolving.insert(key.clone()) {
            return Err(GeometryError::InvalidGraph {
                message: format!(
                    "circular position dependency prevents resolving entity {} axis {}",
                    entity_id,
                    axis.parameter()
                ),
            });
        }

        let result = self.compute_coordinate(entity_id, axis);
        self.resolving.remove(&key);
        if let Ok(value) = result {
            self.coordinates.insert(key, value);
        }
        result
    }

    fn compute_coordinate(
        &mut self,
        entity_id: &EntityId,
        axis: Axis,
    ) -> Result<f64, GeometryError> {
        let entity = self.state.entities.get(entity_id).cloned().ok_or_else(|| {
            GeometryError::InvalidGraph {
                message: format!("position references missing entity {}", entity_id),
            }
        })?;

        if entity.kind == EntityKind::Line {
            return Ok(axis.select(explicit_line_center(&entity)?));
        }
        if entity.kind == EntityKind::Polygon {
            return Err(GeometryError::UnsupportedEntityKind {
                entity_id: entity.id,
                kind: entity.kind,
            });
        }

        let mut candidates: Vec<(String, f64)> = Vec::new();
        if let Some(value) = explicit_coordinate_candidate(&entity, axis)? {
            candidates.push((format!("explicit {} parameter", axis.parameter()), value));
        }

        let positioning_constraints: Vec<RuntimeConstraint> = self
            .state
            .constraints
            .values()
            .filter(|constraint| {
                constraint.state == ConstraintRuntimeState::Active
                    && &constraint.source == entity_id
                    && matches!(
                        constraint.kind,
                        ConstraintKind::Fixed | ConstraintKind::CenterOf
                    )
            })
            .cloned()
            .collect();

        for constraint in positioning_constraints {
            match constraint.kind {
                ConstraintKind::Fixed if constraint.source == constraint.target => {
                    let center = fixed_center_candidate(&entity, &constraint)?;
                    candidates.push((format!("constraint {}", constraint.id), axis.select(center)));
                }
                ConstraintKind::CenterOf if center_constraint_uses_axis(&constraint, axis) => {
                    let value = self.geometric_coordinate(&constraint.target, axis)?;
                    candidates.push((format!("constraint {}", constraint.id), value));
                }
                _ => {}
            }
        }

        if entity.kind == EntityKind::Point && candidates.is_empty() {
            candidates.extend(self.point_zero_distance_candidates(&entity, axis)?);
        }

        let (first_source, first) =
            candidates
                .first()
                .cloned()
                .ok_or_else(|| GeometryError::MissingParameter {
                    entity_id: entity.id.clone(),
                    parameter: format!(
                        "{} or a positioning constraint on that axis",
                        axis.parameter()
                    ),
                })?;

        for (source, candidate) in candidates.iter().skip(1) {
            if !same_coordinate(first, *candidate) {
                return Err(GeometryError::ConflictingParameters {
                    entity_id: entity.id.clone(),
                    reason: format!(
                        "{} resolves axis {} to {}, but {} resolves it to {}",
                        first_source,
                        axis.parameter(),
                        first,
                        source,
                        candidate
                    ),
                });
            }
        }

        Ok(first)
    }

    fn point_zero_distance_candidates(
        &mut self,
        entity: &RuntimeEntity,
        axis: Axis,
    ) -> Result<Vec<(String, f64)>, GeometryError> {
        let mut candidates = Vec::new();
        for constraint in self.state.constraints.values() {
            if constraint.state != ConstraintRuntimeState::Active
                || constraint.kind != ConstraintKind::Distance
                || constraint.source == constraint.target
                || constraint.parameters.get(PARAM_VALUE) != Some(&0.0)
            {
                continue;
            }

            let other_id = if constraint.source == entity.id {
                Some(&constraint.target)
            } else if constraint.target == entity.id {
                Some(&constraint.source)
            } else {
                None
            };
            let Some(other_id) = other_id else {
                continue;
            };

            let other = self.geometric_coordinate(other_id, axis)?;
            candidates.push((format!("constraint {}", constraint.id), other));
        }
        Ok(candidates)
    }
}

fn center_constraint_uses_axis(constraint: &RuntimeConstraint, axis: Axis) -> bool {
    constraint.parameters.is_empty() || constraint.parameters.get(axis.marker()) == Some(&1.0)
}

fn fixed_center_candidate(
    entity: &RuntimeEntity,
    constraint: &RuntimeConstraint,
) -> Result<Point2, GeometryError> {
    let origin = Point2 { x: 0.0, y: 0.0 };
    match constraint.anchor_role {
        Some(AnchorRole::FixedCenter) | Some(AnchorRole::OriginPoint) => Ok(origin),
        Some(AnchorRole::ReferenceEdge) | Some(AnchorRole::DatumLine) => {
            Err(GeometryError::InvalidGraph {
                message: format!(
                    "constraint {} uses datum anchor role with Fixed kind",
                    constraint.id
                ),
            })
        }
        None => coordinate_anchor_to_center(entity, origin),
    }
}

fn coordinate_anchor_to_center(
    entity: &RuntimeEntity,
    position: Point2,
) -> Result<Point2, GeometryError> {
    if entity.kind == EntityKind::Rectangle {
        Ok(Point2 {
            x: position.x + positive_parameter(entity, "width")? / 2.0,
            y: position.y + positive_parameter(entity, "height")? / 2.0,
        })
    } else {
        Ok(position)
    }
}

fn explicit_coordinate_candidate(
    entity: &RuntimeEntity,
    axis: Axis,
) -> Result<Option<f64>, GeometryError> {
    let Some(value) = optional_parameter(entity, axis.parameter())? else {
        return Ok(None);
    };
    if entity.kind != EntityKind::Rectangle {
        return Ok(Some(value));
    }
    let half_extent = match axis {
        Axis::X => positive_parameter(entity, "width")? / 2.0,
        Axis::Y => positive_parameter(entity, "height")? / 2.0,
    };
    Ok(Some(value + half_extent))
}

fn explicit_line_center(entity: &RuntimeEntity) -> Result<Point2, GeometryError> {
    let x1 = required_parameter(entity, "x1")?;
    let y1 = required_parameter(entity, "y1")?;
    let x2 = required_parameter(entity, "x2")?;
    let y2 = required_parameter(entity, "y2")?;
    Ok(Point2 {
        x: (x1 + x2) / 2.0,
        y: (y1 + y2) / 2.0,
    })
}

fn same_coordinate(left: f64, right: f64) -> bool {
    (left - right).abs() <= POSITION_EPSILON
}

fn optional_parameter(entity: &RuntimeEntity, name: &str) -> Result<Option<f64>, GeometryError> {
    let value = entity.parameters.get(name).map(|value| match value {
        ParameterValue::Numeric(value) => *value,
    });
    if value.is_some_and(|value| !value.is_finite()) {
        return Err(GeometryError::InvalidParameter {
            entity_id: entity.id.clone(),
            parameter: name.into(),
            reason: "value must be finite".into(),
        });
    }
    Ok(value)
}

fn required_parameter(entity: &RuntimeEntity, name: &str) -> Result<f64, GeometryError> {
    optional_parameter(entity, name)?.ok_or_else(|| GeometryError::MissingParameter {
        entity_id: entity.id.clone(),
        parameter: name.into(),
    })
}

fn positive_parameter(entity: &RuntimeEntity, name: &str) -> Result<f64, GeometryError> {
    let value = required_parameter(entity, name)?;
    if value <= 0.0 {
        return Err(GeometryError::InvalidParameter {
            entity_id: entity.id.clone(),
            parameter: name.into(),
            reason: "value must be greater than zero".into(),
        });
    }
    Ok(value)
}
