# ADR-028: Centerline & construction presentation

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022..027, `crates/scc-drawing/src/center_geometry.rs`

## Background

Phases 8.1–8.6 gave the drawing object geometry, construction classification,
annotations, dimensions, and dimension layouts. Phase 8.7 completes the
drawing's own presentation vocabulary with derived **center geometry**
(center marks + axial centerlines) and formalizes the construction
presentation policy. Three concepts stay distinct:

1. `ConstructionCircle` → engineering-backed `DrawingPrimitive` with role
   `Construction` (reference geometry that *is* the presentation).
2. Center marks / centerlines → drawing-only derived elements for circular
   object features.
3. Diameter-dimension 45° axes → dimension layout geometry, never centerlines.

## Decisions

### Decision 1 — Construction vs Center geometry are different things

Construction geometry comes from the engineering model through solved
geometry. Center geometry is invented by the drawing engine for presentation.

### Decision 2 — center elements never enter DrawingPrimitive

`DrawingCenterGeometry { CenterMark, Centerline }` lives in
`DrawingView.center_geometry`. No fabricated EntityId or FeatureId; nothing in
the primitive source map; no Drawing→Engineering write path.

### Decision 3 — strong-typed provenance

Every element carries `source_feature: FeatureId` plus an optional
`source_entity: Option<EntityId>` when derived from one specific circle
(always today). Provenance via the existing typed maps — no string parsing.
`SyntheticDrawingKind` remains unused/reserved; center elements are not forced
through the primitive source system.

### Decision 4 — sheet space and paper-mm constants

All coordinates are sheet space mm (ADR-024).
`DEFAULT_CENTER_MARK_HALF_LENGTH_MM = 3.0` (6 mm cross) and
`DEFAULT_CENTERLINE_OVERHANG_MM = 2.0` are paper mm: scale moves/scales the
circle but never these constants. Only witness radius/position transform.

### Decision 5 — ShowCenterlines default = true

Unspecified means visible: mechanical drawings conventionally show hole
centers. `false` suppresses all center geometry. The toggle changes nothing
else — primitives, dimensions, layouts, source maps, engineering/geometry
models are untouched.

### Decision 6 — toggles are independent

`ShowCenterlines` and `ShowConstructionGeometry` never control each other;
all four combinations produce deterministic results (tested).

### Decision 7 — generation policy

Object-role circles whose feature is a `Hole` or `HolePattern` get one center
mark (a proper 6 mm cross) plus horizontal and vertical axial centerlines
extending `radius + overhang` past the centre. Pattern elements each get their
own mark+lines while provenance stays on the single pattern feature. No extra
pattern-centre mark; no automatic pitch-circle construction circle (deferred).

### Decision 8 — ConstructionCircle gets no center marks

It is reference geometry already; adding centers would double-reference.
If a real PCD construction circle is wanted, it is modelled explicitly as a
`ConstructionCircle` feature.

### Decision 9 — stroke semantics without dash patterns

New `DrawingStrokeKind { Object, Construction, Center }` with a single
mapping `DrawingPrimitiveRole::stroke_kind()` (Construction/Reference →
Construction). Center elements are inherently `Center`. No SVG/DXF line-type
data enters the model.

### Decision 10 — diameter axis ≠ centerline

The Phase 8.6 `Diameter.axis_line` exists regardless of `ShowCenterlines`;
turning centerlines off leaves the diameter dimension and its layout fully
intact (tested).

### Decision 11 — envelope includes center geometry

`DrawingSheet::content_bounds()` unions geometry, dimension-layout extents,
**and** center-geometry extents under the single outer margin. With the
default 10 mm margin and 2 mm overhang, centerlines of normally-placed holes
stay inside the existing envelope; offset holes may grow it toward +X/+Y only
(no negative sheet coordinates, geometry never re-placed).

### Decision 12 — integrity validation

`validate_center_geometry_integrity`: finite coordinates; non-degenerate
segments; marks are proper crosses centred on one point; source feature exists,
is a Hole/HolePattern, and still has a represented circle in the view;
referenced entities belong to their feature. Run by `build_drawing`.

### Decision 13 — fail-closed shape checks

A hole/pattern feature whose represented primitive is not a Circle fails with
`SCC-DRW-015` — the builder never guesses a centre from a non-circle.

## Consequences

- New types/constants/errors (`SCC-DRW-015/016`); `DrawingView` gains
  `center_geometry`.
- `ShowCenterlines` is now honoured for the first time (stored since Phase 1).

## Rejected alternatives

Putting marks/lines into `DrawingPrimitive` (pollutes provenance); routing them
through the primitive source map with fake ids (unnecessary — they carry typed
sources); per-pattern single centre mark (loses per-hole manufacturing
semantics); auto-generated pitch-circle constructions (new reference policy —
deferred); honouring the toggle inside the layout engine (couples dimension
presentation to center visibility).

## Compatibility

Additive internally. Envelopes unchanged for existing fixtures (centerlines of
normally-placed holes stay within geometry+margin). Prior phases regression-
tested.

## Migration plan

None. Not wired into CLI/UI yet.

## Technical debt (added)

Automatic PCD construction circles; renderer dash/dot realization; refined
centerline policies per drawing standards (e.g. centre-line-only for large
bore features).
