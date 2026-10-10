# ADR-026: Drawing dimension model

- Status: Accepted
- Date: 2026-08-24
- Related: ADR-022..025, `crates/scc-drawing/src/dimension.rs`

## Background

Phases 8.1–8.4 established the derived `DrawingModel` (primitives, roles,
provenance, sheet space, annotations). Phase 8.5 adds the engineering
dimension *model*: what is measured, and where the number comes from. Layout
(where lines are drawn) and formatting (`"Ø64"`) are explicitly later phases.

## Decisions

### Decision 1 — dimensions are their own type, separate from annotations and primitives

`DrawingDimension` lives in
`DrawingView.dimensions`. No `DrawingAnnotationKind::Dimension`; no dimension
text inside annotations; no geometry primitive masquerading as a dimension.
Dimensions have engineering sources, measurement semantics, targets, units,
and (later) layout needs that annotations do not share.

### Decision 2 — a dimension belongs to a DrawingView

A dimension measures *that view's* engineering geometry. With only the
principal view today every dimension lands there; Phase 9 projected views will
each own theirs.

### Decision 3 — typed source, never an authoritative literal or string

Every number carries a [`DimensionValueSource`]:

```rust
enum DimensionValueSource {
    Parameter { parameter: ParameterId },
    FeatureSlot { feature: FeatureId, slot: FeatureParameterSlot },
}
```

`resolved` values (`LengthMm(f64)`, `Count(u32)`) are **derived presentation
caches** recomputed on every build; the source is the provenance. No
`value: 120.0`, no `"Ø64"` stored anywhere.

### Decision 4 — parameter binding wins over feature literal

Resolution follows the Phase 3 lowering order exactly: if
`feature_parameters[feature][slot]` exists the source is `Parameter`;
otherwise (and only otherwise) `FeatureSlot`. After re-binding
`main.Width → plate_width`, the dimension must reference the parameter — a
stale literal reference would show 120 after the parameter moved to 160.

### Decision 5 — bound-but-unresolved fails closed

An unresolved bound parameter never falls back to the literal or zero:
`SCC-DRW-010 UnresolvedDimensionValue`. In practice unreachable at drawing
time because lowering already rejects it (`SCC-ENG-027`, ADR-017 Decision 4);
the drawing gate is defensive.

### Decision 6 — dimensions express real model values; DrawingScale never changes them

120 mm reads as 120 at scale 1.0, 0.5, and 2.0. The paper presentation size of
the geometry changes; the engineering value does not.

### Decision 7 — pattern dimensions are per-feature and structured

One `HolePattern` yields ONE `Pattern` dimension whose shape mirrors the
engineering variant:

- Circular → `{count, pitch_circle_diameter, hole_diameter}`
- Grid → `{rows, columns, spacing_x, spacing_y, hole_diameter}`
- Corner → `{offset_x, offset_y, hole_diameter}`

Each component carries its own source + resolved cache. Counts never come from
counting solved circles; PCD never from measuring coordinates; diameters never
from `radius × 2` — all from engineering slots. Eight expanded circles still
map back to the single pattern feature and its single dimension.

### Decision 8 — conservative MVP auto-generation policy

BaseProfile → width (Linear Horizontal) + height (Linear Vertical); Hole →
Diameter; HolePattern → one Pattern dimension; ConstructionCircle → none;
Slot/Fillet → none (never lower anyway). No automatic dimensions from
arbitrary parameters, constraints, helper geometry, or user selection.

### Decision 9 — targets are features; representation is verified

`DrawingDimensionTarget::Feature { feature }` — no fake edge/entity ids. Each
generated dimension's target must have at least one represented primitive in
the current presentation (`primitives_for_feature`), else fail-closed
`SCC-DRW-011 InvalidDimensionTarget`.

### Decision 10 — no layout coordinates

No dimension-line position, extension lines, arrows, or text position here —
Phase 8.6. Only engineering orientation (`LinearAxis::Horizontal|Vertical`) is
stored, because it is semantics, not placement.

### Decision 11 — MVP kinds; Radius defined but not generated

`Linear{axis}`, `Diameter`, `Pattern`, plus `Radius` reserved (no current
reliable radius-backed feature; ConstructionCircle generates nothing). Angular
dimensions deferred even though `start_angle` exists on circular patterns.

### Decision 12 — constraint-derived dimensions deferred

EngineeringConstraint::Distance could drive driven dimensions later, but the
Constraint SourceMap counter-prediction debt makes that fragile today; the
feature-based policy proves the architecture first.

### Decision 13 — integrity validation covers dimensions

`validate_dimension_integrity(view, model)` checks target existence, kind/value
correlation, valid caches, slot-valid-for-feature, parameter existence, and —
decisively — that every source **re-resolves against the model to exactly the
cached value**, so tampered models cannot pass silently. `build_drawing` runs
it before returning.

### Decision 14 — builder input widened deliberately, immutably

`DrawingInputs.engineering_model: Option<&EngineeringModel>` (read-only).
Rationale: resolution order must match lowering exactly; duplicating binding/
literal resolution in the drawing crate invites divergence. `None` skips
dimension generation (geometry-only drawings remain legal).

### Decision 15 — stable semantic ids, never parsed

`"{feature}.width"`, `"{feature}.height"`, `"{feature}.diameter"`,
`"{feature}.pattern"` — deterministic identity only; semantics live in typed
fields. Multiple dimensions per feature get distinct ids; a FeatureId is never
used directly as a dimension id.

## Consequences

- `DrawingView` gains `dimensions` (serde shape change; internal only).
- New errors `SCC-DRW-009/010/011`.
- Parameter edits and derived-expression changes propagate automatically:
  re-solve → rebuild → same sources, fresh values (e.g. width 120→160,
  `hole_diameter := plate_height / 8` giving 10→12 when height 80→96).

## Rejected alternatives

Storing formatted strings; per-circle pattern dimensions; geometry-measured
values; `Literal(120)` sources for unbound features (FeatureSlot is formal and
typed); a second public `build_dimensions()` pipeline; new EngineeringCommands
(dimensions are derived state — user-selected dimension intent is recorded as
a future design question).

## Compatibility

Additive internally; prior-phase behavior unchanged and regression-tested
(PartTitle, provenance, sheet space, determinism).

## Migration plan

None. Not wired into CLI/UI yet.

## Technical debt (unchanged)

Constraint SourceMap counter prediction (also why constraint-derived
dimensions are deferred); complex two-pass planner; Polygon solver limitation;
circular non-zero start-angle representation; ConstructionCircle renderer
styling (8.7); long-title overflow (8.4).
