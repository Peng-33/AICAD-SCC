//! Fail-closed consistency validation for the frozen Phase 9.2/9.4/9.5
//! artifacts (Phase 9.6, ADR-037).
//!
//! This crate is an invariant boundary, not a derived-artifact builder. It
//! reads SpatialModel, ProjectionModel, and VisibilityModel; it never mutates
//! them, reprojects, reclassifies, clips, splits, or styles them.

mod validator;

pub mod error;

pub use error::{ConsistencyModel, ProjectionConsistencyError, ProjectionConsistencyErrorKind};
pub use validator::validate_projection_consistency;
