use scc_cli::compile;
use scc_cli::execute;
use scc_cli::inspect;
use scc_core::schema_envelope::{deserialize_envelope_or_legacy, DocumentType};

const VALID_PROPOSAL: &str = r#"{
    "version": "1.0",
    "entities": [
        { "id": "rect_1", "kind": "rectangle" },
        { "id": "circle_1", "kind": "circle" }
    ],
    "relations": [
        { "kind": "centered_in", "subject": "circle_1", "reference": "rect_1", "axis": "both" }
    ],
    "dimensions": [
        { "target": "circle_1", "kind": "diameter", "value": 20 },
        { "target": "rect_1", "kind": "width", "value": 100 }
    ],
    "arrays": [],
    "anchors": [],
    "metadata": {}
}"#;

#[test]
fn envelope_ir_roundtrips_through_execute() {
    let compiled = compile::compile(VALID_PROPOSAL, true).unwrap();
    let ir_json = compiled.ir_json.unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&ir_json).unwrap();
    assert_eq!(parsed["schema_version"], "1.0");
    assert_eq!(parsed["document_type"], "constraint_ir");

    let result = execute::execute(&ir_json, None, true).unwrap();
    let state: serde_json::Value = serde_json::from_str(&result.state_json).unwrap();
    assert_eq!(state["schema_version"], "1.0");
    assert_eq!(state["document_type"], "graph_state");
    assert!(state["payload"]["entities"].is_object());
}

#[test]
fn execute_accepts_legacy_ir_after_compiling_with_envelope() {
    let compiled = compile::compile(VALID_PROPOSAL, true).unwrap();
    let envelope_json = compiled.ir_json.unwrap();

    // Read the payload back out and feed it as legacy JSON to execute.
    let ir: scc_core::constraint_ir::ConstraintIr =
        deserialize_envelope_or_legacy(DocumentType::ConstraintIr, &envelope_json).unwrap();
    let legacy_json = serde_json::to_string(&ir).unwrap();

    let result = execute::execute(&legacy_json, None, false).unwrap();
    let state: serde_json::Value = serde_json::from_str(&result.state_json).unwrap();
    assert!(state["entities"].is_object());
}

#[test]
fn inspect_accepts_envelope_ir() {
    let compiled = compile::compile(VALID_PROPOSAL, true).unwrap();
    let ir_json = compiled.ir_json.unwrap();

    let report = inspect::inspect(&ir_json).unwrap();
    assert_eq!(report.entity_count, 2);
    assert!(report.constraint_count > 0);
}

#[test]
fn inspect_accepts_legacy_ir() {
    let compiled = compile::compile(VALID_PROPOSAL, false).unwrap();
    let ir_json = compiled.ir_json.unwrap();

    let report = inspect::inspect(&ir_json).unwrap();
    assert_eq!(report.entity_count, 2);
}

#[test]
fn execute_rejects_envelope_with_wrong_document_type() {
    let envelope = serde_json::json!({
        "schema_version": "1.0",
        "document_type": "graph_state",
        "payload": { "entities": [], "constraints": [], "graph": { "nodes": [], "edges": [] }, "metadata": { "constraint_levels": { "l0": [], "l1": [], "l2": [], "l3": [] } } }
    });
    let result = execute::execute(&serde_json::to_string(&envelope).unwrap(), None, false);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("document_type"));
}

#[test]
fn execute_rejects_unsupported_schema_major_version() {
    let envelope = serde_json::json!({
        "schema_version": "2.0",
        "document_type": "constraint_ir",
        "payload": { "entities": [], "constraints": [], "graph": { "nodes": [], "edges": [] }, "metadata": { "constraint_levels": { "l0": [], "l1": [], "l2": [], "l3": [] } } }
    });
    let result = execute::execute(&serde_json::to_string(&envelope).unwrap(), None, false);
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("unsupported schema version"));
}

#[test]
fn execute_reports_missing_envelope_payload_with_stable_code() {
    let envelope = serde_json::json!({
        "schema_version": "1.0",
        "document_type": "constraint_ir"
    });
    let error =
        execute::execute(&serde_json::to_string(&envelope).unwrap(), None, false).unwrap_err();

    match error {
        scc_core::error::SccError::Structured(error) => {
            assert_eq!(error.code, "SCC-CLI-007");
            error.validate_contract().unwrap();
        }
        other => panic!("expected structured missing-payload error, got {other:?}"),
    }
}
