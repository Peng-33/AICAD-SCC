use scc_core::error::{ErrorStage, StructuredError};
use std::process::ExitCode;

/// Normalize any [`scc_core::error::SccError`] into a list of structured errors.
///
/// Errors that are already structured are returned as-is. Legacy string-based
/// variants are wrapped with stable CLI/execution codes so downstream consumers
/// do not have to parse message text.
pub fn normalize_errors(err: scc_core::error::SccError) -> Vec<StructuredError> {
    match err {
        scc_core::error::SccError::Structured(e) => vec![*e],
        scc_core::error::SccError::StructuredBatch(errors) => errors,
        scc_core::error::SccError::Execution { message } => {
            vec![StructuredError::new(
                "SCC-EXEC-009",
                ErrorStage::Execution,
                message,
            )]
        }
        scc_core::error::SccError::Conflict { message } => {
            vec![
                StructuredError::new("SCC-CLI-009", ErrorStage::Cli, message)
                    .with_suggestion("review the conflict report"),
            ]
        }
        scc_core::error::SccError::Serialization(e) => {
            vec![StructuredError::new(
                "SCC-CLI-002",
                ErrorStage::Cli,
                format!("input is not valid JSON: {}", e),
            )]
        }
        scc_core::error::SccError::Io(e) => {
            let code = if e.kind() == std::io::ErrorKind::AlreadyExists {
                "SCC-CLI-001"
            } else {
                "SCC-CLI-002"
            };
            vec![StructuredError::new(code, ErrorStage::Cli, e.to_string())]
        }
    }
}

/// Determine the process exit code for a set of errors.
///
/// - `0`: success (caller must handle separately).
/// - `2`: business conflict when `--fail-on-conflict` is enabled (caller must
///   handle before invoking this function).
/// - `3`: any runtime integrity / execution-stage error.
/// - `4`: any geometry-stage error.
/// - `1`: everything else (input, JSON, IO, CLI, proposal, analysis, lowering).
pub fn exit_code_for_errors(errors: &[StructuredError]) -> ExitCode {
    if errors.iter().any(|e| e.stage == ErrorStage::Execution) {
        ExitCode::from(3)
    } else if errors.iter().any(|e| e.stage == ErrorStage::Geometry) {
        ExitCode::from(4)
    } else {
        ExitCode::from(1)
    }
}

/// Render errors as a single JSON document to the given writer.
///
/// Returns `Ok(())` if serialization succeeded; if even the JSON serializer
/// fails, returns an `Io` error so the caller can fall back to plain text.
pub fn write_json_errors<W: std::io::Write>(
    writer: &mut W,
    errors: &[StructuredError],
) -> Result<(), scc_core::error::SccError> {
    // Validate every error before serializing so we never emit an invalid
    // document.
    for err in errors {
        err.validate_contract().map_err(|v| {
            scc_core::error::SccError::Structured(Box::new(StructuredError::new(
                "SCC-CLI-009",
                ErrorStage::Cli,
                format!(
                    "internal error contract violation: {} - {}",
                    v.field, v.reason
                ),
            )))
        })?;
    }

    let envelope = serde_json::json!({
        "status": "error",
        "errors": errors,
    });

    serde_json::to_writer_pretty(writer, &envelope)
        .map_err(scc_core::error::SccError::Serialization)?;
    Ok(())
}

/// Print errors for human consumption.
pub fn write_human_errors(errors: &[StructuredError]) {
    for err in errors {
        eprintln!("error: {}", err);
    }
}

/// Print a success message unless `--quiet` is enabled.
pub fn print_success(quiet: bool, message: &str) {
    if !quiet {
        println!("{}", message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::error::{ErrorStage, SccError, StructuredError};

    #[test]
    fn serialization_error_maps_to_cli_002() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let normalized = normalize_errors(SccError::Serialization(json_err));
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].code, "SCC-CLI-002");
        normalized[0].validate_contract().unwrap();
    }

    #[test]
    fn execution_stage_error_maps_to_exit_3() {
        let errors = vec![StructuredError::new(
            "SCC-EXEC-001",
            ErrorStage::Execution,
            "missing entity",
        )];
        assert_eq!(exit_code_for_errors(&errors), ExitCode::from(3));
    }

    #[test]
    fn proposal_stage_error_maps_to_exit_1() {
        let errors = vec![StructuredError::new(
            "SCC-PROP-001",
            ErrorStage::Proposal,
            "bad json",
        )];
        assert_eq!(exit_code_for_errors(&errors), ExitCode::from(1));
    }

    #[test]
    fn json_errors_output_is_valid_json() {
        let errors = vec![StructuredError::new(
            "SCC-CLI-001",
            ErrorStage::Cli,
            "output file already exists",
        )
        .with_path("out.ir.json")];
        let mut buf = Vec::new();
        write_json_errors(&mut buf, &errors).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["status"], "error");
        assert_eq!(value["errors"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn structured_batch_is_not_truncated() {
        let normalized = normalize_errors(SccError::StructuredBatch(vec![
            StructuredError::new("SCC-ANAL-001", ErrorStage::Analysis, "first"),
            StructuredError::new("SCC-ANAL-002", ErrorStage::Analysis, "second"),
        ]));
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].code, "SCC-ANAL-001");
        assert_eq!(normalized[1].code, "SCC-ANAL-002");
    }

    #[test]
    fn geometry_stage_error_maps_to_exit_4() {
        let errors = vec![StructuredError::new(
            "SCC-GEOM-002",
            ErrorStage::Geometry,
            "missing radius",
        )];
        assert_eq!(exit_code_for_errors(&errors), ExitCode::from(4));
    }
}
