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

fn align_constraint(source: &str, target: &str) -> RuntimeConstraint {
    RuntimeConstraint {
        id: ConstraintId("align".into()),
        kind: ConstraintKind::Align,
        source: EntityId(source.into()),
        target: EntityId(target.into()),
        authority: AuthorityLevel::L1,
        state: ConstraintRuntimeState::Active,
        anchor_role: None,
        parameters: BTreeMap::new(),
    }
}

fn aligned_state(source_start: Point2, source_end: Point2) -> GraphState {
    let mut state = GraphState::new(41);
    state.entities.insert(
        EntityId("reference".into()),
        line(
            "reference",
            Point2 { x: 0.0, y: 0.0 },
            Point2 { x: 10.0, y: 0.0 },
        ),
    );
    state.entities.insert(
        EntityId("subject".into()),
        line("subject", source_start, source_end),
    );
    state.constraints.insert(
        ConstraintId("align".into()),
        align_constraint("subject", "reference"),
    );
    state
}

#[test]
fn collinear_lines_solve_in_either_direction() {
    let state = aligned_state(Point2 { x: 8.0, y: 0.0 }, Point2 { x: 2.0, y: 0.0 });

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("subject".into())].primitive,
        GeometryPrimitive::Line {
            start: Point2 { x: 8.0, y: 0.0 },
            end: Point2 { x: 2.0, y: 0.0 },
        }
    );
    assert_eq!(model.solve_report.degrees_of_freedom.remaining, 0);
}

#[test]
fn parallel_but_offset_lines_are_not_aligned() {
    let state = aligned_state(Point2 { x: 2.0, y: 1.0 }, Point2 { x: 8.0, y: 1.0 });

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}

#[test]
fn nonparallel_lines_are_not_aligned() {
    let state = aligned_state(Point2 { x: 2.0, y: 0.0 }, Point2 { x: 8.0, y: 4.0 });

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}

#[test]
fn zero_length_aligned_line_is_invalid() {
    let state = aligned_state(Point2 { x: 2.0, y: 0.0 }, Point2 { x: 2.0, y: 0.0 });

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-003");
}

#[test]
fn align_between_non_lines_is_unsupported() {
    let mut state = GraphState::new(42);
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
    state
        .constraints
        .insert(ConstraintId("align".into()), align_constraint("a", "b"));

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}

#[test]
fn align_with_unknown_parameters_is_unsupported() {
    let mut state = aligned_state(Point2 { x: 2.0, y: 0.0 }, Point2 { x: 8.0, y: 0.0 });
    state
        .constraints
        .get_mut(&ConstraintId("align".into()))
        .unwrap()
        .parameters
        .insert("axis_x".into(), 1.0);

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}
