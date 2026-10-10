use std::collections::BTreeMap;
use std::time::Instant;

use scc_core::authority::AuthorityLevel;
use scc_core::constraint::{
    ConstraintKind, PARAM_ARRAY_COUNT, PARAM_ARRAY_DIRECTION_X, PARAM_ARRAY_LINEAR,
    PARAM_ARRAY_SPACING,
};
use scc_core::entity::EntityKind;
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::runtime::{
    ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
};
use scc_geometry::{ConvergenceStatus, DeterministicGeometrySolver, GeometrySolver, SolveStatus};

const BASELINE_ENTITY_COUNT: u32 = 10_000;

#[test]
#[ignore = "manual release-mode performance baseline"]
fn explicit_point_solve_baseline() {
    let mut state = GraphState::new(100);
    for index in 0..BASELINE_ENTITY_COUNT {
        let id = EntityId(format!("point_{index:05}"));
        state.entities.insert(
            id.clone(),
            RuntimeEntity {
                id,
                kind: EntityKind::Point,
                parameters: BTreeMap::from([
                    ("x".into(), ParameterValue::Numeric(f64::from(index))),
                    ("y".into(), ParameterValue::Numeric(f64::from(index % 100))),
                ]),
            },
        );
    }

    let started = Instant::now();
    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(model.entities.len(), BASELINE_ENTITY_COUNT as usize);
    assert_eq!(model.solve_report.status, SolveStatus::FullyConstrained);
    assert_eq!(model.solve_report.convergence, ConvergenceStatus::Exact);
    assert_eq!(model.solve_report.degrees_of_freedom.remaining, 0);
    eprintln!(
        "M3_BASELINE explicit_points={} elapsed_us={}",
        BASELINE_ENTITY_COUNT,
        elapsed.as_micros()
    );
}

#[test]
#[ignore = "manual release-mode performance baseline"]
fn linear_array_expansion_baseline() {
    let mut state = GraphState::new(101);
    state.entities.insert(
        EntityId("seed".into()),
        RuntimeEntity {
            id: EntityId("seed".into()),
            kind: EntityKind::Point,
            parameters: BTreeMap::from([
                ("x".into(), ParameterValue::Numeric(0.0)),
                ("y".into(), ParameterValue::Numeric(0.0)),
            ]),
        },
    );
    state.constraints.insert(
        ConstraintId("array".into()),
        RuntimeConstraint {
            id: ConstraintId("array".into()),
            kind: ConstraintKind::ArrayRelation,
            source: EntityId("seed".into()),
            target: EntityId("points".into()),
            authority: AuthorityLevel::L2,
            state: ConstraintRuntimeState::Active,
            anchor_role: None,
            parameters: BTreeMap::from([
                (PARAM_ARRAY_LINEAR.into(), 1.0),
                (PARAM_ARRAY_COUNT.into(), f64::from(BASELINE_ENTITY_COUNT)),
                (PARAM_ARRAY_SPACING.into(), 1.0),
                (PARAM_ARRAY_DIRECTION_X.into(), 1.0),
            ]),
        },
    );
    state.constraints.insert(
        ConstraintId("instance".into()),
        RuntimeConstraint {
            id: ConstraintId("instance".into()),
            kind: ConstraintKind::InstanceOf,
            source: EntityId("points".into()),
            target: EntityId("seed".into()),
            authority: AuthorityLevel::L3,
            state: ConstraintRuntimeState::Active,
            anchor_role: None,
            parameters: BTreeMap::new(),
        },
    );

    let started = Instant::now();
    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(model.entities.len(), BASELINE_ENTITY_COUNT as usize);
    assert_eq!(model.solve_report.status, SolveStatus::FullyConstrained);
    assert_eq!(model.solve_report.convergence, ConvergenceStatus::Exact);
    assert_eq!(model.solve_report.evaluated_constraints, 2);
    assert_eq!(model.solve_report.degrees_of_freedom.remaining, 0);
    eprintln!(
        "M3_BASELINE linear_array_instances={} elapsed_us={}",
        BASELINE_ENTITY_COUNT,
        elapsed.as_micros()
    );
}
