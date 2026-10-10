use scc_core::constraint_ir::ConstraintIr;
use scc_core::error::SccError;
use scc_core::schema_envelope::{
    deserialize_envelope_or_legacy, serialize_envelope, serialize_legacy, DocumentType,
};
use scc_execution::driver;
use scc_execution::overlay_view::MutationOp;

#[derive(Debug)]
pub struct ExecuteResult {
    pub trace_json: String,
    pub state_json: String,
}

fn map_envelope_err(e: scc_core::schema_envelope::EnvelopeError) -> SccError {
    SccError::Structured(Box::new(e.into()))
}

pub fn execute(
    ir_json: &str,
    patch: Option<Vec<MutationOp>>,
    use_envelope: bool,
) -> Result<ExecuteResult, SccError> {
    let ir: ConstraintIr = deserialize_envelope_or_legacy(DocumentType::ConstraintIr, ir_json)
        .map_err(map_envelope_err)?;

    let patch = patch.unwrap_or_default();
    let output = driver::execute(&ir, &patch)?;
    let trace_json = if use_envelope {
        serialize_envelope(DocumentType::ExecutionTrace, &output.trace).map_err(map_envelope_err)?
    } else {
        serialize_legacy(&output.trace).map_err(map_envelope_err)?
    };
    let state_json = if use_envelope {
        serialize_envelope(DocumentType::GraphState, &output.final_state)
            .map_err(map_envelope_err)?
    } else {
        serialize_legacy(&output.final_state).map_err(map_envelope_err)?
    };

    Ok(ExecuteResult {
        trace_json,
        state_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::constraint_ir::{IrConstraintLevels, IrMetadata};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;
    use scc_core::ids::EntityId;

    fn minimal_ir_json() -> String {
        serde_json::to_string(&ConstraintIr {
            entities: vec![Entity {
                id: EntityId("hole".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        })
        .unwrap()
    }

    #[test]
    fn execute_reads_legacy_ir() {
        let result = execute(&minimal_ir_json(), None, false).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&result.state_json).is_ok());
    }

    #[test]
    fn execute_reads_envelope_ir() {
        let ir = ConstraintIr {
            entities: vec![Entity {
                id: EntityId("hole".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        };
        let envelope = serialize_envelope(DocumentType::ConstraintIr, &ir).unwrap();
        let result = execute(&envelope, None, false).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&result.state_json).is_ok());
    }

    #[test]
    fn execute_with_envelope_wraps_outputs() {
        let result = execute(&minimal_ir_json(), None, true).unwrap();
        let trace: serde_json::Value = serde_json::from_str(&result.trace_json).unwrap();
        let state: serde_json::Value = serde_json::from_str(&result.state_json).unwrap();
        assert_eq!(trace["schema_version"], "1.0");
        assert_eq!(trace["document_type"], "execution_trace");
        assert_eq!(state["schema_version"], "1.0");
        assert_eq!(state["document_type"], "graph_state");
    }
}
