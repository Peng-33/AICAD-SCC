use scc_core::error::{SccError, StructuredError};
use scc_proposal::parser;

pub struct ValidateResult {
    pub ok: bool,
    pub diagnostics: Vec<StructuredError>,
}

/// Validate a proposal and return every static diagnostic.
///
/// The result is always `Ok` when the file can be read and parsed; validation
/// failures are represented as structured diagnostics inside `ValidateResult`.
pub fn validate(input: &str) -> Result<ValidateResult, SccError> {
    let proposal = parser::parse(input)?;
    let errors = scc_proposal::error::collect_validation_errors(&proposal);

    if errors.is_empty() {
        Ok(ValidateResult {
            ok: true,
            diagnostics: vec![],
        })
    } else {
        Ok(ValidateResult {
            ok: false,
            diagnostics: errors.into_iter().map(|e| e.into()).collect(),
        })
    }
}

/// Parse a proposal and return a single structured error if parsing fails.
pub fn parse_only(input: &str) -> Result<(), SccError> {
    parser::parse(input).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_PROPOSAL: &str = r#"{
        "version": "1.0",
        "entities": [{ "id": "a", "kind": "circle" }],
        "relations": [],
        "dimensions": [],
        "arrays": [],
        "anchors": [],
        "metadata": {}
    }"#;

    #[test]
    fn valid_proposal_has_no_diagnostics() {
        let result = validate(VALID_PROPOSAL).unwrap();
        assert!(result.ok);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn invalid_proposal_collects_diagnostics() {
        let input = r#"{
            "version": "",
            "entities": [
                { "id": "a", "kind": "circle" },
                { "id": "a", "kind": "circle" }
            ],
            "relations": [
                { "kind": "distance", "subject": "a", "reference": "a" }
            ],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let result = validate(input).unwrap();
        assert!(!result.ok);
        assert!(!result.diagnostics.is_empty());
        let codes: Vec<_> = result.diagnostics.iter().map(|e| e.code.as_str()).collect();
        assert!(codes.contains(&"SCC-PROP-004"));
        assert!(codes.contains(&"SCC-PROP-006"));
        assert!(codes.contains(&"SCC-PROP-007"));
        for diag in &result.diagnostics {
            diag.validate_contract().unwrap();
        }
    }

    #[test]
    fn parse_only_rejects_invalid_json() {
        assert!(parse_only("not json").is_err());
    }
}
