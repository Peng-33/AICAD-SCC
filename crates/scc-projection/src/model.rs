//! The orthographic projection model and its fail-closed builder (Phase 9.3,
//! ADR-034).
//!
//! [`ProjectionModel`] is a pure derived artifact over a
//! [`scc_spatial::SpatialModel`]: three canonical principal views
//! (`Front < Top < Left`), each holding its frozen frame, projected
//! primitives with an embedded depth range, and stored-validated bounds,
//! plus a [`ProjectionSourceMap`] tying every primitive back to its spatial
//! element. The model is read-only — no mutators, no `Default`; it is built
//! once by [`ProjectionModelBuilder::finish`] and afterwards only queried
//! (ADR-034 Decision 17).
//!
//! Validation order inside `add_primitive` is deterministic and
//! documented on the method: view → geometry → depth → duplicate id →
//! source integrity. The FIRST failing gate wins.

use std::collections::{BTreeMap, BTreeSet};

use scc_spatial::SpatialModel;
use serde::{Deserialize, Serialize};

use crate::error::{InvalidProjectionGeometryReason, ProjectionError};
use crate::frame::{OrthographicFrame, PrincipalViewKind, ProjectedPoint2};
use crate::source_map::ProjectionSourceMap;

/// Closed depth interval along a frame's `toward_viewer` axis (mm).
/// Finite, `min <= max`; `min == max` is LEGAL (edge-on face); negatives are
/// legal (Left view depth = −x). Deliberately NOT
/// [`scc_spatial::ZInterval`], which rejects `min == max` and `min < 0` —
/// a documented non-reuse, not an oversight (ADR-034 Decision 8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDepthRange {
    /// Nearest-excluded interval minimum along `toward_viewer` (mm).
    pub min: f64,
    /// Interval maximum along `toward_viewer` (mm); larger = nearer viewer.
    pub max: f64,
}

impl ProjectionDepthRange {
    /// Validate and construct a depth range.
    pub fn new(min: f64, max: f64) -> Result<Self, ProjectionError> {
        if !min.is_finite() || !max.is_finite() || min > max {
            return Err(ProjectionError::InvalidProjectionDepthRange { min, max });
        }
        Ok(Self { min, max })
    }

    /// Whether the interval degenerates to a single depth (an edge-on face).
    pub fn is_point(&self) -> bool {
        self.min == self.max
    }
}

/// Axis-aligned envelope in view (u, v) space (mm). Finite, `max_u >= min_u`
/// and `max_v >= min_v` (degenerate legal — a planar-face-only view, or an
/// empty view's `0..0` envelope; ADR-034 Decision 16).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectionBounds2D {
    /// Minimum `u` (mm).
    pub min_u: f64,
    /// Maximum `u` (mm).
    pub max_u: f64,
    /// Minimum `v` (mm).
    pub min_v: f64,
    /// Maximum `v` (mm).
    pub max_v: f64,
}

impl ProjectionBounds2D {
    /// The degenerate zero envelope used for a view with zero primitives
    /// (empty views must still store valid bounds; `max >= min` holds).
    pub(crate) const EMPTY: ProjectionBounds2D = ProjectionBounds2D {
        min_u: 0.0,
        max_u: 0.0,
        min_v: 0.0,
        max_v: 0.0,
    };

    /// Fail-closed check: finite and non-inverted on both axes.
    fn validate(&self) -> Result<(), ProjectionError> {
        if !self.min_u.is_finite()
            || !self.max_u.is_finite()
            || !self.min_v.is_finite()
            || !self.max_v.is_finite()
            || self.max_u < self.min_u
            || self.max_v < self.min_v
        {
            return Err(ProjectionError::InvalidProjectionBounds { bounds: *self });
        }
        Ok(())
    }

    /// Grow the envelope to include one (u, v) point. Order-independent:
    /// the result is the same envelope regardless of accumulation order.
    fn include(&mut self, u: f64, v: f64) {
        self.min_u = self.min_u.min(u);
        self.max_u = self.max_u.max(u);
        self.min_v = self.min_v.min(v);
        self.max_v = self.max_v.max(v);
    }
}

/// The role of a projected primitive. Exactly two variants in 9.3; no
/// Hidden/Visible/Construction role exists (visibility is Phase 9.5;
/// construction circles project to nothing — reference-only, ADR-033).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPrimitiveRole {
    /// An outline of the spatial body.
    BodyBoundary,
    /// An outline of a cylindrical cut.
    CutBoundary,
}

/// One projected primitive's geometry. Arcs use radians and the positive
/// counter-clockwise sweep from `start_angle` to `end_angle`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectedGeometry {
    /// A straight edge in view space.
    Segment {
        /// Segment start point.
        start: ProjectedPoint2,
        /// Segment end point.
        end: ProjectedPoint2,
    },
    /// A full circle in view space (a cut seen end-on).
    Circle {
        /// Circle center.
        center: ProjectedPoint2,
        /// Circle radius (mm), strictly positive.
        radius: f64,
    },
    /// A circular arc in view space. The endpoint at each angle is derived
    /// as `center + radius * (cos(angle), sin(angle))`.
    Arc {
        /// Arc center point.
        center: ProjectedPoint2,
        /// Arc radius (mm), strictly positive.
        radius: f64,
        /// Counter-clockwise start angle in radians.
        start_angle: f64,
        /// Counter-clockwise end angle in radians. The normalized sweep from
        /// `start_angle` to this angle is strictly between zero and one turn.
        end_angle: f64,
    },
}

impl ProjectedGeometry {
    /// Fail-closed geometry gate: non-finite coordinates, zero-length
    /// segments (exact `==`, no tolerance policy in 9.3), non-positive
    /// radii.
    pub fn validate(&self) -> Result<(), ProjectionError> {
        match self {
            ProjectedGeometry::Segment { start, end } => {
                if !start.u.is_finite()
                    || !start.v.is_finite()
                    || !end.u.is_finite()
                    || !end.v.is_finite()
                {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::NonFiniteCoordinate,
                    });
                }
                if start.u == end.u && start.v == end.v {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::ZeroLengthSegment,
                    });
                }
                Ok(())
            }
            ProjectedGeometry::Circle { center, radius } => {
                if !center.u.is_finite() || !center.v.is_finite() || !radius.is_finite() {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::NonFiniteCoordinate,
                    });
                }
                if *radius <= 0.0 {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::NonPositiveRadius,
                    });
                }
                Ok(())
            }
            ProjectedGeometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                if !center.u.is_finite()
                    || !center.v.is_finite()
                    || !radius.is_finite()
                    || !start_angle.is_finite()
                    || !end_angle.is_finite()
                {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::NonFiniteCoordinate,
                    });
                }
                if *radius <= 0.0 {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::NonPositiveRadius,
                    });
                }
                let sweep = normalized_arc_sweep(*start_angle, *end_angle);
                if sweep <= ARC_SWEEP_EPSILON || sweep >= std::f64::consts::TAU - ARC_SWEEP_EPSILON
                {
                    return Err(ProjectionError::InvalidProjectionGeometry {
                        reason: InvalidProjectionGeometryReason::InvalidArcSweep,
                    });
                }
                Ok(())
            }
        }
    }

    /// The exact finite set of points needed for axis-aligned bounds. For an
    /// arc this is its two endpoints plus every cardinal point lying on its
    /// CCW sweep; the full-circle envelope is never used for a partial arc.
    fn bound_points(&self) -> Vec<(f64, f64)> {
        match self {
            ProjectedGeometry::Segment { start, end } => {
                vec![(start.u, start.v), (end.u, end.v)]
            }
            ProjectedGeometry::Circle { center, radius } => {
                let r = radius.abs();
                vec![
                    (center.u - r, center.v),
                    (center.u + r, center.v),
                    (center.u, center.v - r),
                    (center.u, center.v + r),
                ]
            }
            ProjectedGeometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                let sweep = normalized_arc_sweep(*start_angle, *end_angle);
                let mut points = vec![
                    arc_point(*center, *radius, *start_angle),
                    arc_point(*center, *radius, *end_angle),
                ];
                for (angle, u_sign, v_sign) in [
                    (0.0, 1.0, 0.0),
                    (std::f64::consts::FRAC_PI_2, 0.0, 1.0),
                    (std::f64::consts::PI, -1.0, 0.0),
                    (std::f64::consts::PI * 1.5, 0.0, -1.0),
                ] {
                    if ccw_angle_distance(*start_angle, angle) <= sweep {
                        points.push((center.u + radius * u_sign, center.v + radius * v_sign));
                    }
                }
                points
            }
        }
    }
}

const ARC_SWEEP_EPSILON: f64 = 1.0e-9;

fn normalized_arc_sweep(start_angle: f64, end_angle: f64) -> f64 {
    (end_angle - start_angle).rem_euclid(std::f64::consts::TAU)
}

fn ccw_angle_distance(start_angle: f64, angle: f64) -> f64 {
    (angle - start_angle).rem_euclid(std::f64::consts::TAU)
}

fn arc_point(center: ProjectedPoint2, radius: f64, angle: f64) -> (f64, f64) {
    (
        center.u + radius * angle.cos(),
        center.v + radius * angle.sin(),
    )
}

/// Opaque, deterministic, serde-stable id of one projected primitive.
/// Format frozen in §4.1:
/// `{view}:{source}:p:{index:04}` (view = `front`|`top`|`left`; source =
/// `body` or the `SpatialCutId` verbatim). Never parsed for meaning, never
/// random, never coordinate-derived.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProjectionPrimitiveId(pub String);

impl std::fmt::Display for ProjectionPrimitiveId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Reference to the spatial element a projected primitive came from.
/// Opaque over [`scc_spatial::SpatialCutId`] — never a parsed feature string
/// (ADR-034 Decision 13; no `FeatureId`/`EntityId` appears in scc-projection).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "element", rename_all = "snake_case")]
pub enum SpatialElementRef {
    /// The spatial body.
    Body,
    /// One exact ordered boundary segment of a composite spatial body.
    BodySegment {
        /// Stable segment index from `SpatialProfileSegment`, never parsed
        /// from a primitive or entity identifier.
        index: u32,
    },
    /// One cylindrical cut, by its opaque spatial id.
    Cut {
        /// The opaque `SpatialCutId` of the cut.
        id: scc_spatial::SpatialCutId,
    },
    /// A through-slot cut as a whole (side-view silhouette).
    SlotCut {
        /// The opaque `SpatialCutId` of the slot.
        id: scc_spatial::SpatialCutId,
    },
    /// One exact ordered slot-profile segment on Front.
    SlotSegment {
        /// The opaque `SpatialCutId` of the slot.
        id: scc_spatial::SpatialCutId,
        /// Stable segment index from the slot footprint.
        index: u32,
    },
}

/// One projected primitive. Provenance is NOT embedded (the separate
/// [`ProjectionSourceMap`] stores it once — one element produces many
/// primitives); the depth range IS embedded (§5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionPrimitive {
    /// The projected geometry.
    pub geometry: ProjectedGeometry,
    /// The role of the primitive (body vs cut outline).
    pub role: ProjectionPrimitiveRole,
    /// The depth interval along the view's `toward_viewer` axis; larger =
    /// nearer viewer.
    pub depth: ProjectionDepthRange,
}

/// One principal view: frozen frame + primitives + stored-validated bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionView {
    /// The canonical view kind.
    pub kind: PrincipalViewKind,
    /// The frozen canonical frame (validated canonical construction).
    pub frame: OrthographicFrame,
    /// The projected primitives, keyed by deterministic id.
    pub primitives: BTreeMap<ProjectionPrimitiveId, ProjectionPrimitive>,
    /// Recomputed + validated at `finish()`; the stored value is the
    /// contract (ADR-034 Decision 16).
    pub bounds: ProjectionBounds2D,
}

/// The projection model: one view per principal kind, keyed by the
/// deterministic enum order `Front < Top < Left`, plus the provenance map.
/// Read-only after construction; no `Default` (no empty model outside the
/// builder), no mutators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionModel {
    /// The projected views, in canonical `Front < Top < Left` order.
    pub views: BTreeMap<PrincipalViewKind, ProjectionView>,
    /// Provenance of every primitive, stored alongside (single artifact).
    pub sources: ProjectionSourceMap,
}

/// Staged per-(view, source) occurrence counter for deterministic id
/// allocation (ADR-034 Decision 11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StagedCounter(BTreeMap<(PrincipalViewKind, SpatialElementRef), u64>);

/// Staged state of the builder: views added so far and primitives staged
/// per view (validated at insert time; bounds are recomputed at `finish`).
#[derive(Debug, Default)]
pub struct ProjectionModelBuilder {
    views: BTreeMap<PrincipalViewKind, ProjectionView>,
    sources: ProjectionSourceMap,
    counters: StagedCounter,
}

impl ProjectionModelBuilder {
    /// Create an empty builder. Zero views + zero primitives is a legal
    /// `finish` (empty-model policy, ADR-034 Decision 18).
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the frozen canonical frame for `kind`. Idempotent: adding an
    /// already-present kind is a no-op success (ADR-034 Decision 14).
    /// Frames are created internally — callers never supply custom axes.
    pub fn add_view(&mut self, kind: PrincipalViewKind) -> Result<(), ProjectionError> {
        let frame = match kind {
            PrincipalViewKind::Front => OrthographicFrame::front(),
            PrincipalViewKind::Top => OrthographicFrame::top(),
            PrincipalViewKind::Left => OrthographicFrame::left(),
        };
        self.views.entry(kind).or_insert_with(|| ProjectionView {
            kind,
            frame,
            primitives: BTreeMap::new(),
            bounds: ProjectionBounds2D::EMPTY,
        });
        Ok(())
    }

    /// Insert one primitive. Fail-closed, in this deterministic gate order
    /// (the FIRST failing gate wins):
    ///
    /// 1. view exists (`MissingProjectionView`),
    /// 2. geometry validity (`InvalidProjectionGeometry` — non-finite,
    ///    zero-length segment with exact `==`, non-positive radius),
    /// 3. depth range validity (`InvalidProjectionDepthRange` — non-finite
    ///    or `min > max`; `min == max` is legal),
    /// 4. duplicate id within the view (`DuplicateProjectionPrimitive`),
    /// 5. source integrity (`InvalidProjectionSource` — a `Cut` id absent
    ///    from `spatial.cuts`; `Body` is always valid because a
    ///    `SpatialModel` always has a body).
    #[allow(clippy::too_many_arguments)]
    pub fn add_primitive(
        &mut self,
        view: PrincipalViewKind,
        id: ProjectionPrimitiveId,
        geometry: ProjectedGeometry,
        role: ProjectionPrimitiveRole,
        depth: ProjectionDepthRange,
        source: SpatialElementRef,
        spatial: &SpatialModel,
    ) -> Result<(), ProjectionError> {
        // 1. View must exist first — no staging into an unknown view.
        if !self.views.contains_key(&view) {
            return Err(ProjectionError::MissingProjectionView { kind: view });
        }
        // 2. Geometry gate.
        geometry.validate()?;
        // 3. Depth gate (re-checked even though `new` validated: the struct
        //    may arrive from deserialization or hand construction).
        if !depth.min.is_finite() || !depth.max.is_finite() || depth.min > depth.max {
            return Err(ProjectionError::InvalidProjectionDepthRange {
                min: depth.min,
                max: depth.max,
            });
        }
        // 4. Duplicate id gate.
        if self
            .views
            .get(&view)
            .is_some_and(|entry| entry.primitives.contains_key(&id))
        {
            return Err(ProjectionError::DuplicateProjectionPrimitive { id, kind: view });
        }
        // 5. Source integrity gate (insert-time, fail-closed, no synthetic
        //    fallback).
        match &source {
            SpatialElementRef::Body => {}
            SpatialElementRef::BodySegment { index } => {
                let scc_spatial::SpatialFootprint::Composite { segments } = &spatial.body.footprint
                else {
                    return Err(ProjectionError::InvalidProjectionSource { id });
                };
                if !segments
                    .iter()
                    .any(|segment| segment.segment_index() == *index)
                {
                    return Err(ProjectionError::InvalidProjectionSource { id });
                }
            }
            SpatialElementRef::Cut { id: cut_id } => {
                if !spatial.cuts.contains_key(cut_id) {
                    return Err(ProjectionError::InvalidProjectionSource { id });
                }
            }
            SpatialElementRef::SlotCut { id: cut_id } => {
                if !spatial.slot_cuts.contains_key(cut_id) {
                    return Err(ProjectionError::InvalidProjectionSource { id });
                }
            }
            SpatialElementRef::SlotSegment { id: cut_id, index } => {
                let Some(slot) = spatial.slot_cuts.get(cut_id) else {
                    return Err(ProjectionError::InvalidProjectionSource { id });
                };
                if !slot
                    .segments
                    .iter()
                    .any(|segment| segment.segment_index() == *index)
                {
                    return Err(ProjectionError::InvalidProjectionSource { id });
                }
            }
        }
        // A primitive id is global to the serialized model, not merely local
        // to one view. Reject cross-view collisions before the source map can
        // overwrite its one-to-one entry.
        if self.sources.contains_primitive(&id) {
            return Err(ProjectionError::DuplicateProjectionPrimitive { id, kind: view });
        }
        let view_entry = match self.views.get_mut(&view) {
            Some(entry) => entry,
            // Unreachable: the view gate above already returned.
            None => return Err(ProjectionError::MissingProjectionView { kind: view }),
        };
        view_entry.primitives.insert(
            id.clone(),
            ProjectionPrimitive {
                geometry,
                role,
                depth,
            },
        );
        self.sources.record(id.clone(), source.clone());
        // The (view, source) occurrence index advances only after a
        // successful insert, so `next_id` output stays gapless.
        *self.counters.0.entry((view, source)).or_insert(0) += 1;
        Ok(())
    }

    /// Deterministic id from (view, source, current occurrence count) —
    /// §4.1 format `{view}:{source}:p:{index:04}`. Pure; reads staged state
    /// only; does not allocate the index (repeated calls with no
    /// intervening `add_primitive` return the same id).
    pub fn next_id(
        &self,
        view: PrincipalViewKind,
        source: &SpatialElementRef,
    ) -> ProjectionPrimitiveId {
        let index = self
            .counters
            .0
            .get(&(view, source.clone()))
            .copied()
            .unwrap_or(0);
        ProjectionPrimitiveId(format!(
            "{}:{}:p:{:04}",
            view.id_spelling(),
            source_spelling(source),
            index
        ))
    }

    /// Finish: recompute + validate per-view bounds, freeze into a
    /// read-only `ProjectionModel`. Zero views + zero primitives is legal
    /// (§3.6). An empty view stores the degenerate `0..0` envelope
    /// (`ProjectionBounds2D` allows `max >= min`), documented as the
    /// empty-view bounds policy.
    pub fn finish(self) -> Result<ProjectionModel, ProjectionError> {
        // Staged primitives cannot exist without their view (add_primitive
        // gates on the view first), so this arm is defensive; kept because
        // finish() is the last chance to fail closed before freezing.
        let mut views = self.views;
        for view in views.values_mut() {
            view.bounds = recompute_bounds(&view.primitives)?;
        }
        let sources = self.sources;
        if !sources.covers_exactly(
            views
                .values()
                .flat_map(|view| view.primitives.keys().cloned()),
        ) {
            // Unreachable by construction (provenance is recorded
            // atomically); fail-closed rather than ever shipping a model
            // with a provenance gap.
            return Err(ProjectionError::InvalidProjectionSource {
                id: ProjectionPrimitiveId(String::new()),
            });
        }
        Ok(ProjectionModel { views, sources })
    }
}

/// The `body` spelling used in ids for [`SpatialElementRef::Body`]; a cut
/// contributes its `SpatialCutId` Display verbatim (opaque).
fn source_spelling(source: &SpatialElementRef) -> String {
    match source {
        SpatialElementRef::Body => "body".to_string(),
        SpatialElementRef::BodySegment { index } => format!("body-segment-{index:04}"),
        SpatialElementRef::Cut { id } => id.to_string(),
        SpatialElementRef::SlotCut { id } => format!("slot-cut-{id}"),
        SpatialElementRef::SlotSegment { id, index } => {
            format!("slot-segment-{id}-{index:04}")
        }
    }
}

impl ProjectionModel {
    /// Integrity checks decodable without a spatial model: every primitive
    /// in every view has a source-map entry (and no orphan provenance);
    /// every stored geometry is finite/valid; every view's frame passes
    /// [`OrthographicFrame::validate`]; every stored bounds envelope is
    /// valid. Intended as the post-deserialize gate — deserialization does
    /// NOT auto-validate (Phase 9.2 debt pattern, retained deliberately).
    pub fn validate_integrity(&self) -> Result<(), ProjectionError> {
        if !self.sources.covers_exactly(
            self.views
                .values()
                .flat_map(|view| view.primitives.keys().cloned()),
        ) {
            return Err(ProjectionError::InvalidProjectionSource {
                id: ProjectionPrimitiveId(String::new()),
            });
        }
        for (kind, view) in &self.views {
            if view.kind != *kind || view.frame.kind != *kind {
                return Err(ProjectionError::InvalidProjectionGeometry {
                    reason: InvalidProjectionGeometryReason::NonCanonicalFrame,
                });
            }
            view.frame.validate()?;
            view.bounds.validate()?;
            let recomputed = recompute_bounds(&view.primitives)?;
            if view.bounds != recomputed {
                return Err(ProjectionError::InvalidProjectionBounds {
                    bounds: view.bounds,
                });
            }
            for primitive in view.primitives.values() {
                if !primitive.depth.min.is_finite()
                    || !primitive.depth.max.is_finite()
                    || primitive.depth.min > primitive.depth.max
                {
                    return Err(ProjectionError::InvalidProjectionDepthRange {
                        min: primitive.depth.min,
                        max: primitive.depth.max,
                    });
                }
            }
        }
        Ok(())
    }

    /// Cross-check against the originating spatial model: every
    /// [`SpatialElementRef::Cut`] id exists in `spatial.cuts`. The gate 9.4
    /// runs after building; also the post-deserialize gate for source
    /// integrity (stricter than [`Self::validate_integrity`], which cannot
    /// see the spatial model).
    pub fn validate_against(&self, spatial: &SpatialModel) -> Result<(), ProjectionError> {
        self.validate_integrity()?;
        for (primitive, source) in self.sources.iter() {
            let valid = match source {
                SpatialElementRef::Body => true,
                SpatialElementRef::BodySegment { index } => matches!(
                    &spatial.body.footprint,
                    scc_spatial::SpatialFootprint::Composite { segments }
                        if segments.iter().any(|segment| segment.segment_index() == *index)
                ),
                SpatialElementRef::Cut { id: cut_id } => spatial.cuts.contains_key(cut_id),
                SpatialElementRef::SlotCut { id: cut_id } => spatial.slot_cuts.contains_key(cut_id),
                SpatialElementRef::SlotSegment { id: cut_id, index } => {
                    spatial.slot_cuts.get(cut_id).is_some_and(|slot| {
                        slot.segments
                            .iter()
                            .any(|segment| segment.segment_index() == *index)
                    })
                }
            };
            if !valid {
                return Err(ProjectionError::InvalidProjectionSource {
                    id: primitive.clone(),
                });
            }
        }
        Ok(())
    }

    /// All primitives derived from one spatial element, per view. Composes
    /// the source map's derived-on-demand index with the view membership —
    /// reverse queries are scans over a small map at call time: cheap,
    /// deterministic, zero drift (ADR-034 Decision 11/12).
    pub fn primitives_for_element(
        &self,
        element: &SpatialElementRef,
    ) -> BTreeMap<PrincipalViewKind, BTreeSet<ProjectionPrimitiveId>> {
        let mut grouped: BTreeMap<PrincipalViewKind, BTreeSet<ProjectionPrimitiveId>> =
            BTreeMap::new();
        for (kind, view) in &self.views {
            let matching = view
                .primitives
                .keys()
                .filter(|primitive| {
                    self.sources
                        .element_for_primitive(primitive)
                        .is_some_and(|source| source == element)
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            if !matching.is_empty() {
                grouped.insert(*kind, matching);
            }
        }
        grouped
    }

    /// The spatial element one primitive came from, if known.
    pub fn element_for_primitive(
        &self,
        primitive: &ProjectionPrimitiveId,
    ) -> Option<&SpatialElementRef> {
        self.sources.element_for_primitive(primitive)
    }
}

/// Recompute a view envelope from raw primitive geometry. This is shared by
/// `finish` and the post-deserialize integrity gate so a stored bounds value
/// cannot drift from the primitives it claims to summarize.
fn recompute_bounds(
    primitives: &BTreeMap<ProjectionPrimitiveId, ProjectionPrimitive>,
) -> Result<ProjectionBounds2D, ProjectionError> {
    let mut bounds: Option<ProjectionBounds2D> = None;
    for primitive in primitives.values() {
        primitive.geometry.validate()?;
        for (u, v) in primitive.geometry.bound_points() {
            match &mut bounds {
                Some(acc) => acc.include(u, v),
                None => {
                    bounds = Some(ProjectionBounds2D {
                        min_u: u,
                        max_u: u,
                        min_v: v,
                        max_v: v,
                    })
                }
            }
        }
    }
    let bounds = bounds.unwrap_or(ProjectionBounds2D::EMPTY);
    bounds.validate()?;
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_spatial::SpatialCutId;

    /// A hand-built `SpatialModel` fixture for insert-time source-integrity
    /// gates. All `SpatialModel` fields are `pub`, so the literal is legal;
    /// `Point2`/`FeatureId` are not re-exported by `scc-spatial` (and
    /// scc-projection must not depend on `scc-engineering`/`scc-geometry`),
    /// so the fixture roundtrips through serde_json (a dev-dep) instead.
    fn spatial_model() -> SpatialModel {
        const FIXTURE: &str = r#"{
            "body": {
                "footprint": {
                    "Rectangle": {
                        "origin": { "x": -60.0, "y": -40.0 },
                        "width": 120.0,
                        "height": 80.0
                    }
                },
                "z": { "min": 0.0, "max": 10.0 },
                "source_feature": "plate",
                "source_entity": "plate"
            },
            "cuts": {
                "a_hole:cut:0000": {
                    "center": { "x": 0.0, "y": 0.0 },
                    "radius": 5.0,
                    "z": { "min": 0.0, "max": 10.0 },
                    "kind": { "kind": "through_all" },
                    "source_feature": "a_hole",
                    "source_entity": "a_hole"
                }
            },
            "bounds": {
                "min_x": -60.0, "max_x": 60.0,
                "min_y": -40.0, "max_y": 40.0,
                "min_z": 0.0, "max_z": 10.0
            },
            "construction_circles": {},
            "source": { "geometry_source_root": 1, "geometry_source_step": 1 }
        }"#;
        serde_json::from_str::<SpatialModel>(FIXTURE).expect("spatial fixture must deserialize")
    }

    fn segment(u1: f64, v1: f64, u2: f64, v2: f64) -> ProjectedGeometry {
        ProjectedGeometry::Segment {
            start: ProjectedPoint2 { u: u1, v: v1 },
            end: ProjectedPoint2 { u: u2, v: v2 },
        }
    }

    fn body_depth() -> ProjectionDepthRange {
        ProjectionDepthRange::new(0.0, 10.0).unwrap()
    }

    fn builder_with_view() -> (ProjectionModelBuilder, SpatialModel) {
        let spatial = spatial_model();
        let mut builder = ProjectionModelBuilder::new();
        builder.add_view(PrincipalViewKind::Front).unwrap();
        (builder, spatial)
    }

    #[test]
    fn depth_range_accepts_point_interval() {
        let depth = ProjectionDepthRange::new(5.0, 5.0).unwrap();
        assert!(depth.is_point());
    }

    #[test]
    fn depth_range_rejects_inverted_interval() {
        assert_eq!(
            ProjectionDepthRange::new(5.0, 1.0),
            Err(ProjectionError::InvalidProjectionDepthRange { min: 5.0, max: 1.0 })
        );
    }

    #[test]
    fn depth_range_rejects_non_finite() {
        assert!(matches!(
            ProjectionDepthRange::new(f64::NAN, 5.0),
            Err(ProjectionError::InvalidProjectionDepthRange { .. })
        ));
        assert!(matches!(
            ProjectionDepthRange::new(0.0, f64::INFINITY),
            Err(ProjectionError::InvalidProjectionDepthRange { .. })
        ));
    }

    #[test]
    fn segment_accepts_valid_endpoints() {
        assert!(segment(0.0, 0.0, 1.0, 2.0).validate().is_ok());
    }

    #[test]
    fn segment_rejects_zero_length() {
        assert_eq!(
            segment(1.0, 2.0, 1.0, 2.0).validate(),
            Err(ProjectionError::InvalidProjectionGeometry {
                reason: InvalidProjectionGeometryReason::ZeroLengthSegment
            })
        );
    }

    #[test]
    fn projected_geometry_rejects_non_finite_coordinates() {
        assert!(matches!(
            segment(0.0, f64::NAN, 1.0, 2.0).validate(),
            Err(ProjectionError::InvalidProjectionGeometry {
                reason: InvalidProjectionGeometryReason::NonFiniteCoordinate
            })
        ));
        assert!(matches!(
            ProjectedGeometry::Circle {
                center: ProjectedPoint2 { u: 0.0, v: 0.0 },
                radius: f64::INFINITY,
            }
            .validate(),
            Err(ProjectionError::InvalidProjectionGeometry {
                reason: InvalidProjectionGeometryReason::NonFiniteCoordinate
            })
        ));
    }

    #[test]
    fn circle_rejects_non_positive_radius() {
        for radius in [0.0, -1.0] {
            assert_eq!(
                ProjectedGeometry::Circle {
                    center: ProjectedPoint2 { u: 0.0, v: 0.0 },
                    radius,
                }
                .validate(),
                Err(ProjectionError::InvalidProjectionGeometry {
                    reason: InvalidProjectionGeometryReason::NonPositiveRadius
                }),
                "radius {radius} must be rejected"
            );
        }
    }

    #[test]
    fn arc_rejects_invalid_radius_and_sweep() {
        let base = |radius, start_angle, end_angle| ProjectedGeometry::Arc {
            center: ProjectedPoint2 { u: 0.0, v: 0.0 },
            radius,
            start_angle,
            end_angle,
        };
        for geometry in [
            base(0.0, 0.0, std::f64::consts::FRAC_PI_2),
            base(-1.0, 0.0, std::f64::consts::FRAC_PI_2),
            base(1.0, 0.0, 0.0),
            base(1.0, 0.0, std::f64::consts::TAU),
            base(1.0, 0.0, f64::NAN),
        ] {
            assert!(geometry.validate().is_err(), "invalid arc must fail closed");
        }
    }

    #[test]
    fn arc_bounds_use_endpoints_and_only_reached_cardinals() {
        let arc = ProjectedGeometry::Arc {
            center: ProjectedPoint2 { u: 10.0, v: 20.0 },
            radius: 5.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_2,
        };
        assert!(arc.validate().is_ok());
        let (min_u, max_u, min_v, max_v) = arc.bound_points().into_iter().fold(
            (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ),
            |(min_u, max_u, min_v, max_v), (u, v)| {
                (min_u.min(u), max_u.max(u), min_v.min(v), max_v.max(v))
            },
        );
        assert_eq!((min_u, max_u, min_v, max_v), (10.0, 15.0, 20.0, 25.0));
    }

    #[test]
    fn projection_bounds_rejects_non_finite() {
        let bounds = ProjectionBounds2D {
            min_u: 0.0,
            max_u: f64::NAN,
            min_v: 0.0,
            max_v: 1.0,
        };
        assert!(matches!(
            bounds.validate(),
            Err(ProjectionError::InvalidProjectionBounds { .. })
        ));
    }

    #[test]
    fn projection_bounds_rejects_inverted() {
        let bounds = ProjectionBounds2D {
            min_u: 1.0,
            max_u: 0.0,
            min_v: 0.0,
            max_v: 1.0,
        };
        assert!(matches!(
            bounds.validate(),
            Err(ProjectionError::InvalidProjectionBounds { .. })
        ));
    }

    #[test]
    fn next_id_format_matches_frozen_spec() {
        let (builder, _spatial) = builder_with_view();
        assert_eq!(
            builder
                .next_id(PrincipalViewKind::Front, &SpatialElementRef::Body)
                .0,
            "front:body:p:0000"
        );
        let cut = SpatialElementRef::Cut {
            id: SpatialCutId("a_hole:cut:0000".into()),
        };
        assert_eq!(
            builder.next_id(PrincipalViewKind::Left, &cut).0,
            "left:a_hole:cut:0000:p:0000"
        );
    }

    #[test]
    fn next_id_increments_per_view_and_source() {
        let (mut builder, spatial) = builder_with_view();
        builder.add_view(PrincipalViewKind::Left).unwrap();
        let cut = SpatialElementRef::Cut {
            id: SpatialCutId("a_hole:cut:0000".into()),
        };
        builder
            .add_primitive(
                PrincipalViewKind::Front,
                builder.next_id(PrincipalViewKind::Front, &SpatialElementRef::Body),
                segment(0.0, 0.0, 1.0, 1.0),
                ProjectionPrimitiveRole::BodyBoundary,
                body_depth(),
                SpatialElementRef::Body,
                &spatial,
            )
            .unwrap();
        assert_eq!(
            builder
                .next_id(PrincipalViewKind::Front, &SpatialElementRef::Body)
                .0,
            "front:body:p:0001"
        );
        assert_eq!(
            builder.next_id(PrincipalViewKind::Left, &cut).0,
            "left:a_hole:cut:0000:p:0000"
        );
    }

    /// 11: Serialized view keys appear in canonical Front < Top < Left
    /// order (BTreeMap keyed by the contractual enum order), and primitive
    /// keys appear in id-ascending order.
    #[test]
    fn view_keys_serialize_in_canonical_order() {
        let (mut builder, spatial) = builder_with_view();
        for kind in [
            PrincipalViewKind::Left,
            PrincipalViewKind::Top,
            PrincipalViewKind::Front,
        ] {
            builder.add_view(kind).unwrap();
        }
        let cut = SpatialElementRef::Cut {
            id: SpatialCutId("a_hole:cut:0000".into()),
        };
        builder
            .add_primitive(
                PrincipalViewKind::Top,
                builder.next_id(PrincipalViewKind::Top, &cut),
                ProjectedGeometry::Circle {
                    center: ProjectedPoint2 { u: 0.0, v: 0.0 },
                    radius: 5.0,
                },
                ProjectionPrimitiveRole::CutBoundary,
                ProjectionDepthRange::new(0.0, 0.0).unwrap(),
                cut.clone(),
                &spatial,
            )
            .unwrap();
        builder
            .add_primitive(
                PrincipalViewKind::Front,
                builder.next_id(PrincipalViewKind::Front, &SpatialElementRef::Body),
                segment(0.0, 0.0, 1.0, 1.0),
                ProjectionPrimitiveRole::BodyBoundary,
                body_depth(),
                SpatialElementRef::Body,
                &spatial,
            )
            .unwrap();
        let json = serde_json::to_string_pretty(&builder.finish().unwrap()).unwrap();
        let front = json.find("\"front\"").expect("front view key");
        let top = json.find("\"top\"").expect("top view key");
        assert!(front < top, "view keys must serialize Front before Top");
        assert!(json.find("Left").is_none(), "unadded Left must be absent");
    }

    /// ProjectedGeometry serializes with internally-tagged `kind` and
    /// snake_case variant names, and roundtrips losslessly.
    #[test]
    fn projected_geometry_serde_is_internally_tagged() {
        let circle = ProjectedGeometry::Circle {
            center: ProjectedPoint2 { u: 1.0, v: 2.0 },
            radius: 3.0,
        };
        let json = serde_json::to_string(&circle).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"circle","center":{"u":1.0,"v":2.0},"radius":3.0}"#
        );
        assert_eq!(
            serde_json::from_str::<ProjectedGeometry>(&json).unwrap(),
            circle
        );

        let seg = segment(0.0, 0.0, 1.0, 1.0);
        let json = serde_json::to_string(&seg).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"segment","start":{"u":0.0,"v":0.0},"end":{"u":1.0,"v":1.0}}"#
        );
        assert_eq!(
            serde_json::from_str::<ProjectedGeometry>(&json).unwrap(),
            seg
        );
    }

    /// SpatialElementRef serializes with internally-tagged `element` and
    /// keeps the cut id opaque; it roundtrips and orders deterministically.
    #[test]
    fn spatial_element_ref_serde_and_order() {
        let body = SpatialElementRef::Body;
        let json = serde_json::to_string(&body).unwrap();
        assert_eq!(json, r#"{"element":"body"}"#);
        assert_eq!(
            serde_json::from_str::<SpatialElementRef>(&json).unwrap(),
            body
        );

        let cut = SpatialElementRef::Cut {
            id: SpatialCutId("a_hole:cut:0000".into()),
        };
        let json = serde_json::to_string(&cut).unwrap();
        assert_eq!(
            json, r#"{"element":"cut","id":"a_hole:cut:0000"}"#,
            "the cut id must stay verbatim (opaque)"
        );
        assert_eq!(
            serde_json::from_str::<SpatialElementRef>(&json).unwrap(),
            cut
        );
        assert!(body < cut);
    }

    /// Bounds include circle centers ± radius on both axes and both
    /// segment endpoints; degenerate envelopes stay legal.
    #[test]
    fn finish_bounds_cover_circle_radii_and_endpoints() {
        let (mut builder, spatial) = builder_with_view();
        builder
            .add_primitive(
                PrincipalViewKind::Front,
                ProjectionPrimitiveId("front:a_hole:cut:0000:p:0000".into()),
                ProjectedGeometry::Circle {
                    center: ProjectedPoint2 { u: 0.0, v: 0.0 },
                    radius: 5.0,
                },
                ProjectionPrimitiveRole::CutBoundary,
                ProjectionDepthRange::new(0.0, 0.0).unwrap(),
                SpatialElementRef::Cut {
                    id: SpatialCutId("a_hole:cut:0000".into()),
                },
                &spatial,
            )
            .unwrap();
        let model = builder.finish().unwrap();
        let bounds = &model.views[&PrincipalViewKind::Front].bounds;
        assert_eq!(bounds.min_u, -5.0);
        assert_eq!(bounds.max_u, 5.0);
        assert_eq!(bounds.min_v, -5.0);
        assert_eq!(bounds.max_v, 5.0);
    }
}
