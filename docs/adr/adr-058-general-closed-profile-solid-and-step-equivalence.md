# ADR-058: General ClosedProfile Solid Foundation and STEP Equivalence

Status: Accepted

Date: 2026-09-17

Scope: Phase 13.3A implementation. This ADR establishes an internal manufacturing foundation; it does not enable Polygon, Composite, or Slot as public manufacturing capabilities.

## Context

Phase 13.1 and 13.2 deliver Rectangle, Circle, and RoundedRectangle through the resolved `SolidBuildSpec` → CadQuery/OCP → STEP/reimport chain. Engineering and Spatial already retain exact one-loop Line/Arc geometry for Composite and Polygon, but their manufacturing lowering is intentionally fail-closed. Slot remains a typed subtractive feature and is not a body profile.

The pre-13.3A STEP validator compared single-solid validity, bounds, volume, and sparse material probes. That set was insufficient for general contours: two solids could share bounds and volume while differing in contour or mass distribution.

## Decision

1. Add an additive `SolidFootprint::ClosedProfile { segments }` transport variant. Its segment vocabulary is exactly one ordered outer loop of analytic `Line` and `Arc` segments. Arc records retain center, radius, endpoints, and explicit `Clockwise`/`CounterClockwise` direction.

2. Keep `EngineeringModel` and `SpatialModel` as semantic authorities. Phase 13.3A does not change `lower_solid_build_spec`: Spatial `Composite` and `Polygon` still return typed fail-closed errors, and `SlotCut` is not added. The new footprint is an internal, directly constructible manufacturing foundation for later gated profile phases.

3. Manufacturing validation defensively checks a non-empty loop, finite/non-degenerate segments, line/arc continuity, closure, arc endpoint-radius consistency, directed sweep, non-zero analytic signed area, and exact analytic XY bounds. It does not add an independent general sketch system or inner-loop topology model.

4. The CadQuery backend constructs each supplied segment as a native analytic edge, adds edges to an OCP wire builder in supplied order, creates one planar face, and extrudes along +Z over the resolved body interval. It performs no tessellation, convex-hull/envelope replacement, endpoint-based arc-direction inference, or semantic reconstruction.

5. STEP equivalence now requires all of the following: bounds, volume, surface area, center of mass, stable edge/face/vertex counts, and deterministic material probes. ClosedProfile probes use a deterministic 5×5 grid over the resolved bounds, and operation probes remain typed and depth-aware. Any mismatch rejects delivery before the artifact is returned.

6. Existing schema-version-1 Rectangle, Circle, RoundedRectangle, and cylindrical-cut payloads remain accepted without field changes. The new variant is additive and remains operationally gated until later profile-specific phases.

## Rejected alternatives

- Bounding-box or convex-hull substitution: changes material geometry and can silently manufacture the wrong part.
- PolygonSolidSpec/CompositeSolidSpec specializations: duplicate transport, validation, backend, and STEP machinery.
- Backend reconstruction from EngineeringModel, DSL, Polygon identity, or Slot dimensions: moves semantic authority across the boundary.
- Tessellated arcs or inferred direction: loses analytic geometry and explicit source semantics.
- Adding SlotCut in this phase: expands scope beyond the foundation and weakens typed cut gating.

## Consequences

The manufacturing layer now has a reusable exact contour transport and executor without opening public profile delivery. The stronger witness contract catches same-bounds/same-volume contour substitutions when their mass distribution or deterministic occupancy differs. Topology counts are treated as an additional exact witness and are accepted only after the pinned CadQuery/OCP STEP roundtrip proves stable for the covered fixtures.

The next gated work is Phase 13.3B Polygon Manufacturing & STEP, followed by Phase 13.3C Composite Manufacturing & STEP. Slot remains deferred to Phase 13.4.

## Verification evidence

- CadQuery 2.8.0 native edge/wire/face/extrude APIs were exercised in the pinned Compose runtime.
- Existing Rectangle/Circle/RoundedRectangle STEP regressions remain green.
- ClosedProfile concave Line and mixed Line/Arc fixtures pass real CadQuery construction and STEP reimport.
- Rust, Python, and Node verification details are recorded in `docs/specs/phase13.3a-general-closed-profile-foundation-closure-audit.md`.
