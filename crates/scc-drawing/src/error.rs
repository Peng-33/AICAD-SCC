//! Structured errors for the drawing presentation layer.
//!
//! Every [`DrawingError`] converts losslessly to an
//! [`scc_core::error::StructuredError`] carrying `SCC-DRW-{NNN}` codes under
//! [`scc_core::error::ErrorStage::Drawing`], so the drawing layer participates
//! in the ADR-002 structured error contract without owning the core error
//! types.

use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::EntityId;
use scc_engineering::ParameterId;
use thiserror::Error;

/// A failure while building a derived drawing model.
///
/// Drawing building is fail-closed: no geometry is silently dropped and no
/// provenance is silently guessed. A primitive that cannot be represented, an
/// invalid geometry value, a missing source mapping, an invalid scale, or a
/// source-integrity violation produces a structured error instead of a "mostly
/// right" drawing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DrawingError {
    /// A solved geometry primitive has no drawing representation.
    ///
    /// Phase 8.1 mirrors every current `scc_geometry::GeometryPrimitive`
    /// variant losslessly, so this arm is the documented fail-closed path for
    /// any future primitive added to the geometry layer.
    #[error("entity {entity_id} has geometry primitive kind `{kind}` that has no drawing representation")]
    UnsupportedGeometryPrimitive { entity_id: EntityId, kind: String },

    /// A solved geometry value is not a valid drawing value (e.g. non-finite,
    /// or a degenerate/enclosed value that cannot be represented).
    #[error("entity {entity_id} has invalid geometry value: {reason}")]
    InvalidGeometryValue { entity_id: EntityId, reason: String },

    /// A solved geometry entity has no engineering feature mapping.
    ///
    /// This is a fail-closed integrity violation: the geometry→drawing builder
    /// **never** treats it as [`crate::source_map::DrawingSource::Synthetic`].
    #[error("geometry entity {entity_id} has no engineering feature mapping")]
    MissingSourceMapping { entity_id: EntityId },

    /// The drawing scale is not finite and strictly positive.
    #[error("drawing scale must be finite and strictly positive, got {scale}")]
    InvalidDrawingScale { scale: f64 },

    /// The geometry model has no entities; no meaningful sheet can be built.
    #[error("cannot build a drawing from empty geometry (no solved entities)")]
    EmptyDrawing,

    /// A source-integrity validation failed (orphan primitive, entity missing
    /// from geometry, feature missing, or an inconsistent map).
    #[error("drawing source integrity violation: {reason}")]
    InvalidSourceIntegrity { reason: String },

    /// A view transform is not a valid deterministic placement (non-finite
    /// scale or translation). Fail-closed: an invalid transform can never
    /// produce sheet coordinates.
    #[error("invalid drawing transform: {reason}")]
    InvalidDrawingTransform { reason: String },

    /// A drawing annotation is invalid (e.g. control characters in the
    /// resolved text, non-finite position, or a non-positive text height).
    #[error("invalid drawing annotation: {reason}")]
    InvalidDrawingAnnotation { reason: String },

    /// A drawing dimension is invalid (integrity violation: bad kind/value
    /// pairing, a source that does not re-resolve to the cached value, an
    /// invalid slot-for-feature pair, or a missing feature/literal).
    #[error("invalid drawing dimension: {reason}")]
    InvalidDrawingDimension { reason: String },

    /// A dimension's bound parameter is unresolved. Never falls back to the
    /// feature literal or to zero. Normally blocked earlier in the pipeline
    /// (`SCC-ENG-027` at lowering); this is the defensive drawing-layer gate.
    #[error("dimension parameter {parameter} is unresolved")]
    UnresolvedDimensionValue { parameter: ParameterId },

    /// A dimension targets a feature that has no represented primitive in the
    /// current drawing presentation — fail-closed, never silently kept.
    #[error("invalid dimension target: {reason}")]
    InvalidDimensionTarget { reason: String },

    /// A dimension layout is invalid (orphan layout, non-finite coordinate, or
    /// degenerate dimension line).
    #[error("invalid dimension layout: {reason}")]
    InvalidDimensionLayout { reason: String },

    /// A dimension has no layout. Every dimension must be laid out exactly
    /// once; a missing layout is a fail-closed integrity violation.
    #[error("dimension {dimension} has no layout")]
    MissingDimensionLayout {
        dimension: crate::DrawingDimensionId,
    },

    /// The engineering value of a dimension does not match its geometry
    /// witness — engineering truth and solved geometry disagree.
    #[error("invalid dimension witness: {reason}")]
    InvalidDimensionWitness { reason: String },

    /// A center-mark/centerline element is invalid (non-finite coordinates, a
    /// degenerate segment, a non-cross mark, or a source feature that is not a
    /// circular hole feature).
    #[error("invalid center geometry: {reason}")]
    InvalidCenterGeometry { reason: String },

    /// A center-geometry element has no resolvable engineering source (no
    /// provenance, unknown feature), or its sourced feature has no represented
    /// circle in the presentation.
    #[error("missing center geometry source: {reason}")]
    MissingCenterSource { reason: String },

    /// The SVG renderer rejected the drawing (missing sheet/view, invalid
    /// annotation, non-finite bounds or coordinates).
    #[error("svg rendering failed: {reason}")]
    InvalidSvgDrawing { reason: String },

    /// The DXF renderer rejected the drawing (same gates as SVG, plus
    /// entity-outside-sheet-bounds containment).
    #[error("dxf rendering failed: {reason}")]
    InvalidDxfDrawing { reason: String },
}

impl DrawingError {
    pub fn code(&self) -> &'static str {
        match self {
            DrawingError::UnsupportedGeometryPrimitive { .. } => "SCC-DRW-001",
            DrawingError::InvalidGeometryValue { .. } => "SCC-DRW-002",
            DrawingError::MissingSourceMapping { .. } => "SCC-DRW-003",
            DrawingError::InvalidDrawingScale { .. } => "SCC-DRW-004",
            DrawingError::EmptyDrawing => "SCC-DRW-005",
            DrawingError::InvalidSourceIntegrity { .. } => "SCC-DRW-006",
            DrawingError::InvalidDrawingTransform { .. } => "SCC-DRW-007",
            DrawingError::InvalidDrawingAnnotation { .. } => "SCC-DRW-008",
            DrawingError::InvalidDrawingDimension { .. } => "SCC-DRW-009",
            DrawingError::UnresolvedDimensionValue { .. } => "SCC-DRW-010",
            DrawingError::InvalidDimensionTarget { .. } => "SCC-DRW-011",
            DrawingError::InvalidDimensionLayout { .. } => "SCC-DRW-012",
            DrawingError::MissingDimensionLayout { .. } => "SCC-DRW-013",
            DrawingError::InvalidDimensionWitness { .. } => "SCC-DRW-014",
            DrawingError::InvalidCenterGeometry { .. } => "SCC-DRW-015",
            DrawingError::MissingCenterSource { .. } => "SCC-DRW-016",
            DrawingError::InvalidSvgDrawing { .. } => "SCC-DRW-017",
            DrawingError::InvalidDxfDrawing { .. } => "SCC-DRW-018",
        }
    }
}

impl From<DrawingError> for StructuredError {
    fn from(error: DrawingError) -> Self {
        let code = error.code();
        let structured = match &error {
            DrawingError::UnsupportedGeometryPrimitive { .. } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "geometry primitive has no drawing representation",
            ),
            DrawingError::InvalidGeometryValue { .. } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "solved geometry value is not a valid drawing value",
            ),
            DrawingError::MissingSourceMapping { .. } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "geometry entity has no engineering feature mapping",
            ),
            DrawingError::InvalidDrawingScale { scale } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                format!(
                    "drawing scale must be finite and strictly positive, got {}",
                    scale
                ),
            ),
            DrawingError::EmptyDrawing => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "cannot build a drawing from empty geometry",
            ),
            DrawingError::InvalidSourceIntegrity { .. } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "drawing source integrity violation",
            ),
            DrawingError::InvalidDrawingTransform { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "invalid drawing transform")
            }
            DrawingError::InvalidDrawingAnnotation { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "invalid drawing annotation")
            }
            DrawingError::InvalidDrawingDimension { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "invalid drawing dimension")
            }
            DrawingError::UnresolvedDimensionValue { .. } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "dimension parameter is unresolved",
            ),
            DrawingError::InvalidDimensionTarget { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "invalid dimension target")
            }
            DrawingError::InvalidDimensionLayout { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "invalid dimension layout")
            }
            DrawingError::MissingDimensionLayout { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "dimension has no layout")
            }
            DrawingError::InvalidDimensionWitness { .. } => StructuredError::new(
                code,
                ErrorStage::Drawing,
                "engineering value does not match geometry witness",
            ),
            DrawingError::InvalidCenterGeometry { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "invalid center geometry")
            }
            DrawingError::MissingCenterSource { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "missing center geometry source")
            }
            DrawingError::InvalidSvgDrawing { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "svg rendering failed")
            }
            DrawingError::InvalidDxfDrawing { .. } => {
                StructuredError::new(code, ErrorStage::Drawing, "dxf rendering failed")
            }
        };
        let mut structured = structured.with_recoverable(true);
        // Attach context detail that is cheap and useful for the CLI.
        match &error {
            DrawingError::UnsupportedGeometryPrimitive { entity_id, kind } => {
                structured = structured
                    .with_entity_ids(vec![entity_id.clone()])
                    .with_detail("primitive_kind", serde_json::json!(kind))
            }
            DrawingError::InvalidGeometryValue { entity_id, reason } => {
                structured = structured
                    .with_entity_ids(vec![entity_id.clone()])
                    .with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::MissingSourceMapping { entity_id } => {
                structured = structured.with_entity_ids(vec![entity_id.clone()])
            }
            DrawingError::InvalidSourceIntegrity { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidDrawingTransform { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidDrawingAnnotation { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidDrawingDimension { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::UnresolvedDimensionValue { parameter } => {
                structured =
                    structured.with_detail("parameter", serde_json::json!(parameter.to_string()))
            }
            DrawingError::InvalidDimensionTarget { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidDimensionLayout { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::MissingDimensionLayout { dimension } => {
                structured =
                    structured.with_detail("dimension", serde_json::json!(dimension.to_string()))
            }
            DrawingError::InvalidDimensionWitness { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidCenterGeometry { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::MissingCenterSource { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidSvgDrawing { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidDxfDrawing { reason } => {
                structured = structured.with_detail("reason", serde_json::json!(reason))
            }
            DrawingError::InvalidDrawingScale { .. } | DrawingError::EmptyDrawing => {}
        }
        structured
    }
}

/// Low-level DXF writer failures (from the shared scc-geometry helpers)
/// surface as renderer failures.
impl From<scc_geometry::GeometryError> for DrawingError {
    fn from(error: scc_geometry::GeometryError) -> Self {
        DrawingError::InvalidDxfDrawing {
            reason: error.to_string(),
        }
    }
}

impl From<DrawingError> for SccError {
    fn from(error: DrawingError) -> Self {
        SccError::Structured(Box::new(error.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::error::{ErrorSeverity, ErrorStage};

    #[test]
    fn drawing_error_converts_to_structured_error() {
        let err = DrawingError::InvalidDrawingScale { scale: -2.0 };
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-DRW-004");
        assert_eq!(structured.stage, ErrorStage::Drawing);
        assert_eq!(structured.severity, ErrorSeverity::Error);
        assert!(structured.recoverable);
        structured.validate_contract().unwrap();
    }

    #[test]
    fn every_error_code_passes_contract() {
        let cases = [
            DrawingError::UnsupportedGeometryPrimitive {
                entity_id: EntityId("poly".into()),
                kind: "polygon".into(),
            },
            DrawingError::InvalidGeometryValue {
                entity_id: EntityId("c".into()),
                reason: "radius must be positive".into(),
            },
            DrawingError::MissingSourceMapping {
                entity_id: EntityId("e1".into()),
            },
            DrawingError::InvalidDrawingScale { scale: 0.0 },
            DrawingError::EmptyDrawing,
            DrawingError::InvalidSourceIntegrity {
                reason: "orphan primitive".into(),
            },
            DrawingError::InvalidDrawingTransform {
                reason: "non-finite translation".into(),
            },
            DrawingError::InvalidDrawingAnnotation {
                reason: "control character in text".into(),
            },
            DrawingError::InvalidDrawingDimension {
                reason: "stale cached value".into(),
            },
            DrawingError::UnresolvedDimensionValue {
                parameter: ParameterId("hole_diameter".into()),
            },
            DrawingError::InvalidDimensionTarget {
                reason: "target not represented".into(),
            },
            DrawingError::InvalidDimensionLayout {
                reason: "orphan layout".into(),
            },
            DrawingError::MissingDimensionLayout {
                dimension: crate::DrawingDimensionId("d".into()),
            },
            DrawingError::InvalidDimensionWitness {
                reason: "120 vs 100".into(),
            },
            DrawingError::InvalidCenterGeometry {
                reason: "not a cross".into(),
            },
            DrawingError::MissingCenterSource {
                reason: "no represented circle".into(),
            },
            DrawingError::InvalidSvgDrawing {
                reason: "non-finite coordinate".into(),
            },
            DrawingError::InvalidDxfDrawing {
                reason: "entity outside sheet bounds".into(),
            },
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
    fn error_codes_are_distinct() {
        let codes: Vec<&str> = [
            DrawingError::UnsupportedGeometryPrimitive {
                entity_id: EntityId("a".into()),
                kind: "k".into(),
            },
            DrawingError::InvalidGeometryValue {
                entity_id: EntityId("a".into()),
                reason: "r".into(),
            },
            DrawingError::MissingSourceMapping {
                entity_id: EntityId("a".into()),
            },
            DrawingError::InvalidDrawingScale { scale: 0.0 },
            DrawingError::EmptyDrawing,
            DrawingError::InvalidSourceIntegrity { reason: "r".into() },
            DrawingError::InvalidDrawingTransform { reason: "r".into() },
            DrawingError::InvalidDrawingAnnotation { reason: "r".into() },
            DrawingError::InvalidDrawingDimension { reason: "r".into() },
            DrawingError::UnresolvedDimensionValue {
                parameter: ParameterId("p".into()),
            },
            DrawingError::InvalidDimensionTarget { reason: "r".into() },
            DrawingError::InvalidDimensionLayout { reason: "r".into() },
            DrawingError::MissingDimensionLayout {
                dimension: crate::DrawingDimensionId("d".into()),
            },
            DrawingError::InvalidDimensionWitness { reason: "r".into() },
            DrawingError::InvalidCenterGeometry { reason: "r".into() },
            DrawingError::MissingCenterSource { reason: "r".into() },
            DrawingError::InvalidSvgDrawing { reason: "r".into() },
            DrawingError::InvalidDxfDrawing { reason: "r".into() },
        ]
        .iter()
        .map(|e| e.code())
        .collect();
        let mut sorted = codes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(codes.len(), sorted.len(), "codes must be unique");
    }
}
