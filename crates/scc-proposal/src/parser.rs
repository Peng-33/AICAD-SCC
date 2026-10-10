use crate::error::ProposalValidationError;
use crate::schema::Proposal;
use scc_core::error::SccError;

fn classify_serde_error(error: serde_json::Error) -> ProposalValidationError {
    let msg = error.to_string();
    if let Some(start) = msg.find("missing field `") {
        let rest = &msg[start + "missing field `".len()..];
        if let Some(end) = rest.find('`') {
            let field = &rest[..end];
            return ProposalValidationError::MissingRequiredField {
                path: format!("/{}", field),
            };
        }
    }
    ProposalValidationError::InvalidJson {
        message: msg,
        path: None,
    }
}

/// Parse a proposal with a structured error on failure.
pub fn parse_detailed(input: &str) -> Result<Proposal, ProposalValidationError> {
    serde_json::from_str(input).map_err(classify_serde_error)
}

/// Legacy-compatible parser wrapper.
pub fn parse(input: &str) -> Result<Proposal, SccError> {
    parse_detailed(input).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_proposal_json() -> &'static str {
        r#"{
            "version": "1.0",
            "entities": [{"id": "e1", "kind": "circle"}],
            "relations": [{"kind": "distance", "subject": "e1", "reference": "e1", "value": 10.0}],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#
    }

    #[test]
    fn parses_valid_proposal() {
        let p = parse(valid_proposal_json()).unwrap();
        assert_eq!(p.version, "1.0");
        assert_eq!(p.entities.len(), 1);
        assert_eq!(p.entities[0].id, "e1");
        assert_eq!(p.entities[0].kind, "circle");
    }

    #[test]
    fn invalid_json_fails() {
        let result = parse("not json");
        assert!(result.is_err());
    }

    #[test]
    fn missing_required_field_fails() {
        let json = r#"{
            "entities": [],
            "relations": [],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let error = parse(json).expect_err("missing version field must fail");
        match error {
            SccError::Structured(error) => {
                assert_eq!(error.code, "SCC-PROP-002");
                assert_eq!(error.path, Some("/version".into()));
                error.validate_contract().unwrap();
            }
            other => panic!("expected structured parse error, got {other:?}"),
        }
    }

    #[test]
    fn missing_required_field_maps_to_prop_002() {
        let json = r#"{
            "entities": [],
            "relations": [],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let err = parse_detailed(json).unwrap_err();
        assert_eq!(err.code(), "SCC-PROP-002");
        let structured: scc_core::error::StructuredError = err.into();
        assert_eq!(structured.code, "SCC-PROP-002");
        assert_eq!(structured.path, Some("/version".into()));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn unknown_entity_kind_parsed_as_opaque_string() {
        let json = r#"{
            "version": "1.0",
            "entities": [{"id": "e1", "kind": "hypercube"}],
            "relations": [],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let p = parse(json).unwrap();
        assert_eq!(p.entities[0].kind, "hypercube");
    }

    #[test]
    fn unknown_relation_kind_parsed_as_opaque_string() {
        let json = r#"{
            "version": "1.0",
            "entities": [{"id": "e1", "kind": "circle"}],
            "relations": [{"kind": "entangled_with", "subject": "e1", "reference": "e1"}],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let p = parse(json).unwrap();
        assert_eq!(p.relations[0].kind, "entangled_with");
    }

    #[test]
    fn invalid_array_schema_fails() {
        let json = r#"{
            "version": "1.0",
            "entities": [],
            "relations": [],
            "dimensions": [],
            "arrays": [{"id": "arr1", "kind": "linear", "spacing": 10.0, "element": "e1"}],
            "anchors": [],
            "metadata": {}
        }"#;
        let result = parse(json);
        assert!(result.is_err(), "missing count field must fail");
    }

    #[test]
    fn invalid_anchor_schema_fails() {
        let json = r#"{
            "version": "1.0",
            "entities": [],
            "relations": [],
            "dimensions": [],
            "arrays": [],
            "anchors": [{"id": "origin"}],
            "metadata": {}
        }"#;
        let result = parse(json);
        assert!(result.is_err(), "missing anchor kind must fail");
    }

    #[test]
    fn backward_compatibility_optional_fields_default() {
        let json = r#"{
            "version": "0.9",
            "entities": [{"id": "e1", "kind": "line"}],
            "relations": [{"kind": "parallel_to", "subject": "e1", "reference": "e1"}],
            "dimensions": [{"target": "e1", "kind": "length", "value": 5.0}],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let p = parse(json).unwrap();
        assert_eq!(p.version, "0.9");
        assert!(p.relations[0].axis.is_none());
        assert!(p.relations[0].value.is_none());
    }

    #[test]
    fn empty_proposal_parses() {
        let json = r#"{
            "version": "1.0",
            "entities": [],
            "relations": [],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": null
        }"#;
        let p = parse(json).unwrap();
        assert!(p.entities.is_empty());
    }
}
