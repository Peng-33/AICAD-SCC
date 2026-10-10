use scc_core::error::{ErrorStage, StructuredError};
use thiserror::Error;

/// Fail-closed errors for the backend-neutral manufacturing boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SolidBuildError {
    #[error("invalid manufacturing input: {detail}")]
    InvalidManufacturingInput { detail: String },

    #[error("unsupported manufacturing feature: {detail}")]
    UnsupportedManufacturingFeature { detail: String },

    #[error("invalid manufacturing source: {detail}")]
    InvalidManufacturingSource { detail: String },

    #[error("invalid manufacturing geometry: {detail}")]
    InvalidManufacturingGeometry { detail: String },
}

impl SolidBuildError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidManufacturingInput { .. } => "SCC-MFG-001",
            Self::UnsupportedManufacturingFeature { .. } => "SCC-MFG-002",
            Self::InvalidManufacturingSource { .. } => "SCC-MFG-003",
            Self::InvalidManufacturingGeometry { .. } => "SCC-MFG-004",
        }
    }
}

impl From<SolidBuildError> for StructuredError {
    fn from(error: SolidBuildError) -> Self {
        StructuredError::new(error.code(), ErrorStage::Manufacturing, error.to_string())
    }
}
