# ADR-059: Polygon Manufacturing and Validated STEP Delivery

Status: Accepted

Date: 2026-09-18

Scope: Phase 13.3B. This decision enables the typed Engineering Polygon production path only. Composite and Slot manufacturing remain fail-closed.

## Context

Phase 12.4 established Polygon as a typed Engineering source with implicit closure, canonical counter-clockwise/lexicographic-start vertices, exact Spatial line segments, and source-only project persistence. Phase 13.3A added the internal `SolidFootprint::ClosedProfile` transport and generic CadQuery/STEP machinery, but deliberately did not expose Polygon manufacturing. A Polygon and a Composite can both lower spatially to `SpatialFootprint::Composite`; therefore contour shape alone is not an identity signal.

The manufacturing boundary must enable only an Engineering/Spatial source known to be Polygon, preserve the canonical edge sequence, and reuse the existing validated delivery path without adding a Polygon-specific DTO or backend.

## Decision

1. `SpatialBaseProfile::Polygon` is the sole semantic gate that enables Polygon manufacturing. The lowering requires its exact corresponding `SpatialFootprint::Composite` contour and does not infer the profile from segment shape, count, or closure.
2. Polygon edges are ordered by their typed `PolygonEdge { index }` semantic identity, not lexically parsed entity IDs. Manufacturing checks the contiguous canonical indices, expected source/provenance identity, contour continuity and closure, then copies the exact supplied Line endpoints into the existing `SolidFootprint::ClosedProfile` in that order.
3. Only line segments are accepted for Polygon. An Arc in this route fails closed. Manufacturing does not recanonicalize vertices, reorder by coordinates, tessellate, replace the contour with bounds, or add a Polygon variant to `SolidBuildSpec`.
4. The existing generic ClosedProfile CadQuery route remains authoritative: native analytic line edges, ordered wire, planar face, and +Z extrusion over the resolved interval. Python receives only the generic closed-profile DTO; it does not know the Polygon semantic type and has no Polygon handler.
5. Existing typed cylindrical Hole cuts are reused. Hole patterns are resolved/expanded upstream into individual typed cuts; manufacturing does not reconstruct pattern semantics.
6. STEP export, real reimport, and the strengthened equivalence contract from ADR-058 remain mandatory. The endpoint returns only the exact artifact bytes whose reimport/equivalence witnesses passed; it does not regenerate STEP after validation.
7. Composite, including a structurally line-only Composite, remains fail-closed because its typed identity is not Polygon. Polygon combined with Slot remains fail-closed. Slot remains fail-closed independently.

## Rejected alternatives

- A `PolygonSolidSpec` or `SolidFootprint::Polygon` variant: duplicates the existing generic contour transport and backend.
- Inferring Polygon from a closed all-Line contour: incorrectly enables line-only Composite and loses semantic authority.
- Manufacturing-side canonicalization or segment reordering: changes source identity/provenance and can invalidate downstream witnesses.
- Bounding-box extrusion or tessellation: manufactures a different shape and weakens exactness.
- Reconstructing a pattern in Rust manufacturing or Python: duplicates resolved Engineering/Spatial semantics.
- A Polygon-specific CadQuery worker: creates a second manufacturing path with divergent validation and STEP behavior.

## Consequences

Typed Polygon manufacturing is now supported through the existing general closed-profile mechanism, with the boundary still limited to one exact line-only outer contour and the existing typed cylindrical cuts. Concave and translated polygons are supported when they pass the same contour and equivalence validation. The manufacturing matrix is Rectangle/Circle/RoundedRectangle/Polygon supported; Composite and Slot fail-closed. No other profile, cut, intent, or backend capability is enabled.

## Verification evidence

The implementation and per-gate evidence are recorded in [the Phase 13.3B closure audit](../specs/phase13.3b-polygon-manufacturing-closure-audit.md). It covers the canonical polygon golden, concave and translated fixtures, hole and resolved pattern cuts, line-only Composite rejection, Polygon+Slot rejection, same-bounds wrong-contour rejection, real Rust→CadQuery→STEP/reimport/equivalence, and exact UI artifact delivery.
