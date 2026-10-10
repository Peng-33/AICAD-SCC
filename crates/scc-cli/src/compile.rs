use scc_analysis::declaration_graph;
use scc_core::error::SccError;
use scc_core::schema_envelope::{serialize_envelope, serialize_legacy, DocumentType};
use scc_lowering::lowering;
use scc_proposal::{parser, validate};

#[derive(serde::Serialize)]
struct ConflictReport {
    status: String,
    conflicts: Vec<scc_core::conflict::Conflict>,
}

#[derive(Debug)]
pub struct CompileResult {
    pub ir_json: Option<String>,
    pub conflict_json: Option<String>,
}

fn map_envelope_err(e: scc_core::schema_envelope::EnvelopeError) -> SccError {
    SccError::Structured(Box::new(e.into()))
}

pub fn compile(input: &str, use_envelope: bool) -> Result<CompileResult, SccError> {
    let proposal = parser::parse(input)?;
    let validated = validate::validate(&proposal)?;
    let analysis = declaration_graph::analyze_detailed(&validated.0)?;

    if !analysis.conflicts.is_empty() {
        let report = ConflictReport {
            status: "conflict".into(),
            conflicts: analysis.conflicts,
        };
        let json = serde_json::to_string_pretty(&report)?;
        return Ok(CompileResult {
            ir_json: None,
            conflict_json: Some(json),
        });
    }

    let constraint_ir = lowering::try_lower(&analysis.semantic_ir)?;
    let ir_json = if use_envelope {
        serialize_envelope(DocumentType::ConstraintIr, &constraint_ir).map_err(map_envelope_err)?
    } else {
        serialize_legacy(&constraint_ir).map_err(map_envelope_err)?
    };

    Ok(CompileResult {
        ir_json: Some(ir_json),
        conflict_json: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_PROPOSAL: &str = r#"
    {
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
    }
    "#;

    const CONFLICT_PROPOSAL: &str = r#"
    {
        "version": "1.0",
        "entities": [
            { "id": "a", "kind": "circle" },
            { "id": "b", "kind": "circle" }
        ],
        "relations": [
            { "kind": "inside", "subject": "a", "reference": "b" },
            { "kind": "outside", "subject": "a", "reference": "b" }
        ],
        "dimensions": [],
        "arrays": [],
        "anchors": [],
        "metadata": {}
    }
    "#;

    #[test]
    fn compile_valid_proposal() {
        let result = compile(VALID_PROPOSAL, false).unwrap();
        assert!(result.ir_json.is_some());
        assert!(result.conflict_json.is_none());

        let ir: serde_json::Value = serde_json::from_str(result.ir_json.as_ref().unwrap()).unwrap();
        assert!(ir["entities"].is_array());
        assert!(ir["constraints"].is_array());
    }

    #[test]
    fn compile_v1_envelope() {
        let result = compile(VALID_PROPOSAL, true).unwrap();
        let ir: serde_json::Value = serde_json::from_str(result.ir_json.as_ref().unwrap()).unwrap();
        assert_eq!(ir["schema_version"], "1.0");
        assert_eq!(ir["document_type"], "constraint_ir");
        assert!(ir["payload"]["entities"].is_array());
    }

    #[test]
    fn compile_conflict_proposal() {
        let result = compile(CONFLICT_PROPOSAL, false).unwrap();
        assert!(result.ir_json.is_none());
        assert!(result.conflict_json.is_some());

        let conflict: serde_json::Value =
            serde_json::from_str(result.conflict_json.as_ref().unwrap()).unwrap();
        assert_eq!(conflict["status"], "conflict");
    }

    #[test]
    fn conflict_report_remains_legacy_when_envelope_output_is_requested() {
        let result = compile(CONFLICT_PROPOSAL, true).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(result.conflict_json.as_deref().unwrap()).unwrap();
        assert_eq!(value["status"], "conflict");
        assert!(value.get("schema_version").is_none());
        assert!(value.get("document_type").is_none());
    }

    #[test]
    fn compile_with_envelope_wraps_ir() {
        let result = compile(VALID_PROPOSAL, true).unwrap();
        let json = result.ir_json.unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], "1.0");
        assert_eq!(value["document_type"], "constraint_ir");
        assert!(value["payload"]["entities"].is_array());
    }

    #[test]
    fn compile_without_envelope_is_legacy_unversioned() {
        let result = compile(VALID_PROPOSAL, false).unwrap();
        let json = result.ir_json.unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("schema_version").is_none());
        assert!(value.get("document_type").is_none());
        assert!(value["entities"].is_array());
    }
}
