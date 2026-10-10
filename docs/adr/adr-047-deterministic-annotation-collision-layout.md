# ADR-047: Deterministic annotation collision layout

- Status: accepted
- Scope: Phase 11.2 formal Front/Top/Left drawing annotation layout
- Date: 2026-09-11
- Related: ADR-027, ADR-028, ADR-046, `crates/scc-drawing/src/annotation_layout.rs`

## Background

Phase 11.1 establishes production dimensions and center presentation but leaves
crowded dimension layouts to a later bounded phase. Formal three-view drawings
need a deterministic collision gate without moving engineering geometry or
re-evaluating annotation values.

## Decisions

1. Collision analysis is a derived drawing pass over existing
   `DrawingDimensionLayout` values; it does not change `EngineeringModel`,
   `SpatialModel`, `ProjectionModel`, `VisibilityModel`, or dimension values.
2. Dimension footprints include layout geometry and a conservative paper-space
   text envelope. Text width uses a fixed glyph-advance approximation because
   renderer font metrics are not part of the drawing model.
3. Dimension IDs are processed in `BTreeMap` order. The first non-colliding
   candidate is selected, making repeated builds byte-deterministic.
4. Phase 11.1 linear dimensions move only farther on their established side:
   horizontal dimensions move downward and vertical dimensions move leftward,
   one `DEFAULT_DIMENSION_STACK_PITCH_MM` per candidate.
5. Diameter axes and pattern leaders are witness-attached and are never moved.
   An unresolved collision returns `SCC-DRW-012` instead of detaching the
   presentation from its geometry.
6. Reflow is bounded by `MAX_ANNOTATION_REFLOW_ATTEMPTS`; there is no unbounded
   search, renderer-side layout, or view re-fitting.
7. The pass runs only in the formal three-view builder. Legacy single-view
   Principal output remains byte-stable.

## Rejected alternatives

- Moving view geometry or changing `DrawingViewTransform`: breaks projection
  alignment and scale semantics.
- Moving diameter axes: breaks the through-circle witness contract.
- Using renderer font APIs: creates renderer-dependent model output.
- Silent overlap acceptance: produces non-production drawings without a
  verifiable failure.
- Unbounded optimization: risks non-termination and unstable sheet sizes.

## Consequences

Clean Phase 11.1 fixtures retain their exact layout and golden bytes. Crowded
linear dimensions are stacked outward deterministically. Unsupported or
unresolvable crowded diameter/pattern layouts fail closed with the existing
structured drawing-layout error.

## Migration plan

None. The formal three-view builder invokes the pass before final sheet-bound
normalization; existing generic Phase 8 layout generation remains unchanged.
