# ADR-025: Basic drawing annotations (PartTitle)

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022, ADR-023, ADR-024, `crates/scc-drawing`

## Background

Phases 8.1–8.3 established the derived `DrawingModel` with model-space
primitives, roles, provenance, and the Model/View/Sheet coordinate system.
Phase 8.4 adds the first *non-geometric* drawing information: annotations.
The MVP generates exactly one — the **PartTitle**, derived from
`PartDefinition.name`. This is not a dimension system, not a text-layout
system, and not a renderer.

## Decisions

### Decision 1 — annotations are separate from geometry primitives

`DrawingPrimitive` stays solved/presented *geometry*. Non-geometric drawing
information lives in a parallel collection:
`DrawingSheet.annotations: BTreeMap<DrawingAnnotationId, DrawingAnnotation>`.
Text is never disguised as a geometry primitive.

### Decision 2 — PartTitle is a sheet-level annotation

A part title describes the whole part / whole sheet, not any single view's
geometry, so it belongs to `DrawingSheet`, not to the principal view.

### Decision 3 — annotations are derived presentation data

Engineering truth: `PartDefinition.name` (ADR-016 remains the single source of
truth). Drawing presentation: the resolved annotation text, re-derived on every
build; changing the part name and re-building changes the title with no hand
editing. There is no `MoveAnnotation` / `SetAnnotationText`
`EngineeringCommand`; annotations add no new engineering state and never write
back to `EngineeringModel`.

### Decision 4 — strong-typed provenance, not strings and not synthetic

`DrawingAnnotationSource::PartTitle { part: PartId }` traces an annotation back
to its `PartDefinition`. A PartTitle has a real engineering source, so it is
**never** `DrawingSource::Synthetic` and never carries a fabricated entity or
feature id. The primitive `DrawingSourceMap` (`PrimitiveId → EntityId →
FeatureId`) is untouched — annotations have no entity id, so no fake mapping is
invented; provenance is embedded on the annotation itself (annotations are few
and 1:1 with their source).

### Decision 5 — annotation position is sheet space

`position` is millimetres on paper, `+X` right, `+Y` up, origin bottom-left
(ADR-024). No SVG pixel, CSS, or Y-down value ever enters the model; the future
SVG renderer owns the Y flip.

### Decision 6 — text height is paper mm and is never scaled

`DEFAULT_ANNOTATION_TEXT_HEIGHT_MM = 3.5` paper millimetres. At 1:2 the
geometry halves; a 3.5 mm title stays 3.5 mm. `DrawingScale` applies only to
model→sheet geometry, never to annotation text height or position.

### Decision 7 — renderer-specific text details stay out

No font family, no CSS/SVG anchor names, no DXF style table in `DrawingModel`.
MVP carries only `text_height_mm` and a domain-level
`DrawingTextAnchor { Start, Middle, End }` (vertically the position is the
text baseline). Escaping (`&amp;`, DXF strings) is a renderer concern;
annotation text is stored raw UTF-8.

### Decision 8 — deterministic MVP placement inside the existing margin

```text
title_x     = DEFAULT_SHEET_MARGIN_MM        (10)
title_y     = DEFAULT_SHEET_MARGIN_MM / 2    (5)
anchor      = Start ; height 3.5 mm
```

With Phase 8.3's placement the content minimum sits at `(margin, margin)`, so
the bottom margin band `[0, margin]` is a natural title area and the title
fits inside the existing envelope.

### Decision 9 — the title never moves geometry or the sheet envelope

Annotations do not participate in `DrawingView::model_bounds`,
`DrawingSheet::content_bounds()`, `bounds()`, or any `DrawingViewTransform`.
Re-fitting/re-centering for a long title belongs to later layout phases.
**Known limitation:** a long title may exceed the available horizontal space;
collision / overflow handling is deferred.

### Decision 10 — dimension text is not an annotation

"Ø64", "120", "R30" belong to the Phase 8.5 Dimension Model with
parameter-referencing semantics. `Text("Ø64")` must never become an
authoritative dimension via the annotation path.

### Decision 11 — missing part metadata is not an error

`DrawingInputs.part` is `Option<&PartDefinition>` because geometry-only
drawings legitimately exist (e.g. synthetic fixtures). With `part = None` the
drawing builds without a title. An empty/blank part name likewise skips the
title (an empty display name must not fail the geometry drawing). A name
containing control characters (possible only via hand-constructed models;
`CreatePart` validation already rejects empty/surround-whitespace names) fails
closed with `SCC-DRW-008` rather than being silently altered or rendered.

### Decision 12 — integrity validation covers annotations

`validate_annotation_integrity` checks structural validity (finite position,
positive finite text height, non-empty control-char-free text), at most one
`PartTitle` per sheet, and — when the `PartDefinition` is supplied — that the
source part id and text match it exactly. `build_drawing` runs it before
returning.

### Decision 13 — Unicode is preserved verbatim

Titles like "复杂异形精密安装板" round-trip through serde byte-for-byte. Glyph
availability in renderer fonts is a deployment concern, not a model concern.

## Consequences

- `DrawingSheet` gains an `annotations` map (serde shape change; nothing
  outside the crate consumed it).
- New error `SCC-DRW-008 InvalidDrawingAnnotation`.
- New query APIs without new state: `DrawingSheet::annotation`,
  `DrawingModel::annotation`, `DrawingModel::annotations_for_part`.

## Rejected alternatives

- **`DrawingPrimitive::Text`.** Would mix geometry and non-geometric content
  and drag primitives into typography concerns.
- **Storing titles in a global annotation index keyed by string.** Strong
  types plus natural hierarchy suffice; no duplicated indexes.
- **Failing the build when `part` is missing.** Geometry-only drawings are
  legitimate; only malformed names fail closed.

## Compatibility

Additive internally. All Phase 8.1–8.3 behavior (provenance, roles, transforms,
bounds) is unchanged and regression-tested with annotations present.

## Migration plan

None. Not wired into CLI/UI yet.

## Technical debt (unchanged)

Constraint SourceMap counter prediction; complex two-pass planner; Polygon
solver limitation; circular non-zero start-angle representation;
ConstructionCircle renderer styling (Phase 8.7); long-title overflow /
collision handling deferred.
