//! Deterministic hidden-line classification over frozen principal-view
//! projection evidence (Phase 9.5, ADR-036).
//!
//! This crate is deliberately a sidecar layer. It consumes a spatial model
//! for material context and a projection model for raw geometry/depth evidence,
//! then records exactly one semantic decision per raw projection primitive. It
//! never changes, filters, reprojects, splits, clips, or styles projection
//! geometry.

mod classifier;
pub mod error;
mod geometry;
pub mod model;

pub use classifier::{classify_visibility, validate_visibility_domain};
pub use error::{InvalidVisibilityInputReason, UnsupportedVisibilityReason, VisibilityError};
pub use geometry::{approximately_equal_mm, VISIBILITY_TOLERANCE_MM};
pub use model::{VisibilityClass, VisibilityDecision, VisibilityModel, VisibilityView};
