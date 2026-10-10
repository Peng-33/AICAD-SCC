# ADR-061: Slot Manufacturing and Validated STEP Delivery

- Status: Accepted for Phase 13.4
- Date: 2026-09-18
- Decision owners: SCC architecture

## Context

Phase 12.3 established Slot as an Engineering feature with canonical overall-length, width, center, and orientation semantics. Engineering derives an exact ordered four-segment stadium contour (two Lines and two directed semicircular Arcs); Spatial retains a typed `SlotCut`, per-segment source identity/order, and resolved Z interval. Manufacturing previously rejected Slot because its operation model and backend path were not yet available.

The repository already has an exact generic `ClosedProfile` Line/Arc face/extrusion path and a strengthened STEP reimport/equivalence validator. Phase 13.4 must connect the typed Slot operation to those existing facilities without relocating semantic ownership or reconstructing geometry downstream.

## Decision

1. Slot remains a typed subtractive operation. It is never represented as a `BaseProfile` or body footprint.
2. Engineering and Spatial remain the primary owners of Slot meaning, canonicalization, validation, and exact resolved contour/Z witnesses.
3. Spatial supplies the ordered exact stadium contour and its source-segment identities. Manufacturing validates interval, ordering, provenance, closure/bounds, and copies the exact Line/Arc data; it does not derive centerline endpoints, width, angle, or semicircle geometry from Slot parameters.
4. Manufacturing emits `SolidOperation::SlotCut { profile: ClosedProfileSpec { segments }, z, axis }`. `ClosedProfileSpec` is a derived geometry container, not an editable feature or operation. The operation remains distinctly typed as `SlotCut`.
5. The existing generic exact Line/Arc ClosedProfile builder is reused. CadQuery consumes the transported contour and resolved Z interval for exact face construction and B-Rep subtraction. No Slot-specific Python geometry builder, tessellation, bbox tool, or inferred Arc direction is allowed.
6. The `SolidBuildSpec` v1 envelope remains; this additive typed operation does not require a schema-version bump or a breaking serde migration. Existing operation variants remain unchanged.
7. STEP delivery uses the existing real export → reimport → strengthened geometry/material equivalence path. Only the already-validated STEP bytes are delivered; validation does not trigger a second export.
8. Hole stays a typed cylindrical operation and may coexist with Slot cuts. Multiple independent Slot cuts are supported where Spatial supplies them. Slot patterns are out of scope.
9. Existing explicit validation ownership remains: Engineering/Spatial are primary; Manufacturing retains defensive validation before backend execution.

## Consequences

- Arbitrary canonical Slot orientation and translated centers are preserved because exact world-coordinate segment endpoints/centers are transported unchanged.
- The resolved Z interval is copied from Spatial and consumed directly; Python does not infer it from body thickness or Slot semantics.
- Exact `Line` and `Arc` variants, radius, endpoints, centers, ordering, and direction survive serde and the generic tool builder.
- STEP validation can reject omitted, rotated, widened, chord-substituted, or bbox-substituted cuts using the existing strengthened witnesses.
- The public manufacturing matrix includes Slot after Phase 13.4.

## Rejected alternatives

- Converting Slot into a rectangular or stadium `BaseProfile`: violates feature identity and loses subtractive source semantics.
- Reconstructing Slot from `center`, `overall_length`, `width`, and orientation in Rust Manufacturing or Python: duplicates Engineering geometry authority and can drift.
- Tessellating semicircles or substituting a bounding box: changes exact B-Rep geometry and weakens equivalence.
- Adding a Slot-only backend/DTO or an arbitrary generic profile-cut operation: duplicates the established generic builder and broadens the public contract beyond this phase.
- Re-exporting after successful STEP validation: would mean delivered bytes differ from those actually validated.

## Compatibility and migration

No migration is required. Schema v1 and existing Rectangle/Circle/RoundedRectangle/Polygon/Composite/Hole contracts remain unchanged. Slot becomes accepted only when emitted as the typed operation carrying a valid exact contour and interval; malformed or unsupported upstream data fails closed. Natural-language Slot generation, Slot patterns, inner loops, multiple profile contours, and arbitrary pockets remain deferred.

## Verification

See [the Phase 13.4 closure audit](../specs/phase13.4-slot-manufacturing-closure-audit.md) for the baseline, commit chain, exact test commands/results, adversarial cases, freeze gates, and residual non-blocking notes.
