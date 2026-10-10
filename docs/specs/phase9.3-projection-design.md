# Phase 9.3 — Orthographic Projection Model: Design Freeze (ADR-034 basis)

Status: **FROZEN design** — no code written yet. This document is the binding
contract for `crates/scc-projection`. It consumes ADR-032 (canonical part-local
frame) and ADR-033 (2.5-D spatial lowering) and adds no second readiness gate
(ADR-033 Decision 13).

---

## 1. Projection Readiness Audit

**Verdict: inputs are sufficient.** A successfully constructed `SpatialModel`
is projection-input-ready by construction (ADR-033 Decision 13). No re-gate,
no re-validation of engineering state, no re-solving.

Assumptions relied on (verified against source):

- **Claim integrity (ADR-033 Decisions 5/6/10).** Every solved geometry entity
  is claimed exactly once; cuts are unique per `source_entity`;
  `construction_circles` are reference-only. `scc-projection` relies on this
  and does not re-verify claim coverage — but DOES re-verify that every
  `SpatialElementRef` it stores resolves inside the referenced
  `SpatialModel` (`validate_against`, §3).
- **Canonical frame (ADR-032 spatial.rs).** XY plane at `z = 0` is the Base
  face, +Z points out of the material, body spans `[0, T]`. Projection
  consumers may only USE this frame, never redefine it. The three principal
  views below are pure functions of this convention.
- **Rectangle-only footprint (ADR-033 Decision 4).** `SpatialFootprint` has
  exactly one variant (`Rectangle`). The body projects to 4 segments per
  front-like view and 1 rectangle-outlines-as-4-segments in top view; no
  polygon handling needed in 9.3.
- **`Bounds3D` is body-footprint-derived** (cuts never expand bounds,
  ADR-033 Decision 8) — usable directly for depth-range derivation.
- **`SpatialCutId` is opaque** (ADR-033 Decision 9): carried, compared,
  ordered, serialized — never parsed. `scc-projection` never reads
  `FeatureId` out of id strings.
- **`ZInterval` / `Bounds3D` are NOT reusable for depth ranges**: both reject
  `min == max` and `ZInterval` additionally rejects `min < 0`. Depth ranges
  need `min == max` legal (a planar face seen edge-on) and may legitimately
  be negative (Left view depth = −x). Hence the new `ProjectionDepthRange`
  (§5). This is a deliberate non-reuse, not an oversight.

Gaps / explicit non-dependencies:

- `ProjectionMethod` (First/Third angle) in `scc-engineering` is stored intent
  only (grep: zero consumers in geometry code). Phase 9.3 does NOT read it.
  FirstAngle vs ThirdAngle affects only sheet **placement** (9.7), never
  per-view geometry; nothing in 9.3 or 9.4 branches on it.
- `DrawingViewKind::Principal` in `scc-drawing` is a tag only; scc-projection
  has its own `PrincipalViewKind` and takes no dependency on `scc-drawing`.
- No `Point3`/`Vec3` exists anywhere in the workspace; the minimal
  `PartLocalPoint3` is introduced here (§3) and deliberately not promoted to
  `scc-geometry` (nothing else needs it).
- Known repo debt left untouched: `docs/adr/README.md` missing ADR-030/031/032
  rows. The ADR-034 row IS added when ADR-034 lands (allowed by task §91).

---

## 2. Principal Frame Proposal

Three canonical orthographic frames over the canonical part-local axes
(+X, +Y, +Z of ADR-032). One real direction per frame is named
`toward_viewer`: the direction **from the part toward the viewer**.
`depth = coordinate along toward_viewer`; **larger depth = nearer viewer**.

### 2.1 FINAL frame table (the only normative table; any earlier draft void)

Frames are constructed so that `right × up = toward_viewer` (right-handed)
and the brief's exact formulas hold:

| View  | `right`  | `up`     | `toward_viewer` | u formula | v formula | depth formula | Viewer stands at |
| ----- | -------- | -------- | --------------- | --------- | --------- | ------------- | ---------------- |
| Front | `PosX`   | `PosY`   | `PosZ`          | `u = +x`  | `v = +y`  | `d = +z`      | +Z side, looking −Z |
| Top   | `PosX`   | `NegZ`   | `PosY`          | `u = +x`  | `v = −z`  | `d = +y`      | +Y side, looking −Y |
| Left  | `PosZ`   | `PosY`   | `NegX`          | `u = +z`  | `v = +y`  | `d = −x`      | −X side, looking +X |

Explicit cross products (hand-verified, and locked by
`frame_cross_products_match_names` test):

- Front: `PosX × PosY = PosZ` ✓ (x̂ × ŷ = ẑ)
- Top:   `PosX × NegZ = PosY` ✓ (x̂ × (−ẑ) = −(x̂ × ẑ) = −(−ŷ) = ŷ)
- Left:  `PosZ × PosY = NegX` ✓ (ẑ × ŷ = −x̂)

Physical sanity: the Front viewer at +Z sees the part's top face (the Base
face is at z = 0, farthest, depth 0; the Opposite face at z = T is nearest,
depth T) — so a Front view of the bare plate is the plate outline, and hole
depth evidence ("near face / far face") reads directly off `d`. The Top
viewer at +Y sees the XY sketch plane; paper-up on the Top view is −Z
(material depth runs down the paper). The Left viewer at −X looks along +X;
larger x is farther, hence depth = −x.

### 2.2 Alignment invariants (locked by tests)

- `Front.u == Top.u` (both +x): horizontal alignment between front and top.
- `Front.v == Left.v` (both +y): vertical alignment between front and left.
- `Left.u == −Top.v` (Left.u = +z; Top.v = −z): the left view's horizontal
  axis is the top view's vertical axis mirrored, per standard orthographic
  layout.
- All three frames are orthonormal, right-handed, and pairwise-axis-distinct.

### 2.3 Exact ADR-034 formulas to write

```
project(p, Front) = (u = p.x,  v = p.y,  depth = p.z)
project(p, Top)   = (u = p.x,  v = −p.z, depth = p.y)
project(p, Left)  = (u = p.z,  v = p.y,  depth = −p.x)

depth_range(bounds, Front) = [bounds.min_z, bounds.max_z]
depth_range(bounds, Top)   = [bounds.min_y, bounds.max_y]
depth_range(bounds, Left)  = [−bounds.max_x, −bounds.min_x]
```

### 2.4 Implementation strategy

`OrthographicFrame::project_point` is implemented **generically over the
frame** (u = a·right, v = a·up, depth = a·toward_viewer, where "a·axis" is a
signed coordinate read off the named axis — a match on `AxisDirection`, not a
floating-point matrix). One code path; the three canonical constructors
`front()/top()/left()` produce the only frames that can exist, and each gets
an exact-formula test. No `Matrix3`, no floats in the frame itself — the
frame is three `AxisDirection`s, so construction is total and validation
(orthonormality) reduces to "the three directions are pairwise distinct and
the cross product of their names equals the third name", both exact integer
checks.

---

## 3. Projection Model Proposal

### 3.1 Public types (exact Rust sketches)

All in `crates/scc-projection`; lib.rs re-export style mirrors `scc-spatial`
(`pub use model::{...}` etc.). Units: millimetres throughout, scale always
1:1. No `DrawingScale`, no sheet space, no margins.

```rust
// frame.rs ---------------------------------------------------------------

/// A canonical part-local axis direction. Exact enum, no floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisDirection { PosX, NegX, PosY, NegY, PosZ, NegZ }

/// Which canonical principal view a frame belongs to.
///
/// Discriminant order IS the canonical order `Front < Top < Left` — locked
/// with explicit doc + test (`principal_view_kind_order_is_front_top_left`),
/// never left to alphabetical accident. (Alphabetically F < T < L happens to
/// agree, but the ordering is contractual, not incidental.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalViewKind { Front, Top, Left }

/// A right-handed orthonormal view basis: `right × up == toward_viewer`.
/// Three named axes only — no matrices, no floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrthographicFrame {
    pub kind: PrincipalViewKind,
    pub right: AxisDirection,
    pub up: AxisDirection,
    pub toward_viewer: AxisDirection,
}

impl OrthographicFrame {
    pub fn front() -> Self; // PosX, PosY, PosZ
    pub fn top()  -> Self;  // PosX, NegZ, PosY
    pub fn left() -> Self;  // PosZ, PosY, NegX

    /// Exact named-axis cross product (integer match, total).
    pub fn cross(a: AxisDirection, b: AxisDirection) -> Option<AxisDirection>;

    /// Fail-closed validation: pairwise-distinct axes and
    /// right × up == toward_viewer. Canonical constructors always pass;
    /// this exists for deserialized values (defensive, see §6.3).
    pub fn validate(&self) -> Result<(), ProjectionError>;

    /// Generic single-code-path projection:
    /// u = p·right, v = p·up, depth = p·toward_viewer (signed coordinate
    /// along the named axis).
    pub fn project_point(&self, p: PartLocalPoint3) -> ProjectedPoint;

    /// Depth range of a bounds box in this frame (§5.2 formulas).
    pub fn depth_range(&self, b: Bounds3D)
        -> Result<ProjectionDepthRange, ProjectionError>;
}

/// Minimal 3-D point in the canonical part-local frame (mm).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PartLocalPoint3 { pub x: f64, pub y: f64, pub z: f64 }

/// A projected point. `depth` per the frozen convention:
/// LARGER depth = NEARER viewer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectedPoint { pub u: f64, pub v: f64, pub depth: f64 }

/// A 2-D projected coordinate (no depth) — the drawing-relevant part.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectedPoint2 { pub u: f64, pub v: f64 }

// model.rs ---------------------------------------------------------------

/// Closed depth interval along a frame's `toward_viewer` axis (mm).
/// Finite, `min <= max`; `min == max` is LEGAL (edge-on face).
/// Deliberately NOT `ZInterval` (which rejects min==max and min<0).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDepthRange { pub min: f64, pub max: f64 }

impl ProjectionDepthRange {
    pub fn new(min: f64, max: f64) -> Result<Self, ProjectionError>;
    pub fn is_point(&self) -> bool { self.min == self.max }
}

/// Axis-aligned envelope in view (u, v) space (mm). Finite, `max_u >= min_u`
/// and `max_v >= min_v` (degenerate legal — a planar-face-only view).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectionBounds2D {
    pub min_u: f64, pub max_u: f64, pub min_v: f64, pub max_v: f64,
}

/// The role of a projected primitive. Exactly two variants in 9.3;
/// no Hidden/Visible/Construction role exists (visibility is 9.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPrimitiveRole { BodyBoundary, CutBoundary }

/// One projected primitive. Provenance is NOT embedded (§4.3);
/// depth range IS embedded (§5).
///
/// ARC DECISION: omitted in 9.3 — justification in §3.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectedGeometry {
    Segment { start: ProjectedPoint2, end: ProjectedPoint2 },
    Circle  { center: ProjectedPoint2, radius: f64 },
}

/// One principal view: frozen frame + primitives + stored-validated bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionView {
    pub kind: PrincipalViewKind,
    pub frame: OrthographicFrame,
    pub primitives: BTreeMap<ProjectionPrimitiveId, ProjectionPrimitive>,
    pub bounds: ProjectionBounds2D,   // recomputed + validated at finish()
}

// source_map.rs + model.rs ----------------------------------------------

/// Opaque, deterministic, serde-stable id of one projected primitive.
/// Format frozen in §4.1; never parsed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize)]
pub struct ProjectionPrimitiveId(pub String);

impl std::fmt::Display for ProjectionPrimitiveId { /* self.0 */ }

/// Reference to the spatial element a projected primitive came from.
/// Opaque over `SpatialCutId` — never a parsed FeatureId string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize)]
#[serde(tag = "element", rename_all = "snake_case")]
pub enum SpatialElementRef {
    /// The spatial body.
    Body,
    /// One cylindrical cut, by its opaque spatial id.
    Cut { id: SpatialCutId },
}

/// The projection model: one view per principal kind, keyed by the
/// deterministic enum order Front < Top < Left, plus the provenance map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionModel {
    pub views: BTreeMap<PrincipalViewKind, ProjectionView>,
    pub sources: ProjectionSourceMap,   // stored alongside; single artifact
}
```

### 3.2 Builder API surface (exact signatures)

Fail-closed; no pub mutators on the finished model; no `*_mut()` accessors
(the Phase 8 debt pattern is not repeated).

```rust
pub struct ProjectionModelBuilder { /* private staged state */ }

impl ProjectionModelBuilder {
    pub fn new() -> Self;

    /// Install the frozen canonical frame for `kind`. Idempotent: adding an
    /// already-present kind is a no-op success (Decision 14 / test 25).
    /// Frames are created internally — callers never supply custom axes.
    pub fn add_view(&mut self, kind: PrincipalViewKind)
        -> Result<(), ProjectionError>;

    /// Insert one primitive. Fail-closed on: non-finite coordinates,
    /// zero-length segment, non-positive radius, depth min > max,
    /// non-finite bounds contribution, duplicate id, view not added, or a
    /// `SpatialElementRef::Cut { id }` absent from `spatial.cuts`
    /// (insert-time source integrity gate).
    pub fn add_primitive(
        &mut self,
        view: PrincipalViewKind,
        id: ProjectionPrimitiveId,
        geometry: ProjectedGeometry,
        role: ProjectionPrimitiveRole,
        depth: ProjectionDepthRange,
        source: SpatialElementRef,
        spatial: &SpatialModel,
    ) -> Result<(), ProjectionError>;

    /// Deterministic id from (view, source, current occurrence count) —
    /// §4.1 format. Pure; reads staged state only.
    pub fn next_id(
        &self,
        view: PrincipalViewKind,
        source: &SpatialElementRef,
    ) -> ProjectionPrimitiveId;

    /// Finish: recompute + validate per-view bounds, freeze into a
    /// read-only `ProjectionModel`. Zero views + zero primitives is legal
    /// (§3.6); primitives staged without their view is
    /// `MissingProjectionView`.
    pub fn finish(self) -> Result<ProjectionModel, ProjectionError>;
}

impl ProjectionModel {
    /// Integrity checks decodable without a spatial model: every primitive
    /// in every view has a source-map entry; every stored geometry is
    /// finite; every view's frame passes `OrthographicFrame::validate`.
    pub fn validate_integrity(&self) -> Result<(), ProjectionError>;

    /// Cross-check against the originating spatial model: every
    /// `SpatialElementRef::Cut { id }` exists in `spatial.cuts`.
    /// The gate 9.4 runs after building; also the post-deserialize gate.
    pub fn validate_against(&self, spatial: &SpatialModel)
        -> Result<(), ProjectionError>;

    /// All primitives derived from one spatial element, per view.
    pub fn primitives_for_element(
        &self,
        element: &SpatialElementRef,
    ) -> BTreeMap<PrincipalViewKind, BTreeSet<ProjectionPrimitiveId>>;

    /// The spatial element one primitive came from, if known.
    pub fn element_for_primitive(
        &self,
        primitive: &ProjectionPrimitiveId,
    ) -> Option<&SpatialElementRef>;
}
```

### 3.3 pub vs pub(crate)

- **pub**: all §3.1 types; builder methods; `validate_integrity`;
  `validate_against`; both reverse queries; `ProjectionDepthRange::new` /
  `is_point`; `OrthographicFrame::{front, top, left, cross, validate,
  project_point, depth_range}`.
- **pub(crate)**: the per-view incremental bounds accumulator, the id-index
  allocator internals, `ProjectionSourceMap::record` internals.
- **absent**: `Default` for `ProjectionModel` (no empty model outside the
  builder), any mutators, and — per the brief — any
  `lower_projection()` / `project_principal_views()` (9.3 is types + math +
  validation only; the lowering function itself is 9.4).

### 3.4 Arc decision: OMITTED (with justification)

Foreseeable 9.4 output for the Phase 9.2 spatial domain (rectangular plate
footprint + cylindrical cuts with axes parallel to +Z):

- **Top view**: a cut's axis points at the viewer → the cut projects to a
  **Circle**; the body footprint projects to 4 **Segments**.
- **Front / Left views**: the plate projects to 4 Segments; each cut projects
  to its silhouette rectangle (4 Segments — the cylinder wall seen edge-on
  spans [z_min, z_max] horizontally by diameter, and the openings are
  vertical segment edges). No curved silhouette exists for a cylinder whose
  axis is perpendicular to the view direction only in the degenerate sense
  used here: the axis is parallel to the Front/Left view plane, so the
  silhouette is exactly two parallel lines + two end segments — still
  segments.
- Arcs first appear with non-axis-aligned cylinders, fillets, or arc-edge
  profiles — none spatializable in ADR-033. YAGNI applies.

Frozen extension path: add
`Arc { center: ProjectedPoint2, radius: f64, start_angle: f64, end_angle: f64 }`
to `ProjectedGeometry` — purely additive serde variant, no existing variant
or API changes (same discipline as every prior additive enum here).
`Rectangle` primitive is likewise omitted: a projected rectangle is 4
segments, so 9.4 emits exactly two primitive kinds.

### 3.5 Serde & persistence

`Serialize + Deserialize + PartialEq` everywhere, for tests / debugging /
deterministic transport only (ADR-033 Decision 12 pattern). Deserialization
does NOT auto-validate (Phase 9.2 debt pattern, retained deliberately): any
deserialized model must be passed through `validate_integrity()` (or
`validate_against`) before use; documented on the types. No schema system.

### 3.6 Empty-model policy (decision)

**Empty is legal.** `finish()` with zero staged views and zero primitives
succeeds, producing a model with an empty `views` map. Staged primitives
without their view → `MissingProjectionView` (SCC-PROJ-001). Rationale:
projection is a pure derived artifact and emptiness must stay representable;
a real `SpatialModel` always has a body, so an empty projection can only be
deliberate.

---

## 4. Provenance Proposal

### 4.1 ProjectionPrimitiveId construction (frozen format)

```text
{view}:{source}:p:{index:04}

view    = "front" | "top" | "left"
{source} = "body" | <spatial_cut_id verbatim, opaque>
index   = zero-based occurrence of THIS source within THIS view, in builder
          insertion order (staged per (view, source) counter)
```

Examples: `front:body:p:0000`; `top:a_hole:cut:0000:p:0000`;
`left:a_hole:cut:0000:p:0001` (a cut's opening edge and far edge = occurrences
0 and 1). Ord = String lexical; Display = the string; serde = plain string;
never random, never coordinate-derived, never parsed back for meaning.

### 4.2 SpatialElementRef

The two-variant enum of §3.1 (`Body`, `Cut { id: SpatialCutId }`). It names
spatial elements opaquely; the chain
`ProjectionPrimitiveId → SpatialElementRef → CylindricalCut →
FeatureId/EntityId` completes by looking up the cut in the `SpatialModel`.
**No FeatureId/EntityId appears in scc-projection**: copying them would
create a second, drifting copy of provenance truth — the exact
double-bookkeeping ADR-033 Decision 10 eliminated one level down. (Also
satisfies the brief: no FeatureId string parsing, `SpatialCutId` used
opaquely.)

### 4.3 Where provenance lives (decision)

**Separate map, stored inside the model** — the `DrawingSourceMap` pattern
(scc-drawing Phase 8), not per-element embedding (the ADR-033 pattern).
Deciding factor: one spatial element legitimately produces MANY projected
primitives per view (a cut → up to 4 segments per front-like view, or 1
circle in top), so embedding would duplicate one `SpatialElementRef` N times;
a `BTreeMap<ProjectionPrimitiveId, SpatialElementRef>` stores it once.
Stored inside `ProjectionModel.sources` so a serialized model is
self-contained (no parallel artifact to lose).

### 4.4 ProjectionSourceMap shape & reverse-query strategy

```rust
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectionSourceMap {
    primitive_sources: BTreeMap<ProjectionPrimitiveId, SpatialElementRef>,
}

impl ProjectionSourceMap {
    pub(crate) fn record(&mut self, primitive: ProjectionPrimitiveId,
                         source: SpatialElementRef);
    pub fn element_for_primitive(&self, primitive: &ProjectionPrimitiveId)
        -> Option<&SpatialElementRef>;
    pub fn iter(&self)
        -> impl Iterator<Item = (&ProjectionPrimitiveId, &SpatialElementRef)>;
    /// Derived on demand — NEVER stored (no second index to drift).
    pub fn element_index(&self)
        -> BTreeMap<SpatialElementRef, BTreeSet<ProjectionPrimitiveId>>;
    pub fn primitives_for_element(&self, element: &SpatialElementRef)
        -> BTreeSet<ProjectionPrimitiveId>;
}
```

`ProjectionModel::primitives_for_element` composes the derived index and
groups by view. Reverse queries are scans over a small map at call time —
cheap, deterministic, zero drift. No BTreeMap keyed by geometry anywhere.

### 4.5 Source integrity gates

- **Insert time**: `add_primitive(..., spatial)` rejects
  `SpatialElementRef::Cut { id }` with `id ∉ spatial.cuts` →
  `InvalidProjectionSource` (SCC-PROJ-005). `Body` is always valid
  (SpatialModel always has a body). No synthetic fallback.
- **Model level**: `validate_against(&SpatialModel)` re-checks every entry —
  the post-build and post-deserialize gate 9.4 runs.
- `validate_integrity()`: deserialize-shape checks only (source map covers
  every primitive; all geometry finite; frames canonical) — it cannot check
  spatial existence without the model, which is exactly why
  `validate_against` exists as a separate, stricter gate.

---

## 5. Depth Evidence Proposal

### 5.1 ProjectionDepthRange semantics

Stored **on each `ProjectionPrimitive`** (no separate candidate type exists
in 9.3). `depth = [min, max]` along the view's `toward_viewer` axis;
`min <= max`, `min == max` legal, both finite; **larger = nearer viewer**.
Examples: Front view of the plate body → `[0, T]`; its base-face outline →
`[0, 0]`; a through-hole's Front-view silhouette rectangle → `[0, T]`; its
near-face opening edge (z = T) → `[T, T]`. Validation fail-closed at
`ProjectionDepthRange::new` and re-checked in `add_primitive`
(`InvalidProjectionDepthRange`, SCC-PROJ-003).

### 5.2 Bounds3D helper (exact)

```text
frame.depth_range(Bounds3D) per §2.3:
  Front → ProjectionDepthRange::new(bounds.min_z, bounds.max_z)
  Top   → ProjectionDepthRange::new(bounds.min_y, bounds.max_y)
  Left  → ProjectionDepthRange::new(-bounds.max_x, -bounds.min_x)
```

Total over valid `Bounds3D`; the only failure mode is non-finite (already
excluded by `Bounds3D::new`), so the `Result` is defensive symmetry with the
layer's fail-closed style.

### 5.3 §78 sanity check — sufficient for 9.5 hidden-line?

**Yes.** Hidden-line removal over analytic primitives needs (a) (u, v)
overlap between primitive pairs and (b) a depth ordering. `depth` gives (b)
as a closed interval — the correct granularity for a span (a cylinder wall
is not a point). And whenever 9.5 needs finer structure, it re-queries the
originating `SpatialModel` **analytically** via
`element_for_primitive → SpatialElementRef → spatial.cuts` — exact
center/radius/z-interval, no tessellation, no stored approximation. The
spatial model IS the depth oracle; a richer 9.3 structure (per-endpoint
depths, face ids, normals) would duplicate it. Anything 9.5 cannot decide
from `{geometry, role, depth, source}` + the spatial model is a 9.5 design
responsibility, not a 9.3 gap.

---

## 6. Error Model

### 6.1 scc-core change (additive-variant pattern)

```rust
/// Orthographic projection layer (Phase 9.3 `scc-projection`).
///
/// Additive variant for `SCC-PROJ-{NNN}` codes; never produced by the
/// legacy proposal→geometry pipeline.
Projection,
```

plus `ErrorStage::Projection => "PROJ"` in `prefix()` and
`write!(f, "projection")` in `Display` — added together (validate_contract
requires code segment == stage prefix).

### 6.2 ProjectionError (complete reachable set — 6 codes)

```rust
/// A failure while building or validating a [`crate::ProjectionModel`]
/// (Phase 9.3, ADR-034). All failures are fail-closed: no projection model
/// is ever partially assembled or silently repaired.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ProjectionError {
    /// A primitive was staged for a view that was never added
    /// (`SCC-PROJ-001`).
    #[error("projection view {kind:?} was not added before use")]
    MissingProjectionView {
        kind: PrincipalViewKind,
    },

    /// A projected primitive is geometrically invalid: non-finite
    /// coordinate, zero-length segment, or non-positive radius
    /// (`SCC-PROJ-002`).
    #[error("projected geometry is invalid: {reason:?}")]
    InvalidProjectionGeometry {
        reason: InvalidProjectionGeometryReason,
    },

    /// A depth interval is invalid (non-finite or `min > max`)
    /// (`SCC-PROJ-003`). `min == max` is legal and never rejected.
    #[error("invalid projection depth range [{min}, {max}]")]
    InvalidProjectionDepthRange { min: f64, max: f64 },

    /// A view bounds envelope is invalid (non-finite)
    /// (`SCC-PROJ-004`).
    #[error("invalid projection bounds {bounds:?}")]
    InvalidProjectionBounds {
        bounds: ProjectionBounds2D,
    },

    /// A primitive references a spatial element that does not exist in the
    /// referenced `SpatialModel` (`SCC-PROJ-005`). Fail-closed source
    /// integrity; never a synthetic fallback.
    #[error(
        "projection primitive {id} references a spatial element \
         missing from the spatial model"
    )]
    InvalidProjectionSource {
        id: ProjectionPrimitiveId,
    },

    /// A primitive id collides with an existing primitive in the same view
    /// (`SCC-PROJ-006`). Reachable because callers may supply arbitrary ids.
    #[error("projection primitive id {id} already exists in view {kind:?}")]
    DuplicateProjectionPrimitive {
        id: ProjectionPrimitiveId,
        kind: PrincipalViewKind,
    },
}

/// Why a projected primitive is invalid.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidProjectionGeometryReason {
    /// A coordinate (x/y/z/u/v/depth/radius) is not finite.
    NonFiniteCoordinate,
    /// A segment's start and end are identical (zero length is rejected).
    ZeroLengthSegment,
    /// A circle radius is zero or negative.
    NonPositiveRadius,
}

impl ProjectionError {
    pub fn code(&self) -> &'static str {
        match self {
            ProjectionError::MissingProjectionView { .. } => "SCC-PROJ-001",
            ProjectionError::InvalidProjectionGeometry { .. } => "SCC-PROJ-002",
            ProjectionError::InvalidProjectionDepthRange { .. } => "SCC-PROJ-003",
            ProjectionError::InvalidProjectionBounds { .. } => "SCC-PROJ-004",
            ProjectionError::InvalidProjectionSource { .. } => "SCC-PROJ-005",
            ProjectionError::DuplicateProjectionPrimitive { .. } => "SCC-PROJ-006",
        }
    }
}

impl From<ProjectionError> for StructuredError {
    fn from(error: ProjectionError) -> Self {
        // Fail-closed by contract: severity Error, recoverable false, no
        // extra context (ADR-033 Decision 11 pattern).
        StructuredError::new(error.code(), ErrorStage::Projection, error.to_string())
    }
}
```

### 6.3 Trimmed from the brief's suggestion list

| Brief variant | Verdict | Reason |
| --- | --- | --- |
| `InvalidProjectionCoordinate` | merged into `InvalidProjectionGeometry { NonFiniteCoordinate }` | same gate, same remedy; one fewer code |
| `InvalidProjectionFrame` | removed | the frozen API exposes only the three total canonical constructors; a custom-frame path does not exist, so no frame error is reachable. `OrthographicFrame::validate` (defensive, post-deserialize) maps its unreachable failure to `InvalidProjectionGeometry { NonFiniteCoordinate }`. |
| `DuplicateProjectionView` | removed | `add_view` on an existing kind is defined idempotent (a no-op success); a duplicate view is not an error state. |
| `MissingProjectionSource` | removed | provenance is recorded atomically with the primitive; "missing source" is unrepresentable. Source *integrity* is `InvalidProjectionSource`. |
| `InvalidProjectionDepthRange` | kept | distinct numeric-interval gate (`min == max` must stay legal). |
| `InvalidProjectionBounds` | kept | distinct stored-envelope gate. |

Added beyond the brief: `DuplicateProjectionPrimitive` — reachable because
`add_primitive` accepts caller-supplied ids; fail-closed required.

Registry: `docs/error-codes.md` gains `## Projection errors (`SCC-PROJ-*`)`
using the standard template (phase 9.3 / `scc-projection` preamble: additive,
upstream stages never emit them, `ProjectionError → StructuredError` under
`ErrorStage::Projection`, all fail-closed; then the code/variant/meaning
table). Registry updated first; codes start at 001 (new stage); every variant
gets `From` coverage + `validate_contract` unit test. ADR-002/changelog note:
additive stage, no compatibility impact.

---

## 7. Phase 9.4 Consumption Contract

Exact checklist 9.4 uses (nothing else exists):

```rust
let mut b = ProjectionModelBuilder::new();
for kind in [PrincipalViewKind::Front, PrincipalViewKind::Top,
             PrincipalViewKind::Left] {
    b.add_view(kind)?;                              // frozen canonical frame
}
let frame = model_view_frame;                       // from view.frame
let depth = frame.depth_range(spatial.bounds)?;     // §5.2 helper
let p     = frame.project_point(PartLocalPoint3 { x, y, z }); // §2.3 formulas
b.add_primitive(
    PrincipalViewKind::Front,
    b.next_id(PrincipalViewKind::Front, &SpatialElementRef::Body),
    ProjectedGeometry::Segment { start, end },
    ProjectionPrimitiveRole::BodyBoundary,
    ProjectionDepthRange::new(0.0, 0.0)?,
    SpatialElementRef::Body,
    &spatial,                                       // insert-time integrity
)?;
let model = b.finish()?;
model.validate_against(&spatial)?;                  // 9.4's final gate
// queries: element_for_primitive / primitives_for_element,
// view.bounds, primitive.depth, frame.project_point, view.frame
```

9.3 deliberately does NOT provide:

- No `lower_projection()` / `project_principal_views()` — 9.4's builder owns
  which primitives exist.
- No visibility / hidden / occluded state; no z-ordering; no coincident-
  primitive dedup; no silhouette or intersection math.
- No sheet layout, `DrawingScale`, margins, or `ProjectionMethod` read; no
  `scc-drawing` dependency (integration is 9.7).
- No persistence schema — serde is tests/debug/transport only.
- No exported tolerance constants (9.5's coincidence policy is its own;
  `SPATIAL_LENGTH_TOLERANCE` stays `pub(crate)` in scc-spatial).

---

## 8. ADR-034 decision list

1. **New leaf crate `crates/scc-projection`** (edition 2021): deps exactly
   `scc-core`, `scc-spatial`, `serde(derive)`, `thiserror = "1"`; dev-dep
   `serde_json`. No `scc-drawing`; no engineering types named directly —
   `SpatialElementRef` uses `SpatialCutId` opaquely plus a `Body` variant.
2. **No matrices/camera**: `OrthographicFrame` is three `AxisDirection`
   fields; projection is a signed-coordinate read per named axis. No
   `Matrix3/4`, quaternion, vector package, perspective, or camera state
   (extends ADR-033 non-goal 5).
3. **mm, 1:1 always**: no `DrawingScale`, no sheet space, no margins, no
   view transforms — 9.3 output is pure (u, v, depth) view space; sheet
   placement stays with scc-drawing (9.7, ADR-024).
4. **Frozen frame table (§2.1)**: Front(PosX, PosY, PosZ; u=+x, v=+y,
   d=+z), Top(PosX, NegZ, PosY; u=+x, v=−z, d=+y), Left(PosZ, PosY, NegX;
   u=+z, v=+y, d=−x); each satisfies right × up = toward_viewer; larger
   depth = nearer.
5. **`PrincipalViewKind { Front, Top, Left }` with contractual Ord** =
   Front < Top < Left, locked by doc + test (not left to alphabetical
   coincidence).
6. **Generic single-path projection**: one `project_point` implementation
   over the frame; canonical constructors total; per-frame exact-formula
   tests; three cross-view alignment invariants locked by tests.
7. **`AxisDirection` / `PartLocalPoint3` live in scc-projection** (minimal,
   no arithmetic API); not promoted to scc-geometry.
8. **New `ProjectionDepthRange`** (finite, `min <= max`, `min == max` legal,
   negatives legal) instead of reusing `ZInterval`/`Bounds3D`, which both
   reject degenerate intervals — deliberate non-reuse, documented.
9. **Projected geometry = Segment + Circle only; Arc and Rectangle omitted.**
   Justification §3.4; additive Arc extension path documented.
10. **Roles: `BodyBoundary` | `CutBoundary` only.** No Construction/Hidden/
    Visible roles; construction circles project to nothing (reference-only).
11. **`ProjectionPrimitiveId` format `{view}:{source}:p:{index:04}`** —
    deterministic from (view, opaque source, per-(view,source) occurrence
    index); String-lexical Ord; Display; serde string; never parsed.
12. **Provenance = separate `ProjectionSourceMap` stored inside
    `ProjectionModel`** (DrawingSourceMap pattern, not per-primitive
    embedding): one element → many primitives; reverse index derived on
    demand, never stored.
13. **No FeatureId/EntityId in scc-projection**: the chain completes by
    looking up the cut in the `SpatialModel` (`source_feature` /
    `source_entity`); copying ids would duplicate truth (ADR-033 Decision 10
    rationale, one level up).
14. **`add_view` is idempotent** (re-adding a kind is a no-op success) —
    removes `DuplicateProjectionView` from the error surface.
15. **Source integrity fail-closed at two points**: insert-time existence
    check in `add_primitive` and `validate_against(&SpatialModel)` re-check;
    both → `SCC-PROJ-005`. Deserialization does NOT auto-validate (Phase 9.2
    debt pattern retained; serde is tests/debug/transport only); callers run
    `validate_integrity()` / `validate_against()` explicitly.
16. **Stored, validated `ProjectionBounds2D` per view** (consistent with
    `Bounds3D`/`DrawingBounds` layer convention), recomputed by the builder
    at `finish()`; a derived helper exists for checks, the stored value is
    the contract.
17. **`ProjectionModel` is read-only, no `Default`, no pub mutators** — built
    once by `ProjectionModelBuilder::finish()`; consumers query only
    (extends ADR-033 Decision 1).
18. **Empty model is legal**: `finish()` with zero views + zero primitives
    succeeds; staged primitives without their view → `MissingProjectionView`.
    Rationale: projection is a pure derived artifact; emptiness stays
    representable.
19. **Zero-length segments rejected** (`ZeroLengthSegment`); non-finite
    anything → `NonFiniteCoordinate` (NaN caught by `is_finite`, never by
    relying on PartialEq traps); non-positive radius → `NonPositiveRadius`.
    Exact `==` for zero-length (no tolerance policy in 9.3).
20. **Own error stage `SCC-PROJ`** (6 codes, §6.2): `ErrorStage::Projection`
    additive variant + prefix `PROJ` + `From<ProjectionError> for
    StructuredError`, all fail-closed, every variant contract-tested
    (`all_variants` + `validate_contract`, spatial-layer precedent).
21. **Determinism locked like ADR-033 Decision 9**: `BTreeMap` for views,
    primitives, source map; fixed `PrincipalViewKind` Ord; deterministic
    ids; mandatory 100-run byte-identical-JSON test. Serde conventions
    copied from scc-spatial (internally tagged `kind`, snake_case).
22. **Docs**: `docs/error-codes.md` gains `## Projection errors
    (`SCC-PROJ-*`)` per the standard template (registry updated before
    code); ADR-034 row added to `docs/adr/README.md`; the missing
    ADR-030/031/032 index rows remain untouched known debt. File layout:
    `src/{lib.rs, frame.rs, model.rs, source_map.rs, error.rs}` +
    `tests/projection_model.rs`; lib.rs re-exports mirror scc-spatial.

---

## 9. Test matrix mapping

Brief §94's 37 minimum tests → concrete names (unit tests in-module where
scc-spatial does the same; integration in
`crates/scc-projection/tests/projection_model.rs`):

| # | Brief requirement | Test name |
| -- | -- | -- |
| 1 | Front cross product | `frame_front_cross_product_is_right_handed` |
| 2 | Top cross product | `frame_top_cross_product_is_right_handed` |
| 3 | Left cross product | `frame_left_cross_product_is_right_handed` |
| 4 | Front exact formulas | `front_project_point_exact_formulas` |
| 5 | Top exact formulas | `top_project_point_exact_formulas` |
| 6 | Left exact formulas | `left_project_point_exact_formulas` |
| 7 | Front.u == Top.u | `alignment_front_u_equals_top_u` |
| 8 | Front.v == Left.v | `alignment_front_v_equals_left_v` |
| 9 | Left.u == −Top.v | `alignment_left_u_equals_neg_top_v` |
| 10 | PrincipalViewKind Ord | `principal_view_kind_order_is_front_top_left` |
| 11 | Canonical key order in JSON | `view_keys_serialize_in_canonical_order` |
| 12 | AxisDirection completeness/serde | `axis_direction_roundtrips_all_six` |
| 13 | Depth range Front | `depth_range_front_from_bounds3d` |
| 14 | Depth range Top | `depth_range_top_from_bounds3d` |
| 15 | Depth range Left | `depth_range_left_from_bounds3d` |
| 16 | DepthRange min==max legal | `depth_range_accepts_point_interval` |
| 17 | DepthRange min>max rejected | `depth_range_rejects_inverted_interval` |
| 18 | DepthRange non-finite rejected | `depth_range_rejects_non_finite` |
| 19 | Segment accept | `segment_accepts_valid_endpoints` |
| 20 | Segment zero-length rejected | `segment_rejects_zero_length` |
| 21 | Non-finite coordinates rejected | `projected_geometry_rejects_non_finite_coordinates` |
| 22 | Circle radius validation | `circle_rejects_non_positive_radius` |
| 23 | Bounds2D validation | `projection_bounds_rejects_non_finite` |
| 24 | add_view installs frozen frame | `add_view_installs_canonical_frame` |
| 25 | add_view idempotent | `add_view_twice_is_noop` |
| 26 | add_primitive happy path | `add_primitive_records_geometry_role_depth_source` |
| 27 | duplicate id rejected | `add_primitive_rejects_duplicate_id` |
| 28 | primitive before view rejected | `add_primitive_requires_view` |
| 29 | unknown cut id rejected | `add_primitive_rejects_unknown_cut_id` |
| 30 | finish computes bounds | `finish_computes_view_bounds_from_primitives` |
| 31 | primitives without view | `finish_rejects_primitives_without_view` |
| 32 | empty model legal | `finish_accepts_empty_model` |
| 33 | post-deserialize integrity | `deserialized_model_passes_validate_integrity` |
| 34 | validate_against foreign cut | `validate_against_rejects_cut_missing_from_spatial_model` |
| 35 | reverse query | `primitives_for_element_returns_all_views` |
| 36 | opaque id provenance roundtrip | `spatial_cut_id_stays_opaque_through_projection` |
| 37 | 100-run determinism | `projection_determinism_100_runs` |

Standing error-contract tests (spatial-layer precedent, not counted):
`error_codes_are_distinct_sequential_and_complete`,
`every_variant_passes_contract_and_converts`,
`reason_enums_roundtrip_through_serde`.

---

## Freeze statement

This document freezes the scc-projection design. Implementation (Phase 9.3)
may not extend the public surface beyond §3/§7, may not add an Arc variant,
may not read `ProjectionMethod`, may not parse id strings, and may not
introduce visibility semantics. Deviations require an ADR-034 amendment
first.
