# ADR-027: Deterministic dimension layout

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022..026, `crates/scc-drawing/src/dimension_layout.rs`

## Background

Phase 8.5 established `DrawingDimension` — what is measured and where the
number comes from. Phase 8.6 adds `DrawingDimensionLayout`: where each
dimension is drawn on the sheet, deterministically. Formatting (`"Ø64"`),
fonts, and rendering remain future phases.

## Decisions

### Decision 1 — semantic and layout are separate types

`DrawingDimension` stays purely semantic. New
`DrawingDimensionLayout { dimension: DrawingDimensionId, geometry:
DimensionLayoutGeometry }` holds presentation geometry only and lives in
`DrawingView.dimension_layouts`, keyed 1:1 by dimension id. Layouts never
duplicate values, sources, or targets.

### Decision 2 — all layout coordinates are sheet space

Millimetres on paper, `+X` right, `+Y` up, origin bottom-left (ADR-024). SVG
Y-down conversion is renderer work.

### Decision 3 — value chain ≠ placement chain, cross-checked

Value: engineering model → dimension source → resolved cache. Placement:
`target FeatureId → DrawingSourceMap → primitives → DrawingViewTransform →
sheet-space witness`. Before layout, the two are compared with a fixed
epsilon; a mismatch fails closed with `SCC-DRW-014 InvalidDimensionWitness`.
This is a consistency gate, not re-measuring geometry as truth.

### Decision 4 — witness epsilon

`WITNESS_EPSILON_MM = 1.0e-6` model mm. The solver's own epsilon is tighter;
the looser drawing bound tolerates float noise across lowering→solve→draw.
(No single reusable cross-layer constant existed.)

### Decision 5 — paper-mm constants never scale

```text
DEFAULT_DIMENSION_OFFSET_MM        = 8.0
DEFAULT_DIMENSION_STACK_PITCH_MM   = 7.0
DEFAULT_EXTENSION_OVERHANG_MM      = 2.0
DEFAULT_EXTENSION_GAP_MM           = 1.0
DEFAULT_ARROW_SIZE_MM              = 2.5  (renderer hint only)
DEFAULT_DIMENSION_TEXT_GAP_MM      = 1.5
DIAMETER_AXIS_ANGLE_DEG            = 45.0 (fixed through-circle axis)
```

`DrawingScale` scales only witness spans (120 mm reads as a 60 mm line at
1:2); offsets/pitch/gaps/arrows stay constant on the paper.

### Decision 6 — fixed MVP placement sides

Linear-horizontal → **above** the object; linear-vertical → **right** of the
object; diameter → fixed 45° axis through the circle centre; patterns →
upper-right callout outside the full footprint. No "pick the emptiest side"
intelligence — that is later optimization. These sides keep the bottom/left
margins untouched so the Phase 8.3 principal-view translation is frozen and
PartTitle stays at `(margin, margin/2)`.

### Decision 7 — deterministic stacking

Same-side dimensions stack in `BTreeMap` id order (semantic
`{feature}.{slot}` ids), each successive line one `STACK_PITCH` further out.
No HashMap iteration, no instability.

### Decision 8 — arrows are anchors, not polygons

`ArrowAnchor { point, direction }` records tip + inward unit direction.
Renderers draw arrowheads using `DEFAULT_ARROW_SIZE_MM`.

### Decision 9 — pattern: one feature → one dimension → one layout

A `PatternCallout` layout anchors on the **union footprint** of every
represented circle (never just the first), with a short leader and text anchor
outside the footprint. Circular-pattern count integrity is enforced:
represented circles must equal the engineering count, else `SCC-DRW-014`.
Grid/Corner footprints use the same union rule (strict count equality reserved
for Circular today).

### Decision 10 — layout lines are not primitives

Dimension lines / extension lines / leaders live only in
`DrawingDimensionLayout`. They never enter `DrawingPrimitive`, keeping
Primitive→Entity→Feature provenance unpolluted. `DrawingSource::Synthetic`
remains available for future genuinely-synthetic primitives but is unused
here.

### Decision 11 — sheet expansion without moving geometry

`DrawingSheet::content_bounds()` unions geometry `sheet_bounds()` **and**
layout extents once; `bounds()` applies the single existing outer margin. The
envelope may only grow toward +X/+Y; min stays `(0,0)`; no negative sheet
coordinates; no per-dimension re-margins; the principal transform and
`DrawingView.model_bounds` are untouched by layout work.

### Decision 12 — renderers must not re-layout

Renderers consume layouts verbatim; no re-fitting, re-centering, re-scaling,
or side selection downstream. Text anchors carry position + orientation only —
width/metrics/breaking are formatter concerns. Collision avoidance between
dimensions/text is deferred.

### Decision 13 — integrity validation

`validate_dimension_layout_integrity(view)`: every dimension has exactly one
layout (`SCC-DRW-013` if missing); no orphan layouts (`SCC-DRW-012`); all
coordinates finite; linear/diameter lines non-degenerate. `build_drawing`
runs it before returning.

## Consequences

- Sheet envelopes for dimensioned drawings grow beyond pure geometry+margin
  (e.g. the 120×80 plate at 1:1 goes from 140×100 to 150×110); Phase 8.3
  envelope tests were updated accordingly.
- New error codes `SCC-DRW-012/013/014`; new public constants and layout
  types; `principal_sheet_mut/principal_view_mut` accessors for validators.

## Rejected alternatives

Placing dimensions below/left then translating geometry to compensate (moves
frozen Phase 8.3 placement); per-dimension margin accumulation (unbounded
growth); measuring geometry for values (breaks value/placement separation);
renderer-side layout (forbidden divergence).

## Compatibility

Additive internally; envelope growth changes serialized sheet bounds for
dimensioned drawings (nothing outside the crate consumed them). All prior
phase behaviors regression-tested.

## Migration plan

None. Not wired into CLI/UI yet.

## Technical debt (unchanged)

Formatting/fonts/collision avoidance deferred; angular & constraint-derived
dimensions deferred; user dimension intent deferred; plus prior-phase debt.
