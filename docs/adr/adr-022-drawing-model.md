# ADR-022: Derived, read-only DrawingModel

- Status: Accepted
- Date: 2026-08-23
- Related: ADR-015, ADR-016, ADR-017, ADR-021, `docs/specs/phase8-engineering-drawing-engine.md`, `crates/scc-drawing`

## Background

Phases 1–7 established `EngineeringCommand` → `EngineeringModel` →
`EngineeringLowering` → existing SCC pipeline → `GeometryModel`, plus providers
(DSL/tools) and the Workbench UI. Phase 8 begins turning a solved
`GeometryModel` into an engineering drawing. Phase 8.1 introduces the first
slice: a derived, read-only `DrawingModel` that references already-solved
geometry — deliberately **not** a second editable engineering state.

The Phase 8 readiness audit found the 2-D path **ready with blockers**: the
drawing layer is buildable, but strict Front/Top/Left three-views are **not**
possible because the Rust engineering model lacks thickness, extrusion
direction, hole depth/direction, through/blind, and 3-D orientation (see
`docs/specs/phase8-engineering-drawing-engine.md`).

## Decisions

### Decision 1 — DrawingModel is a pure, derived, read-only representation

`crates/scc-drawing::DrawingModel` is a presentation model computed on demand
from `(GeometryModel, EngineeringSourceMap, DrawingState, part metadata)`. It
is never the authoritative engineering truth and is not persisted as one.

### Decision 2 — EngineeringModel remains the only authoritative engineering state

`EngineeringModel` continues to be the single true design state (ADR-016).
`DrawingModel` carries **no** editable parameters, features, or constraints and
has no command/write surface. There is no `DrawingModel::set_parameter`,
`move_feature`, or `apply_command`.

### Decision 3 — GeometryModel is the sole geometric source for DrawingModel

`DrawingModel` derives every position from the already-solved
`scc_geometry::GeometryModel`. `DrawingBuilder` never re-solves constraints,
never recomputes pattern geometry, never rebuilds feature geometry, never
re-evaluates parameter expressions, and never re-runs engineering lowering.

### Decision 4 — DrawingBuilder does not own engineering state

`build_drawing` (`crates/scc-drawing/src/builder.rs`) consumes narrow,
read-only inputs: `&GeometryModel`, `&EngineeringSourceMap`, `&DrawingState`,
and optional `&PartDefinition`. It does **not** take the whole
`EngineeringModel`, keeping the drawing layer from gaining a wide access path
into engineering state. `DrawingState` stays the authoritative store of drawing
intent on the model.

### Decision 5 — DrawingModel has no EngineeringCommand / write surface

No command type, no mutation method, no `&mut self` on `DrawingModel` or any of
its parts. Rendering a drawing never changes the part. This mirrors ADR-016
Decision 1 (single authoritative engineering state) and ADR-021 Decision 6
(views are derived; no write path back).

### Decision 6 — Drawing source provenance is strong-typed, never string-parsed

`DrawingSourceMap` maps `DrawingPrimitiveId → EntityId`; composed with the
engineering source map's `entity_to_feature()` inverse it yields
`DrawingPrimitiveId → EntityId → FeatureId`. Provenance is **never** recovered
by parsing id strings (no prefix, no `contains`, no `split("_")`, no regex) —
same discipline as ADR-017 Decision 7 / ADR-021 Decision 7. Pattern-derived
primitives resolve back to the **single** `HolePattern` feature; the drawing
layer never expands a pattern into separate engineering `Hole` features.

### Decision 7 — Phase 8 provides a single principal 2-D view

`DrawingViewKind::Principal` is the only view kind. No Front/Top/Left view
kinds are added.

### Decision 8 — Three-view / Projection is deferred to Phase 9

Thickness, hole depth/direction, through/blind, hidden lines, and a
`ProjectionConsistencyValidator` are strictly Phase 9 concerns. Nothing in
Phase 8 guesses thickness/depth/orientation. `DrawingViewKind` remains a plain
additive enum so future kinds extend it without touching existing code.

### Decision 9 — Renderers are not part of DrawingModel

`DrawingModel` has no SVG/DXF/PDF knowledge. Renderers (`drawing_to_svg`,
`drawing_to_dxf`) are Phase 8.8/8.9 additions; Phase 8.1 tests validate the
model data directly. Existing `GeometryModel → SVG/DXF` exporters are
unchanged and unaffected.

### Decision 10 — Construction styling is deferred to Phase 8.7

Phase 8.1 preserves source provenance for construction geometry (so a future
pass can identify `Feature == ConstructionCircle` from a primitive), but does
not implement dashed rendering, show/hide toggles, or a construction flag on
geometry entities.

### Decision 11 — Deterministic default scale

If `DrawingProperty::Scale` is unspecified, the deterministic scale `1.0`
(one-to-one, model units are millimetres) is used. This default is defined here
in ADR-022 and never inferred from the UI. A scale that is not finite and
strictly positive fails closed (`SCC-DRW-004`).

## Consequences

- A solved 2-D `EngineeringModel` can be turned into a deterministic
  `DrawingModel` with correct provenance, with no rendering and no second
  state.
- New `ErrorStage::Drawing` (prefix `DRW`) and `SCC-DRW-*` codes are additive
  to the ADR-002 contract; the legacy proposal→geometry pipeline never emits
  them.
- `scc-drawing` depends only on `scc-geometry`, `scc-engineering`, and
  `scc-core`; `scc-engineering` never depends on `scc-drawing`.

## Rejected alternatives

- **Draw directly from `EngineeringModel` (fork: GeometryModel and DrawingModel
  as siblings).** A drawing is a projection of *solved* geometry; a sibling
  `DrawingModel` would re-derive positions independently or invent a second
  coordinate source — a second state. Chain below `GeometryModel` keeps one
  geometric truth.
- **Give `DrawingBuilder` the whole `EngineeringModel`.** Exposes too much;
  the narrow inputs suffice and keep ownership clear.
- **Reuse `scc_geometry::Point2` for drawing coordinates.** A dedicated
  `DrawingPoint` keeps the drawing model’s public coordinate vocabulary a
  stable contract, following the `scc_engineering::Point2D` precedent.

## Compatibility

Additive. `ErrorStage::Drawing` and `SCC-DRW-*` are new; existing documents,
stages, exporters, and tests are unaffected. No `scc-engineering` or
`scc-geometry` API changes are required (a `geometry.construction` flag is
explicitly deferred to Phase 8.7).

## Migration plan

None. Phase 8.1 adds `crates/scc-drawing` and its workspace entry; nothing is
wired into the CLI or UI yet (that is Phase 8.8+).

## Technical debt (unchanged, not fixed this round)

- Constraint SourceMap counter prediction (does not block Phase 8).
- ConstructionCircle export styling (Phase 8.7).
- Complex two-pass planner.
- Polygon solver limitation.
- Circular non-zero start-angle pattern representation.
