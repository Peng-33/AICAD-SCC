use crate::error::{ErrorStage, StructuredError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Initial schema version for v0.2.0 JSON documents.
pub const SCHEMA_VERSION_1_0: &str = "1.0";

/// Supported schema envelope document types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    Proposal,
    ConstraintIr,
    GraphState,
    ExecutionTrace,
    GeometryModel,
}

impl DocumentType {
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentType::Proposal => "proposal",
            DocumentType::ConstraintIr => "constraint_ir",
            DocumentType::GraphState => "graph_state",
            DocumentType::ExecutionTrace => "execution_trace",
            DocumentType::GeometryModel => "geometry_model",
        }
    }
}

impl fmt::Display for DocumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Errors that can occur while reading or writing a [`SchemaEnvelope`].
#[derive(Debug, Clone, PartialEq)]
pub enum EnvelopeError {
    /// The envelope `document_type` does not match the expected payload type.
    DocumentTypeMismatch { expected: String, actual: String },
    /// The envelope schema major version is not supported.
    UnsupportedSchemaVersion { version: String, supported: String },
    /// The envelope is missing the `payload` field.
    MissingPayload,
    /// The payload could not be deserialized as the expected type.
    PayloadDeserialize {
        document_type: String,
        message: String,
    },
    /// JSON serialization failed.
    Serialize { message: String },
}

impl EnvelopeError {
    pub fn code(&self) -> &'static str {
        match self {
            EnvelopeError::DocumentTypeMismatch { .. } => "SCC-CLI-005",
            EnvelopeError::UnsupportedSchemaVersion { .. } => "SCC-CLI-006",
            EnvelopeError::MissingPayload => "SCC-CLI-007",
            EnvelopeError::PayloadDeserialize { .. } => "SCC-CLI-008",
            EnvelopeError::Serialize { .. } => "SCC-CLI-002",
        }
    }
}

impl From<EnvelopeError> for StructuredError {
    fn from(err: EnvelopeError) -> Self {
        let code = err.code();
        match err {
            EnvelopeError::DocumentTypeMismatch { expected, actual } => StructuredError::new(
                code,
                ErrorStage::Cli,
                format!(
                    "envelope document_type mismatch: expected {}, got {}",
                    expected, actual
                ),
            )
            .with_path("/document_type")
            .with_recoverable(true),
            EnvelopeError::UnsupportedSchemaVersion { version, supported } => StructuredError::new(
                code,
                ErrorStage::Cli,
                format!(
                    "unsupported schema version {}, maximum supported is {}",
                    version, supported
                ),
            )
            .with_path("/schema_version")
            .with_recoverable(true),
            EnvelopeError::MissingPayload => {
                StructuredError::new(code, ErrorStage::Cli, "envelope is missing payload")
                    .with_path("/payload")
                    .with_recoverable(true)
            }
            EnvelopeError::PayloadDeserialize {
                document_type,
                message,
            } => StructuredError::new(
                code,
                ErrorStage::Cli,
                format!(
                    "failed to deserialize {} payload: {}",
                    document_type, message
                ),
            )
            .with_path("/payload")
            .with_recoverable(true),
            EnvelopeError::Serialize { message } => StructuredError::new(
                code,
                ErrorStage::Cli,
                format!("failed to serialize envelope: {}", message),
            )
            .with_recoverable(false),
        }
    }
}

/// A versioned outer wrapper around a payload document.
///
/// The envelope keeps the schema version and document type separate from the
/// payload's own fields (such as `Proposal.version`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaEnvelope<T> {
    pub schema_version: String,
    pub document_type: String,
    pub payload: T,
}

impl<T> SchemaEnvelope<T> {
    /// Wrap a payload in a v1.0 envelope for the given document type.
    pub fn new(document_type: DocumentType, payload: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION_1_0.into(),
            document_type: document_type.as_str().into(),
            payload,
        }
    }
}

/// Serialize a payload as a legacy (unversioned) JSON document.
pub fn serialize_legacy<T: Serialize>(payload: &T) -> Result<String, EnvelopeError> {
    serde_json::to_string_pretty(payload).map_err(|e| EnvelopeError::Serialize {
        message: e.to_string(),
    })
}

/// Serialize a payload inside a v1.0 schema envelope.
pub fn serialize_envelope<T: Serialize>(
    document_type: DocumentType,
    payload: &T,
) -> Result<String, EnvelopeError> {
    let envelope = SchemaEnvelope::new(document_type, payload);
    serde_json::to_string_pretty(&envelope).map_err(|e| EnvelopeError::Serialize {
        message: e.to_string(),
    })
}

/// Deserialize JSON that may be either a schema envelope or a legacy
/// unversioned document.
///
/// - If any envelope marker (`schema_version`, `document_type`, or `payload`)
///   is present, the complete envelope contract is validated before parsing.
/// - Otherwise the entire value is parsed as a legacy payload type.
pub fn deserialize_envelope_or_legacy<T: for<'de> Deserialize<'de>>(
    expected_document_type: DocumentType,
    json: &str,
) -> Result<T, EnvelopeError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| EnvelopeError::PayloadDeserialize {
            document_type: expected_document_type.to_string(),
            message: e.to_string(),
        })?;

    let has_envelope_marker = ["schema_version", "document_type", "payload"]
        .iter()
        .any(|field| value.get(field).is_some());

    if has_envelope_marker {
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| EnvelopeError::PayloadDeserialize {
                document_type: expected_document_type.to_string(),
                message: "envelope schema_version must be a MAJOR.MINOR string".into(),
            })?;
        check_schema_version(version)?;

        let actual_document_type = value
            .get("document_type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| EnvelopeError::PayloadDeserialize {
                document_type: expected_document_type.to_string(),
                message: "envelope document_type must be a string".into(),
            })?;

        if actual_document_type != expected_document_type.as_str() {
            return Err(EnvelopeError::DocumentTypeMismatch {
                expected: expected_document_type.to_string(),
                actual: actual_document_type.into(),
            });
        }

        let envelope_payload = value
            .get("payload")
            .cloned()
            .ok_or(EnvelopeError::MissingPayload)?;

        let payload = serde_json::from_value(envelope_payload).map_err(|e| {
            EnvelopeError::PayloadDeserialize {
                document_type: expected_document_type.to_string(),
                message: e.to_string(),
            }
        })?;
        Ok(payload)
    } else {
        serde_json::from_value(value).map_err(|e| EnvelopeError::PayloadDeserialize {
            document_type: expected_document_type.to_string(),
            message: e.to_string(),
        })
    }
}

fn check_schema_version(version: &str) -> Result<(), EnvelopeError> {
    let mut parts = version.split('.');
    let major = parts.next().and_then(|s| s.parse::<u64>().ok());
    let minor = parts.next().and_then(|s| s.parse::<u64>().ok());
    if major.is_none() || minor.is_none() || parts.next().is_some() {
        return Err(EnvelopeError::UnsupportedSchemaVersion {
            version: version.into(),
            supported: SCHEMA_VERSION_1_0.into(),
        });
    }
    let major = major.unwrap();

    let supported_major = SCHEMA_VERSION_1_0
        .split('.')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1);

    if major != supported_major {
        return Err(EnvelopeError::UnsupportedSchemaVersion {
            version: version.into(),
            supported: format!("{}.x", supported_major),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::EntityId;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct SamplePayload {
        pub id: EntityId,
        pub value: f64,
    }

    fn sample_payload() -> SamplePayload {
        SamplePayload {
            id: EntityId("sample".into()),
            value: 42.0,
        }
    }

    #[test]
    fn envelope_without_payload_returns_missing_payload_error() {
        let json = serde_json::json!({
            "schema_version": "1.0",
            "document_type": "constraint_ir"
        });

        let error = deserialize_envelope_or_legacy::<SamplePayload>(
            DocumentType::ConstraintIr,
            &serde_json::to_string(&json).unwrap(),
        )
        .unwrap_err();

        assert_eq!(error, EnvelopeError::MissingPayload);
        let structured: StructuredError = error.into();
        assert_eq!(structured.code, "SCC-CLI-007");
        structured.validate_contract().unwrap();
    }

    #[test]
    fn envelope_roundtrip() {
        let payload = sample_payload();
        let json = serialize_envelope(DocumentType::ConstraintIr, &payload).unwrap();
        let back: SamplePayload =
            deserialize_envelope_or_legacy(DocumentType::ConstraintIr, &json).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn legacy_json_parses_without_envelope() {
        let payload = sample_payload();
        let json = serde_json::to_string_pretty(&payload).unwrap();
        let back: SamplePayload =
            deserialize_envelope_or_legacy(DocumentType::ConstraintIr, &json).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn envelope_includes_version_and_document_type() {
        let payload = sample_payload();
        let json = serialize_envelope(DocumentType::GraphState, &payload).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], SCHEMA_VERSION_1_0);
        assert_eq!(value["document_type"], "graph_state");
        assert!(value["payload"].is_object());
    }

    #[test]
    fn mismatched_document_type_returns_error() {
        let payload = sample_payload();
        let json = serialize_envelope(DocumentType::Proposal, &payload).unwrap();
        let result: Result<SamplePayload, EnvelopeError> =
            deserialize_envelope_or_legacy(DocumentType::ConstraintIr, &json);
        assert!(matches!(
            result,
            Err(EnvelopeError::DocumentTypeMismatch { .. })
        ));
    }

    #[test]
    fn unsupported_major_version_returns_error() {
        let envelope = serde_json::json!({
            "schema_version": "2.0",
            "document_type": "constraint_ir",
            "payload": sample_payload(),
        });
        let result: Result<SamplePayload, EnvelopeError> = deserialize_envelope_or_legacy(
            DocumentType::ConstraintIr,
            &serde_json::to_string(&envelope).unwrap(),
        );
        assert!(matches!(
            result,
            Err(EnvelopeError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn minor_version_within_same_major_is_accepted() {
        let envelope = serde_json::json!({
            "schema_version": "1.5",
            "document_type": "constraint_ir",
            "payload": sample_payload(),
        });
        let back: SamplePayload = deserialize_envelope_or_legacy(
            DocumentType::ConstraintIr,
            &serde_json::to_string(&envelope).unwrap(),
        )
        .unwrap();
        assert_eq!(back, sample_payload());
    }

    #[test]
    fn malformed_or_incompatible_versions_are_rejected() {
        for version in ["1", "1.", "1.foo", "1.0.0", "0.9", "2.0"] {
            let envelope = serde_json::json!({
                "schema_version": version,
                "document_type": "constraint_ir",
                "payload": sample_payload(),
            });
            let result: Result<SamplePayload, EnvelopeError> = deserialize_envelope_or_legacy(
                DocumentType::ConstraintIr,
                &serde_json::to_string(&envelope).unwrap(),
            );
            assert!(
                matches!(result, Err(EnvelopeError::UnsupportedSchemaVersion { .. })),
                "version {version} must be rejected, got {result:?}"
            );
        }
    }

    #[test]
    fn partial_envelope_cannot_fall_back_to_legacy() {
        let json = serde_json::json!({
            "schema_version": "2.0",
            "id": "sample",
            "value": 42.0
        });
        let result: Result<SamplePayload, EnvelopeError> = deserialize_envelope_or_legacy(
            DocumentType::ConstraintIr,
            &serde_json::to_string(&json).unwrap(),
        );
        assert!(matches!(
            result,
            Err(EnvelopeError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn structured_error_from_envelope_error_passes_contract() {
        let err = EnvelopeError::DocumentTypeMismatch {
            expected: "constraint_ir".into(),
            actual: "proposal".into(),
        };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-CLI-005");
        structured.validate_contract().unwrap();
    }
}
