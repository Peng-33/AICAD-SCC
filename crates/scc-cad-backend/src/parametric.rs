//! Stateless transport contract for generic ordered sketch/extrude rebuilds.
//!
//! Shapes live only inside the CadQuery worker invocation. The Rust side
//! receives validated measurements and read-only spatial evidence; this
//! contract is deliberately separate from SolidBuildSpec v1 and STEP.

use std::collections::BTreeSet;

use scc_spatial::{
    ParametricSpatialEvidence, SpatialEdgeVisibility, SpatialFaceSurface, SpatialKernelEdgeCurve,
    SpatialPrincipalView, SpatialProjectedGeometry,
};
use serde::{Deserialize, Serialize};

use crate::{CadBackendClient, CadBackendError, SolidBackendFailure, SolidResultBounds};

pub const PARAMETRIC_REBUILD_SCHEMA_VERSION: u32 = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricRebuildRequest {
    pub schema_version: u32,
    pub model_sha256: String,
    pub operations: Vec<ParametricRebuildOperation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricExtrudeOperation {
    pub feature_id: String,
    pub sketch_id: String,
    pub operation: ParametricBooleanOperation,
    pub plane: ParametricProfilePlane,
    pub loops: Vec<Vec<ParametricCurve>>,
    pub profile_entity_ids: Vec<Vec<String>>,
    pub direction: [f64; 3],
    pub extent: ParametricExtent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_feature_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_feature_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_face: Option<ParametricFaceReference>,
    #[serde(default)]
    pub blind_hole: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ParametricRebuildOperation {
    Extrude(Box<ParametricExtrudeOperation>),
    Countersink(Box<ParametricCountersinkOperation>),
    Revolve(Box<ParametricRevolveOperation>),
    Sweep(Box<ParametricSweepOperation>),
    Loft(Box<ParametricLoftOperation>),
    Fillet {
        feature_id: String,
        edge_refs: Vec<ParametricEdgeReference>,
        radius: f64,
    },
    Chamfer {
        feature_id: String,
        edge_refs: Vec<ParametricEdgeReference>,
        distance: f64,
    },
}

/// A transient conical cut appended after the cylindrical main bore. It is
/// derived from one EngineeringModel Hole feature and never persisted as a
/// separate editable feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricCountersinkOperation {
    pub feature_id: String,
    pub source_feature_id: String,
    pub source_feature_kind: String,
    pub plane: ParametricProfilePlane,
    pub direction: [f64; 3],
    pub main_diameter: f64,
    pub mouth_diameter: f64,
    pub included_angle_degrees: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_face: Option<ParametricFaceReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricSweepOperation {
    pub feature_id: String,
    pub sketch_id: String,
    pub path_id: String,
    pub operation: ParametricBooleanOperation,
    pub plane: ParametricProfilePlane,
    pub loops: Vec<Vec<ParametricCurve>>,
    pub profile_entity_ids: Vec<Vec<String>>,
    pub profile_origin: [f64; 2],
    pub path_plane: ParametricProfilePlane,
    pub path: Vec<ParametricCurve>,
    pub path_entity_ids: Vec<String>,
    pub direction: ParametricSweepDirection,
    pub orientation: ParametricSweepOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricSweepDirection {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricSweepOrientation {
    FixedBinormal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricLoftSection {
    pub sketch_id: String,
    pub plane: ParametricProfilePlane,
    pub loops: Vec<Vec<ParametricCurve>>,
    pub profile_entity_ids: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricLoftOperation {
    pub feature_id: String,
    pub operation: ParametricBooleanOperation,
    pub mode: ParametricLoftMode,
    pub orientation_policy: ParametricLoftOrientationPolicy,
    pub sections: Vec<ParametricLoftSection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricLoftMode {
    Ruled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricLoftOrientationPolicy {
    Aligned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricRevolveOperation {
    pub feature_id: String,
    pub sketch_id: String,
    pub operation: ParametricBooleanOperation,
    pub plane: ParametricProfilePlane,
    pub loops: Vec<Vec<ParametricCurve>>,
    pub profile_entity_ids: Vec<Vec<String>>,
    pub axis_origin: [f64; 3],
    pub direction: [f64; 3],
    pub angle: ParametricRevolveAngle,
    pub orientation: ParametricRevolveOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParametricRevolveAngle {
    #[serde(rename = "full_360")]
    Full360,
    Partial {
        degrees: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricRevolveOrientation {
    Forward,
    Reverse,
}

impl ParametricRebuildOperation {
    pub fn feature_id(&self) -> &str {
        match self {
            Self::Extrude(operation) => &operation.feature_id,
            Self::Countersink(operation) => &operation.feature_id,
            Self::Revolve(operation) => &operation.feature_id,
            Self::Sweep(operation) => &operation.feature_id,
            Self::Loft(operation) => &operation.feature_id,
            Self::Fillet { feature_id, .. } | Self::Chamfer { feature_id, .. } => feature_id,
        }
    }

    pub fn source_feature_id(&self) -> &str {
        match self {
            Self::Extrude(operation) => operation
                .source_feature_id
                .as_deref()
                .unwrap_or(&operation.feature_id),
            Self::Countersink(operation) => &operation.source_feature_id,
            Self::Revolve(operation) => &operation.feature_id,
            Self::Sweep(operation) => &operation.feature_id,
            Self::Loft(operation) => &operation.feature_id,
            Self::Fillet { feature_id, .. } | Self::Chamfer { feature_id, .. } => feature_id,
        }
    }

    pub fn as_extrude(&self) -> Option<&ParametricExtrudeOperation> {
        match self {
            Self::Extrude(operation) => Some(operation.as_ref()),
            _ => None,
        }
    }

    pub fn as_revolve(&self) -> Option<&ParametricRevolveOperation> {
        match self {
            Self::Revolve(operation) => Some(operation.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricSemanticSurfaceKind {
    Plane,
    Cylinder,
    Cone,
    RuledSurface,
    RevolutionSurface,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParametricGeneratedFaceSelector {
    SweepStartCap,
    SweepEndCap,
    SweepLateral {
        profile_entity_id: String,
    },
    LoftStartCap,
    LoftEndCap,
    LoftLateral {
        start_section_id: String,
        end_section_id: String,
        start_entity_id: String,
        end_entity_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricGeneratedFaceQualifier {
    pub surface_kind: Option<ParametricSemanticSurfaceKind>,
    pub normal: Option<[f64; 3]>,
    pub centroid: Option<[f64; 3]>,
    pub area_mm2: Option<f64>,
    pub area_tolerance_mm2: Option<f64>,
    pub tolerance_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricGeneratedFaceReference {
    pub source_feature_id: String,
    pub selector: ParametricGeneratedFaceSelector,
    pub qualifier: ParametricGeneratedFaceQualifier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricEdgeReference {
    pub source_feature_id: String,
    pub selector: ParametricEdgeSelector,
    pub qualifier: ParametricEdgeQualifier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParametricEdgeSelector {
    RevolveProfileVertex {
        entity_id: String,
        endpoint: ParametricProfileEndpoint,
    },
    ProfileBoundary {
        entity_id: String,
        side: ParametricProfileEdgeSide,
    },
    PocketEntrance {
        entity_id: String,
    },
    HoleEntrance,
    HoleExit,
    BetweenFaces {
        first: ParametricFaceReference,
        second: ParametricFaceReference,
    },
    Convex {
        first: ParametricFaceReference,
        second: ParametricFaceReference,
    },
    Concave {
        first: ParametricFaceReference,
        second: ParametricFaceReference,
    },
    BetweenGeneratedFaces {
        first: ParametricGeneratedFaceReference,
        second: ParametricGeneratedFaceReference,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricProfileEdgeSide {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricEdgeCurveKind {
    Line,
    Circular,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricEdgeQualifier {
    pub curve_kind: Option<ParametricEdgeCurveKind>,
    pub radius_mm: Option<f64>,
    pub length_mm: Option<f64>,
    pub midpoint: Option<[f64; 3]>,
    pub tolerance_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricFaceReference {
    pub source_feature_id: String,
    pub selector: String,
    pub normal: [f64; 3],
    pub plane_offset: f64,
    pub tolerance: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_qualifier: Option<ParametricGeneratedFaceQualifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricBooleanOperation {
    Add,
    Cut,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParametricProfilePlane {
    pub origin: [f64; 3],
    pub u_axis: [f64; 3],
    pub v_axis: [f64; 3],
    pub normal: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParametricCurve {
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    Arc {
        start: [f64; 2],
        end: [f64; 2],
        center: [f64; 2],
        radius: f64,
        direction: ParametricArcDirection,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricArcDirection {
    CounterClockwise,
    Clockwise,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParametricExtent {
    Distance { length: f64 },
    ThroughAll,
    Symmetric { length: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricSectionPlane {
    pub axis: ParametricSectionAxis,
    pub offset_mm: f64,
    pub view_side: ParametricSectionViewSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricSectionAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricSectionViewSide {
    Positive,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricSectionFrame {
    pub axis: ParametricSectionAxis,
    pub offset_mm: f64,
    pub view_side: ParametricSectionViewSide,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub u_axis: [f64; 3],
    pub v_axis: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricSectionGeometry {
    pub schema_version: u32,
    pub model_sha256: String,
    pub plane: ParametricSectionFrame,
    pub loops: Vec<Vec<[f64; 2]>>,
    pub edge_count: u32,
    pub sample_chord_tolerance_mm: f64,
    pub geometry_elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ParametricSectionPayload {
    rebuild: ParametricRebuildResult,
    section_geometry: ParametricSectionGeometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ParametricSectionWireResponse {
    ok: bool,
    #[serde(default)]
    result: Option<ParametricSectionPayload>,
    #[serde(default)]
    error: Option<SolidBackendFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricRebuildResult {
    pub schema_version: u32,
    pub valid: bool,
    pub body_count: u32,
    pub solid_count: u32,
    pub operation_count: u32,
    pub operation_timings: Vec<ParametricOperationTiming>,
    pub bbox: SolidResultBounds,
    pub spatial_evidence: ParametricSpatialEvidence,
    #[serde(skip)]
    pub worker_elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParametricOperationTiming {
    pub feature_id: String,
    pub kind: String,
    pub topology_resolution_ms: f64,
    pub kernel_operation_ms: f64,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ParametricWireResponse {
    ok: bool,
    #[serde(default)]
    result: Option<ParametricRebuildResult>,
    #[serde(default)]
    error: Option<SolidBackendFailure>,
}

impl ParametricWireResponse {
    fn into_result(self) -> Result<ParametricRebuildResult, CadBackendError> {
        if self.ok {
            return self.result.ok_or_else(|| {
                CadBackendError::InvalidResult(
                    "successful parametric rebuild omitted result".into(),
                )
            });
        }
        Err(CadBackendError::Backend(self.error.ok_or_else(|| {
            CadBackendError::InvalidResult("failed parametric rebuild omitted error".into())
        })?))
    }
}

fn curve_is_finite(curve: &ParametricCurve) -> bool {
    match curve {
        ParametricCurve::Line { start, end } => {
            start.iter().chain(end).all(|value| value.is_finite())
        }
        ParametricCurve::Arc {
            start,
            end,
            center,
            radius,
            ..
        } => {
            start
                .iter()
                .chain(end)
                .chain(center)
                .all(|value| value.is_finite())
                && radius.is_finite()
                && *radius > 0.0
        }
        ParametricCurve::Circle { center, radius } => {
            center.iter().all(|value| value.is_finite()) && radius.is_finite() && *radius > 0.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricProfileEndpoint {
    Start,
    End,
}

fn expected_section_frame(plane: ParametricSectionPlane) -> ParametricSectionFrame {
    let sign = match plane.view_side {
        ParametricSectionViewSide::Positive => 1.0,
        ParametricSectionViewSide::Negative => -1.0,
    };
    let mut origin = [0.0; 3];
    let (axis_index, normal, u_axis, v_axis) = match plane.axis {
        ParametricSectionAxis::X => (0, [sign, 0.0, 0.0], [0.0, 0.0, -sign], [0.0, 1.0, 0.0]),
        ParametricSectionAxis::Y => (1, [0.0, sign, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -sign]),
        ParametricSectionAxis::Z => (2, [0.0, 0.0, sign], [1.0, 0.0, 0.0], [0.0, sign, 0.0]),
    };
    origin[axis_index] = plane.offset_mm;
    ParametricSectionFrame {
        axis: plane.axis,
        offset_mm: plane.offset_mm,
        view_side: plane.view_side,
        origin,
        normal,
        u_axis,
        v_axis,
    }
}

fn validate_section_geometry(
    request: &ParametricRebuildRequest,
    expected_plane: ParametricSectionPlane,
    section: &ParametricSectionGeometry,
) -> Result<(), CadBackendError> {
    let frame = &section.plane;
    let finite = frame
        .origin
        .iter()
        .chain(frame.normal.iter())
        .chain(frame.u_axis.iter())
        .chain(frame.v_axis.iter())
        .chain(section.loops.iter().flatten().flatten())
        .all(|value| value.is_finite());
    let expected_frame = expected_section_frame(expected_plane);
    let same_vector = |actual: [f64; 3], expected: [f64; 3]| {
        actual
            .iter()
            .zip(expected.iter())
            .all(|(actual, expected)| (actual - expected).abs() <= 1.0e-9)
    };
    let frame_matches = frame.axis == expected_frame.axis
        && frame.view_side == expected_frame.view_side
        && (frame.offset_mm - expected_frame.offset_mm).abs() <= 1.0e-9
        && same_vector(frame.origin, expected_frame.origin)
        && same_vector(frame.normal, expected_frame.normal)
        && same_vector(frame.u_axis, expected_frame.u_axis)
        && same_vector(frame.v_axis, expected_frame.v_axis);
    let loops_valid = !section.loops.is_empty()
        && section.loops.iter().all(|loop_| {
            if loop_.len() < 3 {
                return false;
            }
            let area = loop_
                .iter()
                .enumerate()
                .map(|(i, point)| {
                    let next = loop_[(i + 1) % loop_.len()];
                    point[0] * next[1] - next[0] * point[1]
                })
                .sum::<f64>()
                / 2.0;
            area.is_finite()
                && area.abs() > 1.0e-6
                && loop_.iter().enumerate().all(|(i, point)| {
                    let next = loop_[(i + 1) % loop_.len()];
                    (point[0] - next[0]).abs() > 1.0e-10 || (point[1] - next[1]).abs() > 1.0e-10
                })
        });
    let normal_length = frame
        .normal
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    let u_length = frame
        .u_axis
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    let v_length = frame
        .v_axis
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    let dot_uv = frame
        .u_axis
        .iter()
        .zip(frame.v_axis.iter())
        .map(|(a, b)| a * b)
        .sum::<f64>();
    if section.schema_version != 1
        || section.model_sha256 != request.model_sha256
        || !frame_matches
        || !finite
        || !loops_valid
        || section.edge_count == 0
        || !section.sample_chord_tolerance_mm.is_finite()
        || section.sample_chord_tolerance_mm <= 0.0
        || section.sample_chord_tolerance_mm > 0.01
        || !section.geometry_elapsed_ms.is_finite()
        || section.geometry_elapsed_ms < 0.0
        || (normal_length - 1.0).abs() > 1.0e-8
        || (u_length - 1.0).abs() > 1.0e-8
        || (v_length - 1.0).abs() > 1.0e-8
        || dot_uv.abs() > 1.0e-8
    {
        return Err(CadBackendError::InvalidResult(
            "parametric section geometry failed schema, identity, topology, or frame validation"
                .into(),
        ));
    }
    Ok(())
}

fn vector_dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

impl ParametricRebuildRequest {
    pub fn validate(&self) -> Result<(), CadBackendError> {
        if self.schema_version != PARAMETRIC_REBUILD_SCHEMA_VERSION {
            return Err(CadBackendError::InvalidResult(format!(
                "unsupported parametric rebuild schema version {}",
                self.schema_version
            )));
        }
        if self.model_sha256.len() != 64
            || !self
                .model_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(CadBackendError::InvalidResult(
                "parametric model identity is not a SHA-256 hex digest".into(),
            ));
        }
        if self.operations.is_empty() {
            return Err(CadBackendError::InvalidResult(
                "parametric rebuild history is empty".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for operation in &self.operations {
            if operation.feature_id().is_empty() || !ids.insert(operation.feature_id()) {
                return Err(CadBackendError::InvalidResult(
                    "parametric operation IDs are empty or duplicated".into(),
                ));
            }
            match operation {
                ParametricRebuildOperation::Extrude(operation) => {
                    if operation.sketch_id.is_empty()
                        || operation.loops.is_empty()
                        || operation.loops.iter().any(|loop_| loop_.is_empty())
                        || operation.profile_entity_ids.len() != operation.loops.len()
                        || operation
                            .profile_entity_ids
                            .iter()
                            .zip(&operation.loops)
                            .any(|(ids, loop_)| {
                                ids.len() != loop_.len() || ids.iter().any(String::is_empty)
                            })
                        || !operation
                            .plane
                            .origin
                            .iter()
                            .chain(operation.plane.u_axis.iter())
                            .chain(operation.plane.v_axis.iter())
                            .chain(operation.plane.normal.iter())
                            .chain(operation.direction.iter())
                            .chain(
                                operation
                                    .support_face
                                    .iter()
                                    .flat_map(|face| face.normal.iter()),
                            )
                            .all(|value| value.is_finite())
                        || operation.support_face.as_ref().is_some_and(|face| {
                            face.source_feature_id.is_empty()
                                || face.selector.is_empty()
                                || !face.plane_offset.is_finite()
                                || !face.tolerance.is_finite()
                                || face.tolerance <= 0.0
                                || face.generated_qualifier.is_some_and(|qualifier| {
                                    !valid_generated_face_qualifier(qualifier)
                                })
                        })
                    {
                        return Err(CadBackendError::InvalidResult(format!(
                            "parametric operation {} has invalid profile or frame values",
                            operation.feature_id
                        )));
                    }
                }
                ParametricRebuildOperation::Countersink(operation) => {
                    let normal_length = operation
                        .plane
                        .normal
                        .iter()
                        .map(|v| v * v)
                        .sum::<f64>()
                        .sqrt();
                    let direction_length = operation
                        .direction
                        .iter()
                        .map(|v| v * v)
                        .sum::<f64>()
                        .sqrt();
                    let orthogonal =
                        vector_dot(operation.plane.u_axis, operation.plane.v_axis).abs() <= 1.0e-8
                            && vector_dot(operation.plane.u_axis, operation.plane.normal).abs()
                                <= 1.0e-8
                            && vector_dot(operation.plane.v_axis, operation.plane.normal).abs()
                                <= 1.0e-8;
                    let derived_depth = (operation.mouth_diameter - operation.main_diameter)
                        / (2.0 * (operation.included_angle_degrees.to_radians() / 2.0).tan());
                    if operation.source_feature_id.is_empty()
                        || operation.source_feature_kind.is_empty()
                        || !operation
                            .plane
                            .origin
                            .iter()
                            .chain(operation.plane.u_axis.iter())
                            .chain(operation.plane.v_axis.iter())
                            .chain(operation.plane.normal.iter())
                            .chain(operation.direction.iter())
                            .all(|v| v.is_finite())
                        || !operation.main_diameter.is_finite()
                        || operation.main_diameter <= 0.0
                        || !operation.mouth_diameter.is_finite()
                        || operation.mouth_diameter <= operation.main_diameter
                        || !operation.included_angle_degrees.is_finite()
                        || !(0.0 < operation.included_angle_degrees
                            && operation.included_angle_degrees < 180.0)
                        || !derived_depth.is_finite()
                        || derived_depth <= 1.0e-7
                        || !normal_length.is_finite()
                        || (normal_length - 1.0).abs() > 1.0e-8
                        || !direction_length.is_finite()
                        || (direction_length - 1.0).abs() > 1.0e-8
                        || (vector_dot(operation.direction, operation.plane.normal).abs() - 1.0)
                            .abs()
                            > 1.0e-8
                        || !orthogonal
                        || operation.support_face.as_ref().is_some_and(|face| {
                            face.source_feature_id.is_empty()
                                || face.selector.is_empty()
                                || !face.plane_offset.is_finite()
                                || !face.tolerance.is_finite()
                                || face.tolerance <= 0.0
                                || face.generated_qualifier.is_some_and(|qualifier| {
                                    !valid_generated_face_qualifier(qualifier)
                                })
                        })
                    {
                        return Err(CadBackendError::InvalidResult(format!(
                            "countersink operation {} has invalid cone dimensions or frame values",
                            operation.feature_id
                        )));
                    }
                }
                ParametricRebuildOperation::Revolve(operation) => {
                    let direction_length = operation
                        .direction
                        .iter()
                        .map(|value| value * value)
                        .sum::<f64>()
                        .sqrt();
                    if operation.feature_id.is_empty()
                        || operation.sketch_id.is_empty()
                        || operation.loops.is_empty()
                        || operation.loops.iter().any(|loop_| loop_.is_empty())
                        || operation.profile_entity_ids.len() != operation.loops.len()
                        || operation
                            .profile_entity_ids
                            .iter()
                            .zip(&operation.loops)
                            .any(|(ids, loop_)| {
                                ids.len() != loop_.len() || ids.iter().any(String::is_empty)
                            })
                        || !operation
                            .axis_origin
                            .iter()
                            .chain(operation.direction.iter())
                            .chain(
                                operation
                                    .plane
                                    .origin
                                    .iter()
                                    .chain(operation.plane.u_axis.iter())
                                    .chain(operation.plane.v_axis.iter())
                                    .chain(operation.plane.normal.iter()),
                            )
                            .all(|value| value.is_finite())
                        || !direction_length.is_finite()
                        || (direction_length - 1.0).abs() > 1.0e-8
                        || operation
                            .loops
                            .iter()
                            .flatten()
                            .any(|curve| !curve_is_finite(curve))
                        || matches!(operation.angle, ParametricRevolveAngle::Partial { degrees } if !degrees.is_finite() || degrees <= 0.0 || degrees >= 360.0)
                    {
                        return Err(CadBackendError::InvalidResult(format!(
                            "revolve operation {} has invalid profile, axis, angle, or frame values",
                            operation.feature_id
                        )));
                    }
                }
                ParametricRebuildOperation::Sweep(operation) => {
                    validate_sweep_operation(operation)?;
                }
                ParametricRebuildOperation::Loft(operation) => {
                    validate_loft_operation(operation)?;
                }
                ParametricRebuildOperation::Fillet {
                    edge_refs, radius, ..
                }
                | ParametricRebuildOperation::Chamfer {
                    edge_refs,
                    distance: radius,
                    ..
                } => {
                    if !radius.is_finite() || *radius <= 0.0 || edge_refs.is_empty() {
                        return Err(CadBackendError::InvalidResult(format!(
                            "parametric edge treatment {} has invalid dimensions or no references",
                            operation.feature_id()
                        )));
                    }
                    let mut seen_refs = BTreeSet::new();
                    for reference in edge_refs {
                        if reference.source_feature_id.is_empty()
                            || !seen_refs.insert(
                                serde_json::to_string(reference).map_err(CadBackendError::from)?,
                            )
                            || !reference.qualifier.tolerance_mm.is_finite()
                            || reference.qualifier.tolerance_mm <= 0.0
                            || reference
                                .qualifier
                                .radius_mm
                                .is_some_and(|value| !value.is_finite() || value <= 0.0)
                            || reference
                                .qualifier
                                .length_mm
                                .is_some_and(|value| !value.is_finite() || value <= 0.0)
                            || reference
                                .qualifier
                                .midpoint
                                .is_some_and(|v| !v.iter().all(|value| value.is_finite()))
                            || matches!(&reference.selector, ParametricEdgeSelector::BetweenGeneratedFaces { first, second }
                                if first == second || first.source_feature_id != reference.source_feature_id
                                    || second.source_feature_id != reference.source_feature_id
                                    || !valid_generated_face_reference(first) || !valid_generated_face_reference(second))
                        {
                            return Err(CadBackendError::InvalidResult(format!(
                                "parametric edge treatment {} has invalid semantic edge references",
                                operation.feature_id()
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn valid_generated_face_qualifier(value: ParametricGeneratedFaceQualifier) -> bool {
    value.tolerance_mm.is_finite()
        && value.tolerance_mm > 0.0
        && value
            .area_mm2
            .is_none_or(|area| area.is_finite() && area > 0.0)
        && value
            .area_tolerance_mm2
            .is_none_or(|tol| tol.is_finite() && tol > 0.0)
        && value.normal.is_none_or(|normal| {
            normal.iter().all(|n| n.is_finite())
                && (normal.iter().map(|n| n * n).sum::<f64>().sqrt() - 1.0).abs() <= 1.0e-8
        })
        && value
            .centroid
            .is_none_or(|point| point.iter().all(|n| n.is_finite()))
}

fn valid_generated_face_reference(value: &ParametricGeneratedFaceReference) -> bool {
    !value.source_feature_id.is_empty()
        && valid_generated_face_qualifier(value.qualifier)
        && match &value.selector {
            ParametricGeneratedFaceSelector::SweepStartCap
            | ParametricGeneratedFaceSelector::SweepEndCap
            | ParametricGeneratedFaceSelector::LoftStartCap
            | ParametricGeneratedFaceSelector::LoftEndCap => true,
            ParametricGeneratedFaceSelector::SweepLateral { profile_entity_id } => {
                !profile_entity_id.is_empty()
            }
            ParametricGeneratedFaceSelector::LoftLateral {
                start_section_id,
                end_section_id,
                start_entity_id,
                end_entity_id,
            } => {
                !start_section_id.is_empty()
                    && !end_section_id.is_empty()
                    && start_section_id != end_section_id
                    && !start_entity_id.is_empty()
                    && !end_entity_id.is_empty()
            }
        }
}

fn validate_loft_operation(operation: &ParametricLoftOperation) -> Result<(), CadBackendError> {
    let invalid = || {
        CadBackendError::InvalidResult(format!(
            "parametric Loft {} has invalid ordered sections, profile correspondence, or orientation",
            operation.feature_id
        ))
    };
    if operation.feature_id.is_empty()
        || !(2..=32).contains(&operation.sections.len())
        || operation.orientation_policy != ParametricLoftOrientationPolicy::Aligned
        || operation.mode != ParametricLoftMode::Ruled
    {
        return Err(invalid());
    }
    let first = operation.sections.first().ok_or_else(invalid)?;
    let mut sketch_ids = BTreeSet::new();
    let first_plane = &first.plane;
    let vectors_valid = |plane: &ParametricProfilePlane| {
        let n = plane.normal;
        let u = plane.u_axis;
        let v = plane.v_axis;
        let norm = |a: [f64; 3]| a.iter().map(|x| x * x).sum::<f64>().sqrt();
        let dot = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        plane
            .origin
            .iter()
            .chain(n.iter())
            .chain(u.iter())
            .chain(v.iter())
            .all(|x| x.is_finite())
            && (norm(n) - 1.0).abs() <= 1.0e-8
            && (norm(u) - 1.0).abs() <= 1.0e-8
            && (norm(v) - 1.0).abs() <= 1.0e-8
            && dot(n, u).abs() <= 1.0e-8
            && dot(n, v).abs() <= 1.0e-8
            && dot(u, v).abs() <= 1.0e-8
    };
    if !vectors_valid(first_plane) {
        return Err(invalid());
    }
    let mut previous_offset = 0.0;
    let mut expected_signature: Option<Vec<u8>> = None;
    let mut expected_area_sign = 0_i8;
    for (index, section) in operation.sections.iter().enumerate() {
        if section.sketch_id.is_empty()
            || !sketch_ids.insert(section.sketch_id.as_str())
            || section.loops.len() != 1
            || section.profile_entity_ids.len() != 1
            || section.profile_entity_ids[0].len() != section.loops[0].len()
            || section.profile_entity_ids[0].iter().any(String::is_empty)
            || section.profile_entity_ids[0]
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != section.profile_entity_ids[0].len()
            || section.loops[0].is_empty()
            || section.loops[0].iter().any(|curve| !curve_is_finite(curve))
            || !vectors_valid(&section.plane)
        {
            return Err(invalid());
        }
        let close = |left: [f64; 3], right: [f64; 3]| {
            left.iter()
                .zip(right)
                .all(|(a, b)| (*a - b).abs() <= 1.0e-8)
        };
        if !close(section.plane.normal, first_plane.normal)
            || !close(section.plane.u_axis, first_plane.u_axis)
            || !close(section.plane.v_axis, first_plane.v_axis)
        {
            return Err(invalid());
        }
        let delta = [
            section.plane.origin[0] - first_plane.origin[0],
            section.plane.origin[1] - first_plane.origin[1],
            section.plane.origin[2] - first_plane.origin[2],
        ];
        let offset = delta
            .iter()
            .zip(first_plane.normal)
            .map(|(a, b)| a * b)
            .sum::<f64>();
        let lateral = (0..3)
            .map(|axis| (delta[axis] - first_plane.normal[axis] * offset).powi(2))
            .sum::<f64>()
            .sqrt();
        if lateral > 1.0e-7 || (index > 0 && offset <= previous_offset + 1.0e-7) {
            return Err(invalid());
        }
        previous_offset = offset;

        let signature = section.loops[0]
            .iter()
            .map(|curve| match curve {
                ParametricCurve::Line { .. } => 0,
                ParametricCurve::Arc { .. } => 1,
                ParametricCurve::Circle { .. } => 2,
            })
            .collect::<Vec<_>>();
        if expected_signature
            .as_ref()
            .is_some_and(|expected| expected != &signature)
        {
            return Err(invalid());
        }
        expected_signature.get_or_insert(signature);
        let area = loop_signed_area(&section.loops[0]);
        if !area.is_finite() || area.abs() <= 1.0e-10 {
            return Err(invalid());
        }
        let sign = if area > 0.0 { 1 } else { -1 };
        if expected_area_sign != 0 && sign != expected_area_sign {
            return Err(invalid());
        }
        expected_area_sign = sign;
    }
    Ok(())
}

fn loop_signed_area(curves: &[ParametricCurve]) -> f64 {
    use std::f64::consts::TAU;
    curves
        .iter()
        .map(|curve| match curve {
            ParametricCurve::Line { start, end } => (start[0] * end[1] - start[1] * end[0]) * 0.5,
            ParametricCurve::Circle { radius, .. } => std::f64::consts::PI * radius * radius,
            ParametricCurve::Arc {
                start,
                end,
                center,
                radius,
                direction,
            } => {
                let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
                let a1 = (end[1] - center[1]).atan2(end[0] - center[0]);
                let delta = if *direction == ParametricArcDirection::CounterClockwise {
                    (a1 - a0).rem_euclid(TAU)
                } else {
                    -(a0 - a1).rem_euclid(TAU)
                };
                0.5 * (radius * center[0] * (a1.sin() - a0.sin())
                    - radius * center[1] * (a1.cos() - a0.cos())
                    + radius * radius * delta)
            }
        })
        .sum()
}

fn validate_sweep_operation(operation: &ParametricSweepOperation) -> Result<(), CadBackendError> {
    let invalid = || {
        CadBackendError::InvalidResult(format!(
            "parametric Sweep {} has an invalid path, profile, or orientation frame",
            operation.feature_id
        ))
    };
    if operation.feature_id.is_empty()
        || operation.sketch_id.is_empty()
        || operation.path_id.is_empty()
        || operation.loops.is_empty()
        || operation.loops.iter().any(Vec::is_empty)
        || operation.profile_entity_ids.len() != operation.loops.len()
        || operation
            .profile_entity_ids
            .iter()
            .zip(&operation.loops)
            .any(|(ids, curves)| ids.len() != curves.len() || ids.iter().any(String::is_empty))
        || operation.path.is_empty()
        || operation.path.len() != operation.path_entity_ids.len()
        || operation.path_entity_ids.iter().any(String::is_empty)
        || operation
            .path_entity_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != operation.path_entity_ids.len()
        || operation
            .path
            .iter()
            .any(|curve| matches!(curve, ParametricCurve::Circle { .. }) || !curve_is_finite(curve))
        || operation
            .loops
            .iter()
            .flatten()
            .any(|curve| !curve_is_finite(curve))
        || !operation
            .profile_origin
            .iter()
            .all(|value| value.is_finite())
    {
        return Err(invalid());
    }
    validate_frame(
        operation.plane.origin,
        operation.plane.u_axis,
        operation.plane.v_axis,
        operation.plane.normal,
    )?;
    validate_frame(
        operation.path_plane.origin,
        operation.path_plane.u_axis,
        operation.path_plane.v_axis,
        operation.path_plane.normal,
    )?;
    let endpoints = |curve: &ParametricCurve| match curve {
        ParametricCurve::Line { start, end } | ParametricCurve::Arc { start, end, .. } => {
            Some((*start, *end))
        }
        ParametricCurve::Circle { .. } => None,
    };
    let start_tangent = |curve: &ParametricCurve, at_start: bool| -> Option<[f64; 2]> {
        let tangent = match curve {
            ParametricCurve::Line { start, end } => [end[0] - start[0], end[1] - start[1]],
            ParametricCurve::Arc {
                start,
                end,
                center,
                direction,
                ..
            } => {
                let point = if at_start { start } else { end };
                let sign = if *direction == ParametricArcDirection::CounterClockwise {
                    1.0
                } else {
                    -1.0
                };
                [
                    -sign * (point[1] - center[1]),
                    sign * (point[0] - center[0]),
                ]
            }
            ParametricCurve::Circle { .. } => return None,
        };
        let length = (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
        (length > 1.0e-8).then_some([tangent[0] / length, tangent[1] / length])
    };
    let mut directed_storage = operation.path.clone();
    if operation.direction == ParametricSweepDirection::Reverse {
        directed_storage.reverse();
        for curve in &mut directed_storage {
            match curve {
                ParametricCurve::Line { start, end } => std::mem::swap(start, end),
                ParametricCurve::Arc {
                    start,
                    end,
                    direction,
                    ..
                } => {
                    std::mem::swap(start, end);
                    *direction = match direction {
                        ParametricArcDirection::CounterClockwise => {
                            ParametricArcDirection::Clockwise
                        }
                        ParametricArcDirection::Clockwise => {
                            ParametricArcDirection::CounterClockwise
                        }
                    };
                }
                ParametricCurve::Circle { .. } => return Err(invalid()),
            }
        }
    }
    let directed_curves = directed_storage.iter().collect::<Vec<_>>();
    for pair in directed_curves.windows(2) {
        let (_, end) = endpoints(pair[0]).ok_or_else(invalid)?;
        let (start, _) = endpoints(pair[1]).ok_or_else(invalid)?;
        if (end[0] - start[0]).hypot(end[1] - start[1]) > 1.0e-8 {
            return Err(invalid());
        }
        let left = start_tangent(pair[0], false).ok_or_else(invalid)?;
        let right = start_tangent(pair[1], true).ok_or_else(invalid)?;
        if left[0] * right[0] + left[1] * right[1] < 1.0 - 1.0e-8 {
            return Err(invalid());
        }
    }
    let first = directed_curves.first().ok_or_else(invalid)?;
    let (path_start, _) = endpoints(first).ok_or_else(invalid)?;
    let tangent2 = start_tangent(first, true).ok_or_else(invalid)?;
    let path_tangent = [
        operation.path_plane.u_axis[0] * tangent2[0] + operation.path_plane.v_axis[0] * tangent2[1],
        operation.path_plane.u_axis[1] * tangent2[0] + operation.path_plane.v_axis[1] * tangent2[1],
        operation.path_plane.u_axis[2] * tangent2[0] + operation.path_plane.v_axis[2] * tangent2[1],
    ];
    let binormal = operation.path_plane.normal;
    let expected_u = [
        binormal[1] * path_tangent[2] - binormal[2] * path_tangent[1],
        binormal[2] * path_tangent[0] - binormal[0] * path_tangent[2],
        binormal[0] * path_tangent[1] - binormal[1] * path_tangent[0],
    ];
    let path_world = [0, 1, 2].map(|i| {
        operation.path_plane.origin[i]
            + operation.path_plane.u_axis[i] * path_start[0]
            + operation.path_plane.v_axis[i] * path_start[1]
    });
    let profile_anchor = [0, 1, 2].map(|i| {
        operation.plane.origin[i]
            + operation.plane.u_axis[i] * operation.profile_origin[0]
            + operation.plane.v_axis[i] * operation.profile_origin[1]
    });
    let near =
        |left: [f64; 3], right: [f64; 3]| (0..3).all(|i| (left[i] - right[i]).abs() <= 1.0e-7);
    if operation.orientation != ParametricSweepOrientation::FixedBinormal
        || !near(path_world, profile_anchor)
        || !near(path_tangent, operation.plane.normal)
        || !near(expected_u, operation.plane.u_axis)
        || !near(binormal, operation.plane.v_axis)
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn validate_parametric_result(
    request: &ParametricRebuildRequest,
    result: &ParametricRebuildResult,
) -> Result<(), CadBackendError> {
    let bounds = &result.bbox;
    let values = [
        bounds.x_min,
        bounds.x_max,
        bounds.y_min,
        bounds.y_max,
        bounds.z_min,
        bounds.z_max,
    ];
    if result.schema_version != PARAMETRIC_REBUILD_SCHEMA_VERSION
        || !result.valid
        || result.body_count != 1
        || result.solid_count != 1
        || result.operation_count != request.operations.len() as u32
        || result.operation_timings.len() != request.operations.len()
        || !values.iter().all(|value| value.is_finite())
        || bounds.x_max <= bounds.x_min
        || bounds.y_max <= bounds.y_min
        || bounds.z_max <= bounds.z_min
        || !result.spatial_evidence.volume_mm3.is_finite()
        || result.spatial_evidence.volume_mm3 <= 0.0
        || result.spatial_evidence.model_sha256 != request.model_sha256
        || result
            .spatial_evidence
            .operation_features
            .iter()
            .map(|feature| feature.0.as_str())
            .collect::<Vec<_>>()
            != request
                .operations
                .iter()
                .map(ParametricRebuildOperation::source_feature_id)
                .collect::<Vec<_>>()
    {
        return Err(CadBackendError::InvalidResult(
            "parametric worker result does not match the request or one-body contract".into(),
        ));
    }

    for (timing, operation) in result.operation_timings.iter().zip(&request.operations) {
        let expected_kind = match operation {
            ParametricRebuildOperation::Extrude(extrude) => match extrude.operation {
                ParametricBooleanOperation::Add => "extrude_add",
                ParametricBooleanOperation::Cut => "extrude_cut",
            },
            ParametricRebuildOperation::Countersink(_) => "countersink_cut",
            ParametricRebuildOperation::Revolve(revolve) => match revolve.operation {
                ParametricBooleanOperation::Add => "revolve_add",
                ParametricBooleanOperation::Cut => "revolve_cut",
            },
            ParametricRebuildOperation::Sweep(sweep) => match sweep.operation {
                ParametricBooleanOperation::Add => "sweep_add",
                ParametricBooleanOperation::Cut => "sweep_cut",
            },
            ParametricRebuildOperation::Loft(loft) => match loft.operation {
                ParametricBooleanOperation::Add => "loft_add",
                ParametricBooleanOperation::Cut => "loft_cut",
            },
            ParametricRebuildOperation::Fillet { .. } => "fillet",
            ParametricRebuildOperation::Chamfer { .. } => "chamfer",
        };
        if timing.feature_id != operation.feature_id()
            || timing.kind != expected_kind
            || !timing.topology_resolution_ms.is_finite()
            || timing.topology_resolution_ms < 0.0
            || !timing.kernel_operation_ms.is_finite()
            || timing.kernel_operation_ms <= 0.0
            || !timing.elapsed_ms.is_finite()
            || timing.elapsed_ms < timing.kernel_operation_ms
        {
            return Err(CadBackendError::InvalidResult(format!(
                "parametric worker returned invalid operation timing for {}",
                operation.feature_id()
            )));
        }
    }

    let evidence = &result.spatial_evidence;
    validate_frame(
        evidence.frame.origin,
        evidence.frame.u_axis,
        evidence.frame.v_axis,
        evidence.frame.axis,
    )?;
    let known_sources = request
        .operations
        .iter()
        .map(|operation| operation.source_feature_id().to_owned())
        .collect::<BTreeSet<_>>();
    let mut edge_ids = BTreeSet::new();
    for edge in &evidence.edges {
        if edge.id.is_empty()
            || !edge_ids.insert(edge.id.as_str())
            || !edge.length_mm.is_finite()
            || edge.length_mm <= 0.0
            || !valid_spatial_edge_curve(&edge.curve)
            || !valid_provenance(&edge.source_features, &known_sources)
        {
            return Err(CadBackendError::InvalidResult(
                "parametric edge evidence or provenance is malformed".into(),
            ));
        }
    }
    let mut face_ids = BTreeSet::new();
    for face in &evidence.faces {
        if face.id.is_empty()
            || !face_ids.insert(face.id.as_str())
            || !face.area_mm2.is_finite()
            || face.area_mm2 <= 0.0
            || !face
                .bounds_min
                .iter()
                .chain(face.bounds_max.iter())
                .all(|value| value.is_finite())
            || (0..3).any(|axis| face.bounds_min[axis] > face.bounds_max[axis])
            || !valid_spatial_face_surface(&face.surface)
            || !valid_provenance(&face.source_features, &known_sources)
        {
            return Err(CadBackendError::InvalidResult(
                "parametric face evidence or provenance is malformed".into(),
            ));
        }
    }
    if !evidence.frame.axis_min.is_finite()
        || !evidence.frame.axis_max.is_finite()
        || evidence.frame.axis_max <= evidence.frame.axis_min
        || evidence.edges.is_empty()
        || evidence.faces.is_empty()
    {
        return Err(CadBackendError::InvalidResult(
            "parametric spatial evidence is incomplete".into(),
        ));
    }
    let expected_views = [
        SpatialPrincipalView::Front,
        SpatialPrincipalView::Top,
        SpatialPrincipalView::Left,
    ];
    if evidence.views.len() != expected_views.len() {
        return Err(CadBackendError::InvalidResult(
            "parametric spatial evidence must contain Front, Top, and Left".into(),
        ));
    }
    for kind in expected_views {
        let edges = evidence.views.get(&kind).ok_or_else(|| {
            CadBackendError::InvalidResult(format!("parametric spatial evidence omitted {kind:?}"))
        })?;
        if edges.is_empty() {
            return Err(CadBackendError::InvalidResult(format!(
                "parametric spatial evidence contains no projected edges for {kind:?}"
            )));
        }
        let mut ids = BTreeSet::new();
        for edge in edges {
            if edge.id.is_empty()
                || !ids.insert(edge.id.as_str())
                || !edge.depth_min.is_finite()
                || !edge.depth_max.is_finite()
                || edge.depth_min > edge.depth_max
                || edge.derived_approximation != edge.approximation_tolerance_mm.is_some()
                || edge
                    .approximation_tolerance_mm
                    .is_some_and(|value| !value.is_finite() || value <= 0.0)
            {
                return Err(CadBackendError::InvalidResult(format!(
                    "parametric projected edge is invalid in {kind:?}"
                )));
            }
            match edge.visibility {
                SpatialEdgeVisibility::Visible | SpatialEdgeVisibility::Hidden => {}
            }
            validate_projected_geometry(&edge.geometry)?;
        }
    }
    Ok(())
}

fn valid_provenance(sources: &[String], known: &BTreeSet<String>) -> bool {
    !sources.is_empty()
        && sources.iter().all(|source| known.contains(source))
        && sources
            .iter()
            .enumerate()
            .all(|(index, source)| !sources[..index].contains(source))
}

fn valid_spatial_edge_curve(curve: &SpatialKernelEdgeCurve) -> bool {
    let finite = |values: &[f64]| values.iter().all(|value| value.is_finite());
    match curve {
        SpatialKernelEdgeCurve::Line { start, end } => finite(start) && finite(end) && start != end,
        SpatialKernelEdgeCurve::Circular {
            center,
            axis,
            radius,
            start,
            end,
        } => {
            finite(center)
                && finite(axis)
                && finite(start)
                && finite(end)
                && radius.is_finite()
                && *radius > 0.0
        }
        SpatialKernelEdgeCurve::Elliptical {
            center,
            axis,
            major_axis,
            major_radius,
            minor_radius,
            start,
            end,
        } => {
            finite(center)
                && finite(axis)
                && finite(major_axis)
                && finite(start)
                && finite(end)
                && major_radius.is_finite()
                && *major_radius > 0.0
                && minor_radius.is_finite()
                && *minor_radius > 0.0
                && major_radius >= minor_radius
        }
        SpatialKernelEdgeCurve::SampledCurve {
            points,
            max_deviation_mm,
        } => {
            points.len() >= 2
                && points.len() <= 8192
                && points.iter().all(|point| finite(point))
                && points.windows(2).all(|pair| pair[0] != pair[1])
                && max_deviation_mm.is_finite()
                && *max_deviation_mm > 0.0
                && *max_deviation_mm <= 0.05
        }
    }
}

fn valid_spatial_face_surface(surface: &SpatialFaceSurface) -> bool {
    let finite = |values: &[f64]| values.iter().all(|value| value.is_finite());
    match surface {
        SpatialFaceSurface::Plane { origin, normal } => finite(origin) && finite(normal),
        SpatialFaceSurface::Cylinder {
            axis_origin,
            axis,
            radius,
        } => finite(axis_origin) && finite(axis) && radius.is_finite() && *radius > 0.0,
        SpatialFaceSurface::Cone {
            axis_origin,
            axis,
            reference_radius,
            semi_angle_radians,
        } => {
            finite(axis_origin)
                && finite(axis)
                && reference_radius.is_finite()
                && *reference_radius > 0.0
                && semi_angle_radians.is_finite()
                && semi_angle_radians.abs() > 0.0
                && semi_angle_radians.abs() < std::f64::consts::FRAC_PI_2
        }
        SpatialFaceSurface::Torus {
            center,
            axis,
            major_radius,
            minor_radius,
        } => {
            finite(center)
                && finite(axis)
                && major_radius.is_finite()
                && *major_radius > 0.0
                && minor_radius.is_finite()
                && *minor_radius > 0.0
                && *major_radius > *minor_radius
        }
        SpatialFaceSurface::SampledBsplineSurface {
            vertices,
            triangle_count,
            linear_deviation_mm,
            angular_tolerance_radians,
        } => {
            vertices.len() >= 3
                && vertices.len() <= 4096
                && vertices.iter().all(|point| finite(point))
                && *triangle_count > 0
                && *triangle_count <= 8192
                && linear_deviation_mm.is_finite()
                && *linear_deviation_mm > 0.0
                && *linear_deviation_mm <= 0.05
                && angular_tolerance_radians.is_finite()
                && *angular_tolerance_radians > 0.0
                && *angular_tolerance_radians <= 0.1
        }
    }
}

fn validate_frame(
    origin: [f64; 3],
    u_axis: [f64; 3],
    v_axis: [f64; 3],
    axis: [f64; 3],
) -> Result<(), CadBackendError> {
    let finite = origin
        .iter()
        .chain(u_axis.iter())
        .chain(v_axis.iter())
        .chain(axis.iter())
        .all(|value| value.is_finite());
    let dot =
        |left: [f64; 3], right: [f64; 3]| left.iter().zip(right).map(|(a, b)| a * b).sum::<f64>();
    let norm = |value: [f64; 3]| dot(value, value).sqrt();
    let unit = |value: [f64; 3]| (norm(value) - 1.0).abs() <= 1.0e-8;
    if !finite
        || !unit(u_axis)
        || !unit(v_axis)
        || !unit(axis)
        || dot(u_axis, v_axis).abs() > 1.0e-8
        || dot(u_axis, axis).abs() > 1.0e-8
        || dot(v_axis, axis).abs() > 1.0e-8
    {
        return Err(CadBackendError::InvalidResult(
            "parametric body frame is not finite and orthonormal".into(),
        ));
    }
    Ok(())
}

fn validate_projected_geometry(value: &SpatialProjectedGeometry) -> Result<(), CadBackendError> {
    let finite = match value {
        SpatialProjectedGeometry::Segment { start, end } => {
            start.x.is_finite()
                && start.y.is_finite()
                && end.x.is_finite()
                && end.y.is_finite()
                && (start.x != end.x || start.y != end.y)
        }
        SpatialProjectedGeometry::Circle { center, radius } => {
            center.x.is_finite() && center.y.is_finite() && radius.is_finite() && *radius > 0.0
        }
        SpatialProjectedGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            center.x.is_finite()
                && center.y.is_finite()
                && radius.is_finite()
                && *radius > 0.0
                && start_angle.is_finite()
                && end_angle.is_finite()
                && start_angle != end_angle
        }
    };
    if finite {
        Ok(())
    } else {
        Err(CadBackendError::InvalidResult(
            "parametric worker returned malformed projected geometry".into(),
        ))
    }
}

impl CadBackendClient {
    /// Rebuild an ordered generic extrude history in the stateless kernel.
    pub fn execute_parametric_section(
        &self,
        request: &ParametricRebuildRequest,
        plane: ParametricSectionPlane,
    ) -> Result<(ParametricRebuildResult, ParametricSectionGeometry), CadBackendError> {
        request.validate()?;
        if !plane.offset_mm.is_finite() {
            return Err(CadBackendError::InvalidResult(
                "section plane offset must be finite".into(),
            ));
        }
        let input = serde_json::to_vec(&serde_json::json!({
            "action": "section",
            "rebuild": request,
            "section_plane": plane,
        }))?;
        let started = std::time::Instant::now();
        let output = self.invoke_worker(&input)?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let wire: ParametricSectionWireResponse =
            serde_json::from_slice(&output).map_err(CadBackendError::Protocol)?;
        if !wire.ok {
            return Err(CadBackendError::Backend(wire.error.ok_or_else(|| {
                CadBackendError::InvalidResult("failed section rebuild omitted error".into())
            })?));
        }
        let mut payload = wire.result.ok_or_else(|| {
            CadBackendError::InvalidResult("successful section rebuild omitted result".into())
        })?;
        validate_parametric_result(request, &payload.rebuild)?;
        validate_section_geometry(request, plane, &payload.section_geometry)?;
        payload.rebuild.worker_elapsed_ms = elapsed_ms;
        Ok((payload.rebuild, payload.section_geometry))
    }

    /// Rebuild an ordered generic extrude history in the stateless kernel.
    pub fn execute_parametric_rebuild(
        &self,
        request: &ParametricRebuildRequest,
    ) -> Result<ParametricRebuildResult, CadBackendError> {
        request.validate()?;
        let input = serde_json::to_vec(request)?;
        let started = std::time::Instant::now();
        let output = self.invoke_worker(&input)?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let wire: ParametricWireResponse =
            serde_json::from_slice(&output).map_err(CadBackendError::Protocol)?;
        let mut result = wire.into_result()?;
        result.worker_elapsed_ms = elapsed_ms;
        validate_parametric_result(request, &result)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        expected_section_frame, valid_spatial_edge_curve, validate_section_geometry,
        ParametricEdgeReference, ParametricRebuildRequest, ParametricSectionAxis,
        ParametricSectionGeometry, ParametricSectionPlane, ParametricSectionViewSide,
    };

    #[test]
    fn section_response_must_match_the_requested_principal_plane_frame() {
        let request = ParametricRebuildRequest {
            schema_version: 3,
            model_sha256: "a".repeat(64),
            operations: Vec::new(),
        };
        for axis in [
            ParametricSectionAxis::X,
            ParametricSectionAxis::Y,
            ParametricSectionAxis::Z,
        ] {
            for view_side in [
                ParametricSectionViewSide::Positive,
                ParametricSectionViewSide::Negative,
            ] {
                let expected = ParametricSectionPlane {
                    axis,
                    offset_mm: 12.5,
                    view_side,
                };
                let geometry = ParametricSectionGeometry {
                    schema_version: 1,
                    model_sha256: request.model_sha256.clone(),
                    plane: expected_section_frame(expected),
                    loops: vec![vec![[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]]],
                    edge_count: 3,
                    sample_chord_tolerance_mm: 0.01,
                    geometry_elapsed_ms: 1.0,
                };
                assert!(validate_section_geometry(&request, expected, &geometry).is_ok());

                let mut wrong_origin = geometry.clone();
                wrong_origin.plane.origin[0] += 1.0;
                assert!(validate_section_geometry(&request, expected, &wrong_origin).is_err());

                let mut wrong_normal = geometry.clone();
                wrong_normal.plane.normal[0] += 0.25;
                assert!(validate_section_geometry(&request, expected, &wrong_normal).is_err());

                let mut wrong_basis = geometry;
                wrong_basis.plane.u_axis[0] += 0.25;
                assert!(validate_section_geometry(&request, expected, &wrong_basis).is_err());
            }
        }
    }

    #[test]
    fn sampled_spatial_edge_curve_requires_finite_bounded_deflection() {
        let valid = scc_spatial::SpatialKernelEdgeCurve::SampledCurve {
            points: vec![[0.0, 0.0, 0.0], [1.0, 0.5, 0.0], [2.0, 0.0, 0.0]],
            max_deviation_mm: 0.001,
        };
        assert!(valid_spatial_edge_curve(&valid));
        for invalid in [
            scc_spatial::SpatialKernelEdgeCurve::SampledCurve {
                points: vec![[0.0, 0.0, 0.0], [f64::NAN, 1.0, 0.0]],
                max_deviation_mm: 0.001,
            },
            scc_spatial::SpatialKernelEdgeCurve::SampledCurve {
                points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
                max_deviation_mm: 0.0501,
            },
            scc_spatial::SpatialKernelEdgeCurve::SampledCurve {
                points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
                max_deviation_mm: 0.001,
            },
        ] {
            assert!(!valid_spatial_edge_curve(&invalid));
        }
    }

    #[test]
    fn semantic_edge_wire_schema_rejects_kernel_indices_at_every_level() {
        let top_level_index = serde_json::json!({
            "source_feature_id": "boss",
            "selector": { "kind": "profile_boundary", "entity_id": "edge-1", "side": "top" },
            "qualifier": {
                "curve_kind": "line",
                "radius_mm": null,
                "length_mm": null,
                "midpoint": null,
                "tolerance_mm": 0.000001
            },
            "edge_index": 7
        });
        assert!(serde_json::from_value::<ParametricEdgeReference>(top_level_index).is_err());

        let nested_index = serde_json::json!({
            "source_feature_id": "boss",
            "selector": { "kind": "profile_boundary", "entity_id": "edge-1", "side": "top", "edge_index": 7 },
            "qualifier": {
                "curve_kind": "line",
                "radius_mm": null,
                "length_mm": null,
                "midpoint": null,
                "tolerance_mm": 0.000001
            }
        });
        assert!(serde_json::from_value::<ParametricEdgeReference>(nested_index).is_err());
    }
}
