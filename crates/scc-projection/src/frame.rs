//! Frozen canonical orthographic view frames and the projection math that
//! goes with them (Phase 9.3, ADR-034).
//!
//! A frame is three named [`AxisDirection`]s (`right`, `up`,
//! `toward_viewer`) with `right × up == toward_viewer` — no matrices, no
//! floats, no camera state. Projection of a point is a signed coordinate
//! read per named axis, implemented once and generically over the frame.
//! The only frames that can exist are the three canonical constructors
//! [`OrthographicFrame::front`] / [`OrthographicFrame::top`] /
//! [`OrthographicFrame::left`] (the frozen §2.1 table); `validate` exists as
//! a defensive gate for deserialized values.
//!
//! Depth convention: `depth = coordinate along toward_viewer`; LARGER depth
//! is NEARER the viewer.

use scc_spatial::Bounds3D;
use serde::{Deserialize, Serialize};

use crate::error::{InvalidProjectionGeometryReason, ProjectionError};
use crate::model::ProjectionDepthRange;

/// A canonical part-local axis direction. Exact enum, no floats (ADR-034
/// Decision 2/7): frames are compared, ordered, and cross-producted by name,
/// never by numeric vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisDirection {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

/// Which canonical principal view a frame belongs to.
///
/// Discriminant order IS the canonical order `Front < Top < Left` (ADR-034
/// Decision 5) — contractual, locked by
/// `principal_view_kind_order_is_front_top_left`; never left to alphabetical
/// accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalViewKind {
    Front,
    Top,
    Left,
}

impl PrincipalViewKind {
    /// The snake_case spelling used in deterministic primitive ids
    /// (`{view}:{source}:p:{index:04}`) and in serde payloads.
    pub(crate) fn id_spelling(self) -> &'static str {
        match self {
            PrincipalViewKind::Front => "front",
            PrincipalViewKind::Top => "top",
            PrincipalViewKind::Left => "left",
        }
    }
}

/// A right-handed orthonormal view basis: `right × up == toward_viewer`.
/// Three named axes only — no matrices, no floats (ADR-034 Decision 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrthographicFrame {
    /// The canonical view this frame belongs to.
    pub kind: PrincipalViewKind,
    /// The view's horizontal axis (increasing `u`).
    pub right: AxisDirection,
    /// The view's vertical axis (increasing `v`).
    pub up: AxisDirection,
    /// The direction from the part toward the viewer (increasing depth).
    pub toward_viewer: AxisDirection,
}

/// Minimal 3-D point in the canonical part-local frame (mm, ADR-032): the XY
/// plane at `z = 0` is the Base face, +Z points out of the material.
/// Deliberately minimal — no arithmetic API, not promoted to `scc-geometry`
/// (ADR-034 Decision 7).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PartLocalPoint3 {
    /// X coordinate (mm).
    pub x: f64,
    /// Y coordinate (mm).
    pub y: f64,
    /// Z coordinate (mm).
    pub z: f64,
}

/// A projected point. `depth` follows the frozen convention: LARGER depth =
/// NEARER viewer (§2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectedPoint {
    /// Horizontal view coordinate (mm).
    pub u: f64,
    /// Vertical view coordinate (mm).
    pub v: f64,
    /// Signed distance along `toward_viewer` (mm); larger = nearer viewer.
    pub depth: f64,
}

/// A 2-D projected coordinate (no depth) — the drawing-relevant part.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectedPoint2 {
    /// Horizontal view coordinate (mm).
    pub u: f64,
    /// Vertical view coordinate (mm).
    pub v: f64,
}

impl OrthographicFrame {
    /// The frozen Front frame: viewer on the +Z side looking −Z; `u = +x`,
    /// `v = +y`, `d = +z` (§2.1).
    pub fn front() -> Self {
        Self {
            kind: PrincipalViewKind::Front,
            right: AxisDirection::PosX,
            up: AxisDirection::PosY,
            toward_viewer: AxisDirection::PosZ,
        }
    }

    /// The frozen Top frame: viewer on the +Y side looking −Y; `u = +x`,
    /// `v = −z`, `d = +y` — paper-up on the Top view is −Z, so material
    /// depth runs down the paper (§2.1).
    pub fn top() -> Self {
        Self {
            kind: PrincipalViewKind::Top,
            right: AxisDirection::PosX,
            up: AxisDirection::NegZ,
            toward_viewer: AxisDirection::PosY,
        }
    }

    /// The frozen Left frame: viewer on the −X side looking +X; `u = +z`,
    /// `v = +y`, `d = −x` — larger x is farther, hence depth = −x (§2.1).
    pub fn left() -> Self {
        Self {
            kind: PrincipalViewKind::Left,
            right: AxisDirection::PosZ,
            up: AxisDirection::PosY,
            toward_viewer: AxisDirection::NegX,
        }
    }

    /// Exact named-axis cross product (integer match, total over the 36
    /// ordered pairs). `Some` only when the axes are perpendicular and
    /// distinct, per the right-hand rule; parallel or equal axes yield
    /// `None`.
    pub fn cross(a: AxisDirection, b: AxisDirection) -> Option<AxisDirection> {
        use AxisDirection::*;
        match (a, b) {
            // x̂ × ±ŷ = ±ẑ
            (PosX, PosY) => Some(PosZ),
            (PosX, NegY) => Some(NegZ),
            (NegX, PosY) => Some(NegZ),
            (NegX, NegY) => Some(PosZ),
            // x̂ × ±ẑ = ∓ŷ
            (PosX, PosZ) => Some(NegY),
            (PosX, NegZ) => Some(PosY),
            (NegX, PosZ) => Some(PosY),
            (NegX, NegZ) => Some(NegY),
            // ŷ × ±x̂ = ∓ẑ
            (PosY, PosX) => Some(NegZ),
            (PosY, NegX) => Some(PosZ),
            (NegY, PosX) => Some(PosZ),
            (NegY, NegX) => Some(NegZ),
            // ŷ × ±ẑ = ±x̂
            (PosY, PosZ) => Some(PosX),
            (PosY, NegZ) => Some(NegX),
            (NegY, PosZ) => Some(NegX),
            (NegY, NegZ) => Some(PosX),
            // ẑ × ±x̂ = ±ŷ
            (PosZ, PosX) => Some(PosY),
            (PosZ, NegX) => Some(NegY),
            (NegZ, PosX) => Some(NegY),
            (NegZ, NegX) => Some(PosY),
            // ẑ × ±ŷ = ∓x̂
            (PosZ, PosY) => Some(NegX),
            (PosZ, NegY) => Some(PosX),
            (NegZ, PosY) => Some(PosX),
            (NegZ, NegY) => Some(NegX),
            // Parallel (opposite) or equal axes have no cross product.
            _ => None,
        }
    }

    /// Fail-closed validation of the frozen canonical frame. Deserialized
    /// values must have pairwise-distinct axes, `right × up == toward_viewer`,
    /// and the exact axis triple associated with their view kind.
    pub fn validate(&self) -> Result<(), ProjectionError> {
        let distinct = self.right != self.up
            && self.up != self.toward_viewer
            && self.toward_viewer != self.right;
        let canonical = match self.kind {
            PrincipalViewKind::Front => Self::front(),
            PrincipalViewKind::Top => Self::top(),
            PrincipalViewKind::Left => Self::left(),
        };
        if !distinct
            || Self::cross(self.right, self.up) != Some(self.toward_viewer)
            || *self != canonical
        {
            return Err(ProjectionError::InvalidProjectionGeometry {
                reason: InvalidProjectionGeometryReason::NonCanonicalFrame,
            });
        }
        Ok(())
    }

    /// Generic single-code-path projection (ADR-034 Decision 6):
    /// `u = p·right`, `v = p·up`, `depth = p·toward_viewer`, where `p·axis`
    /// is a signed coordinate read off the named axis — a `match` on
    /// [`AxisDirection`], not a floating-point matrix.
    pub fn project_point(&self, p: PartLocalPoint3) -> ProjectedPoint {
        ProjectedPoint {
            u: Self::signed_coordinate(self.right, p),
            v: Self::signed_coordinate(self.up, p),
            depth: Self::signed_coordinate(self.toward_viewer, p),
        }
    }

    /// Depth range of a bounds box in this frame (§5.2 formulas). Total over
    /// valid `Bounds3D`; the `Result` is defensive symmetry with the layer's
    /// fail-closed style.
    pub fn depth_range(&self, b: Bounds3D) -> Result<ProjectionDepthRange, ProjectionError> {
        let (min, max) = match self.toward_viewer {
            AxisDirection::PosZ => (b.min_z, b.max_z),
            AxisDirection::PosY => (b.min_y, b.max_y),
            // NegX: larger x is farther, so depth = −x and the interval
            // mirrors: [−max_x, −min_x].
            AxisDirection::NegX => (-b.max_x, -b.min_x),
            // The canonical frames only ever use PosZ / PosY / NegX as
            // toward_viewer; any other frame fails `validate`, so this arm
            // is defensive only (fail-closed, never guessed).
            _ => {
                return Err(ProjectionError::InvalidProjectionGeometry {
                    reason: InvalidProjectionGeometryReason::NonFiniteCoordinate,
                })
            }
        };
        ProjectionDepthRange::new(min, max)
    }

    /// The signed coordinate of `p` along the named axis — the entire
    /// numeric content of the projection.
    fn signed_coordinate(axis: AxisDirection, p: PartLocalPoint3) -> f64 {
        match axis {
            AxisDirection::PosX => p.x,
            AxisDirection::NegX => -p.x,
            AxisDirection::PosY => p.y,
            AxisDirection::NegY => -p.y,
            AxisDirection::PosZ => p.z,
            AxisDirection::NegZ => -p.z,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- cross products (matrix rows 1-3) ----

    /// 1: Front frame is right-handed: PosX × PosY = PosZ.
    #[test]
    fn frame_front_cross_product_is_right_handed() {
        let frame = OrthographicFrame::front();
        assert_eq!(
            OrthographicFrame::cross(frame.right, frame.up),
            Some(frame.toward_viewer)
        );
        assert_eq!(
            OrthographicFrame::cross(AxisDirection::PosX, AxisDirection::PosY),
            Some(AxisDirection::PosZ),
            "x̂ × ŷ must be ẑ"
        );
        assert!(frame.validate().is_ok(), "canonical Front must validate");
    }

    /// 2: Top frame is right-handed: PosX × NegZ = PosY.
    #[test]
    fn frame_top_cross_product_is_right_handed() {
        let frame = OrthographicFrame::top();
        assert_eq!(
            OrthographicFrame::cross(frame.right, frame.up),
            Some(frame.toward_viewer)
        );
        assert_eq!(
            OrthographicFrame::cross(AxisDirection::PosX, AxisDirection::NegZ),
            Some(AxisDirection::PosY),
            "x̂ × (−ẑ) must be ŷ"
        );
        assert!(frame.validate().is_ok(), "canonical Top must validate");
    }

    /// 3: Left frame is right-handed: PosZ × PosY = NegX.
    #[test]
    fn frame_left_cross_product_is_right_handed() {
        let frame = OrthographicFrame::left();
        assert_eq!(
            OrthographicFrame::cross(frame.right, frame.up),
            Some(frame.toward_viewer)
        );
        assert_eq!(
            OrthographicFrame::cross(AxisDirection::PosZ, AxisDirection::PosY),
            Some(AxisDirection::NegX),
            "ẑ × ŷ must be −x̂"
        );
        assert!(frame.validate().is_ok(), "canonical Left must validate");
    }

    /// The cross-product table is total and exact over all 36 ordered
    /// pairs: exactly the 24 perpendicular distinct ordered pairs of the 6
    /// axis directions yield `Some` per the right-hand rule (signs of the
    /// inputs carry through); every parallel or equal pair is `None`.
    #[test]
    fn cross_table_is_exact_over_all_36_pairs() {
        use AxisDirection::*;
        let expected = [
            // x̂ × ±ŷ = ±ẑ
            ((PosX, PosY), Some(PosZ)),
            ((PosX, NegY), Some(NegZ)),
            ((NegX, PosY), Some(NegZ)),
            ((NegX, NegY), Some(PosZ)),
            // x̂ × ±ẑ = ∓ŷ
            ((PosX, PosZ), Some(NegY)),
            ((PosX, NegZ), Some(PosY)),
            ((NegX, PosZ), Some(PosY)),
            ((NegX, NegZ), Some(NegY)),
            // ŷ × ±x̂ = ∓ẑ
            ((PosY, PosX), Some(NegZ)),
            ((PosY, NegX), Some(PosZ)),
            ((NegY, PosX), Some(PosZ)),
            ((NegY, NegX), Some(NegZ)),
            // ŷ × ±ẑ = ±x̂
            ((PosY, PosZ), Some(PosX)),
            ((PosY, NegZ), Some(NegX)),
            ((NegY, PosZ), Some(NegX)),
            ((NegY, NegZ), Some(PosX)),
            // ẑ × ±x̂ = ±ŷ
            ((PosZ, PosX), Some(PosY)),
            ((PosZ, NegX), Some(NegY)),
            ((NegZ, PosX), Some(NegY)),
            ((NegZ, NegX), Some(PosY)),
            // ẑ × ±ŷ = ∓x̂
            ((PosZ, PosY), Some(NegX)),
            ((PosZ, NegY), Some(PosX)),
            ((NegZ, PosY), Some(PosX)),
            ((NegZ, NegY), Some(NegX)),
        ];
        let axes = [PosX, NegX, PosY, NegY, PosZ, NegZ];
        let mut some_count = 0;
        for a in axes {
            for b in axes {
                let result = OrthographicFrame::cross(a, b);
                let expected = expected
                    .iter()
                    .copied()
                    .find(|((ea, eb), _)| *ea == a && *eb == b)
                    .map(|(_, result)| result)
                    .unwrap_or(None);
                assert_eq!(result, expected, "{a:?} × {b:?} must be {expected:?}");
                if result.is_some() {
                    some_count += 1;
                }
            }
        }
        assert_eq!(
            some_count, 24,
            "exactly the 24 perpendicular distinct pairs cross"
        );
        // Anti-commutativity for every crossing pair.
        for ((a, b), result) in expected {
            if let Some(c) = result {
                assert_eq!(
                    OrthographicFrame::cross(b, a),
                    Some(negate(c)),
                    "cross must anti-commute for {a:?}/{b:?}"
                );
            }
        }
    }

    /// The opposite of a named axis, used to check anti-commutativity.
    fn negate(axis: AxisDirection) -> AxisDirection {
        match axis {
            AxisDirection::PosX => AxisDirection::NegX,
            AxisDirection::NegX => AxisDirection::PosX,
            AxisDirection::PosY => AxisDirection::NegY,
            AxisDirection::NegY => AxisDirection::PosY,
            AxisDirection::PosZ => AxisDirection::NegZ,
            AxisDirection::NegZ => AxisDirection::PosZ,
        }
    }

    /// A frame with a repeated axis or a non-right-handed triple fails
    /// `validate` (defensive post-deserialize gate).
    #[test]
    fn validate_rejects_degenerate_frames() {
        let mut frame = OrthographicFrame::front();
        frame.up = AxisDirection::PosX; // right == up
        assert!(frame.validate().is_err());
        let mut frame = OrthographicFrame::front();
        frame.toward_viewer = AxisDirection::NegZ; // cross ≠ toward_viewer
        assert!(frame.validate().is_err());
    }

    // ---- exact formulas (matrix rows 4-6) ----

    fn sample_point() -> PartLocalPoint3 {
        PartLocalPoint3 {
            x: 10.0,
            y: 20.0,
            z: 3.0,
        }
    }

    /// 4: Front projection is exactly (u = +x, v = +y, d = +z):
    /// P = (10, 20, 3) → (10, 20, 3).
    #[test]
    fn front_project_point_exact_formulas() {
        let p = sample_point();
        let projected = OrthographicFrame::front().project_point(p);
        assert_eq!(projected.u, 10.0);
        assert_eq!(projected.v, 20.0);
        assert_eq!(projected.depth, 3.0);
    }

    /// 5: Top projection is exactly (u = +x, v = −z, d = +y):
    /// P = (10, 20, 3) → (10, −3, 20).
    #[test]
    fn top_project_point_exact_formulas() {
        let p = sample_point();
        let projected = OrthographicFrame::top().project_point(p);
        assert_eq!(projected.u, 10.0);
        assert_eq!(projected.v, -3.0);
        assert_eq!(projected.depth, 20.0);
    }

    /// 6: Left projection is exactly (u = +z, v = +y, d = −x):
    /// P = (10, 20, 3) → (3, 20, −10).
    #[test]
    fn left_project_point_exact_formulas() {
        let p = sample_point();
        let projected = OrthographicFrame::left().project_point(p);
        assert_eq!(projected.u, 3.0);
        assert_eq!(projected.v, 20.0);
        assert_eq!(projected.depth, -10.0);
    }

    // ---- alignment invariants (matrix rows 7-9) ----

    /// 7: Front.u == Top.u (both +x): horizontal alignment between front
    /// and top.
    #[test]
    fn alignment_front_u_equals_top_u() {
        let p = sample_point();
        assert_eq!(
            OrthographicFrame::front().project_point(p).u,
            OrthographicFrame::top().project_point(p).u
        );
        assert_eq!(
            OrthographicFrame::front().right,
            OrthographicFrame::top().right
        );
    }

    /// 8: Front.v == Left.v (both +y): vertical alignment between front
    /// and left.
    #[test]
    fn alignment_front_v_equals_left_v() {
        let p = sample_point();
        assert_eq!(
            OrthographicFrame::front().project_point(p).v,
            OrthographicFrame::left().project_point(p).v
        );
        assert_eq!(OrthographicFrame::front().up, OrthographicFrame::left().up);
    }

    /// 9: Left.u == −Top.v (Left.u = +z; Top.v = −z): the left view's
    /// horizontal axis is the top view's vertical axis mirrored, per
    /// standard orthographic layout.
    #[test]
    fn alignment_left_u_equals_neg_top_v() {
        let p = sample_point();
        let left_u = OrthographicFrame::left().project_point(p).u;
        let top_v = OrthographicFrame::top().project_point(p).v;
        assert_eq!(left_u, -top_v);
    }

    // ---- PrincipalViewKind order (matrix row 10) ----

    /// 10: The canonical order Front < Top < Left is contractual (discriminant
    /// declaration order), never alphabetical accident.
    #[test]
    fn principal_view_kind_order_is_front_top_left() {
        assert!(PrincipalViewKind::Front < PrincipalViewKind::Top);
        assert!(PrincipalViewKind::Top < PrincipalViewKind::Left);
        let mut kinds = [
            PrincipalViewKind::Left,
            PrincipalViewKind::Front,
            PrincipalViewKind::Top,
        ];
        kinds.sort();
        assert_eq!(
            kinds,
            [
                PrincipalViewKind::Front,
                PrincipalViewKind::Top,
                PrincipalViewKind::Left
            ]
        );
    }

    // ---- AxisDirection completeness + serde (matrix row 12) ----

    /// 12: All six axis directions roundtrip through serde as snake_case
    /// strings.
    #[test]
    fn axis_direction_roundtrips_all_six() {
        for axis in [
            AxisDirection::PosX,
            AxisDirection::NegX,
            AxisDirection::PosY,
            AxisDirection::NegY,
            AxisDirection::PosZ,
            AxisDirection::NegZ,
        ] {
            let json = serde_json::to_string(&axis).unwrap();
            assert_eq!(
                serde_json::from_str::<AxisDirection>(&json).unwrap(),
                axis,
                "{axis:?} must roundtrip through {json}"
            );
        }
    }

    // ---- depth ranges from Bounds3D (matrix rows 13-15) ----

    fn bounds() -> scc_spatial::Bounds3D {
        scc_spatial::Bounds3D::new([0.0, 0.0, 0.0], [120.0, 80.0, 10.0]).unwrap()
    }

    /// 13: Front depth range is exactly [min_z, max_z] = [0, 10].
    #[test]
    fn depth_range_front_from_bounds3d() {
        let depth = OrthographicFrame::front().depth_range(bounds()).unwrap();
        assert_eq!(depth.min, 0.0);
        assert_eq!(depth.max, 10.0);
        assert!(!depth.is_point());
    }

    /// 14: Top depth range is exactly [min_y, max_y] = [0, 80].
    #[test]
    fn depth_range_top_from_bounds3d() {
        let depth = OrthographicFrame::top().depth_range(bounds()).unwrap();
        assert_eq!(depth.min, 0.0);
        assert_eq!(depth.max, 80.0);
    }

    /// 15: Left depth range is exactly [−max_x, −min_x] = [−120, 0]
    /// (mirrored — larger x is farther from the −X viewer).
    #[test]
    fn depth_range_left_from_bounds3d() {
        let depth = OrthographicFrame::left().depth_range(bounds()).unwrap();
        assert_eq!(depth.min, -120.0);
        assert_eq!(depth.max, 0.0);
    }

    /// Frames, points, and projected points roundtrip through serde
    /// (transport/testing-only per ADR-034 Decision 15/21).
    #[test]
    fn frames_and_points_roundtrip_through_serde() {
        for frame in [
            OrthographicFrame::front(),
            OrthographicFrame::top(),
            OrthographicFrame::left(),
        ] {
            let json = serde_json::to_string(&frame).unwrap();
            assert_eq!(
                serde_json::from_str::<OrthographicFrame>(&json).unwrap(),
                frame
            );
            assert!(frame.validate().is_ok());
        }
        let point = PartLocalPoint3 {
            x: 1.5,
            y: -2.5,
            z: 3.5,
        };
        let json = serde_json::to_string(&point).unwrap();
        assert_eq!(
            serde_json::from_str::<PartLocalPoint3>(&json).unwrap(),
            point
        );
        let projected = OrthographicFrame::top().project_point(point);
        let json = serde_json::to_string(&projected).unwrap();
        assert_eq!(
            serde_json::from_str::<ProjectedPoint>(&json).unwrap(),
            projected
        );
        let flat = ProjectedPoint2 { u: 1.0, v: -1.0 };
        let json = serde_json::to_string(&flat).unwrap();
        assert_eq!(
            serde_json::from_str::<ProjectedPoint2>(&json).unwrap(),
            flat
        );
    }

    /// PrincipalViewKind serde spelling is snake_case (`front`, `top`,
    /// `left`) — the same spelling the deterministic ids use.
    #[test]
    fn principal_view_kind_serde_is_snake_case() {
        for (kind, spelling) in [
            (PrincipalViewKind::Front, "\"front\""),
            (PrincipalViewKind::Top, "\"top\""),
            (PrincipalViewKind::Left, "\"left\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), spelling);
            assert_eq!(
                serde_json::from_str::<PrincipalViewKind>(spelling).unwrap(),
                kind
            );
            assert_eq!(kind.id_spelling(), spelling.trim_matches('"'));
        }
    }
}
