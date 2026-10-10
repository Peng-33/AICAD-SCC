# ADR-033: 2.5-D spatial lowering

- Status: Accepted
- Date: 2026-09-02
- Related: ADR-032 (engineering spatial semantics), ADR-016 (engineering
  model as single source of truth), ADR-017 (typed provenance source map),
  ADR-004 (GeometryModel boundary), ADR-002 (structured error model),
  `crates/scc-spatial/`, `docs/error-codes.md` (`SCC-SPAT-001` … `SCC-SPAT-016`)

## Background

Phase 9.1 (ADR-032) gave the authoritative `EngineeringModel` a complete
spatial vocabulary — part thickness, per-hole extents (`ThroughAll` |
`Blind { start_face, depth }`), and a fail-closed readiness check — but the
result is still pure intent: no coordinates, no solved shape. The solved XY
shape lives in the derived `GeometryModel` produced by `lower_and_solve`, and
the feature → entity provenance lives in the `EngineeringSourceMap`.

Phase 9.2 joins the three into the first deterministic **2.5-D spatial
representation**: a derived, read-only `SpatialModel` (base body + cylindrical
cuts + bounds + typed provenance). It is the input contract a future
projection engine (Phase 9.3+) will need; it is not a B-Rep, not a mesh, and
not a boolean kernel.

## Crate boundary

New leaf crate `crates/scc-spatial`, depending on `scc-core`,
`scc-engineering`, and `scc-geometry` — never the reverse. The engineering
domain must not know about its derived spatial representation: `scc-engineering`
gained **zero** semantic changes for Phase 9.2 (only the pre-existing Phase 9.1
read/resolver APIs are consumed). `scc-drawing` is untouched; the spatial layer
consumes `GeometryPrimitive`/`GeometryEntity` directly and never sees
`DrawingPrimitive`. Placing the lowering in `scc-engineering`
(`spatial_lowering.rs`) was rejected because a derived artifact inside the
authoritative crate invites accidental reverse dependencies and blurs the
truth/derivation line the repository has maintained since ADR-016.

## Input contract

`lower_spatial(SpatialInputs)` takes exactly three borrowed, read-only
sources: the `EngineeringModel` (intent + spatial semantics), the solved
`GeometryModel` (all XY positions/radii), and the `EngineeringSourceMap`
(feature ↔ entity provenance). Neither input alone suffices: XY coordinates
must come from the solved geometry (never recomputed), while thickness,
extents, and hole semantics exist only in the engineering model.

## Decisions

1. **`SpatialModel` is derived and read-only.** It exposes query helpers
   (`cut_for_entity`, `cuts_for_feature`) and no mutators, no command
   application, and no re-solve entry points. The only way to change it is to
   change the engineering model and re-run the pipeline.
2. **Fail-closed gates in fixed order.** `lower_spatial` runs (1) the
   single-owner Phase 9.1 readiness gate — re-implementing readiness checks
   (thickness presence, extent presence, blind-vs-thickness) here is
   forbidden; (2) thickness resolution via the single-sourced Phase 9.1 rule;
   (3) base-body construction; (4) per-feature cut construction; (5) claim
   integrity; (6) bounds. Readiness failure returns
   `EngineeringNotSpatiallyReady` carrying the `SpatialIssue` list verbatim.
3. **`EngineeringModel` remains the sole engineering truth; `GeometryModel`
   remains the sole solved XY source.** The spatial model stores resolved
   numerical snapshots (e.g. `z = [0, 10]`) plus provenance
   (`FeatureId`/`EntityId`) — never parameter objects, never an expression
   graph, never a dependency graph.
4. **Base body = solved footprint × `[0, T]`.** Exactly one `BaseProfile`
   solved into exactly one `Rectangle` (two entities ⇒ `NotUnique`; zero ⇒
   `NoSolvedEntity`). The footprint is taken verbatim from the solved
   rectangle — `BaseProfile.width/height` are never re-solved here. Only the
   rectangle footprint is spatializable in Phase 9.2; a polygon base fails
   closed (`NotRectangle` / unsupported-feature gate).
5. **Array-template provenance is expected, claim-free.** The solver removes
   the pattern template entity during array expansion, so the source map holds
   `{feature}` (template, absent from geometry) plus `{feature}_array[i]`
   instances. Cardinality counts and per-entity iteration use
   `solved_entities` — the deterministic intersection of source-map entries
   with geometry keys. Rule 2 of the §4-A amendment: a single hole whose only
   mapped entity is absent from geometry is `NotInGeometry` (fail-closed),
   not silently skipped.
6. **Z intervals on the canonical Part-Local +Z axis (ADR-032).** The axis is
   implicit and never stored. `ZInterval` must be finite, `min ≥ 0`,
   `max > min`. Body = `[0, T]` (never `[-T/2, +T/2]`). `ThroughAll` = the
   material interval `[0, T]` (no infinite cylinder is kept — the engineering
   intent stays in the engineering model, the spatial model stores the
   resolved physical interval). Blind from base = `[0, D]`; blind from
   opposite = `[T−D, T]`. A blind depth that reaches or exceeds `T` is
   defensively rejected (`InvalidBlindDepthDefensive`) — never converted to
   `ThroughAll`.
7. **Cylindrical cuts are analytic, witnessed, never booleaned.** One solved
   `Circle` (center + radius from geometry, verbatim) plus the resolved Z
   interval becomes one `CylindricalCut`. There is no mesh, no triangle fan,
   no silhouette extraction, no CSG — projection derives views analytically
   later. The diameter witness (`|engineering_diameter − 2·radius|` ≤
   `SCC_SPAT_LENGTH_TOLERANCE` = 1 nm, inclusive) is an integrity check, not
   a re-measurement: engineering diameter stays the semantic truth. A
   mismatch fails closed (`SpatialWitnessMismatch`). The readiness gate
   checks extents and thickness but not diameters, so an unresolved or
   unresolvable bound diameter reaches its own defensive gate
   (`InvalidHoleDiameterDefensive`, `SCC-SPAT-016`) — never a panic, never
   silently re-solved.
8. **Patterns and construction circles.** One `HolePattern` engineering
   feature lowers to N cuts sharing one `source_feature` and keeping distinct
   `source_entity`s; positions are never regenerated from PCD/rows/columns/
   corner offsets — they are read from the solved circles. Cardinality is
   cross-checked (`count`, `rows×columns`, canonical 4 corners). Solved
   `ConstructionCircle`s are reference-only: validated as finite positive
   circles, provenance-cross-checked like every cut, and recorded in
   `construction_circles` (policy B, provenance integrity), producing no
   body, cut, or bounds expansion. Cuts never expand
   outer bounds either — bounds derive from the body footprint only. Hole
   centers outside the footprint or overlapping cuts are NOT policed in
   Phase 9.2 (no new manufacturability policy; recorded as future spatial
   validation debt).
9. **Deterministic IDs and collections.** Cut IDs are `SpatialCutId`, the
   deterministic allocation `{feature_id}:cut:{index:04}` — opaque, never
   parsed for meaning, never random. Every authoritative collection is a
   `BTreeMap`/`BTreeSet`; ordering is identity order only (no depth/visibility
   ordering — that is view-dependent and belongs to projection). 100 runs on
   identical inputs produce byte-identical serialization (locked by test).
10. **`SpatialSourceMap` is folded into the primitives.** Every
    `SpatialBody`/`CylindricalCut` carries `source_feature: FeatureId` and
    `source_entity: EntityId` directly (single copy, no parallel map to drift);
    reverse queries are model helpers, not stored indexes. The
    `EngineeringSourceMap` is never mutated. Missing provenance is
    `MissingSpatialSource`; an entity whose inverse provenance resolves to a
    foreign feature is `InvalidSpatialSource{ForeignFeature}`; a solved
    geometry entity claimed by no spatial element is
    `UnmappedGeometryEntity` — all fail-closed, no synthetic fallback.
11. **Own error stage `SCC-SPAT`.** `SpatialError` → `StructuredError` under
    the new `ErrorStage::Spatial` with codes `SCC-SPAT-001` … `SCC-SPAT-016`,
    all `recoverable: false`. Engineering state can be perfectly valid while
    spatial lowering integrity fails, so reusing `SCC-ENG` (or `SCC-DRW`)
    would misattribute the failing layer. Every variant is contract-tested.
12. **Serialization is transport/testing-only.** `SpatialModel` derives
    `Serialize`/`Deserialize`/`PartialEq` for tests, debugging, and future
    cross-crate transport. Loading a serialized spatial model never re-enters
    engineering editing; it is not persisted engineering state.
13. **No second readiness.** A successfully constructed `SpatialModel` is
    projection-input-ready by construction; Phase 9.3 adds no second gate.

## Explicit non-goals (Phase 9.2)

1. No B-Rep, face/edge topology, mesh, triangles, or boolean/CSG evaluation.
2. No STEP/STL/3MF export.
3. No arbitrary workplanes, 3-D sketches, or arbitrary feature transforms.
4. No projection, views, hidden lines, visibility, or camera state.
5. No vector algebra package (`Vector3`, `Matrix4`, quaternion, ray, plane).
6. No Python/CadQuery involvement — Rust-native deterministic lowering only;
   the CadQuery worker remains a separate legacy/experimental subsystem.
7. No CLI surface and no UI work (no 3-D preview, no three-views panel, no
   spatial JSON panel).
8. No `scc-drawing` changes; the Phase 8 pipeline
   (engineering → geometry → drawing → SVG/DXF) stays byte-identical.
9. No over-deep-blind → through-hole conversion, no clamping/moving/deleting
   of out-of-bounds cuts, no overlap analysis.
10. No caching of resolved state anywhere: every `lower_spatial` call
    re-resolves thickness and depths from the current engineering model.

## Consequences

- Downstream consumers (mass properties, manufacturing, Phase 9.3 projection)
  can be built on a stable, deterministic 2.5-D contract without any boolean
  kernel.
- The readiness gate plus lowering-specific integrity checks give two clearly
  separated failure layers: engineering validity (`SCC-ENG`) and derivation
  integrity (`SCC-SPAT`).
- Polygon bases, slot/fillet features, and new geometry primitives fail
  closed until a later phase gives them explicit spatial semantics.
- Undo/redo and re-solving need no spatial awareness: derived models are
  rebuilt from scratch whenever the engineering model changes.

## Not doing (deferred)

- Phase 9.3 owns the orthographic projection model. Nothing in
  `scc-spatial` may grow view, visibility, or ordering semantics before then.
