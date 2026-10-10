use std::collections::BTreeMap;

use scc_core::entity::EntityKind;
use scc_core::ids::EntityId;
use scc_core::runtime::{GraphState, ParameterValue, RuntimeEntity};
use scc_geometry::{DeterministicGeometrySolver, GeometryPrimitive, GeometrySolver, Point2};

fn polygon_state(parameters: impl IntoIterator<Item = (&'static str, f64)>) -> GraphState {
    let mut state = GraphState::new(21);
    state.entities.insert(
        EntityId("profile".into()),
        RuntimeEntity {
            id: EntityId("profile".into()),
            kind: EntityKind::Polygon,
            parameters: parameters
                .into_iter()
                .map(|(name, value)| (name.into(), ParameterValue::Numeric(value)))
                .collect::<BTreeMap<_, _>>(),
        },
    );
    state
}

#[test]
fn solves_explicit_polygon_vertices_in_declared_order() {
    let state = polygon_state([
        ("vertex_count", 3.0),
        ("x_0", 0.0),
        ("y_0", 0.0),
        ("x_1", 4.0),
        ("y_1", 0.0),
        ("x_2", 0.0),
        ("y_2", 3.0),
    ]);

    let model = DeterministicGeometrySolver.solve(&state).unwrap();
    assert_eq!(
        model.entities[&EntityId("profile".into())].primitive,
        GeometryPrimitive::Polygon {
            vertices: vec![
                Point2 { x: 0.0, y: 0.0 },
                Point2 { x: 4.0, y: 0.0 },
                Point2 { x: 0.0, y: 3.0 },
            ],
        }
    );
}

#[test]
fn rejects_fractional_polygon_vertex_count() {
    let state = polygon_state([("vertex_count", 3.5)]);
    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-003");
}

#[test]
fn rejects_polygon_with_zero_area() {
    let state = polygon_state([
        ("vertex_count", 3.0),
        ("x_0", 0.0),
        ("y_0", 0.0),
        ("x_1", 1.0),
        ("y_1", 1.0),
        ("x_2", 2.0),
        ("y_2", 2.0),
    ]);
    let error = DeterministicGeometrySolver.solve(&state).unwrap_err();
    assert_eq!(error.code(), "SCC-GEOM-006");
}
