# ADR-037: Projection Consistency Validator

- Status: Accepted
- Scope: AICAD Phase 9.6
- Date: 2026-09-04

## Context

Phase 9.2 produces the resolved `SpatialModel`, Phase 9.4 produces raw
principal-view projection evidence, and Phase 9.5 records a separate
`VisibilityModel` sidecar. Each artifact is deliberately read-only and
provenance-bearing, but deserialization and public fields can still represent
a structurally complete yet semantically inconsistent combination.

The next boundary must prove that the three truths agree before a future
drawing layer consumes them. This is a consistency gate, not another
projection engine and not another visibility classifier.

## Decision

Add the independent `scc-projection-consistency` crate. Its public entry point
is:

```rust
pub fn validate_projection_consistency(
    spatial: &SpatialModel,
    projection: &ProjectionModel,
    visibility: &VisibilityModel,
) -> Result<(), ProjectionConsistencyError>;
```

The validator reads all three models, never mutates them, never calls
`project_principal_views()`, and never calls `classify_visibility()`. It uses
the projection source map as the only source of identity and compares the
frozen canonical local formulas against stored geometry and depth evidence.
It returns no report because a successful report would add no stable business
meaning beyond `Ok(())`; all actionable information is in the typed error.

## Layer boundary

The crate is a downstream sidecar validator. It depends directly on
`scc-core`, `scc-spatial`, `scc-projection`, and `scc-visibility`; the existing
engineering/geometry dependencies remain transitive through `scc-spatial`.
There is no direct dependency on DrawingModel, SVG, DXF, styling, UI, DSL, or
intent-service code.

## Validated models

The three arguments are the authoritative artifacts: SpatialModel owns the
resolved body/cut geometry, ProjectionModel owns raw principal-view geometry,
depth, frames, and ProjectionSourceMap, and VisibilityModel owns one class per
raw primitive. The validator does not create a replacement model or identity
index.

## No-recomputation principle

The validator performs local invariant comparisons only. It may use the frozen
named-axis frame transform and the explicitly documented rectangle/cylinder
relations to derive expected values, but it does not invoke the full
projection pipeline or visibility classifier.

## Validation contract

Validation is fail-closed, deterministic, and ordered as follows:

1. the Phase 9.5 supported spatial domain;
2. exact Front/Top/Left views;
3. source-map integrity and source existence;
4. projection model structure and canonical frames;
5. visibility decision coverage;
6. body/cut roles, geometry shapes, and primitive cardinality;
7. source-derived projected geometry;
8. source-derived depth ranges;
9. visibility and coincidence/occlusion invariants.

Body edges are compared as unoriented segments. Cut circles and side-view
segments are compared using the canonical frame relations and deterministic
tolerance matching. Primitive IDs remain opaque. The validator checks source
and role semantics before using geometry, so a plausible shape cannot conceal
bad provenance.

## Geometry consistency

Every view has the eight body-boundary segments, each cylindrical cut has two
Front circles and four Top/Left segments, and every stored shape is compared
against the source rectangle/cylinder relation without relying on primitive ID
spelling or rounded geometry keys. Segment orientation is immaterial.

## Depth consistency

Depth ranges are checked independently after geometry matching. Front uses +Z,
Top uses +Y, and Left uses -X; side-view rim spans and tangent point depths
remain distinct. A finite geometry match with the wrong depth is `SCC-PCV-006`.

## Visibility consistency

The stored sidecar is checked, not regenerated: nearest coincident body/circle
candidates must have the frozen Visible/Hidden/Occluded relationship; tangent
side candidates are Hidden; a rim contained by a visible body edge is Occluded;
otherwise it is Hidden.

## Fail-closed policy

Missing views, primitives, source mappings, decisions, invalid roles, malformed
geometry/depth, unsupported topology, and non-finite values all return `Err`;
no warning or partial validation succeeds.

## Determinism

BTreeMap/BTreeSet traversal and a fixed stage order determine the first error and
its context. Equal inputs therefore produce the same code and offending ID.

## Supported domain

Version 1 is exactly the intersection of the Phase 9.2/9.4/9.5 contracts:

- a rectangular body with canonical local `+Z` interval `[0,T]`;
- strictly internal circular cuts;
- pairwise non-overlapping, non-tangent, non-coincident cuts;
- `ThroughAll` resolves to `[0,T]`; `Blind { Base }` resolves to `[0,D]` and
  `Blind { Opposite }` resolves to `[T-D,T]`; and
- no clipping, splitting, Boolean, mesh, arbitrary topology, or extra
  primitive kinds.

Unsupported topology is an error, never a skipped candidate or warning.

## Errors and numeric policy

The additive `ErrorStage::ProjectionConsistency` uses the `PCV` prefix. The
crate emits `SCC-PCV-001` through `SCC-PCV-008` for invalid model state,
unsupported topology, view/primitive mismatch, source/role mismatch, geometry
mismatch, depth mismatch, visibility mismatch, and non-finite numeric state.
All are non-recoverable errors. The authoritative Phase 9.5 tolerance
`scc_visibility::VISIBILITY_TOLERANCE_MM` is reused; no private epsilon is
introduced. Finite arithmetic is checked before comparison so large finite
coordinates cannot turn into an accidental infinity or overflowed difference.

## Consequences

The future drawing boundary can require this gate without coupling the drawing
layer to lowering or classification internals. The crate intentionally has no
DrawingModel, SVG, DXF, style, UI, DSL, or intent-service dependency. The
existing spatial crate still brings its accepted engineering/geometry
dependencies transitively; Phase 9.6 does not add those as direct
dependencies.

Malformed serde/public-field states are rejected at the gate. A valid
`VisibilityModel` is not treated as proof of semantics: its decisions are
cross-checked against stored projection depth, geometry, source role, and the
frozen Phase 9.5 domain. The validator is therefore a trust boundary, not a
second source of derived artifacts.

## Rejected alternatives

- Re-running `project_principal_views()` and comparing models would create a
  second projection implementation boundary and hide which invariant failed.
- Re-running `classify_visibility()` would make the validator a second
  classifier and could bless a corrupted sidecar with the same corrupted
  assumptions.
- Parsing deterministic IDs would duplicate provenance and violate the opaque
  ID contract.
- Adding DrawingModel integration in this phase would start Phase 9.7 early.

## Future phase boundary

Phase 9.7 owns DrawingModel integration, hidden-line presentation, SVG/DXF
hidden styles, and UI visibility rendering. None of those are started here.

## Migration

No existing model or renderer API changes. Consumers may call the validator
after loading/deserializing the three frozen artifacts. Existing Phase 9.2,
9.3, 9.4, 9.5, Phase 8 renderer, and UI behavior remain unchanged.
