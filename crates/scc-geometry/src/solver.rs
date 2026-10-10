use std::collections::{BTreeMap, BTreeSet};

use scc_core::constraint::{
    AnchorRole, ConstraintKind, PARAM_ARRAY_CIRCULAR, PARAM_ARRAY_COLS, PARAM_ARRAY_COL_SPACING,
    PARAM_ARRAY_COUNT, PARAM_ARRAY_DIRECTION_X, PARAM_ARRAY_DIRECTION_Y, PARAM_ARRAY_GRID,
    PARAM_ARRAY_LINEAR, PARAM_ARRAY_ROWS, PARAM_ARRAY_ROW_SPACING, PARAM_ARRAY_SPACING,
    PARAM_AXIS_X, PARAM_AXIS_Y, PARAM_RADIUS, PARAM_VALUE,
};
use scc_core::entity::EntityKind;
use scc_core::ids::EntityId;
use scc_core::runtime::{
    ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
};

use crate::error::GeometryError;
use crate::model::{GeometryEntity, GeometryModel, GeometryPrimitive, Point2};
use crate::resolution::PositionResolver;

const CONSTRAINT_EPSILON: f64 = 1.0e-9;
const CIRCULAR_COORDINATE_SCALE: f64 = 1.0e12;
const MAX_POLYGON_VERTICES: u32 = 100_000;

pub trait GeometrySolver {
    fn solve(&self, state: &GraphState) -> Result<GeometryModel, GeometryError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DeterministicGeometrySolver;

impl GeometrySolver for DeterministicGeometrySolver {
    fn solve(&self, state: &GraphState) -> Result<GeometryModel, GeometryError> {
        state
            .validate_detailed()
            .map_err(|error| GeometryError::InvalidGraph {
                message: format!("{}: {}", error.code(), error),
            })?;
        validate_supported_constraints(state)?;
        validate_partial_center_degrees_of_freedom(state)?;
        validate_point_distance_degrees_of_freedom(state)?;

        let evaluated_constraints = state
            .constraints
            .values()
            .filter(|constraint| constraint.state == ConstraintRuntimeState::Active)
            .count();
        let mut model = GeometryModel::new(state.root, state.step, evaluated_constraints);
        let mut positions = PositionResolver::new(state);
        for (entity_id, entity) in &state.entities {
            let primitive = solve_entity(&mut positions, entity)?;
            model.entities.insert(
                entity_id.clone(),
                GeometryEntity {
                    id: entity_id.clone(),
                    primitive,
                },
            );
        }
        validate_distance_constraints(state, &model)?;
        validate_align_constraints(state, &model)?;
        validate_parallel_constraints(state, &model)?;
        validate_perpendicular_constraints(state, &model)?;
        expand_arrays(state, &mut model)?;
        Ok(model)
    }
}

fn validate_supported_constraints(state: &GraphState) -> Result<(), GeometryError> {
    for constraint in state.constraints.values() {
        if constraint.state == ConstraintRuntimeState::Suppressed {
            continue;
        }
        let supported = match constraint.kind {
            ConstraintKind::Fixed => fixed_anchor_supported(state, constraint),
            ConstraintKind::Datum => datum_anchor_supported(constraint),
            ConstraintKind::CenterOf => {
                center_positionable_source(state, &constraint.source)
                    && center_axis_selection_supported(constraint)
            }
            ConstraintKind::Distance => {
                constraint.source == constraint.target
                    || point_distance_supported(state, constraint)
            }
            ConstraintKind::Align | ConstraintKind::Parallel | ConstraintKind::Perpendicular => {
                line_direction_constraint_supported(state, constraint)
            }
            ConstraintKind::ArrayRelation => array_relation_supported(state, constraint),
            ConstraintKind::InstanceOf => instance_of_supported(state, constraint),
            _ => false,
        };
        if !supported {
            return Err(GeometryError::UnsupportedConstraint {
                constraint_id: constraint.id.clone(),
                kind: constraint.kind.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ArrayPattern {
    Linear {
        count: u32,
        spacing: f64,
        direction: CoordinateAxis,
    },
    Circular {
        count: u32,
        radius: f64,
    },
    Grid {
        rows: u32,
        cols: u32,
        row_spacing: f64,
        col_spacing: f64,
    },
}

fn array_relation_supported(state: &GraphState, constraint: &RuntimeConstraint) -> bool {
    state.entities.contains_key(&constraint.source)
        && array_pattern(constraint).is_some()
        && state.constraints.values().any(|candidate| {
            candidate.state == ConstraintRuntimeState::Active
                && candidate.kind == ConstraintKind::InstanceOf
                && candidate.source == constraint.target
                && candidate.target == constraint.source
        })
}

fn instance_of_supported(state: &GraphState, constraint: &RuntimeConstraint) -> bool {
    state.entities.contains_key(&constraint.target)
        && state.constraints.values().any(|candidate| {
            candidate.state == ConstraintRuntimeState::Active
                && candidate.kind == ConstraintKind::ArrayRelation
                && candidate.source == constraint.target
                && candidate.target == constraint.source
                && array_pattern(candidate).is_some()
        })
}

fn array_pattern(constraint: &RuntimeConstraint) -> Option<ArrayPattern> {
    let markers = [PARAM_ARRAY_LINEAR, PARAM_ARRAY_CIRCULAR, PARAM_ARRAY_GRID];
    if markers
        .iter()
        .filter(|name| constraint.parameters.get(**name) == Some(&1.0))
        .count()
        != 1
    {
        return None;
    }

    if constraint.parameters.get(PARAM_ARRAY_LINEAR) == Some(&1.0) {
        let count = positive_integer_constraint_parameter(constraint, PARAM_ARRAY_COUNT)?;
        let spacing = nonnegative_constraint_parameter(constraint, PARAM_ARRAY_SPACING)?;
        let direction = match (
            constraint.parameters.get(PARAM_ARRAY_DIRECTION_X),
            constraint.parameters.get(PARAM_ARRAY_DIRECTION_Y),
        ) {
            (Some(1.0), None) => CoordinateAxis::X,
            (None, Some(1.0)) => CoordinateAxis::Y,
            _ => return None,
        };
        return Some(ArrayPattern::Linear {
            count,
            spacing,
            direction,
        });
    }

    if constraint.parameters.get(PARAM_ARRAY_CIRCULAR) == Some(&1.0) {
        let count = positive_integer_constraint_parameter(constraint, PARAM_ARRAY_COUNT)?;
        let radius = *constraint.parameters.get(PARAM_RADIUS)?;
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        return Some(ArrayPattern::Circular { count, radius });
    }

    let rows = positive_integer_constraint_parameter(constraint, PARAM_ARRAY_ROWS)?;
    let cols = positive_integer_constraint_parameter(constraint, PARAM_ARRAY_COLS)?;
    let row_spacing = nonnegative_constraint_parameter(constraint, PARAM_ARRAY_ROW_SPACING)?;
    let col_spacing = nonnegative_constraint_parameter(constraint, PARAM_ARRAY_COL_SPACING)?;
    Some(ArrayPattern::Grid {
        rows,
        cols,
        row_spacing,
        col_spacing,
    })
}

fn positive_integer_constraint_parameter(
    constraint: &RuntimeConstraint,
    name: &str,
) -> Option<u32> {
    let value = *constraint.parameters.get(name)?;
    if !value.is_finite() || value < 1.0 || value > f64::from(u32::MAX) || value.fract() != 0.0 {
        return None;
    }
    Some(value as u32)
}

fn nonnegative_constraint_parameter(constraint: &RuntimeConstraint, name: &str) -> Option<f64> {
    let value = *constraint.parameters.get(name)?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn expand_arrays(state: &GraphState, model: &mut GeometryModel) -> Result<(), GeometryError> {
    let templates = model.entities.clone();
    let arrays: Vec<_> = state
        .constraints
        .values()
        .filter(|constraint| {
            constraint.state == ConstraintRuntimeState::Active
                && constraint.kind == ConstraintKind::ArrayRelation
        })
        .collect();
    let mut template_ids = BTreeSet::new();
    let mut derived_ids = BTreeSet::new();
    let mut derived = Vec::new();

    for constraint in arrays {
        let template =
            templates
                .get(&constraint.source)
                .ok_or_else(|| GeometryError::InvalidGraph {
                    message: format!(
                        "array {} references missing template geometry {}",
                        constraint.target, constraint.source
                    ),
                })?;
        template_ids.insert(constraint.source.clone());
        let pattern =
            array_pattern(constraint).ok_or_else(|| GeometryError::UnsupportedConstraint {
                constraint_id: constraint.id.clone(),
                kind: constraint.kind.clone(),
            })?;

        let offsets = array_offsets(pattern);
        for (index, offset) in offsets.into_iter().enumerate() {
            let id = EntityId(format!("{}[{}]", constraint.target.0, index));
            if templates.contains_key(&id) || !derived_ids.insert(id.clone()) {
                return Err(GeometryError::InvalidGraph {
                    message: format!("array-derived entity id {} is not unique", id),
                });
            }
            derived.push(GeometryEntity {
                id,
                primitive: translate_primitive(&template.primitive, offset),
            });
        }
    }

    for template_id in template_ids {
        model.entities.remove(&template_id);
    }
    for entity in derived {
        model.entities.insert(entity.id.clone(), entity);
    }
    Ok(())
}

fn array_offsets(pattern: ArrayPattern) -> Vec<Point2> {
    match pattern {
        ArrayPattern::Linear {
            count,
            spacing,
            direction,
        } => (0..count)
            .map(|index| match direction {
                CoordinateAxis::X => Point2 {
                    x: f64::from(index) * spacing,
                    y: 0.0,
                },
                CoordinateAxis::Y => Point2 {
                    x: 0.0,
                    y: f64::from(index) * spacing,
                },
            })
            .collect(),
        ArrayPattern::Circular { count, radius } => (0..count)
            .map(|index| {
                let angle = std::f64::consts::TAU * f64::from(index) / f64::from(count);
                Point2 {
                    x: stable_circular_coordinate(radius * angle.cos()),
                    y: stable_circular_coordinate(radius * angle.sin()),
                }
            })
            .collect(),
        ArrayPattern::Grid {
            rows,
            cols,
            row_spacing,
            col_spacing,
        } => (0..rows)
            .flat_map(|row| {
                (0..cols).map(move |col| Point2 {
                    x: f64::from(col) * col_spacing,
                    y: f64::from(row) * row_spacing,
                })
            })
            .collect(),
    }
}

fn stable_circular_coordinate(value: f64) -> f64 {
    let rounded = (value * CIRCULAR_COORDINATE_SCALE).round() / CIRCULAR_COORDINATE_SCALE;
    if rounded == -0.0 {
        0.0
    } else {
        rounded
    }
}

fn translate_primitive(primitive: &GeometryPrimitive, offset: Point2) -> GeometryPrimitive {
    let point = |point: Point2| Point2 {
        x: point.x + offset.x,
        y: point.y + offset.y,
    };
    match primitive {
        GeometryPrimitive::Point { position } => GeometryPrimitive::Point {
            position: point(*position),
        },
        GeometryPrimitive::Line { start, end } => GeometryPrimitive::Line {
            start: point(*start),
            end: point(*end),
        },
        GeometryPrimitive::Circle { center, radius } => GeometryPrimitive::Circle {
            center: point(*center),
            radius: *radius,
        },
        GeometryPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => GeometryPrimitive::Arc {
            center: point(*center),
            radius: *radius,
            start_angle: *start_angle,
            end_angle: *end_angle,
        },
        GeometryPrimitive::Rectangle {
            origin,
            width,
            height,
        } => GeometryPrimitive::Rectangle {
            origin: point(*origin),
            width: *width,
            height: *height,
        },
        GeometryPrimitive::Polygon { vertices } => GeometryPrimitive::Polygon {
            vertices: vertices.iter().copied().map(point).collect(),
        },
    }
}

fn point_distance_supported(
    state: &GraphState,
    constraint: &scc_core::runtime::RuntimeConstraint,
) -> bool {
    if constraint.source == constraint.target {
        return false;
    }
    let Some(value) = constraint.parameters.get(PARAM_VALUE) else {
        return false;
    };
    if !value.is_finite() || *value < 0.0 {
        return false;
    }
    matches!(state.entities.get(&constraint.source), Some(entity) if entity.kind == EntityKind::Point)
        && matches!(state.entities.get(&constraint.target), Some(entity) if entity.kind == EntityKind::Point)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CoordinateAxis {
    X,
    Y,
}

impl CoordinateAxis {
    fn parameter(self) -> &'static str {
        match self {
            CoordinateAxis::X => "x",
            CoordinateAxis::Y => "y",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            CoordinateAxis::X => PARAM_AXIS_X,
            CoordinateAxis::Y => PARAM_AXIS_Y,
        }
    }
}

fn validate_partial_center_degrees_of_freedom(state: &GraphState) -> Result<(), GeometryError> {
    let sources: BTreeSet<_> = state
        .constraints
        .values()
        .filter(|constraint| {
            constraint.state == ConstraintRuntimeState::Active
                && constraint.kind == ConstraintKind::CenterOf
                && center_is_partial(constraint)
        })
        .map(|constraint| constraint.source.clone())
        .collect();

    let mut unresolved_entities = Vec::new();
    let mut remaining_dof = 0_u32;
    for entity_id in sources {
        for axis in [CoordinateAxis::X, CoordinateAxis::Y] {
            let mut visiting = BTreeSet::new();
            let mut complete = BTreeSet::new();
            if center_axis_dependency_has_cycle(
                state,
                &entity_id,
                axis,
                &mut visiting,
                &mut complete,
            ) {
                return Err(GeometryError::InvalidGraph {
                    message: format!(
                        "circular position dependency prevents resolving entity {} axis {}",
                        entity_id,
                        axis.parameter()
                    ),
                });
            }
        }

        let missing_axes = [CoordinateAxis::X, CoordinateAxis::Y]
            .into_iter()
            .filter(|axis| !coordinate_axis_is_positioned(state, &entity_id, *axis))
            .count() as u32;
        if missing_axes > 0 {
            unresolved_entities.push(entity_id);
            remaining_dof += missing_axes;
        }
    }

    if remaining_dof > 0 {
        return Err(GeometryError::Underconstrained {
            entity_ids: unresolved_entities,
            remaining_dof,
            reason:
                "partial-axis CenterOf constraints leave one or more coordinate axes unresolved"
                    .into(),
        });
    }
    Ok(())
}

fn center_axis_dependency_has_cycle(
    state: &GraphState,
    entity_id: &EntityId,
    axis: CoordinateAxis,
    visiting: &mut BTreeSet<(EntityId, CoordinateAxis)>,
    complete: &mut BTreeSet<(EntityId, CoordinateAxis)>,
) -> bool {
    let key = (entity_id.clone(), axis);
    if complete.contains(&key) {
        return false;
    }
    if !visiting.insert(key.clone()) {
        return true;
    }

    let has_cycle = state.constraints.values().any(|constraint| {
        constraint.state == ConstraintRuntimeState::Active
            && constraint.kind == ConstraintKind::CenterOf
            && constraint.source == *entity_id
            && center_uses_axis(constraint, axis)
            && center_axis_dependency_has_cycle(state, &constraint.target, axis, visiting, complete)
    });
    visiting.remove(&key);
    if !has_cycle {
        complete.insert(key);
    }
    has_cycle
}

fn coordinate_axis_is_positioned(
    state: &GraphState,
    entity_id: &EntityId,
    axis: CoordinateAxis,
) -> bool {
    let mut memo = BTreeMap::new();
    let mut resolving = BTreeSet::new();
    coordinate_axis_is_positioned_inner(state, entity_id, axis, &mut memo, &mut resolving)
}

fn coordinate_axis_is_positioned_inner(
    state: &GraphState,
    entity_id: &EntityId,
    axis: CoordinateAxis,
    memo: &mut BTreeMap<(EntityId, CoordinateAxis), bool>,
    resolving: &mut BTreeSet<(EntityId, CoordinateAxis)>,
) -> bool {
    let key = (entity_id.clone(), axis);
    if let Some(positioned) = memo.get(&key) {
        return *positioned;
    }
    if !resolving.insert(key.clone()) {
        return false;
    }

    let positioned = state.entities.get(entity_id).is_some_and(|entity| {
        if entity.kind == EntityKind::Line {
            let parameters = match axis {
                CoordinateAxis::X => ["x1", "x2"],
                CoordinateAxis::Y => ["y1", "y2"],
            };
            return parameters
                .iter()
                .all(|name| entity.parameters.contains_key(*name));
        }
        if entity.parameters.contains_key(axis.parameter()) {
            return true;
        }
        if state.constraints.values().any(|constraint| {
            if constraint.state != ConstraintRuntimeState::Active || constraint.source != *entity_id
            {
                return false;
            }
            match constraint.kind {
                ConstraintKind::Fixed if constraint.source == constraint.target => true,
                ConstraintKind::CenterOf if center_uses_axis(constraint, axis) => {
                    coordinate_axis_is_positioned_inner(
                        state,
                        &constraint.target,
                        axis,
                        memo,
                        resolving,
                    )
                }
                _ => false,
            }
        }) {
            return true;
        }
        entity.kind == EntityKind::Point
            && state.constraints.values().any(|constraint| {
                if constraint.state != ConstraintRuntimeState::Active
                    || constraint.kind != ConstraintKind::Distance
                    || constraint.source == constraint.target
                    || constraint.parameters.get(PARAM_VALUE) != Some(&0.0)
                {
                    return false;
                }
                let other = if constraint.source == *entity_id {
                    Some(&constraint.target)
                } else if constraint.target == *entity_id {
                    Some(&constraint.source)
                } else {
                    None
                };
                other.is_some_and(|other_id| {
                    coordinate_axis_is_positioned_inner(state, other_id, axis, memo, resolving)
                })
            })
    });

    resolving.remove(&key);
    if positioned {
        memo.insert(key, true);
    }
    positioned
}

fn point_has_independent_position(state: &GraphState, entity_id: &EntityId) -> bool {
    [CoordinateAxis::X, CoordinateAxis::Y]
        .into_iter()
        .all(|axis| coordinate_axis_is_positioned(state, entity_id, axis))
}

fn validate_point_distance_degrees_of_freedom(state: &GraphState) -> Result<(), GeometryError> {
    let edges: Vec<_> = state
        .constraints
        .values()
        .filter(|constraint| {
            constraint.state == ConstraintRuntimeState::Active
                && constraint.kind == ConstraintKind::Distance
                && constraint.source != constraint.target
                && point_distance_supported(state, constraint)
        })
        .collect();
    let mut adjacency: BTreeMap<EntityId, Vec<usize>> = BTreeMap::new();
    for (index, constraint) in edges.iter().enumerate() {
        adjacency
            .entry(constraint.source.clone())
            .or_default()
            .push(index);
        adjacency
            .entry(constraint.target.clone())
            .or_default()
            .push(index);
    }

    for entity_id in adjacency.keys() {
        let entity = &state.entities[entity_id];
        let has_x = entity.parameters.contains_key("x");
        let has_y = entity.parameters.contains_key("y");
        if has_x != has_y && !point_has_independent_position(state, entity_id) {
            return Err(GeometryError::MissingParameter {
                entity_id: entity_id.clone(),
                parameter: if has_x { "y".into() } else { "x".into() },
            });
        }
    }

    let mut visited = BTreeSet::new();
    for start in adjacency.keys() {
        if visited.contains(start) {
            continue;
        }
        let mut stack = vec![start.clone()];
        let mut component_entities = BTreeSet::new();
        let mut component_edges = BTreeSet::new();
        while let Some(entity_id) = stack.pop() {
            if !visited.insert(entity_id.clone()) {
                continue;
            }
            component_entities.insert(entity_id.clone());
            for index in adjacency.get(&entity_id).into_iter().flatten() {
                component_edges.insert(*index);
                let constraint = edges[*index];
                let other = if constraint.source == entity_id {
                    &constraint.target
                } else {
                    &constraint.source
                };
                if !visited.contains(other) {
                    stack.push(other.clone());
                }
            }
        }

        let mut free_entities = Vec::new();
        let mut anchored_count = 0_u32;
        for entity_id in &component_entities {
            if point_has_independent_position(state, entity_id) {
                anchored_count += 1;
            } else {
                free_entities.push(entity_id.clone());
            }
        }
        if free_entities.is_empty() {
            continue;
        }

        let free_set: BTreeSet<_> = free_entities.iter().cloned().collect();
        let mut removed_dof = 0_u32;
        let mut has_nonzero_distance = false;
        for index in &component_edges {
            let constraint = edges[*index];
            if !free_set.contains(&constraint.source) && !free_set.contains(&constraint.target) {
                continue;
            }
            let value = constraint.parameters[PARAM_VALUE];
            if value == 0.0 {
                removed_dof += 2;
            } else {
                removed_dof += 1;
                has_nonzero_distance = true;
            }
        }
        let raw_remaining = (free_entities.len() as u32)
            .saturating_mul(2)
            .saturating_sub(removed_dof);
        let rigid_motion_floor = if anchored_count == 0 {
            if has_nonzero_distance {
                3
            } else {
                2
            }
        } else {
            0
        };
        let remaining_dof = raw_remaining.max(rigid_motion_floor);

        if remaining_dof > 0 {
            return Err(GeometryError::Underconstrained {
                entity_ids: free_entities,
                remaining_dof,
                reason: "point-distance constraints determine separation but do not uniquely determine every point position".into(),
            });
        }
        if has_nonzero_distance {
            let constraint = component_edges
                .iter()
                .map(|index| edges[*index])
                .find(|constraint| constraint.parameters[PARAM_VALUE] != 0.0)
                .ok_or_else(|| GeometryError::InvalidGraph {
                    message: "distance component lost its nonzero constraint".into(),
                })?;
            return Err(GeometryError::UnsupportedConstraint {
                constraint_id: constraint.id.clone(),
                kind: constraint.kind.clone(),
            });
        }
    }
    Ok(())
}

fn validate_distance_constraints(
    state: &GraphState,
    model: &GeometryModel,
) -> Result<(), GeometryError> {
    for constraint in state.constraints.values() {
        if constraint.state != ConstraintRuntimeState::Active
            || constraint.kind != ConstraintKind::Distance
        {
            continue;
        }
        if constraint.source == constraint.target || !point_distance_supported(state, constraint) {
            continue;
        }

        let expected = constraint
            .parameters
            .get(PARAM_VALUE)
            .copied()
            .ok_or_else(|| GeometryError::UnsupportedConstraint {
                constraint_id: constraint.id.clone(),
                kind: constraint.kind.clone(),
            })?;
        let source = point_position(model, &constraint.source)?;
        let target = point_position(model, &constraint.target)?;
        let actual = ((source.x - target.x).powi(2) + (source.y - target.y).powi(2)).sqrt();
        if (actual - expected).abs() > 1.0e-9 {
            return Err(GeometryError::ConflictingParameters {
                entity_id: constraint.source.clone(),
                reason: format!(
                    "constraint {} expects distance {}, but solved distance is {}",
                    constraint.id, expected, actual
                ),
            });
        }
    }
    Ok(())
}

fn point_position(model: &GeometryModel, entity_id: &EntityId) -> Result<Point2, GeometryError> {
    let entity = model
        .entities
        .get(entity_id)
        .ok_or_else(|| GeometryError::InvalidGraph {
            message: format!("geometry model is missing entity {}", entity_id),
        })?;
    match entity.primitive {
        GeometryPrimitive::Point { position } => Ok(position),
        _ => Err(GeometryError::InvalidGraph {
            message: format!("distance validation expected point entity {}", entity_id),
        }),
    }
}

fn line_direction_constraint_supported(
    state: &GraphState,
    constraint: &scc_core::runtime::RuntimeConstraint,
) -> bool {
    constraint.parameters.is_empty()
        && matches!(state.entities.get(&constraint.source), Some(entity) if entity.kind == EntityKind::Line)
        && matches!(state.entities.get(&constraint.target), Some(entity) if entity.kind == EntityKind::Line)
}

fn validate_align_constraints(
    state: &GraphState,
    model: &GeometryModel,
) -> Result<(), GeometryError> {
    for constraint in state.constraints.values() {
        if constraint.state != ConstraintRuntimeState::Active
            || constraint.kind != ConstraintKind::Align
        {
            continue;
        }

        let (source_start, source_end) = line_points(model, &constraint.source)?;
        let (target_start, target_end) = line_points(model, &constraint.target)?;
        let source_direction = line_unit_direction(&constraint.source, source_start, source_end)?;
        let target_direction = line_unit_direction(&constraint.target, target_start, target_end)?;
        let offset = Point2 {
            x: source_start.x - target_start.x,
            y: source_start.y - target_start.y,
        };
        let direction_error = cross(source_direction, target_direction).abs();
        let offset_error = cross(target_direction, offset).abs();
        if direction_error > CONSTRAINT_EPSILON || offset_error > CONSTRAINT_EPSILON {
            return Err(GeometryError::ConflictingParameters {
                entity_id: constraint.source.clone(),
                reason: format!(
                    "constraint {} expects lines {} and {} to be collinear",
                    constraint.id, constraint.source, constraint.target
                ),
            });
        }
    }
    Ok(())
}

fn validate_parallel_constraints(
    state: &GraphState,
    model: &GeometryModel,
) -> Result<(), GeometryError> {
    for constraint in state.constraints.values() {
        if constraint.state != ConstraintRuntimeState::Active
            || constraint.kind != ConstraintKind::Parallel
        {
            continue;
        }

        let (source_start, source_end) = line_points(model, &constraint.source)?;
        let (target_start, target_end) = line_points(model, &constraint.target)?;
        let source_direction = line_unit_direction(&constraint.source, source_start, source_end)?;
        let target_direction = line_unit_direction(&constraint.target, target_start, target_end)?;
        if cross(source_direction, target_direction).abs() > CONSTRAINT_EPSILON {
            return Err(GeometryError::ConflictingParameters {
                entity_id: constraint.source.clone(),
                reason: format!(
                    "constraint {} expects lines {} and {} to be parallel",
                    constraint.id, constraint.source, constraint.target
                ),
            });
        }
    }
    Ok(())
}

fn validate_perpendicular_constraints(
    state: &GraphState,
    model: &GeometryModel,
) -> Result<(), GeometryError> {
    for constraint in state.constraints.values() {
        if constraint.state != ConstraintRuntimeState::Active
            || constraint.kind != ConstraintKind::Perpendicular
        {
            continue;
        }

        let (source_start, source_end) = line_points(model, &constraint.source)?;
        let (target_start, target_end) = line_points(model, &constraint.target)?;
        let source_direction = line_unit_direction(&constraint.source, source_start, source_end)?;
        let target_direction = line_unit_direction(&constraint.target, target_start, target_end)?;
        if dot(source_direction, target_direction).abs() > CONSTRAINT_EPSILON {
            return Err(GeometryError::ConflictingParameters {
                entity_id: constraint.source.clone(),
                reason: format!(
                    "constraint {} expects lines {} and {} to be perpendicular",
                    constraint.id, constraint.source, constraint.target
                ),
            });
        }
    }
    Ok(())
}

fn line_points(
    model: &GeometryModel,
    entity_id: &EntityId,
) -> Result<(Point2, Point2), GeometryError> {
    let entity = model
        .entities
        .get(entity_id)
        .ok_or_else(|| GeometryError::InvalidGraph {
            message: format!("geometry model is missing entity {}", entity_id),
        })?;
    match &entity.primitive {
        GeometryPrimitive::Line { start, end } => Ok((*start, *end)),
        _ => Err(GeometryError::InvalidGraph {
            message: format!(
                "directional constraint validation expected line entity {}",
                entity_id
            ),
        }),
    }
}

fn line_unit_direction(
    entity_id: &EntityId,
    start: Point2,
    end: Point2,
) -> Result<Point2, GeometryError> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    if length <= CONSTRAINT_EPSILON {
        return Err(GeometryError::InvalidParameter {
            entity_id: entity_id.clone(),
            parameter: "line direction".into(),
            reason: "directional constraints require distinct line endpoints".into(),
        });
    }
    Ok(Point2 {
        x: dx / length,
        y: dy / length,
    })
}

fn cross(left: Point2, right: Point2) -> f64 {
    left.x * right.y - left.y * right.x
}

fn dot(left: Point2, right: Point2) -> f64 {
    left.x * right.x + left.y * right.y
}

fn fixed_anchor_supported(
    state: &GraphState,
    constraint: &scc_core::runtime::RuntimeConstraint,
) -> bool {
    if constraint.source != constraint.target {
        return false;
    }
    match constraint.anchor_role {
        None => true,
        Some(AnchorRole::FixedCenter) => center_positionable_source(state, &constraint.source),
        Some(AnchorRole::OriginPoint) => state
            .entities
            .get(&constraint.source)
            .is_some_and(|entity| entity.kind == EntityKind::Point),
        Some(AnchorRole::ReferenceEdge | AnchorRole::DatumLine) => false,
    }
}

fn datum_anchor_supported(constraint: &scc_core::runtime::RuntimeConstraint) -> bool {
    constraint.source == constraint.target
        && matches!(
            constraint.anchor_role,
            None | Some(AnchorRole::ReferenceEdge | AnchorRole::DatumLine)
        )
}

fn center_positionable_source(state: &GraphState, entity_id: &scc_core::ids::EntityId) -> bool {
    state.entities.get(entity_id).is_some_and(|entity| {
        matches!(
            entity.kind,
            EntityKind::Point | EntityKind::Circle | EntityKind::Arc | EntityKind::Rectangle
        )
    })
}

fn center_axis_selection_supported(constraint: &scc_core::runtime::RuntimeConstraint) -> bool {
    if constraint.parameters.is_empty() {
        return true;
    }
    constraint.parameters.len() <= 2
        && constraint.parameters.iter().all(|(name, value)| {
            matches!(name.as_str(), PARAM_AXIS_X | PARAM_AXIS_Y) && *value == 1.0
        })
}

fn center_uses_axis(
    constraint: &scc_core::runtime::RuntimeConstraint,
    axis: CoordinateAxis,
) -> bool {
    constraint.parameters.is_empty() || constraint.parameters.get(axis.marker()) == Some(&1.0)
}

fn center_is_partial(constraint: &scc_core::runtime::RuntimeConstraint) -> bool {
    center_axis_selection_supported(constraint)
        && !constraint.parameters.is_empty()
        && constraint.parameters.len() == 1
}

fn solve_entity(
    positions: &mut PositionResolver<'_>,
    entity: &RuntimeEntity,
) -> Result<GeometryPrimitive, GeometryError> {
    match entity.kind {
        EntityKind::Point => Ok(GeometryPrimitive::Point {
            position: positions.primitive_position(entity)?,
        }),
        EntityKind::Line => Ok(GeometryPrimitive::Line {
            start: Point2 {
                x: required_parameter(entity, "x1")?,
                y: required_parameter(entity, "y1")?,
            },
            end: Point2 {
                x: required_parameter(entity, "x2")?,
                y: required_parameter(entity, "y2")?,
            },
        }),
        EntityKind::Circle => Ok(GeometryPrimitive::Circle {
            center: positions.primitive_position(entity)?,
            radius: circle_radius(entity)?,
        }),
        EntityKind::Arc => {
            let start_angle = required_parameter(entity, "start_angle")?;
            let end_angle = required_parameter(entity, "end_angle")?;
            validate_arc_span(entity, start_angle, end_angle)?;
            Ok(GeometryPrimitive::Arc {
                center: positions.primitive_position(entity)?,
                radius: circle_radius(entity)?,
                start_angle,
                end_angle,
            })
        }
        EntityKind::Rectangle => Ok(GeometryPrimitive::Rectangle {
            origin: positions.primitive_position(entity)?,
            width: positive_parameter(entity, "width")?,
            height: positive_parameter(entity, "height")?,
        }),
        EntityKind::Polygon => solve_polygon(entity),
    }
}

fn solve_polygon(entity: &RuntimeEntity) -> Result<GeometryPrimitive, GeometryError> {
    let vertex_count = required_parameter(entity, "vertex_count")?;
    if vertex_count.fract() != 0.0
        || !(3.0..=f64::from(MAX_POLYGON_VERTICES)).contains(&vertex_count)
    {
        return Err(GeometryError::InvalidParameter {
            entity_id: entity.id.clone(),
            parameter: "vertex_count".into(),
            reason: format!(
                "value must be an integer from 3 through {}",
                MAX_POLYGON_VERTICES
            ),
        });
    }

    let mut vertices = Vec::with_capacity(vertex_count as usize);
    for index in 0..vertex_count as u32 {
        vertices.push(Point2 {
            x: required_parameter(entity, &format!("x_{}", index))?,
            y: required_parameter(entity, &format!("y_{}", index))?,
        });
    }
    validate_polygon_vertices(entity, &vertices)?;
    Ok(GeometryPrimitive::Polygon { vertices })
}

fn validate_polygon_vertices(
    entity: &RuntimeEntity,
    vertices: &[Point2],
) -> Result<(), GeometryError> {
    for (index, vertex) in vertices.iter().enumerate() {
        if vertices.iter().take(index).any(|candidate| {
            (candidate.x - vertex.x).abs() <= CONSTRAINT_EPSILON
                && (candidate.y - vertex.y).abs() <= CONSTRAINT_EPSILON
        }) {
            return Err(GeometryError::ConflictingParameters {
                entity_id: entity.id.clone(),
                reason: format!("polygon vertex {} duplicates an earlier vertex", index),
            });
        }
    }

    let signed_double_area: f64 = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(left, right)| left.x * right.y - right.x * left.y)
        .sum();
    if signed_double_area.abs() <= CONSTRAINT_EPSILON {
        return Err(GeometryError::ConflictingParameters {
            entity_id: entity.id.clone(),
            reason: "polygon vertices must enclose nonzero area".into(),
        });
    }
    Ok(())
}

fn validate_arc_span(
    entity: &RuntimeEntity,
    start_angle: f64,
    end_angle: f64,
) -> Result<(), GeometryError> {
    let span = (end_angle - start_angle).abs();
    if span <= CONSTRAINT_EPSILON || span >= std::f64::consts::TAU - CONSTRAINT_EPSILON {
        return Err(GeometryError::InvalidParameter {
            entity_id: entity.id.clone(),
            parameter: "start_angle/end_angle".into(),
            reason: "arc span must be greater than zero and less than one full turn in radians"
                .into(),
        });
    }
    Ok(())
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

fn circle_radius(entity: &RuntimeEntity) -> Result<f64, GeometryError> {
    let radius = optional_parameter(entity, "radius")?;
    let diameter = optional_parameter(entity, "diameter")?;
    match (radius, diameter) {
        (Some(radius), Some(diameter)) => {
            if (radius * 2.0 - diameter).abs() > 1.0e-9 {
                return Err(GeometryError::ConflictingParameters {
                    entity_id: entity.id.clone(),
                    reason: format!("radius {} does not match diameter {}", radius, diameter),
                });
            }
            validate_radius(entity, radius)
        }
        (Some(radius), None) => validate_radius(entity, radius),
        (None, Some(diameter)) => validate_radius(entity, diameter / 2.0),
        (None, None) => Err(GeometryError::MissingParameter {
            entity_id: entity.id.clone(),
            parameter: "radius or diameter".into(),
        }),
    }
}

fn validate_radius(entity: &RuntimeEntity, radius: f64) -> Result<f64, GeometryError> {
    if radius <= 0.0 {
        return Err(GeometryError::InvalidParameter {
            entity_id: entity.id.clone(),
            parameter: "radius".into(),
            reason: "value must be greater than zero".into(),
        });
    }
    Ok(radius)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use scc_core::authority::AuthorityLevel;
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::runtime::{RuntimeConstraint, RuntimeEntity};

    use super::*;

    fn fixed_circle_state() -> GraphState {
        let mut state = GraphState::new(7);
        state.entities.insert(
            EntityId("circle".into()),
            RuntimeEntity {
                id: EntityId("circle".into()),
                kind: EntityKind::Circle,
                parameters: BTreeMap::from([("diameter".into(), ParameterValue::Numeric(10.0))]),
            },
        );
        state.constraints.insert(
            ConstraintId("fixed".into()),
            RuntimeConstraint {
                id: ConstraintId("fixed".into()),
                kind: ConstraintKind::Fixed,
                source: EntityId("circle".into()),
                target: EntityId("circle".into()),
                authority: AuthorityLevel::L0,
                state: ConstraintRuntimeState::Active,
                anchor_role: None,
                parameters: BTreeMap::new(),
            },
        );
        state
    }

    #[test]
    fn solves_fixed_circle_at_origin() {
        let model = DeterministicGeometrySolver
            .solve(&fixed_circle_state())
            .unwrap();
        let circle = &model.entities[&EntityId("circle".into())];
        assert_eq!(
            circle.primitive,
            GeometryPrimitive::Circle {
                center: Point2 { x: 0.0, y: 0.0 },
                radius: 5.0,
            }
        );
    }

    #[test]
    fn solves_explicit_rectangle() {
        let mut state = GraphState::new(8);
        state.entities.insert(
            EntityId("plate".into()),
            RuntimeEntity {
                id: EntityId("plate".into()),
                kind: EntityKind::Rectangle,
                parameters: BTreeMap::from([
                    ("x".into(), ParameterValue::Numeric(-5.0)),
                    ("y".into(), ParameterValue::Numeric(-2.0)),
                    ("width".into(), ParameterValue::Numeric(10.0)),
                    ("height".into(), ParameterValue::Numeric(4.0)),
                ]),
            },
        );
        let model = DeterministicGeometrySolver.solve(&state).unwrap();
        assert_eq!(model.source_root, 8);
        assert_eq!(model.entities.len(), 1);
    }

    #[test]
    fn rejects_missing_circle_size() {
        let mut state = fixed_circle_state();
        state
            .entities
            .get_mut(&EntityId("circle".into()))
            .unwrap()
            .parameters
            .clear();
        let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
        assert_eq!(error.code(), "SCC-GEOM-002");
    }

    #[test]
    fn rejects_unsupported_active_constraint() {
        let mut state = fixed_circle_state();
        state.constraints.insert(
            ConstraintId("tangent".into()),
            RuntimeConstraint {
                id: ConstraintId("tangent".into()),
                kind: ConstraintKind::Tangent,
                source: EntityId("circle".into()),
                target: EntityId("circle".into()),
                authority: AuthorityLevel::L1,
                state: ConstraintRuntimeState::Active,
                anchor_role: None,
                parameters: BTreeMap::new(),
            },
        );
        let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
        assert_eq!(error.code(), "SCC-GEOM-005");
    }

    #[test]
    fn solving_is_deterministic_and_read_only() {
        let state = fixed_circle_state();
        let before = state.clone();
        let first = DeterministicGeometrySolver.solve(&state).unwrap();
        let second = DeterministicGeometrySolver.solve(&state).unwrap();
        assert_eq!(state, before);
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
    }
}
