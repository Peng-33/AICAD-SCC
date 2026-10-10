# ADR-057: General Closed Profile Manufacturing Foundation

Status: Accepted / Planned

Date: 2026-09-17

Scope: Phase 13.3 architecture audit only. No Composite, Polygon, or Slot manufacturing implementation is included in this decision.

## Context

Phase 12.2 and Phase 12.4 already establish exact engineering and Spatial representations for one simple outer loop made from Line and Arc segments. Polygon remains a typed Polygon source with implicit closure and canonical CCW/lexicographic-start identity; Composite remains a typed ordered Line/Arc source. Phase 12.3 establishes Slot as a separate through-cut feature with semantic length, width, center, orientation, and exact stadium segments.

Phase 13.1 and Phase 13.2 currently manufacture Rectangle, Circle, and RoundedRectangle through the existing SolidBuildSpec → CadQuery/OCP → STEP/reimport path. Composite, Polygon, and Slot are intentionally fail-closed.

The current manufacturing contract is intentionally small and must not become a second sketch system or a place where Python reconstructs engineering intent.

## Decision

1. EngineeringModel remains the sole mutable engineering truth. Polygon remains Polygon, Composite remains Composite, and Slot remains EngineeringFeature::Slot. No generic contour replaces engineering source identity.

2. SpatialModel remains the resolved 2.5-D truth. It is the first downstream layer allowed to converge Polygon and Composite into their exact ordered Line/Arc contour representation. Spatial provenance continues to carry source entities and segment indices, and Arc direction remains explicit.

3. The future manufacturing base abstraction is one derived exact ClosedProfile representation for a single simple outer loop. It is a manufacturing payload, not an EngineeringModel type and not a general sketch language.

4. Polygon and Composite both use the future ClosedProfile base representation:
   - Polygon lowers to Line-only ClosedProfile.
   - Composite lowers to Line/Arc ClosedProfile.
   - Neither gets PolygonSolidSpec or CompositeSolidSpec.
   - No tessellation, bounding-box substitution, or profile-pattern inference is permitted.

5. Slot remains a typed subtractive cut. The future cut contract is conceptually CutSpec::Slot, carrying an exact derived ClosedProfile contour and its resolved Z interval. Slot semantic identity and source provenance remain typed until SpatialModel; the backend must consume the exact contour and must not reconstruct the stadium from length, width, or orientation.

6. Hole remains a typed cylindrical cut. General closed contours do not replace cylindrical Hole semantics or hole-pattern expansion.

7. Primary profile validation is owned by Engineering and Spatial:
   - Engineering validates source meaning, ordered continuity, closure, finite values, non-zero area, non-self-intersection, arc radius, and explicit arc direction.
   - Spatial validates solved segment finiteness, endpoint continuity, closure, bounds, source/geometry correspondence, and the one-outer-loop boundary.
   - SolidBuildSpec validation performs the same checks defensively on deserialized transport data.
   - CadQuery is an executor and defensive final geometry gate, not the primary semantic validator.

8. The future SolidBuildSpec evolution is additive. Existing Rectangle, Circle, and RoundedRectangle payloads remain valid and unchanged. The conceptual minimum is:

~~~rust
struct SolidBuildSpec {
    schema_version: u32,
    part: SolidPartSpec,
    operations: BTreeMap<SolidOperationId, SolidOperation>,
    source_map: SolidBuildSourceMap,
}

enum SolidFootprint {
    Rectangle { ... },
    Circle { ... },
    RoundedRectangle { ... },
    ClosedProfile { segments: Vec<SolidProfileSegment> },
}

enum SolidOperation {
    CylindricalCut { ... },
    SlotCut { profile: ClosedProfileSpec, z: SolidZInterval, ... },
}
~~~

The exact Rust names remain an implementation-phase concern. The existing body and operation source maps remain the provenance boundary; a generic contour must not be dispatched by parsing feature IDs or semantic text.

9. The future CadQuery backend uses native analytic edges/wires/faces and linear extrusion in the canonical XY plus-Z frame. Arcs remain analytic and retain CW/CCW direction. No point sampling or tessellation is allowed. CadQuery 2.8.0 documentation exposes native wire/face/extrude primitives, but Phase 13.3A must still establish deterministic construction and failure behavior before any profile is enabled.

10. The current STEP equivalence witness is insufficient for general profiles. Before Polygon, Composite, or Slot STEP delivery, the validator must be extended beyond single-solid, bbox, volume, and sparse material probes. The minimum gate must include a topology/material witness plan that distinguishes same-bbox/same-volume contours, such as surface area, center of mass, stable edge/face counts, and deterministic contour-derived occupancy probes. The exact Phase 13.3A witness contract is a blocker for later profile delivery.

11. Scope remains one outer loop:
   - INNER_LOOPS = DEFERRED
   - MULTIPLE_CONTOURS = DEFERRED
   - ISLANDS = DEFERRED
   - GENERAL_SKETCH_SYSTEM = DEFERRED

12. The coordinate contract remains profile XY with material over Z=[0,T]. No mid-plane extrusion, alternate backend frame, implicit origin normalization, or 0/90-degree-only restriction is introduced.

## CadQuery capability evidence

CadQuery 2.8.0 is pinned by the repository. Its official documentation describes native wire construction, planar face construction from wires/edges, and extrusion:

- https://cadquery.readthedocs.io/en/stable/free-func.html
- https://cadquery.readthedocs.io/en/latest/classreference.html

This proves the candidate API family exists; it does not constitute implementation or validation of the future adapter.

## Consequences

Polygon and Composite share one exact additive manufacturing geometry path without erasing their engineering identity. Slot remains a typed cut while reusing the same exact contour extrusion primitive. Existing Hole, Rectangle, Circle, and RoundedRectangle contracts remain conceptually stable.

The additional STEP witness work is deliberately a prerequisite. No future phase may claim validated Polygon, Composite, or Slot STEP delivery until the stronger equivalence contract is implemented and tested.

## Rejected alternatives

### Dedicated variants

PolygonSolidSpec, CompositeSolidSpec, and SlotSolidSpec would duplicate transport, validation, backend dispatch, goldens, and STEP handling. They also make each new profile a new one-off manufacturing authority. This is rejected.

### One generic operation/extrusion abstraction for every additive and subtractive case

A universal profile-plus-operation kernel would blur base/cut type safety and invite a second sketch/topology system. It also makes Hole semantics unnecessarily generic. This is rejected.

### Backend semantic reconstruction

Sending Polygon vertices, Composite source syntax, or Slot dimensions to Python would move engineering meaning into the backend and duplicate Spatial lowering. This is rejected.

### Approximation

Bounding envelopes, sampled arcs, polygonized rounded profiles, or inferred profile identity would change geometry or provenance. This is rejected.

## Planned implementation gates

Phase 13.3A must first define and test ClosedProfile transport, native analytic construction, defensive validation, and stronger STEP witnesses. Only then may Phase 13.3B/C/D enable individual profiles.
