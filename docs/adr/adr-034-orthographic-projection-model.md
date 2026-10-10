# ADR-034: Orthographic projection model

- Status: Accepted
- Date: 2026-09-02
- Related: ADR-032 (engineering spatial semantics, canonical part-local
  frame), ADR-033 (2.5-D spatial lowering), ADR-016 (engineering model as
  single source of truth), ADR-024 (drawing view / sheet space),
  `crates/scc-projection/`, `docs/error-codes.md` (`SCC-PROJ-001` …
  `SCC-PROJ-006`)

## Background

Phase 9.2 (ADR-033) produced the first deterministic 2.5-D representation:
a derived, read-only `SpatialModel` — the plate solid (solved rectangular
footprint × `[0, T]` on the canonical part-local +Z axis) plus witnessed
cylindrical cuts and body-derived bounds. A spatial model is
projection-input-ready by construction (ADR-033 Decision 13): a
successfully constructed one needs no second readiness gate.

Phase 9.3 adds the orthographic projection layer: a new leaf crate
`scc-projection` that derives three canonical principal views (Front, Top,
Left) over that spatial model — frozen named-axis frames, projected
`Segment`/`Circle` primitives with embedded depth evidence, stored-validated
view bounds, and a provenance map back to the spatial elements. Phase 9.3
delivers **types + math + validation only**: the lowering function that
fills the views is Phase 9.4, hidden-line/visibility is 9.5, sheet
integration is 9.7.

## Crate boundary

New leaf crate `crates/scc-projection`, depending on exactly `scc-core`,
`scc-spatial`, `serde` (derive), and `thiserror = "1"` (dev-dep
`serde_json`) — never the reverse. `scc-spatial` and `scc-engineering`
gained **zero** Phase 9.3 changes. No `scc-drawing` dependency exists
(integration is 9.7), and no engineering type (`FeatureId`, `EntityId`,
`ProjectionMethod`) is named anywhere in the crate: `SpatialElementRef`
carries `SpatialCutId` opaquely plus a `Body` variant, and the provenance
chain `ProjectionPrimitiveId → SpatialElementRef → CylindricalCut →
FeatureId/EntityId` completes by looking the cut up in the `SpatialModel`.
Copying engineering ids would duplicate provenance truth — the exact
double-bookkeeping ADR-033 Decision 10 eliminated one level down.

## Decisions

1. **`OrthographicFrame` is three named axes — no matrices, no camera.**
   The frame stores `right`, `up`, and `toward_viewer` as `AxisDirection`
   enums; projection is a signed-coordinate read per named axis in one
   generic code path. No `Matrix3`/`Matrix4`, no quaternion, no vector
   package, no perspective, no camera state (extends ADR-033 non-goal 5).
   Construction is total; orthonormality validation reduces to exact
   integer checks (`cross(right, up) == Some(toward_viewer)` plus pairwise
   distinctness).
2. **The frozen frame table is normative.** Front (`PosX, PosY, PosZ`;
   `u = +x`, `v = +y`, `d = +z`), Top (`PosX, NegZ, PosY`; `u = +x`,
   `v = −z`, `d = +y`), Left (`PosZ, PosY, NegX`; `u = +z`, `v = +y`,
   `d = −x`), each satisfying `right × up = toward_viewer` (right-handed).
   Depth convention: larger depth = nearer viewer. The Front viewer at +Z
   sees the Base face (z = 0) farthest; hole near/far evidence reads
   directly off `d`. Top paper-up is −Z (material depth runs down the
   paper); the Left viewer at −X looks along +X, so depth = −x. The three
   alignment invariants (`Front.u == Top.u`, `Front.v == Left.v`,
   `Left.u == −Top.v`) are locked by tests.
3. **`PrincipalViewKind { Front, Top, Left }` with contractual Ord.** The
   discriminant declaration order IS the canonical order `Front < Top <
   Left`, locked by doc + test — never left to alphabetical accident
   (alphabetical F < L < T would order Left first). The BTreeMap keyed by
   it therefore serializes views in canonical order.
4. **mm, 1:1, always.** No `DrawingScale`, no sheet space, no margins, no
   view transforms: 9.3 output is pure (u, v, depth) view space. Sheet
   placement, scale semantics, and paper space stay with `scc-drawing`
   (ADR-024; integration 9.7). A projected primitive carries no scale
   field.
5. **First-angle vs third-angle is sheet placement, not geometry.**
   `ProjectionMethod` (FirstAngle | ThirdAngle) in `scc-engineering` is
   stored intent with zero geometry consumers; it affects only where views
   sit on the sheet (9.7), never the per-view (u, v, depth) content. Phase
   9.3 does not read it, and nothing in 9.3 or 9.4 branches on it.
6. **`AxisDirection` and `PartLocalPoint3` live in scc-projection.** They
   are minimal (no arithmetic API) and are deliberately not promoted to
   `scc-geometry` — nothing else needs them (no `Point3`/`Vec3` exists
   anywhere in the workspace).
7. **New `ProjectionDepthRange` instead of reusing `ZInterval`/`Bounds3D`.**
   Depth ranges need `min == max` legal (a planar face seen edge-on) and
   negatives legal (Left view depth = −x); both `ZInterval` (rejects
   `min == max` and `min < 0`) and `Bounds3D` reject degenerate intervals.
   This is a deliberate, documented non-reuse. The range is embedded on
   each `ProjectionPrimitive` (`{geometry, role, depth}`) — sufficient for
   9.5 hidden-line decisions; anything finer re-queries the originating
   `SpatialModel` analytically via the provenance chain. The spatial model
   IS the depth oracle; no per-endpoint depths, face ids, or normals are
   stored.
8. **Projected geometry = Segment + Circle only.** The Phase 9.2 spatial
   domain (rectangular footprint + +Z-parallel cylindrical cuts) projects
   to segments (footprint outlines, cut silhouettes) and circles (a cut
   seen end-on in Top). `Arc` is a frozen purely-additive serde extension
   path, omitted now (YAGNI — arcs first appear with non-axis-aligned
   cylinders, fillets, or arc-edge profiles, none spatializable in
   ADR-033). A projected rectangle is 4 segments; no `Rectangle` variant.
9. **Roles: `BodyBoundary` | `CutBoundary` only.** No Hidden/Visible/
   Construction role exists — visibility is 9.5, and construction circles
   project to nothing (reference-only, ADR-033). Coincident primitives
   (same geometry, different source or depth) are preserved: no dedup, no
   z-ordering, no intersection or silhouette math in 9.3.
10. **`ProjectionPrimitiveId` format `{view}:{source}:p:{index:04}`** with
    view = `front`|`top`|`left` and source = `body` or the `SpatialCutId`
    Display verbatim. Deterministic from (view, opaque source, per-(view,
    source) occurrence index in builder insertion order — gapless, advanced
    only after a successful insert); String-lexical Ord; Display; serde
    plain string. Never random, never coordinate-derived, never parsed for
    meaning.
11. **Provenance = separate `ProjectionSourceMap` stored inside the model**
    (the `DrawingSourceMap` pattern, not per-primitive embedding): one
    spatial element legitimately produces many primitives per view (a cut →
    up to 4 segments per front-like view, 1 circle in Top), so embedding
    would duplicate one `SpatialElementRef` N times. Stored inside
    `ProjectionModel.sources` so a serialized model is self-contained. The
    reverse index (`element_index`) is derived on demand — never stored, no
    second index to drift. `validate_integrity` checks the map covers
    exactly the existing primitives (no orphan provenance, no gaps).
12. **`add_view` is idempotent** (re-adding a kind is a no-op success):
    frames are created internally from the frozen table — callers never
    supply custom axes — so a duplicate view is not an error state and no
    `DuplicateProjectionView` code exists.
13. **`add_primitive` fail-closed gates in fixed order:** view exists →
    geometry (non-finite coordinate, zero-length segment with exact `==`
    and no tolerance policy, non-positive radius) → depth range →
    duplicate id → source integrity. The FIRST failing gate wins, locked by
    tests. Non-finite anything is caught by `is_finite()` (never by
    relying on NaN `PartialEq` traps). `Body` is always a valid source
    (a `SpatialModel` always has a body); a `Cut` id absent from
    `spatial.cuts` is `InvalidProjectionSource` — insert-time and
    re-checkable, never a synthetic fallback.
14. **Stored, validated `ProjectionBounds2D` per view** (the
    `Bounds3D`/`DrawingBounds` layer convention): recomputed by the builder
    at `finish()` from primitive geometries — segment endpoints, and
    circle center ± radius on both axes — then validated (finite,
    non-inverted; degenerate legal). A derived helper exists for checks;
    the stored value is the contract. An added view with zero primitives
    stores the degenerate `0..0` envelope so an empty view still carries
    valid bounds.
15. **Empty model is legal.** `finish()` with zero views and zero
    primitives succeeds (projection is a pure derived artifact; emptiness
    must stay representable — a real `SpatialModel` always has a body, so
    an empty projection can only be deliberate). `ProjectionModel` has no
    `Default` and no mutators; it is built once by
    `ProjectionModelBuilder::finish()` and afterwards only queried.
16. **Two validation gates, two jobs.** `validate_integrity()` checks
    everything decodable without the spatial model (provenance coverage,
    geometry validity, depth validity, canonical frames, bounds) — the
    post-deserialize gate. `validate_against(&SpatialModel)` re-checks
    every cut reference against `spatial.cuts` — the post-build and 9.4
    final gate. Deserialization does NOT auto-validate (Phase 9.2 debt
    pattern, retained deliberately); callers run the gates explicitly.
17. **Own error stage `SCC-PROJ`.** `ErrorStage::Projection` additive
    variant (prefix `PROJ`, Display `projection`) + 6 codes
    `SCC-PROJ-001`…`006` (missing view, invalid geometry, invalid depth
    range, invalid bounds, invalid source, duplicate primitive) +
    `From<ProjectionError> for StructuredError`, all fail-closed
    (`recoverable: false`). Every variant is contract-tested
    (`all_variants` + `validate_contract`, spatial-layer precedent).
    `docs/error-codes.md` was updated before the code landed; the additive
    stage has no compatibility impact (ADR-002).
18. **Determinism locked like ADR-033 Decision 9.** `BTreeMap` for views,
    primitives, and the source map; fixed `PrincipalViewKind` Ord;
    deterministic gapless ids; no randomness anywhere. 100 runs on
    identical inputs produce byte-identical pretty JSON, including through
    a serde roundtrip (locked by `projection_determinism_100_runs`).
    Serde conventions copied from scc-spatial: internally tagged
    `kind`/`element`, snake_case, `Serialize + Deserialize + PartialEq`
    for tests/debug/transport only — no schema system, no persistence
    contract.
19. **Test matrix mapped from the brief.** 37 numbered requirements
    concrete-named and placed (unit in-module where scc-spatial does the
    same; builder/provenance/determinism in
    `tests/projection_model.rs`), plus the standing error-contract tests
    (`error_codes_are_distinct_sequential_and_complete`,
    `every_variant_passes_contract_and_converts`,
    `reason_enums_roundtrip_through_serde`) and Phase-9.2-brief 64/65
    analogues (provenance chain consistency, coincident-primitive
    preservation).

## Explicit non-goals (Phase 9.3)

1. No `lower_projection()` / `project_principal_views()` — 9.4's builder
   owns which primitives exist; 9.3 provides the types, math, and
   validation it will call.
2. No visibility, hidden/occluded lines, dashed strokes, z-ordering,
   coincidence dedup, silhouette extraction, or intersection math (9.5).
3. No sheet layout, `DrawingScale`, margins, sheet/view/space transforms,
   or `ProjectionMethod` reads; no `scc-drawing` dependency (9.7).
4. No matrices, camera, perspective, or vector package (Decision 1).
5. No persistence schema — serde is tests/debug/transport only.
6. No exported tolerance constants (9.5's coincidence policy is its own;
   `SPATIAL_LENGTH_TOLERANCE` stays `pub(crate)` in scc-spatial).
7. No `Arc`/`Rectangle` geometry variants, no polygon handling, no
   non-+Z-parallel cylinders.
8. No CLI surface and no UI work.
9. No re-validation of engineering state, no re-solving, no second
   readiness gate (ADR-033 Decision 13).
10. No `HashMap` anywhere (BTreeMap only), no `unwrap`/`expect`/`panic` in
    production code, no string parsing of ids.

## Consequences

- Phase 9.4 gets a complete, tested consumption contract: `add_view`,
  `next_id`, `add_primitive` (with the spatial model for insert-time
  integrity), `finish`, then `validate_against` — nothing else exists or
  is needed.
- The projection layer inherits the repository's fail-closed discipline:
  every rejection is a stable `SCC-PROJ-*` code, and no model is ever
  partially assembled.
- Depth evidence is closed-interval per primitive; 9.5 can order and
  overlap-test primitives without new 9.3 fields, and can always recover
  exact analytic geometry through the provenance chain.
- Canonical `Ord` on `PrincipalViewKind` plus BTreeMaps make serialized
  models byte-stable, enabling golden tests and diff-friendly transport.
- New geometry capabilities (arcs, fillets, non-axis-aligned cylinders)
  require extending both ADR-033's spatial domain and this crate's
  `ProjectedGeometry` — the additive-serde-variant discipline keeps that
  path open without breaking stored payloads.

## Rejected alternatives

- **Float-matrix frames (column/row vectors, rotation matrices).** Exact
  integer checks would become float equality; the frozen frames are three
  named axes, so matrices add nothing but instability.
- **Per-primitive provenance embedding** (ADR-033 Decision 10 pattern,
  one level up): one element → many primitives would copy one
  `SpatialElementRef` N times; the source map stores it once and is
  self-contained inside the model.
- **Reusing `ZInterval` for depth ranges:** it rejects `min == max`
  (edge-on faces are common) and `min < 0` (Left depth = −x); widening it
  would loosen a spatial invariant that is correct for Z intervals.
- **A `DuplicateProjectionView` error code:** idempotent `add_view` makes
  the state unrepresentable; an error would punish a defined no-op.
- **An `InvalidProjectionFrame` error code:** the API exposes only the
  three total canonical constructors, so no user-reachable frame failure
  exists; the defensive post-deserialize `OrthographicFrame::validate`
  maps its unreachable failure to `InvalidProjectionGeometry`.
- **Per-endpoint depth structures / face ids / normals on primitives:**
  the spatial model already holds exact analytic data; duplicating it in
  projection would create a second depth oracle to keep in sync.
- **Storing the reverse provenance index:** a stored index drifts; the
  derived-on-demand `element_index` over a small BTreeMap is cheap and
  always correct.
- **Visibility (`Visible`/`Hidden`) or z-order fields now:** 9.5's
  policy; adding fields 9.5 cannot justify yet would freeze the wrong
  shape (the 9.2 debt pattern is retained deliberately for deserialization
  only, not for speculative surface).

## Compatibility

- `ErrorStage::Projection` is an additive enum variant with prefix `PROJ`
  and Display `projection`, added together so
  `StructuredError::validate_contract` (code segment == stage prefix)
  holds; existing stages, documents, and exhaustive stage consumers are
  unaffected (no workspace code matches on `ErrorStage` exhaustively
  outside `scc-core`). No compatibility impact (ADR-002).
- The new crate adds no dependencies to `scc-spatial`, `scc-engineering`,
  or `scc-drawing`; the Phase 8 pipeline (engineering → geometry →
  drawing → SVG/DXF) stays byte-identical.
- Serde payloads of `scc-projection` types are tests/debug/transport only;
  the frozen extension paths (`Arc` geometry variant, future roles) are
  purely additive serde variants.

## Migration plan

None required — new leaf crate, additive core enum variant, additive
registry section. Consumers adopt `scc-projection` (Phase 9.4) by
depending on it and calling the builder contract of §7 of the frozen
design; nothing existing changes behavior.

## Technical debt

- Deserialization does not auto-validate (retained Phase 9.2 pattern,
  documented on the types; callers must run `validate_integrity()` /
  `validate_against()` explicitly).
- `docs/adr/README.md` is missing ADR-030/031/032 rows — pre-existing
  known debt left untouched; the ADR-034 row is added with this ADR.
- `PrincipalViewKind::id_spelling` and `ProjectionBounds2D::validate` /
  `ProjectedGeometry::bound_points` are `pub(crate)`; they become crate
  internals for 9.4's lowering and may be promoted if a consumer
  legitimately needs them (no surface change before then).
