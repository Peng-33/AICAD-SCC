# ADR-052: Arc, Rounded, and Composite 2D Profile Foundation

Status: Accepted for Phase 12.2

## Context

Phase 12.1 froze Rectangle and Circle as typed base-profile semantics and kept
the three-view pipeline, drawing model, project persistence, and manufacturing
boundary deterministic. Phase 12.2 adds the smallest exact curved-profile
foundation: rounded rectangles and one ordered closed outer loop composed only
of lines and circular arcs. This is not a general sketcher, constraint system,
or manufacturing expansion.

## Decision

1. `EngineeringModel` remains the only editable source of truth.
   `BaseProfile` keeps the legacy Rectangle and Circle wire forms and adds
   explicitly discriminated RoundedRectangle and Composite forms. New forms
   reject unknown or mixed fields; old Rectangle/Circle project JSON remains
   readable without shape guessing.
2. RoundedRectangle stores center, width, height, and one corner radius. Its
   contract is finite center, positive width/height, and
   `0 <= radius <= min(width, height) / 2`. Radius zero remains a typed rounded
   source profile but lowers canonically to four lines and no degenerate arcs.
3. Composite stores exactly one non-empty, ordered outer loop of typed
   `Line` and `Arc` segments. A line stores explicit start/end points. An arc
   stores explicit start/end/center, a positive radius, and clockwise or
   counter-clockwise direction. Direction resolves one unique sweep in
   `(0, 2π)`; full circles remain `BaseProfile::Circle`.
4. All profile validation uses one scale-aware `1e-9 mm` tolerance. It rejects
   non-finite values, zero-length lines, non-positive radii, radius/endpoint
   mismatch, coincident arc endpoints, open or disconnected loops, zero-area
   loops, and every non-adjacent line-line, line-arc, or arc-arc intersection
   or touch. Adjacent segments may meet only at their expected shared endpoint.
5. The normal SCC proposal → analysis → lowering → execution → geometry chain
   is extended with explicit line endpoint and arc angle dimensions. Composite
   lowering does not fabricate a solved model or bypass the formal pipeline.
   Each generated entity has typed `(feature, segment_index, semantic)` source
   provenance; semantic meaning is never recovered by parsing entity IDs.
6. `SpatialFootprint` adds an exact ordered composite boundary containing
   native line/arc geometry. RoundedRectangle lowers to that same exact
   footprint. Bounds are derived from line endpoints and arc cardinal extrema;
   they are an envelope only and never substitute for profile geometry.
7. Front projection emits near/far native line and arc witnesses. Top and Left
   derive their planar interval from exact profile extrema and combine that
   interval with the real thickness. They do not copy Front primitives and do
   not use stored bounds as geometric truth. Existing first/third-angle view
   placement remains presentation-only.
8. Projection adds a native Arc geometry and body-segment provenance. Visibility
   and consistency validate the generalized body contract while preserving the
   existing Rectangle/Circle behavior and cut semantics.
9. Drawing maps projected arcs to the existing `DrawingPrimitive::Arc` and uses
   the existing DrawingModel, SVG/DXF renderers, and Phase 11 layout engine.
   RoundedRectangle owns width, height, and one corner-radius annotation.
   Generic Composite arc-radius auto-dimensioning is deferred; no radius is
   inferred from coincident geometry.
10. Project v1 persists only typed engineering source, including segment order,
    arc direction, and exact source values. Geometry, spatial, projection,
    drawing, renderer paths, and layout coordinates remain derived artifacts.
11. RoundedRectangle and Composite fail closed at the manufacturing boundary
    with typed unsupported errors. No bounding rectangle, polygonal sampling,
    SolidBuildSpec fabrication, CadQuery approximation, or STEP export is
    permitted in Phase 12.2.

## Frozen scope

Included: exact line/arc outer loops, RoundedRectangle, profile validation,
typed segment provenance, exact spatial footprint and three-view presentation,
native SVG/DXF arcs, project round-trip, rounded DSL/tool entry points, and
compatibility/regression witnesses.

Deferred: manufacturing and STEP for new profiles, inner/multiple loops,
islands, slots/obrounds, pockets/bosses, fillets/chamfers as generic features,
Bezier/spline/ellipse curves, general sketch constraints, geometric solving of
free sketches, and heuristic recognition of rounded profiles.

## Consequences

`SpatialFootprint` is no longer `Copy`; downstream code matches it by reference.
The formal SCC dimension vocabulary gains only the parameters required to
materialize already-resolved line and arc entities. Existing Rectangle/Circle
goldens and rectangular manufacturing remain frozen. Invalid curved profiles
stop before spatial, drawing, project delivery, or manufacturing artifacts can
be produced.
