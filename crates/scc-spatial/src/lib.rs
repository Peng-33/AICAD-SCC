//! AICAD 2.5-D spatial lowering layer (Phase 9.2).
//!
//! `scc-spatial` lowers engineering spatial intent plus solved geometry into
//! a derived, read-only [`SpatialModel`]: the plate solid (the solved
//! `BaseProfile` rectangle extruded over `z = [0, thickness]` on the
//! canonical part-local +Z axis, ADR-032) plus cylindrical cuts witnessed by
//! solved hole circles, reference-only construction circles, and a bounds
//! envelope derived from the body footprint only.
//!
//! Dependency direction is strictly one-way and leaf-shaped:
//!
//! ```text
//! scc-spatial
//!    ├── scc-engineering (readiness gate, spatial intent, source map)
//!    ├── scc-geometry    (solved geometry — authoritative positions)
//!    └── scc-core        (ErrorStage / StructuredError / EntityId)
//! ```
//!
//! Like every other AICAD layer it is a pure, deterministic function of its
//! inputs: `lower_spatial` never mutates the model, never re-solves
//! constraints, never regenerates pattern geometry, and never caches derived
//! state. The [`SpatialModel`] has no mutators — consumers query, they do not
//! edit. No B-Rep, mesh, boolean, or projection logic lives in this crate
//! (Phase 9.3 will own projection as a separate derived model, ADR-033).

pub mod error;
pub mod lowering;
pub mod model;

pub use error::{
    InvalidBaseReason, InvalidGeometryReason, InvalidSpatialSourceReason, PrimitiveKind,
    SpatialError,
};
pub use lowering::{lower_spatial, SpatialInputs};
pub use model::{
    Bounds3D, CylindricalCut, ParametricSpatialEvidence, SlotCut, SpatialArcDirection,
    SpatialBaseProfile, SpatialBody, SpatialBodyFrame, SpatialCutId, SpatialCutKind,
    SpatialEdgeEvidence, SpatialEdgeVisibility, SpatialFaceEvidence, SpatialFaceSurface,
    SpatialFootprint, SpatialKernelEdgeCurve, SpatialModel, SpatialModelSource,
    SpatialPrincipalView, SpatialProfileSegment, SpatialProjectedEdgeEvidence,
    SpatialProjectedGeometry, ZInterval,
};
// `SpatialCutKind::Blind` exposes this upstream semantic type as part of its
// public value representation. Re-export it so downstream consumers can
// validate the resolved interval without taking a second dependency on the
// engineering crate.
pub use scc_engineering::HoleStartFace;
