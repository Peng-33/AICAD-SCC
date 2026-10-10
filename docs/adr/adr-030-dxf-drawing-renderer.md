# ADR-030: DXF drawing renderer

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022..029, `crates/scc-drawing/src/dxf.rs`

## Background

Phase 8.8 delivered `drawing_to_svg` over the derived `DrawingModel`. Phase
8.9 completes the renderer pair with `drawing_to_dxf(&DrawingModel) ->
Result<String, DrawingError>` — the same drawing truth rendered into
engineering DXF. SVG and DXF are two renderers of one model; neither owns CAD
logic.

## Decisions

### Decision 1 — consumes only DrawingModel, immutable

No engineering/geometry models, no source maps, no toggles, no UI state. All
positions/values/styles-semantics are already decided.

### Decision 2 — Sheet Space maps directly to DXF Cartesian space

`dxf = (sheet_x, sheet_y)`. Both are +X right / +Y up / mm; there is **no**
SVG-style Y-flip here, and the SVG flip must never be copied.

### Decision 3 — 1 DXF drawing unit = 1 sheet millimetre

Declared via `$INSUNITS = 4` (millimeters).

### Decision 4 — DrawingScale applied exactly once (by the model)

Primitives transform model→sheet through the stored view transform once;
layouts/centers/annotations arrive already in sheet space and are never
transformed again. Dimension text renders the resolved engineering value at
any scale (120 stays 120 while the entity spans 60 at 1:2).

### Decision 5 — extents come from DrawingSheet.bounds

Header `$EXTMIN/$EXTMAX` derive strictly from `DrawingSheet::bounds()`
(`0,0 → W,H`). Entities outside the bounds are a consistency error
(fail-closed), never a reason to re-fit.

### Decision 6 — layer policy (renderer-owned names)

```text
AICAD_OBJECT       object primitives        CONTINUOUS
AICAD_CONSTRUCTION construction primitives  AICAD_DASHED
AICAD_CENTER       center marks/lines       AICAD_CENTERLINE
AICAD_DIMENSION    all dimension geometry   CONTINUOUS
AICAD_ANNOTATION   part title               CONTINUOUS
```

Layers are presentation routing only — never reverse-engineered into
features. Color stays default/ByLayer-compatible (7); no per-entity styling
truth.

### Decision 7 — linetypes declared for everything referenced

The TABLES section defines `CONTINUOUS`, `AICAD_DASHED`
(4/−2 mm pattern), and `AICAD_CENTERLINE` (8/−2/2/−2 mm) in paper-mm terms;
construction and center are visually distinct by definition, not just name.
Pattern lengths mirror the SVG visual constants numerically — similarity of
values does not create shared engineering truth.

### Decision 8 — no native DXF DIMENSION entities

Native DIMENSION would introduce DIMSTYLE/anonymous blocks/CAD-recomputed
text and arrows — a second layout authority that could contradict
`DrawingDimensionLayout`. Every dimension renders as primitive DXF:
dimension line, extension lines, leader/axis lines, SOLID arrowheads, and
TEXT. Native DIMENSION export is a possible future semantic-DXF phase behind
its own ADR.

### Decision 9 — dimension rendering consumes layout + formatter

Lines/extents/arrows come from `DrawingDimensionLayout`; text comes from the
shared [`format_dimension`](crate::dimension::format_dimension) used by SVG.
Values are never re-measured from geometry; pattern/grid/corner strings are
byte-identical between the two renderers.

### Decision 10 — TEXT semantics under Y-up

Single-line `TEXT` (not MTEXT): height paper-mm
(`DEFAULT_DIMENSION_TEXT_HEIGHT_MM` / annotation height); rotation degrees
CCW in Y-up — vertical dimension text uses **90** (the SVG `rotate(-90)` must
not be copied). Anchors map: Start→left-aligned insertion point,
Middle→group 72=1 with alignment point, End→72=2.

### Decision 11 — Unicode policy

AC1027 files encode text as UTF-8; titles like "复杂异形精密安装板" and glyphs
"Ø ×" are written verbatim as UTF-8 and verified by tests.

### Decision 12 — DXF escaping ≠ XML escaping

No XML entities. Defensive sanitization strips ASCII control characters
(including CR/LF) that would corrupt group-code structure; legitimate Unicode
content is never altered.

### Decision 13 — deterministic file order

Fixed sections HEADER→TABLES→ENTITIES→EOF; fixed table declaration order;
fixed entity group order objects→construction→centers→dimensions→
annotations, each iterated in BTreeMap order; locale-free number formatting
shared with SVG. Byte-for-byte identical output across runs; golden fixture
locked.

### Decision 14 — not an engineering round-trip format

DXF output is a drawing export. No handles-for-provenance, XDATA, APPID, or
importer metadata in this phase.

### Decision 15 — legacy GeometryModel DXF exporter preserved

Its low-level writers (`group`, point/line/circle/arc/closed-polyline
entities) were extracted as public helpers parameterized by layer and are now
shared by both exporters; the legacy file's bytes are unchanged (golden-
verified).

## Consequences

- New public API `drawing_to_dxf`; new error `SCC-DRW-018 InvalidDxfDrawing`.
- Golden fixture `tests/golden/engineering_plate.dxf`; cross-renderer parity
  test asserts SVG/DXF semantic agreement on one shared DrawingModel.

## Rejected alternatives

Native DIMENSION entities (second layout truth); MTEXT (formatter is
single-line); copying SVG's Y-flip (wrong under Y-up); XML escaping (wrong
encoding family); auto-growing extents (bounds are truth).

## Compatibility

Additive; legacy exporter byte-identical (geometry golden verifies).

## Migration plan

None. CLI/UI integration deferred.

## Technical debt (added)

Native DIMENSION/DIMSTYLE semantic export; ISO/GB/ASME formatting; fonts/text
metrics; paper sizes/title blocks; collision avoidance; angular & constraint
dimensions; DXF round-trip metadata.
