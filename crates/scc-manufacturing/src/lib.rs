//! Backend-neutral lowering from the resolved 2.5-D spatial model to a
//! deterministic manufacturing build specification.
//!
//! [`SolidBuildSpec`] is a derived snapshot, not an editable engineering
//! model. It deliberately contains no backend-specific execution, persistence,
//! kernel, projection, visibility, or drawing vocabulary. A future backend may read
//! this contract, but it may not read [`scc_engineering::EngineeringModel`]
//! directly.

mod error;
mod lowering;
mod model;

pub use error::SolidBuildError;
pub use lowering::lower_solid_build_spec;
pub use model::{
    ClosedProfileSpec, SolidArcDirection, SolidAxis, SolidBodySource, SolidBodySpec, SolidBounds3D,
    SolidBuildSourceMap, SolidBuildSpec, SolidFootprint, SolidOperation, SolidOperationId,
    SolidPartSpec, SolidPoint2, SolidProfileSegment, SolidZInterval,
    SOLID_BUILD_SPEC_SCHEMA_VERSION,
};
