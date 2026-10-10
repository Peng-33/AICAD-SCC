use std::collections::BTreeMap;

use scc_core::authority::AuthorityLevel;
use scc_core::constraint::{
    ConstraintKind, PARAM_ARRAY_CIRCULAR, PARAM_ARRAY_COLS, PARAM_ARRAY_COL_SPACING,
    PARAM_ARRAY_COUNT, PARAM_ARRAY_DIRECTION_X, PARAM_ARRAY_GRID, PARAM_ARRAY_LINEAR,
    PARAM_ARRAY_ROWS, PARAM_ARRAY_ROW_SPACING, PARAM_ARRAY_SPACING, PARAM_RADIUS,
};
use scc_core::entity::EntityKind;
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::runtime::{
    ConstraintRuntimeState, GraphState, ParameterValue, RuntimeConstraint, RuntimeEntity,
};
use scc_geometry::{DeterministicGeometrySolver, GeometryPrimitive, GeometrySolver, Point2};

fn array_state(parameters: BTreeMap<String, f64>) -> GraphState {
    let mut state = GraphState::new(22);
    state.entities.insert(
        EntityId("seed".into()),
        RuntimeEntity {
            id: EntityId("seed".into()),
            kind: EntityKind::Point,
            parameters: BTreeMap::from([
                ("x".into(), ParameterValue::Numeric(1.0)),
                ("y".into(), ParameterValue::Numeric(2.0)),
            ]),
        },
    );
    state.constraints.insert(
        ConstraintId("array".into()),
        RuntimeConstraint {
            id: ConstraintId("array".into()),
            kind: ConstraintKind::ArrayRelation,
            source: EntityId("seed".into()),
            target: EntityId("copies".into()),
            authority: AuthorityLevel::L2,
            state: ConstraintRuntimeState::Active,
            anchor_role: None,
            parameters,
        },
    );
    state.constraints.insert(
        ConstraintId("instance".into()),
        RuntimeConstraint {
            id: ConstraintId("instance".into()),
            kind: ConstraintKind::InstanceOf,
            source: EntityId("copies".into()),
            target: EntityId("seed".into()),
            authority: AuthorityLevel::L3,
            state: ConstraintRuntimeState::Active,
            anchor_role: None,
            parameters: BTreeMap::new(),
        },
    );
    state
}

fn point(model: &scc_geometry::GeometryModel, id: &str) -> Point2 {
    match model.entities[&EntityId(id.into())].primitive {
        GeometryPrimitive::Point { position } => position,
        ref primitive => panic!("expected point, got {primitive:?}"),
    }
}

#[test]
fn linear_array_replaces_template_with_stable_instances() {
    let state = array_state(BTreeMap::from([
        (PARAM_ARRAY_LINEAR.into(), 1.0),
        (PARAM_ARRAY_COUNT.into(), 3.0),
        (PARAM_ARRAY_SPACING.into(), 5.0),
        (PARAM_ARRAY_DIRECTION_X.into(), 1.0),
    ]));

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert!(!model.entities.contains_key(&EntityId("seed".into())));
    assert_eq!(model.entities.len(), 3);
    assert_eq!(point(&model, "copies[0]"), Point2 { x: 1.0, y: 2.0 });
    assert_eq!(point(&model, "copies[1]"), Point2 { x: 6.0, y: 2.0 });
    assert_eq!(point(&model, "copies[2]"), Point2 { x: 11.0, y: 2.0 });
}

#[test]
fn grid_array_uses_row_major_instance_ids() {
    let state = array_state(BTreeMap::from([
        (PARAM_ARRAY_GRID.into(), 1.0),
        (PARAM_ARRAY_ROWS.into(), 2.0),
        (PARAM_ARRAY_COLS.into(), 2.0),
        (PARAM_ARRAY_ROW_SPACING.into(), 7.0),
        (PARAM_ARRAY_COL_SPACING.into(), 5.0),
    ]));

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(point(&model, "copies[0]"), Point2 { x: 1.0, y: 2.0 });
    assert_eq!(point(&model, "copies[1]"), Point2 { x: 6.0, y: 2.0 });
    assert_eq!(point(&model, "copies[2]"), Point2 { x: 1.0, y: 9.0 });
    assert_eq!(point(&model, "copies[3]"), Point2 { x: 6.0, y: 9.0 });
}

#[test]
fn circular_array_coordinates_are_normalized() {
    let state = array_state(BTreeMap::from([
        (PARAM_ARRAY_CIRCULAR.into(), 1.0),
        (PARAM_ARRAY_COUNT.into(), 4.0),
        (PARAM_RADIUS.into(), 10.0),
    ]));

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(point(&model, "copies[0]"), Point2 { x: 11.0, y: 2.0 });
    assert_eq!(point(&model, "copies[1]"), Point2 { x: 1.0, y: 12.0 });
    assert_eq!(point(&model, "copies[2]"), Point2 { x: -9.0, y: 2.0 });
    assert_eq!(point(&model, "copies[3]"), Point2 { x: 1.0, y: -8.0 });
}

#[test]
fn unpaired_array_relation_is_rejected() {
    let mut state = array_state(BTreeMap::from([
        (PARAM_ARRAY_LINEAR.into(), 1.0),
        (PARAM_ARRAY_COUNT.into(), 2.0),
        (PARAM_ARRAY_SPACING.into(), 5.0),
        (PARAM_ARRAY_DIRECTION_X.into(), 1.0),
    ]));
    state.constraints.remove(&ConstraintId("instance".into()));

    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-005");
}
