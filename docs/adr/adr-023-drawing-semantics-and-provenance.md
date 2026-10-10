# ADR-023: Drawing semantics and provenance hardening

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022, `crates/scc-drawing`, `docs/specs/phase8-engineering-drawing-engine.md`

## Background

Phase 8.1 (ADR-022) established `scc-drawing` as a derived, read-only
`DrawingModel` whose primitives mirror solved `GeometryModel` geometry with a
`DrawingPrimitiveId → EntityId` source map. Phase 8.2 upgrades each
`DrawingPrimitive` from a pure geometry mirror into a representation with
explicit engineering drawing semantics (a *role*) and hardened, fail-closed
provenance (an explicit *source*).

## Decisions

### Decision 1 — DrawingPrimitive carries a DrawingPrimitiveRole

Every `DrawingPrimitive` variant gains a `role: DrawingPrimitiveRole` field.
`DrawingPrimitiveRole = { Object, Construction, Reference, Synthetic }`:
- **Object** — real material part geometry (base profiles, holes, pattern
  holes).
- **Construction** — reference-only geometry (e.g. `ConstructionCircle`).
- **Reference** — reserved datum/reference geometry; no current feature
  produces it.
- **Synthetic** — drawing-only geometry created by the drawing engine itself
  (future: dimension extension lines, arrows, center marks, view/sheet
  borders). **Never** a fallback for missing provenance.

`Hidden`, `Centerline`, `Dimension`, `ExtensionLine`, and `Leader` are
deliberately **not** roles: those are *generated* by later Drawing Engine
stages, not classified from solved geometry.

### Decision 2 — the role is derived from strong-typed provenance, never shape or id

The builder classifies each solved entity by:
`EntityId → (EngineeringSourceMap.entity_to_feature) → FeatureId →
(EngineeringModel.features) → EngineeringFeature → role`.

`ConstructionCircle` → `Construction`; `BaseProfile`/`Hole`/`HolePattern` →
`Object`. Classification is based on the feature kind, never on id strings and
never on geometry shape (an identical circle is `Construction` for a
`ConstructionCircle` and `Object` for a `Hole`).

### Decision 3 — GeometryPrimitive and DrawingPrimitive do not re-solve anything

`DrawingPrimitive` remains a lossless presentation mirror of solved
`GeometryPrimitive`. The builder never re-solves constraints, never recomputes
patterns, never rebuilds feature geometry, and never re-evaluates parameters.

### Decision 4 — DrawingSource is formal: Engineering vs Synthetic

`DrawingSource::Engineering { feature, entity }` is the **only** source of a
geometry-backed primitive. `DrawingSource::Synthetic { kind }` is reserved for
future drawing-engine-created primitives and is **never** produced by the
geometry→drawing builder.

### Decision 5 — missing provenance fails closed, Synthetic is not a fallback

A solved geometry entity with no `EntityId → FeatureId` mapping is a
fail-closed `SCC-DRW-003 MissingSourceMapping`. The builder never guesses a
feature and never classifies an unmapped entity as synthetic. (Phase 8.1's
`drawing_sources` previously fell back to `Synthetic` for unmapped entities;
that fallback is removed.)

### Decision 6 — pattern primitives keep mapping to a single feature

Every solved pattern entity (e.g. the 8 circles of a circular `HolePattern`)
resolves via the source chain to the **one** `HolePattern` feature; the drawing
layer never expands a pattern into separate engineering `Hole` features.
`primitives_for_feature` returns the stable `BTreeSet` of all drawing
primitives of a feature (8 for a circular 8-hole pattern).

### Decision 7 — source queries are strong-typed, never string-parsed

`DrawingSourceMap` provides, by derivation:
- `entity_for_primitive(id) -> Option<&EntityId>`;
- `primitives_for_entity(entity) -> BTreeSet<DrawingPrimitiveId>`;
- `feature_for_primitive(id, eng_map) -> Result<Option<FeatureId>>` (fail-closed
  on a missing entity→feature mapping);
- `primitives_for_feature(feature, eng_map) -> BTreeSet<DrawingPrimitiveId>`.

No cached `PrimitiveId → FeatureId` state is kept — the reliable chain
`Primitive → Entity → Feature` is derived on demand and deterministic.

### Decision 8 — source integrity is validated

`validate_drawing_source_integrity(drawing, geometry, engineering_source_map,
features)` verifies: every model primitive has a source-map entry; every map
entry refers to an existing primitive; the entity exists in the solved
geometry; the entity maps to a feature; and the feature exists in the
engineering model. Violations fail closed (`SCC-DRW-006`); a missing
entity→feature mapping is `SCC-DRW-003`. Duplicate/conflicting
primitive→entity mappings are structurally prevented (a `BTreeMap` key).

### Decision 9 — renderers do not participate in semantic classification

The role is a domain enum on the primitive. Renderer-specific styling
(SVG `stroke-dasharray`, DXF `LTYPE`, colors) is **not** part of
`DrawingModel`; a future line-style resolver maps
`DrawingPrimitiveRole → renderer style`.

### Decision 10 — ShowConstructionGeometry is a presentation decision

`DrawingBuilder` applies `DrawingState.ShowConstructionGeometry`:
- unspecified → construction geometry is **visible** (reference geometry is
  shown by default; deterministic default defined here, never inferred from
  the UI);
- `false` → `Construction`-role primitives are omitted from the drawing.

`DrawingModel` is therefore the **final presentation** (not a togglable
catch-all). Omitting construction never erases or corrupts engineering
provenance: the source chain is resolved before filtering, and re-running with
visibility on restores the primitives deterministically.

`ShowCenterlines` is **not** honored in Phase 8.2: there is no centerline
engine yet, and `true` must not fabricate centerlines (Phase 8.7).

### Decision 11 — ProjectionMethod is retained intent only

`ProjectionMethod` remains stored on the model but is unused by the single
principal view. No `Front`/`Top`/`Left` view kinds are added (Phase 9).

### Decision 12 — empty presentation is allowed, empty geometry is not

A drawing whose every primitive is filtered (e.g. a construction-only part
with `ShowConstructionGeometry=false`) produces a legitimately empty view with
degenerate `DrawingBounds::zero()`. A `GeometryModel` with **no solved
entities** is a `SCC-DRW-005 EmptyDrawing` error. The two cases are distinct:
one is a presentation decision, the other is "no geometry".

## Consequences

- `DrawingPrimitive` carries a role; `DrawingSource` distinguishes
  engineering from synthetic; missing provenance fails closed.
- New `SCC-DRW-006 InvalidSourceIntegrity`; `SCC-DRW-003` semantics clarified.
- `build_drawing` now requires the engineering `features` map (narrow, read-only
  — not the whole model) to classify roles and verify feature existence.
- `DrawingSourceMap::drawing_sources` now returns `Result` and never fabricates
  `Synthetic`.

## Rejected alternatives

- **Classify role from geometry shape.** An identical circle is a hole or a
  construction circle depending on intent; shape cannot tell them apart.
- **`Synthetic` as a missing-mapping fallback.** Would hide real provenance
  failures and silently produce "drawings that just look close".
- **Keep `ShowConstructionGeometry` unfiltered in the model and let the
  renderer decide.** `DrawingModel` is the presentation representation; leaving
  the toggle to a future renderer would make the model not-final and invite
  per-renderer divergence.

## Compatibility

Additive. `DrawingPrimitiveRole` and the `role` field are new; existing
`DrawingPrimitive` matching with `..` still compiles. `SCC-DRW-006` is new;
`SCC-DRW-003` carries the same code but a clarified meaning. Existing Phase 8.1
tests are updated where they assumed the old `Synthetic` fallback behavior.

## Migration plan

None. Phase 8.2 changes `crates/scc-drawing` internally; nothing is wired into
the CLI or UI yet.

## Technical debt (unchanged, not fixed this round)

- Constraint SourceMap counter prediction.
- ConstructionCircle export styling (Phase 8.7).
- Complex two-pass planner.
- Polygon solver limitation.
- Circular non-zero start-angle pattern representation.
