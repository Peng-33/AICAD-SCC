//! Read-only value types of the 2.5-D spatial model (Phase 9.2, ADR-033).
//!
//! [`SpatialModel`] is a pure derived artifact: it is assembled once by
//! [`crate::lowering::lower_spatial`] and afterwards only read. There are no
//! mutators, no second editable truth, and no solver state. Identifiers are
//! deterministic strings (`SpatialCutId`) — never parsed back for meaning and
//! never randomly generated (ADR-014 / ADR-033 Decision 9).

use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_engineering::{FeatureId, HoleStartFace};
use scc_geometry::Point2;
use serde::{Deserialize, Serialize};

use crate::error::SpatialError;

/// A stable, deterministic identifier of one spatial cut.
///
/// Allocated as `{feature_id}:cut:{index:04}` (ADR-033 Decision 9). The id is
/// opaque: consumers must never parse it to recover feature or index — use
/// [`CylindricalCut::source_feature`] / [`CylindricalCut::source_entity`]
/// instead.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SpatialCutId(pub String);

impl std::fmt::Display for SpatialCutId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A closed interval on the canonical +Z axis (millimetres).
///
/// Valid intervals are finite, strictly non-empty (`max > min`), and start at
/// or above the base plane (`min >= 0`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZInterval {
    pub min: f64,
    pub max: f64,
}

impl ZInterval {
    /// Validate and construct a Z interval.
    pub fn new(min: f64, max: f64) -> Result<Self, SpatialError> {
        if !min.is_finite() || !max.is_finite() || max <= min || min < 0.0 {
            return Err(SpatialError::InvalidSpatialInterval {
                z_min: min,
                z_max: max,
            });
        }
        Ok(Self { min, max })
    }
}

/// The axis-aligned 3-D envelope of the spatial model (millimetres).
///
/// Derived from the body footprint only; cuts never expand it
/// (ADR-033 Decision 8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds3D {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl Bounds3D {
    /// Validate and construct a bounds envelope; every axis must be finite
    /// with `max > min`.
    pub fn new(min: [f64; 3], max: [f64; 3]) -> Result<Self, SpatialError> {
        let valid = min.iter().all(|v| v.is_finite())
            && max.iter().all(|v| v.is_finite())
            && max[0] > min[0]
            && max[1] > min[1]
            && max[2] > min[2];
        if !valid {
            return Err(SpatialError::InvalidSpatialBounds { min, max });
        }
        Ok(Self {
            min_x: min[0],
            max_x: max[0],
            min_y: min[1],
            max_y: max[1],
            min_z: min[2],
            max_z: max[2],
        })
    }
}

/// Direction used by an oriented profile arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialArcDirection {
    CounterClockwise,
    Clockwise,
}

/// One solved boundary segment of a composite spatial footprint.
///
/// The source entity and segment index are carried with the derived segment so
/// a composite loop can retain deterministic provenance without parsing entity
/// identifiers. Angles are radians and use the same +X reference as
/// `GeometryPrimitive::Arc`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpatialProfileSegment {
    Line {
        start: Point2,
        end: Point2,
        source_entity: EntityId,
        segment_index: u32,
    },
    Arc {
        start: Point2,
        end: Point2,
        center: Point2,
        radius: f64,
        direction: SpatialArcDirection,
        source_entity: EntityId,
        segment_index: u32,
    },
}

impl SpatialProfileSegment {
    pub fn start(&self) -> Point2 {
        match self {
            Self::Line { start, .. } => *start,
            Self::Arc { start, .. } => *start,
        }
    }

    pub fn end(&self) -> Point2 {
        match self {
            Self::Line { end, .. } => *end,
            Self::Arc { end, .. } => *end,
        }
    }

    pub fn source_entity(&self) -> &EntityId {
        match self {
            Self::Line { source_entity, .. } | Self::Arc { source_entity, .. } => source_entity,
        }
    }

    pub fn segment_index(&self) -> u32 {
        match self {
            Self::Line { segment_index, .. } | Self::Arc { segment_index, .. } => *segment_index,
        }
    }

    /// Exact axis-aligned bounds of the finite line or directed arc.
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        match self {
            Self::Line { start, end, .. } => Some((
                start.x.min(end.x),
                start.x.max(end.x),
                start.y.min(end.y),
                start.y.max(end.y),
            )),
            Self::Arc {
                center,
                radius,
                start,
                end,
                direction,
                ..
            } => {
                if !center.x.is_finite()
                    || !center.y.is_finite()
                    || !radius.is_finite()
                    || *radius <= 0.0
                    || !finite_point(*start)
                    || !finite_point(*end)
                {
                    return None;
                }
                let start_angle = (start.y - center.y).atan2(start.x - center.x);
                let end_angle = (end.y - center.y).atan2(end.x - center.x);
                let mut points = vec![*start, *end];
                for angle in [
                    0.0,
                    std::f64::consts::FRAC_PI_2,
                    std::f64::consts::PI,
                    3.0 * std::f64::consts::FRAC_PI_2,
                ] {
                    if angle_on_directed_arc(start_angle, end_angle, *direction, angle) {
                        points.push(point_on_circle(*center, *radius, angle));
                    }
                }
                let first = points.first()?;
                let (mut min_x, mut max_x, mut min_y, mut max_y) =
                    (first.x, first.x, first.y, first.y);
                for point in points.iter().skip(1) {
                    min_x = min_x.min(point.x);
                    max_x = max_x.max(point.x);
                    min_y = min_y.min(point.y);
                    max_y = max_y.max(point.y);
                }
                Some((min_x, max_x, min_y, max_y))
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        let start = self.start();
        let end = self.end();
        match self {
            Self::Line { .. } => finite_point(start) && finite_point(end) && start != end,
            Self::Arc { center, radius, .. } => {
                finite_point(*center)
                    && radius.is_finite()
                    && *radius > 0.0
                    && finite_point(start)
                    && finite_point(end)
                    && (distance(*center, start) - *radius).abs() <= PROFILE_TOLERANCE
                    && (distance(*center, end) - *radius).abs() <= PROFILE_TOLERANCE
                    && start != end
                    && directed_sweep(
                        angle_of(*center, start),
                        angle_of(*center, end),
                        self.direction(),
                    ) > 0.0
                    && directed_sweep(
                        angle_of(*center, start),
                        angle_of(*center, end),
                        self.direction(),
                    ) < std::f64::consts::TAU
            }
        }
    }

    fn direction(&self) -> SpatialArcDirection {
        match self {
            Self::Line { .. } => SpatialArcDirection::CounterClockwise,
            Self::Arc { direction, .. } => *direction,
        }
    }
}

/// The solved 2-D outline of the spatial body. Rectangle and Circle are exact
/// profile variants; Composite preserves one ordered outer loop of exact line
/// and arc segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpatialFootprint {
    Rectangle {
        origin: Point2,
        width: f64,
        height: f64,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
    Composite {
        segments: Vec<SpatialProfileSegment>,
    },
    /// Exact read-only evidence extracted from a validated parametric B-Rep.
    /// This variant is derived from EngineeringModel history and is never an
    /// editable shape or manufacturing/STEP authority.
    Parametric {
        evidence: Box<ParametricSpatialEvidence>,
    },
}

/// Typed identity of the resolved engineering base profile.
///
/// `SpatialFootprint::Composite` remains the exact boundary representation
/// used by projection and drawing. This parallel typed identity preserves the
/// RoundedRectangle semantics for manufacturing without asking downstream
/// consumers to infer them from line/arc segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum SpatialBaseProfile {
    Rectangle {
        origin: Point2,
        width: f64,
        height: f64,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
    RoundedRectangle {
        center: Point2,
        width: f64,
        height: f64,
        corner_radius: f64,
    },
    Composite,
    Polygon,
    /// Exact body came from an ordered parametric feature-history rebuild.
    Parametric,
    /// Compatibility marker for SpatialModel JSON written before typed
    /// profile identity existed. It is never emitted by fresh lowering.
    #[default]
    Legacy,
}

/// One of the canonical views named by the Spatial → Projection contract.
/// This belongs to spatial kernel evidence to avoid a dependency from
/// `scc-spatial` back to `scc-projection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialPrincipalView {
    Front,
    Top,
    Left,
}

/// Whether a B-Rep edge is visible in one orthographic view. This is raw
/// kernel visibility evidence; the `VisibilityModel` remains the formal
/// downstream sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialEdgeVisibility {
    Visible,
    Hidden,
}

/// Exact line, circle, or circular-arc projection evidence in view space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpatialProjectedGeometry {
    Segment {
        start: Point2,
        end: Point2,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
    Arc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
}

/// Exact geometric evidence for one final B-Rep edge. IDs are derived from
/// canonical geometry descriptions, never from a kernel edge ordinal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialEdgeEvidence {
    pub id: String,
    pub curve: SpatialKernelEdgeCurve,
    pub length_mm: f64,
    /// Derived source features inferred from ordered rebuild topology deltas.
    #[serde(default)]
    pub source_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpatialKernelEdgeCurve {
    Line {
        start: [f64; 3],
        end: [f64; 3],
    },
    Circular {
        center: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        start: [f64; 3],
        end: [f64; 3],
    },
    Elliptical {
        center: [f64; 3],
        axis: [f64; 3],
        major_axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
        start: [f64; 3],
        end: [f64; 3],
    },
    /// A non-analytic OCP boundary curve sampled with a bounded maximum
    /// deflection of at most 0.05 mm. The source solid remains exact; this representation is for
    /// stable evidence and bounded projection only.
    SampledCurve {
        points: Vec<[f64; 3]>,
        max_deviation_mm: f64,
    },
}

/// Analytic supporting surface information for one final B-Rep face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpatialFaceSurface {
    Plane {
        origin: [f64; 3],
        normal: [f64; 3],
    },
    Cylinder {
        axis_origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Cone {
        axis_origin: [f64; 3],
        axis: [f64; 3],
        reference_radius: f64,
        semi_angle_radians: f64,
    },
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
    /// A transient Ruled Loft side patch represented by bounded OCP face
    /// tessellation. This is derived drawing evidence, not an editable NURBS
    /// surface or a semantic topology selector.
    SampledBsplineSurface {
        vertices: Vec<[f64; 3]>,
        triangle_count: usize,
        linear_deviation_mm: f64,
        angular_tolerance_radians: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialFaceEvidence {
    pub id: String,
    pub surface: SpatialFaceSurface,
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub area_mm2: f64,
    /// Derived source features inferred from ordered rebuild topology deltas.
    #[serde(default)]
    pub source_features: Vec<String>,
}

/// One exact raw orthographic edge emitted by the kernel's HLR calculation.
/// Projection converts the curve to its owned model and Visibility converts
/// the kernel classification to the formal sidecar; drawing consumes neither
/// this record nor the CAD kernel directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialProjectedEdgeEvidence {
    pub id: String,
    pub geometry: SpatialProjectedGeometry,
    pub depth_min: f64,
    pub depth_max: f64,
    pub visibility: SpatialEdgeVisibility,
    /// True only when an HLR spline was restored to an analytic Segment/Arc
    /// inside the recorded millimetre tolerance.
    #[serde(default)]
    pub derived_approximation: bool,
    #[serde(default)]
    pub approximation_tolerance_mm: Option<f64>,
}

/// The common axis-aligned profile frame used by one single-body rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpatialBodyFrame {
    pub origin: [f64; 3],
    pub u_axis: [f64; 3],
    pub v_axis: [f64; 3],
    pub axis: [f64; 3],
    pub axis_min: f64,
    pub axis_max: f64,
}

/// Validated immutable output of a complete parametric-history rebuild.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricSpatialEvidence {
    pub model_sha256: String,
    pub frame: SpatialBodyFrame,
    pub volume_mm3: f64,
    pub operation_features: Vec<FeatureId>,
    pub edges: Vec<SpatialEdgeEvidence>,
    pub faces: Vec<SpatialFaceEvidence>,
    pub views: BTreeMap<SpatialPrincipalView, Vec<SpatialProjectedEdgeEvidence>>,
}

fn finite_point(point: Point2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn point_on_circle(center: Point2, radius: f64, angle: f64) -> Point2 {
    Point2 {
        x: center.x + radius * angle.cos(),
        y: center.y + radius * angle.sin(),
    }
}

const PROFILE_TOLERANCE: f64 = 1.0e-9;

fn distance(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn angle_of(center: Point2, point: Point2) -> f64 {
    (point.y - center.y).atan2(point.x - center.x)
}

fn normalize_angle(angle: f64) -> f64 {
    angle.rem_euclid(std::f64::consts::TAU)
}

fn directed_sweep(start: f64, end: f64, direction: SpatialArcDirection) -> f64 {
    match direction {
        SpatialArcDirection::CounterClockwise => (end - start).rem_euclid(std::f64::consts::TAU),
        SpatialArcDirection::Clockwise => (start - end).rem_euclid(std::f64::consts::TAU),
    }
}

fn angle_on_directed_arc(
    start: f64,
    end: f64,
    direction: SpatialArcDirection,
    candidate: f64,
) -> bool {
    let sweep = directed_sweep(start, end, direction);
    if sweep <= 0.0 || sweep >= std::f64::consts::TAU {
        return false;
    }
    let delta = match direction {
        SpatialArcDirection::CounterClockwise => {
            (normalize_angle(candidate) - normalize_angle(start)).rem_euclid(std::f64::consts::TAU)
        }
        SpatialArcDirection::Clockwise => {
            (normalize_angle(start) - normalize_angle(candidate)).rem_euclid(std::f64::consts::TAU)
        }
    };
    delta <= sweep
}

/// How deep a cut reaches on the +Z axis. Mirrors the engineering
/// [`scc_engineering::HoleExtent`] intent with the depth already resolved and
/// turned into a validated [`ZInterval`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpatialCutKind {
    /// The cut passes entirely through the plate.
    ThroughAll,
    /// The cut stops after a resolved blind depth from `start_face`.
    Blind { start_face: HoleStartFace },
}

/// The extruded base profile: the plate solid over `z = [0, thickness]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialBody {
    pub footprint: SpatialFootprint,
    #[serde(default)]
    pub profile: SpatialBaseProfile,
    pub z: ZInterval,
    pub source_feature: FeatureId,
    pub source_entity: EntityId,
}

/// One cylindrical (round) cut through or into the plate, witnessed by one
/// solved geometry circle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CylindricalCut {
    pub center: Point2,
    pub radius: f64,
    pub z: ZInterval,
    pub kind: SpatialCutKind,
    pub source_feature: FeatureId,
    pub source_entity: EntityId,
}

/// Exact through-slot cut: a Line/Arc stadium extruded on Z, never a bounding
/// rectangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotCut {
    pub center: Point2,
    pub overall_length: f64,
    pub width: f64,
    pub orientation_degrees: f64,
    pub segments: Vec<SpatialProfileSegment>,
    pub z: ZInterval,
    pub kind: SpatialCutKind,
    pub source_feature: FeatureId,
}

/// The provenance of a spatial model: which geometry generation it was
/// lowered from. Purely informational; re-lowering is always from an explicit
/// [`crate::SpatialInputs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpatialModelSource {
    pub geometry_source_root: u64,
    pub geometry_source_step: u64,
}

/// The derived, read-only 2.5-D spatial model (Phase 9.2, ADR-033).
///
/// Assembled once by [`crate::lowering::lower_spatial`]; consumers query it
/// but never mutate it. Cuts and construction circles are stored in
/// `BTreeMap`s so serialization order is deterministic (byte-stable JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialModel {
    /// The extruded base profile (the plate solid).
    pub body: SpatialBody,
    /// All cylindrical cuts, keyed by deterministic [`SpatialCutId`].
    pub cuts: BTreeMap<SpatialCutId, CylindricalCut>,
    /// Exact through-slot cuts. Empty on models without slots; omitted from
    /// legacy JSON.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slot_cuts: BTreeMap<SpatialCutId, SlotCut>,
    /// The bounds envelope derived from the body footprint only.
    pub bounds: Bounds3D,
    /// Reference-only circles: geometry entity → owning feature.
    pub construction_circles: BTreeMap<EntityId, FeatureId>,
    /// Which geometry generation this model was lowered from.
    pub source: SpatialModelSource,
}

impl SpatialModel {
    /// The cut witnessed by `entity`, if any. Deterministic: cuts are unique
    /// per source entity (claim integrity, ADR-033 Decision 6).
    pub fn cut_for_entity(&self, entity: &EntityId) -> Option<&CylindricalCut> {
        self.cuts.values().find(|cut| &cut.source_entity == entity)
    }

    /// All cuts of one feature in deterministic (cut-id ascending) order.
    pub fn cuts_for_feature(&self, feature: &FeatureId) -> Vec<(&SpatialCutId, &CylindricalCut)> {
        self.cuts
            .iter()
            .filter(|(_, cut)| &cut.source_feature == feature)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpatialError;

    fn fid(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    fn entity(id: &str) -> EntityId {
        EntityId(id.into())
    }

    fn point(x: f64, y: f64) -> Point2 {
        Point2 { x, y }
    }

    fn body() -> SpatialBody {
        SpatialBody {
            footprint: SpatialFootprint::Rectangle {
                origin: point(-60.0, -40.0),
                width: 120.0,
                height: 80.0,
            },
            profile: SpatialBaseProfile::Rectangle {
                origin: point(-60.0, -40.0),
                width: 120.0,
                height: 80.0,
            },
            z: ZInterval::new(0.0, 10.0).unwrap(),
            source_feature: fid("plate"),
            source_entity: entity("plate"),
        }
    }

    fn cut(id: &str, feature: &str, source: &str) -> (SpatialCutId, CylindricalCut) {
        (
            SpatialCutId(id.into()),
            CylindricalCut {
                center: point(0.0, 0.0),
                radius: 5.0,
                z: ZInterval::new(0.0, 10.0).unwrap(),
                kind: SpatialCutKind::ThroughAll,
                source_feature: fid(feature),
                source_entity: entity(source),
            },
        )
    }

    fn sample_model() -> SpatialModel {
        let mut cuts = BTreeMap::new();
        cuts.extend([cut("b_pattern:cut:0000", "b_pattern", "b_pattern[0]")]);
        cuts.extend([cut("a_hole:cut:0000", "a_hole", "a_hole")]);
        let mut construction_circles = BTreeMap::new();
        construction_circles.insert(entity("cc"), fid("cc"));
        SpatialModel {
            body: body(),
            cuts,
            slot_cuts: BTreeMap::new(),
            bounds: Bounds3D::new([-60.0, -40.0, 0.0], [60.0, 40.0, 10.0]).unwrap(),
            construction_circles,
            source: SpatialModelSource {
                geometry_source_root: 1,
                geometry_source_step: 7,
            },
        }
    }

    // ---- ZInterval accept / reject tables ----

    #[test]
    fn z_interval_accepts_valid_intervals() {
        for (min, max) in [(0.0, 10.0), (6.0, 10.0), (0.0, f64::MAX), (2.5, 2.75)] {
            let z = ZInterval::new(min, max).unwrap_or_else(|e| panic!("{min}/{max}: {e:?}"));
            assert_eq!(z.min, min);
            assert_eq!(z.max, max);
        }
    }

    #[test]
    fn z_interval_rejects_invalid_intervals() {
        for (min, max) in [
            (10.0, 10.0), // empty
            (10.0, 0.0),  // inverted
            (-1.0, 5.0),  // below the base plane
            (0.0, f64::INFINITY),
            (f64::NEG_INFINITY, 5.0),
        ] {
            assert_eq!(
                ZInterval::new(min, max),
                Err(SpatialError::InvalidSpatialInterval {
                    z_min: min,
                    z_max: max
                }),
                "{min}/{max} must be rejected"
            );
        }
        // NaN payloads never compare equal, so assert by variant shape.
        for (min, max) in [(f64::NAN, 5.0), (0.0, f64::NAN)] {
            assert!(
                matches!(
                    ZInterval::new(min, max),
                    Err(SpatialError::InvalidSpatialInterval { .. })
                ),
                "{min}/{max} must be rejected"
            );
        }
    }

    // ---- Bounds3D accept / reject tables ----

    #[test]
    fn bounds_accepts_valid_envelopes() {
        for (min, max) in [
            ([-60.0, -40.0, 0.0], [60.0, 40.0, 10.0]),
            ([0.0, 0.0, 0.0], [0.5, 0.5, 0.5]),
        ] {
            let b = Bounds3D::new(min, max).unwrap();
            assert_eq!(b.min_x, min[0]);
            assert_eq!(b.max_x, max[0]);
            assert_eq!(b.min_y, min[1]);
            assert_eq!(b.max_y, max[1]);
            assert_eq!(b.min_z, min[2]);
            assert_eq!(b.max_z, max[2]);
        }
    }

    #[test]
    fn bounds_rejects_invalid_envelopes() {
        for (min, max) in [
            ([0.0, 0.0, 0.0], [0.0, 1.0, 1.0]),           // degenerate X
            ([0.0, 0.0, 0.0], [1.0, 0.0, 1.0]),           // degenerate Y
            ([0.0, 0.0, 0.0], [1.0, 1.0, 0.0]),           // degenerate Z
            ([1.0, 0.0, 0.0], [0.0, 1.0, 1.0]),           // inverted X
            ([0.0, 0.0, 0.0], [f64::INFINITY, 1.0, 1.0]), // non-finite max
        ] {
            assert_eq!(
                Bounds3D::new(min, max),
                Err(SpatialError::InvalidSpatialBounds { min, max }),
                "{min:?}/{max:?} must be rejected"
            );
        }
        // NaN payloads never compare equal, so assert by variant shape.
        assert!(matches!(
            Bounds3D::new([f64::NAN, 0.0, 0.0], [1.0, 1.0, 1.0]),
            Err(SpatialError::InvalidSpatialBounds { .. })
        ));
    }

    // ---- SpatialCutId ----

    #[test]
    fn cut_ids_order_by_string_not_allocation() {
        let a = SpatialCutId("a_hole:cut:0000".into());
        let b = SpatialCutId("b_pattern:cut:0000".into());
        let a1 = SpatialCutId("a_hole:cut:0001".into());
        assert!(a < a1);
        assert!(a1 < b);
        assert_eq!(SpatialCutId("x".into()).to_string(), "x");
    }

    #[test]
    fn cut_ids_roundtrip_through_serde() {
        let id = SpatialCutId("bolt_pattern:cut:0007".into());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"bolt_pattern:cut:0007\"");
        assert_eq!(serde_json::from_str::<SpatialCutId>(&json).unwrap(), id);
    }

    // ---- SpatialModel ----

    #[test]
    fn spatial_model_roundtrips_through_serde() {
        let model = sample_model();
        let json = serde_json::to_string(&model).unwrap();
        let deserialized: SpatialModel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, model);
    }

    #[test]
    fn serialized_cut_keys_are_in_ascending_order() {
        let model = sample_model();
        let json = serde_json::to_string(&model).unwrap();
        let a = json.find("\"a_hole:cut:0000\"").unwrap();
        let b = json.find("\"b_pattern:cut:0000\"").unwrap();
        assert!(a < b, "JSON keys must appear in ascending cut-id order");
    }

    #[test]
    fn cut_for_entity_finds_the_owning_cut() {
        let model = sample_model();
        let found = model.cut_for_entity(&entity("b_pattern[0]")).unwrap();
        assert_eq!(found.source_feature, fid("b_pattern"));
        assert!(model.cut_for_entity(&entity("ghost")).is_none());
        assert!(model.cut_for_entity(&entity("cc")).is_none());
    }

    #[test]
    fn cuts_for_feature_returns_deterministic_order() {
        let mut model = sample_model();
        model
            .cuts
            .extend([cut("a_hole:cut:0001", "a_hole", "extra")]);
        let cuts = model.cuts_for_feature(&fid("a_hole"));
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].0 .0, "a_hole:cut:0000");
        assert_eq!(cuts[1].0 .0, "a_hole:cut:0001");
        assert!(model.cuts_for_feature(&fid("missing")).is_empty());
    }

    #[test]
    fn composite_arc_bounds_include_reachable_cardinal_extrema() {
        let arc = SpatialProfileSegment::Arc {
            start: point(1.0, 0.0),
            end: point(0.0, 1.0),
            center: point(0.0, 0.0),
            radius: 1.0,
            direction: SpatialArcDirection::Clockwise,
            source_entity: entity("corner_arc"),
            segment_index: 3,
        };
        assert!(arc.is_valid());
        assert_eq!(arc.source_entity(), &entity("corner_arc"));
        assert_eq!(arc.segment_index(), 3);
        assert_eq!(arc.bounds(), Some((-1.0, 1.0, -1.0, 1.0)));
    }

    #[test]
    fn composite_preserves_ordered_segment_provenance_through_serde() {
        let footprint = SpatialFootprint::Composite {
            segments: vec![
                SpatialProfileSegment::Line {
                    start: point(-1.0, -1.0),
                    end: point(1.0, -1.0),
                    source_entity: entity("edge_0"),
                    segment_index: 0,
                },
                SpatialProfileSegment::Line {
                    start: point(1.0, -1.0),
                    end: point(1.0, 1.0),
                    source_entity: entity("edge_1"),
                    segment_index: 1,
                },
            ],
        };
        let json = serde_json::to_string(&footprint).unwrap();
        let decoded: SpatialFootprint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, footprint);
        let SpatialFootprint::Composite { segments } = decoded else {
            panic!("expected composite footprint");
        };
        assert_eq!(segments[0].segment_index(), 0);
        assert_eq!(segments[1].source_entity(), &entity("edge_1"));
    }
}
