# ADR-055: Circular Manufacturing and STEP Support

Status: Accepted for Phase 13.1

## Context

Phase 12.4 supported Circle through engineering, spatial, projection,
visibility, PCV, and drawing layers, while manufacturing and STEP remained
fail-closed. Phase 13.1 adds only the circular manufacturing path.

## Decision

1. Circle manufacturing uses the existing pipeline:
   SpatialModel -> SolidBuildSpec -> CadQuery -> STEP export -> STEP
   reimport -> equivalence validation -> exact artifact delivery.
2. Manufacturing consumes SpatialModel only. It does not reread
   EngineeringModel, DSL, parameters, or pattern definitions.
3. SolidBuildSpec v1 gains typed
   `SolidFootprint::Circle { center, radius }`; Rectangle and schema version
   remain compatible.
4. Circle bounds come from center and radius and must match the SpatialModel
   bounds. Bounding-box replacement is forbidden.
5. Non-zero centers and the exact Z interval [0,T] are preserved.
6. Existing cylindrical Hole cuts are reused as resolved
   `SolidOperation::CylindricalCut` operations.
7. CircularPattern is expanded upstream by SpatialModel. Manufacturing never
   reconstructs count, PCD, or hole centers.
8. Python receives only the serialized SolidBuildSpec contract.
9. CadQuery creates an exact circular extrusion; no polygon approximation is
   permitted.
10. STEP export and real reimport are both required. Equivalence validates
    single-solid, bbox, volume, material witnesses, and provenance.
11. The delivered bytes are the bytes that passed validation; no re-export
    occurs after validation.
12. Invalid geometry and unsupported profiles fail closed.
13. Rectangle behavior and its historical SolidBuildSpec golden remain
    unchanged.
14. RoundedRectangle, Composite, Polygon, and Slot manufacturing remain
    deferred. General BRep, reverse STEP engineering, pockets, bosses,
    grooves, and viewer work remain out of scope.

## Consequences

Circle and Rectangle share one backend-neutral contract and one CadQuery/STEP
delivery path. No Circle-specific API, pattern type, annulus source type, or
second semantic authority is introduced.
