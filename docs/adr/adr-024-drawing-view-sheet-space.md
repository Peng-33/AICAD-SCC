# ADR-024: Drawing view / sheet space

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022, ADR-023, `crates/scc-drawing`, `docs/specs/phase8-engineering-drawing-engine.md`

## Background

Phases 8.1–8.2 established the derived `DrawingModel` (model-space primitives,
roles, fail-closed provenance). Phase 8.3 formalizes *where* that model-space
geometry lands on a drawing sheet: one deterministic coordinate system that
every later element (dimensions, annotations, centerlines) and every renderer
(SVG, DXF) must reuse. No renderer may ever compute its own fit/center/scale.

## Decisions

### Decision 1 — three formally named spaces

- **Model space** — the solved geometry's own millimetre coordinates
  (`GeometryModel` / SCC engineering convention; may be negative and centered
  at the origin). `DrawingPrimitive`s store model-space geometry **only**.
- **View space** — model geometry after the engineering drawing scale
  (`view = model × DrawingScale`), local to a view.
- **Sheet space** — millimetres on paper, `+X` right, `+Y` **up**, origin at
  the bottom-left corner. Renderer-independent.

### Decision 2 — primitives are never duplicated into a second transformed copy

`DrawingPrimitive` keeps only authoritative model-space coordinates. Sheet
positions are derived through `DrawingViewTransform` helpers
(`model_to_view`, `view_to_sheet`, `model_to_sheet`, `transform_bounds`,
`DrawingView::sheet_bounds`). There is no stored sheet-circle next to a
model-circle.

### Decision 3 — the transform is exactly scale + translation

```text
sheet = model × scale + translation      (no rotation, no matrix, no camera)
```

`DrawingViewTransform::new(scale, translation)` validates both (finite scale,
strictly positive; finite translation) and fails closed with `SCC-DRW-007`.
Composition is explicit: `model_to_sheet(p) == view_to_sheet(model_to_view(p))`.

### Decision 4 — DrawingScale is engineering drawing scale, never UI zoom

`1.0` = 1:1, `0.5` = 1:2, `2.0` = 2:1 (model mm → paper mm). UI zoom / pan is
UI-only state and must never be written into `DrawingScale`; a future workbench
may display a 1:1 drawing at 137% zoom without touching the drawing.

### Decision 5 — deterministic principal-view placement

The single principal view is placed by one rule:

```text
translation = DEFAULT_SHEET_MARGIN_MM − model_bounds.min × scale
```

so the scaled presentation minimum lands exactly on `(margin, margin)` and all
content sits inside the margin frame in positive sheet space. Example:
a −60..60 × −40..40 plate at 1:1 gives `tx = 70`, `ty = 50`, content
`10..130 × 10..90`.

### Decision 6 — the sheet envelope is a derived presentation default, not truth

`DrawingSheet::bounds()` derives the paper envelope as
`content size + DEFAULT_SHEET_MARGIN_MM on every side`, origin `(0,0)`
bottom-left. `DEFAULT_SHEET_MARGIN_MM = 10.0`. The EngineeringModel has no
paper-size concept, so standard formats (A4/A3/A2, orientation selection,
"auto-fit to paper") are explicitly deferred — this envelope is a minimal
deterministic presentation default, not an engineering property and not a new
`EngineeringCommand`.

### Decision 7 — renderers must not re-fit, re-center, re-scale, or re-place

SVG/DXF renderers consume `DrawingViewTransform` and the derived sheet bounds
as-is. Their only permitted work is format mapping (e.g. SVG's Y-down pixel
space is produced by the SVG renderer from Y-up sheet space; DXF details stay
in the DXF renderer). No second fit/scale/center/margin algorithm may appear
in any renderer or the UI.

### Decision 8 — bounds semantics are explicit

`DrawingView.model_bounds` (renamed from the ambiguous `bounds`) is the
axis-aligned extent of the view's **final presentation** primitives in **model
space**. Sheet-space bounds derive via `DrawingView::sheet_bounds()`.
`DrawingSheet::content_bounds()` unions view sheet bounds;
`DrawingSheet::bounds()` adds the margin frame. Nothing is stored twice.

### Decision 9 — empty presentation vs truly empty geometry

A construction-only part with `ShowConstructionGeometry=false` yields a valid,
empty view: zero model bounds, degenerate sheet content at the margin corner,
and the minimal deterministic envelope `20 mm × 20 mm` — no NaN, no negative
extent. A `GeometryModel` with no solved entities remains a hard
`SCC-DRW-005 EmptyDrawing` failure. The two cases are distinct by design
(extends ADR-023 Decision 12).

### Decision 10 — visibility filtering happens before bounds/placement

Pipeline order: validate non-empty geometry → resolve scale & construction
visibility → per-entity provenance → role → presentation filter → convert →
accumulate presentation model bounds → compute transform → derive sheet
bounds → assemble. Bounds therefore always describe what the drawing actually
presents.

### Decision 11 — conservative arc bounds remain acceptable

Arc bounds stay the conservative `center ± radius` box from Phase 8.1. Precise
arc extrema are a future optimization and do not block any current consumer.

### Decision 12 — three-view projection is out of scope

No Front/Top/Left views, no hidden line, no spatial semantics. The transform
and placement rules here are per-view and extend unchanged when Phase 9 adds
projected views.

## Consequences

- Every future drawing element shares one placement rule; renderers become
  dumb formatters over sheet space.
- `DrawingView.bounds` was renamed to `model_bounds` (serde shape changes;
  nothing outside `scc-drawing` consumed it).
- New error `SCC-DRW-007 InvalidDrawingTransform`.
- Convenience accessors added without new state: `DrawingModel::principal_sheet`,
  `principal_view`, `locate_primitive` (typed traversal), `DrawingSheet::bounds`,
  `content_bounds`, `DrawingView::sheet_bounds`.

## Rejected alternatives

- **Storing both model-space and sheet-space primitives.** Two authoritative
  copies invite drift; the brief forbids it.
- **Y-down sheet space to match SVG.** Would let an implementation detail leak
  into drawing truth; the SVG renderer owns the Y flip instead.
- **Auto paper-size selection / fit-to-A4.** No paper-size semantics exists in
  the engineering model; inventing one now would put layout policy ahead of a
  real requirement.
- **A separate `build_sheet` pipeline.** One public entry point
  (`build_drawing`) stays; placement is an internal step.

## Compatibility

Additive internally; the `DrawingView.bounds → model_bounds` rename changes
the drawing JSON shape but nothing outside the crate serialized it yet.
`SCC-DRW-007` is new. All prior behavior (provenance, roles, filters,
determinism) is preserved and now tested across scales 1:2, 1:1, 2:1.

## Migration plan

None. Not wired into CLI/UI yet.

## Technical debt (unchanged)

Constraint SourceMap counter prediction; complex two-pass planner; Polygon
solver limitation; circular non-zero start-angle representation;
ConstructionCircle renderer styling (Phase 8.7).
