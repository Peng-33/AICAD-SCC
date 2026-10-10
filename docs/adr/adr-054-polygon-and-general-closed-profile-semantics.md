# ADR-054: Polygon and General Closed Profile Semantics

Status: Accepted for Phase 12.4

## Context

Phase 12.2 froze typed `RoundedRectangle` and one-loop Line/Arc `Composite`
profiles. Phase 12.3 froze through Slot cuts. Phase 12.4 adds a typed polygon
base profile and confirms Composite as the single general closed Line/Arc
profile. This is not a sketcher, multi-contour system, or manufacturing
expansion.

## Decision

1. `BaseProfile` is Rectangle | Circle | RoundedRectangle | Polygon |
   Composite. Polygon is a typed engineering semantic, not anonymous Composite.
2. Composite remains the only general Line/Arc closed-profile truth. No
   `GeneralClosedProfile`, `ArbitraryProfile`, or second composite type.
3. Polygon source is an ordered vertex list. Closure is implicit: last vertex
   connects to the first. The first vertex is not repeated at the end.
4. A polygon has at least three vertices.
5. Canonical winding is counter-clockwise (positive signed area). Clockwise
   input is reversed on ingest.
6. Canonical start vertex is the lexicographically smallest `(x, y)` using
   total ordering of finite coordinates, then the remaining CCW cycle.
   Same geometry therefore has one stored identity.
7. Composite keeps author-supplied segment order and arc direction. Polygon
   canonicalization must not rotate or reverse Composite segments.
8. Degenerate polygons are rejected: fewer than three vertices, duplicate
   vertices, zero-length edges, all-collinear / zero area, NaN, Inf.
9. Self-intersection, including bow-tie polygons, is rejected using the
   existing Phase 12.2 line-line intersection helpers. No second geometry
   kernel.
10. Polygon lowers to `Line(v_i, v_{i+1})` with implicit close. Engineering
    source map records `PolygonEdge { index }`. Front projection continues
    to use `BodySegment { index }`. IDs are never parsed.
11. Spatial footprint is the exact extruded polygon (stored as
    `SpatialFootprint::Composite` of those lines). Bounding-rectangle
    replacement is forbidden (`POLYGON_BBOX_AS_SPATIAL = NO`).
12. Front is the exact n lines. Top/Left are true silhouettes of that
    footprint × thickness, never copies of Front.
13. PCV remains validator-only. Visibility is segment-count driven.
14. Drawing shows the outline plus overall X/Y extents from resolved
    geometry bounds. Per-edge lengths and angles are deferred. Phase 11
    remains the sole layout authority.
15. Hole and Slot remain separate cut features. Containment fail-closed,
    never clipped. No `PolygonHole` / `PolygonSlot` types.
16. Project v1 stores Polygon vertices and Composite segments only. Derived
    geometry, spatial, projection, and drawing are not persisted.
17. Manufacturing is fail-closed deferred for Polygon and general Composite.
    No bounding-box SolidBuildSpec fallback.
18. Multiple contours, inner loops, islands, Bezier/Spline/Ellipse, fillet,
    chamfer, pocket, and general sketch constraints are deferred.
19. Tool calling for Polygon is supported through `create_base_polygon` with
    a bounded `points` array of two-number coordinate pairs. The adapter
    validates and canonicalizes before emitting `EngineeringCommand`. DSL
    `BASE_POLYGON` / `POINT` also lowers to `EngineeringCommand`.
    Natural-language Polygon is deferred.

## Frozen scope

Included: typed Polygon, Composite as the general Line/Arc closed profile,
canonical winding/start-vertex for Polygon only, exact lowering/spatial/
projection/drawing, Hole/Slot coexistence, project round-trip, DSL/tool
surfaces, containment coverage, Phase 12.4 goldens, and old golden
compatibility.

Deferred: NL Polygon, per-edge dimensions, manufacturing/STEP, and
multi-contour/inner-loop work.

## Consequences

Existing Rectangle/Circle/RoundedRectangle/Composite JSON remains readable.
Unknown `profile_kind` still fail-closed. Polygon JSON uses
`profile_kind = "polygon"` plus `vertices`. Composite identity is unchanged.
