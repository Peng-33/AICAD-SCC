//! Structured errors for the AICAD Engineering DSL.
//!
//! Every [`DslError`] converts to an [`scc_core::error::StructuredError`]
//! carrying `SCC-DSL-{NNN}` codes under
//! [`scc_core::error::ErrorStage::Dsl`], so DSL failures participate in the
//! ADR-002 structured error contract. DSL errors always carry a line/column
//! position where available and never panic.

use scc_core::error::{ErrorStage, StructuredError};

/// A DSL lexing / parsing / building failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DslError {
    #[error("unexpected token at {line}:{column}: {message}")]
    UnexpectedToken {
        message: String,
        line: u32,
        column: u32,
    },

    #[error("unexpected end of file: {message}")]
    UnexpectedEof { message: String },

    #[error("unknown keyword `{keyword}` at {line}:{column}")]
    UnknownKeyword {
        keyword: String,
        line: u32,
        column: u32,
    },

    #[error(
        "invalid identifier `{identifier}` at {line}:{column} (must match [a-z][a-z0-9_-]{{0,63}})"
    )]
    InvalidIdentifier {
        identifier: String,
        line: u32,
        column: u32,
    },

    #[error("invalid number `{text}` at {line}:{column}")]
    InvalidNumber {
        text: String,
        line: u32,
        column: u32,
    },

    #[error("duplicate field `{field}` at {line}:{column} (silent overwrite is forbidden)")]
    DuplicateField {
        field: String,
        line: u32,
        column: u32,
    },

    #[error("feature `{feature}` is missing required field `{missing}` at {line}:{column}")]
    MissingRequiredField {
        feature: String,
        missing: String,
        line: u32,
        column: u32,
    },

    #[error("field `{field}` is not valid for `{feature}` at {line}:{column}: {message}")]
    InvalidFeatureField {
        feature: String,
        field: String,
        message: String,
        line: u32,
        column: u32,
    },

    #[error("feature `{feature}` is not supported by AICAD DSL v1 at {line}:{column}")]
    UnsupportedDslFeature {
        feature: String,
        line: u32,
        column: u32,
    },

    #[error(
        "unsupported DSL version {version} at {line}:{column} (v1 is the only supported version)"
    )]
    UnsupportedVersion {
        version: u32,
        line: u32,
        column: u32,
    },

    #[error("unsupported unit `{unit}` at {line}:{column} (v1 only supports mm)")]
    UnsupportedUnit {
        unit: String,
        line: u32,
        column: u32,
    },

    #[error("invalid expression at {line}:{column}: {message}")]
    InvalidExpression {
        message: String,
        line: u32,
        column: u32,
    },

    #[error("unknown parameter reference `{parameter}` at {line}:{column} (parameters must be declared before use)")]
    UnknownParameterReference {
        parameter: String,
        line: u32,
        column: u32,
    },

    #[error("invalid binding at {line}:{column}: {message}")]
    InvalidBinding {
        message: String,
        line: u32,
        column: u32,
    },

    #[error("command build failed: {message}")]
    CommandBuildFailed { message: String },

    #[error("unterminated string literal at {line}:{column}")]
    UnterminatedString { line: u32, column: u32 },

    #[error("missing `AICAD 1` header at {line}:{column}")]
    MissingHeader { line: u32, column: u32 },
}

impl From<DslError> for StructuredError {
    fn from(err: DslError) -> Self {
        let code = match &err {
            DslError::UnexpectedToken { .. } => "SCC-DSL-001",
            DslError::UnexpectedEof { .. } => "SCC-DSL-002",
            DslError::UnknownKeyword { .. } => "SCC-DSL-003",
            DslError::InvalidIdentifier { .. } => "SCC-DSL-004",
            DslError::InvalidNumber { .. } => "SCC-DSL-005",
            DslError::DuplicateField { .. } => "SCC-DSL-006",
            DslError::MissingRequiredField { .. } => "SCC-DSL-007",
            DslError::InvalidFeatureField { .. } => "SCC-DSL-008",
            DslError::UnsupportedDslFeature { .. } => "SCC-DSL-009",
            DslError::UnsupportedVersion { .. } => "SCC-DSL-010",
            DslError::UnsupportedUnit { .. } => "SCC-DSL-011",
            DslError::InvalidExpression { .. } => "SCC-DSL-012",
            DslError::UnknownParameterReference { .. } => "SCC-DSL-013",
            DslError::InvalidBinding { .. } => "SCC-DSL-014",
            DslError::CommandBuildFailed { .. } => "SCC-DSL-015",
            DslError::UnterminatedString { .. } => "SCC-DSL-016",
            DslError::MissingHeader { .. } => "SCC-DSL-017",
        };
        StructuredError::new(code, ErrorStage::Dsl, err.to_string())
            .with_recoverable(true)
            .with_suggestion("fix the DSL source before retrying")
    }
}

/// Convenience: format a DSL error as `SCC-DSL-NNN` with position.
pub fn format_dsl_error(err: &DslError) -> String {
    let structured: StructuredError = err.clone().into();
    structured.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dsl_error_code_passes_contract() {
        let cases = [
            DslError::UnexpectedToken {
                message: "boom".into(),
                line: 1,
                column: 2,
            },
            DslError::UnexpectedEof {
                message: "missing field".into(),
            },
            DslError::UnknownKeyword {
                keyword: "MAGIC_CUT".into(),
                line: 1,
                column: 1,
            },
            DslError::InvalidIdentifier {
                identifier: "Bad".into(),
                line: 1,
                column: 1,
            },
            DslError::InvalidNumber {
                text: "12x".into(),
                line: 1,
                column: 1,
            },
            DslError::DuplicateField {
                field: "DIA".into(),
                line: 1,
                column: 1,
            },
            DslError::MissingRequiredField {
                feature: "h1".into(),
                missing: "DIA".into(),
                line: 1,
                column: 1,
            },
            DslError::InvalidFeatureField {
                feature: "HOLE".into(),
                field: "WIDTH".into(),
                message: "not valid".into(),
                line: 1,
                column: 1,
            },
            DslError::UnsupportedDslFeature {
                feature: "FILLET".into(),
                line: 1,
                column: 1,
            },
            DslError::UnsupportedVersion {
                version: 2,
                line: 1,
                column: 1,
            },
            DslError::UnsupportedUnit {
                unit: "inch".into(),
                line: 1,
                column: 1,
            },
            DslError::InvalidExpression {
                message: "bad".into(),
                line: 1,
                column: 1,
            },
            DslError::UnknownParameterReference {
                parameter: "x".into(),
                line: 1,
                column: 1,
            },
            DslError::InvalidBinding {
                message: "bad".into(),
                line: 1,
                column: 1,
            },
            DslError::CommandBuildFailed {
                message: "bad".into(),
            },
            DslError::UnterminatedString { line: 1, column: 1 },
            DslError::MissingHeader { line: 1, column: 1 },
        ];
        for err in cases {
            let structured: StructuredError = err.into();
            assert!(
                structured.validate_contract().is_ok(),
                "{} must pass contract",
                structured.code
            );
        }
    }

    #[test]
    fn error_includes_line_and_column() {
        let err = DslError::UnknownKeyword {
            keyword: "MAGIC_CUT".into(),
            line: 17,
            column: 5,
        };
        assert!(err.to_string().contains("17:5"));
    }
}
