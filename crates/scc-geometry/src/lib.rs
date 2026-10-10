pub mod dxf;
pub mod error;
pub mod model;
mod resolution;
pub mod solver;
pub mod svg;

use scc_core::error::SccError;
use scc_core::runtime::GraphState;
use scc_core::schema_envelope::{
    deserialize_envelope_or_legacy, serialize_envelope, serialize_legacy, DocumentType,
    SCHEMA_VERSION_1_0,
};

pub use error::GeometryError;
pub use model::{
    ConvergenceStatus, DegreesOfFreedomReport, GeometryEntity, GeometryModel, GeometryPrimitive,
    GeometrySolveReport, Point2, SolveMethod, SolveStatus,
};
pub use solver::{DeterministicGeometrySolver, GeometrySolver};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Dxf,
}

/// Render a solved GeometryModel as deterministic SVG text.
pub fn export_svg(model: &GeometryModel) -> Result<String, GeometryError> {
    validate_export_model(model)?;
    svg::render_svg(model)
}

/// Render a solved GeometryModel as deterministic ASCII DXF (AC1027).
pub fn export_dxf(model: &GeometryModel) -> Result<String, GeometryError> {
    validate_export_model(model)?;
    dxf::render_dxf(model)
}

fn validate_export_model(model: &GeometryModel) -> Result<(), GeometryError> {
    let finite_point = |point: &Point2| point.x.is_finite() && point.y.is_finite();
    for (key, entity) in &model.entities {
        if key != &entity.id {
            return Err(GeometryError::InvalidGraph {
                message: format!(
                    "geometry entity map key {} does not match entity id {}",
                    key, entity.id
                ),
            });
        }
        let valid = match &entity.primitive {
            GeometryPrimitive::Point { position } => finite_point(position),
            GeometryPrimitive::Line { start, end } => finite_point(start) && finite_point(end),
            GeometryPrimitive::Circle { center, radius } => {
                finite_point(center) && radius.is_finite() && *radius > 0.0
            }
            GeometryPrimitive::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                let span = (*end_angle - *start_angle).abs();
                finite_point(center)
                    && radius.is_finite()
                    && *radius > 0.0
                    && start_angle.is_finite()
                    && end_angle.is_finite()
                    && span > 1.0e-9
                    && span < std::f64::consts::TAU - 1.0e-9
            }
            GeometryPrimitive::Rectangle {
                origin,
                width,
                height,
            } => {
                finite_point(origin)
                    && width.is_finite()
                    && *width > 0.0
                    && height.is_finite()
                    && *height > 0.0
            }
            GeometryPrimitive::Polygon { vertices } => {
                vertices.len() >= 3 && vertices.iter().all(finite_point)
            }
        };
        if !valid {
            return Err(GeometryError::InvalidGraph {
                message: format!("entity {} is not valid for CAD export", entity.id),
            });
        }
    }
    Ok(())
}

/// Export a legacy or schema-1.x GeometryModel JSON document.
pub fn export_geometry_model_json(input: &str, format: ExportFormat) -> Result<String, SccError> {
    let model: GeometryModel = deserialize_envelope_or_legacy(DocumentType::GeometryModel, input)
        .map_err(map_envelope_error)?;
    match format {
        ExportFormat::Svg => export_svg(&model),
        ExportFormat::Dxf => export_dxf(&model),
    }
    .map_err(Into::into)
}

/// GraphState schema accepted by the M3 geometry boundary.
pub const SUPPORTED_GRAPH_STATE_SCHEMA: &str = SCHEMA_VERSION_1_0;

fn map_envelope_error(error: scc_core::schema_envelope::EnvelopeError) -> SccError {
    SccError::Structured(Box::new(error.into()))
}

/// Solve a legacy or schema-1.x GraphState JSON document into GeometryModel JSON.
///
/// The input state is deserialized and consumed read-only. Output can remain
/// legacy JSON or use the schema envelope with `document_type=geometry_model`.
pub fn solve_graph_state_json(input: &str, use_envelope: bool) -> Result<String, SccError> {
    let state: GraphState = deserialize_envelope_or_legacy(DocumentType::GraphState, input)
        .map_err(map_envelope_error)?;
    let model = DeterministicGeometrySolver.solve(&state)?;
    if use_envelope {
        serialize_envelope(DocumentType::GeometryModel, &model).map_err(map_envelope_error)
    } else {
        serialize_legacy(&model).map_err(map_envelope_error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use scc_core::authority::AuthorityLevel;
    use scc_core::constraint::ConstraintKind;
    use scc_core::entity::EntityKind;
    use scc_core::ids::{ConstraintId, EntityId};
    use scc_core::runtime::{
        ConstraintRuntimeState, ParameterValue, RuntimeConstraint, RuntimeEntity,
    };

    use super::*;

    fn solvable_state() -> GraphState {
        let mut state = GraphState::new(9);
        state.entities.insert(
            EntityId("origin".into()),
            RuntimeEntity {
                id: EntityId("origin".into()),
                kind: EntityKind::Point,
                parameters: BTreeMap::new(),
            },
        );
        state.constraints.insert(
            ConstraintId("fixed".into()),
            RuntimeConstraint {
                id: ConstraintId("fixed".into()),
                kind: ConstraintKind::Fixed,
                source: EntityId("origin".into()),
                target: EntityId("origin".into()),
                authority: AuthorityLevel::L0,
                state: ConstraintRuntimeState::Active,
                anchor_role: None,
                parameters: BTreeMap::new(),
            },
        );
        state
    }

    #[test]
    fn solves_legacy_graph_state_json() {
        let input = serde_json::to_string(&solvable_state()).unwrap();
        let output = solve_graph_state_json(&input, false).unwrap();
        let model: GeometryModel = serde_json::from_str(&output).unwrap();
        assert_eq!(model.entities.len(), 1);
    }

    #[test]
    fn writes_geometry_model_envelope() {
        let input = serialize_envelope(DocumentType::GraphState, &solvable_state()).unwrap();
        let output = solve_graph_state_json(&input, true).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["schema_version"], SUPPORTED_GRAPH_STATE_SCHEMA);
        assert_eq!(value["document_type"], "geometry_model");
        assert_eq!(
            value["payload"]["solve_report"]["status"],
            "fully_constrained"
        );
        assert_eq!(value["payload"]["solve_report"]["convergence"], "exact");
        assert_eq!(
            value["payload"]["solve_report"]["degrees_of_freedom"]["remaining"],
            0
        );
    }

    #[test]
    fn runtime_parameter_shape_remains_numeric() {
        let value = ParameterValue::Numeric(4.0);
        assert_eq!(serde_json::to_value(value).unwrap()["Numeric"], 4.0);
    }

    #[test]
    fn exports_enveloped_geometry_model() {
        let model = DeterministicGeometrySolver
            .solve(&solvable_state())
            .unwrap();
        let input = serialize_envelope(DocumentType::GeometryModel, &model).unwrap();
        let svg = export_geometry_model_json(&input, ExportFormat::Svg).unwrap();
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("id=\"origin\""));
    }

    #[test]
    fn rejects_invalid_geometry_model_before_export() {
        let mut model = GeometryModel::new(1, 0, 0);
        model.entities.insert(
            EntityId("bad".into()),
            GeometryEntity {
                id: EntityId("bad".into()),
                primitive: GeometryPrimitive::Circle {
                    center: Point2 { x: 0.0, y: 0.0 },
                    radius: 0.0,
                },
            },
        );
        let error = export_svg(&model).unwrap_err();
        assert_eq!(error.code(), "SCC-GEOM-001");
    }
}
