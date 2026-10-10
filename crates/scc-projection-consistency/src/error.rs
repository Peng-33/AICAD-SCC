//! Structured errors for the Phase 9.6 consistency boundary.

use scc_core::error::{ErrorStage, StructuredError};
use scc_projection::{PrincipalViewKind, ProjectionPrimitiveId, SpatialElementRef};
use thiserror::Error;

/// Which input truth was malformed or disagreed with another truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsistencyModel {
    Spatial,
    Projection,
    Visibility,
    CrossModel,
}

/// Stable PCV error kind independent of the human-readable detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionConsistencyErrorKind {
    InvalidInputModel,
    UnsupportedTopology,
    ViewOrPrimitiveMismatch,
    SourceOrRoleMismatch,
    GeometryMismatch,
    DepthMismatch,
    VisibilityMismatch,
    NonFiniteNumericState,
}

/// Fail-closed errors emitted by the Phase 9.6 validator.
///
/// The optional view/primitive/source context is diagnostic only. Identity is
/// always obtained from ProjectionSourceMap; primitive IDs are never parsed.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ProjectionConsistencyError {
    #[error("invalid {model:?} model: {detail}")]
    InvalidInputModel {
        model: ConsistencyModel,
        detail: String,
    },

    #[error("unsupported consistency topology: {detail}")]
    UnsupportedTopology { detail: String },

    #[error("view or primitive mismatch (view={view:?}, primitive={primitive:?}): {detail}")]
    ViewOrPrimitiveMismatch {
        view: Option<PrincipalViewKind>,
        primitive: Option<ProjectionPrimitiveId>,
        detail: String,
    },

    #[error("source or role mismatch (view={view:?}, primitive={primitive:?}, element={element:?}): {detail}")]
    SourceOrRoleMismatch {
        view: Option<PrincipalViewKind>,
        primitive: Option<ProjectionPrimitiveId>,
        element: Option<SpatialElementRef>,
        detail: String,
    },

    #[error("projected geometry mismatch (view={view:?}, primitive={primitive:?}, element={element:?}): {detail}")]
    GeometryMismatch {
        view: Option<PrincipalViewKind>,
        primitive: Option<ProjectionPrimitiveId>,
        element: Option<SpatialElementRef>,
        detail: String,
    },

    #[error("projected depth mismatch (view={view:?}, primitive={primitive:?}, element={element:?}): {detail}")]
    DepthMismatch {
        view: Option<PrincipalViewKind>,
        primitive: Option<ProjectionPrimitiveId>,
        element: Option<SpatialElementRef>,
        detail: String,
    },

    #[error("visibility mismatch (view={view:?}, primitive={primitive:?}): {detail}")]
    VisibilityMismatch {
        view: Option<PrincipalViewKind>,
        primitive: Option<ProjectionPrimitiveId>,
        detail: String,
    },

    #[error(
        "non-finite or numerically unsafe state (view={view:?}, primitive={primitive:?}): {detail}"
    )]
    NonFiniteNumericState {
        view: Option<PrincipalViewKind>,
        primitive: Option<ProjectionPrimitiveId>,
        detail: String,
    },
}

impl ProjectionConsistencyError {
    pub fn kind(&self) -> ProjectionConsistencyErrorKind {
        match self {
            Self::InvalidInputModel { .. } => ProjectionConsistencyErrorKind::InvalidInputModel,
            Self::UnsupportedTopology { .. } => ProjectionConsistencyErrorKind::UnsupportedTopology,
            Self::ViewOrPrimitiveMismatch { .. } => {
                ProjectionConsistencyErrorKind::ViewOrPrimitiveMismatch
            }
            Self::SourceOrRoleMismatch { .. } => {
                ProjectionConsistencyErrorKind::SourceOrRoleMismatch
            }
            Self::GeometryMismatch { .. } => ProjectionConsistencyErrorKind::GeometryMismatch,
            Self::DepthMismatch { .. } => ProjectionConsistencyErrorKind::DepthMismatch,
            Self::VisibilityMismatch { .. } => ProjectionConsistencyErrorKind::VisibilityMismatch,
            Self::NonFiniteNumericState { .. } => {
                ProjectionConsistencyErrorKind::NonFiniteNumericState
            }
        }
    }

    pub fn code(&self) -> &'static str {
        match self.kind() {
            ProjectionConsistencyErrorKind::InvalidInputModel => "SCC-PCV-001",
            ProjectionConsistencyErrorKind::UnsupportedTopology => "SCC-PCV-002",
            ProjectionConsistencyErrorKind::ViewOrPrimitiveMismatch => "SCC-PCV-003",
            ProjectionConsistencyErrorKind::SourceOrRoleMismatch => "SCC-PCV-004",
            ProjectionConsistencyErrorKind::GeometryMismatch => "SCC-PCV-005",
            ProjectionConsistencyErrorKind::DepthMismatch => "SCC-PCV-006",
            ProjectionConsistencyErrorKind::VisibilityMismatch => "SCC-PCV-007",
            ProjectionConsistencyErrorKind::NonFiniteNumericState => "SCC-PCV-008",
        }
    }

    pub fn stage(&self) -> ErrorStage {
        ErrorStage::ProjectionConsistency
    }
}

impl From<ProjectionConsistencyError> for StructuredError {
    fn from(error: ProjectionConsistencyError) -> Self {
        let path = error.location_path();
        let mut structured = StructuredError::new(error.code(), error.stage(), error.to_string());
        if let Some(path) = path {
            structured = structured.with_path(path);
        }
        structured
    }
}

impl ProjectionConsistencyError {
    /// Stable logical location for structured diagnostics. The source segment
    /// is derived from the typed source-map value, never from primitive-id
    /// parsing.
    fn location_path(&self) -> Option<String> {
        let (view, primitive, element) = match self {
            Self::InvalidInputModel { .. } | Self::UnsupportedTopology { .. } => return None,
            Self::ViewOrPrimitiveMismatch {
                view, primitive, ..
            }
            | Self::VisibilityMismatch {
                view, primitive, ..
            }
            | Self::NonFiniteNumericState {
                view, primitive, ..
            } => (*view, primitive.as_ref(), None),
            Self::SourceOrRoleMismatch {
                view,
                primitive,
                element,
                ..
            }
            | Self::GeometryMismatch {
                view,
                primitive,
                element,
                ..
            }
            | Self::DepthMismatch {
                view,
                primitive,
                element,
                ..
            } => (*view, primitive.as_ref(), element.as_ref()),
        };

        let mut path = String::new();
        if let Some(view) = view {
            path.push_str("projection.views.");
            path.push_str(match view {
                PrincipalViewKind::Front => "front",
                PrincipalViewKind::Top => "top",
                PrincipalViewKind::Left => "left",
            });
        } else if primitive.is_some() {
            path.push_str("projection.primitives");
        }
        if let Some(primitive) = primitive {
            path.push_str(".primitive[");
            path.push_str(&primitive.0);
            path.push(']');
        }
        if let Some(element) = element {
            path.push_str(";source=");
            match element {
                SpatialElementRef::Body => path.push_str("body"),
                SpatialElementRef::BodySegment { index } => {
                    path.push_str(&format!("body.segment[{index}]"));
                }
                SpatialElementRef::Cut { id } => path.push_str(&id.0),
                SpatialElementRef::SlotCut { id } => path.push_str(&format!("slot[{}]", id.0)),
                SpatialElementRef::SlotSegment { id, index } => {
                    path.push_str(&format!("slot[{}].segment[{index}]", id.0));
                }
            }
        }
        (!path.is_empty()).then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::error::{ErrorSeverity, StructuredError};

    fn errors() -> Vec<ProjectionConsistencyError> {
        vec![
            ProjectionConsistencyError::InvalidInputModel {
                model: ConsistencyModel::Spatial,
                detail: "bad".into(),
            },
            ProjectionConsistencyError::UnsupportedTopology {
                detail: "bad".into(),
            },
            ProjectionConsistencyError::ViewOrPrimitiveMismatch {
                view: Some(PrincipalViewKind::Front),
                primitive: None,
                detail: "bad".into(),
            },
            ProjectionConsistencyError::SourceOrRoleMismatch {
                view: None,
                primitive: None,
                element: Some(SpatialElementRef::Body),
                detail: "bad".into(),
            },
            ProjectionConsistencyError::GeometryMismatch {
                view: None,
                primitive: None,
                element: None,
                detail: "bad".into(),
            },
            ProjectionConsistencyError::DepthMismatch {
                view: None,
                primitive: None,
                element: None,
                detail: "bad".into(),
            },
            ProjectionConsistencyError::VisibilityMismatch {
                view: None,
                primitive: None,
                detail: "bad".into(),
            },
            ProjectionConsistencyError::NonFiniteNumericState {
                view: None,
                primitive: None,
                detail: "bad".into(),
            },
        ]
    }

    #[test]
    fn error_codes_and_structured_stage_are_sequential() {
        for (index, error) in errors().iter().enumerate() {
            assert_eq!(error.code(), format!("SCC-PCV-{:03}", index + 1));
            let structured: StructuredError = error.clone().into();
            assert_eq!(structured.stage, ErrorStage::ProjectionConsistency);
            assert_eq!(structured.severity, ErrorSeverity::Error);
            assert!(!structured.recoverable);
            structured.validate_contract().unwrap();
        }
    }

    #[test]
    fn display_location_is_deterministic() {
        let error = ProjectionConsistencyError::VisibilityMismatch {
            view: Some(PrincipalViewKind::Top),
            primitive: Some(ProjectionPrimitiveId("opaque".into())),
            detail: "wrong class".into(),
        };
        assert!(error
            .to_string()
            .contains("primitive=Some(ProjectionPrimitiveId(\"opaque\"))"));
        let structured: StructuredError = error.into();
        assert_eq!(
            structured.path.as_deref(),
            Some("projection.views.top.primitive[opaque]")
        );
        assert!(structured.message.contains("opaque"));
    }
}
