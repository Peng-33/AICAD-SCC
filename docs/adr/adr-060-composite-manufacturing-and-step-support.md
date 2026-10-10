# ADR-060: Composite Manufacturing and Validated STEP Delivery

Status: Accepted

Date: 2026-09-18

Scope: Phase 13.3C. This decision enables manufacturing and validated STEP delivery only for a typed Engineering/Spatial Composite using the existing generic `ClosedProfile` route. Slot remains fail-closed.

## Context

Phase 12.2 established `BaseProfile::Composite` as one ordered, simple closed outer loop of exact Line and Arc segments. Spatial lowering retains `SpatialBaseProfile::Composite` as typed identity while representing its exact contour in `SpatialFootprint::Composite`; Polygon shares the structural footprint but has distinct typed identity and provenance. Phase 13.3A supplied the generic `SolidFootprint::ClosedProfile`, native CadQuery construction, and strengthened STEP equivalence. Phase 13.3B enabled that route for typed Polygon only.

Composite must be enabled by its upstream type, never inferred from a line/arc contour. The manufacturing contract must preserve exact geometry and order while avoiding a second Composite-specific DTO or worker.

## Decision

1. `SpatialBaseProfile::Composite` is the semantic gate. Lowering requires the exact `SpatialFootprint::Composite` contour and contiguous typed source indices/entities. It does not infer Composite from segment count or shape, and it does not accept another profile identity through this route.
2. Engineering's ordered `Vec<ProfileSegment>` and Spatial's ordered `Vec<SpatialProfileSegment>` remain authoritative. Spatial retains each segment's `segment_index`, source entity, and explicit Arc direction. Manufacturing validates that provenance/order and copies the exact ordered geometry into the existing generic `SolidFootprint::ClosedProfile`; the SolidBuildSpec body source map identifies the source feature and first source entity, while per-segment provenance remains upstream in Spatial.
3. Line endpoints and Arc endpoints, center, radius, and explicit CW/CCW direction are transported without tessellation, reconstruction from endpoints, reordering, normalization, or bounding-box substitution. Translation and valid concave contours are preserved. Directed major arcs are supported. Full-circle Composite arcs are forbidden, and exactly-180-degree arcs remain forbidden because their direction/sweep is ambiguous under the contract.
4. `SolidBuildSpec` gains no Composite-specific variant. The existing generic CadQuery backend constructs native analytic edges, an ordered wire and planar face, then extrudes along +Z over the resolved body interval. Python receives only the generic ClosedProfile DTO and has no Composite type or handler.
5. Typed cylindrical Hole cuts remain typed manufacturing operations. Hole-pattern instances are resolved upstream into individual cuts; manufacturing does not reconstruct pattern semantics. Composite combined with Slot remains fail-closed; Slot Manufacturing and Slot STEP remain fail-closed independently.
6. STEP export is accepted only after real reimport and the strengthened equivalence witnesses. Validated delivery returns the exact artifact bytes that passed validation; STEP is not re-exported after validation. Polygon support remains unchanged and enabled.
7. The supported Composite boundary is one exact outer contour. Inner loops, multiple contours, islands, and generalized sketch profiles remain deferred.

## Rejected alternatives

- A `CompositeSolidSpec` or `SolidFootprint::Composite` variant: duplicates generic exact profile transport and backend validation.
- Inferring profile identity from an all-Line contour or from mixed Line/Arc geometry: loses typed source authority and risks enabling Polygon/Composite incorrectly.
- Reordering segments or reconstructing arc direction from endpoints: changes source semantics and is ambiguous for the forbidden half-turn case.
- Tessellation or bounding-envelope extrusion: manufactures geometry different from the authoritative contour.
- A Composite-specific CadQuery/Python handler or a second STEP validation path: duplicates the generic backend and equivalence contract.
- Reconstructing hole patterns in manufacturing: duplicates upstream resolved semantics.

## Consequences

Composite Manufacturing and Composite STEP are supported through the generic exact ClosedProfile route. Rectangle, Circle, RoundedRectangle, Polygon, and Composite are supported; Slot remains fail-closed. Exact segment order and explicit arc direction remain required. No generic profile inference, Composite DTO/backend, tessellation, bbox substitution, or expansion of the contour/cut domain is authorized.

## Verification evidence

The implementation and final gate evidence are recorded in [the Phase 13.3C closure audit](../specs/phase13.3c-composite-manufacturing-closure-audit.md). The audit includes the line-only identity regression, mixed Line/Arc and major-arc coverage, translated concavity, typed Hole and resolved patterns, identity/order/direction/radius adversarials, Polygon regression, Slot fail-closed boundary, real CadQuery STEP reimport/equivalence, and unchanged validated artifact delivery.
