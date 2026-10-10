use std::collections::BTreeMap;

use scc_core::authority::AuthorityLevel;
use scc_core::constraint::ConstraintKind;
use scc_core::entity::EntityKind;
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::runtime::{
    ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
};
use scc_geometry::{DeterministicGeometrySolver, GeometryPrimitive, GeometrySolver, Point2};

fn line(id: &str, start: Point2, end: Point2) -> RuntimeEntity {
    RuntimeEntity {
        id: EntityId(id.into()),
        kind: EntityKind::Line,
        parameters: BTreeMap::from([
            ("x1".into(), ParameterValue::Numeric(start.x)),
            ("y1".into(), ParameterValue::Numeric(start.y)),
            ("x2".into(), ParameterValue::Numeric(end.x)),
            ("y2".into(), ParameterValue::Numeric(end.y)),
        ]),
    }
}

fn parallel_constraint(source: &str, target: &str) -> RuntimeConstraint {
    RuntimeConstraint {
        id: ConstraintId("parallel".into()),
        kind: ConstraintKind::Parallel,
        source: EntityId(source.into()),
        target: EntityId(target.into()),
        authority: AuthorityLevel::L1,
        state: ConstraintRuntimeState::Active,
        anchor_role: None,
        parameters: BTreeMap::new(),
    }
}

fn parallel_state(
    reference_start: Point2,
    reference_end: Point2,
    source_start: Point2,
    source_end: Point2,
) -> GraphState {
    let mut state = GraphState::new(51);
    state.entities.insert(
        EntityId("reference".into()),
        line("reference", reference_start, reference_end),
    );
    state.entities.insert(
        EntityId("subject".into()),
        line("subject", source_start, source_end),
    );
    state.constraints.insert(
        ConstraintId("parallel".into()),
        parallel_constraint("subject", "reference"),
    );
    state
}

#[test]
fn offset_antiparallel_lines_solve() {
    let state = parallel_state(
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 10.0, y: 0.0 },
        Point2 { x: 8.0, y: 3.0 },
        Point2 { x: 2.0, y: 3.0 },
    );

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("subject".into())].primitive,
        GeometryPrimitive::Line {
            start: Point2 { x: 8.0, y: 3.0 },
            end: Point2 { x: 2.0, y: 3.0 },
        }
    );
    assert_eq!(model.solve_report.evaluated_constraints, 1);
    assert_eq!(model.solve_report.degrees_of_freedom.remaining, 0);
}

#[test]
fn offset_diagonal_parallel_lines_solve() {
    let state = parallel_state(
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 4.0, y: 4.0 },
        Point2 { x: 1.0, y: 3.0 },
        Point2 { x: 5.0, y: 7.0 },
    );

    DeterministicGeometrySolver.solve(&state).unwrap();
}

#[test]
fn nonparallel_lines_conflict() {
    let state = parallel_state(
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 10.0, y: 0.0 },
        Point2 { x: 2.0, y: 1.0 },
        Point2 { x: 8.0, y: 4.0 },
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}

#[test]
fn zero_length_parallel_line_is_invalid() {
    let state = parallel_state(
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 10.0, y: 0.0 },
        Point2 { x: 2.0, y: 1.0 },
        Point2 { x: 2.0, y: 1.0 },
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-003");
}

#[test]
fn missing_parallel_line_endpoint_is_reported() {
    let mut state = parallel_state(
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 10.0, y: 0.0 },
        Point2 { x: 2.0, y: 1.0 },
        Point2 { x: 8.0, y: 1.0 },
    );
    state
        .entities
        .get_mut(&EntityId("subject".into()))
        .unwrap()
        .parameters
        .remove("x2");

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-002");
}

#[test]
fn parallel_between_non_lines_is_unsupported() {
    let mut state = GraphState::new(52);
    for id in ["a", "b"] {
        state.entities.insert(
            EntityId(id.into()),
            RuntimeEntity {
                id: EntityId(id.into()),
                kind: EntityKind::Point,
                parameters: BTreeMap::from([
                    ("x".into(), ParameterValue::Numeric(0.0)),
                    ("y".into(), ParameterValue::Numeric(0.0)),
                ]),
            },
        );
    }
    state.constraints.insert(
        ConstraintId("parallel".into()),
        parallel_constraint("a", "b"),
    );

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}

#[test]
fn parallel_with_unknown_parameters_is_unsupported() {
    let mut state = parallel_state(
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 10.0, y: 0.0 },
        Point2 { x: 2.0, y: 1.0 },
        Point2 { x: 8.0, y: 1.0 },
    );
    state
        .constraints
        .get_mut(&ConstraintId("parallel".into()))
        .unwrap()
        .parameters
        .insert("axis_x".into(), 1.0);

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}
