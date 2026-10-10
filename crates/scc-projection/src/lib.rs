//! AICAD orthographic projection layer (Phase 9.4).
//!
//! `scc-projection` derives an orthographic projection model from the 2.5-D
//! [`scc_spatial::SpatialModel`] (ADR-033): three canonical principal views —
//! Front, Top, Left — each with a frozen named-axis frame, projected
//! `Segment`/`Circle` primitives carrying depth evidence, stored-validated
//! view bounds, and a provenance map back to the spatial elements
//! (ADR-034).
//!
//! Dependency direction is strictly one-way and leaf-shaped:
//!
//! ```text
//! scc-projection
//!    ├── scc-spatial (2.5-D SpatialModel — the projection input)
//!    └── scc-core    (ErrorStage / StructuredError)
//! ```
//!
//! Like every other AICAD layer it is a pure, deterministic function of its
//! inputs: `BTreeMap`-ordered collections, deterministic ids, no randomness,
//! no cached state, byte-stable serialization (locked by a 100-run test).
//! No drawing, no visibility/hidden-line semantics, no sheet layout or
//! scale, and no matrices or camera state — the frame is three named axes
//! and projection is a signed-coordinate read per axis (ADR-034 Decision 2).
//! Phase 9.4 adds the raw principal-view lowering; sheet integration is Phase
//! 9.7.

pub mod error;
pub mod frame;
pub mod model;
pub mod principal;
pub mod source_map;

pub use error::{InvalidProjectionGeometryReason, InvalidSpatialModelReason, ProjectionError};
pub use frame::{
    AxisDirection, OrthographicFrame, PartLocalPoint3, PrincipalViewKind, ProjectedPoint,
    ProjectedPoint2,
};
pub use model::{
    ProjectedGeometry, ProjectionBounds2D, ProjectionDepthRange, ProjectionModel,
    ProjectionModelBuilder, ProjectionPrimitive, ProjectionPrimitiveId, ProjectionPrimitiveRole,
    ProjectionView, SpatialElementRef,
};
pub use principal::project_principal_views;
pub use source_map::ProjectionSourceMap;
