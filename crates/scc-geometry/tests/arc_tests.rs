use std::collections::BTreeMap;

use scc_core::entity::EntityKind;
use scc_core::ids::EntityId;
use scc_core::runtime::{GraphState, ParameterValue, RuntimeEntity};
use scc_geometry::{DeterministicGeometrySolver, GeometryPrimitive, GeometrySolver};

fn arc_state(start_angle: f64, end_angle: f64) -> GraphState {
    let mut state = GraphState::new(23);
    state.entities.insert(
        EntityId("arc".into()),
        RuntimeEntity {
            id: EntityId("arc".into()),
            kind: EntityKind::Arc,
            parameters: BTreeMap::from([
                ("x".into(), ParameterValue::Numeric(0.0)),
                ("y".into(), ParameterValue::Numeric(0.0)),
                ("radius".into(), ParameterValue::Numeric(5.0)),
                ("start_angle".into(), ParameterValue::Numeric(start_angle)),
                ("end_angle".into(), ParameterValue::Numeric(end_angle)),
            ]),
        },
    );
    state
}

#[test]
fn solves_arc_with_radian_angles() {
    let model = DeterministicGeometrySolver
        .solve(&arc_state(0.0, std::f64::consts::FRAC_PI_2))
        .unwrap();
    assert!(matches!(
        model.entities[&EntityId("arc".into())].primitive,
        GeometryPrimitive::Arc { radius: 5.0, .. }
    ));
}

#[test]
fn rejects_zero_or_full_turn_arc_span() {
    for end_angle in [0.0, std::f64::consts::TAU] {
        let error = DeterministicGeometrySolver
            .solve(&arc_state(0.0, end_angle))
            .unwrap_err();
        assert_eq!(error.code(), "SCC-GEOM-003");
    }
}
