use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_engineering::FeatureId;
use scc_spatial::SpatialCutId;
use serde::{Deserialize, Serialize};

/// Current backend-neutral SolidBuildSpec schema version.
pub const SOLID_BUILD_SPEC_SCHEMA_VERSION: u32 = 1;

/// A finite point in the canonical part-local XY plane, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolidPoint2 {
    pub x: f64,
    pub y: f64,
}

/// A resolved physical interval on the canonical +Z axis, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolidZInterval {
    pub min: f64,
    pub max: f64,
}

/// An axis-aligned resolved envelope, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolidBounds3D {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

/// Direction of an exact circular profile arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidArcDirection {
    Clockwise,
    CounterClockwise,
}

/// One exact, ordered segment in a single outer closed profile loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SolidProfileSegment {
    Line {
        start: SolidPoint2,
        end: SolidPoint2,
    },
    Arc {
        start: SolidPoint2,
        end: SolidPoint2,
        center: SolidPoint2,
        radius: f64,
        direction: SolidArcDirection,
    },
}

/// A derived exact ordered Line/Arc loop used by profile-based operations.
/// It carries geometry only; it is not an editable feature or an operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosedProfileSpec {
    pub segments: Vec<SolidProfileSegment>,
}

/// The manufacturing body footprint.
///
/// `ClosedProfile` stores one exact outer Line/Arc loop. Typed Polygon emits
/// an ordered line-only contour; typed Composite emits its ordered exact
/// Line/Arc contour. Slot remains fail-closed at the SpatialModel boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SolidFootprint {
    Rectangle {
        origin: SolidPoint2,
        width: f64,
        height: f64,
    },
    Circle {
        center: SolidPoint2,
        radius: f64,
    },
    RoundedRectangle {
        center: SolidPoint2,
        width: f64,
        height: f64,
        corner_radius: f64,
    },
    ClosedProfile {
        segments: Vec<SolidProfileSegment>,
    },
}

/// Canonical axis semantics understood by a future solid backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidAxis {
    PositiveZ,
}

/// The resolved single-body manufacturing representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidBodySpec {
    pub footprint: SolidFootprint,
    pub z: SolidZInterval,
}

/// The one-body part contract. Bounds are copied from SpatialModel as a
/// witness, never independently recomputed as another source of truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidPartSpec {
    pub body: SolidBodySpec,
    pub bounds: SolidBounds3D,
}

/// Typed operations supported by the manufacturing contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SolidOperation {
    CylindricalCut {
        center: SolidPoint2,
        radius: f64,
        z: SolidZInterval,
        axis: SolidAxis,
    },
    /// A through-slot cut whose exact ordered contour is resolved upstream.
    /// This is an operation only; it cannot be interpreted as a body footprint.
    SlotCut {
        profile: ClosedProfileSpec,
        z: SolidZInterval,
        axis: SolidAxis,
    },
}

/// Opaque, deterministic operation identity. The ordinal is allocated from
/// the sorted union of SpatialModel hole and slot cut IDs and is never parsed
/// back for semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SolidOperationId(u32);

impl SolidOperationId {
    pub(crate) fn from_ordinal(ordinal: u32) -> Self {
        Self(ordinal)
    }
}

/// Typed provenance for the single spatial body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolidBodySource {
    pub feature: FeatureId,
    pub entity: EntityId,
}

/// Typed source map. Operations point to opaque SpatialCutId values; the
/// source feature/entity remain authoritative in the SpatialModel cut record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolidBuildSourceMap {
    pub body: SolidBodySource,
    pub operations: BTreeMap<SolidOperationId, SpatialCutId>,
}

/// A derived, read-only, backend-neutral manufacturing build snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidBuildSpec {
    pub schema_version: u32,
    pub part: SolidPartSpec,
    pub operations: BTreeMap<SolidOperationId, SolidOperation>,
    pub source_map: SolidBuildSourceMap,
}

impl SolidBuildSpec {
    /// Validate the public snapshot contract without mutating it.
    pub fn validate(&self) -> Result<(), crate::SolidBuildError> {
        crate::lowering::validate_spec(self)
    }

    /// Resolve an operation's spatial source without interpreting its ID.
    pub fn operation_source(&self, id: SolidOperationId) -> Option<&SpatialCutId> {
        self.source_map.operations.get(&id)
    }
}
