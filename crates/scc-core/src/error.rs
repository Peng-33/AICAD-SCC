use std::collections::BTreeMap;

use crate::ids::{ConstraintId, EntityId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stage at which a structured error originated.
///
/// Stages follow the accepted ADR-002 naming convention and are serialized as
/// lowercase strings for stable JSON output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ErrorStage {
    Proposal,
    Analysis,
    Lowering,
    Execution,
    Geometry,
    Cli,
    /// Engineering-layer command validation (AICAD Phase 1 `scc-engineering`).
    ///
    /// Added as an additive variant for `SCC-ENG-{NNN}` codes. Existing
    /// documents and exhaustive stage consumers are unaffected; this stage is
    /// never produced by the legacy proposal→geometry pipeline.
    Engineering,
    /// AICAD Engineering DSL lexing/parsing/building (Phase 4 `scc-dsl`).
    ///
    /// Additive variant for `SCC-DSL-{NNN}` codes; never produced by the
    /// legacy pipeline.
    Dsl,
    /// Provider-neutral tool-calling layer (Phase 5 `scc-tools`).
    ///
    /// Additive variant for `SCC-TOOL-{NNN}` codes; never produced by the
    /// legacy pipeline.
    Tool,
    /// Engineering drawing presentation layer (Phase 8 `scc-drawing`).
    ///
    /// Additive variant for `SCC-DRW-{NNN}` codes; never produced by the
    /// legacy proposal→geometry pipeline.
    Drawing,
    /// 2.5-D spatial lowering layer (Phase 9.2 `scc-spatial`).
    ///
    /// Additive variant for `SCC-SPAT-{NNN}` codes; never produced by the
    /// legacy proposal→geometry pipeline.
    Spatial,
    /// Orthographic projection layer (Phase 9.3 `scc-projection`).
    ///
    /// Additive variant for `SCC-PROJ-{NNN}` codes; never produced by the
    /// legacy proposal→geometry pipeline.
    Projection,
    /// Hidden-line visibility classification layer (Phase 9.5).
    ///
    /// Additive variant for SCC-VIS-{NNN} codes.
    Visibility,
    /// Projection, spatial, and visibility sidecar consistency validation
    /// (Phase 9.6).
    ///
    /// Additive variant for SCC-PCV-{NNN} codes.
    ProjectionConsistency,
    /// Backend-neutral manufacturing lowering (Phase 10.1).
    Manufacturing,
}

impl ErrorStage {
    /// Stable prefix used in error codes (`SCC-{PREFIX}-{NNN}`).
    pub fn prefix(self) -> &'static str {
        match self {
            ErrorStage::Proposal => "PROP",
            ErrorStage::Analysis => "ANAL",
            ErrorStage::Lowering => "LOW",
            ErrorStage::Execution => "EXEC",
            ErrorStage::Geometry => "GEOM",
            ErrorStage::Cli => "CLI",
            ErrorStage::Engineering => "ENG",
            ErrorStage::Dsl => "DSL",
            ErrorStage::Tool => "TOOL",
            ErrorStage::Drawing => "DRW",
            ErrorStage::Spatial => "SPAT",
            ErrorStage::Projection => "PROJ",
            ErrorStage::Visibility => "VIS",
            ErrorStage::ProjectionConsistency => "PCV",
            ErrorStage::Manufacturing => "MFG",
        }
    }
}

impl std::fmt::Display for ErrorStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorStage::Proposal => write!(f, "proposal"),
            ErrorStage::Analysis => write!(f, "analysis"),
            ErrorStage::Lowering => write!(f, "lowering"),
            ErrorStage::Execution => write!(f, "execution"),
            ErrorStage::Geometry => write!(f, "geometry"),
            ErrorStage::Cli => write!(f, "cli"),
            ErrorStage::Engineering => write!(f, "engineering"),
            ErrorStage::Dsl => write!(f, "dsl"),
            ErrorStage::Tool => write!(f, "tool"),
            ErrorStage::Drawing => write!(f, "drawing"),
            ErrorStage::Spatial => write!(f, "spatial"),
            ErrorStage::Projection => write!(f, "projection"),
            ErrorStage::Visibility => write!(f, "visibility"),
            ErrorStage::ProjectionConsistency => write!(f, "projection_consistency"),
            ErrorStage::Manufacturing => write!(f, "manufacturing"),
        }
    }
}

/// Severity of a structured error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    Error,
    Warning,
}

/// Cross-crate structured error contract.
///
/// Follows ADR-002. Each error carries a stable code, machine-readable stage,
/// human-readable message, and optional context (path, entity IDs, constraint
/// IDs, source chain, recoverability flag, a suggestion, and deterministic details).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredError {
    pub code: String,
    pub stage: ErrorStage,
    pub severity: ErrorSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_ids: Option<Vec<EntityId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint_ids: Option<Vec<ConstraintId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Box<StructuredError>>,
    pub recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, serde_json::Value>>,
}

/// Violation of the structured error contract reported by
/// [`StructuredError::validate_contract`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorContractViolation {
    pub field: &'static str,
    pub reason: String,
}

impl StructuredError {
    /// Create a new structured error with the required fields.
    pub fn new(code: impl Into<String>, stage: ErrorStage, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            stage,
            severity: ErrorSeverity::Error,
            message: message.into(),
            path: None,
            entity_ids: None,
            constraint_ids: None,
            source: None,
            recoverable: false,
            suggestion: None,
            details: None,
        }
    }

    /// Set the severity.
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Set the JSON path or logical path to the offending input.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the related entity IDs.
    pub fn with_entity_ids(mut self, ids: Vec<EntityId>) -> Self {
        self.entity_ids = Some(ids);
        self
    }

    /// Set the related constraint IDs.
    pub fn with_constraint_ids(mut self, ids: Vec<ConstraintId>) -> Self {
        self.constraint_ids = Some(ids);
        self
    }

    /// Wrap a source error as the cause of this error.
    pub fn with_source(mut self, source: StructuredError) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Mark whether the error is recoverable by modifying input or retrying.
    pub fn with_recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }

    /// Set a human-readable suggestion for fixing the error.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Add a deterministic machine-readable diagnostic detail.
    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value);
        self
    }

    /// Validate that this error obeys the ADR-002 contract.
    ///
    /// Checks performed without regex:
    /// - `code` is non-empty.
    /// - `message` is non-empty after trimming whitespace.
    /// - `code` has the exact shape `SCC-{STAGE}-{NNN}`.
    /// - The `{NNN}` suffix is three ASCII digits.
    /// - The `{STAGE}` segment matches [`ErrorStage::prefix`] of `self.stage`.
    /// - If a `source` is present, it also passes contract validation.
    pub fn validate_contract(&self) -> Result<(), ErrorContractViolation> {
        if self.code.is_empty() {
            return Err(ErrorContractViolation {
                field: "code",
                reason: "code must not be empty".into(),
            });
        }

        if self.message.trim().is_empty() {
            return Err(ErrorContractViolation {
                field: "message",
                reason: "message must not be empty or only whitespace".into(),
            });
        }

        if self
            .path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err(ErrorContractViolation {
                field: "path",
                reason: "path must be omitted or non-empty".into(),
            });
        }

        if self
            .entity_ids
            .as_ref()
            .is_some_and(|entity_ids| entity_ids.is_empty())
        {
            return Err(ErrorContractViolation {
                field: "entity_ids",
                reason: "entity_ids must be omitted or non-empty".into(),
            });
        }

        if self
            .constraint_ids
            .as_ref()
            .is_some_and(|constraint_ids| constraint_ids.is_empty())
        {
            return Err(ErrorContractViolation {
                field: "constraint_ids",
                reason: "constraint_ids must be omitted or non-empty".into(),
            });
        }

        if self.details.as_ref().is_some_and(|details| {
            details.is_empty() || details.keys().any(|key| key.trim().is_empty())
        }) {
            return Err(ErrorContractViolation {
                field: "details",
                reason: "details must be omitted or contain non-empty keys".into(),
            });
        }

        if self
            .suggestion
            .as_deref()
            .is_some_and(|suggestion| suggestion.trim().is_empty())
        {
            return Err(ErrorContractViolation {
                field: "suggestion",
                reason: "suggestion must be omitted or non-empty".into(),
            });
        }

        let parts: Vec<&str> = self.code.split('-').collect();
        if parts.len() != 3 {
            return Err(ErrorContractViolation {
                field: "code",
                reason: format!(
                    "code must be SCC-{{STAGE}}-{{NNN}} (three dash-separated segments), got {}",
                    self.code
                ),
            });
        }

        if parts[0] != "SCC" {
            return Err(ErrorContractViolation {
                field: "code",
                reason: format!("code must start with SCC-, got {}", self.code),
            });
        }

        let expected_prefix = self.stage.prefix();
        if parts[1] != expected_prefix {
            return Err(ErrorContractViolation {
                field: "code",
                reason: format!(
                    "code stage segment must be {} for stage {}, got {}",
                    expected_prefix, self.stage, parts[1]
                ),
            });
        }

        let suffix = parts[2];
        if suffix.len() != 3 || !suffix.chars().all(|c| c.is_ascii_digit()) {
            return Err(ErrorContractViolation {
                field: "code",
                reason: format!(
                    "code numeric suffix must be exactly three ASCII digits, got {}",
                    suffix
                ),
            });
        }

        if let Some(source) = &self.source {
            source.validate_contract()?;
        }

        Ok(())
    }
}

impl std::fmt::Display for StructuredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}] {}", self.code, self.stage, self.message)?;
        if let Some(path) = &self.path {
            write!(f, " at {}", path)?;
        }
        Ok(())
    }
}

/// Historical cross-crate error type.
///
/// `SccError` retains its v0.1.0 variants for backward compatibility. v0.2.0
/// adds the `Structured` variant so new code can propagate rich error context
/// without breaking existing callers.
#[derive(Debug, Error)]
pub enum SccError {
    #[error("execution error: {message}")]
    Execution { message: String },
    #[error("conflict: {message}")]
    Conflict { message: String },
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("structured error: {0}")]
    Structured(Box<StructuredError>),
    /// Multiple structured diagnostics produced by one deterministic pass.
    ///
    /// This variant preserves every diagnostic for machine-readable CLI
    /// output instead of silently discarding all but the first error.
    #[error("multiple structured errors")]
    StructuredBatch(Vec<StructuredError>),
}

impl From<StructuredError> for SccError {
    fn from(e: StructuredError) -> Self {
        SccError::Structured(Box::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_error_full_serialization() {
        let err = StructuredError::new(
            "SCC-PROP-001",
            ErrorStage::Proposal,
            "JSON parsing failed: unexpected token",
        )
        .with_path("/relations/0/reference")
        .with_entity_ids(vec![EntityId("missing_entity".into())])
        .with_constraint_ids(vec![ConstraintId("c1".into())])
        .with_recoverable(true)
        .with_suggestion("Declare the referenced entity before using it")
        .with_detail("remaining", serde_json::json!(1));

        let json = serde_json::to_string(&err).unwrap();
        let back: StructuredError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, back);

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["code"], "SCC-PROP-001");
        assert_eq!(value["stage"], "proposal");
        assert_eq!(value["severity"], "error");
        assert_eq!(value["path"], "/relations/0/reference");
        assert_eq!(value["recoverable"], true);
        assert!(value["entity_ids"].is_array());
        assert!(value["constraint_ids"].is_array());
        assert_eq!(value["details"]["remaining"], 1);
        assert!(value["source"].is_null());
    }

    #[test]
    fn structured_error_optional_fields_omitted_when_empty() {
        let err =
            StructuredError::new("SCC-CLI-001", ErrorStage::Cli, "output file already exists");
        let json = serde_json::to_string(&err).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["code"], "SCC-CLI-001");
        assert!(value.get("path").is_none());
        assert!(value.get("entity_ids").is_none());
        assert!(value.get("constraint_ids").is_none());
        assert!(value.get("source").is_none());
        assert!(value.get("suggestion").is_none());
        assert!(value.get("details").is_none());
        assert_eq!(value["recoverable"], false);
    }

    #[test]
    fn error_stage_serialization_is_lowercase() {
        for stage in [
            ErrorStage::Proposal,
            ErrorStage::Analysis,
            ErrorStage::Lowering,
            ErrorStage::Execution,
            ErrorStage::Geometry,
            ErrorStage::Cli,
            ErrorStage::Engineering,
            ErrorStage::Dsl,
            ErrorStage::Tool,
            ErrorStage::Drawing,
            ErrorStage::Spatial,
            ErrorStage::Projection,
            ErrorStage::Visibility,
            ErrorStage::ProjectionConsistency,
            ErrorStage::Manufacturing,
        ] {
            let json = serde_json::to_string(&stage).unwrap();
            let back: ErrorStage = serde_json::from_str(&json).unwrap();
            assert_eq!(stage, back);
            assert!(json.chars().all(|c| c.is_ascii_lowercase() || c == '"'));
        }
    }

    #[test]
    fn error_severity_serialization() {
        for severity in [ErrorSeverity::Error, ErrorSeverity::Warning] {
            let json = serde_json::to_string(&severity).unwrap();
            let back: ErrorSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(severity, back);
        }
    }

    #[test]
    fn structured_error_source_chain_roundtrip() {
        let source = StructuredError::new(
            "SCC-EXEC-001",
            ErrorStage::Execution,
            "entity not found in state",
        )
        .with_entity_ids(vec![EntityId("e1".into())]);
        let err = StructuredError::new("SCC-CLI-003", ErrorStage::Cli, "execution failed")
            .with_source(source);

        let json = serde_json::to_string(&err).unwrap();
        let back: StructuredError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, back);
        assert_eq!(back.source.as_ref().unwrap().code, "SCC-EXEC-001");
    }

    #[test]
    fn old_scc_error_variants_preserved() {
        let e = SccError::Execution {
            message: "disk full".into(),
        };
        assert!(e.to_string().contains("disk full"));

        let e = SccError::Conflict {
            message: "circular dependency".into(),
        };
        assert!(e.to_string().contains("circular dependency"));
    }

    #[test]
    fn structured_error_into_scc_error() {
        let structured =
            StructuredError::new("SCC-LOW-001", ErrorStage::Lowering, "unsupported relation");
        let scc: SccError = structured.into();
        match scc {
            SccError::Structured(e) => {
                assert_eq!(e.code, "SCC-LOW-001");
                assert_eq!(e.stage, ErrorStage::Lowering);
            }
            _ => panic!("expected SccError::Structured"),
        }
    }

    #[test]
    fn structured_error_display_includes_path() {
        let err = StructuredError::new("SCC-PROP-002", ErrorStage::Proposal, "missing field")
            .with_path("/entities/0/id");
        let text = err.to_string();
        assert!(text.contains("SCC-PROP-002"));
        assert!(text.contains("proposal"));
        assert!(text.contains("missing field"));
        assert!(text.contains("/entities/0/id"));
    }

    #[test]
    fn error_stage_prefix_matches_code_segment() {
        assert_eq!(ErrorStage::Proposal.prefix(), "PROP");
        assert_eq!(ErrorStage::Analysis.prefix(), "ANAL");
        assert_eq!(ErrorStage::Lowering.prefix(), "LOW");
        assert_eq!(ErrorStage::Execution.prefix(), "EXEC");
        assert_eq!(ErrorStage::Geometry.prefix(), "GEOM");
        assert_eq!(ErrorStage::Cli.prefix(), "CLI");
        assert_eq!(ErrorStage::Engineering.prefix(), "ENG");
        assert_eq!(ErrorStage::Dsl.prefix(), "DSL");
        assert_eq!(ErrorStage::Tool.prefix(), "TOOL");
        assert_eq!(ErrorStage::Visibility.prefix(), "VIS");
        assert_eq!(ErrorStage::ProjectionConsistency.prefix(), "PCV");
        assert_eq!(ErrorStage::Manufacturing.prefix(), "MFG");
    }

    #[test]
    fn valid_structured_error_passes_contract() {
        let err = StructuredError::new(
            "SCC-PROP-001",
            ErrorStage::Proposal,
            "input is not valid JSON",
        );
        assert!(err.validate_contract().is_ok());
    }

    #[test]
    fn empty_code_fails_contract() {
        let err = StructuredError::new("", ErrorStage::Proposal, "message");
        let violation = err.validate_contract().unwrap_err();
        assert_eq!(violation.field, "code");
    }

    #[test]
    fn empty_message_fails_contract() {
        let err = StructuredError::new("SCC-PROP-001", ErrorStage::Proposal, "   ");
        let violation = err.validate_contract().unwrap_err();
        assert_eq!(violation.field, "message");
    }

    #[test]
    fn present_optional_context_must_not_be_empty() {
        let cases = [
            StructuredError::new("SCC-PROP-001", ErrorStage::Proposal, "message").with_path("  "),
            StructuredError::new("SCC-PROP-001", ErrorStage::Proposal, "message")
                .with_entity_ids(vec![]),
            StructuredError::new("SCC-PROP-001", ErrorStage::Proposal, "message")
                .with_constraint_ids(vec![]),
            StructuredError::new("SCC-PROP-001", ErrorStage::Proposal, "message")
                .with_suggestion("  "),
            StructuredError::new("SCC-PROP-001", ErrorStage::Proposal, "message")
                .with_detail("  ", serde_json::json!(1)),
        ];
        let fields = [
            "path",
            "entity_ids",
            "constraint_ids",
            "suggestion",
            "details",
        ];

        for (error, field) in cases.into_iter().zip(fields) {
            assert_eq!(error.validate_contract().unwrap_err().field, field);
        }
    }

    #[test]
    fn malformed_code_segments_fails_contract() {
        for code in ["SCC-PROP", "SCC-PROP-01", "SCC-PROP-0001", "FOO-PROP-001"] {
            let err = StructuredError::new(code, ErrorStage::Proposal, "msg");
            let violation = err.validate_contract().unwrap_err();
            assert_eq!(violation.field, "code", "failed for {}", code);
        }
    }

    #[test]
    fn stage_mismatch_fails_contract() {
        let err = StructuredError::new("SCC-EXEC-001", ErrorStage::Proposal, "msg");
        let violation = err.validate_contract().unwrap_err();
        assert_eq!(violation.field, "code");
        assert!(violation.reason.contains("EXEC"));
    }

    #[test]
    fn non_numeric_suffix_fails_contract() {
        let err = StructuredError::new("SCC-PROP-00A", ErrorStage::Proposal, "msg");
        let violation = err.validate_contract().unwrap_err();
        assert_eq!(violation.field, "code");
    }

    #[test]
    fn invalid_source_propagates_contract_violation() {
        let source = StructuredError::new("bad-code", ErrorStage::Execution, "bad");
        let err = StructuredError::new("SCC-CLI-001", ErrorStage::Cli, "outer").with_source(source);
        let violation = err.validate_contract().unwrap_err();
        assert_eq!(violation.field, "code");
        assert!(violation.reason.contains("bad-code"));
    }

    #[test]
    fn valid_source_chain_passes_contract() {
        let source = StructuredError::new("SCC-EXEC-001", ErrorStage::Execution, "inner");
        let err = StructuredError::new("SCC-CLI-001", ErrorStage::Cli, "outer").with_source(source);
        assert!(err.validate_contract().is_ok());
    }
}
