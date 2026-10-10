use std::collections::BTreeMap;

use scc_core::authority::AuthorityLevel;
use scc_core::constraint::{AnchorRole, ConstraintKind, PARAM_VALUE};
use scc_core::entity::EntityKind;
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::runtime::{
    ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
};
use scc_geometry::{DeterministicGeometrySolver, GeometryPrimitive, GeometrySolver, Point2};

fn entity(id: &str, kind: EntityKind, parameters: &[(&str, f64)]) -> RuntimeEntity {
    RuntimeEntity {
        id: EntityId(id.into()),
        kind,
        parameters: parameters
            .iter()
            .map(|(name, value)| ((*name).to_string(), ParameterValue::Numeric(*value)))
            .collect(),
    }
}

fn constraint(
    id: &str,
    kind: ConstraintKind,
    source: &str,
    target: &str,
    anchor_role: Option<AnchorRole>,
) -> RuntimeConstraint {
    RuntimeConstraint {
        id: ConstraintId(id.into()),
        kind,
        source: EntityId(source.into()),
        target: EntityId(target.into()),
        authority: AuthorityLevel::L0,
        state: ConstraintRuntimeState::Active,
        anchor_role,
        parameters: BTreeMap::new(),
    }
}

fn distance_constraint(id: &str, source: &str, target: &str, value: f64) -> RuntimeConstraint {
    let mut constraint = constraint(id, ConstraintKind::Distance, source, target, None);
    constraint.parameters.insert(PARAM_VALUE.into(), value);
    constraint
}

#[test]
fn fixed_center_rectangle_drives_centered_circle() {
    let mut state = GraphState::new(31);
    state.entities.insert(
        EntityId("plate".into()),
        entity(
            "plate",
            EntityKind::Rectangle,
            &[("width", 100.0), ("height", 60.0)],
        ),
    );
    state.entities.insert(
        EntityId("hole".into()),
        entity("hole", EntityKind::Circle, &[("diameter", 20.0)]),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "plate",
            "plate",
            Some(AnchorRole::FixedCenter),
        ),
    );
    state.constraints.insert(
        ConstraintId("center".into()),
        constraint("center", ConstraintKind::CenterOf, "hole", "plate", None),
    );

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("plate".into())].primitive,
        GeometryPrimitive::Rectangle {
            origin: Point2 { x: -50.0, y: -30.0 },
            width: 100.0,
            height: 60.0,
        }
    );
    assert_eq!(
        model.entities[&EntityId("hole".into())].primitive,
        GeometryPrimitive::Circle {
            center: Point2 { x: 0.0, y: 0.0 },
            radius: 10.0,
        }
    );
}

#[test]
fn explicit_position_conflicting_with_fixed_center_is_rejected() {
    let mut state = GraphState::new(32);
    state.entities.insert(
        EntityId("circle".into()),
        entity(
            "circle",
            EntityKind::Circle,
            &[("diameter", 10.0), ("x", 1.0), ("y", 0.0)],
        ),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "circle",
            "circle",
            Some(AnchorRole::FixedCenter),
        ),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}

#[test]
fn circular_center_dependencies_are_rejected() {
    let mut state = GraphState::new(33);
    state.entities.insert(
        EntityId("a".into()),
        entity("a", EntityKind::Circle, &[("diameter", 10.0)]),
    );
    state.entities.insert(
        EntityId("b".into()),
        entity("b", EntityKind::Circle, &[("diameter", 20.0)]),
    );
    state.constraints.insert(
        ConstraintId("a_center".into()),
        constraint("a_center", ConstraintKind::CenterOf, "a", "b", None),
    );
    state.constraints.insert(
        ConstraintId("b_center".into()),
        constraint("b_center", ConstraintKind::CenterOf, "b", "a", None),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-001");
}
#[test]
fn circular_partial_axis_dependencies_are_rejected() {
    let mut state = GraphState::new(34);
    state.entities.insert(
        EntityId("a".into()),
        entity("a", EntityKind::Point, &[("y", 0.0)]),
    );
    state.entities.insert(
        EntityId("b".into()),
        entity("b", EntityKind::Point, &[("y", 0.0)]),
    );
    let mut a_center = constraint("a_center", ConstraintKind::CenterOf, "a", "b", None);
    a_center
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_X.into(), 1.0);
    let mut b_center = constraint("b_center", ConstraintKind::CenterOf, "b", "a", None);
    b_center
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_X.into(), 1.0);
    state
        .constraints
        .insert(ConstraintId("a_center".into()), a_center);
    state
        .constraints
        .insert(ConstraintId("b_center".into()), b_center);

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-001");
}

#[test]
fn single_axis_center_reports_one_unresolved_coordinate() {
    let mut state = GraphState::new(34);
    state.entities.insert(
        EntityId("plate".into()),
        entity(
            "plate",
            EntityKind::Rectangle,
            &[("width", 100.0), ("height", 60.0)],
        ),
    );
    state.entities.insert(
        EntityId("hole".into()),
        entity("hole", EntityKind::Circle, &[("diameter", 20.0)]),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "plate",
            "plate",
            Some(AnchorRole::FixedCenter),
        ),
    );
    let mut centered = constraint("center", ConstraintKind::CenterOf, "hole", "plate", None);
    centered
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_X.into(), 1.0);
    state
        .constraints
        .insert(ConstraintId("center".into()), centered);

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-007");
    assert!(matches!(
        error,
        scc_geometry::GeometryError::Underconstrained {
            entity_ids,
            remaining_dof: 1,
            ..
        } if entity_ids == vec![EntityId("hole".into())]
    ));
}

#[test]
fn x_center_combines_with_explicit_y() {
    let mut state = GraphState::new(35);
    state.entities.insert(
        EntityId("plate".into()),
        entity(
            "plate",
            EntityKind::Rectangle,
            &[("width", 100.0), ("height", 60.0)],
        ),
    );
    state.entities.insert(
        EntityId("hole".into()),
        entity(
            "hole",
            EntityKind::Circle,
            &[("diameter", 20.0), ("y", 12.0)],
        ),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "plate",
            "plate",
            Some(AnchorRole::FixedCenter),
        ),
    );
    let mut centered = constraint("center_x", ConstraintKind::CenterOf, "hole", "plate", None);
    centered
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_X.into(), 1.0);
    state
        .constraints
        .insert(ConstraintId("center_x".into()), centered);

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("hole".into())].primitive,
        GeometryPrimitive::Circle {
            center: Point2 { x: 0.0, y: 12.0 },
            radius: 10.0,
        }
    );
}

#[test]
fn independent_x_and_y_center_constraints_combine() {
    let mut state = GraphState::new(36);
    state.entities.insert(
        EntityId("plate".into()),
        entity(
            "plate",
            EntityKind::Rectangle,
            &[("width", 100.0), ("height", 60.0)],
        ),
    );
    state.entities.insert(
        EntityId("hole".into()),
        entity("hole", EntityKind::Circle, &[("diameter", 20.0)]),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "plate",
            "plate",
            Some(AnchorRole::FixedCenter),
        ),
    );
    let mut center_x = constraint("center_x", ConstraintKind::CenterOf, "hole", "plate", None);
    center_x
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_X.into(), 1.0);
    let mut center_y = constraint("center_y", ConstraintKind::CenterOf, "hole", "plate", None);
    center_y
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_Y.into(), 1.0);
    state
        .constraints
        .insert(ConstraintId("center_x".into()), center_x);
    state
        .constraints
        .insert(ConstraintId("center_y".into()), center_y);

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("hole".into())].primitive,
        GeometryPrimitive::Circle {
            center: Point2 { x: 0.0, y: 0.0 },
            radius: 10.0,
        }
    );
}

#[test]
fn partial_center_detects_conflict_only_on_selected_axis() {
    let mut state = GraphState::new(37);
    state.entities.insert(
        EntityId("base".into()),
        entity("base", EntityKind::Point, &[("x", 0.0), ("y", 0.0)]),
    );
    state.entities.insert(
        EntityId("tip".into()),
        entity("tip", EntityKind::Point, &[("x", 2.0), ("y", 12.0)]),
    );
    let mut centered = constraint("center_x", ConstraintKind::CenterOf, "tip", "base", None);
    centered
        .parameters
        .insert(scc_core::constraint::PARAM_AXIS_X.into(), 1.0);
    state
        .constraints
        .insert(ConstraintId("center_x".into()), centered);

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}

#[test]
fn unknown_center_axis_remains_unsupported() {
    let mut state = GraphState::new(38);
    state.entities.insert(
        EntityId("base".into()),
        entity("base", EntityKind::Point, &[("x", 0.0), ("y", 0.0)]),
    );
    state.entities.insert(
        EntityId("tip".into()),
        entity("tip", EntityKind::Point, &[("x", 0.0), ("y", 0.0)]),
    );
    let mut centered = constraint("center_z", ConstraintKind::CenterOf, "tip", "base", None);
    centered.parameters.insert("unsupported_axis:z".into(), 1.0);
    state
        .constraints
        .insert(ConstraintId("center_z".into()), centered);

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}

#[test]
fn anchored_point_distance_reports_one_remaining_degree_of_freedom() {
    let mut state = GraphState::new(35);
    state.entities.insert(
        EntityId("base".into()),
        entity("base", EntityKind::Point, &[]),
    );
    state.entities.insert(
        EntityId("tip".into()),
        entity("tip", EntityKind::Point, &[]),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "base",
            "base",
            Some(AnchorRole::FixedCenter),
        ),
    );
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "tip", "base", 25.0),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-007");
    assert_eq!(
        error,
        scc_geometry::GeometryError::Underconstrained {
            entity_ids: vec![EntityId("tip".into())],
            remaining_dof: 1,
            reason: "point-distance constraints determine separation but do not uniquely determine every point position".into(),
        }
    );
}

#[test]
fn unanchored_point_pair_reports_three_rigid_motion_degrees() {
    let mut state = GraphState::new(36);
    state
        .entities
        .insert(EntityId("a".into()), entity("a", EntityKind::Point, &[]));
    state
        .entities
        .insert(EntityId("b".into()), entity("b", EntityKind::Point, &[]));
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "b", "a", 5.0),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert!(matches!(
        error,
        scc_geometry::GeometryError::Underconstrained {
            remaining_dof: 3,
            ..
        }
    ));
}

#[test]
fn zero_distance_can_propagate_an_anchored_position() {
    let mut state = GraphState::new(37);
    state.entities.insert(
        EntityId("base".into()),
        entity("base", EntityKind::Point, &[]),
    );
    state.entities.insert(
        EntityId("tip".into()),
        entity("tip", EntityKind::Point, &[]),
    );
    state.constraints.insert(
        ConstraintId("fixed".into()),
        constraint(
            "fixed",
            ConstraintKind::Fixed,
            "base",
            "base",
            Some(AnchorRole::FixedCenter),
        ),
    );
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "tip", "base", 0.0),
    );

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("tip".into())].primitive,
        GeometryPrimitive::Point {
            position: Point2 { x: 0.0, y: 0.0 },
        }
    );
}

#[test]
fn partial_point_coordinates_keep_missing_parameter_diagnostic() {
    let mut state = GraphState::new(38);
    state.entities.insert(
        EntityId("base".into()),
        entity("base", EntityKind::Point, &[("x", 0.0), ("y", 0.0)]),
    );
    state.entities.insert(
        EntityId("tip".into()),
        entity("tip", EntityKind::Point, &[("x", 5.0)]),
    );
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "tip", "base", 5.0),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-002");
}

#[test]
fn explicit_points_satisfying_distance_are_fully_constrained() {
    let mut state = GraphState::new(36);
    state.entities.insert(
        EntityId("a".into()),
        entity("a", EntityKind::Point, &[("x", 0.0), ("y", 0.0)]),
    );
    state.entities.insert(
        EntityId("b".into()),
        entity("b", EntityKind::Point, &[("x", 3.0), ("y", 4.0)]),
    );
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "b", "a", 5.0),
    );

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.solve_report.status,
        scc_geometry::SolveStatus::FullyConstrained
    );
    assert_eq!(model.solve_report.evaluated_constraints, 1);
    assert_eq!(model.solve_report.degrees_of_freedom.remaining, 0);
    assert!(model.solve_report.degrees_of_freedom.entities.is_empty());
}

#[test]
fn explicit_points_conflicting_with_distance_are_rejected() {
    let mut state = GraphState::new(36);
    state.entities.insert(
        EntityId("a".into()),
        entity("a", EntityKind::Point, &[("x", 0.0), ("y", 0.0)]),
    );
    state.entities.insert(
        EntityId("b".into()),
        entity("b", EntityKind::Point, &[("x", 5.0), ("y", 0.0)]),
    );
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "b", "a", 10.0),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}

#[test]
fn distance_between_non_points_remains_unsupported() {
    let mut state = GraphState::new(37);
    state.entities.insert(
        EntityId("a".into()),
        entity(
            "a",
            EntityKind::Circle,
            &[("diameter", 10.0), ("x", 0.0), ("y", 0.0)],
        ),
    );
    state.entities.insert(
        EntityId("b".into()),
        entity(
            "b",
            EntityKind::Circle,
            &[("diameter", 10.0), ("x", 5.0), ("y", 0.0)],
        ),
    );
    state.constraints.insert(
        ConstraintId("distance".into()),
        distance_constraint("distance", "b", "a", 5.0),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}
