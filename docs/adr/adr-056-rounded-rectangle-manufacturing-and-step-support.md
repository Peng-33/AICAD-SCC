# ADR-056: RoundedRectangle Manufacturing and STEP Support

Status: Accepted for Phase 13.2

## Context

Phase 12.2 established the typed RoundedRectangle engineering profile and
exact Line/Arc spatial and drawing semantics. Phase 13.2 extends the existing
Phase 13.1 manufacturing path without introducing a second design authority.

## Decision

1. EngineeringModel remains the sole editable engineering truth.
2. SpatialModel remains the resolved 2.5-D manufacturing input. It preserves
   a typed `SpatialBaseProfile::RoundedRectangle` identity containing center,
   width, height, and corner radius. The exact Composite boundary remains
   available for projection and drawing.
3. Manufacturing consumes that typed SpatialModel value and emits
   `SolidFootprint::RoundedRectangle { center, width, height, corner_radius }`.
   It does not re-evaluate expressions, read bindings, or infer identity from
   Line/Arc segments.
4. RoundedRectangle bounds are the resolved center plus/minus half width and
   half height, with Z fixed to `[0,T]`; the SpatialModel bounds are a witness
   and must match.
5. Width, height, radius, position, and thickness are validated as finite and
   positive. The radius follows the engineering maximum extent rule, with the
   manufacturing contract rejecting zero radius.
6. CadQuery constructs the native rounded profile from a rectangle extrusion
   and vertical-edge fillets. No bbox substitution, polygonization, sampling,
   or arbitrary generated Python is permitted.
7. Existing resolved cylindrical cuts are reused. Hole patterns are expanded
   upstream and are never reconstructed in manufacturing.
8. STEP export uses the existing build-once, private artifact, real reimport,
   single-solid/bbox/volume/material/provenance equivalence path. The exact
   bytes that pass validation are delivered; no re-export occurs afterward.
9. Rectangle and Circle remain compatible. Composite, Polygon, and Slot remain
   fail-closed and do not receive envelope approximations.

## Rejected alternatives

- Recognizing an eight-segment Line/Arc loop downstream would lose typed
  authority and make manufacturing depend on geometry pattern inference.
- Replacing the profile with its bounding rectangle would change volume and
  corner material witnesses.
- Tessellating arcs would make the backend approximate rather than exact.
- Adding a RoundedRectangle-specific hole or STEP API would duplicate the
  existing resolved operation and delivery contracts.

## Consequences

The generic EngineeringModel → SpatialModel → SolidBuildSpec → CadQuery →
STEP pipeline now supports Rectangle, Circle, and RoundedRectangle. The
unsupported profile set remains explicit and fail-closed, while existing
Rectangle/Circle schema and goldens remain stable.
