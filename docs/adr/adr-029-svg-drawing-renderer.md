# ADR-029: SVG drawing renderer

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022..028, `crates/scc-drawing/src/svg.rs`

## Background

Phases 8.1–8.7 built the complete derived `DrawingModel`: objects,
construction geometry, annotations, dimensions, dimension layouts, and center
geometry — all in defined coordinate spaces with fail-closed integrity.
Phase 8.8 adds the first renderer: `drawing_to_svg(&DrawingModel) ->
Result<String, DrawingError>`, a deliberately dumb translator.

## Decisions

### Decision 1 — the renderer consumes only DrawingModel

No `EngineeringModel`, no `GeometryModel`, no source maps, no UI state, no
LLM. It takes `&DrawingModel` (immutable) and produces a string.

### Decision 2 — zero layout authority

The renderer never computes bounds for fitting, never centers, never scales,
never moves anything. Every position comes from the model: sheet bounds from
`DrawingSheet::bounds()`, view geometry through the stored
`DrawingViewTransform` (applied exactly once), layouts/annotations/centers at
their stored coordinates.

### Decision 3 — viewBox is exclusively DrawingSheet.bounds

`viewBox="0 0 W H"` and `width/height="…mm"` derive from
`DrawingSheet::bounds()` only — never re-derived from primitive extents.

### Decision 4 — one Y-conversion rule

Sheet +Y-up → SVG +Y-down via a single pointwise rule
`svg = (x, H − y)` applied to every emitted coordinate. No flip groups, so
text can never mirror; text baselines map directly (ADR-025's baseline
semantics equal SVG's default alphabetic baseline).

### Decision 5 — styles are renderer-owned mappings

```text
Object       stroke black 0.5 mm solid        (group "objects")
Construction stroke black 0.25 mm dash  "4 2" (group "construction")
Center       stroke black 0.25 mm dash-dot "8 2 2 2" (group "centers")
Dimensions   solid thin lines + filled triangles + text (group "dimensions")
Annotations  sans-serif text (group "annotations")
```

Dash patterns (`SVG_CONSTRUCTION_DASH_PATTERN`, `SVG_CENTER_DASH_PATTERN`),
widths, and arrow triangles exist only here; the DrawingModel stores semantic
kinds/roles. The renderer reads roles — it never re-checks feature kinds or
visibility toggles.

### Decision 6 — dimension formatting is a pure presentation function

`format_dimension(&DrawingDimension) -> String` lives beside the dimension
model, is deterministic and locale-free, and its strings are ephemeral —
never written back into the model. Canonical MVP forms:
Linear `"120"`; Diameter `"Ø64"`; Radius `"R30"`;
Circular `"8×Ø10 PCD Ø126"`; Grid `"2×3 Ø8 X30 Y30"`;
Corner `"4×Ø10 OFF X15 Y15"`. Numbers reuse the legacy trimmed formatter
(trailing zeros stripped, `-0`→`0`). These are internal canonical forms, not
ISO/GB/ASME standards.

### Decision 7 — values come from semantics, not measurement

Text renders from `DrawingDimension.value.resolved`; layout geometry decides
placement only. The renderer never measures line lengths/radii to produce a
value.

### Decision 8 — arrowheads are renderer geometry

Filled triangles generated deterministically from
`ArrowAnchor { point, direction }` with `DEFAULT_ARROW_SIZE_MM` length and a
renderer half-width constant. No SVG markers, no browser auto-sizing.

### Decision 9 — XML escaping and Unicode belong to the renderer

Model text stays raw (`A&B <Plate> "V2"`, UTF-8 titles preserved verbatim);
the renderer escapes `& < > " '` on emission.

### Decision 10 — determinism

Fixed group order (`objects, construction, centers, dimensions,
annotations`), fixed attribute order, `BTreeMap` iteration, locale-free
number formatting ⇒ byte-for-byte identical output across runs. A golden
fixture (`tests/golden/engineering_plate.svg`) locks one full drawing
byte-stably, asserted by structure (viewBox/counts/styles/texts), not
appearance.

### Decision 11 — SVG ids are hooks, not provenance

Element ids derive from typed drawing ids via a conservative sanitizer
(prefixed, non-alphanumerics → `_`) plus `data-drawing-id`; they serve
debugging/selection only. Engineering truth remains in the DrawingModel maps;
ids are never parsed back.

### Decision 12 — fail-closed rendering

Before emission: sheet/view presence, layout pairing validator, annotation
validity, finite positive bounds. During emission: every coordinate must be
finite. Failures surface as their own structured codes where they exist
(e.g. `SCC-DRW-013`); renderer-specific failures use new `SCC-DRW-017
InvalidSvgDrawing`. Nothing is silently dropped — all six primitive kinds
render.

### Decision 13 — legacy exporter untouched

The GeometryModel→SVG path keeps working unchanged; only its two pure syntax
helpers (`number`, `escape_xml`) were made public for reuse. No DXF/PDF in
this phase.

## Consequences

- New public API `drawing_to_svg` + renderer constants; new error
  `SCC-DRW-017`.
- Golden fixture file added under `crates/scc-drawing/tests/golden/`.

## Rejected alternatives

Group-level Y-flip transform (mirrors text unless extra work per text);
renderer-side fit/margin (forbidden second layout); copying the old exporter
(divergence); emitting formatted strings into the model (breaks 8.5
ownership).

## Compatibility

Purely additive; legacy exports and all prior-phase behavior regression-
tested.

## Migration plan

None. CLI/UI integration deliberately deferred.

## Technical debt (added)

Real drawing-standard formatting (ISO/GB/ASME), precision policy beyond
zero-trim, text metrics/collision avoidance, arrow/style tuning, paper sizes
and title blocks, angular/constraint dimensions.
