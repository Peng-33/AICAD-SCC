# ADR-051: Circular Base Profile and Circular-part Three-view Semantics

Status: Accepted for Phase 12.1

## Context

The frozen Phase 10 engineering model and Phase 11 three-view pipeline represent
the base body as a rectangular profile. Phase 12.1 adds the smallest circular
extension needed for disk/flange parts while preserving the existing rectangle
contract, typed provenance, deterministic projections, annotation ownership,
project persistence, and fail-closed manufacturing behavior.

## Decision

1. `EngineeringFeature::BaseProfile` becomes a typed `BaseProfile` sum. The
   existing rectangle form remains source-compatible, and the new circular
   form stores one canonical engineering diameter plus an explicit center.
   Radius is derived only at geometry boundaries; diameter and radius are never
   independent source fields.
2. A circular base resolves to a geometry `Circle` with body/base provenance,
   not to a rectangle or a largest-circle heuristic. Its spatial footprint is
   `SpatialFootprint::Circle { center, radius }`, with exact circle containment
   and an envelope used only for bounds.
3. Circular principal projection is explicit: Front contains the two body
   circles (near/far depth witnesses); Top and Left contain the two-surface
   rectangular silhouette of the circular prism. Visibility and projection
   consistency validate this shape-specific contract.
4. A circular drawing owns one typed overall-diameter annotation in Front and
   thickness in Left. It does not emit width/height overall dimensions. Body
   center mark and axial centerlines reuse the Phase 11 typed center geometry;
   hole circles remain distinct by `SpatialElementRef::Cut` provenance.
5. Scale and first/third-angle placement remain presentation-only. Dimension
   values and engineering sources are invariant under scale and placement.
6. Project JSON persists the typed source profile. Legacy rectangle JSON keeps
   its existing shape and values. Derived geometry, drawings, and STEP bytes
   are not persisted as source truth.
7. The manufacturing boundary is Strategy B for this phase: the existing
   rectangle-only SolidBuildSpec/CadQuery/STEP path remains unchanged, while a
   circular spatial body returns a typed unsupported-manufacturing error. No
   circular body may be silently converted to a bounding rectangle.
8. Natural-language and formal tool paths may create a circular base only via
   a typed command with the same validation as direct engineering commands.
   Missing circular dimensions remain clarification-required; preview and
   commit use the same command application path.

## Contract witnesses

- Rectangle fixtures, Phase 11 SVG/DXF outputs, and SolidBuildSpec rectangle
  golden remain unchanged.
- Circular fixtures cover a centered disk, a non-zero-center disk, a disk with
  a center hole, and the OD/ID/PCD/thickness flange (`160/60/120/20`,
  `8xØ14`). An expression-driven OD (`OD = PCD + 40`) resolves through the
  existing parameter binding path.
- Invalid, unresolved, non-positive, NaN, and infinite circular values fail
  closed before projection/drawing/manufacturing delivery.

## Rejected alternatives

- Keeping `shape_type` strings or optional width/height/diameter fields: this
  permits contradictory states and weakens slot validation.
- Converting a circle to its bounding rectangle: this breaks Front projection,
  provenance, containment, and manufacturing truth.
- Treating every projected circle as a hole: body and cut provenance are
  already distinct and must remain so.
- Implementing Arc/Ellipse/RoundedRect/Polygon/Slot in this phase: those are
  outside the Phase 12.1 scope and would broaden the frozen contract.

## Consequences and migration

Rectangle construction, parameter bindings, projection, annotations, and
manufacturing goldens remain compatible. New shape branches are added at the
engineering, spatial, projection, visibility, consistency, drawing, project,
and command/tool boundaries. Circular manufacturing is intentionally reported
as deferred until a later phase can add a real circular SolidBuildSpec and
CadQuery/STEP witness without changing this drawing contract.
